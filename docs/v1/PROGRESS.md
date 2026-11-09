# Arcade Link v1 progress

Detail for `qa-p1/Arcade-catalog` › `dev/v1/PROGRESS.md`. Newest first.

## 2026-11-09 (1.0.6)

**v1.0.5 tagged** on 601af05 (PR #53). Box's first Tier 1 run found the harness portal had no host app registry
(D12): **1.0.6 prepared**, the stand-in serves `Registry.Register` and the self-test checks it. Tag after CI,
Ecosystem e2e and the desktop harness pass on the merge.

## 2026-11-08 (after 1.0.4)

**v1.0.4 tagged** on a076adb (the merge of PR #51) through "Tag a release", after CI, Ecosystem e2e and the
desktop harness passed on it; Tools moved to it. Next: Tier 2 `smoke.py` polls `--status` (a fixed 8 s sleep
missed Tools' first start once on sway): **1.0.5 prepared** in PR #52, merged as 926341f. Before tagging,
`perf_budget.py`'s wakeup count was found unsound (Tools' open window read -2.3/s in CI: whole-tree differences
lose exited threads); fixed per thread for 1.0.5 too (11.2/s measured). Tag after CI, e2e and the harness pass on
that merge.

## 2026-11-08 (1.0.4)

**1.0.4 prepared** in PR #51 (version bump on top of ac5ea80, whose CI, Ecosystem e2e and harness passed after
GitHub's release-download outage of ~08:45–11:10 UTC): the desktop workflows run the app from `$RUNNER_TEMP` and cache
released AppImages by URL, `perf_budget.py`, the template's PNG `.DirIcon` and cached AppImage runtime. Tag after the
merge commit passes CI, e2e and the harness on main.

## 2026-11-08 (1.0.3)

**v1.0.3 tagged** at 4cf9eff (CI, Ecosystem e2e and the desktop harness green on that commit). Follow-up: template
and reusable `link-ref` defaults → v1.0.3, and the desktop workflows download the app to `$RUNNER_TEMP` (inside the
caller's checkout it counted as a dev build and refused `--install`; Tools' Tier 1 test now copies it out anyway).
The workflow fix reaches apps with the next Link tag. Added `tools/fleet/perf_budget.py` and Tools' `perf`
budgets (local: cold start 690 ms, show 614 ms, idle CPU 0.0 / 0.4 / 0.03 % background / open / closed, wakeups
0.2 / 2.2 / 0.4 per second, disk 370 MB = 88 AppImage + 282 unpacked); `app-checks.yml` gets a budgets job when
Box, the next app, adopts it. Known: Tauri's AppImage keeps three copies of a few versioned libraries (librsvg,
libgio, ~20 MB of the unpacked copy); not worth a repack step now.

**v1.0.2 tagged** at 3e4aa6c (CI, Ecosystem e2e and the desktop harness green on that commit); template and the
reusable workflows' `link-ref` moved to it (#49). **1.0.3 prepared** (#50): W3b page-out covers every private mapping
of a hidden app (Tools' CI after-close PSS was 36 vs 30), and Tools' window-open cap comes from CI (Tools D10).

## 2026-11-08 (1.0.2)

**1.0.2 prepared**: PR #47 merged (19a4025) with the fixes found in Tools' Round 5: design-kit tab-panel focus ring,
Qt tray startup race, trim that drops clean `r--p` data and no longer reverts written code pages, and a clean
uninstall (lock file retired under the lock; empty icon/desktop caches removed), each with a test that failed first.
Version 1.0.2, CHANGELOG dated. Next: release PR to main, desktop harness on main, "Tag a release" `v1.0.2`, then
the template and Tools move to it.

## 2026-11-08

**1.0.1 prepared** on the branch: version 1.0.1, CHANGELOG dated. Contents: the legacy-migration archive fix (Tools
T3) and the committed `template/Cargo.lock`. Next: PR to main, CI green, "Tag a release" `v1.0.1` with the merge
SHA, then move `template/` to `v1.0.1` with its lockfile (D7), and bump Tools. Later shared fixes go into 1.0.2.

## 2026-11-07

**v1.0.0 tagged** at `d60772c` (main, through "Tag a release"), 17:45 UTC. Gates on that commit: CI (Rust and Qt on
Linux, Windows, macOS; design kit; template build, AppImage and conformance; cargo deny; N-1), Ecosystem e2e,
release-flow e2e (draft rc → promote → signature → byte-identical, run 42397612055); Desktop harness and Contract on
`7b42736` (same code). `template/Cargo.lock` committed against the tag (D7). Next for Link: 1.0.1 with shared fixes
found while doing the apps (first: legacy folders with conflicting files are archived, not kept forever: Tools T3).

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

State at 17:25 UTC: Desktop harness all green on `7b42736` (run 42395710146): selftest, popup-fixed (exit 0),
popup-broken (exit 10), clipboard-c0 (exit 10: C0 reproduced on released Clipboard 0.3.2), Tier 2 sway/KWin/Mutter/X11.
Last harness fixes: Clipboard first-run setup driven by clicks (the switch row sits just above the button), and a
popup that turns back into the app's main window (same address) now counts as closed.

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
