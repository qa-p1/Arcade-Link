#!/usr/bin/env python3
"""Harness self-test: every driver helper against a real client (foot)."""
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from arcade_desktop import Desktop  # noqa: E402

d = Desktop()
mons = d.monitors()
assert [m["name"] for m in mons] == ["HEADLESS-1", "HEADLESS-2"], mons
term = subprocess.Popen(["foot", "--app-id", "harness.term", "--title", "harness", "sh", "-c", "cat > \"$0\"", str(Path.cwd() / "typed.txt")])
w = d.wait_window(cls="harness.term")
d.focus(w["address"])
for key in "hi":
    d.send_shortcut(w["address"], "", key)
d.send_shortcut(w["address"], "", "Return")
d.wait(lambda: Path("typed.txt").exists() and Path("typed.txt").read_text() == "hi\n", 5, "keys typed into the terminal")

cx, cy = d.center_of(w)
d.pointer_move(cx, cy)
assert d.cursor() == (cx, cy), d.cursor()
d.pointer_move(2500, 500)
assert d.cursor() == (2500, 500), d.cursor()
d.click(cx, cy)

d.clipboard_set("arcade harness")
assert d.clipboard_get() == "arcade harness"
shot = d.screenshot("selftest")
assert shot.stat().st_size > 10_000, shot.stat().st_size
assert isinstance(d.layers(), list)
assert d.bound_shortcuts() == []
assert d.registered_apps() == []
registry = subprocess.run([sys.executable, "-c", (
    "import dbus; o = dbus.SessionBus().get_object('org.freedesktop.portal.Desktop', '/org/freedesktop/portal/desktop');"
    "dbus.Interface(o, 'org.freedesktop.host.portal.Registry').Register('harness.term', {})")], capture_output=True, text=True)
assert registry.returncode == 0, registry.stderr
assert d.registered_apps() == ["harness.term"], d.registered_apps()
print(f"harness ok: {len(mons)} outputs, keys, pointer, clipboard, screenshot ({shot.stat().st_size} bytes)")
term.terminate()
term.wait(5)
