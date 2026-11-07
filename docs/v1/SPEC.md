# Spec 1: Arcade Link 1.0.0 and the Arcade app standard

**Outcome:** Link `v1.0.0` is tagged. It holds every piece of shared runtime code the apps duplicate today, the
release and update primitives Tools needs (signed manifests, the chunk format), the desktop end-to-end harness, and the
Arcade app standard in four parts: `ARCADE_APP.md`, a template repo, a conformance checker, and a `/new-arcade-app`
skill. `NEW_APP_SPEC.md` is replaced (its content is kept inside `ARCADE_APP.md`).

Starting point: `qa-p1/Arcade-link` main at `8751969` (workspace version 0.4.15). It contains the crates
`arcade-link` (accelerator, app, childenv, client, content, design, diagnostics, endpoint, engines, handoff,
install, logs, manifest, oneshot, paths, presence, receipt, registry, server, shortcuts, transport, trayhost, wire)
and `arcade-link-cli` (with a mock and a tray), plus `design/` (tokens, CSS, Dart, QML, fonts, icons) and the
workflows `ci.yml`, `e2e.yml` and `tag.yml`.

## 1. Desktop end-to-end harness (do this first; it unblocks every gate)

1. A reusable workflow, `.github/workflows/desktop-e2e.yml` (`workflow_call`), that boots **headless Hyprland**
   with `AQ_NO_KMS_REQUIREMENT=1`, so it creates the virtual `HEADLESS-0` output.
   - Precedent: `hyprwm/Hyprland` `nix-test.yml`, and `RESEARCH.md` §headless.
   - Pin the Hyprland major and minor version to the owner's (0.56.x).
   - Try a plain container or Ubuntu build first. If that fails, use Nix with the hyprland.cachix.org cache.
   - The workflow takes the app's published AppImage URL (or a CI artifact) and a test script. It provides:
     - a private D-Bus and an XDG runtime;
     - a StatusNotifierWatcher (for the tray);
     - `wl-clipboard`, `grim`, a virtual pointer and keyboard (`wlrctl` or `ydotool` in the compositor);
     - `hyprctl` access.
   - It uploads screenshots and logs as artifacts.
2. A Python driver library, `tools/desktop/arcade_desktop.py`, with helpers:
   - `send_shortcut(window_address, mods, key)`, using `hl.dsp.send_shortcut`;
   - `pointer_move`, `click`, `drag` (press, moves, release), `screenshot`, `clients()`, `layers()`;
   - `clipboard_get/set`;
   - `tray_menu(app)` and `tray_click(app, label)` over dbusmenu;
   - `global_shortcut(id)`, with press and release both sent through the portal (the audit showed `hl.dsp.global`
     sends Activated only).
3. **Tier 2 smoke runners**, using the same driver API where possible: sway headless, `kwin_wayland --virtual`,
   `mutter --headless --wayland`, and Xvfb plus a minimal WM. Each starts every app, shows its window, and checks the
   tray or its fallback, the CLI and `--status`.
4. **Proof:** run the harness against today's released Clipboard. It must reproduce C0 (the picker closes on
   pointer leave with `follow_mouse=1`) and pass with a stubbed fix. Time box: one day. Otherwise use the sway
   fallback described in `PLAN.md`.
5. **Fleet memory job:** start all 8 apps (published or CI artifacts) under the harness, go through the states in
   `AUDIT/memory.md` (all background; background with one window open), and sum PSS and RSS per process tree,
   including children and stubs. Fail if the total is over 500 MB or an app is over its budget. The budgets come from
   `AUDIT/memory.md` and are recorded in `ARCADE_APP.md`.

## 2. Shared runtime moves into Link

Inventory every app's own copy first, and record it in `docs/v1/DECISIONS.md`. Then give Link one tested
implementation, and move the Rust apps and Clipboard's Rust core onto it in their own specs:
- install and desktop integration;
- launch and restart, which re-executes `$APPIMAGE` or the installed launcher, never a mount-internal binary;
- child-process spawning that always reaps;
- environment sanitizing;
- single instance;
- the tray host and client;
- logs with levels, rotation and a noise filter;
- local crash reports (panic hook plus signal handler writing a report under `arcade/<app>/crashes/`);
- `--status` and CLI argument conventions;
- XDG paths and one-time legacy-folder retirement.

Shelf and Wheel (C++) keep their own implementations but must pass the same **contract tests**. Write those tests
as a language-neutral suite in `spec/contract/`, run against the built binary.

Also in Link:
- **Protocol version.** Add a protocol version to the wire handshake. Peers negotiate the version and reject
  unknown majors with a clear error. A peer that sends no version (every 0.4.x app) is treated as the legacy
  protocol and keeps working, because apps ship one at a time. Add an N-1 compatibility test that runs the previous Link release's mock peer
  against the new one.
- **IPC peer check.** Sockets stay 0600 in a 0700 directory. Also add an `SO_PEERCRED` UID check on accept, and
  reject other UIDs.
- **Shared QML dialog** (`design/qml/Arcade/Dialog.qml:21`). Escape cancels destructive dialogs. Only `busy`
  disables Escape. This fixes Wheel WH-5 and the same bug in Shelf.
- **Design kit tokens:** the colour emoji fallback font (Clipboard CB-10). Also, the a11y contrast pairs are
  checked in `generate.py` (WCAG AA).

## 2b. Memory runtime (`AUDIT/memory.md` §4, items U1, W0, W1, OPEN1, W3b)

- **U1, unpacked runtime.** The AppImage stays the single distributed and verified file. On first run its runtime
  extracts once to `$XDG_DATA_HOME/arcade/runtime/<app>-<ver>-<hash>/`, then `execv`s `AppRun` with `APPIMAGE` and
  `APPDIR` set. There is then no FUSE helper process: 112 MB RSS / 112 MB PSS saved on the real fleet, and cold
  start roughly halves. Tools garbage-collects old runtime folders.
- **W0, trim after init and after each hide.** One Link function does `malloc_trim` plus `madvise(DONTNEED)` on the
  app's own read-only executable mappings. A C twin serves Qt and Flutter.
- **W1, release after the last WebKit window closes.** SIGTERM the child `WebKitNetworkProcess`, then run W0.
  - Measured: Tools 203/104 → 60/38, Box 258/153 → 92/71, Look 250/144 → 80/59.
  - Reopen stays as fast or faster (0.20–0.30 s).
- **OPEN1, open-window trim.** The UI process trims itself 2 s after page-ready and every 15–30 s. WebKit's helper
  processes get the same through an `LD_PRELOAD` trimmer (`AUDIT/memory/tools/trimshim.c`) set in the AppRun hook.
  - Measured: Tools open 528/319 → 292–325 / 210–228.
- **W3b, page-out of hidden WebKit heaps.** `MADV_PAGEOUT` on the anonymous heap of a hidden WebKit app, saving
  about 60 MB fleet-wide. It's invisible unless the machine is under memory pressure.
- **CI check:** `AUDIT/memory/tools/fleet_budget.py` with `budgets.json` becomes the §1.5 fleet memory job. Budgets
  are from `AUDIT/memory.md` §6.1 (PSS/RSS MB):

  | Tools | Box | Look | Find | Lens | Shelf | Wheel | Clipboard |
  |---|---|---|---|---|---|---|---|
  | 30/50 | 60/80 | 50/70 | 42/48 | 8/8 | 40/48 | 30/50 | 105/130 |

  The fleet totals must stay under 500/500 in the background.
- All numbers are labelled RSS/PSS, and Tools Home shows tree PSS, not single-process RSS (TD1).

## 3. Release and update primitives (used by Tools and every repo's CI)

- **`arcade-link release-manifest`.** For a set of release files, it writes `manifest.json`: version, channel, and
  per file the size, SHA-256 and FastCDC chunk list (8/16/32 KB, BLAKE3 per chunk, as measured in `RESEARCH.md`).
  It signs the manifest with ed25519 into `manifest.json.sig`, minisign-compatible.
  - The key comes from the env var `ARCADE_UPDATE_SIGNING_KEY`.
  - Verification takes a list of trusted public keys (current and next).
- **Library API, used by Tools:** `verify_manifest`, `plan_delta(old_file, manifest) -> missing chunk ranges`, and
  `assemble(chunks) -> file + verify`. It falls back to a full download on any mismatch.
  - Fuzz the manifest parser.
  - Test tampered chunks, a reordered manifest, a wrong key and a truncated file.
- **macOS and Windows:** CI also publishes the uncompressed app archive (`.app.tar`, `.zip` with store
  compression), so they can be chunked.

## 4. CI shape for every repo (template and reusable workflows)

Reusable workflows in Link `.github/workflows/`, called by every repo:
- `ci.yml` (PRs):
  - lint, tests (sharded where it pays off), conformance;
  - one Linux package build, uploaded as an artifact, then the desktop end-to-end tests on that artifact.
  - Caches: sccache or rust-cache keyed per target; Flutter pub; Qt via `install-qt-action` with cache. Caches are
    saved only on main.
  - Target under 8 minutes warm.
- `release.yml` (tag):
  - build every platform once in parallel;
  - write the manifest (unsigned for `test` and `nightly`);
  - publish as a prerelease on the `test` channel.
  - Target under 15 minutes.
- `promote.yml`: copy the rc assets byte-for-byte to the stable release, sign the manifest and verify the hashes.
  Under 5 minutes.
- `nightly.yml`: a schedule that skips when main hasn't changed since the last nightly tag. It promotes main CI's
  already-tested artifact to `X.Y.Z-nightly.YYYYMMDD.N` with no rebuild. Under 5 minutes.
- Every `apt-get` and network step has `timeout-minutes` and a retry (CI1: the Shelf apt step hung for 25 minutes).

## 5. The Arcade app standard

1. **`ARCADE_APP.md`:** short and normative. It replaces `NEW_APP_SPEC.md` and keeps its content. It has MUST /
   SHOULD items, each one with the conformance check ID that enforces it:
   - identity: `dev.arcade.<App>` app_id, window class and desktop file; one launcher entry and one autostart entry;
   - CLI and `--status`; paths; logs; crash reports; the Link registration and protocol version;
   - the tray menu shape; global shortcut and Hyprland binding behaviour;
   - the Escape and popup rules; single instance; restart via `$APPIMAGE`; child reaping;
   - the design kit and the accessibility basics;
   - memory and performance budgets;
   - data schema version and migrations;
   - LICENSE, `THIRD_PARTY_NOTICES`, `SECURITY.md` and `CHANGELOG.md`;
   - the CI workflows, release channels and signed manifests.
2. **`arcade-link conformance <app-dir|AppImage>`.** It checks everything that can be checked statically or by
   running the binary under the harness, and prints a report with the IDs. Every app's CI runs it.
3. **Template repo `qa-p1/Arcade-template`.** A minimal app (Rust + egui, the lightest toolkit, with notes for the
   other toolkits) that builds, tests, passes conformance, releases rc → stable and registers with Tools. It is
   marked as a GitHub template.
4. **Skill `skills/new-arcade-app/SKILL.md`** (in Link). It goes from a vague idea to a released app:
   - clarify the idea into a one-page spec;
   - pick the toolkit;
   - generate from the template;
   - implement against `ARCADE_APP.md`;
   - make conformance green;
   - cut an rc and promote;
   - add the app to `catalog.json`.

   Keep the skill short. It points to `ARCADE_APP.md` rather than repeating it.

## 6. Link's own v1 work

Link gets the same loop as every app:
- behaviour lock;
- untangle;
- dead code;
- CI under 3 minutes;
- `SECURITY.md`, `cargo deny`, docs.

Then release `v1.0.0`. Every app pins Link by tag (`v1.0.0` or later patches), never by rev.

## Acceptance

- The harness proof (§1.4) passes, or the documented fallback is in place.
- The release-manifest round-trip tests pass, including the tampered-chunk ones.
- Conformance runs against all 8 currently released apps and produces a report, saved to
  `AUDIT/conformance-baseline.md`. It's expected to fail today; it's the to-do list for the app specs.
- The template repo releases an rc and promotes it to stable successfully.
- The Wheel and Shelf contract suites run in CI (they may fail until their specs).
- Link `v1.0.0` is tagged, with its CHANGELOG.
