# e2e check modules

Each `*.py` here is loaded by `tools/e2e.py`. Names injected into the module:
`check(group)` (decorator), `Session`, `APPS`, `CLI`. A check takes the
`Session` and returns an optional detail string; raise `AssertionError` to fail.

```python
@check("look")
def inspect_reports_dimensions(s):
    s.start("arcade.look")
    r = s.cli("invoke", "look", "look.inspect", "--file", str(png), "--json", check=True)
    assert '"width": 2' in r.stdout, r.stdout
```

Run one group: `python3 tools/e2e.py --only look`.

`s.screenshot("look-strip")` saves the virtual screen as a PNG in
`$ARCADE_E2E_SHOTS` (default: the run's temporary root); pass an X window id
from `s.wait_window(...)` to capture one window.

Use `--real-apps` for the Tools plan §17.2 matrix. Without that flag the four
new group names retain the Phase 1 mock/harness contract checks. A full real run
starts a fresh private D-Bus/Xvfb/profile for each group and prints pass/fail
counts. `ARCADE_E2E_REPORT_DIR` writes one JSON result per selected group.

All eight checkouts accept `ARCADE_<APP>_REPO` and `ARCADE_<APP>_BIN`, where APP
is BOX, LENS, LOOK, WHEEL, SHELF, FIND, CLIPBOARD or TOOLS. A binary override can
be relative to its checkout or absolute. Defaults preserve the sibling main
checkout layout (Shelf is under `C++`). Set `ARCADE_BOX_CLI_BIN` for the headless
Box CLI and `ARCADE_OVERLAY_BIN` for Tools' native overlay. Install checks accept
`ARCADE_<APP>_APPIMAGE`, otherwise selecting the newest package under `dist/`
or a Tauri release bundle directory. Real install groups require ELF AppImages;
shell wrappers and mocks are rejected.

The runner never changes or rebuilds app sources. Build the named artifacts
first; binaries/packages older than relevant source inputs are
reported as **stale or missing build**. The Tools adapter is generated/built
under Link's ignored `target/`, using the selected Tools checkout's actual core
and shortcut libraries as path dependencies. It executes explicit manager
operations in the isolated profile; it does not test GUI confirmation dialogs.
The existing `tools` group retains native GUI confirmation coverage.

Example for the hub worktrees:

```sh
export ARCADE_FIND_REPO="$PWD/../Arcade-Find-wt-hub"
export ARCADE_SHELF_REPO="$PWD/../../C++/Arcade-Shelf-wt-hub"
# Set the other ARCADE_<APP>_REPO values similarly; see tools/hub-env.sh.
. tools/hub-env.sh
python3 tools/e2e.py --real-apps
python3 tools/e2e.py --real-apps --only trayhost
```

`tools/hub-env.sh` is a command-scoped convenience: source it in a task shell;
never source it from login profiles or persistent environment configuration.
