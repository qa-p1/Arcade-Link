# Changelog

Arcade Link follows the frozen protocol v1 and manifest schema 1. Apps pin an
immutable tag; a new tag never changes the wire format.

## Unreleased

- Programs from another image (the installed copy, another Arcade app, a
  restart through the AppImage file) inherit nothing above stdio:
  `childenv::detach_from_image` (Qt: `detachFromImage`), applied by
  `sanitize`, `spawn_detached`, `reexec`, one-shot invokes and Qt's launches.
  The runtime's keep-alive pipe was passed to the installed copy after an
  install from Downloads, so the downloaded image's runtime and mount stayed
  alive. `on_startup` itself no longer releases that pipe (the previous
  change did): a helper an app starts from its own image, such as Find's
  settings window, lost its files when the app quit.
- `childenv::on_startup` (Qt: `onStartup`), called first thing in each app's
  `main`: an AppImage's open handles on its mount (the runtime's fd 1023 and
  files libraries leave open) become close-on-exec, and the working directory
  goes back to `$OWD`. Programs an app started inherited those handles, so a
  child that outlived it (the installed copy after an install from Downloads,
  a lean restart, another Arcade app) kept the old mount busy and its runtime
  never exited; and relative command-line paths resolved inside the mount.
  `release_mount_handles`/`releaseMountHandles` also runs before every launch
  Link makes.
- Install, repair and uninstall of one app take a per-app lock
  (`<installs>/<id>.lock`, the same file and advisory lock in Rust and Qt), so
  a reinstall and an uninstall can no longer interleave into a "successful"
  install with no executable. Stale `.stage` copies from a killed installer are
  removed under that lock.
- A dangling symlink at the install destination counts as an existing entry
  and is never replaced without a receipt.
- Launcher `Exec` lines escape newlines/tabs, and paths containing `%` run
  through `env` (GLib checks the program before expanding `%%`, so a `%` in
  HOME broke every launcher). Ownership checks read both forms.
- Launchers and login entries written by pre-installer releases (e.g.
  `ArcadeLens-0.1.1-linux-x86_64.AppImage` under the app's own desktop id) are
  taken over instead of skipped; the older copy's processes are asked to quit
  and, with "Remove the downloaded file", a copy in Downloads is removed.
  `install::legacy_copies`/`legacyCopies` lists them for prompts.
- "Remove the downloaded file" only ever removes a file inside the Downloads
  folder (`XDG_DOWNLOAD_DIR`), and failing to remove it after the receipt is
  committed no longer reports the install as failed.
- Install errors name the folder involved.
- `reexec` from an AppImage starts the installed copy detached and returns, so
  the downloaded image's mount goes away; the successor gets the user's
  environment (`childenv`). New `childenv::sanitize` strips AppImage runtime
  variables and mount paths for any program an app starts; Qt has
  `ArcadeInstall::childEnvironment`.
- One-shot calls honour cancellation while a provider streams progress, and
  stop waiting 250 ms after the final response.
- `TrayHostWatcher::finish` (Qt `ArcadeTrayHost::finish`) stops without
  restoring the app's own icon, for quitting apps.
- Qt: the `/tmp/arcade-<uid>` socket fallback refuses a folder that is not
  private to the user, like the Rust transport.

## 0.3.0 — 2026-10-11

- Adds optional settings, restart, menu, menu invocation, shortcut rebinding
  and settings export/import methods, describe.methods and typed clients.
  Old Handler implementations and public struct literals still compile.
- Adds opt-in tray hosting: OS registry watch, blocked subscription reads,
  immediate fallback on host loss, 300 ms startup and 5 s restart grace,
  exclusions and clean Tools shutdown. Windows named-pipe deadlines use
  cancellable OS event waits. Qt's watcher uses background socket/registry
  work and queued GUI-thread signals, with the same shared state vectors.
- Adds canonical accelerator normalize/display/conflicts, app shortcut docs
  and third-party sheets, JSON schemas, vectors and a stdlib Python validator
  and Markdown generator, including XKB/Hyprland input aliases.
- Adds private atomic receipts and explicit-path install detection, Linux
  AppImage install/update/rollback/repair/uninstall, native Windows PATH/shim
  and macOS bundle helpers, and shared first-run dialog strings.
- Adds manifest menu/logs/docs/install extensions without changing existing
  Manifest fields, mock peers for optional methods and tray hosting, CLI
  tray/shortcuts commands, install/tray diagnostics and Tools fixtures.
- Adds shared per-user Inno/NSIS includes and a Windows silent-install CI
  job, release installArgs and portable asset metadata. Protocol/schema stay 1.
- Ports optional methods, accelerators, shortcut validation, receipts and
  install/platform helpers to Qt; adds Rust interop and complete checksum
  vendoring. Covers launcher ownership, native path preflight, backup
  collisions, symlink rollback, desktop Exec parsing, tray recovery, sheet
  globals/alternative conflicts and successor-safe Windows uninstall.
- Uses the same FNV-1a isolated Windows PATH identity in Rust, Qt and
  PowerShell, pinned by shared vectors.
- Adds isolated presence/tray/install/shortcut e2e scaffolds, tray hand-back
  benchmarks and native-size PNG renders for all eight family glyphs.
  Keeps existing fleet checks and benchmarks, fixes Shelf's checkout path,
  and mirrors the shared new-app brief.

## 0.2.0 — 2026-10-10

- Registers Arcade Shelf (`arcade.shelf`) and Arcade Find (`arcade.find`) in
  the Rust and Qt app metadata: names, pitches and releases pages, so
  Connected apps lists show both apps with a Get link before they are
  installed.
- Glyphs (`assets/glyphs/arcade.shelf.svg`, `arcade.find.svg`) and accents
  (Shelf `#94A8FF`, Find `#22C55E`) in `tokens.json`.
- SPEC catalog rows for `shelf.add`, `shelf.show`, `shelf.pick`,
  `find.search` and `find.show`, and the Find shortcut (Ctrl+Alt+F).
- Mock fixtures `fixtures/shelf.json` and `fixtures/find.json`, real-binary
  Shelf and Find groups in `tools/e2e.py`, and benchmark entries.
- No protocol, manifest or API changes: apps on `v0.1.0` keep working and
  talk to apps on `v0.2.0`.

## 0.1.0 — 2026-10-08

- Protocol v1: registry, local transport, content matching, invocation,
  lifecycle and conformance, in Rust (`watch`, `engines` features) and the
  Qt module. Used by Box, Look, Lens, Clipboard, Wheel and Tools.
