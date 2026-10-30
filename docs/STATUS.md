# Arcade Link status

## 2026-10-30: repeated egui row labels and disabled text (0.4.8)

Find's real Settings matrix exposed repeated layout discards on Shortcuts.
Different key combinations share the same title, so title-only measurement
ids collided. Rows now include their UI occurrence in the cached identity.
The duplicate-name regression fails on 0.4.7 and passes with this change.

A test of actual rendered disabled glyph vertices also fails on 0.4.7 at
3.37:1 in the light theme. Components already choose disabled inks; egui's
extra opacity reduction is now removed. Both themes pass the rendered check.

Local: 19 targeted egui tests, all-features workspace Clippy with zero warnings,
and design generation/contrast checks pass. Full native three-OS, Flutter,
Qt and installer CI remain required before merge/tag. Find must rerun and
review its real native matrix after adopting the tag.

## 2026-10-29: egui layout fixes (0.4.7 candidate)

Find's 48 real eframe screenshots exposed descriptions crossing row dividers,
long folder names wrapping repeatedly, and repeated layout-pass warnings.
Rows now reserve current content bounds before advancing to the next row,
request a repaint when another measurement is needed, and ellipsize names with
tooltips. New page helpers keep headings outside the body's scroll area.

The changed-description regression failed on 0.4.6. The focused component suite
passes locally on Linux. Native screenshots of Find using this candidate,
full three-OS Rust CI, Qt and Flutter checks remain required. No tag or release
is claimed by this entry.

## 2026-10-29: egui component set for Find and Lens (0.4.5 candidate)

The `design-egui` feature now exposes shared components alongside the existing
palette/style API. Find and Lens can use one shell, rows, controls, recorder
and feedback components while keeping their own native surfaces. Real Inter
500/600 instances supply the specified weights; hashes and SFNT weight tables
are checked without adding a runtime or CI Python dependency.

Local Rust 1.88 verification: generated design/contrast checks, all-features
workspace Clippy with `-D warnings`, and the focused design/component tests.
The layout tests render egui shapes at 720×520, in both themes and at 100% and
125% scale; switch label/keyboard/disabled and busy-button behavior are checked.
Python's 43-test suite completes with its two pre-existing optional
schema skips.

The full workspace test command has not passed locally: two inherited-file-
descriptor tests inspect `/proc/$$/fd` in a shell child; this environment's
PID namespace resolves that numeric PID to a different host process. Keep the
tests unchanged and require the native Linux/Windows/macOS CI matrix before
merging/tagging. Qt and Flutter verification is also required from CI; no
native desktop or release is claimed here.

Review pass (same day): the components were rendered in a real eframe window
under Xvfb (light and dark at 760×560 and 720×520, dark at 125 % text) and
compared with `design/css/components.css`. That found layout bugs the earlier
tests could not see: right-to-left rows reversed multi-part controls (the
segmented theme picker read Dark, Light, System; the shortcut recorder put
Reset first and the keys backwards) and let a wide control cover the row
text; buttons inside wrapping rows shrank and overflowed instead of wrapping,
which widened the whole page; key chips stretched to the full line; and the
busy spinner moved the layout cursor so the next button overlapped. Rows now
measure their control and stack it below the text when it doesn't fit, and
the colours, borders, pills, notices, toast and empty state follow the CSS kit.
New tests fail on the earlier code: control order and placement beside the
text, a busy button beside another, and a wrapped row of every button kind
inside the 720×520 overflow check.
A second pass on a realistic Find settings page (page and section actions,
location rows, the shortcut recorder, notices with Retry, danger rows) centred
both sides of every row and head on each other, moved buttons to the CSS
padding, made the shortcut recorder a compact 28 px control beside its label,
and gave selects the 32 px control height and kit chevron.

## 2026-10-29: shared foreground contrast correction (unreleased)

Look's adoption checks found light status text below 4.5:1 and metadata using
`faint` in both themes. The kit now derives readable text inks and maps its
shared components to them, retaining the existing fill palette. Small metadata
uses `muted`. Regression checks cover neutral, hover, sunken and matching soft
surfaces for every app accent and theme. No protocol change or release has been
published for this correction.

## 2026-10-28: v0.4.0 (design kit)

Protocol 1 and manifest schema 1 are unchanged. v0.4.0 adds the design kit
(`design/`, see its README), the `design`/`design-egui` features, the shared
format helpers, `paths::{app_dirs, legacy_dirs, migrate_app_dirs}` and the
diagnostics header (Rust and Qt), and a stricter harness (output checks in the
stress and leak runs, development builds and `--require-artifacts` in the
nightly real-fleet run). Verified locally on Linux: `design/generate.py
--check`, `cargo clippy --all-features -D warnings`, `cargo test` with every
feature, the Qt module's seven suites (including the QML kit under Qt Quick
Test) and the Dart kit under `flutter analyze --fatal-infos`. CI repeats them on
Linux, Windows and macOS. Apps adopt the kit one by one with their overhaul
specs; until then they keep their own styling.

## v0.3.0

Updated 2026-10-15 for v0.3.0, protocol 1 and manifest schema 1.
[PR #3](https://github.com/qa-p1/Arcade-Link/pull/3) is merged and v0.3.0 is
tagged.

## Implemented

Rust and Qt provide optional app control methods, feature detection, tray
hosting, canonical accelerators, shortcut documents/sheets, receipts and
installation/platform helpers. Existing APIs and the original accelerator
vector pairs remain compatible. The CLI and mock peers exercise the new
methods and hosting; fixtures include menu/docs/methods and Tools' hosting
settings. Shared Windows installer includes, dialog strings, release metadata,
all eight SVG glyphs and their 16/22/24/32/48 PNG renders are present.

Qt watches the registry/socket off the GUI thread and delivers queued signals
on its owning thread. Its conformance and interop tests cover all vectors,
Qt clients against Rust methods, Rust clients against Qt methods and the Qt
watcher against the Rust fake host. `tools/vendor_qt.py` creates/verifies a
complete commit/checksum-pinned bundle for app adoption.

The isolated `presence,trayhost,install,shortcuts` e2e selection uses mock
peers, crate-backed watchers and a temporary AppImage harness. Existing real
fleet groups remain available, with missing/stale binary diagnostics. Bench
probes measure tray hand-back and scaffold host idle/overlay instrumentation;
the mock overlay paints nothing and does not establish production budgets.
The shared new-app brief is mirrored in `NEW_APP_SPEC.md`.

## Verified

Linux local verification uses system Rust 1.99, C++20/Qt 6.11 and Python 3.14.
Format, Clippy (including locked), workspace tests, each feature alone,
Python tests, five Qt test executables, isolated e2e and benchmark scaffolds
pass. Find and Tools compile unchanged against a patched Link in temporary
source copies; their original checkouts remain clean.

[Native CI](https://github.com/qa-p1/Arcade-Link/actions/runs/38154982039)
passes Rust 1.88 and Qt 6.8 on Linux, Windows and macOS, plus silent Inno/NSIS
install/uninstall. Installer CI also covers old-copy uninstall after a newer
installation and isolated PATH identity parity. Later updates and the current
head's check results are on [the PR checks page](https://github.com/qa-p1/Arcade-Link/pull/3/checks).

The [implementation report](dev/history/V0.3_REPORT.md) gives commands/output tails,
regression test names, focused commits and current CI evidence.

## Limits

- The real-fleet e2e groups (`install`, `shortcuts`, `tools-hub`,
  `trayhost`) run on main in the scheduled
  [Ecosystem e2e](https://github.com/qa-p1/Arcade-Link/actions/runs/38698730640)
  workflow, under CI X11 only. Every app now ships user docs, including
  `docs/user/shortcuts.json`. Real desktop dialogs, tray icon painting and
  production Tools/overlay performance remain owner checks on a real desktop
  (see [the benchmark report](dev/history/benchmarks-0.3.md)).
- One Rust engine-download test is intentionally ignored. Two optional Python
  JSON Schema tests skip without `jsonschema`; semantic/shared-vector tests
  and canonical key schema-regex checks run without extra dependencies.
- Linux rollback covers ordinary I/O failure across multiple paths, not
  power-loss atomicity. Cache refresh is non-fatal; repair cannot recreate
  missing icon bytes without the app's bundled assets.
- Native CI is not an interactive desktop check. macOS ditto retains
  quarantine; signing/notarization, Gatekeeper and real translocation remain
  app release checks. Windows receipts use private user/SYSTEM ACLs, not
  Unix permission bits.
- Same-user IPC authentication does not prove app identity. Apps own secret
  exclusion, imported-settings validation/backups, live shortcut registration,
  persistence, dialogs and restart successor waiting.

No real profile, shell startup file, autostart, icon or Hyprland configuration
was changed. Every mutating check uses explicit temporary roots or scoped
ARCADE_HOME/XDG values, and only started child processes are stopped.
