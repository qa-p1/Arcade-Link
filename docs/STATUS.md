# Arcade Link status

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
