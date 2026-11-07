# v1 research notes (facts, not decisions)

## T3 Code: CI, release and update (measured 2026-11-06)

Source: github.com/pingdotgg/t3code, workflows `release.yml` (1491 lines), `release-desktop.yml` (852), `ci.yml` (638).
Copies are in the session scratchpad under `t3ref/`.

- **Release run 42182419098** (scheduled nightly) took 9 minutes wall-clock in total:
  - resolve commit and preflight: about 0.5 minutes
  - 8 sharded server-test jobs, web tests, quality checks and the JS bundle, all in parallel: about 3.5 minutes
  - six desktop builds in parallel: Linux x64 1.8 minutes, Windows 2.6–2.8, Linux arm64 2.9, macOS 3.9–4.4
  - verify: about 1 minute
  - publish: about 1.4 minutes
  - npm, AUR, web deploy and Discord announcement, in parallel
- **Why it's fast.** Electron apps compile nothing native per platform. The JS bundle is built once ("Build JS bundle",
  0.8 minutes) and every platform job packages it with a prebuilt Electron runtime. Native helpers are cached
  (`resource_monitor_cache`, `capture_helper_cache`). The pnpm store is cached, and only gets saved when new content
  appears. Tests are split into 8 shards.
- **Scheduled nightly with a skip.** It runs on a schedule; the "Resolve release commit" and "Preflight" steps end the run
  in under 1 minute when nothing changed (the many 0-minute scheduled runs).
- **Channels.** Nightly is a separate GitHub prerelease, tagged `v0.1.8-nightly.YYYYMMDD.N`. The app is built with
  `app-update.yml` set to `channel: nightly` and `releaseType: prerelease`, and it reads its own feed files
  (`nightly-linux.yml`, `nightly-mac.yml`, `nightly.yml`). It's a separate app identity ("T3 Code Nightly",
  `T3-Code-Nightly.AppImage`), so nightly and stable can be installed side by side.
- **Updates are differential (electron-updater).** Each installer is published with a `.blockmap`, about 0.1% of its size
  (200 KB for 200 MB). The AppImage carries its blockmap inside the file; the feed lists `blockMapSize`.
  Blockmaps use content-defined chunks (8–32 KB, median about 19 KB; 15,470 chunks for the Windows exe).
  The updater compares the old and new blockmaps, downloads only the changed chunks with HTTP Range requests, rebuilds
  the file, checks its sha512, and keeps a pending update in `~/.cache/t3code-updater/pending/`. Nightly updates feel
  instant because the 200 MB Electron runtime rarely changes; usually only `app.asar` does.

## Arcade measured against that

Content-defined chunking (FastCDC, 8/16/32 KB, BLAKE3) over the real published AppImages. Tool:
`scratchpad/cdc/src/main.rs`.

| Update | Full file | Download needed |
| --- | --- | --- |
| Look 0.4.1 → 0.4.2 | 157.0 MB | 9.0 MB (5.7%) |
| Look 0.4.2 → 0.4.3 | 157.0 MB | 8.9 MB (5.7%) |
| Tools 0.4.5 → 0.4.6 | 93.9 MB | 9.1 MB (9.7%) |
| Tools 0.4.4 → 0.4.6 | 93.9 MB | 9.5 MB (10.1%) |
| Box 0.4.1 → 0.4.2 | 119.7 MB | 31.5 MB (26.3%) |

Sharing between apps, meaning what a fresh install would need if Tools kept one chunk store for every app:

| Already have | Install | Download needed |
| --- | --- | --- |
| Tools | Look | 81.0 of 157.0 MB (52%) |
| Shelf | Wheel | 11.1 of 48.3 MB (23%) |
| Tools or Look | Box | 118.5 of 119.7 MB (99%). Box's AppImage is packaged differently from the other Tauri apps. |
| Tools | Lens | 14.1 of 15.1 MB |

- GitHub release assets are served from object storage that supports HTTP Range requests, so chunked downloads need
  no server of our own.
- The Arcade repos are public, so standard GitHub-hosted runners are free. Measured Arcade CI (see `AUDIT/FINDINGS.md`
  CI2) is dominated by Rust/C++/Flutter compilation, which Electron doesn't have. The equivalents are: build once and
  promote the same bytes, cache compiled dependencies, shard tests, and run nightly only when something changed.

## Real-desktop checks without a human: headless Hyprland in CI

- Hyprland's own CI (`hyprwm/Hyprland/.github/workflows/nix-test.yml`) runs the real compositor on `ubuntu-latest`
  GitHub runners. It builds a NixOS test VM (`#checks.x86_64-linux.tests`) and runs `hyprtester` plus gtests inside it,
  using a binary cache (hyprland.cachix.org). That's the precedent for running Hyprland in Actions.
- The Hyprland wiki's Virtual GPU page says that with `AQ_NO_KMS_REQUIREMENT=1` and no display output, Hyprland creates a
  virtual `HEADLESS-0` output at 1920x1080@60. A plain container route, without Nix, may also work; that still needs
  to be proven.
- Forum thread "Headless Hyprland in Github Actions" (forum.hypr.land/t/headless-hyprland-in-github-actions/1361):
  it isn't trivial, and the maintainers point to the Nix test job.
- What this means for the plan: Link owns one reusable desktop end-to-end workflow that boots headless Hyprland, matching
  the owner's major version, and runs the app's real published AppImage. It drives the app with hyprctl, wtype or
  ydotool, and checks layer-shell or floating placement, popups surviving pointer leave, tray and global shortcuts, and
  screenshots. The generic part runs under Xvfb and private D-Bus (already used). Proving the harness is the first
  task of the Link spec.

## Claude Code cloud sessions: what they can do

Owner's experience (2026-11-06), which overrides the default limits in the public docs: with the Claude GitHub app
installed and full access granted, cloud sessions (including from the mobile app) have merged PRs, created GitHub
releases, and run Arcade apps on Linux with screenshots. Sessions may do all of that directly.

Design choices kept on their own merits:
1. **Releases still go through each repo's own workflows** ("Tag a release" → "Release" → verification), so the same
   tested files get promoted, and the release history is reproducible whoever triggers it.
2. **`docs/v1/PROGRESS.md` in every repo** is updated after each step, so a new session can pick up where a previous
   one stopped (usage limits, an interruption).
3. **One environment setup script** with everything installed (Xvfb, D-Bus, WebKitGTK dev, Qt 6, Flutter, Android SDK,
   headless Hyprland test dependencies), so each session doesn't reinstall them. It's optional if the current
   environment already has them.
