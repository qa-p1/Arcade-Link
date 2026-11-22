# arcade-helper

Arcade's actions everywhere: in the file manager's right-click menu and in the browser extension, without opening
the apps first. Arcade Tools ships it, starts `arcade-helper --background` at login and runs
`arcade-helper integrate --remove` before uninstalling it. See Link's SPEC §5.5 (offers) and §5.6 (the helper).

| command | what it does |
|---|---|
| `arcade-helper --background` | the resident helper: runs jobs, keeps the menu entries in step |
| `arcade-helper menu <files…>` | the picker ("Arcade…" in the menus) |
| `arcade-helper run <app/action> <files…>` | a pinned menu entry |
| `arcade-helper progress` | the progress window (the resident starts it) |
| `arcade-helper offers [--surface S] <files…>` | the offers for these files, as JSON |
| `arcade-helper integrate [--remove]` | write (or remove) the menu entries |

Started by a browser with an extension origin (`chrome-extension://…`) or a manifest path and add-on ID (Firefox),
it serves the extension over native messaging instead (`bridge` module).

Modules: `resident` (jobs), `jobs` (requests and placing results), `integrations` (menus and browser host
manifests), `pins` (which actions get their own menu entry), `bridge` and `native` (the extension), `ui` (the
windows), `client` (talking to the resident), `mime` and `ids`.
