"""A real StatusNotifierWatcher for isolated e2e checks, like each app's
tests/tray-registration.py. Import it only inside a check, which runs on the
session's private D-Bus. Like Caelestia's tray, it lists Passive items too, so
an app must remove its item, not hide it.
"""
import time

import dbus
import dbus.bus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

WATCHER = "org.kde.StatusNotifierWatcher"
PROPERTIES = "org.freedesktop.DBus.Properties"
ACTIVE_WATCHERS = 0


class Watcher(dbus.service.Object):
    def __init__(self):
        global ACTIVE_WATCHERS
        self.bus = dbus.SessionBus(private=True, mainloop=DBusGMainLoop())
        assert self.bus.request_name(WATCHER, dbus.bus.NAME_FLAG_DO_NOT_QUEUE) == dbus.bus.REQUEST_NAME_REPLY_PRIMARY_OWNER
        super().__init__(self.bus, "/StatusNotifierWatcher")
        self.items = {}
        self.bus.add_signal_receiver(self.owner_changed, signal_name="NameOwnerChanged",
                                     dbus_interface="org.freedesktop.DBus")
        ACTIVE_WATCHERS += 1

    @dbus.service.method(WATCHER, in_signature="s", out_signature="", sender_keyword="sender")
    def RegisterStatusNotifierItem(self, service, sender=None):
        name, path = (str(sender), str(service)) if service.startswith("/") else (str(service), "/StatusNotifierItem")
        # libappindicator registers an object path, so its owner is the sender.
        owner = name if name.startswith(":") else str(self.bus.get_name_owner(name))
        self.items[name + path] = (name, path, owner)
        self.StatusNotifierItemRegistered(name + path)

    @dbus.service.method(WATCHER, in_signature="s", out_signature="")
    def RegisterStatusNotifierHost(self, _service):
        pass

    @dbus.service.signal(WATCHER, signature="s")
    def StatusNotifierItemRegistered(self, _item):
        pass

    @dbus.service.signal(WATCHER, signature="s")
    def StatusNotifierItemUnregistered(self, _item):
        pass

    def owner_changed(self, name, _old, new):
        for key, (service, _path, owner) in list(self.items.items()):
            if str(name) in (service, owner) and str(new) != owner:
                del self.items[key]
                self.StatusNotifierItemUnregistered(key)

    def properties(self):
        return {"RegisteredStatusNotifierItems": dbus.Array(sorted(self.items), signature="s"),
                "IsStatusNotifierHostRegistered": dbus.Boolean(True), "ProtocolVersion": dbus.Int32(0)}

    @dbus.service.method(PROPERTIES, in_signature="ss", out_signature="v")
    def Get(self, interface, name):
        assert interface == WATCHER
        return self.properties()[str(name)]

    @dbus.service.method(PROPERTIES, in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        assert interface == WATCHER
        return self.properties()

    def owner_pids(self):
        """The process ID behind every registered item."""
        pids = []
        for _name, _path, owner in list(self.items.values()):
            try:
                pids.append(int(self.bus.call_blocking(
                    "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                    "GetConnectionUnixProcessID", "s", (owner,))))
            except dbus.DBusException:
                pass  # the owner left meanwhile; owner_changed drops it
        return pids

    def close(self):
        global ACTIVE_WATCHERS
        self.bus.remove_signal_receiver(self.owner_changed, signal_name="NameOwnerChanged",
                                        dbus_interface="org.freedesktop.DBus")
        self.remove_from_connection()
        self.bus.release_name(WATCHER)
        self.bus.flush()
        self.bus.close()
        ACTIVE_WATCHERS -= 1


def pump():
    if not ACTIVE_WATCHERS:
        return
    context = GLib.MainContext.default()
    while context.pending():
        context.iteration(False)


def until(predicate, message, timeout=15):
    """Like real_apps.wait, but serves the watcher while it waits: apps call
    RegisterStatusNotifierItem synchronously."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        pump()
        if result := predicate():
            return result
        time.sleep(.01)
    raise AssertionError(message() if callable(message) else message)
