# v1 audit findings

Real-use audit of the installed fleet on the owner's machine (CachyOS, Hyprland), started 2026-11-06.
Installed versions: Tools 0.4.6, Box 0.4.3, Look 0.4.3, Find 0.5.4, Lens 0.4.2, Clipboard 0.3.2, Shelf 0.4.0, Wheel 0.5.0.
Each finding names the app, what happens, how it was observed, and what has been confirmed so far.

## Per-app reports

- Box: `box.md` (isolated, 27 findings) and `box-real.md` (real desktop). Owner requirement **BR-0**: Escape inside a
  tool returns to the Island's search and never closes the Island.
- Look, Find, Lens: `look.md`, `find.md`, `lens.md`. Clipboard, Shelf, Wheel, Tools: `clipboard.md`, `shelf.md`,
  `wheel.md`, `tools.md`. Real-desktop follow-ups: `real-desktop.md`.

## Tools

- **T1 — Idle CPU and thread growth.** *Real-desktop recheck: background 0.02% (old process), 0.0% fresh. With the window open on Home it costs 6.1%, because `src-ui/tabs/Home.svelte:71` polls all apps every 2 s. Fix: poll only while visible, every 10 s or event-driven.* Original note: Background Tools (pid 1355, up 1h25m, no window) measured 8.9% CPU over 20 s
  while idle; the Nov 3 baseline was 0.22%. Its threads grew from 68 to 75 within minutes. Breakdown:
  29 `tokio-rt-worker` threads (five of them each busy about 17% of the sample), 6 `tools-tray`, 6 `arcade-link-con`,
  4 inotify watchers, 32 sockets, 16 eventfds, 12 epoll instances, and 6 open `/proc/<pid>/stat` handles.
  Suspect: more than one tokio runtime, leaked tray or Link connection threads, and a tight poll of other apps' `/proc` stats.
  Load before it was seen: five app installs between 16:42 and 17:19, then app restarts. Still to do: reproduce from a
  fresh start, and check whether CPU climbs with each install or Link registry change.
- **T2 — Installer cache never pruned.** `~/.cache/arcade/tools/installers` holds 1.1 GB, unchanged since Oct 25.
  Keep only what an in-progress or rollback operation needs.
- **T3 — Legacy data folders kept forever.** `~/.local/share/arcade-tools` (22 MB) and `~/.local/share/app.arcadetools`
  are never retired. Every start logs "Left … in place: … already exists with different contents" for
  `update-history.jsonl` and `webview/mediakeys/v1/salt`. Migration should finish once, merging or archiving the old
  folders, and stop logging on every start.
- **T4 — No `--status`.** `Arcade-Tools.AppImage --status` prints nothing. Every other Rust app prints one-line JSON.

## Look

- **L1 — Duplicate login and launcher entries.** Both `arcade-look.desktop` and `dev.arcade.Look.desktop` exist in
  `~/.config/autostart` and `~/.local/share/applications`. Look logs on every start: "old integration entries were kept:
  dev.arcade.Look.desktop already exists; both entries were kept". One identity, one entry; the legacy one gets retired.
- **L2 — AppImage size.** 157 MB (WebKitGTK bundled). Size reduction is a polish candidate.

## Clipboard

- **C0 — Picker closes when the pointer leaves it** (owner report, 2026-11-06). The Ctrl+Shift+Space picker popup
  closes as soon as the mouse leaves it, the same behaviour Look had and the owner had removed. A popup closes only on
  Escape, choosing an item, the shortcut again, or an explicit outside click. Leaving with the pointer or a transient
  focus change never closes it. Every app's popups and overlays are checked for this.
- **C1 — SIGBUS on quit.** *Resolved as CB-4: the restart successor runs from an unmounted AppImage. A normal `--quit` is clean on the real desktop too.* Original: At 17:48:24 the diagnostics log shows `[lifecycle] quitting`, and coredumpctl records a
  SIGBUS in `/usr/lib/arcade-clipboard/clipboard` at the same second. The process was killed with its session scope,
  so the AppImage mount probably went away first. Still to check whether a normal `--quit` also crashes, since a clean
  quit must never core dump.
- Identity recovery fixed in 0.3.2: a real-profile start shows no "Secure identity is missing" error.

## Wheel

- **W1 — Two cache folders.** `~/.cache/arcade/wheel` (the standard location) and `~/.cache/Arcade Wheel/Arcade Wheel`
  (Qt's default, from the organization and application name). Everything belongs under `~/.cache/arcade/wheel`.

## Find

- **F1 — Tray registration error.** *Downgraded: it was shutdown noise from Nov 3, and Find re-registers when Tools comes back.* Original: `tray: failed to register to the StatusNotifierWatcher: NoReply`, followed by
  "other error during loop operation" and a broken-pipe IO error. Possibly from session shutdown; still to confirm
  whether Find re-registers when the watcher comes back.

## Fleet-wide

- **X1 — Log noise.** Box and Look logs are mostly GTK accessibility-bus warnings ("Can't connect to a11y bus") and
  "Gdk-Message: Error reading events from display: Broken pipe". Real errors are hard to spot. Logs should have levels
  and drop known-harmless toolkit noise.
- **X2 — AppImage sizes.** Look 157 MB, Box 120 MB, Tools 94 MB, Shelf 52 MB, Wheel 48 MB.
  `~/Applications/Arcade/.previous` keeps another 487 MB for rollback (intended, one version each).

## CI

- **CI1 — apt steps have no timeout.** The Shelf development build in the Link real-fleet end-to-end run
  (42213071676) hung in "System packages" for more than 25 minutes. Every apt step needs a timeout and a retry.
- **CI2 — Measured bottlenecks** (`.arcade-verification/ci-performance-20261105/REPORT.md`): Box Windows job about
  51 minutes, dominated by compilation; Look rebuilds everything again at release (74 runner-minutes per release);
  release Link CLI built twice in Box and Look (partly fixed); PR caches written where nothing reuses them (partly fixed).
  Main CI typical times: Box 47–61 min, Look 10–75 min, Tools 9–36 min, Wheel 8–37 min, Find 7–82 min,
  Lens 8–24 min, Clipboard 7–15 min, Link 3 min.

## Verification log (orchestrator, 2026-11-06 evening)

Checked report claims against the cited source lines. Confirmed means the code does what the report says.

- **Look (`look.md`, `real-desktop.md` RD-LOOK-1): confirmed.**
  - LK-1: `src/app.ts:593` skips the auto-dismiss timer for "Couldn't…" toasts.
  - LK-10 and RD-LOOK-1: `app.ts:779-789` handles fullscreen and then closes, ignoring the Info panel.
    `lib/actions-strip.ts:66-70` handles Escape even while the menu is closed and steals focus back.
  - LK-11: `src-tauri/Cargo.toml:43` builds the image crate without `avif`.
- **Clipboard (`clipboard.md`, RD-CLIPBOARD-1/2): confirmed.**
  - C0 = CB-3 = RD-CLIPBOARD-1: `desktop_adapter.dart:248-258`, where `onWindowBlur` dismisses after 400 ms.
    On Hyprland with `follow_mouse=1`, pointer motion alone closes the picker.
  - CB-4: `desktop_adapter.dart:194-199` restarts `Platform.resolvedExecutable` (inside the AppImage mount) instead of
    `$APPIMAGE`. **This explains C1:** both SIGBUS cores are the orphaned restart successor; a plain `--quit` was clean 5/5.
  - CB-1: the banner says `clipboard --overlay` (`desktop_adapter.dart:2053`).
  - CB-2: oversize clips, directories and missing files are dropped without a notice (`app_controller.dart:1427-1437`).
- **Shelf (`shelf.md`): confirmed.**
  - SH-4: the quota is MiB divided by 10^6 and labelled MB (`SettingsWindow.qml:192`).
  - SH-8: `peekDismissTimer` collapses a hover-peek when the pointer leaves (`Main.qml:198-201`).
    Under the owner rule, a hover-peek that the user has clicked or typed into must never collapse. A pure hover preview
    with no interaction is the one case the owner reviews before the spec is final.
- **Tools T1 (idle CPU): not reproduced from a fresh start.** Isolated background measured 0.00%, and 0.17% with one
  peer (`tools.md`). It still needs a measurement on the long-running real instance after installs. The real-desktop
  agent is re-measuring.
- **Box:** the isolated `box.md` and real `box-real.md` rounds 1–3 were verified by the orchestrator during the earlier
  rounds. BR-1 is a dispatch artefact (only Activated is sent), but the plan keeps the robustness fix: reset
  `shortcut_down` when the Island hides.

- **Find (`find.md`, FD-1..FD-10): confirmed.**
  - FD-7: in Results mode, Escape hides even when the box has text (`ui/model.rs:984-989`). A unit test locks this in,
    so that test changes with the owner's Escape rule.
  - FD-9: the Hyprland section is gated only on `target_os = "linux"` (`settings_ui.rs:959`).
  - FD-1 and FD-5: see the CLI paths in `main.rs`.
  - Not reproduced: idle CPU (0.1% on a 20k fixture) and startup (0.35 s).
- **Lens (`lens.md`, LN-1..LN-13): accepted on the evidence.**
  - LN-3 (zombies) was repeated independently.
  - LN-1 (first-run font panic), LN-6 (orphan recorder) and LN-8 (unmasked history thumbnails) are release blockers.
  - LN-9..LN-13 are recognizer accuracy items.

- **Real desktop, round 4 (`real-desktop.md`): verified.**
  - RD-WHEEL-1: `C++/Arcade wheel/src/main.cpp:120-122` sets org = app = "Arcade Wheel", which is W1's root cause.
  - RD-LENS-1: Ctrl+C is handled as a key, but egui-winit 0.33 delivers `Event::Copy` instead, so Ctrl+C does nothing on
    Wayland (`overlay.rs:980`).
  - Confirmed on Hyprland: CB-4, SH-8, WH-1, TL-3, TL-4, TL-6, LN-3, LN-4 and BR-9.
  - New: RD-TOOLS-1 (6 false shortcut conflicts), RD-TOOLS-2 (engine version checks time out), RD-TOOLS-3
    ("Not checked" contradicting Home), RD-TOOLS-6 (window class `arcade-tools`), RD-WHEEL-3 (pointer warped and not
    restored), RD-LENS-2/3/4/6, RD-FIND-2/3/4, RD-SHELF-2/3/4.
  - Downgraded on Hyprland: FD-2 (Find's layer-shell keyboard grab keeps it open); LN-2 (Wayland copy works);
    LN-6 and LN-7 (Wayland has no window list, so X11-only, kept as X11 bugs); TL-1, TL-2 and TL-5 not reproduced on
    Wayland but stay as X11 and packaging bugs.
  - Could not test: drag and drop (the virtual pointer can't drive it), Wheel's physical hold, Shelf's other edges,
    and tray Quit items.
  - **Owner heads-up:** Clipboard "Private mode" is ON on the real desktop and nothing has been captured since 19:13.
    The cause is unknown, possibly an earlier Box experiment. It was left untouched.

- **Wheel and Tools isolated (`wheel.md` WH-2..WH-6, `tools.md` TL-8..TL-12): verified.**
  - TL-8 is T1's root cause.
    - Home polls every 2 s, and each poll does a full uncached `checksum(&executable)` of every unmanaged AppImage
      (`manager.rs:1163`): 12% CPU and about 4 GB read in 20 s.
    - The thread and fd counts are one per running peer, not a leak.
    - Fix: cache the hash by (size, mtime, inode) and poll only while Home is visible.
  - TL-11: `source.rs:106` sets `.timeout(120 s)` as a total request timeout. Downloads slower than about 6 Mbit/s fail,
    and there is no resume. The v1 chunked updater replaces this; meanwhile the timeout becomes a read-idle timeout.
  - TL-9: the Nightly switch only writes a preference, so the v1 nightly work starts from a half-built feature.
  - WH-5: the shared Link QML kit sets `closePolicy: destructive || busy ? NoAutoClose`
    (`arcade-link/design/qml/Arcade/Dialog.qml:21`). Escape never cancels a destructive dialog in Shelf or Wheel.
    The fix goes in Link: Escape cancels unless busy.
  - WH-3: a 5 MB imported deck name freezes Wheel at 900 MB. Import needs size and length limits plus schema validation
    (WH-4, WH-6).

All audit parts are complete. The memory investigation (`memory.md`) is still running.

## Not app bugs (recorded so nobody chases them)

- Apps launched from an agent session die when that session ends (systemd scope teardown). Launch them with
  `systemd-run --user --scope --slice=app.slice`.
- xdg-desktop-portal-hyprland segfaulted twice today (15:01, 15:08); that's a system package, not Arcade.
