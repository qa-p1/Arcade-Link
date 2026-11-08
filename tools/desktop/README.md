# Desktop harness

Real-desktop checks without a human (`specs/LINK.md` §1). Two reusable workflows:

| Workflow | Desktop | Use it for | Blocks a release on |
|---|---|---|---|
| `desktop-e2e.yml` | Tier 1: headless Hyprland 0.56 (`tools/desktop/session.sh`) | popups, Escape, tray, global shortcuts (press and release), drag and drop, multi-monitor, clipboard | any failure |
| `desktop-tier2.yml` | Tier 2: sway, KWin, Mutter, X11 (`tools/desktop/tier2.sh`) | start, `--status`, tray or fallback, quit, no coredump | a crash or data loss |

## Calling it from an app

```yaml
jobs:
  build: …            # uploads the AppImage as artifact "release-linux"
  popups:
    needs: build
    uses: qa-p1/Arcade-Link/.github/workflows/desktop-e2e.yml@v1.0.2
    with:
      test: tests/desktop/popups.py      # in the app repo; "link:<path>" for a script in Link
      args: '"$APP"'                     # $APP is the AppImage
      artifact: release-linux
  tier2:
    needs: build
    uses: qa-p1/Arcade-Link/.github/workflows/desktop-tier2.yml@v1.0.2
    with: { artifact: release-linux, show: --show }
```

A test that must reproduce a known bug passes `expect-exit: 10` and exits 10 when the rule is broken, so a harness
failure can never count as the bug being reproduced.

## Writing a test

```python
import sys; sys.path.insert(0, ".arcade-link/tools/desktop")
from arcade_desktop import Desktop
d = Desktop()
w = d.wait_window(cls="dev.arcade.Look")
d.send_shortcut(w["address"], "CTRL", "f")       # key press and release to a window
d.key_state(None, "SUPER", "w", "down")         # hold…
d.key_state(None, "SUPER", "w", "up")           # …and release
d.pointer_move(*d.outside_of(w))                # real motion: enter/leave, follow_mouse
d.drag((100, 100), (900, 400))                  # press, move, release
d.global_shortcut("wheel.open", hold=0.5)       # portal Activated … Deactivated
d.tray_click("Look", "Quit Arcade Look")        # dbusmenu
d.screenshot("after-escape")                    # uploaded as an artifact
```

`session.sh` gives each run a private D-Bus, a StatusNotifierWatcher, the GlobalShortcuts portal stand-in
(`portal_shortcuts.py`), two 1920×1080 outputs (`HEADLESS-1`, `HEADLESS-2`) and `input:follow_mouse = 1`.
Hyprland's own log and any crash report land in the artifacts.

Proof of the harness itself: `.github/workflows/desktop-harness.yml` (driver self-test; the popup rule on a fixed and
a broken stand-in; C0 reproduced on the released Clipboard 0.3.2).
