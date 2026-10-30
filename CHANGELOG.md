# Changelog

## 0.4.9 — 2026-10-30

- Add the egui welcome hero and disclosure controls for Find's standard Settings flow.
- Supply shared Find peer descriptions for Connected apps.

## 0.4.8 — 2026-10-30

- Give repeated egui row titles independent measurement caches. Shortcut sheets with repeated labels now settle without continuous layout discards.
- Keep disabled egui labels readable by using the kit disabled inks without a second opacity reduction. Rendered text contrast is tested in both themes.

Arcade Link follows the frozen protocol v1 and manifest schema 1. Apps pin an
immutable tag; a new tag never changes the wire format.

## 0.4.7 (2026-10-29)

- Keep egui row descriptions above their dividers when text, font scale or
  window width changes. Allocate the current text bounds even when egui's
  layout-pass budget is exhausted, then finish centring on the next frame.
- Ellipsize long row names with their full tooltip; descriptions and paths
  continue wrapping inside the row.
- Add `page_with_head` / `page_with_head_actions`: the page head and sidebar
  stay in place while long content scrolls in its own panel.
- Add regressions for changed-description overlap, long glyph names and paths,
  stable layout, resizing and the body scroll clip. The overlap and ellipsis
  checks fail with the previous row implementation.

## 0.4.6 (2026-10-29)

- egui kit: `glyph_tile` and `row_with_glyph` (a settings row with a leading
  36 px glyph tile, for the Connected apps page), `checkbox` (selection in
  lists and dialog options, with the label in its hit target) and
  `dialog_head` / `dialog_actions` (title-sm and body, then actions
  right-aligned in order with the primary last, wrapping when narrow).
  `paint_icon_sized` draws kit icons at the tile and small sizes.

## 0.4.5 (2026-10-29)

- Add the shared egui component set for Find and Lens: sidebar, page/section
  heads, lists and settings rows, switches, buttons, inputs, selects,
  segmented controls, badges, notices, key chips, empty states and toasts.
- Add a keyboard shortcut recorder with modifier validation, platform display,
  reset and explicit conflict handling. Apps supply registry/desktop conflicts.
- Bundle real 500/600 Inter instances for egui, whose `strong()` changes only
  colour, with reproducible generation, provenance hashes and weight checks.
- Use readable foreground inks, bundled Inter and the shared measurements;
  wrap setting controls on narrow pages and keep busy-button widths stable.
- Check minimum-window layouts in both themes at 100% and 125% scale, switch
  mouse/keyboard/disabled behavior and notification lifetime rules.
- Match the CSS kit's look, checked on rendered screenshots in both themes at
  720×520, 760×560 and 125 % text: line-strong button borders, a borderless
  muted ghost, danger's red border on hover, ghost icon buttons, the sunken
  segmented track, pill badges and sidebar counts, key chips with a bottom
  edge, notices and toasts with a tone icon and the action beside the text,
  a dashed empty state, and `page()` for Template A's padded, centred content.
- Rows keep a control's natural width and left-to-right order beside the text
  (it moves below the text when it needs more than about half the row), and
  separate rows with a line only between them. Buttons wrap to the next line
  instead of shrinking, and a busy button no longer overlaps its neighbour.
- Both sides of a row, section head, page head and notice are measured and
  centred on each other, so buttons line up with their titles. Buttons use the
  CSS padding (12 px, 10 px small); a busy button shows a centred spinner at
  the same width. Selects are 32 px high with the kit chevron.
- Add `page_head_with` and `section_head_with` for trailing actions, and
  `ShortcutRecorder::row` (with `ShortcutSpec`): key chips, "Change…" and,
  only when it differs from the default, "Reset", all 28 px high beside the
  label, with conflicts shown under the row.

## 0.4.4 (2026-10-29)

Design kit fix; no protocol or API change.

- Fix low-contrast status badges, accent links and small metadata in the shared
  kit. Generated readable inks meet 4.5:1 on neutral, hovered, sunken and
  matching tinted surfaces in both themes; brand and status fills are unchanged.
- Keep the existing Rust palette and Flutter constructors compatible. Rust's
  `design::ink()` and additive Flutter/QML/CSS ink tokens expose the foregrounds.
- Check these pairs in generator, Rust and Flutter tests; reject metadata
  colours below AA before generating files.

## 0.4.3 (2026-10-29)

Design kit fix; no protocol or API change.

- `ArcadeHintBar` (Flutter) shares the free width between its labels by
  length. Equal shares cut "Paste as plain text" in Clipboard's 480 px
  picker while "Paste" and "Close" left space unused.
- `tools/check_dart_kit.sh` checks that a long hint label isn't cut off.

## 0.4.2 (2026-10-29)

Design kit fix; no protocol or API change.

- `ArcadeRow` and `ArcadeNotice` (Flutter) put their trailing control or
  action under the text when it doesn't fit beside it (the text keeps at
  least 160 px), so a segmented control, a badge and button, or two notice
  buttons never overflow a narrow window or a phone. The layout reports
  intrinsic sizes, so rows still work inside dialogs.
- `tools/check_dart_kit.sh` checks rows and notices at phone width and a
  row inside a dialog.

## 0.4.1 (2026-10-28)

Flutter components for the design kit; no protocol or API change.

- `design/dart/arcade_widgets.dart`: the kit's components for Flutter apps
  (Clipboard), as thin wrappers over Material themed by `arcadeTheme()`:
  buttons in every variant (busy and icon-only included), settings and list
  rows, switch rows, badges and dots, key chips, notices, the dashed empty
  state, toasts, sidebar and nav items, page head, segmented control, search
  field (Esc clears, then leaves), overlay rows and hint bar, the kit dialog,
  the confirm-before-destroying dialog and skeleton rows.
- `tools/check_dart_kit.sh` renders every component in light and dark and
  fails on overflow, and checks the confirm dialog.
- `tools/e2e.py` finds Box's data in `arcade/box` from Box 0.4.0.

## 0.4.0 (2026-10-28)

Arcade design kit; shared app folders and diagnostics; a stricter test harness.

- `design/`: the design kit. `tokens.json` holds every value of
  `design/STANDARD.md` §2-4 (shared neutrals and status colours, one accent per
  app, Inter, type scale, spacing, radii, sizes, motion, shadows, icons).
  `design/generate.py` (stdlib only) writes `css/arcade.css` (Tools 0.3.8's
  look made reusable, with light/dark/system themes and `[data-app]` accents),
  `css/icons.ts`, the QML `Arcade` module (`Theme`, `Fonts`, `Icon`,
  `Format.js`), `dart/arcade_theme.dart` and `arcade_icons.dart`, and the Rust
  tokens; `--check` fails on stale output, an accent below its contrast pair,
  a default-shortcut clash or a literal colour in the component CSS. Find
  moves to lime and Shelf to fuchsia; Tools' light green is `#16884b` (the
  0.3.8 `#168a4c` gives white text 4.40:1, under the 4.5:1 rule).
- `design` feature: `arcade_link::design` (`palette(theme, app_id)`, type,
  space, radius, size and motion consts, `ICONS`, `inter_font_bytes()`,
  `format`), and `design-egui` with `apply_egui` (Visuals, spacing, Inter).
- Inter 4.1 variable (`design/fonts`, SIL OFL), the outline icon set
  (`design/icons`, Tools' icons plus Box's and Look's), the app marks
  (`design/glyphs`, canonical; `assets/glyphs` keeps generated copies) and
  every app's launcher icon set (`design/app-icons/make.py`).
- Shared format helpers (`formatSize`, `formatCount`, `formatDuration`,
  `formatRelative`) in Rust, TypeScript, QML and Dart with one case file.
- `design/shortcuts.json`: the default global shortcut of each app.
- `paths::{app_dirs, legacy_dirs, migrate_app_dirs}` (Qt: `ArcadePaths.h`):
  `arcade/<app>` config, data and cache folders, every old location per OS
  (`spec/app-dirs.json`; an entry can name a subfolder, such as `webview` for
  a Tauri app's old webview storage), and a copy-verify-remove migration that
  leaves a folder in place on any conflict and is safe to run on every start.
- `diagnostics::Header` (Qt: `DiagnosticsHeader`): the shared first lines of
  "Copy diagnostics".
- `manifest::app_pitch` (Qt: `appPitch`) returns each app's tagline, its README
  one-liner, so every app describes its peers the same way (STANDARD §8).
- `manifest::os_identifier` (Qt: `osIdentifier`): an app's `dev.arcade.<App>`
  bundle id, desktop file name and window class (STANDARD §10).
- Long socket paths fall back to a private `arcade-<uid>/` under
  `$XDG_RUNTIME_DIR`, then `$TMPDIR`, then `/tmp` (was always `/tmp`), so
  isolated runs keep their sockets inside their own root (SPEC §4.1).
- Test harness: `tools/stress.py` requires a non-empty output file of the
  expected type (`{}`, `outputs: []` and text-only results no longer pass);
  `tools/leak.py` checks every result's text, not only exit codes;
  `tools/real_bench.py` honours `--runs` for the tray hand-back probe. The
  nightly e2e picks one green run per app, builds each app's development
  executable at that commit (`tools/fleet_build.py`), lists the skipped checks
  in the job summary and fails a check whose package or build is missing
  (`--require-artifacts`).
- `LICENSE-MIT`, `LICENSE-APACHE` and `rust-toolchain.toml` (1.88); the early
  plans and 0.3 reports moved to `docs/dev/history/`; links in vendored files
  are absolute.

- `childenv::enter_image_dir`: an app that shows a WebKitGTK webview (Tauri)
  goes back into `$APPDIR/usr` once it has read its command line.
  linuxdeploy's GTK plugin points WebKit's helpers at `././/lib/...`, relative
  to that directory, so after `on_startup` moved the app to `$OWD` every Tauri
  app aborted on its first webview ("Unable to spawn ... WebKitNetworkProcess").
  Tools, Box and Look call it; programs they start still begin in the user's
  home through `sanitize`.
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
