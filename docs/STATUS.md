# Arcade Link: status

Review: 2026-10-11. This branch prepares Rust/CLI v0.3.0 with protocol 1,
manifest schema 1, and source compatibility for the v0.2 public API.
The apps still pin published v0.2.0. No tag, push or merge is part of this work.

## Implemented and verified

The Rust library implements optional app settings/restart/menu/rebinding/
settings transfer, manifest extensions, accelerator notation, shortcut docs
and sheets, and receipt validation/storage. Optional features are install,
trayhost (which includes watch), watch and engines; defaults remain empty.

Linux temp-root tests cover install/update/previous/repair/uninstall,
rollback after integration failure, duplicate launcher removal, autostart
policy, PATH hints, receipt privacy/atomicity and dev detection. IPC tests
cover every optional method and old-server unsupported detection, mock
methods, Tools hosting/crash fallback, exclusions, Link-off, startup cap and
restart grace with and without a replacement host. Callbacks run on workers;
no idle polling is used. These are synthetic process/socket tests, not
interactive desktop validation or a measurement of whole-app idle cost.

The full required local check set, output tails, public API and design
choices are recorded in [the v0.3 report](V0.3_REPORT.md). Local checks use
system Rust 1.99. CI is configured for Rust 1.88; no rustup was used here.
Find and Tools were checked against this crate in disposable source copies,
leaving the original repositories unchanged. The existing Qt module still
builds and passes its two tests; its v0.3 install/tray/shortcut port is pending.

Windows pipe deadlines/cancellation use overlapped I/O and OS event waits,
covering closure before and during a read without polling. The transport
backend was reviewed against the cached windows-sys/interprocess interfaces
and Microsoft documentation; native execution still requires CI.
Windows user PATH/shim helpers have isolated-HKCU native tests; macOS bundle
moves/CLI helpers have temp-root native tests. The shared Inno/NSIS includes
and PowerShell helper have a windows-latest job that compiles two sample
installers, installs twice silently, verifies shim/receipt/tasks/metadata,
and uninstalls with last-shim PATH removal. None of these new native checks
has run on this Linux host or GitHub yet. The earlier v0.2 native CI evidence
in [the historical completion report](../COMPLETION_REPORT.md) does not verify
these additions.

## Limits

- New Windows/macOS builds, installer execution and MSRV 1.88 require CI.
  Windows/macOS interactive app flows, Wayland UI and actual app self-install
  flows are not verified by these library tests.
- Qt v0.3 modules, app adoption, new ecosystem e2e groups, benchmarks, glyphs
  and NEW_APP_SPEC updates belong to later phases.
- Linux rollback restores files after ordinary I/O failure; the operation
  spans multiple paths and is not a power-loss transaction. Cache refresh
  failures are non-fatal. Repair cannot regenerate missing icon bytes from
  a receipt; apps can reinstall their bundled icons.
- macOS move uses ditto to preserve quarantine and bundle metadata. Real
  translocation, signing/notarization and Gatekeeper flows await native runs.
- Same-user transport authentication does not establish application identity.
  An app remains responsible for validating imported settings, excluding
  secrets on export, persisting rebinding and applying its existing safeguards.
- Optional JSON Schema validation tests skip when jsonschema is absent; the
  stdlib semantic/vector tests and canonical-key schema-regex tests still run.

## Documents

- [README](../README.md): API, CLI, feature and build overview.
- [SPEC](../SPEC.md): current protocol and installation/shortcut contract.
- [v0.3 report](V0.3_REPORT.md): commands, results, API and implementation decisions.
- [Windows packaging](../packaging/windows/README.md): vendoring and installer interface.
- [Historical completion report](../COMPLETION_REPORT.md): prior ecosystem evidence.
- [New-app brief](../NEW_APP_SPEC.md), [ecosystem plan](../ARCADE_ECOSYSTEM_PLAN.md)
  and [benchmarks](../benchmarks/baseline.md): earlier-phase guidance and measurements.
