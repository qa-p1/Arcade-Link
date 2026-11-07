#!/usr/bin/env python3
"""A stand-in popup for the harness proof: a small floating GTK window that,
with --close-on-blur, hides 400 ms after losing focus (Clipboard's C0 bug)
and otherwise closes only on Escape."""
import sys

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

close_on_blur = "--close-on-blur" in sys.argv
GLib.set_prgname("dev.arcade.Stub")
w = Gtk.Window(title="Stub picker")
w.set_default_size(480, 520)
w.set_type_hint(1)  # DIALOG: Hyprland floats it
w.add(Gtk.Label(label="Stub picker (close on blur)" if close_on_blur else "Stub picker"))
w.connect("key-press-event", lambda _w, e: Gtk.main_quit() if e.keyval == 0xFF1B else None)
if close_on_blur:
    w.connect("focus-out-event", lambda *_: GLib.timeout_add(400, Gtk.main_quit) and False)
w.connect("destroy", Gtk.main_quit)
w.show_all()
Gtk.main()
