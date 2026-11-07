# Arcade Link v1 progress

Detail for `qa-p1/Arcade-catalog` › `dev/v1/PROGRESS.md`. Newest first.

## 2026-11-07

Open PR: https://github.com/qa-p1/Arcade-Link/pull/32 (branch `claude/tender-feynman-3vt7dg`).

Done on the branch (each with tests; local runs green): see the PR description for the full list. Highlights:
release primitives (§3), runtime incl. U1 (§2/2b), peer UID check + legacy hello + N-1 job, QML Escape fix, emoji
fallback, ARCADE_APP.md + conformance (+ baseline), template/ + skill, reusable publish/promote/nightly/checks
workflows, desktop harness (Hyprland + Tier 2), fleet memory job, contract suite, SECURITY/deny/notices, dead code,
version 1.0.0 + CHANGELOG.

State at 13:20 UTC:
- Link CI green except as noted in the latest run; Tier 2 smoke green on sway, KWin, Mutter, X11 (Find 0.5.4).
- Hyprland harness: first cached build of 0.56.1 in progress (D10). Proof jobs: selftest, popup-fixed (exit 0),
  popup-broken (exit 10), clipboard-c0 (exit 10 = C0 reproduced on Clipboard 0.3.2).

State at 14:10 UTC (harness bring-up, each found on CI and fixed):
- seatd was never installed (the azure kernel has no `linux-modules-extra`; vkms is built in) → installed alone.
- The Nix cache never restored (root-owned multi-user store; tar EACCES) → `nix-quick-install-action`, key `-2`.
- GBM had no driver (`/run/opengl-driver` exists only on NixOS) → Mesa from Hyprland's own nixpkgs revision.
- Also fixed on the way: macOS lock-test fork race; anonymous API 403 in contract/fleet fetch; `| tee` without
  pipefail hid a failing conformance gate in `app-checks.yml` (now `shell: bash`).

Remaining before tagging v1.0.0:
1. Harness proof green (else the sway fallback per PLAN.md "When blocked").
2. Merge PR #32; run "Tag a release" with `v1.0.0` and the merge SHA.
3. Dispatch `release-flow-e2e.yml` on main (draft publish → promote with a throwaway key).
4. Commit `template/Cargo.lock` (resolved against the v1.0.0 tag).
5. Catalog PROGRESS: Link done; start Tools (`specs/TOOLS.md`).

### Earlier

- Step 0: copied `specs/LINK.md` to `SPEC.md` and the relevant audit text (FINDINGS, memory, real-desktop, RESEARCH,
  memory tools) to `audit/`. Decisions D1–D3 recorded. Baseline: main `8751969`, CI and the scheduled ecosystem e2e
  green.
