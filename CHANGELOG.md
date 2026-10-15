# Changelog

Arcade Link follows the frozen protocol v1 and manifest schema 1. Apps pin an
immutable tag; a new tag never changes the wire format.

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
