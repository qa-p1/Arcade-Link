# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.1.2] — 2026-11-24

### Fixed
- `arcade-helper` runs an action that takes no input (empty `accepts`: Arcade Box's password generator, timer, IP,
  loan calculator…) when asked with none, once, instead of refusing with "nothing to run on". The browser
  extension's search runs these from its popup.
- `arcade-helper`'s progress window no longer sends a system notification for jobs started from the browser: the
  extension announces them itself, with the result, so they aren't announced twice.
- `arcade-helper` keeps "no number" as no number when an app sends progress without a fraction (Arcade Box looking
  up a video, joining files). Its progress window and the browser extension then show a moving bar instead of one
  sitting at 0. With several runs, the finished ones still count.

## [1.1.1] — 2026-11-23

### Added
- `arcade-helper` on macOS: Finder Quick Actions ("Arcade…" and each pinned action) in `~/Library/Services`,
  filtered to each action's file types and kept in step like the Linux and Windows entries. Checked in CI with
  `plutil` and by running each Quick Action under `automator`.

### Fixed
- `arcade-helper` stops when its card is removed (uninstalling with `arcade-helper integrate --remove` while it
  runs) instead of writing its menu entries back on the next card or catalog change.
- Tier 2 smoke (`tools/desktop/tests/smoke.py`) sets `ARCADE_NO_INSTALL_PROMPT=1`, so an app that asks to install
  itself before it starts (Arcade Box, Arcade Lens) doesn't wait on that dialog when started with `--dashboard`.

## [1.1.0] — 2026-11-22

### Added
- `arcade-helper` (new crate, SPEC §5.6): Arcade's actions in the file manager menus (Linux and Windows; macOS
  through Arcade Tools) and, through native messaging, the browser extension. It runs in the background, owns the
  jobs, shows a picker and a progress window drawn with the design kit, places results without overwriting and
  keeps its menu entries in step with the installed apps (`arcade-helper jobs` lists the jobs as JSON for
  scripts). `client::request_install` asks Arcade Tools to install an
  app.
- "What can I do with this?" (SPEC §5.5): `offers::offers` (Qt `offers`) gives every caller the same ranked list of
  actions for a selection on a surface, from the installed cards, the catalog's `rules` (hide, rename, boost, jobs)
  and the user's choices in `offers.json` (`offers::Prefs`, Qt `loadOfferPrefs`/`saveOfferPrefs`): install entries
  for actions missing an engine or app, greyed-out entries over `maxItems`/`maxBytes`, and two apps doing one job both
  shown until the user picks a default. `offers::input_type` (Qt `offerInputType`) names the type of a multi-selection.
  `Catalog::rules`, `Locations::offer_prefs`. Vectors: `spec/vectors/offers.json`.
- Cards can describe much more about each action, so other apps, the file manager menus and the browser extension
  can offer it without code of their own (SPEC §3, all optional, ignored by Link 1.0 apps): `options` (the options
  form, in Arcade Box's catalog format: new `form` module with defaults, visibility and value checks), `surfaces`
  (where it makes sense: file menu, browser image, selection, …; derived from `accepts` when absent), `rank`
  (supersedes `featuredFor`), `keywords`, `job` (recognizes two apps doing the same job), `oneClick`, `maxItems`,
  `requires` and `output`. A malformed value in one of these reads as absent instead of hiding the whole card.
  Rust: `Action::surfaces`, `on_surface`, `is_one_click`, `rank_for`; Qt: `actionSurfaces`, `actionIsOneClick`,
  `actionRankFor`. New vectors: `spec/vectors/actions.json`.
- The app list is data (SPEC §3.1): `catalog` module (Qt `Catalog`) reads names, taglines, release pages and pair
  sentences from the copy of Arcade-catalog's `catalog.json` that Arcade Tools keeps at `Locations::catalog()`
  (`<registry>/../catalog.json`), and `catalog::write_shared` (Qt `writeSharedCatalog`) stores it. The compiled
  values are the fallback, so a new Arcade app needs a catalog entry, not a Link release. Qt gains
  `peerDescription`. The CLI names apps from the catalog. Vectors: `spec/vectors/catalog.json`.
- Refined content types (SPEC §5.1): `file/svg`, `file/csv`, `file/json`, `file/html`, `file/markdown`,
  `file/ebook` and `text/markdown`, each also its broader type (`file/csv` is a `file/spreadsheet`), plus the new
  `file/subtitle` (srt, vtt, ass, …, formerly `file/any`). `content::parent_type`, `is_kind`, `type_for`; Qt
  `parentType`, `ancestorTypes`, `isKind`, `typeFor`, `adaptInputs`.

### Changed
- `invoke_action` (Rust and Qt) sends each input as the type the target's card names: a `.csv` reaches an app that
  only knows `file/spreadsheet` as `file/spreadsheet` (SPEC §5.2), so older apps never see a type they don't know.
- File kinds by extension: `.svg`, `.csv`/`.tsv`, `.json`, `.html`, `.md` and e-book files now report their refined
  kind, and subtitle files report `file/subtitle`. When an app moves to Link 1.1, any code of its own that compares
  a kind by name (`kind == "spreadsheet"`) should use `content::is_kind` / `isKind` so `.csv` still counts.
- Release signing moves to `ed25519-dalek` 3, `sha2` 0.11, `blake2` 0.11 and `base64` 0.23. Signatures and keys are
  unchanged: the published Arcade Tools 1.0.2 `manifest.json.sig` still verifies with the release key and a tampered
  copy is still refused, and the secret key's BLAKE2b-256 checksum is byte-identical to the former variable-output
  hasher (so `ARCADE_UPDATE_SIGNING_KEY` keeps working).
- `serde_json` 1.0.154, `interprocess` 2.4.9, `notify` 8.2.2, `libc` 0.2.191.
- Workflows (including the reusable app workflows): `actions/checkout` v7, `actions/download-artifact` v8,
  `actions/upload-artifact` v7, `actions/setup-node` v7, `nix-community/cache-nix-action` v7.
- egui stays at 0.33: Find and Lens draw with Link's egui design kit, so the three move to a newer egui together
  when those apps adopt Link 1.1.

### Security
- The app template locks `url` 2.5.9 (RUSTSEC-2026-0592: stack overflow in `Url::origin` on nested `blob:` URLs).
  Link's own crates don't use `url`.

## [1.0.15] — 2026-11-22

### Added
- `design::linux_prefers_dark_without_portal` (and `design::prefers_dark_from`): whether a Linux desktop prefers dark
  when the XDG Settings portal gives no answer, from GNOME's `color-scheme`, the GTK theme name (`GTK_THEME`, GNOME's
  `gtk-theme`, GTK 3's `settings.ini`) or `gtk-application-prefer-dark-theme`; otherwise light, as GTK and WebKit
  show. egui apps fell back to dark there, so Lens and Find were dark next to a light Arcade Tools on openbox.

### Fixed
- `engines::download_tesseract` runs curl, the downloaded AppImage's unpacking and its version check with the user's
  environment (`childenv::sanitize`). From Arcade Tools' AppImage the system curl loaded Tools' bundled, older
  libnghttp2 and failed with "symbol lookup error", so "Download with Arcade" never worked on a current distribution
  (Tools D37; Tools 1.0.1 works around it until it moves to this Link).
- The real-fleet e2e (`tools/e2e_artifacts.py`) took an app's package from its five newest successful main runs,
  and the apps' daily scheduled Nightly runs filled them: Box's and Look's CI packages were never found and every
  check that needs them failed ("no unexpired … artifact"). Scheduled runs are skipped, like Dependabot's.
- The real-fleet check `disabling_link_in_find_keeps_its_own_icon` asked Find for `--status` while the Find it had
  just started was still unpacking itself (the harness runs AppImages with `APPIMAGE_EXTRACT_AND_RUN`, one folder per
  AppImage). On a cold runner the second unpack broke the first and Find exited 126, so the check failed whenever it
  was Find's first start in the job. It now waits, with a start's 20 s, until Find runs from the unpacked folder.
- The e2e harness gives each session its own `TMPDIR`. AppImages run with `APPIMAGE_EXTRACT_AND_RUN` unpack into
  `$TMPDIR/appimage_extracted_<hash>` and rewrite it on every start, so two sessions starting the same app in one
  shared `/tmp` broke each other's start (`portable_backup_restores_real_app_settings_in_a_second_arcade_home`: Find
  exited 127). A session's unpacked copies are removed when it closes.

## [1.0.14] — 2026-11-17

### Fixed
- Tier 2 (`desktop-tier2.yml`, `tools/desktop/tier2.sh`) started no Secret Service, unlike Tier 1's `session.sh` and
  every real KWin or Mutter desktop (KWallet, GNOME Keyring). An app that keeps its keys there (Clipboard's history
  key) could not open its profile, so `smoke.py` reported it "not running" on all four desktops. Tier 2 now installs
  `gnome-keyring` and starts an unlocked one in each session, as Tier 1 does.
- Tier 2's package install timed out on a slow Ubuntu mirror (8 minutes for ~170 packages; Shelf rc.3's gates and
  Wheel's CI, all on the install step, before any test ran). apt fetches one package at a time and the mirror took
  ~3 s per request; the step now fetches what is not cached 16 at a time first, falling back to two other Ubuntu
  mirrors for a file that stalls (apt still checks every hash and fetches anything missing itself), caches the
  packages per runner image and package list, and has 15 minutes (the job 25).
- `tier2.sh` waited 20 s for the compositor's display and then went on without one (`DISPLAY: unbound variable`).
  Mutter took 19 s on a runner once; it now waits up to 60 s and otherwise fails with the compositor's log.

## [1.0.13] — 2026-11-16

### Fixed
- `runtime::relaunch_unpacked` (and its C twin `arcade_relaunch_unpacked`) started the unpacked copy with the
  launcher's environment, in which the AppImage's `AppRun` had pointed `GSETTINGS_SCHEMA_DIR`, `GIO_EXTRA_MODULES`,
  `XDG_DATA_DIRS`, `GTK_*` and more into the FUSE mount. The launcher exits at once and the mount goes away, but the
  copy's own `AppRun` runs linuxdeploy's GTK hook, and that hook calls `gsettings` before it sets those variables
  again: `gsettings` read the vanishing mount, crashed, and hung in its core dump, so the copy never started. This
  happens when the desktop portal does not answer the hook's colour-scheme query (headless CI, some desktops);
  Look's rc.2 gates hit it on Restart, about one start in twelve on ubuntu-22.04. The copy now gets
  `childenv::changes` (every reference to the mount removed) except `APPIMAGE`, `OWD` and `ARGV0`.

## [1.0.12] — 2026-11-16

### Fixed
- The `setup-arcade-link` action cached the built CLI under `runner.os` and Link's commit only, so a job on
  ubuntu-22.04 restored a CLI built on ubuntu-24.04 and failed with "GLIBC_2.39 not found" (Box's rc.2 gates, Linux
  conformance). The key now names the runner image (`tools/ci/runner-image.sh`).
- The desktop harness's `pointer_move` stopped the run when the cursor read back one pixel short: the virtual pointer's
  absolute move is x/width of the layout and Hyprland reports the cursor truncated, so (505, 286) read (504, 286)
  (Lens's Tier 1 drag). A one-pixel difference is accepted; the harness self-test covers that point.

## [1.0.11] — 2026-11-15

### Fixed
- An app started from another app's unpacked copy, or a Restart's successor, inherited `ARCADE_UNPACKED` and kept
  running from its FUSE mount (RT-7). The copy now runs with `ARCADE_UNPACKED` set to its own folder; a value that
  does not name the folder the executable runs from is inherited, so it is dropped and the process still relaunches
  (`"1"` still means never relaunch, for test harnesses). `childenv::sanitize` and Qt's `childEnvironment` also drop
  it. `runtime::is_unpacked_copy()` and C `arcade_is_unpacked_copy()` answer "is this process the copy" for apps
  that skip work there (first-run prompts, stale-copy cleanup).
- `app-publish.yml` (`release_flow.py publish`) published files whose names have spaces (Tauri names them after the
  product name, "Arcade Box_1.0.0_amd64.AppImage") under GitHub's changed name ("Arcade.Box_..."), while
  `SHA256SUMS.txt`, `arcade-release.json` and `manifest.json` named the original, so the rc's own gates (and Tools)
  could not find them. Spaces now become "-" before the metadata is written (the names 0.x releases had), other
  characters GitHub would change are refused, and the published release must hold exactly the files the metadata
  names.

## [1.0.10] — 2026-11-15

### Added
- `qt/arcade_unpacked.h`, the header-only C twin of `runtime::relaunch_unpacked` (RT-7; AUDIT/memory.md U1) for the
  Qt apps (Shelf, Wheel) and the Flutter Linux runner (Clipboard): `arcade_relaunch_unpacked(app_id, version, wait,
  argv)` copies the AppImage's contents once to `<data>/arcade/runtime/<app>-<version>-<key>/` and runs the same
  command line from there, returning -1 when nothing was done and otherwise the code to exit with. Folder names are
  the ones Tools' `runtime_gc` recognises. Tested in `tst_Unpacked` with a helper executable run from a fake AppDir.
- `runtime::remove_unpacked_of(root, app_id)`: one app's unpacked copies, other apps' kept.

### Changed
- Uninstall (Rust `install::uninstall`, Qt `ArcadeInstall::uninstall`) also removes the app's own unpacked runtime
  copies, so an uninstalled app leaves none behind; other apps' copies stay. Best effort: a copy that cannot be removed
  is reported, not an error (Tools also collects them).

## [1.0.9] — 2026-11-15

### Added
- Dart kit: the app marks. `design/generate.py` writes them into `arcade_icons.dart` as `arcadeGlyphOps` (16 grid,
  from `design/glyphs`, the same source as QML's `GlyphData.js`), with `arcadeGlyphPath`, `ArcadeGlyphPainter` and an
  `ArcadeGlyph` widget (stroke 1.5, round caps and joins, like the QML `Glyph`). Clipboard drew the marks from
  hand-copied paths, and its Tools and Look marks no longer matched the kit. Tested in `tools/check_dart_kit.sh`
  (every app has a mark inside the 16 grid; an unknown id draws nothing).
- `app-gates.yml`, a reusable workflow for an app's release gates on its published rc (PLAN "Release" step 2), with
  Tools' Gates as the model: the rc's and the previous release's files against SHA256SUMS and the rc's manifest,
  `arcade-link conformance` on the published AppImage, the app's own Linux checks, N-1 (`tools/gates/n1_app.py`:
  the Link CLIs the previous released peers ship see the rc running, read its status and describe it), the memory
  and performance budgets, the delta gate through Tools' updater (`ARCADE_TOOLS_GATE_APP`), and the app's Windows
  and macOS smoke commands. `n1_app.py` was checked against Link's mock app in the isolated session, passing with
  the app up and failing when it never starts.

### Changed
- The e2e harness starts a real AppImage with `ARCADE_UNPACKED=1` as well as extract-and-run, so an app built on
  `runtime::relaunch_unpacked` (Box from step 5) keeps running as the process the harness started rather than
  handing over to a copy and exiting 0; the stop, kill and crash checks act on that process.
- Dependabot groups minor and patch updates into one weekly pull request per ecosystem and no longer rebases
  them on every push to main, which queued a full CI run per open pull request ahead of release work.
- The real-fleet install check expects every app's update to retire its old launcher and login item: Clipboard
  and Shelf are on Link 1.0.8 (their main builds are green), so nothing is an expected failure any more.

## [1.0.8] — 2026-11-14

### Added
- `app-publish.yml` and `app-nightly.yml` take `windows-installer` (`nsis`, the default, or `inno`), passed to
  `arcade-release.py` through `release_flow.py publish --windows-installer`. Promotion reads it from the rc's own
  `arcade-release.json`, so a stable release tells Tools the same silent switches the rc was tested with. Before,
  every reusable publish described a Windows `.exe` as NSIS, which would have run Clipboard's, Find's and Lens's
  Inno installers with `/S`. Tested in `tools/test_release_flow.py` and `release-flow-e2e.yml` (an Inno rc
  promoted to a draft stable).
- `app-nightly.yml` leaves out the release metadata (`arcade-release.json`, `SHA256SUMS.txt`, `manifest.json`)
  that per-platform CI artifacts carry, which made the `mv` into one folder fail on duplicate names (Wheel's
  bundles each carry their own). Publishing writes it again for the whole set.

### Fixed
- Dart kit: `ArcadeKeyChip` stays the size of its key in a stretched row. The Container's `alignment` made it fill
  the offered width, so Clipboard's shortcut recorder showed three full-width bars (RD-CLIPBOARD-2). Tested in
  `tools/check_dart_kit.sh` (failed before: a 900 px chip).
- QML kit: `HintBar`'s trailing text is elided before it reaches the hints instead of being drawn over them
  (Shelf SH-3).

### Tests
- The real-fleet e2e expects `*_update_retires_old_desktop_ids` to pass for Tools, Box, Find, Lens, Look and Wheel,
  whose main builds now vendor Link 1.0.7; only Clipboard and Shelf stay in `LEGACY_PENDING`.
- `*_update_retires_old_desktop_ids` plants the old login items where Link's installer looks for them:
  `$ARCADE_HOME/.config/autostart` when `ARCADE_HOME` is set, as in every e2e session (it used `$XDG_CONFIG_HOME`,
  which the installer ignores then, so the 1.0.7 apps "left" entries they could never see). The fleet's development
  builds get `libdav1d-dev` (Look's image decoder).
- An old entry that the update keeps as the app's recorded login item no longer counts as left behind: Tools'
  login-item name `arcade-tools` is also its old launcher name, and Link's installer rightly keeps that login entry
  (start at login carries over) while it retires the old launcher. The real-fleet job installs `libdav1d7`, which
  Look's development build needs at run time.

## [1.0.7] — 2026-11-10

### Fixed
- Updates retire the app's icons installed under an older desktop id (`arcade-find.png` beside the new
  `dev.arcade.Find.png`) once this install has written its own icons, in both the Rust and the Qt installer. Icons
  this install did not supply were all carried over in the receipt, so Box kept its `dev.arcadebox.app` icons after
  moving ids (Box audit BP-4). Icons under the current id are still kept when the new build ships none. Tested in
  `tests/install.rs` and `qt/tests/tst_Install.cpp`.
- Updates and repairs retire an app's launcher and login entries under its older desktop ids
  (`AppInfo::legacy_desktop_ids` / `legacyDesktopIds`) even when they run the very file that is installed now, in
  both the Rust and the Qt installer. Only entries for another copy were removed, so on a real desktop updated from
  the 0.4 releases Box, Look, Shelf, Clipboard and Wheel each showed two menu entries (e.g. `arcade.shelf` and
  `dev.arcade.Shelf`) and Look started twice at login. Start at login carries over to the new id, with the entry's
  own on/off choice. Tested in `tests/install.rs` and `qt/tests/tst_Install.cpp` (both failed before the fix); the
  real-fleet e2e plants the old entries before an update (`*_update_retires_old_desktop_ids`, XFAIL per app until
  it vendors 1.0.7: `LEGACY_PENDING` in `tools/e2e_checks/real_hub.py`).
- `tools/fleet/wmclose.c` (the fleet harness's window close, a title bar's WM_DELETE_WINDOW) waits for the X server
  with `XSync` before it exits. It only flushed, and an X server that polls a client after it has exited sees its
  request and the hang-up together and drops the client unread, so on a busy runner the close never reached the app:
  a window "would not close" in the memory gate about half the time under load (7 of 13 local runs; 0 of 10 with the
  fix; an X protocol trace showed the event never delivered, to the app or to any other listener). Not a Tools bug.
  `fleet_budget.py` rebuilds the helper when its source is newer than the (untracked) binary.
- `tools/fleet/fleet_budget.py` measures the open window once the app is idle: 6 s after the open as before, but on a
  loaded machine whose app still uses 5 % of a CPU or more then, after its first quiet 2 s; an app not quiet within
  30 s fails. A saturated runner's 6 s sample read the page still loading (file-backed code in WebKit), 263–290 MB
  against 206–222 when settled (Tools D21/D22). Idle runners are measured exactly as before.
- The desktop harness's portal stand-in forgets a client that left the bus, as the real portal does: its sessions
  and host-registry entry end with its connection. It kept them until `Session.Close`, which a quitting app never
  sends, so `Bound()` still listed a quit app's shortcut and `Press` reached its dead session (Box's Island test
  pressed before the restarted Box listened). The self-test checks both.
- The WebKit trimmer (`packaging/linux/arcade-trim-preload.c`) trims again each time a helper process settles during
  its first 15 s, not only at 2 s and then every 20 s. On a slow runner the page was still loading at 2 s, the code it
  ran afterwards stayed resident until the 22 s trim, and a window read about 48 MB more PSS for that time: Tools'
  open-window gate measured 250 on most runners and 289–309 on a slow one, on the same build (Link D14, Tools D21).
- `runtime::relaunch_unpacked` no longer passes the arena cap to the unpacked copy. The copy took the inherited
  `MALLOC_ARENA_MAX=2` for the user's own setting, so `childenv::sanitize` kept it and every app or installer it
  started ran with two arenas, contrary to Tools D18. The copy now sets the cap itself (test fails without the fix).
- The real-fleet e2e adapter (`tools/real_app_driver.rs`) uses the backup Tools ships (`backup::export`,
  `plan_restore`, `restore`). It called `portable_backup`, a second implementation Tools removed as unused
  (Tools 1953dbf), so every real-fleet group stopped compiling. The backup checks now read that module's report.
- The real-fleet loopback releases are signed and listed as GitHub releases are: Link's CLI signs each fixture's
  `manifest.json` with the test-only key debug builds of Tools trust for loopback sources, and the server answers
  Tools' release list. Tools refused the unsigned fixtures ("That release isn't published on GitHub."), so every
  install, update and self-update check failed.
- The real-fleet artifact picker skips Dependabot's update jobs, which report as successful runs on main without
  artifacts; five of them hid Box's packaging run, so every Box check failed as "missing artifact".

## [1.0.6] — 2026-11-09

### Added
- `runtime::cap_malloc_arenas()`: caps glibc's malloc arenas at two in the app and its WebKit processes (unless the
  user set `MALLOC_ARENA_MAX`), which keeps a WebKit window's memory from varying with thread timing (Tools D18);
  `childenv::sanitize` removes the cap for programs the app starts. Shared so Box and Look need no copy of Tools'.

### Fixed
- The desktop harness's portal stand-in serves the host app registry
  (`org.freedesktop.host.portal.Registry.Register`), as xdg-desktop-portal 1.19 and later do. Box registers its
  desktop-file ID there before binding its shortcut and gave up on the stand-in's UnknownMethod, so no Box shortcut
  could be tested on headless Hyprland. `Desktop.registered_apps()` lists the registered IDs.
- The portal stand-in reports bound shortcuts as a real portal does, with `description` and `trigger_description`
  (the app's preferred trigger), instead of echoing the request. Box's portal client (ashpd) refused the reply for
  the missing `trigger_description`, so Box still reported no shortcut.
- Under Hyprland the portal stand-in registers each bound shortcut with the compositor as `<app id>:<id>`, as
  xdg-desktop-portal-hyprland does (new `gshortcuts` helper, hyprland_global_shortcuts_manager_v1), and forwards the
  compositor's pressed/released events as Activated/Deactivated. Box checks that `hyprctl globalshortcuts` lists its
  shortcut before adding its key binding, and reported "hyprland_binding_unavailable" against the stand-in. The
  stand-in now starts after Hyprland; the harness self-test covers the path.
- The desktop harness waits until seatd's socket accepts the runner's connection, after stopping the seatd service
  Ubuntu's package starts. It waited only for the socket to exist, which the service's socket already did, so
  Hyprland could start before ours replaced it and crash on "Permission denied" (seen in Box's first Tier 1 run).
- The desktop harness downloads the Nix archive itself, failing on HTTP errors, retrying and checking it
  decompresses, and gives it to the Nix install action as a file. The action streamed GitHub's error pages into
  unzstd ("unsupported format"), which failed whole runs and also defeated its own retries.
- `runtime::install_crash_reports` no longer creates the crash folder up front: the first report makes it (the
  fatal-signal handler makes the missing folders with `mkdir`, which is async-signal-safe). An app that installed
  the reports at start broke CLI-4, since `--version` or `--arcade-manifest` then left `arcade/<app>/crashes/`
  behind (found adding crash reports to Tools).

## [1.0.5] — 2026-11-08

### Fixed
- `perf_budget.py` counts wakeups per thread, sampled every second. It differenced whole-tree totals, so every
  thread that exited during a window took its lifetime of context switches out of the count: Tools' open window
  read -2.3 wakeups/s (and 2.2 locally) where it does about 11, and a negative figure passes any budget.
- Tier 2 `smoke.py` waits up to 45 s for `--status` to say running instead of sleeping 8 s and asking once: a
  first start that unpacks the app before its resident starts (Tools' U1) could miss the window on a slow runner
  (seen once on sway). It then checks the app is still running 5 s later, reports a hanging `--status` as a
  problem instead of a traceback, and removes its temporary home.

## [1.0.4] — 2026-11-08

### Added
- `tools/fleet/perf_budget.py`: one app's performance budgets in CI (RT-6, PERF-2) under Xvfb, in a private profile:
  cold start and show latency (launch → window mapped), idle CPU and wakeups in the background, with the default page
  open for 60 s and after closing it, and disk use. Limits live in the `perf` block of `budgets.json` (Tools first).

### Fixed
- `desktop-e2e.yml` and `desktop-tier2.yml` put the app under `$RUNNER_TEMP`, outside the caller's checkout. Inside a
  git work tree the AppImage counted as a dev build, so a harness test's `--install` was refused (seen in Tools).
- CI no longer fails on a GitHub download outage (HTTP 504 for 10+ minutes): appimagetool and its runtime for the template,
  and a released AppImage the desktop workflows fetch by URL, are cached and retried with a delay.
- Template: the AppImage's root icon and `.DirIcon` are a PNG (appimagetool warns that it will refuse an SVG
  `.DirIcon`); the SVG stays in `hicolor/scalable`. `build-appimage.sh` takes `APPIMAGE_RUNTIME`, a downloaded type2
  runtime, so appimagetool need not fetch it on every build.

### Changed
- The template and the reusable workflows' default `link-ref` are v1.0.3.

## [1.0.3] — 2026-11-08

### Changed
- W3b page-out (`runtime::page_out_hidden`, `arcade_page_out_hidden()`) now covers every private mapping of a hidden
  app: the heap (skipped before, with every other `[...]` name), anonymous memory, and the pages of private file
  mappings (written RELRO and `.data` go to swap, clean pages are read back from the file). `MADV_PAGEOUT` never
  changes contents; stacks, guard pages and the kernel's mappings are left alone. Measured on a Tools AppImage
  (Xvfb, with swap): background PSS 15 → 4 MB, 20 s after closing its window 29–34 → 12 MB, reopen latency
  unchanged (1.41 → 1.40 s over 8 reopens).
- `tools/fleet/budgets.json`: Tools' window-open cap on Xvfb is 281 MB PSS (was 225, the audit's lab estimate). The
  value is the higher of two CI runs (267.7) plus 5 %. With the window open, WebKitWebProcess keeps about 123 MB of
  in-use malloc for llvmpipe compositing, which only UX trades (compositing off, paging out the open window) would
  remove (Tools D10). RSS stays at 310.
- The template pins `v1.0.2` (Cargo, lockfile and workflows); the reusable workflows' `link-ref` default is now
  `v1.0.2`.

## [1.0.2] — 2026-11-08

### Fixed
- Design kit: a tab panel focused from the keyboard shows the focus ring (`.tab-panel:focus` removed it for every
  focus, so Tab could land on an invisible stop). Found by Tools' accessibility checks.
- Qt tray host: when the startup gate decided first (a slow machine), the worker's late undecided report put the
  state back to `none` and emitted `stateChanged`; a decided state now only changes to another decided one.
- Runtime trim (`runtime::trim`, `arcade_trim()` and the WebKit helper preload) reverted private copies in an
  executable mapping (text relocations, a debugger's breakpoints) to the file's bytes. A mapping is now dropped only
  when `/proc/self/smaps` shows no anonymous or swapped pages.
- Uninstall (Rust and Qt) left `<installs>/<id>.lock` and, on Linux, an `icon-theme.cache` / `mimeinfo.cache` that
  described nothing, so a fresh-HOME uninstall was not clean. Uninstall now retires the lock file while it holds it
  (`AppLock::retire`: a waiter that then gets the lock on the removed file sees the marker and locks the path afresh,
  so two installers still never interleave) and removes a cache only when it is the last file in its folder. The
  Qt lock opens the file with delete sharing on Windows, as Rust does.
- A runtime test that runs a copy of its own binary retries while a fork on another test thread keeps the copy busy
  (`ETXTBSY`).

### Changed
- Runtime trim also drops clean read-only data mappings (`r--p`, e.g. WebKit's and the app's `.rodata`), which
  stayed resident after every trim. RELRO and other copied mappings
  are still never touched.
- `install::repair` documents that `moved_to` must only be passed when the receipt's file is gone.
- The template pins `v1.0.1` (Cargo, lockfile and workflows) and passes `link-ref` to Link's reusable workflows
  explicitly; their `link-ref` default is now `v1.0.1`.

## [1.0.1] — 2026-11-08

### Fixed
- Legacy folder migration finishes once: a legacy file whose name is taken by different contents in the new folder
  is moved to `archived-legacy/<old folder>/` there (`Migration::archived`) instead of keeping the whole legacy folder
  and logging it on every start (FS-1; Tools T3).

## [1.0.0] — 2026-11-07

Arcade Link 1.0: the shared runtime, the release and update primitives, the desktop harness and the Arcade app
standard. Apps pin `v1.0.0` (or a later 1.0.x) by tag.

### Added
- Signed release manifests: `release-manifest`, `promote-manifest`, `verify-manifest`, `keygen`; FastCDC chunk lists,
  minisign-compatible ed25519 signatures, `plan_delta` and `assemble` (`release` feature).
- `runtime`: unpacked AppImage runtime (`relaunch_unpacked`, U1), single instance, restart via `$APPIMAGE`, reaped children, local crash reports, memory trim (W0/W1/W3b);
  C twin `qt/arcade_trim.h` and the WebKit-helper preload trimmer.
- `ARCADE_APP.md` (the app standard, replacing `NEW_APP_SPEC.md`, now `docs/ARCADE_APP_GUIDE.md`) and
  `arcade-link conformance`.
- Reusable workflows: publish (test/nightly), promote, nightly, app checks, desktop e2e (headless Hyprland) with the
  `tools/desktop` driver.
- N-1 CI job; `SECURITY.md`, `THIRD_PARTY_NOTICES.md`, `cargo deny`, Dependabot.

### Changed
- A `hello` without `protocol` is accepted as protocol 1 (legacy peers).
- Unknown CLI commands print usage to stderr and exit 2.
- QML `Dialog`: Escape cancels destructive dialogs; only `busy` blocks it.

### Security
- Link servers (Rust and Qt) drop connections from other users (peer UID check).

## 0.4.15 — 2026-11-01

- QML design kit: the component set for Shelf and Wheel in `design/qml/Arcade` (window shell, settings rows, controls, status, key chips and marks, dialogs and menus, overlay rows), built on `QtQuick.Templates`. `Theme` gains the rest and pop shadow tokens; `Glyph` draws the app marks. `design/qml/gallery/Gallery.qml` shows every component in light and dark.
- CI lints the QML kit with `qmllint --max-warnings 0` and runs `qt/tests/qml/tst_components.qml`.
- Icons: add `undo`.

## 0.4.14 — 2026-11-01

- `tools/e2e.py`: removing the isolated root after a run detaches FUSE mounts the private session left inside it (the document portal) and never fails a finished run. Python 3.12's `shutil.rmtree` raised ECONNABORTED from a dying portal mount despite `ignore_errors`, which failed Find's stable release run after every check had passed.

## 0.4.13 — 2026-10-31

- egui `page_with_head_actions`: the body scrolls across the whole page, so its scroll bar sits at the window edge instead of on the cards' right border (Settings in Find and Lens).
- Supply shared Lens peer descriptions for Connected apps.
- Install prompt: the second button reads "Just run it", as STANDARD.md specifies (was "Just run it once"); SPEC and the Qt strings follow.

## 0.4.12 — 2026-10-31

- Replace a launcher and login entry recorded under an app's old desktop id when an update changes the id (Find's `arcade-find` → `dev.arcade.Find`). Updates no longer leave a second menu entry, or a second login item that started the app twice. Start at login, and the desktop's own switched-off choice, carry over to the new entry; an old entry another app now owns is left alone.
- Add `design::format::format_datetime` for exact local timestamps in detail views (`2026-10-09 14:32`).

## 0.4.11 — 2026-10-30

- Keep the egui welcome mark neutral, using the same sunken tile and text ink as the shared glyph tiles.

## 0.4.10 — 2026-10-30

- Keep enabled unchecked egui checkboxes visible without hover. Their painted boundary now meets 3:1 against every neutral surface in both themes.

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
