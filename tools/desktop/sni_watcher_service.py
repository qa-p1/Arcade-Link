#!/usr/bin/env python3
"""Runs Link's real StatusNotifierWatcher (tools/sni_watcher.py) as a service
in the harness session, so apps can register tray items."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from gi.repository import GLib  # noqa: E402

import sni_watcher  # noqa: E402

watcher = sni_watcher.Watcher()
print("StatusNotifierWatcher ready", flush=True)
GLib.MainLoop().run()
