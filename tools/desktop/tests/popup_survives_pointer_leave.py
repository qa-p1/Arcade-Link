#!/usr/bin/env python3
"""UI-1 (fleet popup rule) and the C0 proof (specs/LINK.md §1.4).

    popup_survives_pointer_leave.py clipboard <AppImage>   the real app's picker
    popup_survives_pointer_leave.py stub [--close-on-blur] the stand-in, fixed or with the bug

Opens the popup, puts the pointer inside it, then moves the pointer (real
motion, follow_mouse = 1) into another window and waits. Passes only if the
popup is still mapped. Exit 10 means the popup closed on pointer leave (the
rule is broken); any other failure is the harness's.
"""
import json
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
    # One process: "--overlay" with nothing running starts the app and opens the
    # picker (the same picker and blur handler as the Ctrl+Shift+Space path),
    # without depending on the D-Bus hand-off's timing.
    log = open(d.artifact("clipboard.log"), "wb")
    # A fresh profile opens on first-run setup; the picker needs an identity.
    # Set it up as a user would: the "Start on this device" card, then the
    # form's primary "Start on this device" button.
    subprocess.Popen([str(installed)], env=env, stdout=log, stderr=subprocess.STDOUT)
    main = d.wait_window(cls="dev.arcade.Clipboard", timeout=60)
    time.sleep(12)  # first frames under software GL
    d.screenshot("clipboard-setup")
    main = d.find_window(cls="dev.arcade.Clipboard")
    cx, cy = d.center_of(main)
    d.click(cx - 146, cy + 26)  # the card, left of centre (welcome.dart layout)
    time.sleep(3)
    d.screenshot("clipboard-setup-form")
    d.click(cx + 209, cy + 178)  # the form's primary "Start on this device" button (cy + 147 hit the switch row)
    time.sleep(8)
    d.screenshot("clipboard-setup-done")
    # Ask the running app for the picker (the Ctrl+Shift+Space command).
    subprocess.Popen([str(installed), "--overlay"], env=env, stdout=log, stderr=subprocess.STDOUT)
    timeout = 60
    # The picker is the window it re-maps under this title (desktop_adapter.dart).
    is_popup = lambda c: c.get("title") == "Arcade Clipboard Picker"
else:
    args = [sys.executable, str(Path(__file__).with_name("stub_popup.py"))] + sys.argv[2:]
    subprocess.Popen(args)
    timeout = 20
    is_popup = lambda c: c["address"] not in before

def still_mapped() -> bool:
    # Same window and still the popup: Clipboard turns its one window back into
    # the History window when the picker closes, keeping the address.
    return any(c["address"] == popup["address"] and is_popup(c) for c in d.clients())


popup = d.wait(lambda: next((c for c in d.clients() if is_popup(c)), None), timeout, "the popup window")
d.screenshot(f"{mode}-popup-open")
print(f"popup: class={popup['class']} title={popup['title']!r} at={popup['at']} size={popup['size']} floating={popup['floating']}")
time.sleep(0.6)
px, py = d.center_of(popup)
d.pointer_move(px, py)
time.sleep(0.6)
# Re-read: tiling the popup beside it has resized and moved the other window.
other_w = d.find_window(cls="harness.other")
ox, oy = d.center_of(other_w)
d.pointer_move(ox, oy)
time.sleep(1.5)
active = json.loads(d.hyprctl("-j", "activewindow") or "{}")
print(f"after leave: cursor={d.cursor()} other={other_w['at']}+{other_w['size']} focused={active.get('class')!r} {active.get('title')!r}")
if active.get("address") == popup["address"] and still_mapped():
    sys.exit("harness error: focus never left the popup, so the rule was not exercised")
still = still_mapped()
d.screenshot(f"{mode}-after-pointer-leave")
other.terminate()
if not still:
    print("FAIL: the popup closed when the pointer left it (fleet rule UI-1)")
    sys.exit(10)
print("PASS: the popup stayed open after the pointer left it")
d.escape(popup["address"])
