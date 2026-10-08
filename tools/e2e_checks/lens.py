"""Real Lens server checks and consumer flows, always under the e2e runner."""

import csv
import difflib
import io
import json
import select
import signal
import sqlite3
import struct
import subprocess
import time
import tomllib
import zlib
from pathlib import Path


def _png(s, name="lens-input.png", width=240, height=160):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + bytes([200, 40, 40]) * width for _ in range(height))
    path = s.root / name
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return path


def _start(s):
    s.env["LENS_DEBUG"] = "1"
    p = s.procs.get("arcade.lens")
    if p is None or p.poll() is not None:
        s.start("arcade.lens")


def _focus(s, name="^Arcade Lens$"):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        wins = s.xdotool("search", "--onlyvisible", "--name", name).splitlines()
        for win in wins:
            geometry = s.xdotool("getwindowgeometry", "--shell", win)
            # The resident root is briefly mapped at 1×1 during startup and
            # when creating child windows. That is not an input-ready overlay.
            if name == "^Arcade Lens$" and not ("WIDTH=1920" in geometry and "HEIGHT=1080" in geometry):
                continue
            # Xvfb has no window manager. Explicit automation focus is not
            # evidence that Lens's native remapping always takes focus itself.
            # Without a window manager, a peer window opened by an earlier
            # step (Look's preview) can also stack above Lens's overlay.
            s.xdotool("windowraise", win)
            s.xdotool("windowfocus", "--sync", win)
            if s.xdotool("getwindowfocus") == win:
                if name == "^Arcade Lens$":
                    # XSync confirms the X server, not that winit/egui has
                    # processed FocusIn. A harmless modifier event wakes a
                    # frame; wait for the application's focus acknowledgement
                    # before sending Escape (or a palette key).
                    log = s.root / "arcade.lens.log"
                    offset = log.stat().st_size
                    s.xdotool("key", "Shift_L")
                    _wait(lambda: "focused=true, visible=true" in log.read_text(errors="replace")[offset:],
                          "Lens did not process FocusIn", timeout=3)
                return win
        time.sleep(0.05)
    raise AssertionError(f"no mapped, full-size, focused Lens window: {name}; "
                         f"focus={s.xdotool('getwindowfocus')}; windows={s.xdotool('search', '--name', name)}; "
                         f"visible={s.xdotool('search', '--onlyvisible', '--name', name)}; "
                         f"Lens log:\n{s.log('arcade.lens')}")


def _closed(s, name="^Arcade Lens$"):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if not s.xdotool("search", "--onlyvisible", "--name", name):
            return
        time.sleep(0.05)
    raise AssertionError(f"Lens window did not unmap: {name}")


def _finish(p):
    out, err = p.communicate(timeout=30)
    return p.returncode, json.loads(out) if p.returncode == 0 else (err or out)


def _wait(predicate, message, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError(message)


@check("lens")
def recognize_one_shot_and_resident(s):
    png = _png(s)
    exe = APPS["arcade.lens"]["dir"] / APPS["arcade.lens"]["bin"]
    request = {"v": 1, "id": 7, "method": "invoke", "params": {
        "action": "lens.recognize", "inputs": [{"type": "file/image", "path": str(png)}],
        "options": {}, "context": {"source": "e2e", "interactive": False, "reason": "test"}}}
    r = subprocess.run([str(exe), "--arcade-invoke"], input=json.dumps(request) + "\n",
                       env=s.env, capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stdout + r.stderr
    reply = next(json.loads(line) for line in r.stdout.splitlines() if '"result"' in line)
    assert reply["id"] == 7, reply
    assert not s.endpoint("arcade.lens").exists(), "one-shot started a listener"
    assert not s.xdotool("search", "--onlyvisible", "--name", "Arcade Lens"), "one-shot opened a window"
    one = next(o for o in reply["result"]["outputs"] if o["type"] == "structured/findings")
    assert one["data"], one
    _start(s)
    code, resident = s.invoke("lens", "lens.recognize", "--file", str(png), timeout=120)
    assert code == 0, resident
    found = next(o for o in resident["outputs"] if o["type"] == "structured/findings")
    assert found["data"] == one["data"], (one, found)
    return f"one-shot and resident: {[f['capability'] for f in found['data']]}"


@check("lens")
def capture_drag_returns_pixels_and_rectangle(s):
    _start(s)
    p = s.invoke("lens", "lens.capture", background=True)
    _focus(s)
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2", "mousemove", "340", "260", "sleep", "0.2", "mouseup", "1")
    code, r = _finish(p)
    assert code == 0, r
    file = next(o for o in r["outputs"] if o["type"] == "file/image")
    rect = next(o for o in r["outputs"] if o["type"] == "screen/region")["data"]["rect"]
    assert rect == {"x": 100, "y": 100, "width": 240, "height": 160}, rect
    data = Path(file["path"]).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", file
    assert struct.unpack(">II", data[16:24]) == (240, 160), file
    _closed(s)
    return json.dumps(rect)


@check("lens")
def capture_escape_reports_user_cancelled(s):
    _start(s)
    p = s.invoke("lens", "lens.capture", background=True)
    _focus(s)
    s.xdotool("key", "Escape")
    code, r = _finish(p)
    assert code != 0 and "user_cancelled" in r and "denied" in r, r
    _closed(s)
    return r.strip()


@check("lens")
def analyze_opens_selected_image(s):
    _start(s)
    code, r = s.invoke("lens", "lens.analyze", "--file", str(_png(s)))
    assert code == 0 and r["message"] == "Opened in Arcade Lens", r
    _focus(s)
    s.xdotool("key", "Escape")
    _closed(s)
    return r["message"]


@check("lens")
def pin_opens_a_floating_window(s):
    _start(s)
    code, r = s.invoke("lens", "lens.pin", "--file", str(_png(s)))
    assert code == 0 and r["message"] == "Pinned", r
    _focus(s, "^Arcade Lens Pin$")
    s.xdotool("key", "Escape")
    _closed(s, "^Arcade Lens Pin$")
    return r["message"]


@check("lens")
def capture_and_act_opens_the_palette(s):
    _start(s)
    code, r = s.invoke("lens", "lens.capture_and_act")
    assert code == 0 and r["message"] == "Arcade Lens is open", r
    _focus(s)
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2", "mousemove", "340", "260", "sleep", "0.2", "mouseup", "1")
    time.sleep(0.8)
    # The universal Copy Image action proves the palette is active.
    s.xdotool("key", "ctrl+c")
    time.sleep(0.3)
    assert "panic" not in s.log("arcade.lens"), s.log("arcade.lens")
    s.xdotool("key", "Escape")
    _closed(s)
    return "selection and palette Copy Image shortcut completed"


@check("lens")
def pin_and_analyze_own_pixels_before_success(s):
    _start(s)
    for action, name in [("lens.pin", "^Arcade Lens Pin$"), ("lens.analyze", "^Arcade Lens$")]:
        path = _png(s, f"{action}-ephemeral.png", 256, 144)
        code, result = s.invoke("lens", action, "--file", str(path))
        assert code == 0, result
        path.unlink()  # SPEC §5.7: the caller may clean up immediately.
        win = _focus(s, name)
        if action == "lens.analyze":
            _palette(s, lambda es: any(e["action"] == "core.region.copy" for e in es))
        s.screenshot(f"lens-{action.split('.')[1]}-after-input-deleted", win)
        s.xdotool("key", "Escape")
        _closed(s, name)
    bad = s.root / "lens-invalid.png"
    bad.write_bytes(b"not an image")
    code, result = s.invoke("lens", "lens.pin", "--file", str(bad))
    assert code != 0 and "unsupported_input" in result, result
    return "caller deleted both inputs immediately after success; pin and analyzed palette still rendered; invalid image failed"


@check("lens")
def capture_mode_pin_and_color(s):
    _start(s)
    code, result = s.invoke("lens", "lens.capture_and_act", "--option", "mode=pin")
    assert code == 0, result
    _focus(s)
    s.screenshot("lens-capture-pin-mode")
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2", "mousemove", "340", "260", "sleep", "0.2", "mouseup", "1")
    win = _focus(s, "^Arcade Lens Pin$")
    _closed(s)
    s.screenshot("lens-capture-pin-result", win)
    s.xdotool("key", "Escape")
    _closed(s, "^Arcade Lens Pin$")
    s.lens_palette_offset = len(_lens_log(s))
    code, result = s.invoke("lens", "lens.capture_and_act", "--option", "mode=color")
    assert code == 0, result
    _focus(s)
    s.screenshot("lens-capture-color-mode")
    s.xdotool("mousemove", "120", "120", "sleep", "0.1", "click", "1")
    entries = _palette(s, lambda es: any(e["action"] == "core.color.copy-hex" for e in es))
    assert any(e["capability"] == "color" for e in entries), entries
    s.screenshot("lens-capture-color-point")
    _close_palette(s)
    code, result = s.invoke("lens", "lens.capture_and_act", "--option", "mode=color")
    assert code == 0, result
    s.lens_palette_offset = len(_lens_log(s))
    _focus(s)
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2", "mousemove", "340", "260", "sleep", "0.2", "mouseup", "1")
    _palette(s, lambda es: any(e["capability"] == "color" for e in es))
    s.screenshot("lens-capture-color-region")
    _close_palette(s)
    return "mode=pin selected and pinned directly; mode=color sampled a pixel and selected a region into color findings"


def _lens_log(s):
    return (s.root / "arcade.lens.log").read_text(errors="replace")


def _palette(s, predicate=lambda entries: bool(entries)):
    def read():
        lines = _lens_log(s)[getattr(s, "lens_palette_offset", 0):].splitlines()
        for line in reversed(lines):
            if "palette actions: " in line:
                entries = json.loads(line.split("palette actions: ", 1)[1])
                return entries if predicate(entries) else None
        return None
    return _wait(read, "Lens palette did not reach the expected cached state")


def _analyze(s, path, predicate=lambda entries: bool(entries)):
    # Lens ignores analyze while an overlay is open, so a check that failed
    # with its palette up must not break the next one.
    _close_palette(s)
    s.lens_palette_offset = len(_lens_log(s))
    code, result = s.invoke("lens", "lens.analyze", "--file", str(path))
    assert code == 0, result
    _focus(s)
    return _palette(s, predicate)


def _close_palette(s):
    for _ in range(3):
        if not s.xdotool("search", "--onlyvisible", "--name", "^Arcade Lens$"):
            return
        _focus(s)
        s.xdotool("key", "Escape")
        time.sleep(0.15)
    _closed(s)


def _filter(s, text, rendered=True):
    # Space opens More with the filter focused. Refocus first: first-run peer
    # windows can briefly take focus after mapping in Xvfb. (OCR can't read
    # the faint "Filter actions…" placeholder reliably, so it isn't clicked.)
    _focus(s)
    s.xdotool("key", "space")
    time.sleep(0.3)
    _focus(s)
    s.xdotool("key", "ctrl+a")
    s.xdotool("type", "--clearmodifiers", "--delay", "20", text)
    if not rendered:
        time.sleep(0.5)
        return
    shown = lambda: any(_near(line["text"], text) for line in _ui_lines(s))
    try:
        _wait(shown, f"Lens did not render filter {text!r}", timeout=5)
    except AssertionError:
        # A peer window mapping late can still steal the keystrokes once.
        s.screenshot(f"lens-filter-retry-{text.replace(' ', '-')}")
        _focus(s)
        s.xdotool("key", "ctrl+a")
        s.xdotool("type", "--clearmodifiers", "--delay", "20", text)
        _wait(shown, f"Lens did not render filter {text!r}")


OCR_SCALE = 3


def _has(line, text):
    """OCR sometimes drops the spaces between short words."""
    return "".join(text.lower().split()) in "".join(line.lower().split())


def _near(line, text):
    """Like _has, but tolerates a misread character or two (the filter's text
    cursor reads as a letter; the row icon splits words)."""
    want, got = "".join(text.lower().split()), "".join(line.lower().split())
    return any(difflib.SequenceMatcher(None, want, got[i:i + len(want)]).ratio() >= 0.85
               for i in range(max(1, len(got) - len(want) + 2)))


def _ui_lines(s, win="root"):
    """Text lines on screen. egui's small antialiased labels need enlarging
    for Tesseract; dark surfaces (the palette) also read far better inverted
    and thresholded, light ones (Settings) as they are, so both are read."""
    shot = s.root / "lens-ui.png"
    subprocess.run(["import", "-window", win, str(shot)], env=s.env, check=True)
    out = []
    for prep in (["-resize", f"{OCR_SCALE * 100}%"],
                 ["-colorspace", "Gray", "-negate", "-resize", f"{OCR_SCALE * 100}%", "-threshold", "60%"]):
        img = s.root / "lens-ui-ocr.png"
        subprocess.run(["magick", str(shot), *prep, str(img)], env=s.env, check=True)
        r = subprocess.run(["tesseract", str(img), "stdout", "--psm", "11", "tsv"],
                           env=dict(s.env, OMP_THREAD_LIMIT="1"), capture_output=True, text=True, timeout=15)
        assert r.returncode == 0, r.stderr
        lines = {}
        for word in csv.DictReader(io.StringIO(r.stdout), delimiter="\t"):
            if word.get("text", "").strip():
                key = tuple(word[k] for k in ("block_num", "par_num", "line_num"))
                lines.setdefault(key, []).append(word)
        out += [{"text": " ".join(w["text"] for w in words), "words": words} for words in lines.values()]
    return out


def _ui_click(s, text, win="root", occurrence=0):
    def find():
        matches = [line for line in _ui_lines(s, win) if _has(line["text"], text)]
        # Both OCR passes may report the same line: keep one per row, top first.
        rows = {}
        for line in sorted(matches, key=lambda l: int(l["words"][0]["top"])):
            row = int(line["words"][0]["top"]) // (12 * OCR_SCALE)
            rows.setdefault(row, line)
        matches = list(rows.values())
        return matches[occurrence] if len(matches) > occurrence else None
    line = _wait(find, f"Lens did not render {text!r}")
    first = text.split()[0].lower()
    word = next((w for w in line["words"] if w["text"].lower().strip("·↗…. ").startswith(first)), line["words"][0])
    x = str((int(word["left"]) + int(word["width"]) // 2) // OCR_SCALE)
    y = str((int(word["top"]) + int(word["height"]) // 2) // OCR_SCALE)
    args = ("--window", win) if win != "root" else ()
    # egui acts on a click only after a frame has seen the pointer hover.
    s.xdotool("mousemove", *args, x, y, "sleep", "0.15", "click", "1")


def _qr(s, name, text):
    path = s.root / name
    r = subprocess.run(["qrencode", "-o", str(path), "-s", "8", "-m", "4", text],
                       env=s.env, capture_output=True, text=True, timeout=10)
    assert r.returncode == 0, r.stderr
    return path


def _clipboard_driver(s, *requests):
    exe = APPS["arcade.clipboard"]["dir"] / "target/debug/arcade_test_driver"
    assert exe.exists(), "Clipboard's real test driver must be built by its agent"
    p = subprocess.Popen([str(exe)], env=s.env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, text=True, bufsize=1)
    replies = []
    try:
        for req in requests:
            p.stdin.write(json.dumps(req) + "\n")
            p.stdin.flush()
            readable, _, _ = select.select([p.stdout], [], [], 20)
            assert readable, f"Clipboard driver did not answer {req['op']}"
            line = p.stdout.readline()
            assert line, f"Clipboard driver exited during {req['op']}"
            reply = json.loads(line)
            assert "ok" in reply, reply
            replies.append(reply["ok"])
        return replies
    finally:
        p.stdin.close()
        try:
            p.wait(timeout=1)
        except subprocess.TimeoutExpired:
            # Its shutdown response is complete, but a peer-discovery blocking
            # task currently prevents the test driver's Tokio runtime exiting.
            p.terminate()
            p.wait(timeout=5)
        p.stdout.close()
        p.stderr.close()


def _clipboard_init(s):
    return {"op": "initialize", "data_dir": s.env["ARCADE_DATA_DIR"],
            "device_name": "Lens e2e", "link": {"enabled": False}}


def _clipboard_stop(s):
    s.cli("quit", "clipboard", check=True)
    s.procs["arcade.clipboard"].wait(timeout=10)
    s.procs.pop("arcade.clipboard")


def _settings(s):
    exe = APPS["arcade.lens"]["dir"] / APPS["arcade.lens"]["bin"]
    r = subprocess.run([str(exe)], env=s.env, capture_output=True, text=True, timeout=15)
    assert r.returncode == 0, r.stderr
    win = _focus(s, "^Arcade Lens Settings$")
    s.xdotool("windowraise", win)
    _ui_click(s, "Connected apps", win)
    _wait(lambda: any(_has(l["text"], "Connect with other Arcade apps") for l in _ui_lines(s, win)),
          "Lens Connected apps page did not render")
    return win


def _restart_lens(s):
    exe = APPS["arcade.lens"]["dir"] / APPS["arcade.lens"]["bin"]
    r = subprocess.run([str(exe), "--quit"], env=s.env, capture_output=True, text=True, timeout=10)
    assert r.returncode == 0, r.stderr
    s.procs.pop("arcade.lens").wait(timeout=10)
    _start(s)


@check("lens")
def standalone_palette_and_repeated_capture_cancellation(s):
    _start(s)
    entries = _analyze(s, _png(s), lambda es: any(e["action"] == "core.region.copy" for e in es))
    assert all(not e["action"].startswith("arcade.") for e in entries), entries
    s.screenshot("lens-palette-without-peers")
    _close_palette(s)
    for _ in range(5):
        p = s.invoke("lens", "lens.capture", background=True)
        _focus(s)
        s.xdotool("key", "Escape")
        code, result = _finish(p)
        assert code != 0 and "user_cancelled" in result and "denied" in result, result
        _closed(s)
    status = json.loads(s.cli("status", "lens", check=True).stdout)
    assert status["status"]["mode"] == "background", status
    return "no peer entries; capture Escape passed five immediate remaps; startup mode=background"


@check("lens")
def connected_apps_without_peers_and_get_fallback(s):
    browser = s.root / "lens-test-browser"
    opened = s.root / "lens-opened-url.json"
    browser.write_text("#!/usr/bin/env python3\nimport json,sys\nfrom pathlib import Path\n"
                       f"Path({str(opened)!r}).write_text(json.dumps(sys.argv[1:]))\n")
    browser.chmod(0o700)
    s.env["BROWSER"] = str(browser)
    apps = Path(s.env["XDG_DATA_HOME"]) / "applications"
    apps.mkdir(parents=True, exist_ok=True)
    (apps / "lens-e2e-browser.desktop").write_text(
        f"[Desktop Entry]\nType=Application\nName=Lens test browser\nExec={browser} %u\n"
        "MimeType=x-scheme-handler/https;x-scheme-handler/http;\nNoDisplay=true\n")
    (Path(s.env["XDG_CONFIG_HOME"]) / "mimeapps.list").write_text(
        "[Default Applications]\nx-scheme-handler/https=lens-e2e-browser.desktop\n"
        "x-scheme-handler/http=lens-e2e-browser.desktop\n")
    try:
        win = _settings(s)
        lines = "\n".join(l["text"] for l in _ui_lines(s, win))
        assert _has(lines, "Not installed") and not _has(lines, "Use with Arcade Lens"), lines  # §10.2: no toggle for a missing app
        s.screenshot("lens-connected-apps-without-peers", win)
        _ui_click(s, "Get", win)
        _wait(opened.exists, "Lens Get did not open the isolated release URL")
        urls = json.loads(opened.read_text())
        assert len(urls) == 1 and urls[0].startswith("https://github.com/qa-p1/Arcade-") and urls[0].endswith("/releases"), urls
    finally:
        _restart_lens(s)
    return f"missing peers without toggles, Get and diagnostics; Get opened {urls[0]}"


@check("lens")
def region_send_real_clipboard_and_secret_guard(s):
    _clipboard_driver(s, _clipboard_init(s), {"op": "create_mesh", "device_name": "Lens e2e"}, {"op": "shutdown"})
    s.start("arcade.clipboard")  # late start: the already-running Lens must discover it
    entries = _analyze(s, _png(s, "lens-send.png"),
                       lambda es: any(e["action"] == "arcade.clipboard.add" and e["key"] == "m" for e in es))
    assert any(e["capability"] == "region" and e["action"] == "arcade.clipboard.add" for e in entries)
    s.screenshot("lens-palette-with-peers")
    _filter(s, "Send to my devices")
    s.screenshot("lens-send-payload-preview")
    _close_palette(s)
    # Exercise the actual capture flow, rather than only analyzing a file.
    s.lens_palette_offset = len(_lens_log(s))
    code, result = s.invoke("lens", "lens.capture_and_act")
    assert code == 0, result
    _focus(s)
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2", "mousemove", "340", "260", "sleep", "0.2", "mouseup", "1")
    _palette(s, lambda es: any(e["action"] == "arcade.clipboard.add" and e["key"] == "m" for e in es))
    s.xdotool("key", "m")
    _wait(lambda: "completed action arcade.clipboard.add" in _lens_log(s)[s.lens_palette_offset:],
          "Lens did not complete clipboard.add")
    _closed(s)
    _clipboard_stop(s)
    replies = _clipboard_driver(s, _clipboard_init(s), {"op": "history"}, {"op": "shutdown"})
    rows = replies[1] if isinstance(replies[1], list) else replies[1]["items"]
    assert any(row["kind"] == "image" for row in rows), rows
    s.start("arcade.clipboard")
    secret = _qr(s, "lens-secret.png", "password=lens-synthetic-password-123456")
    code, recognized = s.invoke("lens", "lens.recognize", "--file", str(secret))
    assert code == 0, recognized
    findings = next(o["data"] for o in recognized["outputs"] if o["type"] == "structured/findings")
    assert any(f["capability"] == "secret" for f in findings), findings
    entries = _analyze(s, secret, lambda es: any(e["capability"] == "secret" for e in es))
    assert not any(e["action"] == "arcade.clipboard.add" for e in entries), entries
    # The palette log above is the assertion; this shows the filtered palette
    # (no "Send to my devices") for review.
    _filter(s, "Send", rendered=False)
    s.screenshot("lens-secret-guard")
    _close_palette(s)
    return "region M sent an image into real Clipboard history; decoded secret removed all Clipboard sends"


@check("lens")
def quick_look_path_finding_real_peer(s):
    s.env["ALOOK_E2E_MAP_EARLY"] = "1"
    s.start("arcade.look")
    target = _png(s, "lens-preview-target.png", width=320, height=200)
    qr = _qr(s, "lens-path-qr.png", str(target))
    entries = _analyze(s, qr, lambda es: any(e["action"] == "arcade.look.preview" and e["key"] == "y" for e in es))
    assert any(e["action"] == "arcade.look.preview" and e["capability"] == "path" for e in entries), entries
    _filter(s, "Quick Look")
    s.screenshot("lens-quick-look-preview")
    s.xdotool("key", "Escape", "y")
    _wait(lambda: "completed action arcade.look.preview" in _lens_log(s)[s.lens_palette_offset:],
          "Lens did not complete look.preview")
    code, result = s.invoke("look", "look.inspect", "--file", str(target))
    assert code == 0 and result["outputs"][0]["data"]["width"] == 320, result
    _close_palette(s)
    return "existing path decoded from screen pixels → Y → real look.preview; target inspected at 320×200"


@check("lens")
def command_finding_opens_real_wheel_settings(s):
    # Windows left by earlier checks (Look's preview, Clipboard) would stack
    # over Wheel's Settings on a display without a window manager.
    for app in ("arcade.look", "arcade.clipboard"):
        s.kill(app)
    s.start("arcade.wheel")
    command = "/usr/bin/printf 'lens command'"
    qr = _qr(s, "lens-command-qr.png", command)
    entries = _analyze(s, qr, lambda es: any(e["action"] == "arcade.wheel.add_action" and e["capability"] == "command" for e in es))
    _filter(s, "Add to Wheel")
    s.screenshot("lens-add-to-wheel-palette")
    s.xdotool("key", "Return")
    win = s.wait_window("Arcade Wheel.*Settings")
    s.xdotool("windowsize", win, "1240", "820")
    s.xdotool("windowfocus", "--sync", win)
    time.sleep(1.0)  # let Settings paint before the screenshot
    s.screenshot("lens-wheel-command-prefilled", win)
    # Lens hands the job off and closes its overlay, so Wheel's window is
    # reachable (a full-screen overlay used to cover it).
    s.screenshot("lens-wheel-command-root")
    configs = [p for p in (s.root / "config").rglob("config.json") if "Arcade Wheel" in str(p)]
    assert len(configs) == 1, configs
    before = configs[0].read_bytes()
    # Wheel's single explicit confirmation: choose a slot, then "Add to
    # selected slot". Lens must wait while that owner UI is open.
    assert "completed action arcade.wheel.add_action" not in _lens_log(s)[s.lens_palette_offset:]
    click = lambda x, y: s.xdotool("windowraise", win, "windowfocus", win, "mousemove", "--window", win, str(x), str(y), "sleep", "0.15", "click", "1")
    click(825, 187)  # Slot
    s.xdotool("key", "Home", "Return")
    time.sleep(0.4)
    assert configs[0].read_bytes() == before, "pending Wheel draft silently changed a deck"
    s.screenshot("lens-wheel-command-confirmation", win)
    click(960, 757)  # Add to selected slot
    _wait(lambda: "completed action arcade.wheel.add_action" in _lens_log(s)[s.lens_palette_offset:],
          "Wheel did not confirm the Lens command")
    saved = json.loads(configs[0].read_text())["decks"][0]["actions"][0]
    assert saved["type"] == "command" and saved["payload"]["command"] == command, saved
    s.xdotool("windowfocus", win, "key", "ctrl+w")
    _close_palette(s)
    return "command finding → Wheel Settings draft → explicit slot/save; saved direct command exactly"


def _box_start_with_pipelines(s):
    db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
    if not db.exists():
        exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
        r = subprocess.run([str(exe), "--arcade-invoke"], env=s.env, capture_output=True, text=True,
                           input=json.dumps({"v": 1, "id": 1, "method": "invoke", "params": {"action": "box.pipelines"}}) + "\n",
                           timeout=20)
        assert r.returncode == 0, r.stdout + r.stderr
    pipelines = [
        {"id": "lens-web-image", "name": "Lens web image", "version": 1,
         "nodes": [{"id": "convert", "toolId": "arcade.image.convert",
                    "inputs": [{"kind": "external", "index": 0}], "options": {"format": "webp"}}],
         "outputNodes": ["convert"]},
        {"id": "lens-uppercase", "name": "Lens uppercase", "version": 1,
         "nodes": [{"id": "upper", "toolId": "arcade.text.case",
                    "inputs": [{"kind": "external", "index": 0}], "options": {"mode": "upper"}}],
         "outputNodes": ["upper"]},
    ]
    with sqlite3.connect(db) as conn:
        conn.execute("INSERT OR REPLACE INTO settings(key,value) VALUES('onboarding_complete','true')")
        for p in pipelines:
            conn.execute("INSERT OR REPLACE INTO pipelines(id,version,definition_json) VALUES(?,?,?)",
                         (p["id"], 1, json.dumps(p)))
    s.start("arcade.box")
    def ready():
        actions = json.loads(s.cli("describe", "box", "--json", check=True).stdout)
        return any(a["id"] == "box:arcade.image.convert#webp" and a.get("available", True) for a in actions)
    _wait(ready, "Box image provider did not become available", timeout=20)


@check("lens")
def real_box_preset_and_cached_pipeline_entries(s):
    _box_start_with_pipelines(s)  # Box arrives after Lens: watch + cache refresh.
    image = _png(s, "lens-box-image.png", 240, 160)
    entries = _analyze(s, image, lambda es: any(e["action"] == "arcade.box.pipeline.lens-web-image" for e in es))
    assert not any(e["action"] == "arcade.box.pipeline.lens-uppercase" for e in entries), entries
    _filter(s, "Convert to WebP")
    s.screenshot("lens-box-preset-palette")
    s.xdotool("key", "Return")
    _wait(lambda: "completed action arcade.box:arcade.image.convert#webp" in _lens_log(s)[s.lens_palette_offset:],
          "Lens did not complete the real Box preset")
    _closed(s)
    outputs = list(s.root.rglob("*.webp"))
    assert outputs and outputs[0].read_bytes().startswith(b"RIFF"), outputs
    entries = _analyze(s, image, lambda es: any(e["action"] == "arcade.box.pipeline.lens-web-image" for e in es))
    _filter(s, "Lens web image")
    s.screenshot("lens-box-image-pipeline")
    s.xdotool("key", "Return")
    _wait(lambda: "completed action arcade.box.pipeline.lens-web-image" in _lens_log(s)[s.lens_palette_offset:],
          "Lens did not complete the image pipeline")
    _closed(s)
    qr = _qr(s, "lens-pipeline-text.png", "hello lens pipeline")
    _analyze(s, qr, lambda es: any(e["action"] == "arcade.box.pipeline.lens-uppercase" for e in es))
    _filter(s, "Lens uppercase")
    s.screenshot("lens-box-text-pipeline")
    s.xdotool("key", "Return")
    _wait(lambda: "completed action arcade.box.pipeline.lens-uppercase" in _lens_log(s)[s.lens_palette_offset:],
          "Lens did not complete the text pipeline")
    _closed(s)
    return "real Box preset produced WebP; cached image and QR-text pipeline entries matched and ran"


@check("lens")
def connected_apps_real_toggles_and_shortcut_clash(s):
    settings = Path(s.env["ARCADE_LENS_HOME"]) / "config/settings.toml"
    manifest = Path(s.env["ARCADE_HOME"]) / "apps/arcade.lens.json"
    try:
        win = _settings(s)
        lines = "\n".join(l["text"] for l in _ui_lines(s, win))
        assert _has(lines, "Running") and _has(lines, "Use with Arcade Lens"), lines
        _ui_click(s, "Diagnostics", win)
        s.screenshot("lens-connected-apps", win)
        _ui_click(s, "Use with Arcade Lens", win)
        _ui_click(s, "Save", win, occurrence=-1)  # the button, not "Save to apply changes"
        _wait(lambda: settings.exists() and "arcade.box" in tomllib.loads(settings.read_text())["link"]["disabled_peers"],
              "Lens did not persist the Box toggle")
        _restart_lens(s)
        entries = _analyze(s, _png(s), lambda es: any(e["action"] == "arcade.clipboard.add" for e in es))
        assert not any(e["action"].startswith("arcade.box") for e in entries), entries
        _close_palette(s)
        win = _settings(s)
        _ui_click(s, "Use with Arcade Lens", win)
        _ui_click(s, "Connect with other Arcade apps", win)
        _ui_click(s, "Save", win, occurrence=-1)  # the button, not "Save to apply changes"
        _wait(lambda: not s.endpoint("arcade.lens").exists(), "master off left a Lens listener")
        off = json.loads(manifest.read_text())
        assert not off["settings"]["linkEnabled"] and not off["actions"], off
        s.screenshot("lens-connected-apps-master-off", win)
        _ui_click(s, "Connect with other Arcade apps", win)
        _ui_click(s, "Save", win, occurrence=-1)  # the button, not "Save to apply changes"
        s.wait_running("arcade.lens")
        assert json.loads(manifest.read_text())["actions"], "master on did not republish Lens actions"
        # A running Box holds ctrl+alt+space as an X11 key grab, so the key would
        # never reach the recorder. Stop it: the clash comes from its cached manifest.
        s.kill("arcade.box", signal.SIGTERM)
        _ui_click(s, "Shortcut", win)
        _ui_click(s, "Change", win)
        s.xdotool("key", "ctrl+alt+space")
        try:
            _wait(lambda: any(_has(l["text"], "Used by Arcade Box") for l in _ui_lines(s, win)),
                  "recorder did not identify Box's cached shortcut")
        except AssertionError:
            s.screenshot("lens-shortcut-clash-failed", win)
            raise
        s.screenshot("lens-shortcut-clash", win)
        _ui_click(s, "Revert", win)  # the test's draft must not change the saved shortcut
    finally:
        _restart_lens(s)
    return "peer off hid Box entries; master off stopped listener/cleared actions; restored; cached recorder clash rendered"
