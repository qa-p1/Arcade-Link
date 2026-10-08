"""Real Clipboard checks. Every process inherits the runner's private session."""

import csv
import io
import json
import re
import shutil
import sys
import select
import signal
import subprocess
import time
from pathlib import Path


def _driver(s, *requests):
    exe = APPS["arcade.clipboard"]["dir"] / "target/debug/arcade_test_driver"
    r = subprocess.run(
        [str(exe)], input="".join(json.dumps(r) + "\n" for r in requests),
        env=s.env, capture_output=True, text=True, timeout=30,
    )
    assert r.returncode == 0, r.stderr
    replies = [json.loads(line) for line in r.stdout.splitlines()]
    assert len(replies) == len(requests), r.stdout + r.stderr
    assert all("ok" in r for r in replies), replies
    return [r["ok"] for r in replies]


def _initialize(s):
    return {"op": "initialize", "data_dir": s.env["ARCADE_DATA_DIR"],
            "device_name": "Clipboard e2e", "link": {"enabled": False}}


def _stop(s):
    if "arcade.clipboard" not in s.procs:
        return
    s.cli("quit", "clipboard")
    p = s.procs["arcade.clipboard"]
    try:
        p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        s.kill("arcade.clipboard", signal.SIGTERM)
        raise AssertionError("Clipboard did not quit")
    s.procs.pop("arcade.clipboard", None)


def _start(s):
    if "arcade.clipboard" not in s.procs:
        s.start("arcade.clipboard")


def _finish(p):
    out, err = p.communicate(timeout=20)
    return p.returncode, json.loads(out) if p.returncode == 0 else err


def _picker(s):
    p = s.invoke("clipboard", "clipboard.pick", background=True)
    win = s.wait_window("Arcade Clipboard")
    s.xdotool("windowfocus", win)
    # The native window appears before Flutter paints the picker.
    time.sleep(0.7)
    return p


@check("clipboard")
def clipboard_without_a_mesh(s):
    _start(s)
    status = json.loads(s.cli("status", "clipboard", "--json", check=True).stdout)
    assert status["status"]["mode"] == "background", status
    code, result = s.invoke("clipboard", "clipboard.devices")
    assert code == 0 and result["outputs"][0]["data"] == [], result
    code, result = s.invoke("clipboard", "clipboard.add", "--text", "hello without mesh")
    assert code != 0 and "unavailable" in result, result
    assert "Arcade Clipboard can't do this yet: no devices are set up yet." in result, result
    _stop(s)
    return "devices=[]; add=unavailable (no mesh)"


@check("clipboard")
def clipboard_add_text_url_image_files_and_size_limit(s):
    _driver(s, _initialize(s), {"op": "create_mesh", "device_name": "Clipboard e2e"},
            {"op": "shutdown"})
    _start(s)
    png = s.root / "clipboard-photo.png"
    # A valid PNG, generated entirely in the private test root.
    import struct
    import zlib
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    png.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
                    + chunk(b"IDAT", zlib.compress((b"\0" + b"\xff\0\0" * 2) * 2)) + chunk(b"IEND", b""))
    one, two = s.root / "one.txt", s.root / "two.txt"
    one.write_text("first file")
    two.write_text("second file")
    for args in [("--text", "Clipboard synthetic text"), ("--url", "https://example.com/clipboard"),
                 ("--file", str(png)), ("--input-json", json.dumps(
                     {"type": "file/any[]", "paths": [str(one), str(two)]}))]:
        code, result = s.invoke("clipboard", "clipboard.add", *args)
        assert code == 0 and result["message"] == "Sent to your devices", result
    big = s.root / "clipboard-big.png"
    with big.open("wb") as f:
        f.truncate(16 * 1024 * 1024 + 1)
    code, result = s.invoke("clipboard", "clipboard.add", "--file", str(big))
    assert code != 0 and "too_large" in result, result
    assert "Too large to send to your devices (limit 16 MB)." in result, result
    _stop(s)
    replies = _driver(s, _initialize(s), {"op": "history"}, {"op": "shutdown"})
    history = replies[1]
    rows = history if isinstance(history, list) else history["items"]
    assert {r["kind"] for r in rows} == {"text", "url", "image", "files"}, rows
    _start(s)
    return "add: text, URL, PNG, files stored; >16 MiB=too_large; history kinds verified"


@check("clipboard")
def clipboard_pick_choose_escape_and_caller_cancel(s):
    _start(s)
    code, result = s.invoke("clipboard", "clipboard.add", "--text", "Choose this synthetic clip")
    assert code == 0, result
    p = _picker(s)
    s.xdotool("type", "--clearmodifiers", "Choose this synthetic clip")
    time.sleep(0.5)
    s.xdotool("key", "Return")
    code, result = _finish(p)
    assert code == 0 and result["outputs"][0]["text"] == "Choose this synthetic clip", result
    p = _picker(s)
    s.xdotool("key", "Escape")
    code, result = _finish(p)
    assert code != 0 and "user_cancelled" in result, result
    p = _picker(s)
    p.send_signal(signal.SIGINT)
    code, result = _finish(p)
    assert code != 0 and "cancelled" in result, result
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline and s.xdotool("search", "--onlyvisible", "--name", "Arcade Clipboard"):
        time.sleep(0.05)
    assert not s.xdotool("search", "--onlyvisible", "--name", "Arcade Clipboard"), "caller cancel left picker open"
    return "pick=chosen text; Escape=denied/user_cancelled; SIGINT=cancelled and picker hidden"


@check("clipboard")
def clipboard_private_mode_denies_add(s):
    _stop(s)
    _driver(s, _initialize(s), {"op": "settings", "values": {"paused": True}}, {"op": "shutdown"})
    _start(s)
    code, result = s.invoke("clipboard", "clipboard.add", "--text", "private test")
    assert code != 0 and "private_mode" in result, result
    assert "Arcade Clipboard is in Private mode." in result, result
    _stop(s)
    _driver(s, _initialize(s), {"op": "settings", "values": {"paused": False}}, {"op": "shutdown"})
    return "add=denied/private_mode; standard message verified"


class _Core:
    """Drive the same async FRB JSON boundary Dart uses, in the isolated bus."""
    def __init__(self, s):
        self.s = s
        exe = APPS["arcade.clipboard"]["dir"] / "target/debug/arcade_test_driver"
        self.p = subprocess.Popen([str(exe)], env=s.env, stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=open(s.root / "consumer-core.log", "w"),
                                  text=True, start_new_session=True)
        s.procs["clipboard-consumer"] = self.p
        try:
            self.request("initialize", data_dir=s.env["ARCADE_DATA_DIR"], device_name="Consumer e2e")
            self.event("registry_changed")
        except Exception:
            s.kill("clipboard-consumer")
            raise

    def request(self, op, **args):
        self.p.stdin.write(json.dumps({"op": op, **args}) + "\n")
        self.p.stdin.flush()
        assert select.select([self.p.stdout], [], [], 30)[0], f"core {op} timed out"
        row = json.loads(self.p.stdout.readline())
        assert "ok" in row, row
        return row["ok"]

    def event(self, kind, request=None):
        kinds = {}
        deadline = time.monotonic() + 130
        while time.monotonic() < deadline:
            event = self.request("link_wait")
            kinds[event["kind"]] = kinds.get(event["kind"], 0) + 1
            if event["kind"] == kind and (request is None or event.get("request") == request):
                return event
        raise AssertionError(f"no {kind} event; seen {kinds}")

    def invoke(self, peer, action, **args):
        ticket = self.request("link_invoke", peer=peer, action=action, **args)["request"]
        return self.event("invoke_done", ticket)

    def close(self):
        try:
            if self.p.poll() is None:
                self.request("shutdown")
                self.p.stdin.close()
                self.p.wait(timeout=10)
        finally:
            self.s.kill("clipboard-consumer")


def _mock(s, peer, actions, oneshot=True):
    s.kill(peer)
    fixture = s.root / f"{peer}-clipboard-fixture.json"
    fixture.write_text(json.dumps({"id": peer, "oneshot": oneshot, "actions": actions,
                                  "shortcuts": [{"id": "test", "accelerator": "Alt+Control+Space"}]}))
    p = subprocess.Popen([str(CLI), "mock", "--as", peer, "--actions", str(fixture)], env=s.env,
                         stdout=subprocess.DEVNULL, stderr=open(s.root / f"{peer}.log", "ab"), start_new_session=True)
    s.procs[peer] = p
    s.wait_running(peer)
    return p


def _action(action, accepts, result=None, **behavior):
    title = action.split("#")[-1]
    row = {"id": action, "title": title, "accepts": accepts, "version": 1,
           "mock": behavior}
    if "#" in action:
        row["preset"] = action.split("#")[1]
    if result is not None:
        row["mock"]["result"] = result
    return row


@check("clipboard")
def clipboard_consumer_discovery_transforms_and_handoff(s):
    _stop(s)
    c = _Core(s)
    try:
        assert c.request("link_offers")["offers"]["image"] == []
        png = s.root / "clipboard-photo.png"
        image = {"type": "file/image", "path": str(png)}
        _mock(s, "arcade.look", [_action("look.preview", ["file/*", "file/*[]"], {"message": "Previewing"})])
        c.event("registry_changed")
        offers = c.request("link_offers")["offers"]["image"]
        assert any(o["title"] == "Quick Look" for o in offers), offers
        _mock(s, "arcade.lens", [
            _action("lens.recognize", ["file/image"], {"outputs": [{"type": "text/plain", "text": "Synthetic OCR result"}]}),
            _action("lens.pin", ["file/image"], {"message": "Pinned"}),
            _action("lens.analyze", ["file/image"], {"message": "Analyzed"}),
        ])
        _mock(s, "arcade.box", [
            _action("box:arcade.image.convert#png", ["file/image"], {"outputs": [image]}, steps=3, stepMs=40),
            _action("box:arcade.image.compress#web-200kb", ["file/image"], {"outputs": [image]}),
            _action("box:arcade.text.structured#format-json", ["text/plain"],
                    {"outputs": [{"type": "structured/json", "text": '{\n  "synthetic": true\n}', "data": {"synthetic": True}}]}),
            _action("box:arcade.text.clean#clean", ["text/plain"], {"outputs": [{"type": "text/plain", "text": "Clean synthetic text"}]}),
        ])
        # Wait for the watcher, without asking discovery to read the filesystem.
        for _ in range(10):
            c.event("registry_changed")
            offers = c.request("link_offers")["offers"]["image"]
            if any(o["title"] == "Convert to PNG" for o in offers):
                break
        assert {"Quick Look", "Extract text", "Convert to PNG", "Pin"} <= {o["title"] for o in offers}, offers
        text = {"type": "text/plain", "text": '{"synthetic":true}'}
        for peer, action, content in [
            ("arcade.look", "look.preview", image),
            ("arcade.lens", "lens.recognize", image),
            ("arcade.lens", "lens.pin", image),
            ("arcade.lens", "lens.analyze", image),
            ("arcade.box", "box:arcade.image.convert#png", image),
            ("arcade.box", "box:arcade.text.structured#format-json", text),
            ("arcade.box", "box:arcade.text.clean#clean", text),
        ]:
            done = c.invoke(peer, action, input=content)
            assert "result" in done, done
        history = c.request("history")
        rows = history if isinstance(history, list) else history["items"]
        assert {"Synthetic OCR result", "Clean synthetic text", '{\n  "synthetic": true\n}'} <= {r["text"] for r in rows}, rows
        photo = next(r for r in rows if r["kind"] == "image")
        done = c.invoke("arcade.look", "look.preview", item_id=photo["id"])
        assert "result" in done, done
        handoff = s.root / "arcade/handoff"
        assert not list(handoff.glob("*/clip-*.png")), "outbound handoff leaked after completion"
        stage = c.request("link_stage_image", mime="image/png")["stage"]
        import base64
        chunk = base64.b64encode(b"x" * (512 * 1024)).decode()
        for _ in range(33):
            c.request("link_stage_image", stage=stage, data_base64=chunk)
        done = c.invoke("arcade.box", "box:arcade.image.compress#web-200kb", stage=stage)
        assert "result" in done, done
        assert not list(handoff.glob("*/photo.png")), "staged oversized photo leaked"
        c.request("link_configure", link={"enabled": True, "disabled_peers": ["arcade.box"]})
        assert not any(o["peer"] == "arcade.box" for o in c.request("link_offers")["offers"]["image"])
        done = c.invoke("arcade.box", "box:arcade.image.convert#png", input=image)
        assert done["error"]["reason"] == "disabled", done
        c.request("link_configure", link={"enabled": False})
        assert all(not offers for offers in c.request("link_offers")["offers"].values())
        assert not s.endpoint("arcade.clipboard").exists()
        return "late peers watched; photo/text actions and imported clips; handoff cleanup; >16 MiB staging/compress; toggles hide/refuse"
    finally:
        c.close()
        for peer in ("arcade.look", "arcade.lens", "arcade.box"):
            s.kill(peer)
            (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)


@check("clipboard")
def clipboard_consumer_failure_progress_cancel_and_timeout(s):
    c = _Core(s)
    try:
        action = "box:arcade.image.convert#png"
        image = {"type": "file/image", "path": str(s.root / "clipboard-photo.png")}
        unavailable = _action(action, ["file/image"])
        unavailable.update(available=False, reason="converter disabled")
        limited = _action("lens.pin", ["file/image"], {"message": "Pinned"})
        limited["maxBytes"] = 1
        _mock(s, "arcade.box", [unavailable])
        _mock(s, "arcade.lens", [limited])
        c.event("registry_changed")
        assert not any(o["peer"] == "arcade.box" for o in c.request("link_offers")["offers"]["image"]), "unavailable action must be hidden"
        done = c.invoke("arcade.box", action, input=image)
        assert done["error"]["code"] == "unavailable" and done["message"] == "Arcade Box can't do this yet: converter disabled.", done
        done = c.invoke("arcade.lens", "lens.pin", input=image)
        assert done["error"]["code"] == "too_large", done
        _mock(s, "arcade.box", [_action(action, ["file/image"], steps=100, stepMs=30)])
        c.event("registry_changed")
        ticket = c.request("link_invoke", peer="arcade.box", action=action, input=image)["request"]
        c.event("invoke_progress", ticket)
        t0 = time.monotonic()
        c.request("status")
        elapsed = time.monotonic() - t0
        assert elapsed < 0.2, f"core boundary blocked {elapsed:.3f}s during peer job"
        assert c.request("link_cancel", request=ticket)["cancelled"]
        done = c.event("invoke_done", ticket)
        assert done["error"]["code"] == "cancelled" and done["message"] == "Cancelled.", done
        done = c.invoke("arcade.box", action, input=image, timeout_ms=200)
        assert done["error"]["code"] == "timeout" and done["message"] == "Arcade Box didn't respond in time.", done
        _mock(s, "arcade.box", [_action(action, ["file/image"], steps=100, stepMs=30, crashAfterMs=80)])
        c.event("registry_changed")
        done = c.invoke("arcade.box", action, input=image)
        assert done["error"]["code"] == "not_running" and done["message"] == "Arcade Box isn't running.", done
        _mock(s, "arcade.box", [_action(action, ["file/image"], {"outputs": []})])
        c.event("registry_changed")
        c.request("settings", values={"paused": True})
        done = c.invoke("arcade.box", action, input=image)
        assert done["error"]["reason"] == "private_mode" and done["message"] == "Arcade Clipboard is in Private mode.", done
        c.request("settings", values={"paused": False})
        done = c.invoke("arcade.box", action, input={**image, "hints": ["secret"]})
        assert done["error"]["reason"] == "secret", done
        return f"unavailable, maxBytes, progress, cancel, timeout, peer crash, Private mode, secret guard; concurrent core status={elapsed * 1000:.1f}ms"
    finally:
        c.close()
        for peer in ("arcade.lens", "arcade.box"):
            s.kill(peer)
            (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)


@check("clipboard")
def clipboard_foreground_launch_mode(s):
    _stop(s)
    spec = APPS["arcade.clipboard"]
    original = spec["args"]
    try:
        spec["args"] = []
        _start(s)
        status = json.loads(s.cli("status", "clipboard", "--json", check=True).stdout)
        assert status["status"]["mode"] == "foreground", status
        return "Handler.status reports foreground; background mode checked on normal startup"
    finally:
        _stop(s)
        spec["args"] = original


@check("clipboard")
def clipboard_photo_flagship_with_real_look_lens_and_box(s):
    _start(s)
    photo = s.root / "clipboard-real-photo.jpg"
    subprocess.run(["magick", "-size", "900x240", "xc:white", "-fill", "black", "-font", "DejaVu-Sans",
                    "-pointsize", "64", "-gravity", "center", "-annotate", "0", "Clipboard photo example",
                    str(photo)], env=s.env, capture_output=True, check=True, timeout=15)
    code, result = s.invoke("clipboard", "clipboard.add", "--file", str(photo))
    assert code == 0, result
    _stop(s)
    s.env["ALOOK_E2E_MAP_EARLY"] = "1"
    s.env["ALOOK_DEBUG"] = "1"
    for peer in ("arcade.look", "arcade.lens", "arcade.box"):
        s.start(peer)
    c = _Core(s)
    try:
        status = json.loads(s.cli("status", "clipboard", "--json", check=True).stdout)
        assert status["status"]["mode"] == "foreground", status
        deadline = time.monotonic() + 20
        while True:
            offers = c.request("link_offers")["offers"]
            if {"Quick Look", "Extract text", "Convert to PNG", "Pin"} <= {o["title"] for o in offers["image"]}:
                break
            if time.monotonic() >= deadline:
                raise AssertionError("real image offers missing: " + s.cli("describe", "box", "--json").stdout)
            time.sleep(0.1)
        history = c.request("history")
        rows = history if isinstance(history, list) else history["items"]
        # The arrival is the newest JPEG, captured through the real Link server.
        item = next(r for r in rows if r["kind"] == "image")
        image_id = item["id"]
        before = {r["id"] for r in rows}
        print("     real flow: Quick Look", flush=True)
        quick = c.invoke("arcade.look", "look.preview", item_id=image_id)
        assert "result" in quick, quick
        print("     real flow: OCR", flush=True)
        extracted = c.invoke("arcade.lens", "lens.recognize", item_id=image_id)
        assert "result" in extracted, extracted
        recognized = next((o.get("text", "") for o in extracted["result"].get("outputs", []) if o["type"] == "text/plain"), "")
        assert "clipboard" in recognized.lower(), extracted
        print("     real flow: PNG", flush=True)
        converted = c.invoke("arcade.box", "box:arcade.image.convert#png", item_id=image_id)
        assert "result" in converted, converted
        image = next(o for o in converted["result"]["outputs"] if o["type"] == "file/image")
        assert Path(image["path"]).read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
        print("     real flow: Pin", flush=True)
        pinned = c.invoke("arcade.lens", "lens.pin", item_id=image_id)
        assert "result" in pinned, pinned
        s.wait_window("Arcade Lens Pin", timeout=5)
        history = c.request("history")
        rows = history if isinstance(history, list) else history["items"]
        assert any(r["kind"] == "text" and "clipboard" in r["text"].lower() and r["id"] not in before for r in rows), rows
        assert any(r["kind"] == "image" and r["id"] not in before for r in rows), rows
        assert not list((s.root / "arcade/handoff").glob("*/clip-*.jpg")), "photo handoff leaked"
        # A stopped headless peer is relaunched as a one-shot without a listener.
        s.cli("quit", "lens", "--force", check=True)
        s.procs["arcade.lens"].wait(timeout=10)
        s.procs.pop("arcade.lens")
        oneshot = c.invoke("arcade.lens", "lens.recognize", item_id=image_id)
        assert "result" in oneshot, oneshot
        assert not s.endpoint("arcade.lens").exists(), "headless recognition started a resident listener"
        # Real Box text presets also return new clips.
        for action in ("box:arcade.text.structured#format-json", "box:arcade.text.clean#clean"):
            done = c.invoke("arcade.box", action, input={"type": "text/plain", "text": '{"clipboard":true}'})
            assert "result" in done, done
            assert any(o["type"] == "text/plain" and "clipboard" in o.get("text", "")
                       for o in done["result"].get("outputs", [])), done
        history = c.request("history")
        rows = history if isinstance(history, list) else history["items"]
        assert any(r["kind"] == "text" and '"clipboard"' in r["text"] and "\n" in r["text"] for r in rows), rows
        return "real photo arrival -> Quick Look -> OCR text clip -> PNG clip -> Lens pin; Lens one-shot; real Box JSON/clean text"
    finally:
        c.close()
        for peer in ("arcade.look", "arcade.lens", "arcade.box"):
            s.kill(peer)
            (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)


def _ui_words(s, win):
    # Flutter's custom canvas has no X11 child controls. Locate labels in a
    # private screenshot so checks follow the displayed UI, not guessed rows.
    current = s.root / "clipboard-ui-current.png"
    subprocess.run(["import", "-window", win, str(current)], env=s.env,
                   capture_output=True, check=True, timeout=10)
    result = subprocess.run(["tesseract", str(current), "stdout", "--psm", "11", "tsv"],
                            env={**s.env, "OMP_THREAD_LIMIT": "1"},
                            capture_output=True, text=True, check=True, timeout=10)
    return [row for row in csv.DictReader(io.StringIO(result.stdout), delimiter="\t", quoting=csv.QUOTE_NONE)
            if row["text"].strip()]


def _ui_matches(words, phrase):
    def normalized(text):
        return re.sub(r"[^a-z0-9]", "", text.lower())
    wanted = normalized(phrase)
    tokens = [normalized(row["text"]) for row in words]
    matches = []
    for index in range(len(tokens)):
        combined = ""
        end = index
        for end in range(index, len(tokens)):
            combined += tokens[end]
            if combined == wanted or not wanted.startswith(combined):
                break
        if combined != wanted:
            continue
        rows = words[index:end + 1]
        left = min(int(r["left"]) for r in rows)
        right = max(int(r["left"]) + int(r["width"]) for r in rows)
        top = min(int(r["top"]) for r in rows)
        bottom = max(int(r["top"]) + int(r["height"]) for r in rows)
        matches.append(((left + right) // 2, (top + bottom) // 2))
    return matches


def _ui_wait(s, win, phrase, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        words = _ui_words(s, win)
        if _ui_matches(words, phrase):
            return words
        time.sleep(0.2)
    raise AssertionError(f"UI label {phrase!r} missing: " + " ".join(r["text"] for r in words))


def _ui_click(s, win, phrase, after=None):
    words = _ui_wait(s, win, phrase)
    matches = _ui_matches(words, phrase)
    if after:
        y = _ui_matches(words, after)[0][1]
        matches = [m for m in matches if m[1] > y]
    assert matches, f"no {phrase!r} below {after!r}"
    x, y = matches[0]
    s.xdotool("windowfocus", "--sync", win)
    s.xdotool("mousemove", "--window", win, str(x), str(y), "click", "1")
    time.sleep(0.5)


def _ui_navigate(s, win, label, destination):
    # Window restoration after a picker completes can briefly consume the
    # first focus/click. Retry navigation only; never retry a settings toggle.
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        _ui_click(s, win, label)
        words = _ui_words(s, win)
        if _ui_matches(words, destination):
            return
        time.sleep(0.2)
    raise AssertionError(f"navigation to {destination!r} failed")


def _menu_shot(s, win, kind, peers):
    y = {"file": 259, "text": 337, "image": 418}[kind]
    required = {"file": ["Quick Look"], "text": ["Format JSON", "Clean text"],
                "image": ["Quick Look", "Analyze with Lens", "Extract text", "Pin", "Convert to PNG", "Compress"]}[kind] if peers else []
    deadline = time.monotonic() + 25
    while True:
        s.xdotool("mousemove", "--window", win, "1118", str(y), "click", "1")
        words = _ui_wait(s, win, "Inspect clip")
        # The menu grows open: re-read it for a moment before reopening.
        settle = time.monotonic() + 2
        while not all(_ui_matches(words, label) for label in required) and time.monotonic() < settle:
            time.sleep(0.3)
            words = _ui_words(s, win)
        if all(_ui_matches(words, label) for label in required):
            break
        if time.monotonic() >= deadline:
            s.screenshot(f"clipboard-menu-{kind}-failed", win)
        s.xdotool("key", "Escape")
        assert time.monotonic() < deadline, "menu entries missing: " + " ".join(r["text"] for r in words)
        # A menu snapshots cached offers at open. Reopen after a real Box
        # provider finishes initialization and publishes late availability.
        time.sleep(0.3)
    if not peers:
        text = " ".join(r["text"] for r in words)
        assert not any(label in text for label in ("Quick Look", "Format JSON", "Extract text", "Convert to PNG")), text
    s.screenshot(f"clipboard-menu-{kind}-{'peers' if peers else 'alone'}", win)
    s.xdotool("key", "Escape")
    time.sleep(0.2)


def _publish_clipboard(s, kind, value):
    # Copy through GTK's normal text/image/URI clipboard APIs. The helper
    # owns the selection only in this Xvfb; no global clipboard is touched.
    s.kill("clipboard-owner")
    program = r'''
import ctypes as c
import sys
gtk = c.CDLL("libgtk-3.so.0")
gdk = c.CDLL("libgdk-3.so.0")
class Entry(c.Structure):
    _fields_ = [("target", c.c_char_p), ("flags", c.c_uint), ("info", c.c_uint)]
get_type = c.CFUNCTYPE(None, c.c_void_p, c.c_void_p, c.c_uint, c.c_void_p)
clear_type = c.CFUNCTYPE(None, c.c_void_p, c.c_void_p)
gdk.gdk_atom_intern_static_string.argtypes = [c.c_char_p]
gdk.gdk_atom_intern_static_string.restype = c.c_void_p
gtk.gtk_clipboard_get.argtypes = [c.c_void_p]
gtk.gtk_clipboard_get.restype = c.c_void_p
gtk.gtk_selection_data_set.argtypes = [c.c_void_p, c.c_void_p, c.c_int, c.c_void_p, c.c_int]
gtk.gtk_clipboard_set_with_data.argtypes = [c.c_void_p, c.POINTER(Entry), c.c_uint, get_type, clear_type, c.c_void_p]
assert gtk.gtk_init_check(None, None)
payload = sys.argv[2].encode() + b"\r\n"
buffer = c.create_string_buffer(payload)
uri_atom = gdk.gdk_atom_intern_static_string(b"text/uri-list")
@get_type
def get_data(clipboard, selection, info, data):
    gtk.gtk_selection_data_set(selection, uri_atom, 8, buffer, len(payload))
@clear_type
def clear_data(clipboard, data):
    pass
clipboard = gtk.gtk_clipboard_get(gdk.gdk_atom_intern_static_string(b"CLIPBOARD"))
if sys.argv[1] == "text":
    gtk.gtk_clipboard_set_text.argtypes = [c.c_void_p, c.c_char_p, c.c_int]
    text = sys.argv[2].encode()
    gtk.gtk_clipboard_set_text(clipboard, text, len(text))
elif sys.argv[1] == "image":
    pixbuf = c.CDLL("libgdk_pixbuf-2.0.so.0")
    pixbuf.gdk_pixbuf_new_from_file.argtypes = [c.c_char_p, c.c_void_p]
    pixbuf.gdk_pixbuf_new_from_file.restype = c.c_void_p
    image = pixbuf.gdk_pixbuf_new_from_file(sys.argv[2].encode(), None)
    assert image, "image could not be decoded"
    gtk.gtk_clipboard_set_image.argtypes = [c.c_void_p, c.c_void_p]
    gtk.gtk_clipboard_set_image(clipboard, image)
else:
    entry = Entry(b"text/uri-list", 0, 0)
    assert gtk.gtk_clipboard_set_with_data(clipboard, c.byref(entry), 1, get_data, clear_data, None)
print("ready", flush=True)
gtk.gtk_main()
'''
    p = subprocess.Popen([sys.executable, "-c", program, kind, str(value)], env=s.env,
                         stdout=subprocess.PIPE, stderr=open(s.root / "clipboard-owner.log", "w"),
                         text=True, start_new_session=True)
    s.procs["clipboard-owner"] = p
    assert select.select([p.stdout], [], [], 10)[0], "GTK clipboard owner did not start"
    assert p.stdout.readline().strip() == "ready", "GTK clipboard owner failed"


def _publish_file_clipboard(s, path):
    _publish_clipboard(s, "uri", path.as_uri())


@check("clipboard_ui")
def clipboard_ui_surfaces(s):
    _stop(s)
    s.env["ARCADE_DATA_DIR"] = str(s.root / "clipboard-ui-data")
    status = _driver(s, _initialize(s), {"op": "shutdown"})[0]
    if not status.get("mesh_id"):
        _driver(s, _initialize(s), {"op": "create_mesh", "device_name": "Clipboard UI"}, {"op": "shutdown"})
    prefs = Path(s.env["XDG_DATA_HOME"]) / "dev.arcade.clipboard/shared_preferences.json"
    prefs.parent.mkdir(parents=True, exist_ok=True)
    values = json.loads(prefs.read_text()) if prefs.exists() else {}
    values["flutter.mesh_shortcut"] = "CTRL+ALT+F9"
    prefs.write_text(json.dumps(values))
    _start(s)
    photo = s.root / "clipboard-ui-photo.jpg"
    subprocess.run(["magick", "-size", "900x240", "xc:white", "-fill", "black", "-font", "DejaVu-Sans",
                    "-pointsize", "64", "-gravity", "center", "-annotate", "0", "Clipboard UI photo", str(photo)],
                   env=s.env, capture_output=True, check=True)
    for args in [("--file", str(photo)), ("--text", '{"clipboard":true}')]:
        code, result = s.invoke("clipboard", "clipboard.add", *args)
        assert code == 0, result
    file = s.root / "clipboard-ui-notes.txt"
    file.write_text("Synthetic file clip")
    code, result = s.invoke("clipboard", "clipboard.add", "--file", str(file))
    assert code == 0, result
    s.cli("activate", "clipboard", check=True)
    win = s.wait_window("Arcade Clipboard")
    s.xdotool("windowsize", "--sync", win, "1180", "820")
    s.xdotool("windowfocus", "--sync", win)
    _ui_wait(s, win, "3 clips")
    for kind in ("file", "text", "image"):
        _menu_shot(s, win, kind, peers=False)
    _ui_navigate(s, win, "Settings", "Private mode")
    _ui_navigate(s, win, "Connected apps", "Connect with other Arcade apps")
    s.xdotool("windowsize", "--sync", win, "1180", "1040")
    _ui_wait(s, win, "Not installed")
    s.screenshot("clipboard-connected-alone", win)
    s.xdotool("mousemove", "--window", win, "28", "28", "click", "1")
    _ui_click(s, win, "Clipboard")
    s.xdotool("windowsize", "--sync", win, "1180", "820")
    s.env["ALOOK_E2E_MAP_EARLY"] = "1"
    for peer in ("arcade.look", "arcade.lens", "arcade.box"):
        s.start(peer)
    for kind in ("file", "text", "image"):
        _menu_shot(s, win, kind, peers=True)
    picker = _picker(s)
    win = s.wait_window("Arcade Clipboard")
    _ui_wait(s, win, "Choose")
    s.xdotool("key", "End")
    _ui_wait(s, win, "Convert to PNG")
    s.screenshot("clipboard-picker-image-shortcuts", win)
    s.xdotool("key", "Home", "Down")
    _ui_wait(s, win, "Format JSON")
    s.screenshot("clipboard-picker-text-shortcuts", win)
    s.xdotool("key", "ctrl+alt+j")
    _ui_wait(s, win, "added a new clip")
    s.xdotool("key", "Escape")
    code, result = _finish(picker)
    assert code != 0 and "user_cancelled" in result, result
    s.cli("activate", "clipboard", check=True)
    win = s.wait_window("Arcade Clipboard")
    s.xdotool("windowsize", "--sync", win, "1180", "1040")
    s.xdotool("windowfocus", "--sync", win)
    _ui_navigate(s, win, "Settings", "Private mode")
    _ui_navigate(s, win, "Connected apps", "Connect with other Arcade apps")
    _ui_wait(s, win, "Running")
    _ui_click(s, win, "Diagnostics")
    _ui_wait(s, win, "Listening")
    s.screenshot("clipboard-connected-peers", win)
    _ui_click(s, win, "Use with Arcade Clipboard", after="Arcade Box")
    time.sleep(0.4)
    s.screenshot("clipboard-connected-box-off", win)
    _ui_click(s, win, "Connect with other Arcade apps")
    deadline = time.monotonic() + 5
    while s.endpoint("arcade.clipboard").exists() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert not s.endpoint("arcade.clipboard").exists(), "master-off left a listener"
    manifest = json.loads((s.root / "arcade/apps/arcade.clipboard.json").read_text())
    assert manifest["actions"] == [], manifest
    s.screenshot("clipboard-connected-master-off", win)
    _ui_click(s, win, "Connect with other Arcade apps")
    s.wait_running("arcade.clipboard")
    _ui_click(s, win, "Use with Arcade Clipboard", after="Arcade Box")
    s.xdotool("mousemove", "--window", win, "28", "28", "click", "1")
    # Box's native global grab would intercept the recorder's XTest chord.
    # Its persisted manifest still claims the shortcut when it is installed
    # but stopped, exactly what the registry-backed warning must inspect.
    s.kill("arcade.box", signal.SIGTERM)
    _ui_click(s, win, "CTRL+ALT+F9")
    s.xdotool("key", "ctrl+alt+space")
    _ui_wait(s, win, "Used by Arcade Box")
    s.screenshot("clipboard-shortcut-clash", win)
    _ui_click(s, win, "Cancel")
    _ui_click(s, win, "Clipboard")
    s.xdotool("windowsize", "--sync", win, "1180", "820")
    s.start("arcade.box")
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        actions = json.loads(s.cli("describe", "box", "--json", check=True).stdout)
        if any(a["id"] == "box:arcade.image.compress#web-200kb" and a.get("available", True)
               for a in actions):
            break
        time.sleep(0.2)
    else:
        raise AssertionError("Real Box compressor did not become available")
    time.sleep(0.3)  # let the OS directory-watch notification reach Dart
    big = s.root / "clipboard-ui-large-photo.jpg"
    shutil.copy2(photo, big)
    with big.open("ab") as f:
        f.truncate(17 * 1024 * 1024)
    _publish_file_clipboard(s, big)
    _ui_wait(s, win, "Image too large to sync")
    s.screenshot("clipboard-compress-offer", win)
    _ui_click(s, win, "Compress")
    _ui_wait(s, win, "Compress")
    _ui_wait(s, win, "added a new clip", timeout=30)
    s.screenshot("clipboard-compress-result", win)
    _stop(s)
    history = _driver(s, _initialize(s), {"op": "history"}, {"op": "shutdown"})[1]
    rows = history if isinstance(history, list) else history["items"]
    assert all(r["size"] <= 16 * 1024 * 1024 for r in rows), rows
    assert len([r for r in rows if r["kind"] == "image"]) == 2, rows
    for peer in ("arcade.look", "arcade.lens", "arcade.box", "clipboard-owner"):
        s.kill(peer)
        (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)
    return "real UI: six menus; picker image/text shortcuts and JSON import; Connected apps/toggles/diagnostics; recorder clash; >16 MiB opt-in compress imports a limited image"


@check("clipboard")
@check("clipboard_shutdown")
def clipboard_shutdown_with_real_peers(s):
    _stop(s)
    for peer in ("arcade.lens", "arcade.box"):
        s.start(peer)
    try:
        exe = APPS["arcade.clipboard"]["dir"] / "target/debug/arcade_test_driver"
        for enabled in (False, True, False, True):
            p = subprocess.Popen([str(exe)], env=s.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=open(s.root / "shutdown-regression.log", "ab"), text=True,
                                 start_new_session=True)
            s.procs["clipboard-shutdown-driver"] = p
            for row in ({**_initialize(s), "link": {"enabled": enabled}}, {"op": "shutdown"}):
                p.stdin.write(json.dumps(row) + "\n")
                p.stdin.flush()
                assert select.select([p.stdout], [], [], 10)[0], f"no {row['op']} response"
                reply = json.loads(p.stdout.readline())
                assert "ok" in reply, reply
            p.stdin.close()
            p.wait(timeout=3)
            assert p.returncode == 0, p.returncode
            assert not s.endpoint("arcade.clipboard").exists(), "startup resurrected a stopped endpoint"
            s.procs.pop("clipboard-shutdown-driver")
        _start(s)
        t0 = time.monotonic()
        _stop(s)  # app.quit
        app_quit_ms = (time.monotonic() - t0) * 1000
        _start(s)
        p = s.procs["arcade.clipboard"]
        t0 = time.monotonic()
        r = subprocess.run([str(APPS["arcade.clipboard"]["dir"] / APPS["arcade.clipboard"]["bin"]), "--quit"],
                           env=s.env, capture_output=True, text=True, timeout=10)
        assert r.returncode == 0, r.stderr
        p.wait(timeout=5)
        assert p.returncode == 0, s.log("arcade.clipboard")
        assert not s.endpoint("arcade.clipboard").exists()
        s.procs.pop("arcade.clipboard")
        return f"four immediate driver shutdowns exit with Lens/Box registered; real app.quit={app_quit_ms:.0f}ms; real --quit={(time.monotonic() - t0) * 1000:.0f}ms"
    finally:
        for peer in ("arcade.lens", "arcade.box", "clipboard-shutdown-driver"):
            s.kill(peer)
            (s.root / "arcade/apps" / f"{peer}.json").unlink(missing_ok=True)
