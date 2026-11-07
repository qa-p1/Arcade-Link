#!/usr/bin/env python3
"""UI-1 (fleet popup rule) and the C0 proof (specs/LINK.md §1.4).

    popup_survives_pointer_leave.py clipboard <AppImage>   the real app's picker
    popup_survives_pointer_leave.py stub [--close-on-blur] the stand-in, fixed or with the bug

Opens the popup, puts the pointer inside it, then moves the pointer (real
motion, follow_mouse = 1) into another window and waits. Passes only if the
popup is still mapped. Exit 10 means the popup closed on pointer leave (the
rule is broken); any other failure is the harness's.
"""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from arcade_desktop import Desktop  # noqa: E402

d = Desktop()
other = subprocess.Popen(["foot", "--app-id", "harness.other", "--title", "other window"])
other_w = d.wait_window(cls="harness.other")
before = {c["address"] for c in d.clients()}

mode = sys.argv[1]
if mode == "clipboard":
    app = sys.argv[2]
    home = tempfile.mkdtemp(prefix="c0-home-")
    env = {**os.environ, "HOME": home, "XDG_CONFIG_HOME": f"{home}/.config", "XDG_DATA_HOME": f"{home}/.local/share",
           "XDG_CACHE_HOME": f"{home}/.cache", "ARCADE_HOME": f"{home}/arcade-link"}
    subprocess.run([app, "--install", "--silent", "--no-launch"], env=env, timeout=60)
    installed = next(Path(home).rglob("*Clipboard*.AppImage"), Path(app))
    subprocess.Popen([str(installed), "--background"], env=env)
    time.sleep(4)
    subprocess.Popen([str(installed), "--overlay"], env=env)
else:
    args = [sys.executable, str(Path(__file__).with_name("stub_popup.py"))] + sys.argv[2:]
    subprocess.Popen(args)

popup = d.wait(lambda: next((c for c in d.clients() if c["address"] not in before), None), 20, "the popup window")
d.screenshot(f"{mode}-popup-open")
print(f"popup: class={popup['class']} title={popup['title']!r} at={popup['at']} size={popup['size']} floating={popup['floating']}")
time.sleep(0.6)
px, py = d.center_of(popup)
d.pointer_move(px, py)
time.sleep(0.6)
ox, oy = d.center_of(other_w)
d.pointer_move(ox, oy)
time.sleep(1.5)
still = any(c["address"] == popup["address"] for c in d.clients())
d.screenshot(f"{mode}-after-pointer-leave")
other.terminate()
if not still:
    print("FAIL: the popup closed when the pointer left it (fleet rule UI-1)")
    sys.exit(10)
print("PASS: the popup stayed open after the pointer left it")
d.escape(popup["address"])
