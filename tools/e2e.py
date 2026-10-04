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
import tempfile
import time
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
        "bin": "src-tauri/target/release/arcade-look",
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
        display = ":96"
        self.xvfb = subprocess.Popen(["Xvfb", display, "-screen", "0", "1920x1080x24", "-nolisten", "tcp"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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


def run_checks(root: Path, only: str | None) -> int:
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
