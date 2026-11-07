# Arcade Link v1 progress

Detail for `qa-p1/Arcade-catalog` › `dev/v1/PROGRESS.md`. Newest first.

## 2026-11-07

Open PR: https://github.com/qa-p1/Arcade-Link/pull/32 (branch `claude/tender-feynman-3vt7dg`).

Done on the branch (each with tests; local runs green):
- §3 release primitives: `arcade_link::release` + CLI `release-manifest`, `promote-manifest`, `verify-manifest`,
  `keygen`; minisign interop; fuzz target `fuzz/`; `docs/SIGNING.md`.
- §2 protocol: legacy hello = protocol 1; N-1 CI job (`tools/n1_compat.py`); SO_PEERCRED/getpeereid UID check in
  the Rust and Qt servers (root-only test, run with sudo in CI).
- §2 shared runtime: `arcade_link::runtime` (RT-1…RT-5), `qt/arcade_trim.h`, `packaging/linux/arcade-trim-preload.c`.
- §2 QML dialog: Escape cancels destructive dialogs (WH-5); regression test fails on the old policy.
- §5.1/5.2 `ARCADE_APP.md` (+ `docs/ARCADE_APP_GUIDE.md`, former NEW_APP_SPEC) and `arcade-link conformance`;
  baseline of the 8 released apps in catalog `dev/v1/AUDIT/conformance-baseline.md`.
- §4 reusable workflows: `app-publish.yml`, `app-promote.yml`, `app-nightly.yml`, `app-checks.yml`,
  `.github/actions/setup-arcade-link`, logic in `tools/release_flow.py`; `release-flow-e2e.yml` (drafts, dispatch-only
  once on main).
- §1 harness: `desktop-e2e.yml` (reusable), `tools/desktop/` (session, driver, vpointer, portal stand-in),
  `desktop-harness.yml` proof jobs. Not yet confirmed green in CI.

Next: confirm harness in CI (fallback: sway headless) · Tier 2 runners · fleet memory job · template/ + skill ·
untangle/dead code/SECURITY/deny · design kit items (emoji fallback, contrast) · CLI unknown-arg exit 2 · v1.0.0.

### Earlier

- Step 0: copied `specs/LINK.md` to `SPEC.md` and the relevant audit text (FINDINGS, memory, real-desktop, RESEARCH,
  memory tools) to `audit/`. Decisions D1–D3 recorded. Baseline: main `8751969`, CI and the scheduled ecosystem e2e
  green.
