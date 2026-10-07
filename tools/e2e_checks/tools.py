"""Real Tools GUI + local GitHub Releases, driven only inside the isolated runner.

The downloaded AppImage fixture is a verified shell wrapper for the real Lens
binary. Lens serves its actual Link actions, busy jobs, quit and manifest writer.
Debug-only observations describe the rendered DOM; clicks still use native X11.
No test command can approve an installation or bypass the confirmation dialog.
"""
import contextlib
import hashlib
import http.server
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import threading
import time

TOOLS = Path(__file__).resolve().parents[3] / "Arcade-tools"
GENERATOR = TOOLS / "scripts/arcade-release.py"


def wait_for(predicate, detail, timeout=15):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        result = predicate()
        if result:
            return result
        time.sleep(.05)
    raise AssertionError(detail() if callable(detail) else detail)


class Releases:
    def __init__(self, s):
        self.root = s.root / "tools-releases"
        self.root.mkdir(exist_ok=True)
        self.routes = {}
        self.gates = {}
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                body = owner.routes.get(self.path)
                self.send_response(200 if body is not None else 404)
                body = body or b""
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                gate = owner.gates.get(self.path)
                if gate and not gate.wait(12):
                    return
                with contextlib.suppress(BrokenPipeError, ConnectionResetError):
                    self.wfile.write(body)

            def log_message(self, *_):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_port}/"
        lens = APPS["arcade.lens"]["dir"] / APPS["arcade.lens"]["bin"]
        assert lens.is_file(), f"Build the real Lens app first: {lens}"
        self.wrapper = (f'#!/bin/sh\nexport APPIMAGE="$0"\nexport APPDIR={shlex.quote(str(lens.parent))}\n'
                        f'exec {shlex.quote(str(lens))} "$@"\n').encode()

    def publish(self, version, channel="stable", corrupt=False, gated=False):
        directory = self.root / f"{version}-{channel}"
        directory.mkdir(exist_ok=True)
        installer = directory / "Lens_x64.AppImage"
        body = self.wrapper + f"# release {version}\n".encode() + b"# padding\n" * 14000
        installer.write_bytes(body)
        subprocess.run(["python3", str(GENERATOR), "--id", "arcade.lens", "--version", version,
                        "--channel", channel, "--notes", "https://github.com/qa-p1/Arcade-lens/releases",
                        str(directory)], check=True, capture_output=True)
        prefix = f"assets/{version}-{channel}/"
        assets = []
        for path in directory.iterdir():
            route = prefix + path.name
            self.routes["/" + route] = b"corrupt" if corrupt and path == installer else path.read_bytes()
            assets.append({"name": path.name, "browser_download_url": self.url + route})
        selector = "latest" if channel == "stable" else "tags/nightly"
        self.routes[f"/repos/qa-p1/Arcade-lens/releases/{selector}"] = json.dumps({
            "tag_name": f"v{version}" if channel == "stable" else "nightly", "draft": False,
            "prerelease": channel == "nightly", "assets": assets,
        }).encode()
        route = "/" + prefix + installer.name
        if gated:
            self.gates[route] = threading.Event()
        return route, hashlib.sha256(body).hexdigest()

    def close(self):
        for gate in self.gates.values():
            gate.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=3)


class GUI:
    def __init__(self, s, releases):
        self.s = s
        self.trace = s.root / f"tools-ui-{time.time_ns()}.jsonl"
        self.previous = {k: s.env.get(k) for k in ("ARCADE_TOOLS_TEST_SOURCE", "ARCADE_TOOLS_SMOKE_LOG")}
        s.env.update(ARCADE_TOOLS_TEST_SOURCE=releases.url, ARCADE_TOOLS_SMOKE_LOG=str(self.trace))
        self.window = None

    def start(self, background=False):
        binary = TOOLS / "target/debug/arcade-tools"
        assert binary.is_file(), f"Build Tools first: {binary}"
        log = open(self.s.root / "arcade.tools.log", "ab")
        p = subprocess.Popen([str(binary), *(["--background"] if background else [])], env=self.s.env,
                             stdout=log, stderr=log, start_new_session=True)
        log.close()
        self.s.procs["arcade.tools"] = p
        self.s.wait_running("arcade.tools")
        if not background:
            self.window = self.s.wait_window("^Arcade Tools$")
            try:
                self.wait(lambda r: len(r.get("apps", [])) == 5)
            except AssertionError:
                self.shot("tools-startup-error")
                raise
            self.visible()

    def command(self, *args):
        return subprocess.run([str(TOOLS / "target/debug/arcade-tools"), *args], env=self.s.env,
                              capture_output=True, text=True, timeout=15)

    def visible(self):
        self.window = self.s.wait_window("^Arcade Tools$")
        self.s.xdotool("windowsize", self.window, "940", "1020")
        self.s.xdotool("windowfocus", "--sync", self.window)

    def records(self):
        if not self.trace.exists():
            return []
        records = []
        for line in self.trace.read_text().splitlines():
            with contextlib.suppress(json.JSONDecodeError):
                records.append(json.loads(line))
        return records

    def report(self):
        reports = [r["report"] for r in self.records() if r.get("event") == "rendered"]
        return max(reports, key=lambda r: r["revision"]) if reports else {}

    def wait(self, predicate, timeout=15):
        return wait_for(lambda: self.report() if predicate(self.report()) else None,
                        lambda: f"Tools UI: { {k: self.report().get(k) for k in ('revision', 'dialog', 'message', 'phase', 'busy')} }\n{self.s.log('arcade.tools')}", timeout)

    def app(self, id="arcade.lens"):
        return next(a for a in self.report()["apps"] if a["id"] == id)

    def click(self, label=None, app=None, id=None):
        def find(report):
            return next((c for c in report.get("controls", []) if (label is None or c["label"] == label)
                         and (app is None or c.get("app") == app) and (id is None or c["id"] == id)
                         and not c["disabled"] and c["width"] > 0), None)
        control = find(self.wait(lambda r: find(r) is not None))
        self.s.xdotool("windowraise", self.window, "windowfocus", "--sync", self.window)
        height = int(next(line.split('=')[1] for line in self.s.xdotool("getwindowgeometry", "--shell", self.window).splitlines() if line.startswith("HEIGHT=")))
        if control["y"] < 0 or control["y"] + control["height"] > height:
            before = self.report()["revision"]
            self.s.xdotool("key", "ctrl+Home" if control["y"] < 0 else "ctrl+End")
            self.wait(lambda r: r["revision"] > before)
            control = find(self.report())
        self.s.xdotool("mousemove", "--window", self.window, str(round(control["x"] + control["width"] / 2)),
                       str(round(control["y"] + control["height"] / 2)), "click", "1")

    def dialog(self, contains):
        self.wait(lambda r: contains in (r.get("dialog") or ""))

    def confirm(self):
        self.click(id="confirm-action")
        self.wait(lambda r: not r.get("dialog") or "Choose how to reopen" in r.get("dialog", ""))

    def finished(self, text):
        return self.wait(lambda r: not r.get("busy") and text in (r.get("message") or ""), timeout=30)

    def shot(self, name):
        self.s.xdotool("windowraise", self.window)
        time.sleep(.25)  # DOM acknowledgement precedes the native webview's paint.
        self.s.screenshot(name, self.window)

    def close(self):
        # Every PID comes from this run's private endpoint, never the desktop.
        for app in ("arcade.lens", "arcade.tools"):
            self.s.cli("quit", app)
            endpoint = self.s.endpoint(app)
            if endpoint.exists():
                with contextlib.suppress(ProcessLookupError, FileNotFoundError):
                    os.kill(json.loads(endpoint.read_text())["pid"], signal.SIGTERM)
        self.s.kill("arcade.tools", signal.SIGTERM)
        for key, value in self.previous.items():
            if value is None:
                self.s.env.pop(key, None)
            else:
                self.s.env[key] = value


@check("tools")
def gui_verified_lifecycle_with_real_busy_peer(s):
    releases = Releases(s)
    gui = GUI(s, releases)
    try:
        route, first_sha = releases.publish("1.0.0", gated=True)
        gui.start()
        before = gui.report()["revision"]
        gui.click(id="refresh")
        gui.wait(lambda r: r["revision"] > before and len(r.get("apps", [])) == 5)
        gui.click("Arcade Box channel", app="arcade.box")
        s.xdotool("key", "Escape")  # Also force a native paint after the Xvfb resize.
        assert all(not a["managed"] for a in gui.report()["apps"]), gui.report()
        gui.shot("tools-list")
        s.cli("ls", "--json", check=True)
        actions = json.loads(s.cli("describe", "tools", "--json", check=True).stdout)
        assert [a["id"] for a in actions] == ["tools.install"], actions
        gui.click("Check & install", app="arcade.lens")
        gui.dialog("Install Arcade Lens 1.0.0?")
        gui.click("Open in background")
        gui.confirm()
        gui.wait(lambda r: r.get("busy") and "Downloading" in (r.get("phase") or ""))
        gui.shot("tools-install-progress")
        releases.gates[route].set()
        gui.finished("1.0.0 installed")
        s.wait_running("arcade.lens")
        exe = s.root / "home/Applications/Arcade/Arcade-Lens.AppImage"
        assert hashlib.sha256(exe.read_bytes()).hexdigest() == first_sha
        manifest = json.loads((s.root / "arcade/apps/arcade.lens.json").read_text())
        assert manifest["executable"] == str(exe), manifest

        route, second_sha = releases.publish("2.0.0")
        gui.click("Check updates", app="arcade.lens")
        gui.finished("2.0.0 is available")
        gui.shot("tools-update-available")
        # A pending real capture job keeps app.quit busy; Tools must keep old bytes.
        capture = s.invoke("lens", "lens.capture", background=True)
        wait_for(lambda: json.loads(s.cli("status", "lens", "--json").stdout or "{}").get("busy"), "Lens did not become busy")
        gui.click("Update", app="arcade.lens")
        gui.dialog("Update Arcade Lens")
        gui.confirm()
        gui.finished("Arcade Lens is busy. Try again when its current job finishes.")
        assert hashlib.sha256(exe.read_bytes()).hexdigest() == first_sha
        lens_window = s.wait_window("^Arcade Lens$")
        s.xdotool("windowraise", lens_window, "windowfocus", "--sync", lens_window, "key", "Shift_L")
        time.sleep(.15)
        s.xdotool("key", "Escape")
        capture.communicate(timeout=10)
        assert capture.returncode != 0
        wait_for(lambda: not json.loads(s.cli("status", "lens", "--json").stdout).get("busy"), "Lens remained busy")
        gui.click("Update", app="arcade.lens")
        gui.dialog("Update Arcade Lens")
        gui.confirm()
        result = gui.wait(lambda r: "Choose how to reopen" in (r.get("dialog") or "") or "2.0.0 installed" in (r.get("message") or ""))
        if "Choose how to reopen" in (result.get("dialog") or ""):
            # Older peers omit mode. Keep the explicit background fallback.
            gui.click("Reopen mode")
            s.xdotool("key", "Home", "Down", "Return")
            gui.confirm()
        gui.finished("2.0.0 installed")
        s.wait_running("arcade.lens")
        assert hashlib.sha256(exe.read_bytes()).hexdigest() == second_sha

        # Missing files are repairable, and channel selection downloads that channel.
        s.cli("quit", "lens", check=True)
        wait_for(lambda: s.cli("status", "lens").returncode != 0, "Lens did not quit")
        exe.unlink()
        gui.click(id="refresh")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and not a["healthy"] for a in r["apps"]))
        gui.click("Repair", app="arcade.lens")
        gui.dialog("Repair Arcade Lens")
        gui.confirm()
        gui.finished("2.0.0 installed")
        assert exe.is_file()
        releases.publish("2.1.0-nightly", channel="nightly")
        gui.click("Arcade Lens channel", app="arcade.lens")
        s.xdotool("key", "End", "Return")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and a["channel"] == "nightly" for a in r["apps"]))
        gui.click("Check updates", app="arcade.lens")
        gui.finished("2.1.0-nightly is available")
        gui.click("Update", app="arcade.lens")
        gui.dialog("Update Arcade Lens")
        gui.confirm()
        gui.finished("2.1.0-nightly installed")
        record = json.loads((s.root / "data/arcade-tools/arcade.lens.json").read_text())
        assert record["channel"] == "nightly" and record["version"] == "2.1.0-nightly", record

        # Start-at-login writes stay entirely inside the throwaway HOME/XDG roots.
        gui.click("Arcade Lens start at login", app="arcade.lens")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and a["startAtLogin"] for a in r["apps"]))
        login = s.root / "config/autostart/arcade-lens.desktop"
        assert str(exe) in login.read_text()
        data = s.root / "config/arcadelens"
        data.mkdir(exist_ok=True)
        (data / "keep.json").write_text("keep")
        gui.click("Remove", app="arcade.lens")
        gui.dialog("Remove Arcade Lens?")
        gui.shot("tools-uninstall-dialog")
        gui.confirm()
        gui.finished("Settings and data kept")
        assert not exe.exists() and (data / "keep.json").exists() and not login.exists()
        assert not (s.root / "arcade/apps/arcade.lens.json").exists()
        return "real GUI install → busy refusal → verified update/quit → repair → nightly → login → uninstall/data retained"
    finally:
        gui.close()
        releases.close()


@check("tools")
def handoff_confirmation_cancel_checksum_and_disabled_presence(s):
    releases = Releases(s)
    gui = GUI(s, releases)
    try:
        preferences = s.root / "data/arcade-tools/preferences.json"
        preferences.parent.mkdir(parents=True, exist_ok=True)
        preferences.write_text(json.dumps({"linkEnabled": True, "channels": {"arcade.lens": "nightly"}}))
        releases.publish("3.0.0", channel="nightly")
        gui.start(background=True)
        status = json.loads(s.cli("status", "tools", "--json", check=True).stdout)
        assert status["status"]["mode"] == "background", status
        assert not s.xdotool("search", "--onlyvisible", "--name", "^Arcade Tools$")
        code, result = s.invoke("tools", "tools.install", "--option", "app=arcade.lens")
        assert code == 0 and result["data"]["confirmationRequired"], result
        gui.visible()
        gui.dialog("Install Arcade Lens 3.0.0?")
        gui.shot("tools-install-confirmation")
        exe = s.root / "home/Applications/Arcade/Arcade-Lens.AppImage"
        assert not exe.exists(), "handoff installed without confirmation"
        gui.click("Cancel")
        assert not exe.exists()
        code, result = s.invoke("tools", "tools.install", "--text", "x" * 65)
        assert code != 0 and "too_large" in result, result
        code, result = s.invoke("tools", "tools.not-an-action", "--text", "arcade.lens")
        assert code != 0 and "unavailable" in result, result

        route, _ = releases.publish("3.0.0", channel="nightly", gated=True)
        gui.click("Check & install", app="arcade.lens")
        gui.dialog("Install Arcade Lens")
        gui.confirm()
        gui.wait(lambda r: r.get("busy") and "Downloading" in (r.get("phase") or ""))
        quit = gui.command("--quit")
        assert quit.returncode != 0 and "busy" in quit.stderr, quit
        assert s.procs["arcade.tools"].poll() is None
        gui.click(id="cancel")
        gui.wait(lambda r: r.get("message") == "Cancelling…")  # Main thread remains responsive during the blocked download.
        releases.gates[route].set()
        gui.finished("Cancelled.")
        assert not exe.exists()
        releases.publish("3.0.0", channel="nightly", corrupt=True)
        gui.click("Check & install", app="arcade.lens")
        gui.dialog("Install Arcade Lens")
        gui.confirm()
        gui.finished("failed SHA-256")
        gui.shot("tools-error")
        assert not exe.exists()

        # The master switch removes the endpoint and actions; management still works.
        gui.click(id="link")
        gui.finished("Arcade Link is disabled")
        wait_for(lambda: not s.endpoint("arcade.tools").exists(), "Tools endpoint still exists")
        manifest = json.loads((s.root / "arcade/apps/arcade.tools.json").read_text())
        assert not manifest["settings"]["linkEnabled"] and manifest["actions"] == []
        gui.click(id="link")
        gui.finished("Arcade Link is enabled")
        s.wait_running("arcade.tools")
        return "background startup; real confirmation/cancel; unavailable + oversized; UI responsive during download; checksum rejection; master off/on"
    finally:
        gui.close()
        releases.close()


@check("tools")
def live_discovery_crash_idle_and_native_cli(s):
    releases = Releases(s)
    gui = GUI(s, releases)
    capture = None
    try:
        def snapshot():
            return {str(p): p.read_bytes() for base in (s.root / "arcade/apps", s.root / "data/arcade-tools")
                    for p in base.glob("*.json")}
        before = snapshot()
        version = gui.command("--version")
        assert version.returncode == 0 and version.stdout.strip() == "Arcade Tools 0.1.0", version
        manifest = gui.command("--arcade-manifest")
        assert manifest.returncode == 0 and json.loads(manifest.stdout)["id"] == "arcade.tools", manifest
        assert snapshot() == before, "informational flags changed registry or preferences"

        # Discovery must work when the real peer starts before Tools and later.
        lens = s.start("arcade.lens")
        gui.start()
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and a["running"] for a in r["apps"]))
        assert gui.app()["installed"] and not gui.app()["managed"], gui.app()
        status = json.loads(s.cli("status", "tools", "--json", check=True).stdout)
        assert status["status"]["mode"] == "foreground", status
        s.cli("quit", "lens", check=True)
        lens.wait(timeout=15)
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and not a["running"] for a in r["apps"]))
        s.start("arcade.lens")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and a["running"] for a in r["apps"]))

        capture = s.invoke("lens", "lens.capture", background=True)
        wait_for(lambda: json.loads(s.cli("status", "lens", "--json").stdout or "{}").get("busy"), "Lens did not start a job")
        s.kill("arcade.lens")
        _, err = capture.communicate(timeout=10)
        assert capture.returncode != 0 and "not_running" in err, err
        gui.click(id="refresh")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and not a["running"] for a in r["apps"]))
        gui.click("Open", app="arcade.lens")
        s.wait_running("arcade.lens")
        gui.wait(lambda r: any(a["id"] == "arcade.lens" and a["running"] for a in r["apps"]))

        # Read-only probes must not cause a watch → refresh → probe loop.
        time.sleep(.3)
        revision = gui.report()["revision"]
        for _ in range(12):
            s.cli("status", "tools", "--json", check=True)
            s.cli("ls", "--json", check=True)
        time.sleep(1)
        assert gui.report()["revision"] == revision, "read-only probes caused idle UI refreshes"

        gui.click(id="link")
        gui.finished("Arcade Link is disabled")
        assert not s.endpoint("arcade.tools").exists()
        quit = gui.command("--quit")
        assert quit.returncode == 0, quit.stderr
        s.procs["arcade.tools"].wait(timeout=15)
        before = snapshot()
        quit = gui.command("--quit")
        assert quit.returncode == 0 and snapshot() == before, "quit without an instance wrote state"
        return "peer first/later; real peer crash mid-job + Open recovery; no idle refresh on probes; mode/CLI flags; native quit with Link disabled"
    finally:
        if capture and capture.poll() is None:
            capture.kill()
            capture.wait()
        gui.close()
        releases.close()
