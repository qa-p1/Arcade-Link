#!/usr/bin/env python3
"""Startup, warm-invoke and idle figures for the five Arcade apps (Linux).

Every app runs isolated from the desktop: a private D-Bus session, Xvfb,
temporary HOME/XDG_*/ARCADE_* directories, no Wayland or Hyprland
variables. Nothing is written to the real profile.

    python3 benchmarks/bench.py --json out.json             # measure
    python3 benchmarks/bench.py --json now.json --compare benchmarks/baseline.json
                                                            # fail on a >5% regression

Definitions (identical before and after every phase):
  startup      spawn -> the app's own readiness signal (see READY below)
  warm invoke  a second-instance command against the running app, median
  idle RSS     resident memory of the whole process tree after 3 s idle
  idle CPU     CPU time of the whole tree during 5 idle seconds
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

CODING = Path(__file__).resolve().parents[3]
BOX = CODING / "Rust/Arcade Box"
LENS = CODING / "Rust/Arcade-lens"
LOOK = CODING / "Rust/arcade-look"
WHEEL = CODING / "C++/Arcade wheel"
CLIP = CODING / "App dev/Arcade-clipboard"
SHELF = Path(os.environ.get("ARCADE_SHELF_REPO", str(Path(__file__).resolve().parents[2] / "Arcade-Shelf")))
FIND = Path(os.environ.get("ARCADE_FIND_REPO", str(Path(__file__).resolve().parents[2] / "Arcade-Find")))

APPS = {
    "box": {
        "start": [str(BOX / "target/release/arcade-desktop")],
        "ready": "box",
        "warm": [str(BOX / "target/release/arcadebox"), "run", "arcade.developer.hash", "abc", "--set", "algorithm=sha256", "--json"],
    },
    "lens": {
        "start": [str(LENS / "target/release/arcade-lens"), "--background"],
        "ready": "lens",
        "warm": [str(LENS / "target/release/arcade-lens"), "--background"],
    },
    "look": {
        "start": [str(LOOK / "src-tauri/target/release/arcade-look"), "--service"],
        "ready": "look",
        "warm": [str(LOOK / "src-tauri/target/release/arcade-look"), "--service"],
    },
    "wheel": {
        "start": [str(WHEEL / "build/arcade-wheel"), "--background"],
        "ready": "wheel",
        "warm": [str(WHEEL / "build/arcade-wheel"), "--status"],
    },
    "shelf": {
        "start": [str(SHELF / "build/arcade-shelf"), "--background"],
        "ready": "shelf",
        "warm": [str(SHELF / "build/arcade-shelf"), "--status"],
    },
    "find": {
        "start": [str(FIND / "target/release/arcade-find"), "--background"],
        "ready": "find",
        "warm": [str(FIND / "target/release/arcade-find"), "--status"],
    },
    "clipboard": {
        "start": [str(CLIP / "apps/flutter_app/build/linux/x64/release/bundle/clipboard"), "--background"],
        "ready": "clipboard",
        # Isolated profiles (ARCADE_DATA_DIR) run non-unique, so there is no
        # second-instance channel to time.
        "warm": None,
    },
}

READY_TIMEOUT = 20.0


def tree_pids(root: int) -> list[int]:
    pids, frontier = [root], [root]
    while frontier:
        nxt = []
        for pid in frontier:
            try:
                for tid in os.listdir(f"/proc/{pid}/task"):
                    with open(f"/proc/{pid}/task/{tid}/children") as f:
                        nxt += [int(c) for c in f.read().split()]
            except OSError:
                pass
        pids += nxt
        frontier = nxt
    return pids


def tree_rss_kib(root: int) -> int:
    total = 0
    for pid in tree_pids(root):
        try:
            with open(f"/proc/{pid}/status") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
        except OSError:
            pass
    return total


def tree_cpu_ms(root: int) -> float:
    """utime+stime of the tree; process-level figures include exited threads."""
    ticks = 0
    for pid in tree_pids(root):
        try:
            with open(f"/proc/{pid}/stat") as f:
                fields = f.read().rsplit(")", 1)[1].split()
            ticks += int(fields[11]) + int(fields[12])
        except (OSError, IndexError, ValueError):
            pass
    return ticks * 1000 / os.sysconf("SC_CLK_TCK")


def name_has_owner(name: str, env: dict) -> bool:
    r = subprocess.run(
        ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
         "--method", "org.freedesktop.DBus.NameHasOwner", name],
        env=env, capture_output=True, text=True)
    return "true" in r.stdout


def lens_ping(home: Path) -> bool:
    try:
        port, token = (home / "data/instance").read_text().split()
        with socket.create_connection(("127.0.0.1", int(port)), timeout=0.3) as s:
            s.sendall(f"{token} ping\n".encode())
            return s.recv(64).strip() == b"pong"
    except (OSError, ValueError):
        return False


def wheel_status(runtime: Path) -> bool:
    for sock in glob.glob(str(runtime / "arcade-wheel-*.sock")):
        try:
            with socket.socket(socket.AF_UNIX) as s:
                s.settimeout(0.3)
                s.connect(sock)
                s.sendall(b"--status\n")
                return b"running" in s.recv(4096)
        except OSError:
            continue
    return False


def profile_locked(path: Path) -> bool:
    """Clipboard's core holds an flock on profile.lock once it has opened the profile."""
    import fcntl
    try:
        fd = os.open(path, os.O_RDWR)
    except OSError:
        return False
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    except BlockingIOError:
        return True
    finally:
        os.close(fd)


def box_window(pid: int, env: dict) -> bool:
    # The island starts hidden; Box is ready once its tray menu and window
    # exist. xdotool sees the (unmapped) window as soon as GTK creates it.
    r = subprocess.run(["xdotool", "search", "--pid", str(pid)], env=env, capture_output=True, text=True)
    return bool(r.stdout.strip())


def is_ready(kind: str, pid: int, root: Path, env: dict) -> bool:
    if kind == "box":
        return box_window(pid, env)
    if kind == "lens":
        return lens_ping(root / "lens")
    if kind == "look":
        return name_has_owner("org.gnome.NautilusPreviewer", env)
    if kind == "wheel":
        return wheel_status(root / "runtime")
    if kind in ("shelf", "find"):
        endpoint = root / f"arcade/run/arcade.{kind}.endpoint"
        try:
            entry = json.loads(endpoint.read_text())
            with socket.socket(socket.AF_UNIX) as channel:
                channel.settimeout(0.15)
                channel.connect(entry["address"])
                request = {"v": 1, "id": 1, "method": "hello", "params": {
                    "token": entry["token"], "client": {"id": "arcade.tools", "version": "benchmark"}, "protocol": [1]}}
                channel.sendall((json.dumps(request) + "\n").encode())
                return "result" in json.loads(channel.recv(4096))
        except (OSError, ValueError, KeyError):
            return False
    if kind == "clipboard":
        return profile_locked(root / "clipdata/profile.lock")
    raise ValueError(kind)


def isolated_env(root: Path) -> dict:
    env = {k: v for k, v in os.environ.items() if k not in (
        "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION", "SWAYSOCK", "XDG_SESSION_TYPE", "APPIMAGE", "APPDIR", "OWD", "ARGV0")}
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
        "NO_AT_BRIDGE": "1",
    })
    return env


def stop(proc: subprocess.Popen) -> None:
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait()


def start(app: dict, root: Path, env: dict) -> tuple[subprocess.Popen, float | None]:
    t0 = time.perf_counter()
    proc = subprocess.Popen(app["start"], env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=open(root / "stderr.log", "ab"), start_new_session=True)
    while time.perf_counter() - t0 < READY_TIMEOUT:
        if proc.poll() is not None:
            return proc, None
        if is_ready(app["ready"], proc.pid, root, env):
            return proc, (time.perf_counter() - t0) * 1000
        time.sleep(0.002)
    return proc, None


def measure(name: str, runs: int) -> dict:
    app = APPS[name]
    if not Path(app["start"][0]).exists():
        return {"error": f"not built: {app['start'][0]}"}
    root = Path(tempfile.mkdtemp(prefix=f"arcade-bench-{name}-"))
    env = isolated_env(root)
    out: dict = {"startup_ms": [], "idle_rss_kib": [], "idle_cpu_ms_5s": []}
    try:
        # First run: first-run setup (profile, defaults) is not part of startup.
        proc, t = start(app, root, env)
        time.sleep(1.0)
        stop(proc)
        if t is None:
            return {"error": "did not become ready", "stderr": (root / "stderr.log").read_text()[-2000:]}
        for i in range(runs):
            proc, t = start(app, root, env)
            if t is None:
                stop(proc)
                return {"error": "did not become ready", "stderr": (root / "stderr.log").read_text()[-2000:]}
            out["startup_ms"].append(round(t, 1))
            time.sleep(3.0)
            out["idle_rss_kib"].append(tree_rss_kib(proc.pid))
            c0 = tree_cpu_ms(proc.pid)
            time.sleep(5.0)
            out["idle_cpu_ms_5s"].append(round(tree_cpu_ms(proc.pid) - c0, 1))
            if i == 0 and app["warm"]:
                warm = []
                for _ in range(11):
                    w0 = time.perf_counter()
                    subprocess.run(app["warm"], env=env, capture_output=True, timeout=30)
                    warm.append((time.perf_counter() - w0) * 1000)
                out["warm_invoke_ms"] = round(statistics.median(warm[1:]), 1)
            stop(proc)
            time.sleep(0.5)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    out["startup_ms_median"] = round(statistics.median(out["startup_ms"]), 1)
    out["idle_rss_kib_median"] = int(statistics.median(out["idle_rss_kib"]))
    out["idle_cpu_ms_5s_median"] = statistics.median(out["idle_cpu_ms_5s"])
    return out


def compare(now: dict, base: dict, tolerance: float) -> list[str]:
    """Regressions over `tolerance` (startup and warm invoke also get a 5 ms
    floor, so scheduler noise on a sub-30 ms figure isn't reported)."""
    problems = []
    for app, b in base.get("apps", {}).items():
        n = now.get("apps", {}).get(app)
        if not n or "error" in b:
            continue
        if "error" in n:
            problems.append(f"{app}: {n['error']}")
            continue
        for key, floor in (("startup_ms_median", 5.0), ("warm_invoke_ms", 5.0), ("idle_rss_kib_median", 0.0)):
            if key in b and key in n and n[key] > b[key] * (1 + tolerance) + floor:
                problems.append(f"{app}: {key} {b[key]} -> {n[key]} (> {tolerance:.0%})")
    return problems


def inner(args) -> int:
    xvfb = subprocess.Popen(["Xvfb", ":97", "-screen", "0", "1920x1080x24", "-nolisten", "tcp"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.environ["DISPLAY"] = ":97"
    for _ in range(100):
        if subprocess.run(["xdotool", "getdisplaygeometry"], capture_output=True).returncode == 0:
            break
        time.sleep(0.05)
    keyring = None
    if shutil.which("gnome-keyring-daemon"):
        kr_home = Path(tempfile.mkdtemp(prefix="arcade-bench-keyring-"))
        kenv = dict(os.environ, HOME=str(kr_home), XDG_DATA_HOME=str(kr_home))
        keyring = subprocess.Popen(["gnome-keyring-daemon", "--foreground", "--unlock", "--components=secrets"],
                                   env=kenv, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        keyring.stdin.write(b"bench\n")
        keyring.stdin.close()
        time.sleep(0.5)
    try:
        result = {"date": time.strftime("%Y-%m-%d %H:%M"), "runs": args.runs, "apps": {}}
        for name in args.apps.split(","):
            print(f"measuring {name}…", file=sys.stderr, flush=True)
            result["apps"][name] = measure(name, args.runs)
            print(json.dumps(result["apps"][name]), file=sys.stderr, flush=True)
    finally:
        if keyring:
            keyring.terminate()
        xvfb.terminate()
    text = json.dumps(result, indent=2)
    if args.json:
        Path(args.json).write_text(text + "\n")
    print(text)
    if args.compare:
        problems = compare(result, json.loads(Path(args.compare).read_text()), args.tolerance)
        for p in problems:
            print(f"REGRESSION {p}", file=sys.stderr)
        return 1 if problems else 0
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--apps", default=",".join(APPS))
    p.add_argument("--runs", type=int, default=5)
    p.add_argument("--json")
    p.add_argument("--compare")
    p.add_argument("--tolerance", type=float, default=0.05)
    args = p.parse_args()
    if os.environ.get("ARCADE_BENCH_INNER") == "1":
        return inner(args)
    for tool in ("Xvfb", "dbus-run-session", "xdotool", "gdbus"):
        if not shutil.which(tool):
            print(f"bench: {tool} is required", file=sys.stderr)
            return 2
    # The private bus starts inside an isolated environment too, so services
    # it activates (portals, gvfs) never see the real profile.
    bus_root = Path(tempfile.mkdtemp(prefix="arcade-bench-bus-"))
    env = isolated_env(bus_root)
    env.pop("DBUS_SESSION_BUS_ADDRESS", None)
    env.pop("DISPLAY", None)
    env["ARCADE_BENCH_INNER"] = "1"
    try:
        return subprocess.call(["dbus-run-session", "--", sys.executable, *sys.argv], env=env)
    finally:
        shutil.rmtree(bus_root, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
