#!/usr/bin/env python3
"""A minimal GlobalShortcuts portal client for the self-test: names itself through the host registry, binds one
shortcut, prints "bound <trigger>", then prints "activated <id>" / "deactivated <id>" for each signal it receives."""
import sys

import dbus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

APP_ID, SHORTCUT = sys.argv[1], sys.argv[2]
PORTAL = "org.freedesktop.portal.GlobalShortcuts"

DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
obj = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
dbus.Interface(obj, "org.freedesktop.host.portal.Registry").Register(APP_ID, {})
portal = dbus.Interface(obj, PORTAL)
loop = GLib.MainLoop()
session = None


def on_response(response, results, path=None):
    global session
    if "session_handle" in results:
        session = str(results["session_handle"])
        portal.BindShortcuts(session, [(SHORTCUT, {"description": "harness self-test",
                                                   "preferred_trigger": "SUPER+F12"})], "", {"handle_token": "bind"})
    elif "shortcuts" in results:
        print("bound " + ", ".join(f"{s}={p.get('trigger_description', '')}" for s, p in results["shortcuts"]), flush=True)


def on_signal(kind):
    return lambda s, sid, ts, opts: print(f"{kind} {sid}", flush=True) if str(s) == session else None


bus.add_signal_receiver(on_response, "Response", "org.freedesktop.portal.Request", path_keyword="path")
bus.add_signal_receiver(on_signal("activated"), "Activated", PORTAL)
bus.add_signal_receiver(on_signal("deactivated"), "Deactivated", PORTAL)
portal.CreateSession({"handle_token": "create", "session_handle_token": "selftest"})
loop.run()
