#!/usr/bin/env python3
"""Tier 2 smoke: smoke.py <AppImage> [--show ARG] — the app starts on this
desktop, stays up, answers --status, registers a tray item (or says why not),
and leaves no coredump. Window placement is Tier 1 (Hyprland) only."""
import json
import os
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
time.sleep(8)
problems = []
if proc.poll() is not None and proc.returncode != 0:
    problems.append(f"exited {proc.returncode}: {proc.stdout.read().decode(errors='replace')[-800:]}")
status = subprocess.run([app, "--status"], env=env, capture_output=True, text=True, timeout=30)
try:
    st = json.loads(status.stdout.strip().splitlines()[0])
except (json.JSONDecodeError, IndexError):
    st = None
    problems.append(f"--status gave {status.stdout!r} (exit {status.returncode})")
if st is not None and not st.get("running", False):
    problems.append(f"--status says not running: {st}")
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
print(json.dumps({"desktop": desktop, "app": Path(app).name, "status": st, "tray_items": tray, "problems": problems}))
sys.exit(1 if problems else 0)
