#!/usr/bin/env python3
"""A stand-in xdg-desktop-portal GlobalShortcuts backend for the harness.

Apps bind their shortcuts through it exactly as through the real portal. The
driver then presses and releases a shortcut with dev.arcade.TestPortal.Press /
Release, which emit Activated and Deactivated (the real `hl.dsp.global` sends
Activated only, so hold-to-use shortcuts can't be tested through it).

Under Hyprland it also does what xdg-desktop-portal-hyprland does for a bound
shortcut: registers it with the compositor (hyprland_global_shortcuts_manager_v1,
through the `gshortcuts` helper built by build-vpointer.sh) as
"<app id>:<shortcut id>", so `hyprctl globalshortcuts` lists it, and forwards the
compositor's pressed/released events as Activated/Deactivated. The app ID is the
one the caller registered (empty if it did not, as with the real portal).

Like xdg-desktop-portal 1.19 and later, it also serves the host app registry
(`org.freedesktop.host.portal.Registry.Register`), which unsandboxed apps call
first to name their desktop-file ID; dev.arcade.TestPortal.Registered lists them.
"""
import os
import select
import subprocess
import time
from pathlib import Path

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


class Hyprland:
    """The gshortcuts helper: one Wayland client holding every registered shortcut."""

    def __init__(self, on_event):
        helper = Path(__file__).with_name("gshortcuts")
        self.proc = subprocess.Popen([str(helper)], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.fd = self.proc.stdout.fileno()
        self.buf = b""
        self.on_event = on_event
        self.registered = set()  # (app id, shortcut id)
        GLib.io_add_watch(self.fd, GLib.IO_IN | GLib.IO_HUP, self._readable)

    def _drain(self):
        """Handles the complete lines read so far; returns how many were "ok"."""
        oks = 0
        while b"\n" in self.buf:
            line, self.buf = self.buf.split(b"\n", 1)
            words = line.decode().split(" ", 1)
            if words[0] == "ok":
                oks += 1
            elif words[0] in ("pressed", "released") and len(words) == 2:
                self.on_event(words[0], words[1])
        return oks

    def _readable(self, fd, condition):
        data = os.read(fd, 4096)
        if not data:
            print(f"gshortcuts exited with {self.proc.wait()}", flush=True)
            return False
        self.buf += data
        self._drain()
        return True

    def register(self, app_id, sid, description, trigger):
        if (app_id, sid) in self.registered:  # a second registration is a protocol error (already_taken)
            return
        clean = lambda v: str(v).replace("\t", " ").replace("\n", " ")
        self.proc.stdin.write(("\t".join(["register", clean(sid), clean(app_id), clean(description), clean(trigger)]) + "\n").encode())
        self.proc.stdin.flush()
        # Before BindShortcuts answers, as with xdph: the app checks the compositor's list right after.
        deadline = time.monotonic() + 5
        while not self._drain():
            left = deadline - time.monotonic()
            if left <= 0 or not select.select([self.fd], [], [], left)[0]:
                raise dbus.exceptions.DBusException("Hyprland did not confirm the shortcut", name="org.freedesktop.portal.Error.Failed")
            data = os.read(self.fd, 4096)
            if not data:
                raise dbus.exceptions.DBusException(f"gshortcuts exited with {self.proc.wait()}", name="org.freedesktop.portal.Error.Failed")
            self.buf += data
        self.registered.add((app_id, sid))
        print(f"registered {app_id}:{sid} with Hyprland", flush=True)


class Request(dbus.service.Object):
    @dbus.service.signal(REQUEST, signature="ua{sv}")
    def Response(self, response, results):
        pass


class Session(dbus.service.Object):
    @dbus.service.method(SESSION, in_signature="", out_signature="")
    def Close(self):
        portal.sessions.pop(self.__dbus_object_path__, None)
        portal.owners.pop(self.__dbus_object_path__, None)


class Portal(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, PATH)
        self.bus = bus
        self.sessions = {}  # session path -> {shortcut id: description}
        self.registered = {}  # bus name -> desktop-file ID
        self.owners = {}  # session path -> bus name
        self.n = 0
        self.hyprland = Hyprland(self._compositor_event) if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") else None

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
        self.owners[path] = sender
        return self._request(sender, options, {"session_handle": dbus.String(path)})

    @dbus.service.method(PORTAL, in_signature="oa(sa{sv})sa{sv}", out_signature="o", sender_keyword="sender")
    def BindShortcuts(self, session, shortcuts, parent_window, options, sender=None):
        bound = self.sessions.setdefault(str(session), {})
        for sid, props in shortcuts:
            bound[str(sid)] = dict(props)
            if self.hyprland:
                self.hyprland.register(self.registered.get(sender, ""), str(sid), props.get("description", ""),
                                       props.get("preferred_trigger", ""))
        print(f"bound {sorted(bound)} for {session}", flush=True)
        return self._request(sender, options, {"shortcuts": self._results(bound)})

    @staticmethod
    def _results(bound):
        """What the portal reports for bound shortcuts: the description and the trigger it assigned. Real portals
        always send trigger_description (ashpd, which Box uses, refuses a result without it); the stand-in assigns
        the app's preferred trigger, or none."""
        return dbus.Array([
            dbus.Struct((dbus.String(k), dbus.Dictionary({
                "description": dbus.String(str(v.get("description", ""))),
                "trigger_description": dbus.String(str(v.get("preferred_trigger", ""))),
            }, signature="sv")))
            for k, v in bound.items()], signature="(sa{sv})")

    @dbus.service.method(PORTAL, in_signature="oa{sv}", out_signature="o", sender_keyword="sender")
    def ListShortcuts(self, session, options, sender=None):
        bound = self.sessions.get(str(session), {})
        return self._request(sender, options, {"shortcuts": self._results(bound)})

    @dbus.service.signal(PORTAL, signature="osta{sv}")
    def Activated(self, session, shortcut, timestamp, options):
        pass

    @dbus.service.signal(PORTAL, signature="osta{sv}")
    def Deactivated(self, session, shortcut, timestamp, options):
        pass

    def _compositor_event(self, kind, target):
        """Hyprland pressed or released "<app id>:<shortcut id>": signal the sessions of that app that bound it."""
        app_id, _, sid = target.partition(":")  # desktop-file IDs have no colon
        signal = self.Activated if kind == "pressed" else self.Deactivated
        hits = self._emit(sid, signal, lambda s: self.registered.get(self.owners.get(s), "") == app_id)
        print(f"Hyprland {kind} {target}: {int(hits)} session(s)", flush=True)

    def _emit(self, shortcut, signal, owner_matches=lambda s: True):
        hits = [s for s, ids in self.sessions.items() if shortcut in ids and owner_matches(s)]
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
