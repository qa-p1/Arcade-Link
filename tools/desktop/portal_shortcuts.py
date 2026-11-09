#!/usr/bin/env python3
"""A stand-in xdg-desktop-portal GlobalShortcuts backend for the harness.

Apps bind their shortcuts through it exactly as through the real portal. The
driver then presses and releases a shortcut with dev.arcade.TestPortal.Press /
Release, which emit Activated and Deactivated (the real `hl.dsp.global` sends
Activated only, so hold-to-use shortcuts can't be tested through it).

Like xdg-desktop-portal 1.19 and later, it also serves the host app registry
(`org.freedesktop.host.portal.Registry.Register`), which unsandboxed apps call
first to name their desktop-file ID; dev.arcade.TestPortal.Registered lists them.
"""
import time

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

PORTAL = "org.freedesktop.portal.GlobalShortcuts"
REQUEST = "org.freedesktop.portal.Request"
SESSION = "org.freedesktop.portal.Session"
CONTROL = "dev.arcade.TestPortal"
REGISTRY = "org.freedesktop.host.portal.Registry"
PATH = "/org/freedesktop/portal/desktop"


class Request(dbus.service.Object):
    @dbus.service.signal(REQUEST, signature="ua{sv}")
    def Response(self, response, results):
        pass


class Session(dbus.service.Object):
    @dbus.service.method(SESSION, in_signature="", out_signature="")
    def Close(self):
        portal.sessions.pop(self.__dbus_object_path__, None)


class Portal(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, PATH)
        self.bus = bus
        self.sessions = {}  # session path -> {shortcut id: description}
        self.registered = {}  # bus name -> desktop-file ID
        self.n = 0

    def _request(self, sender, options, results):
        token = str(options.get("handle_token", f"t{self.n}"))
        self.n += 1
        path = f"/org/freedesktop/portal/desktop/request/{sender[1:].replace('.', '_')}/{token}"
        req = Request(self.bus, path)
        GLib.timeout_add(20, lambda: (req.Response(dbus.UInt32(0), results), req.remove_from_connection(), False)[2])
        return dbus.ObjectPath(path)

    @dbus.service.method(REGISTRY, in_signature="sa{sv}", out_signature="", sender_keyword="sender")
    def Register(self, app_id, options, sender=None):
        # The real portal refuses a second registration from one connection.
        if sender in self.registered:
            raise dbus.exceptions.DBusException("Connection already associated with an application ID",
                                                name="org.freedesktop.portal.Error.Failed")
        self.registered[sender] = str(app_id)
        print(f"registered {app_id} for {sender}", flush=True)

    @dbus.service.method(CONTROL, in_signature="", out_signature="as")
    def Registered(self):
        return sorted(set(self.registered.values()))

    @dbus.service.method(PORTAL, in_signature="a{sv}", out_signature="o", sender_keyword="sender")
    def CreateSession(self, options, sender=None):
        token = str(options.get("session_handle_token", f"s{self.n}"))
        path = f"/org/freedesktop/portal/desktop/session/{sender[1:].replace('.', '_')}/{token}"
        Session(self.bus, path)
        self.sessions[path] = {}
        return self._request(sender, options, {"session_handle": dbus.String(path)})

    @dbus.service.method(PORTAL, in_signature="oa(sa{sv})sa{sv}", out_signature="o", sender_keyword="sender")
    def BindShortcuts(self, session, shortcuts, parent_window, options, sender=None):
        bound = self.sessions.setdefault(str(session), {})
        for sid, props in shortcuts:
            bound[str(sid)] = dict(props)
        print(f"bound {sorted(bound)} for {session}", flush=True)
        out = dbus.Array([dbus.Struct((dbus.String(k), dbus.Dictionary(v, signature="sv"))) for k, v in bound.items()], signature="(sa{sv})")
        return self._request(sender, options, {"shortcuts": out})

    @dbus.service.method(PORTAL, in_signature="oa{sv}", out_signature="o", sender_keyword="sender")
    def ListShortcuts(self, session, options, sender=None):
        bound = self.sessions.get(str(session), {})
        out = dbus.Array([dbus.Struct((dbus.String(k), dbus.Dictionary(v, signature="sv"))) for k, v in bound.items()], signature="(sa{sv})")
        return self._request(sender, options, {"shortcuts": out})

    @dbus.service.signal(PORTAL, signature="osta{sv}")
    def Activated(self, session, shortcut, timestamp, options):
        pass

    @dbus.service.signal(PORTAL, signature="osta{sv}")
    def Deactivated(self, session, shortcut, timestamp, options):
        pass

    def _emit(self, shortcut, signal):
        hits = [s for s, ids in self.sessions.items() if shortcut in ids]
        for s in hits:
            signal(dbus.ObjectPath(s), shortcut, dbus.UInt64(int(time.time() * 1000)), dbus.Dictionary({}, signature="sv"))
        return dbus.UInt32(len(hits))

    @dbus.service.method(CONTROL, in_signature="s", out_signature="u")
    def Press(self, shortcut):
        return self._emit(shortcut, self.Activated)

    @dbus.service.method(CONTROL, in_signature="s", out_signature="u")
    def Release(self, shortcut):
        return self._emit(shortcut, self.Deactivated)

    @dbus.service.method(CONTROL, in_signature="", out_signature="as")
    def Bound(self):
        return sorted({i for ids in self.sessions.values() for i in ids})

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        if prop == "version":
            return dbus.UInt32(1)
        raise dbus.exceptions.DBusException("no such property")


DBusGMainLoop(set_as_default=True)
session_bus = dbus.SessionBus()
name = dbus.service.BusName("org.freedesktop.portal.Desktop", session_bus)
portal = Portal(session_bus)
print("GlobalShortcuts portal stand-in ready", flush=True)
GLib.MainLoop().run()
