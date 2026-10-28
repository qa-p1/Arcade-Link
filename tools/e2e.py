#!/usr/bin/env python3
"""Ecosystem end-to-end runner (Linux, headless).

Starts the Arcade apps from their sibling checkouts in background mode
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
import re
import shutil
import signal
import subprocess
import sys
import struct
import tempfile
import time
import traceback
import zlib
from pathlib import Path

# Helpers import e2e even when this file is the script entry point. Share the
# same Session, registry and SkipCheck type instead of importing a second copy.
if __name__ == '__main__':
    sys.modules['e2e'] = sys.modules[__name__]

LINK = Path(__file__).resolve().parents[1]
CODING = LINK.parents[1]
CLI = LINK / "target/debug/arcade-link"


class SkipCheck(AssertionError):
    """An explicit unavailable prerequisite; never counted as a passing check."""


class MissingArtifact(SkipCheck):
    """The app's package or development build is missing. With
    --require-artifacts (the nightly real-fleet run) this fails the check."""


def link_app_ids() -> list[str]:
    source = (LINK / "crates/arcade-link/src/manifest.rs").read_text(encoding="utf-8")
    constants = dict(re.findall(r'pub const (\w+): &str = "(arcade\.[^"]+)";', source))
    names = re.search(r'pub const APPS:.*?= \[([^]]+)\]', source).group(1)
    return [constants[name.strip()] for name in names.split(",") if name.strip()]


APP_IDS = link_app_ids()

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
    "arcade.shelf": {
        "dir": Path(os.environ.get("ARCADE_SHELF_REPO", str(CODING / "C++/Arcade-Shelf"))),
        "bin": "build/arcade-shelf",
        "args": ["--background"],
        "build": ["cmake", "--build", "build"],
    },
    "arcade.find": {
        "dir": Path(os.environ.get("ARCADE_FIND_REPO", str(LINK.parent / "Arcade-Find"))),
        "bin": "target/release/arcade-find",
        "args": ["--background"],
        "build": ["cargo", "build", "--release", "-p", "arcade-find"],
    },
    "arcade.clipboard": {
        "dir": CODING / "App dev/Arcade-clipboard",
        "bin": "apps/flutter_app/build/linux/x64/release/bundle/clipboard",
        "args": ["--background"],
        "build": ["bash", "scripts/build-linux.sh"],
        "source_suffixes": {".cpp", ".cc", ".c", ".h", ".hpp"},
        "source_names": {"CMakeLists.txt"},
        "components": [
            {"bin": "apps/flutter_app/build/linux/x64/release/bundle/lib/libapp.so", "source_suffixes": {".dart"}, "source_names": {"pubspec.yaml", "pubspec.lock"}},
            {"bin": "apps/flutter_app/build/linux/x64/release/bundle/lib/libarcade_core.so", "source_suffixes": {".rs", ".toml"}, "source_names": {"Cargo.lock"}},
        ],
    },
}

TOOLS = {"dir": CODING / "Rust/Arcade-tools", "bin": "target/debug/arcade-tools",
         "args": ["--background"], "build": ["cargo", "build", "-p", "arcade-tools"]}
FLEET = {**APPS, "arcade.tools": TOOLS}
# Arcade Tools' data folder under a session root. Since Tools 0.4.0 it comes
# from the paths module (`arcade/tools`); Tools moves its 0.3 folder there.
TOOLS_DATA = Path("arcade/.local/share/arcade/tools")
# Protocol IDs, CLI names, source trees/binaries, package globs and tray
# expectations live together in FLEET. APPS retains the seven-peer contract.
ARTIFACT_NAMES = {
    "arcade.box": ("Arcade-box", "arcade-box-appimage"),
    "arcade.lens": ("Arcade-lens", "linux-appimage"),
    "arcade.look": ("Arcade-look", "arcade-look-linux-x86_64"),
    "arcade.wheel": ("Arcade-wheel", "ArcadeWheel-Linux-x86_64"),
    "arcade.shelf": ("Arcade-Shelf", "Arcade-Shelf-ubuntu-*"),
    "arcade.find": ("Arcade-Find", "package-linux-x64"),
    "arcade.clipboard": ("Arcade-clipboard", "Arcade-Clipboard-linux-x64"),
    "arcade.tools": ("Arcade-tools", "tools-installer-ubuntu-*"),
}
# Box's installer links its desktop app as arcade-desktop; arcade-box is
# its separate terminal tool (Box installer.rs CLI_NAME).
CLI_NAMES = {"arcade.box": "arcade-desktop"}
ARTIFACTS = json.loads(Path(os.environ["ARCADE_E2E_ARTIFACTS"]).read_text(encoding="utf-8")) if os.environ.get("ARCADE_E2E_ARTIFACTS") else {}
for app, spec in FLEET.items():
    prefix = "ARCADE_" + app.removeprefix("arcade.").upper()
    repository, artifact = ARTIFACT_NAMES[app]
    spec.update(id=app, cli=CLI_NAMES.get(app, app.replace('.', '-')), repo='qa-p1/' + repository, artifact=artifact,
                tray={"own": 1, "hosted": 1 if app == "arcade.tools" else 0,
                      "hostable": app != "arcade.tools"},
                packages=("dist/*.AppImage", "src-tauri/target/release/bundle/appimage/*.AppImage",
                          "target/release/bundle/appimage/*.AppImage"))
    spec["dir"] = Path(os.environ.get(prefix + "_REPO", ARTIFACTS.get(app, {}).get("repoPath", str(spec["dir"])))).resolve()
    spec["bin"] = os.environ.get(prefix + "_BIN", spec["bin"])


STRIP = ("WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP",
         "DESKTOP_SESSION", "SWAYSOCK", "XDG_SESSION_TYPE", "APPIMAGE", "APPDIR", "OWD", "ARGV0",
         "DBUS_SESSION_BUS_ADDRESS", "DISPLAY", "ARCADE_DEV_BUILD")


def source_input(path: Path) -> bool:
    return path.suffix in {".rs", ".cpp", ".cc", ".c", ".h", ".hpp", ".qml", ".dart", ".svelte", ".ts", ".js", ".toml", ".cmake", ".css", ".html", ".qrc", ".ui"} or path.name in {
        "CMakeLists.txt", "Cargo.lock", "package.json", "package-lock.json", "pubspec.yaml", "pubspec.lock", "tauri.conf.json"}


def source_paths(directory: Path) -> list[Path]:
    tracked = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=directory, capture_output=True)
    if tracked.returncode == 0:
        paths = [directory / os.fsdecode(name) for name in tracked.stdout.split(b"\0") if name]
    else:
        paths = []
        for root, dirs, files in os.walk(directory):
            dirs[:] = [d for d in dirs if d not in {".git", "target", "build", "node_modules", ".dart_tool", ".tools", "dist"}]
            paths.extend(Path(root) / name for name in files)
    return [path for path in paths if path.is_file() and source_input(path)
            and not {'tests', 'benches', '.github'} & set(path.relative_to(directory).parts)]


def require_current_build(spec: dict) -> Path:
    """Refuse missing binaries and builds older than their source inputs."""
    executable = spec["dir"] / spec["bin"]
    hint = " ".join(spec.get("build") or ["run this app's documented build command"])
    message = f"stale or missing build: {executable}; rebuild in {spec['dir']}: {hint}"
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise MissingArtifact(message + " (missing executable build)")
    stamp = executable.stat().st_mtime_ns
    sources = source_paths(spec["dir"])
    if "source_suffixes" in spec:
        sources = [p for p in sources if p.suffix in spec["source_suffixes"] or p.name in spec.get("source_names", set())]
    newest = max(sources, key=lambda p: p.stat().st_mtime_ns, default=None)
    if newest and newest.stat().st_mtime_ns > stamp:
        raise SkipCheck(message + f" (stale build; newer source: {newest})")
    for component in spec.get("components", []):
        path = spec["dir"] / component["bin"]
        if not path.is_file():
            raise SkipCheck(message + f" (missing component: {path})")
        inputs = [p for p in source_paths(spec["dir"]) if p.suffix in component["source_suffixes"] or p.name in component.get("source_names", set())]
        newest = max(inputs, key=lambda p: p.stat().st_mtime_ns, default=None)
        if newest and newest.stat().st_mtime_ns > path.stat().st_mtime_ns:
            raise SkipCheck(message + f" (stale component: {path}; newer source: {newest})")
    return executable


def isolated_env(root: Path) -> dict:
    env = {k: v for k, v in os.environ.items() if k not in STRIP}
    for d in ("home", "config", "data", "cache", "runtime", "lens", "clipdata", "arcade", "wheel", "shelf", "find"):
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
        "ARCADE_SHELF_HOME": str(root / "shelf"),
        "ARCADE_FIND_HOME": str(root / "find"),
        "ARCADE_WHEEL_DISABLE_GLOBAL_SHORTCUT": "1",
        "QT_QPA_PLATFORM": "xcb",
        "GDK_BACKEND": "x11",
        "WEBKIT_DISABLE_COMPOSITING_MODE": "1",
        "NO_AT_BRIDGE": "1",
        "ARCADE_NO_INSTALL_PROMPT": "1",
        "PATH": f"{CLI.parent}:{os.environ.get('PATH', '')}",
    })
    # Toolchains stay shared: with the temporary HOME, cargo would otherwise
    # start from an empty registry and rebuild every dependency.
    for name, default in (("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")):
        path = os.environ.get(name) or str(Path.home() / default)
        if Path(path).is_dir():
            env[name] = path
    return env


class Session:
    """Xvfb + keyring inside the private D-Bus session this process runs in."""

    def __init__(self, root: Path, headless: bool = False, mock_peers: bool = False, real_apps: bool = False):
        sandbox = os.environ.get("ARCADE_E2E_ROOT")
        if os.environ.get("ARCADE_E2E_INNER") != "1" or not sandbox or not root.is_relative_to(Path(sandbox)):
            raise AssertionError("lifecycle runs require tools/e2e.py's isolated session")
        self.root = root
        self.env = isolated_env(root)
        self.procs: dict[str, subprocess.Popen] = {}
        self.mock_peers = mock_peers
        self.real_apps = real_apps
        self.xvfb = self.keyring = None
        if headless:
            if os.environ.get('ARCADE_E2E_RUN') == '1':
                self.env.update({key: os.environ[key] for key in ('DISPLAY', 'DBUS_SESSION_BUS_ADDRESS') if key in os.environ})
            return
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
        r = subprocess.run([str(CLI), *args], env=self.env, capture_output=True, text=True, encoding="utf-8", timeout=timeout)
        if check and r.returncode != 0:
            raise AssertionError(f"arcade-link {' '.join(args)} failed:\n{r.stdout}{r.stderr}")
        return r

    def start(self, app: str, wait: bool = True) -> subprocess.Popen:
        spec = TOOLS if app == "arcade.tools" else APPS[app]
        if app in self.procs and self.procs[app].poll() is None:
            return self.procs[app]
        if self.mock_peers:
            exe = CLI
        elif self.real_apps:
            from real_apps import binary
            exe = binary(app)
        else:
            exe = require_current_build(spec)
        args = ["mock", "--as", app.removeprefix("arcade."), "--actions", str(LINK / "fixtures" / (app.removeprefix("arcade.") + ".json"))] if self.mock_peers else spec["args"]
        log = open(self.root / f"{app}.log", "ab")
        env = dict(self.env, APPIMAGE_EXTRACT_AND_RUN="1") if exe.suffix == ".AppImage" else self.env
        p = subprocess.Popen([str(exe), *args], env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                             start_new_session=True)
        log.close()
        self.procs[app] = p
        if wait:
            self.wait_running(app)
        return p

    def endpoint(self, app: str) -> Path:
        return self.root / "arcade/run" / f"{app}.endpoint"

    def wait_running(self, app: str, timeout: float = 20) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if 'sni_watcher' in sys.modules:
                sys.modules['sni_watcher'].pump()
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
        r = subprocess.run(cmd, env=self.env, capture_output=True, text=True, encoding="utf-8", timeout=timeout)
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

    def kill(self, app: str, sig=None) -> None:
        p = self.procs.pop(app, None)
        if p and p.poll() is None:
            if os.name == "nt":
                if sig is None:
                    p.kill()
                else:
                    p.terminate()
                p.wait(timeout=5)
                return
            if sig is None:
                sig = signal.SIGKILL
            try:
                os.killpg(p.pid, sig)
            except ProcessLookupError:
                pass
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                p.wait()
        if p and os.name != "nt":
            # An AppImage runtime (the group leader) exits before the app it
            # runs. Wait for the whole group, so the app's global shortcut
            # grabs and bus names are free before the next profile starts.
            if not wait_gone(lambda: os.killpg(p.pid, 0), 5):
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                wait_gone(lambda: os.killpg(p.pid, 0), 5)

    def stop_apps(self) -> None:
        quitting = []
        if CLI.is_file():
            for endpoint in sorted((self.root / "arcade/run").glob("*.endpoint")):
                try:
                    entry = json.loads(endpoint.read_text(encoding="utf-8"))
                    pid = entry["pid"]
                    environment = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
                    if f"ARCADE_HOME={self.env['ARCADE_HOME']}".encode() in environment:
                        quitting.append(pid)
                        self.cli("quit", endpoint.stem, "--force", timeout=3)
                except (OSError, ValueError, KeyError, subprocess.TimeoutExpired):
                    pass
        for app in list(self.procs):
            self.kill(app, signal.SIGTERM)
        # `quit` only asks; apps that Tools or Link launched have no Popen here.
        # (POSIX only: on Windows os.kill(pid, 0) terminates instead of probing.)
        for pid in quitting if os.name != "nt" else []:
            if not wait_gone(lambda: os.kill(pid, 0), 5):
                try:
                    os.kill(pid, signal.SIGKILL)
                except OSError:
                    pass

    def close(self) -> None:
        self.stop_apps()
        if self.keyring:
            self.keyring.terminate()
        if self.xvfb:
            self.xvfb.terminate()


def wait_gone(probe, timeout: float) -> bool:
    """Poll `probe` (a signal-0 send) until the process or group is gone."""
    deadline = time.monotonic() + timeout
    while True:
        try:
            probe()
        except ProcessLookupError:
            return True
        except PermissionError:
            return True  # the pid was reused by a process we don't own
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.05)


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
    # A fresh Box answers "still checking its engines" until its first probe ends.
    deadline = time.monotonic() + 60
    while True:
        code, r = s.invoke("box", "box:arcade.image.convert#webp", "--file", str(png), timeout=120)
        if code == 0 or "still checking its engines" not in str(r) or time.monotonic() > deadline:
            break
        time.sleep(1)
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
    # One-shot stdin carries one ordinary `invoke` request message (SPEC §4.4).
    msg = {"v": 1, "id": 1, "method": "invoke", "params": req}
    r = subprocess.run([str(require_current_build({**spec, "bin": os.environ.get("ARCADE_BOX_CLI_BIN", "target/release/arcadebox")})), "--arcade-invoke"], input=json.dumps(msg) + "\n",
                       env=s.env, capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stdout + r.stderr
    final = json.loads(r.stdout.strip().splitlines()[-1])
    assert "result" in final, final
    out = final["result"]["outputs"][0]
    assert out["type"] == "file/image" and Path(out["path"]).read_bytes()[:8] == b"\x89PNG\r\n\x1a\n", final
    return out["path"]


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
    overlay = s.wait_window("^Arcade Lens$")
    s.xdotool("windowraise", overlay, "windowfocus", overlay)
    # egui needs a frame between pointer events to see a drag.
    s.xdotool("mousemove", "100", "100", "sleep", "0.2", "mousedown", "1", "sleep", "0.2",
              "mousemove", "300", "250", "sleep", "0.2", "mouseup", "1")
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
    # The request waits in Wheel's Settings; closing that window cancels it.
    win = s.wait_window("Arcade Wheel.*Settings")
    time.sleep(0.5)
    s.xdotool("windowfocus", win, "key", "ctrl+w")
    code, r = finish(p)
    assert code != 0 and "Cancelled." in r and "[denied:" in r, r
    return r


@check("actions")
def clipboard_actions_without_devices(s: Session) -> str:
    ensure_running(s, "arcade.clipboard")
    code, r = s.invoke("clipboard", "clipboard.devices")
    assert code == 0 and outputs(r)[0]["data"] == [], r
    code, r = s.invoke("clipboard", "clipboard.add", "--text", "hello")
    assert code != 0 and "unavailable" in r, r
    return r


def load_check_modules(real_apps: bool = False) -> None:
    """Per-app check groups live in tools/e2e_checks/<name>.py; each module
    uses `check(group)` and the `Session` helpers from this file."""
    import importlib.util
    for path in sorted((Path(__file__).parent / "e2e_checks").glob("*.py")):
        if (path.stem == "link_v3" and real_apps) or (path.stem == "real_hub" and not real_apps):
            continue
        spec = importlib.util.spec_from_file_location(f"e2e_checks.{path.stem}", path)
        module = importlib.util.module_from_spec(spec)
        module.__dict__.update({"check": check, "Session": Session, "APPS": APPS, "APP_IDS": APP_IDS, "FLEET": FLEET, "SkipCheck": SkipCheck, "MissingArtifact": MissingArtifact, "CLI": CLI, "LINK": LINK, "CODING": CODING, "TOOLS": TOOLS, "TOOLS_DATA": TOOLS_DATA, "require_current_build": require_current_build})
        spec.loader.exec_module(module)
        if path.stem == 'real_hub':
            for group in ('install', 'trayhost', 'tools-hub', 'shortcuts'):
                for function in CHECKS[group]:
                    if not hasattr(function, 'apps'):
                        function.apps = list(FLEET)


def run_checks(root: Path, only: str | None, scaffold: bool = False, real_apps: bool = False, names: str | None = None, allow_skips: bool = False,
               require_artifacts: bool = False) -> int:
    load_check_modules(real_apps)
    if scaffold:
        print("Link scaffold: mock peer presence, crate watchers/install harness, sibling shortcut documents")
    selected = [g.strip() for g in only.split(",")] if only is not None else None
    if selected is not None and (not all(selected) or any(g not in CHECKS for g in selected)):
        # A typo or an empty selection must never report success.
        print(f"unknown or empty check group selection {only!r}; known: {', '.join(CHECKS)}")
        return 2
    selected_names = names.split(",") if names else None
    if selected_names and not set(selected_names) <= {f.__name__ for group, funcs in CHECKS.items() if selected is None or group in selected for f in funcs}:
        print(f"unknown check selection {names!r}")
        return 2
    s = Session(root, headless=scaffold or os.environ.get('ARCADE_E2E_RUN') == '1', mock_peers=scaffold, real_apps=real_apps)
    failures = 0
    not_run = 0
    expected = 0
    ran = 0
    results = []
    try:
        for group, funcs in CHECKS.items():
            if selected is not None and group not in selected:
                continue
            for f in funcs:
                if selected_names and f.__name__ not in selected_names:
                    continue
                ran += 1
                name = f"{group}: {f.__name__}"
                t0 = time.time()
                # A documented product defect outside Link: its failure is
                # reported as XFAIL, and an unexpected pass fails the run so
                # the stale note is removed (strict expected failure).
                known = getattr(f, "known_bug", None)
                match = getattr(f, "known_bug_match", None)
                try:
                    detail = f(s)
                    if known:
                        failures += 1
                        print(f"XPASS {name}: passed although marked as a known product bug; remove the note: {known}", flush=True)
                        results.append({"group": group, "check": f.__name__, "apps": getattr(f, "apps", []), "passed": False, "status": "unexpected_pass", "seconds": round(time.time() - t0, 3), "detail": str(detail or ""), "knownBug": known})
                        continue
                    print(f"PASS {name} ({time.time() - t0:.1f}s)", flush=True)
                    results.append({"group": group, "check": f.__name__, "apps": getattr(f, "apps", []), "passed": True, "status": "pass", "seconds": round(time.time() - t0, 3), "detail": str(detail or "")})
                    if detail and os.environ.get("E2E_VERBOSE"):
                        print("     " + str(detail).replace("\n", "\n     "))
                except Exception as e:  # noqa: BLE001 - report every failure and keep going
                    # A check expected to run may not skip for a missing artifact.
                    missing = isinstance(e, MissingArtifact)
                    unavailable = isinstance(e, SkipCheck) and not (missing and require_artifacts)
                    # A known bug excuses only its own documented symptom.
                    status = "skipped" if unavailable else "expected_failure" if known and (not match or match in str(e)) else "fail"
                    if unavailable:
                        not_run += 1
                    elif status == "expected_failure":
                        expected += 1
                    else:
                        failures += 1
                    label = {"skipped": "SKIP", "expected_failure": "XFAIL", "fail": "FAIL"}[status]
                    note = " (missing artifact; --require-artifacts)" if missing and require_artifacts else ""
                    print(f"{label} {name}{note}: {e}" + (f"\n     known product bug: {known}" if status == "expected_failure" else ""), flush=True)
                    results.append({"group": group, "check": f.__name__, "apps": getattr(f, "apps", []), "passed": False, "status": status, "seconds": round(time.time() - t0, 3), "detail": str(e) or type(e).__name__, "traceback": traceback.format_exc(),
                                    **({"missingArtifact": True} if missing else {}), **({"knownBug": known} if known else {})})
                finally:
                    if real_apps:
                        s.stop_apps()
    finally:
        s.close()
        if os.environ.get("E2E_KEEP"):
            print(f"kept {root}")
    if not ran:
        print("no checks ran")
        return 1
    passed = sum(r["status"] == "pass" for r in results)
    print(f"RESULT {only or 'all'}: {passed} passed, {failures} failed, {expected} expected failures (known product bugs), {not_run} skipped ({ran} checks)", flush=True)
    report_dir = os.environ.get("ARCADE_E2E_REPORT_DIR")
    if report_dir:
        Path(report_dir).mkdir(parents=True, exist_ok=True)
        report_name = (only or "all") + ("-" + names if names else "")
        (Path(report_dir) / (report_name.replace(",", "-") + ".json")).write_text(
            json.dumps({"dateUtc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "realApps": real_apps, "passed": passed, "failed": failures, "expectedFailures": expected, "skipped": not_run, "notRun": not_run, "checks": results}, indent=2) + "\n", encoding="utf-8")
    return 1 if failures or (not_run and not allow_skips) else 0


def run_command(root: Path, cmd: list[str]) -> int:
    s = Session(root)
    s.env['ARCADE_E2E_RUN'] = '1'
    try:
        return subprocess.call(cmd, env=s.env)
    finally:
        s.close()


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--only", help="comma-separated check groups")
    p.add_argument("--check", help="comma-separated check function names, within --only")
    p.add_argument("--real-apps", action="store_true", help="exercise the real fleet and Tools core; require current builds and AppImages")
    p.add_argument("--allow-skips", action="store_true", help="accept explicitly reported unavailable prerequisites; failures still fail")
    p.add_argument("--require-artifacts", action="store_true", help="fail (not skip) a check whose app package or development build is missing")
    p.add_argument("command", nargs="*", help="`run -- CMD…` runs CMD in the isolated session")
    args = p.parse_args()
    if args.command == ["run"]:
        p.error("run needs a command: tools/e2e.py run -- CMD...")
    if args.command and args.command[0] != "run":
        if args.only or len(args.command) != 1:
            p.error("select one group, or use --only with comma-separated groups")
        args.only = args.command[0]
        args.command = []
    scaffold = bool(args.only and set(args.only.split(",")) <= {"presence", "trayhost", "install", "shortcuts", "tools-hub"} and set(args.only.split(",")) & {"trayhost", "install", "shortcuts", "tools-hub"} and not args.real_apps)
    if os.environ.get("ARCADE_E2E_INNER") != "1":
        for tool in ("Xvfb", "dbus-run-session", "xdotool"):
            if not shutil.which(tool):
                print(f"e2e: {tool} is required (not run)", file=sys.stderr)
                return 2
        subprocess.check_call(["cargo", "build", "-p", "arcade-link-cli", "--locked"], cwd=LINK, env=dict(os.environ, CARGO_BUILD_JOBS="3"))
        if args.real_apps and (args.only is None or set(args.only.split(",")) & {"tools-hub", "shortcuts"}):
            from real_apps import build_driver
            try:
                build_driver()
            except SkipCheck as error:
                print(f"SKIP Tools adapter: {error}", flush=True)
        if not args.command and (args.only is None or "install" in args.only.split(",")):
            subprocess.check_call(["cargo", "build", "-p", "arcade-link", "--example", "install-harness", "--features", "install", "--locked"], cwd=LINK, env=dict(os.environ, CARGO_BUILD_JOBS="3"))
        if args.only is None and not args.command and not args.check:
            # A full run gives every group its own fresh session: groups leave
            # apps running in states (open pickers, toggled settings, held
            # profiles) that the next group must not inherit.
            load_check_modules(args.real_apps)
            failed = [g for g in CHECKS if isolated(sys.argv[:1] + ['run', '--', sys.executable] + sys.argv[:1] + ["--only", g] + (["--real-apps"] if args.real_apps else []) + (["--allow-skips"] if args.allow_skips else []) + (["--require-artifacts"] if args.require_artifacts else [])) != 0]
            print(f"groups failed: {', '.join(failed)}" if failed else "all groups passed")
            return 1 if failed else 0
        argv = sys.argv if args.command else sys.argv[:1] + ['run', '--', sys.executable] + sys.argv
        return isolated(argv)
    root = Path(os.environ["ARCADE_E2E_ROOT"])
    if args.command and args.command[0] == "run":
        return run_command(root, args.command[1:])
    return run_checks(root, args.only, scaffold, args.real_apps, args.check, args.allow_skips, args.require_artifacts)


def isolated(argv: list[str]) -> int:
    """Runs this script with `argv` inside a new isolated session."""
    # The private bus starts inside the isolated environment too, so
    # services it activates (portals, gvfs) never see the real profile.
    root = Path(tempfile.mkdtemp(prefix="arcade-e2e-"))
    env = isolated_env(root)
    env["ARCADE_E2E_INNER"] = "1"
    env["ARCADE_E2E_ROOT"] = str(root)
    try:
        return subprocess.call(["dbus-run-session", "--", sys.executable, *argv], env=env)
    finally:
        if not os.environ.get("E2E_KEEP"):
            shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
