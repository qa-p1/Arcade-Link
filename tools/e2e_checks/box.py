"""Arcade Box real-run checks. Loaded by tools/e2e.py, always isolated."""

import hashlib
import csv
import io
import json
import re
import os
import signal
import socket
import sqlite3
import struct
import subprocess
import time
import zlib
from pathlib import Path


def png(path, width=24, height=16):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = (b"\0" + bytes([30, 100, 220]) * width) * height
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return path


def resident(s):
    # WebKit software painting avoids transparent-window artifacts on the
    # private Xvfb display, which deliberately has no compositor.
    s.env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"
    if "arcade.box" not in s.procs or s.procs["arcade.box"].poll() is not None:
        browser = s.root / "box-test-browser"
        browser.write_text("#!/usr/bin/env python3\nimport os,sys,json\nfrom pathlib import Path\nPath(os.environ['BOX_E2E_OPENED_URL']).write_text(json.dumps(sys.argv[1:]))\n")
        browser.chmod(0o700)
        s.env["BROWSER"] = str(browser)
        s.env["BOX_E2E_OPENED_URL"] = str(s.root / "box-opened-url.json")
        apps = Path(s.env["XDG_DATA_HOME"]) / "applications"
        apps.mkdir(parents=True, exist_ok=True)
        (apps / "box-e2e-browser.desktop").write_text(
            f"[Desktop Entry]\nType=Application\nName=Box test browser\nExec={browser} %u\nMimeType=x-scheme-handler/https;x-scheme-handler/http;\nNoDisplay=true\n")
        (Path(s.env["XDG_CONFIG_HOME"]) / "mimeapps.list").write_text(
            "[Default Applications]\nx-scheme-handler/https=box-e2e-browser.desktop\nx-scheme-handler/http=box-e2e-browser.desktop\n")
        db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
        if not db.exists():
            exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
            initialized = subprocess.run([str(exe), "--arcade-invoke"], env=s.env, capture_output=True, text=True,
                input=json.dumps({"v":1,"id":1,"method":"invoke","params":{"action":"box.pipelines"}}) + "\n", timeout=20)
            assert initialized.returncode == 0, initialized.stdout + initialized.stderr
        with sqlite3.connect(db) as conn:
            conn.execute("INSERT OR REPLACE INTO settings(key, value) VALUES('onboarding_complete','true')")
        s.start("arcade.box")
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        actions = json.loads(s.cli("describe", "box", "--json", check=True).stdout)
        if any(a["id"] == "box:arcade.image.convert#webp" and a.get("available", True) for a in actions):
            return actions
        time.sleep(.1)
    raise AssertionError("Box's vips provider did not become available")


class Wire:
    def __init__(self, s, app="arcade.box"):
        endpoint = json.loads(s.endpoint(app).read_text())
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(40)
        self.sock.connect(endpoint["address"])
        self.reader = self.sock.makefile("r")
        self.next_id = 0
        self.events = []
        self.call("hello", {"token": endpoint["token"], "client": {"id": "arcade.test", "version": "1"}, "protocol": [1]})

    def read(self):
        line = self.reader.readline()
        assert line, "Link disconnected without a result"
        return json.loads(line)

    def call(self, method, params):
        self.next_id += 1
        self.sock.sendall((json.dumps({"v": 1, "id": self.next_id, "method": method, "params": params}) + "\n").encode())
        while True:
            message = self.read()
            if message.get("id") == self.next_id:
                return message
            self.events.append(message)

    def invoke(self, action, inputs=(), options=None):
        return self.call("invoke", {"action": action, "inputs": list(inputs), "options": options or {},
                                   "context": {"source": "arcade.test", "interactive": True, "reason": "user-click"}})

    def done(self, job):
        for message in self.events:
            if message.get("method") == "job.done" and message["params"]["job"] == job:
                return message["params"]
        while True:
            message = self.read()
            self.events.append(message)
            if message.get("method") == "job.done" and message["params"]["job"] == job:
                return message["params"]

    def close(self):
        self.reader.close()
        self.sock.close()


@check("box")
def preset_resident_progress_and_delegated_files(s):
    resident(s)
    source = png(s.root / "outside-box.png")
    before = hashlib.sha256(source.read_bytes()).hexdigest()
    wire = Wire(s)
    try:
        request = wire.invoke("box:arcade.image.convert#webp", [{"type": "file/image", "path": str(source)}])
        job = request["result"]["job"]
        done = wire.done(job)
        assert done["status"] == "success", done
        assert any(e.get("method") == "job.progress" for e in wire.events), wire.events
        output = Path(done["outputs"][0]["path"])
        assert output.is_file() and output.suffix == ".webp" and output != source, done
        assert hashlib.sha256(source.read_bytes()).hexdigest() == before
        second = wire.invoke("box:arcade.image.convert#webp", [{"type": "file/image", "path": str(source)}])
        again = wire.done(second["result"]["job"])
        assert again["status"] == "success", again
        assert again["outputs"][0]["path"] != str(output), "output was overwritten"
        return f"job.progress → job.done success; {output.name}; input unchanged; repeated output is a new file"
    finally:
        wire.close()


@check("box")
def saved_pipeline_list_and_run_contract(s):
    resident(s)
    s.kill("arcade.box", signal.SIGTERM)
    pipeline = {"id": "link-uppercase", "name": "Link uppercase", "version": 1,
                "nodes": [{"id": "upper", "toolId": "arcade.text.case", "inputs": [{"kind": "external", "index": 0}],
                           "options": {"mode": "upper"}}], "outputNodes": ["upper"]}
    db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
    with sqlite3.connect(db) as conn:
        conn.execute("INSERT OR REPLACE INTO pipelines(id, version, definition_json) VALUES(?, ?, ?)",
                     (pipeline["id"], 1, json.dumps(pipeline)))
    resident(s)
    code, result = s.invoke("box", "box.pipelines")
    assert code == 0, result
    offers = result["outputs"][0]["data"]
    assert offers == [{"id": "link-uppercase", "name": "Link uppercase", "version": 1,
                       "accepts": ["text/plain"], "produces": ["text/plain"], "effects": [], "interactive": False}], offers
    code, result = s.invoke("box", "box.pipeline.run", "--text", "hello pipeline", "--option", "pipeline=link-uppercase")
    assert code == 0 and result["outputs"][0]["text"] == "HELLO PIPELINE", result
    exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
    request = {"v": 1, "id": 1, "method": "invoke", "params": {
        "action": "box.pipeline.run", "options": {"pipeline": "link-uppercase"}, "inputs": [{"type": "text/plain", "text": "one shot"}]}}
    r = subprocess.run([str(exe), "--arcade-invoke"], env=s.env, input=json.dumps(request) + "\n",
                       text=True, capture_output=True, timeout=20)
    assert r.returncode == 0, r.stdout + r.stderr
    assert json.loads(r.stdout.splitlines()[-1])["result"]["outputs"][0]["text"] == "ONE SHOT", r.stdout
    return "structured/pipelines matches SPEC §5.4; options.pipeline runs resident and one-shot"


@check("box")
def oneshot_preset_has_no_ui_or_listener(s):
    source = png(s.root / "oneshot.png")
    isolated = s.root / "oneshot-arcade"
    env = dict(s.env, ARCADE_HOME=str(isolated), XDG_DATA_HOME=str(s.root / "oneshot-data"))
    request = {"v": 1, "id": 27, "method": "invoke", "params": {
        "action": "box:arcade.image.convert#webp", "inputs": [{"type": "file/image", "path": str(source)}]}}
    exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
    result = subprocess.run([str(exe), "--arcade-invoke"], input=json.dumps(request) + "\n", env=env,
                            capture_output=True, text=True, timeout=40)
    assert result.returncode == 0, result.stdout + result.stderr
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    assert any(m.get("method") == "job.progress" for m in messages), messages
    final = messages[-1]
    assert final["id"] == 27 and Path(final["result"]["outputs"][0]["path"]).is_file(), final
    assert not (isolated / "run/arcade.box.endpoint").exists()
    assert not (isolated / "apps/arcade.box.json").exists()
    return "--arcade-invoke: job.progress + final response id=27; no manifest/listener"


@check("box")
def unavailable_provider_reason_and_box_open(s):
    actions = resident(s)
    missing = next(a for a in actions if not a.get("available", True))
    assert missing.get("reason"), missing
    wire = Wire(s)
    try:
        status = wire.call("app.status", {})
        assert status["result"]["status"]["mode"] == "background", status
        denied = wire.invoke(missing["id"])
        assert denied["error"]["code"] == "unavailable" and denied["error"].get("reason"), denied
        opened = wire.invoke("box.open", [{"type": "text/plain", "text": "hello from Link"}],
                             {"tool": "arcade.text.case", "options": {"mode": "upper"}})
        assert opened["result"]["message"] == "Opened in Arcade Box", opened
        win = s.wait_window("Arcade Box")
        assert win, "Island did not open"
        s.xdotool("key", "Escape")
        return f"{missing['id']}: {denied['error']['reason']}; box.open displayed Island"
    finally:
        wire.close()


@check("box")
def cancel_long_job_removes_partial_outputs(s):
    resident(s)
    source = s.root / "long.mp4"
    generated = subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30",
                                "-t", "20", "-c:v", "mpeg4", "-q:v", "3", str(source)],
                               env=s.env, capture_output=True, text=True, timeout=60)
    assert generated.returncode == 0, generated.stderr
    wire = Wire(s)
    try:
        started = wire.invoke("box:arcade.video.compress#share-25mb", [{"type": "file/video", "path": str(source)}])
        job = started["result"]["job"]
        cancelled = wire.call("job.cancel", {"job": job})
        assert cancelled["result"]["cancelled"], cancelled
        done = wire.done(job)
        assert done["status"] == "cancelled" and not done.get("outputs"), done
        assert not list(s.root.glob("long_*.mp4")), "cancel left partial output"
        return "job.cancel → cancelled=true → job.done cancelled; no partial outputs"
    finally:
        wire.close()


@check("box")
def connected_apps_master_switch_stops_and_restarts_presence(s):
    resident(s)
    exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
    opened = subprocess.run([str(exe), "--settings"], env=s.env, capture_output=True, text=True, timeout=20)
    assert opened.returncode == 0, opened.stderr
    s.wait_window("Arcade Box")
    screenshot = s.root / "box-connected-apps.png"
    target = None
    for _ in range(3):
        time.sleep(.7)
        subprocess.run(["import", "-window", "root", str(screenshot)], env=s.env, check=True)
        text = subprocess.run(["tesseract", str(screenshot), "stdout", "tsv"], env=s.env, capture_output=True, text=True, timeout=10)
        words = list(csv.DictReader(io.StringIO(text.stdout), delimiter="\t"))
        target = next((w for w in words if w.get("text") == "Connect"), None)
        if target:
            break
    assert target, "Connected apps master switch did not render"
    # Click the actual label detected on the private display, then verify the
    # observable server state (no assumptions about hardcoded pixel positions).
    x = int(target["left"]) + int(target["width"]) // 2
    y = int(target["top"]) + int(target["height"]) // 2
    s.xdotool("mousemove", str(x), str(y), "click", "1")
    deadline = time.monotonic() + 5
    manifest = Path(s.env["ARCADE_HOME"]) / "apps/arcade.box.json"
    while time.monotonic() < deadline and s.endpoint("arcade.box").exists():
        time.sleep(.05)
    disabled = json.loads(manifest.read_text())
    assert not disabled["settings"]["linkEnabled"] and not disabled["actions"], disabled
    assert not s.endpoint("arcade.box").exists()
    s.xdotool("mousemove", str(x), str(y), "click", "1")
    s.wait_running("arcade.box")
    assert json.loads(manifest.read_text())["settings"]["linkEnabled"]
    time.sleep(.5)
    ui_click(s, "Connected apps")
    s.screenshot("box-connected-apps-no-peers", s.wait_window("Arcade Box"))
    ui_click(s, "Get")
    opened_url = Path(s.env["BOX_E2E_OPENED_URL"])
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline and not opened_url.exists():
        time.sleep(.05)
    assert opened_url.exists(), "Get did not open the isolated default browser"
    # The first Get row is whichever missing app sorts first (Arcade Tools today).
    opened = json.loads(opened_url.read_text())
    assert len(opened) == 1 and re.fullmatch(r"https://github\.com/qa-p1/Arcade-[a-z]+/releases", opened[0]), opened
    if os.environ.get("E2E_VERBOSE"):
        import shutil
        shutil.copy(screenshot, "/tmp/arcade-box-connected-apps.png")
    s.xdotool("key", "Escape")
    return "real Settings click: master off → empty actions/no endpoint; master on → serving again"


def ui_lines(s):
    win = s.wait_window("^Arcade Box$")
    s.xdotool("windowraise", win, "windowfocus", "--sync", win)
    geometry = dict(line.split("=", 1) for line in s.xdotool("getwindowgeometry", "--shell", win).splitlines() if "=" in line)
    s.box_ui_origin = (int(geometry["X"]), int(geometry["Y"]))
    shot = s.root / "box-ui.png"
    # Capture Box itself: its transparent Island canvas otherwise mixes peer
    # windows into OCR on Xvfb, which has no compositor.
    subprocess.run(["import", "-window", win, str(shot)], env=s.env, check=True)
    # Two readings: a hard threshold suits bold labels, enlarging first keeps
    # small text (file names, status) legible. Lines from both are kept.
    out = []
    for scale, prep in ((2, ["-colorspace", "Gray", "-threshold", "80%", "-resize", "200%"]),
                        (3, ["-resize", "300%", "-colorspace", "Gray", "-threshold", "70%"])):
        processed = s.root / "box-ui-ocr.png"
        subprocess.run(["magick", str(shot), *prep, str(processed)], env=s.env, check=True)
        result = subprocess.run(["tesseract", str(processed), "stdout", "--psm", "11", "tsv"], env=s.env,
                                capture_output=True, text=True, timeout=15)
        assert result.returncode == 0, result.stderr
        lines = {}
        for word in csv.DictReader(io.StringIO(result.stdout), delimiter="\t"):
            if not word.get("text", "").strip():
                continue
            key = tuple(word[k] for k in ("block_num", "par_num", "line_num"))
            lines.setdefault(key, []).append(word)
        def at(word):
            return ((int(word["left"]) + int(word["width"]) // 2) // scale + s.box_ui_origin[0],
                    (int(word["top"]) + int(word["height"]) // 2) // scale + s.box_ui_origin[1])
        for words in lines.values():
            x, y = at(words[0])
            out.append({"text": " ".join(w["text"] for w in words), "x": x, "y": y,
                        "words": [(w["text"], *at(w)) for w in words]})
    return sorted(out, key=lambda line: (line["y"], line["x"]))


def ui_wait(s, text, timeout=10):
    deadline = time.monotonic() + timeout
    seen = []
    while time.monotonic() < deadline:
        seen = ui_lines(s)
        # OCR sometimes drops the space between short words.
        line = next((line for line in seen if "".join(text.lower().split()) in "".join(line["text"].lower().split())), None)
        if line:
            return line
        time.sleep(.1)
    raise AssertionError(f"Box did not render {text!r}: {[line['text'] for line in seen]}")


def ui_click(s, text, last=False):
    ui_wait(s, text)
    lines = [line for line in ui_lines(s) if "".join(text.lower().split()) in "".join(line["text"].lower().split())]
    line = lines[-1] if last else lines[0]
    # Locate the requested words within the OCR line; its first word may be a
    # neighboring action in the same row.
    first = text.split()[0].lower()
    words = [w for w in line["words"] if w[0].lower().strip("·↗") == first]
    x, y = (words[-1] if last else words[0])[1:] if words else (line["x"], line["y"])
    s.xdotool("mousemove", str(x), str(y), "sleep", "0.15", "click", "1")


def open_tool(s, tool, inputs=(), options=None):
    resident(s)
    wire = Wire(s)
    try:
        reply = wire.invoke("box.open", inputs, {"tool": tool, "options": options or {}})
        assert "result" in reply, reply
    finally:
        wire.close()
    win = s.wait_window("Arcade Box")
    s.xdotool("windowfocus", "--sync", win)
    ui_wait(s, tool.rsplit(".", 1)[-1].replace("-", " "))
    return win


def run_open_tool(s):
    s.xdotool("key", "ctrl+Return")
    ui_wait(s, "converted", timeout=30)


def clipboard_driver(s, *requests):
    exe = APPS["arcade.clipboard"]["dir"] / "target/debug/arcade_test_driver"
    reply = subprocess.run([str(exe)], input="".join(json.dumps(r) + "\n" for r in requests),
                           env=s.env, capture_output=True, text=True, timeout=30)
    assert reply.returncode == 0, reply.stdout + reply.stderr
    rows = [json.loads(line) for line in reply.stdout.splitlines()]
    assert len(rows) == len(requests) and all("ok" in row for row in rows), rows
    return [row["ok"] for row in rows]


def clipboard_stop(s):
    if "arcade.clipboard" not in s.procs:
        return
    s.cli("quit", "clipboard", check=True)
    try:
        s.procs["arcade.clipboard"].wait(timeout=10)
    finally:
        s.kill("arcade.clipboard", signal.SIGTERM)


def clipboard_initialize(s):
    return {"op": "initialize", "data_dir": s.env["ARCADE_DATA_DIR"],
            "device_name": "Box e2e", "link": {"enabled": False}}


def clipboard_mesh(s):
    clipboard_stop(s)
    clipboard_driver(s, clipboard_initialize(s), {"op": "create_mesh", "device_name": "Box e2e"},
                     {"op": "shutdown"})
    s.start("arcade.clipboard")


@check("box")
def result_row_missing_and_real_peer_actions(s):
    # Start with no other app registered: earlier checks may have left peers
    # installed (a manifest without a process still counts as installed).
    for app in ("arcade.lens", "arcade.look", "arcade.wheel", "arcade.clipboard"):
        s.kill(app)
        (s.root / "arcade/apps" / f"{app}.json").unlink(missing_ok=True)
    source = png(s.root / "box-result.png", 240, 160)
    win = open_tool(s, "arcade.image.convert", [{"type": "file/image", "path": str(source)}],
                    {"format": "webp"})
    run_open_tool(s)
    lines = "\n".join(line["text"] for line in ui_lines(s))
    assert "sendtomydevices" not in "".join(lines.lower().split()) and "addtowheel" not in "".join(lines.lower().split()), lines
    s.screenshot("box-result-no-peers", win)
    clipboard_mesh(s)
    s.env["ALOOK_E2E_MAP_EARLY"] = "1"
    for app in ("arcade.lens", "arcade.look", "arcade.wheel"):
        s.start(app)
    ui_wait(s, "Send to my devices")
    lines = ui_lines(s)
    row = "\n".join(line["text"] for line in lines)
    flat = lambda text: "".join(text.lower().split())  # OCR drops spaces between short words
    assert all(flat(label) in flat(row) for label in ("Preview", "Send to my devices", "Add to Wheel", "Pin")), row
    send = next(line for line in lines if flat("Send to my devices") in flat(line["text"]))
    # "Continue in another tool" may be scrolled below the visible panel,
    # which also means it's under the peer row.
    continuation = next((line for line in lines if flat("Continue in another") in flat(line["text"])), None)
    assert continuation is None or send["y"] < continuation["y"], lines
    s.screenshot("box-result-peers", win)
    ui_click(s, "Send to my devices")
    ui_wait(s, "Cancel")
    s.screenshot("box-send-preview", win)
    ui_click(s, "Send to my devices", last=True)
    ui_wait(s, "Sent to your devices")
    ui_click(s, "Preview")
    s.wait_window("Arcade Look")
    s.xdotool("key", "Escape")
    # The Island hides when another app takes focus; bring it back.
    s.cli("activate", "box", check=True)
    ui_click(s, "Pin")
    pin = s.wait_window("Arcade Lens Pin")
    s.xdotool("windowfocus", pin, "key", "Escape")
    s.cli("activate", "box", check=True)
    ui_click(s, "Add to Wheel")
    s.wait_window("Arcade Wheel")
    clipboard_stop(s)
    history = clipboard_driver(s, clipboard_initialize(s), {"op": "history"}, {"op": "shutdown"})[1]
    clips = history if isinstance(history, list) else history["items"]
    assert clips, history
    s.start("arcade.clipboard")
    return "row absent without peers; peers arriving live add first row; real Preview/Send/Pin/Wheel invoked"


@check("box")
def connected_apps_real_peers_and_per_peer_toggle(s):
    for app in ("arcade.lens", "arcade.look", "arcade.wheel", "arcade.clipboard"):
        if app not in s.procs or s.procs[app].poll() is not None:
            s.start(app)
    exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
    subprocess.run([str(exe), "--settings"], env=s.env, capture_output=True, text=True, check=True, timeout=20)
    win = s.wait_window("Arcade Box")
    s.xdotool("windowfocus", "--sync", win)
    ui_wait(s, "Use with Arcade Box")
    lines = "\n".join(line["text"] for line in ui_lines(s))
    assert "Use with Box" not in lines, lines
    s.screenshot("box-connected-apps-peers", win)
    ui_click(s, "Use with Arcade Box")
    db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
    with sqlite3.connect(db) as conn:
        disabled = json.loads(conn.execute("SELECT value FROM settings WHERE key='link_disabled_peers'").fetchone()[0])
    assert "arcade.lens" in disabled, disabled
    ui_click(s, "Use with Arcade Box")
    s.xdotool("key", "Escape")
    return "all peer states rendered; per-peer label is full app name; actual Lens toggle persisted"


@check("box")
@check("box-overlap")
def real_lens_ocr_provider_records_provenance(s):
    if "arcade.lens" in s.procs:
        s.kill("arcade.lens", signal.SIGTERM)
    resident(s)
    if "arcade.lens" not in s.procs:
        s.start("arcade.lens")
    source = s.root / "box-lens-ocr.png"
    subprocess.run(["magick", "-size", "800x180", "xc:white", "-font", "DejaVu-Sans",
                    "-fill", "black", "-pointsize", "50", "-gravity", "center", "-annotate", "0",
                    "ARCADE BOX LOCAL OCR", str(source)], env=s.env, check=True)
    before = hashlib.sha256(source.read_bytes()).hexdigest()
    code, result = s.invoke("box", "box:arcade.image.ocr", "--file", str(source), "--option", "provider=lens", timeout=120)
    assert code == 0 and "ARCADE BOX" in result["outputs"][0]["text"], result
    assert result["data"]["providerId"] == "ocr.lens", result
    assert result["data"]["providerSource"] == "arcade-app", result
    assert result["data"]["providerVersion"], result
    assert hashlib.sha256(source.read_bytes()).hexdigest() == before
    return "real lens.recognize ocrOnly → text + Lens provenance; source unchanged"


def lens_select(s, rect=(100, 100, 340, 260)):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        for win in s.xdotool("search", "--onlyvisible", "--name", "^Arcade Lens$").splitlines():
            geometry = s.xdotool("getwindowgeometry", "--shell", win)
            if "WIDTH=1920" in geometry and "HEIGHT=1080" in geometry:
                s.xdotool("windowraise", win, "windowfocus", "--sync", win)
                time.sleep(.4)
                x1, y1, x2, y2 = map(str, rect)
                s.xdotool("mousemove", x1, y1, "mousedown", "1")
                time.sleep(.15)
                s.xdotool("mousemove", x2, y2)
                time.sleep(.15)
                s.xdotool("mouseup", "1")
                return win
        time.sleep(.1)
    raise AssertionError("Lens selection overlay did not open")


@check("box")
@check("box-overlap")
def screen_ocr_uses_real_lens_selection(s):
    resident(s)
    if "arcade.lens" not in s.procs:
        s.start("arcade.lens")
    # The Box capture must disappear before Lens freezes the desktop.
    win = open_tool(s, "arcade.screen.ocr")
    ui_wait(s, "Arcade Lens")
    s.screenshot("box-screen-ocr-lens", win)
    fixture = subprocess.Popen(["display", "-title", "Box OCR fixture", "-geometry", "+80+80", str(s.root / "box-lens-ocr.png")], env=s.env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    s.procs["box-ocr-fixture"] = fixture
    s.wait_window("Box OCR fixture")
    time.sleep(.5)
    s.xdotool("windowraise", win, "windowfocus", "--sync", win, "key", "ctrl+Return")
    lens_select(s, (85, 85, 875, 255))
    ui_wait(s, "ARCADE BOX", timeout=60)
    s.screenshot("box-screen-ocr-result", win)
    s.kill("box-ocr-fixture", signal.SIGTERM)
    return "Box OCR screen opens real Lens region selector; selected image OCR result returned to Box"


def store_pipeline(s, pipeline):
    resident(s)
    s.kill("arcade.box", signal.SIGTERM)
    db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
    with sqlite3.connect(db) as conn:
        conn.execute("INSERT OR REPLACE INTO pipelines(id,version,definition_json) VALUES(?,?,?)", (pipeline["id"],pipeline["version"],json.dumps(pipeline)))
    resident(s)


def optimized_screenshot_pipeline():
    return {"id":"optimized-screenshot","name":"Send optimized screenshot","version":1,"nodes":[
        {"id":"capture","link":{"app":"arcade.lens","action":"lens.capture","version":1},"inputs":[],"options":{}},
        {"id":"resize","toolId":"arcade.image.resize","inputs":[{"kind":"node","nodeId":"capture","outputIndex":0}],"options":{"mode":"percentage","percentage":50}},
        {"id":"convert","toolId":"arcade.image.convert","inputs":[{"kind":"node","nodeId":"resize","outputIndex":0}],"options":{"format":"webp"}},
        {"id":"send","link":{"app":"arcade.clipboard","action":"clipboard.add","version":1},"inputs":[{"kind":"node","nodeId":"convert","outputIndex":0}],"options":{}}
    ],"outputNodes":["send"]}


@check("box")
@check("box-pipeline")
def cross_app_pipeline_real_lens_and_clipboard(s):
    resident(s)
    clipboard_mesh(s)
    for app in ("arcade.lens", "arcade.look", "arcade.wheel"):
        if app not in s.procs or s.procs[app].poll() is not None:
            s.start(app)
    definition = optimized_screenshot_pipeline()
    store_pipeline(s, definition)
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        code, listing = s.invoke("box", "box.pipelines")
        offers = listing["outputs"][0]["data"] if code == 0 else []
        offer = next((p for p in offers if p["id"] == definition["id"]), None)
        if offer:
            break
        time.sleep(.1)
    assert offer and offer["interactive"] and offer["accepts"] == [] and offer["produces"] == [], listing
    assert "sends-to-device" in offer["effects"] and "opens-ui" in offer["effects"], offer
    wire = Wire(s)
    try:
        response = wire.invoke("box.pipeline.run", options={"pipeline":definition["id"]})
        assert "result" in response, response
        try:
            confirm = s.wait_window("Confirm pipeline effects", timeout=15)
        except AssertionError:
            s.screenshot("box-pipeline-debug")
            names = [(w, s.xdotool("getwindowname", w)) for w in s.xdotool("search", "--onlyvisible", "--name", ".").splitlines()]
            raise AssertionError(f"no effects dialog; windows={names}; job={wire.done(response['result']['job'])}")
        s.screenshot("box-pipeline-effects", confirm)
        s.xdotool("windowfocus", "--sync", confirm, "key", "Return")
        lens_select(s)
        done = wire.done(response["result"]["job"])
        assert done["status"] == "success", done
        # Approval is remembered; a second run reaches Lens without a prompt.
        second = wire.invoke("box.pipeline.run", options={"pipeline":definition["id"]})
        lens_select(s)
        again = wire.done(second["result"]["job"])
        assert again["status"] == "success", again
        third = wire.invoke("box.pipeline.run", options={"pipeline":definition["id"]})
        s.wait_window("^Arcade Lens$", timeout=15)
        cancelled = wire.call("job.cancel", {"job":third["result"]["job"]})
        assert cancelled["result"]["cancelled"], cancelled
        stopped = wire.done(third["result"]["job"])
        assert stopped["status"] == "cancelled" and not stopped.get("outputs"), stopped
        # Lens currently leaves a cancelled capture's selector open; dismiss it.
        for win in s.xdotool("search", "--onlyvisible", "--name", "^Arcade Lens$").splitlines():
            s.xdotool("windowfocus", "--sync", win, "key", "Escape")
        time.sleep(.3)
        fourth = wire.invoke("box.pipeline.run", options={"pipeline":definition["id"]})
        s.wait_window("^Arcade Lens$", timeout=15)
        s.kill("arcade.lens", signal.SIGKILL)
        crashed = wire.done(fourth["result"]["job"])
        assert crashed["status"] == "error" and not crashed.get("outputs"), crashed
        s.start("arcade.lens")
    finally:
        wire.close()
    db = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3"
    with sqlite3.connect(db) as conn:
        assert conn.execute("SELECT value FROM settings WHERE key=?", ("pipeline_effect_approval."+definition["id"],)).fetchone()
    clipboard_stop(s)
    history = clipboard_driver(s, clipboard_initialize(s), {"op":"history"}, {"op":"shutdown"})[1]
    clips = history if isinstance(history, list) else history["items"]
    assert clips and any(
        representation.get("name", "").endswith(".webp") and representation.get("size", 0) > 0
        for clip in clips for representation in clip.get("representations", [])
    ), history
    root = Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/job-artifacts"
    assert not list(root.glob("pipeline-job-*")), "intermediate job dir survived"
    s.start("arcade.clipboard")
    return "real Lens capture → resize 50% → WebP → Clipboard history; effects confirmed once; cancel/crash clean up intermediates"


@check("box")
@check("box-pipeline")
def peer_pipeline_editor_and_version_repair(s):
    resident(s)
    for app in ("arcade.lens", "arcade.clipboard"):
        if app not in s.procs or s.procs[app].poll() is not None:
            s.start(app)
    pipeline = optimized_screenshot_pipeline()
    pipeline["nodes"][0]["link"]["version"] = 99
    store_pipeline(s, pipeline)
    code, listing = s.invoke("box", "box.pipelines")
    assert code == 0 and not any(p["id"] == pipeline["id"] for p in listing["outputs"][0]["data"]), listing
    wire = Wire(s)
    try:
        denied = wire.invoke("box.pipeline.run", options={"pipeline":pipeline["id"]})
        assert "error" in denied and "Needs repair" in str(denied), denied
    finally:
        wire.close()
    exe = APPS["arcade.box"]["dir"] / APPS["arcade.box"]["bin"]
    subprocess.run([str(exe), "--settings"], env=s.env, capture_output=True, text=True, check=True, timeout=20)
    win = s.wait_window("Arcade Box")
    s.xdotool("windowraise", win, "windowfocus", "--sync", win)
    ui_wait(s, "Pipelines")
    # Wait for the Settings first frame to settle before changing dashboard mode.
    time.sleep(.5)
    ui_click(s, "Pipelines")
    time.sleep(.3)
    ui_click(s, "Pipelines")
    ui_wait(s, "Needs repair")
    s.screenshot("box-pipeline-needs-repair", win)
    ui_click(s, "Edit", last=True)
    ui_wait(s, "Starting input")
    s.screenshot("box-pipeline-editor-peer", win)
    geometry = dict(line.split("=", 1) for line in s.xdotool("getwindowgeometry", "--shell", win).splitlines() if "=" in line)
    s.xdotool("mousemove", str(int(geometry["X"]) + int(geometry["WIDTH"]) - 80), str(int(geometry["Y"]) + int(geometry["HEIGHT"]) - 80), "click", "5", "click", "5", "click", "5")
    ui_wait(s, "Use current action version")
    s.screenshot("box-pipeline-editor-version", win)
    ui_click(s, "Use current action version")
    s.xdotool("click", "5", "click", "5", "click", "5", "click", "5", "click", "5", "click", "5", "click", "5", "click", "5")
    ui_click(s, "Save pipeline")
    with sqlite3.connect(Path(s.env["XDG_DATA_HOME"]) / "dev.arcadebox.app/arcade.sqlite3") as conn:
        saved = json.loads(conn.execute("SELECT definition_json FROM pipelines WHERE id=?", (pipeline["id"],)).fetchone()[0])
    assert saved["nodes"][0]["link"]["version"] == 1, saved
    return "changed action version hidden from consumers; Needs repair editor updates the pinned version and saves all four stages"
