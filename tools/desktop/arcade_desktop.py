"""Drive the headless Hyprland harness (`session.sh`) from a test script.

Every helper talks to the real compositor or the session's D-Bus:
`hyprctl` for windows, layers, keys and the pointer position; `vpointer` (the
wlr virtual-pointer protocol) for motion, clicks and drags; `grim` for screenshots;
`wl-copy`/`wl-paste` for the clipboard; dbusmenu for tray menus; and the
GlobalShortcuts portal stand-in for shortcut press and release.

    from arcade_desktop import Desktop
    d = Desktop()
    w = d.wait_window(cls="dev.arcade.Clipboard")
    d.send_shortcut(w["address"], "CTRL", "v")
"""
from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

ARTIFACTS = Path(os.environ.get("ARCADE_DESKTOP_ARTIFACTS", "desktop-artifacts"))


class DesktopError(AssertionError):
    pass


def _run(*args: str, timeout: float = 10, input: bytes | None = None) -> str:
    r = subprocess.run(args, capture_output=True, timeout=timeout, input=input)
    if r.returncode != 0:
        raise DesktopError(f"{' '.join(args)}: exit {r.returncode}: {r.stderr.decode(errors='replace').strip()}")
    return r.stdout.decode(errors="replace")


def _lua(value: str) -> str:
    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"


class Desktop:
    # ----------------------------------------------------------- compositor
    def hyprctl(self, *args: str) -> str:
        return _run("hyprctl", *args)

    def dispatch(self, lua: str) -> None:
        out = self.hyprctl("dispatch", lua).strip()
        if out != "ok":
            raise DesktopError(f"dispatch {lua}: {out}")

    def clients(self) -> list[dict]:
        return json.loads(self.hyprctl("-j", "clients"))

    def layers(self) -> list[dict]:
        """Flattened layer surfaces: [{monitor, level, namespace, x, y, w, h, address}]."""
        out = []
        for monitor, data in json.loads(self.hyprctl("-j", "layers")).items():
            for level, surfaces in data.get("levels", {}).items():
                for s in surfaces:
                    out.append({"monitor": monitor, "level": int(level), **s})
        return out

    def monitors(self) -> list[dict]:
        return json.loads(self.hyprctl("-j", "monitors"))

    def wait(self, predicate, timeout: float = 10, what: str = "condition"):
        end = time.time() + timeout
        while time.time() < end:
            value = predicate()
            if value:
                return value
            time.sleep(0.05)
        self.screenshot(f"timeout-{what.replace(' ', '-')}")
        raise DesktopError(f"timed out after {timeout} s waiting for {what}")

    def find_window(self, cls: str | None = None, title: str | None = None) -> dict | None:
        for c in self.clients():
            if (cls is None or c.get("class") == cls or c.get("initialClass") == cls) and (title is None or title in c.get("title", "")):
                return c
        return None

    def wait_window(self, cls: str | None = None, title: str | None = None, timeout: float = 15) -> dict:
        return self.wait(lambda: self.find_window(cls, title), timeout, f"window {cls or ''} {title or ''}".strip())

    def wait_gone(self, cls: str | None = None, title: str | None = None, timeout: float = 10) -> None:
        self.wait(lambda: self.find_window(cls, title) is None, timeout, f"window {cls or title} to close")

    def find_layer(self, namespace: str) -> dict | None:
        return next((l for l in self.layers() if l.get("namespace") == namespace), None)

    def focus(self, address: str) -> None:
        self.dispatch(f"hl.dsp.focus({{ window = {_lua('address:' + address)} }})")

    # ----------------------------------------------------------- keyboard
    def send_shortcut(self, window_address: str | None, mods: str, key: str) -> None:
        """A key press and release with modifiers (e.g. "CTRL SHIFT", "v"), to
        one window or, without an address, the focused one."""
        win = f", window = {_lua('address:' + window_address)}" if window_address else ""
        self.dispatch(f"hl.dsp.send_shortcut({{ mods = {_lua(mods)}, key = {_lua(key)}{win} }})")

    def key_state(self, window_address: str | None, mods: str, key: str, state: str) -> None:
        """Press (`down`) or release (`up`) a key separately: holds."""
        win = f", window = {_lua('address:' + window_address)}" if window_address else ""
        self.dispatch(f"hl.dsp.send_key_state({{ mods = {_lua(mods)}, key = {_lua(key)}, state = {_lua(state)}{win} }})")

    def escape(self, window_address: str | None = None) -> None:
        self.send_shortcut(window_address, "", "Escape")

    # ----------------------------------------------------------- pointer
    _vpointer: subprocess.Popen | None = None

    def _pointer(self, command: str) -> None:
        """One command to the virtual pointer (real motion and button events)."""
        if self._vpointer is None or self._vpointer.poll() is not None:
            mons = self.monitors()
            width = max(m["x"] + m["width"] for m in mons)
            height = max(m["y"] + m["height"] for m in mons)
            self._vpointer = subprocess.Popen([str(Path(__file__).with_name("vpointer")), str(width), str(height)],
                                              stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert self._vpointer.stdin and self._vpointer.stdout
        self._vpointer.stdin.write(command + "\n")
        self._vpointer.stdin.flush()
        if self._vpointer.stdout.readline().strip() != "ok":
            raise DesktopError(f"virtual pointer refused {command!r}")

    def pointer_move(self, x: int, y: int, step: int = 24) -> None:
        """Glide to (x, y) like a real mouse: relative motion in small steps
        (enter/leave on every edge crossed), then an exact absolute fix-up.
        A cursor that does not arrive is a harness error, never a test result."""
        x, y = int(x), int(y)
        cx, cy = self.cursor()
        n = max(1, max(abs(x - cx), abs(y - cy)) // step)
        done_x = done_y = 0
        for i in range(1, n + 1):
            tx, ty = (x - cx) * i // n, (y - cy) * i // n
            self._pointer(f"rel {tx - done_x} {ty - done_y}")
            done_x, done_y = tx, ty
            time.sleep(0.004)
        self._pointer(f"move {x} {y}")
        time.sleep(0.03)
        if self.cursor() != (x, y):
            self.screenshot("pointer-did-not-arrive")
            raise DesktopError(f"pointer at {self.cursor()} after moving to {(x, y)}")

    def cursor(self) -> tuple[int, int]:
        x, y = self.hyprctl("cursorpos").strip().split(",")
        return int(x), int(y)

    def click(self, x: int | None = None, y: int | None = None, button: str = "left") -> None:
        if x is not None and y is not None:
            self.pointer_move(x, y)
        self._pointer("click right" if button == "right" else "click")
        time.sleep(0.05)

    def drag(self, start: tuple[int, int], end: tuple[int, int], steps: int = 12) -> None:
        """Press at `start`, move in steps, release at `end` (drag and drop)."""
        self.pointer_move(*start)
        self._pointer("press")
        for i in range(1, steps + 1):
            self.pointer_move(start[0] + (end[0] - start[0]) * i // steps, start[1] + (end[1] - start[1]) * i // steps)
        self._pointer("release")

    def center_of(self, window: dict) -> tuple[int, int]:
        x, y = window["at"]
        w, h = window["size"]
        return x + w // 2, y + h // 2

    def outside_of(self, window: dict, margin: int = 40) -> tuple[int, int]:
        """A point on the same monitor but outside the window."""
        x, y = window["at"]
        w, h = window["size"]
        mon = next(m for m in self.monitors() if m["id"] == window["monitor"])
        right = x + w + margin
        if right < mon["x"] + mon["width"]:
            return right, y + h // 2
        return max(mon["x"] + 5, x - margin), y + h // 2

    # ----------------------------------------------------------- output
    def screenshot(self, name: str) -> Path:
        ARTIFACTS.mkdir(parents=True, exist_ok=True)
        path = ARTIFACTS / f"{int(time.time() * 1000)}-{name}.png"
        try:
            _run("grim", str(path), timeout=20)
        except DesktopError as e:
            print(f"screenshot failed: {e}")
        return path

    def artifact(self, name: str) -> Path:
        """A path in the uploaded artifacts (app logs and the like)."""
        ARTIFACTS.mkdir(parents=True, exist_ok=True)
        return ARTIFACTS / name

    # ----------------------------------------------------------- clipboard
    def clipboard_get(self, mime: str = "text/plain") -> str:
        return _run("wl-paste", "--no-newline", "--type", mime)

    def clipboard_set(self, text: str, mime: str = "text/plain") -> None:
        subprocess.Popen(["wl-copy", "--type", mime], stdin=subprocess.PIPE).communicate(text.encode(), timeout=5)

    # ----------------------------------------------------------- tray
    def tray_items(self) -> list[tuple[str, str]]:
        import dbus
        bus = dbus.SessionBus()
        watcher = bus.get_object("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher")
        items = watcher.Get("org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems", dbus_interface="org.freedesktop.DBus.Properties")
        out = []
        for item in items:
            service, _, path = str(item).partition("/")
            out.append((service, "/" + path if path else "/StatusNotifierItem"))
        return out

    def _tray_item(self, app: str):
        import dbus
        bus = dbus.SessionBus()
        for service, path in self.tray_items():
            obj = bus.get_object(service, path)
            props = dbus.Interface(obj, "org.freedesktop.DBus.Properties")
            try:
                ident = str(props.Get("org.kde.StatusNotifierItem", "Id"))
                title = str(props.Get("org.kde.StatusNotifierItem", "Title"))
            except dbus.DBusException:
                continue
            if app.lower() in ident.lower() or app.lower() in title.lower():
                return bus, service, props
        raise DesktopError(f"no tray item for {app}; items: {self.tray_items()}")

    def tray_menu(self, app: str) -> list[tuple[int, str]]:
        """The tray menu's top-level entries as (id, label); separators are ''."""
        bus, service, props = self._tray_item(app)
        menu_path = props.Get("org.kde.StatusNotifierItem", "Menu")
        menu = bus.get_object(service, menu_path)
        _, (_, _, children) = menu.GetLayout(0, 1, [], dbus_interface="com.canonical.dbusmenu")
        out = []
        for _, (cid, cprops, _) in [(None, c) for c in children]:
            label = "" if str(cprops.get("type", "")) == "separator" else str(cprops.get("label", "")).replace("_", "")
            if cprops.get("visible", True):
                out.append((int(cid), label))
        return out

    def tray_click(self, app: str, label: str) -> None:
        bus, service, props = self._tray_item(app)
        menu_path = props.Get("org.kde.StatusNotifierItem", "Menu")
        menu = bus.get_object(service, menu_path)
        for cid, text in self.tray_menu(app):
            if text == label:
                import dbus
                menu.Event(dbus.Int32(cid), "clicked", dbus.String("", variant_level=1), dbus.UInt32(int(time.time())),
                           dbus_interface="com.canonical.dbusmenu")
                return
        raise DesktopError(f"{app} tray menu has no {label!r}: {self.tray_menu(app)}")

    # ----------------------------------------------------------- shortcuts
    def _portal(self):
        import dbus
        return dbus.Interface(dbus.SessionBus().get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop"), "dev.arcade.TestPortal")

    def bound_shortcuts(self) -> list[str]:
        return [str(s) for s in self._portal().Bound()]

    def global_shortcut(self, shortcut_id: str, hold: float = 0.08) -> None:
        """Press, hold for `hold` seconds, and release a portal shortcut."""
        portal = self._portal()
        if int(portal.Press(shortcut_id)) == 0:
            raise DesktopError(f"no app bound {shortcut_id}; bound: {self.bound_shortcuts()}")
        time.sleep(hold)
        portal.Release(shortcut_id)
