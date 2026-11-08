#!/usr/bin/env python3
"""Tier 2 smoke: smoke.py <AppImage> [--show ARG] — the app starts on this
desktop, stays up, answers --status, registers a tray item (or says why not),
and leaves no coredump. Window placement is Tier 1 (Hyprland) only."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

app = sys.argv[1]
show = sys.argv[sys.argv.index("--show") + 1] if "--show" in sys.argv else "--background"
desktop = os.environ.get("ARCADE_DESKTOP", "?")
home = tempfile.mkdtemp(prefix="smoke-home-")
env = {**os.environ, "HOME": home, "XDG_CONFIG_HOME": f"{home}/.config", "XDG_DATA_HOME": f"{home}/.local/share",
       "XDG_CACHE_HOME": f"{home}/.cache", "ARCADE_HOME": f"{home}/arcade-link", "APPIMAGE_EXTRACT_AND_RUN": "1"}
subprocess.run([app, "--install", "--silent", "--no-launch"], env=env, timeout=120, capture_output=True)
proc = subprocess.Popen([app, show], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
problems = []


def status_now():
    try:
        result = subprocess.run([app, "--status"], env=env, capture_output=True, text=True, timeout=30)
    except subprocess.TimeoutExpired as error:
        return None, subprocess.CompletedProcess(error.cmd, "timed out after 30 s", stdout=error.stdout or "")
    try:
        return json.loads(result.stdout.strip().splitlines()[0]), result
    except (json.JSONDecodeError, IndexError):
        return None, result


# A first start can take a while (an app may unpack itself before its resident starts, and its launcher may exit 0
# once it has handed over), so wait for --status to say running rather than sleep a fixed time.
deadline = time.monotonic() + 45
st, status = status_now()
while not (st or {}).get("running") and time.monotonic() < deadline:
    if proc.poll() not in (None, 0):
        break
    time.sleep(0.5)
    st, status = status_now()
if proc.poll() not in (None, 0):
    problems.append(f"exited {proc.returncode}: {proc.stdout.read().decode(errors='replace')[-800:]}")
if st is None:
    problems.append(f"--status gave {status.stdout!r} (exit {status.returncode})")
elif not st.get("running", False):
    problems.append(f"--status says not running after 45 s: {st}")
# Stays up: still running a few seconds after it first said so.
time.sleep(5)
st, status = status_now()
if st is not None and not st.get("running", False):
    problems.append(f"stopped within 5 s of starting: {st}")
try:
    import dbus
    w = dbus.SessionBus().get_object("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher")
    items = w.Get("org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems", dbus_interface="org.freedesktop.DBus.Properties")
    tray = len(items)
except Exception as e:  # the watcher is part of the session; report, don't hide
    tray = None
    problems.append(f"tray watcher unreadable: {e}")
cores = list(Path("/var/lib/systemd/coredump").glob("*")) if Path("/var/lib/systemd/coredump").exists() else []
if cores:
    problems.append(f"coredumps: {cores}")
subprocess.run([app, "--quit"], env=env, timeout=30, capture_output=True)
try:
    proc.wait(15)
except subprocess.TimeoutExpired:
    proc.kill()
    problems.append("did not quit within 15 s")
shutil.rmtree(home, ignore_errors=True)
print(json.dumps({"desktop": desktop, "app": Path(app).name, "status": st, "tray_items": tray, "problems": problems}))
sys.exit(1 if problems else 0)
