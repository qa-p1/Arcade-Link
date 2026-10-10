# Arcade Link: status

Documentation review: 2026-10-08. Protocol v1 and manifest schema 1 remain
compatible. The existing apps pin published tag `v0.2.0` (2026-10-10, adds
Shelf and Find; see the [changelog](../CHANGELOG.md)); changes on `main` do
not move a tag.

## Implemented and verified

The Rust library and Qt module provide the registry, local transport, content
matching, invocation, lifecycle and conformance behavior documented in
[SPEC.md](../SPEC.md). Rust's optional features are `watch` and `engines`.
There is no always-running broker. Shared engine support currently provides
Tesseract discovery and an explicit per-user download.

Native Linux, Windows and macOS Rust/Qt CI builds and tests passed at
`e0e20c2` ([CI evidence](https://github.com/qa-p1/Arcade-Link/actions/runs/37795738247)).
The completed app integration passed 74/74 isolated ecosystem checks; detailed
commands, implementation commits, stress results and platform evidence are in
the [completion report](../COMPLETION_REPORT.md). These are recorded runs, not
a claim that a documentation edit reran the interactive suite.

## Limits

- Windows/macOS CI tests are not interactive app validation. Full Wayland
  screen/recorder flows still need a real-platform run.
- Whole-app benchmark regressions passed, but the data do not prove zero idle
  wakeups or isolate Link's one-MiB RSS target. Box recorded nonzero idle CPU.
- Same-user transport authentication does not establish application identity.
- Shared app lists and Tools mappings are explicit; new app onboarding requires
  the integration changes described in the [new-app brief](../NEW_APP_SPEC.md).
- Signed releases/minisign, targeted mesh delivery and remote invocation remain
  outside the completed scope. Each app's status file records its own release gates.

## Documents

| Document | Purpose |
|---|---|
| [README](../README.md) | Library/CLI overview and build commands |
| [SPEC](../SPEC.md) | Current protocol and integration contract |
| [New-app brief](../NEW_APP_SPEC.md) | Portable instructions to attach to a new app idea |
| [Completion report](../COMPLETION_REPORT.md) | Implementation results, evidence and remaining limits |
| [Ecosystem plan](../ARCADE_ECOSYSTEM_PLAN.md) | Historical plan with final results in §16 |
| [Benchmark record](../benchmarks/baseline.md) | Baseline, final measurements and caveats |
| [E2E check modules](../tools/e2e_checks/README.md) | Adding isolated ecosystem checks |
| [Assets](../assets/README.md) | Shared glyphs and integration tokens |
