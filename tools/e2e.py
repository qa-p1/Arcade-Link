#!/usr/bin/env python3
"""Ecosystem end-to-end runner (Linux, headless).

Starts the five Arcade apps from their sibling checkouts in background mode
under one temporary ARCADE_HOME, drives the flagship flows through the
`arcade-link` debug CLI, then kills each app in turn to check that every
consumer degrades cleanly.

Everything runs isolated from the desktop: a private D-Bus session (with an
unlocked throwaway keyring for Clipboard), Xvfb, temporary HOME/XDG_*/
ARCADE_* directories, and no Wayland or Hyprland variables.

    python3 tools/e2e.py                 # every check
    python3 tools/e2e.py --only presence # one group
    python3 tools/e2e.py run -- arcade-link ls   # a command in the isolated session
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import struct
import tempfile
import time
import zlib
from pathlib import Path

LINK = Path(__file__).resolve().parents[1]
CODING = LINK.parents[1]
CLI = LINK / "target/debug/arcade-link"

APPS = {
    "arcade.box": {
        "dir": CODING / "Rust/Arcade Box",
        "bin": "target/release/arcade-desktop",
        "args": ["--background"],
        "build": ["cargo", "build", "--release", "-p", "arcade-desktop", "-p", "arcadebox"],
    },
    "arcade.lens": {
        "dir": CODING / "Rust/Arcade-lens",
        "bin": "target/release/arcade-lens",
        "args": ["--background"],
        "build": ["cargo", "build", "--release", "-p", "arcade-lens"],
    },
    "arcade.look": {
        "dir": CODING / "Rust/arcade-look",
        "bin": "src-tauri/target/link-tests/debug/arcade-look",  # scripts/verify-link.py --build-e2e
        "args": ["--background"],
        "build": None,  # npm run build && cargo build --release (in src-tauri)
    },
    "arcade.wheel": {
        "dir": CODING / "C++/Arcade wheel",
        "bin": "build/arcade-wheel",
        "args": ["--background"],
        "build": ["cmake", "--build", "build"],
    },
    "arcade.clipboard": {
        "dir": CODING / "App dev/Arcade-clipboard",
        "bin": "apps/flutter_app/build/linux/x64/release/bundle/clipboard",
        "args": ["--background"],
        "build": ["bash", "scripts/build-linux.sh"],
    },
}

STRIP = ("WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP",
         "DESKTOP_SESSION", "SWAYSOCK", "XDG_SESSION_TYPE", "APPIMAGE", "APPDIR", "OWD", "ARGV0",
         "DBUS_SESSION_BUS_ADDRESS", "DISPLAY")


def isolated_env(root: Path) -> dict:
    env = {k: v for k, v in os.environ.items() if k not in STRIP}
    for d in ("home", "config", "data", "cache", "runtime", "lens", "clipdata", "arcade", "wheel"):
        (root / d).mkdir(parents=True, exist_ok=True)
    os.chmod(root / "runtime", 0o700)
    env.update({
        "HOME": str(root / "home"),
        "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_DATA_HOME": str(root / "data"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_RUNTIME_DIR": str(root / "runtime"),
        "XDG_SESSION_TYPE": "x11",
        "ARCADE_HOME": str(root / "arcade"),
        "ARCADE_LENS_HOME": str(root / "lens"),
        "ARCADE_DATA_DIR": str(root / "clipdata"),
        "ARCADE_WHEEL_INSTANCE": str(root / "wheel"),
        "ARCADE_WHEEL_DISABLE_GLOBAL_SHORTCUT": "1",
        "QT_QPA_PLATFORM": "xcb",
        "GDK_BACKEND": "x11",
        "NO_AT_BRIDGE": "1",
        "PATH": f"{CLI.parent}:{os.environ.get('PATH', '')}",
    })
    return env


class Session:
    """Xvfb + keyring inside the private D-Bus session this process runs in."""

    def __init__(self, root: Path):
        self.root = root
        self.env = isolated_env(root)
        self.procs: dict[str, subprocess.Popen] = {}
        # Xvfb picks a free display itself, so several runs can go in parallel.
        rfd, wfd = os.pipe()
        self.xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(wfd), "-screen", "0", "1920x1080x24", "-nolisten", "tcp"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, pass_fds=(wfd,))
        os.close(wfd)
        with os.fdopen(rfd) as r:
            display = ":" + r.readline().strip()
        self.env["DISPLAY"] = display
        self.env["DBUS_SESSION_BUS_ADDRESS"] = os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
        # Let D-Bus-activated services (portals) reach the virtual display.
        subprocess.run(["dbus-update-activation-environment", f"DISPLAY={display}"], env=self.env, capture_output=True)
        for _ in range(100):
            if subprocess.run(["xdotool", "getdisplaygeometry"], env=self.env, capture_output=True).returncode == 0:
                break
            time.sleep(0.05)
        self.keyring = None
        if shutil.which("gnome-keyring-daemon"):
            kenv = dict(self.env, HOME=str(root / "keyring"), XDG_DATA_HOME=str(root / "keyring"))
            (root / "keyring").mkdir(exist_ok=True)
            self.keyring = subprocess.Popen(["gnome-keyring-daemon", "--foreground", "--unlock", "--components=secrets"],
                                            env=kenv, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.keyring.stdin.write(b"e2e\n")
            self.keyring.stdin.close()
            time.sleep(0.3)

    def cli(self, *args: str, timeout: float = 60, check: bool = False) -> subprocess.CompletedProcess:
        r = subprocess.run([str(CLI), *args], env=self.env, capture_output=True, text=True, timeout=timeout)
        if check and r.returncode != 0:
            raise AssertionError(f"arcade-link {' '.join(args)} failed:\n{r.stdout}{r.stderr}")
        return r

    def start(self, app: str, wait: bool = True) -> subprocess.Popen:
        spec = APPS[app]
        exe = spec["dir"] / spec["bin"]
        log = open(self.root / f"{app}.log", "ab")
        p = subprocess.Popen([str(exe), *spec["args"]], env=self.env, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                             start_new_session=True)
        self.procs[app] = p
        if wait:
            self.wait_running(app)
        return p

    def endpoint(self, app: str) -> Path:
        return self.root / "arcade/run" / f"{app}.endpoint"

    def wait_running(self, app: str, timeout: float = 20) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.endpoint(app).exists() and "running" in self.state(app):
                return
            p = self.procs.get(app)
            if p and p.poll() is not None:
                raise AssertionError(f"{app} exited ({p.returncode}); log:\n{self.log(app)}")
            time.sleep(0.05)
        raise AssertionError(f"{app} did not start serving the Link; log:\n{self.log(app)}")

    def invoke(self, app: str, action: str, *args: str, timeout: float = 60, background: bool = False):
        """`arcade-link invoke … --json`: (exit code, outputs/result or error text).

        With `background`, returns the Popen so the caller can drive the UI first.
        """
        cmd = [str(CLI), "invoke", app, action, *args, "--json"]
        if background:
            return subprocess.Popen(cmd, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        r = subprocess.run(cmd, env=self.env, capture_output=True, text=True, timeout=timeout)
        return parse_invoke(r.returncode, r.stdout, r.stderr)

    def xdotool(self, *args: str) -> str:
        return subprocess.run(["xdotool", *args], env=self.env, capture_output=True, text=True).stdout.strip()

    def wait_window(self, name: str, timeout: float = 10) -> str:
        deadline = time.time() + timeout
        while time.time() < deadline:
            found = self.xdotool("search", "--onlyvisible", "--name", name)
            if found:
                return found.splitlines()[0]
            time.sleep(0.1)
        raise AssertionError(f"no visible window named {name!r}")

    def screenshot(self, name: str, window: str = "root") -> Path:
        """Saves the virtual screen (or one X window id) as `<name>.png` in
        `$ARCADE_E2E_SHOTS` (default: the run's temporary root) and returns the path."""
        out = Path(os.environ.get("ARCADE_E2E_SHOTS") or self.root) / f"{name}.png"
        out.parent.mkdir(parents=True, exist_ok=True)
        r = subprocess.run(["import", "-window", window, str(out)], env=self.env, capture_output=True, text=True)
        if r.returncode != 0:
            raise AssertionError(f"screenshot {name} failed: {r.stderr}")
        return out

    def state(self, app: str) -> str:
        r = self.cli("ls", "--json")
        for row in json.loads(r.stdout or "[]"):
            if row["id"] == app:
                return row["state"]
        return "not installed"

    def log(self, app: str) -> str:
        p = self.root / f"{app}.log"
        return p.read_text(errors="replace")[-3000:] if p.exists() else ""

    def kill(self, app: str, sig=signal.SIGKILL) -> None:
        p = self.procs.pop(app, None)
        if p and p.poll() is None:
            try:
                os.killpg(p.pid, sig)
            except ProcessLookupError:
                pass
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                p.wait()

    def close(self) -> None:
        for app in list(self.procs):
            self.kill(app, signal.SIGTERM)
        if self.keyring:
            self.keyring.terminate()
        self.xvfb.terminate()


def parse_invoke(code: int, stdout: str, stderr: str):
    if code == 0:
        try:
            return 0, json.loads(stdout[: stdout.rfind("}") + 1] or "{}")
        except json.JSONDecodeError:
            return 0, stdout
    return code, (stderr or stdout).strip()


def finish(p: subprocess.Popen, timeout: float = 20):
    out, err = p.communicate(timeout=timeout)
    return parse_invoke(p.returncode, out, err)


def write_png(path: Path, width: int = 8, height: int = 6) -> Path:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + bytes([200, 40, 40]) * width for _ in range(height))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return path


# ---- checks -----------------------------------------------------------------

CHECKS: dict[str, callable] = {}


def check(group: str):
    def deco(f):
        CHECKS.setdefault(group, []).append(f)
        return f
    return deco


@check("presence")
def every_app_registers_and_serves(s: Session) -> str:
    for app in APPS:
        s.start(app)
    r = s.cli("ls", check=True)
    missing = [a for a in APPS if a not in r.stdout]
    assert not missing, f"missing from arcade-link ls: {missing}\n{r.stdout}"
    assert r.stdout.count("running") == len(APPS), r.stdout
    return r.stdout.strip()


def ensure_running(s: Session, *apps: str) -> None:
    for app in apps:
        if app not in s.procs or s.procs[app].poll() is not None:
            s.start(app)


def outputs(result) -> list:
    assert isinstance(result, dict), result
    return result.get("outputs", [])


@check("actions")
def box_runs_a_tool_preset_headless(s: Session) -> str:
    ensure_running(s, "arcade.box")
    png = write_png(s.root / "in.png", 64, 48)
    code, r = s.invoke("box", "box:arcade.image.convert#webp", "--file", str(png), timeout=120)
    assert code == 0, r
    out = outputs(r)[0]
    assert out["type"] == "file/image" and out["path"].endswith(".webp"), out
    assert Path(out["path"]).read_bytes()[8:12] == b"WEBP", out
    return out["path"]


@check("actions")
def box_one_shot_without_a_running_box(s: Session) -> str:
    spec = APPS["arcade.box"]
    png = write_png(s.root / "oneshot.png", 64, 48)
    req = {"action": "box:arcade.image.convert", "preset": "png", "inputs": [{"type": "file/image", "path": str(png)}],
           "options": {}, "context": {"source": "e2e", "interactive": False, "reason": "test"}}
    r = subprocess.run([str(spec["dir"] / "target/release/arcadebox"), "--arcade-invoke"], input=json.dumps(req),
                       env=s.env, capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stdout + r.stderr
    return r.stdout.strip()[:200]


@check("actions")
def look_inspects_and_previews(s: Session) -> str:
    ensure_running(s, "arcade.look")
    png = write_png(s.root / "look.png", 12, 7)
    code, r = s.invoke("look", "look.inspect", "--file", str(png))
    assert code == 0, r
    info = outputs(r)[0]
    assert info["type"] == "structured/file-info", info
    assert (info["data"]["width"], info["data"]["height"]) == (12, 7), info
    code, r = s.invoke("look", "look.preview", "--file", str(png))
    assert code == 0 and "Previewing" in r.get("message", ""), r
    code, r = s.invoke("look", "look.preview", "--url", "https://example.com")
    assert code != 0 and "unsupported_input" in r, r
    return json.dumps(info["data"])


@check("actions")
def lens_recognizes_pins_and_analyzes(s: Session) -> str:
    ensure_running(s, "arcade.lens")
    png = write_png(s.root / "lens.png", 40, 30)
    code, r = s.invoke("lens", "lens.recognize", "--file", str(png), timeout=120)
    assert code == 0, r
    found = outputs(r)
    code, r = s.invoke("lens", "lens.pin", "--file", str(png))
    assert code == 0, r
    p = s.invoke("lens", "lens.analyze", "--file", str(png), background=True)
    time.sleep(2)
    s.xdotool("key", "Escape")
    code, r = finish(p)
    assert code in (0, 1), r
    return f"recognize -> {[o['type'] for o in found]}; analyze -> {r if code else 'done'}"


@check("actions")
def lens_capture_returns_the_selected_region(s: Session) -> str:
    ensure_running(s, "arcade.lens")
    p = s.invoke("lens", "lens.capture", background=True)
    time.sleep(1.5)
    s.xdotool("mousemove", "100", "100", "mousedown", "1", "mousemove", "300", "250", "mouseup", "1")
    code, r = finish(p)
    assert code == 0, r
    kinds = [o["type"] for o in outputs(r)]
    assert "file/image" in kinds and "screen/region" in kinds, r
    region = next(o for o in outputs(r) if o["type"] == "screen/region")
    return json.dumps(region.get("data"))


@check("actions")
def wheel_shows_and_offers_add_action(s: Session) -> str:
    ensure_running(s, "arcade.wheel")
    code, r = s.invoke("wheel", "wheel.show")
    assert code == 0, r
    s.xdotool("key", "Escape")
    p = s.invoke("wheel", "wheel.add_action", "--url", "https://example.com", background=True)
    win = s.wait_window("Arcade Wheel")
    time.sleep(0.5)
    s.xdotool("windowclose", win)
    code, r = finish(p)
    assert code != 0 and "user_cancelled" in r, r
    return r


@check("actions")
def clipboard_actions_without_devices(s: Session) -> str:
    ensure_running(s, "arcade.clipboard")
    code, r = s.invoke("clipboard", "clipboard.devices")
    assert code == 0 and outputs(r)[0]["data"] == [], r
    code, r = s.invoke("clipboard", "clipboard.add", "--text", "hello")
    assert code != 0 and "unavailable" in r, r
    return r


def load_check_modules() -> None:
    """Per-app check groups live in tools/e2e_checks/<name>.py; each module
    uses `check(group)` and the `Session` helpers from this file."""
    import importlib.util
    for path in sorted((Path(__file__).parent / "e2e_checks").glob("*.py")):
        spec = importlib.util.spec_from_file_location(f"e2e_checks.{path.stem}", path)
        module = importlib.util.module_from_spec(spec)
        module.__dict__.update({"check": check, "Session": Session, "APPS": APPS, "CLI": CLI})
        spec.loader.exec_module(module)


def run_checks(root: Path, only: str | None) -> int:
    load_check_modules()
    s = Session(root)
    failures = 0
    try:
        for group, funcs in CHECKS.items():
            if only and group not in only.split(","):
                continue
            for f in funcs:
                name = f"{group}: {f.__name__}"
                t0 = time.time()
                try:
                    detail = f(s)
                    print(f"PASS {name} ({time.time() - t0:.1f}s)")
                    if detail and os.environ.get("E2E_VERBOSE"):
                        print("     " + str(detail).replace("\n", "\n     "))
                except Exception as e:  # noqa: BLE001 - report every failure and keep going
                    failures += 1
                    print(f"FAIL {name}: {e}")
    finally:
        s.close()
        if os.environ.get("E2E_KEEP"):
            print(f"kept {root}")
    print(f"{'all checks passed' if not failures else f'{failures} check(s) failed'}")
    return 1 if failures else 0


def run_command(root: Path, cmd: list[str]) -> int:
    s = Session(root)
    try:
        return subprocess.call(cmd, env=s.env)
    finally:
        s.close()


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--only", help="comma-separated check groups")
    p.add_argument("command", nargs="*", help="`run -- CMD…` runs CMD in the isolated session")
    args = p.parse_args()
    if os.environ.get("ARCADE_E2E_INNER") != "1":
        for tool in ("Xvfb", "dbus-run-session", "xdotool"):
            if not shutil.which(tool):
                print(f"e2e: {tool} is required (not run)", file=sys.stderr)
                return 2
        if not CLI.exists():
            subprocess.check_call(["cargo", "build", "-p", "arcade-link-cli"], cwd=LINK)
        # The private bus starts inside the isolated environment too, so
        # services it activates (portals, gvfs) never see the real profile.
        root = Path(tempfile.mkdtemp(prefix="arcade-e2e-"))
        env = isolated_env(root)
        env["ARCADE_E2E_INNER"] = "1"
        env["ARCADE_E2E_ROOT"] = str(root)
        try:
            return subprocess.call(["dbus-run-session", "--", sys.executable, *sys.argv], env=env)
        finally:
            if not os.environ.get("E2E_KEEP"):
                shutil.rmtree(root, ignore_errors=True)
    root = Path(os.environ["ARCADE_E2E_ROOT"])
    if args.command and args.command[0] == "run":
        return run_command(root, args.command[1:])
    return run_checks(root, args.only)


if __name__ == "__main__":
    sys.exit(main())
