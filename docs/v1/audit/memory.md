# Arcade memory audit — can all eight apps stay under 500 MB (PSS and RSS)?  (research agent, 2026-11-06/07, no code changed)

Evidence: `AUDIT/memory/` (raw JSONL, logs, screenshots, harness in `memory/tools/`, Haiku-collected source references in `memory/refs/`). Every number is labelled RSS/PSS MB; labels in §1. Findings log with every individual measurement is Appendix A.

## 0. Answer

**PSS vs RSS.** RSS is what htop/btop (and Tools' Home "847 MB") show: every mapped RAM page, shared libraries counted again in each process. PSS splits shared pages between the processes that use them, so it is what the apps really occupy. You asked for < 500 MB on **both**, FUSE stubs included.

**Today** (real, aged): fleet **1196 MB RSS / 767 MB PSS** = apps 1084/655 + 8 AppImage helper processes 112/112. A lab fleet of the same installed builds gives 922/767. Fresh apps are small (5 apps right after boot: 457/388 with stubs); what hurts is that WebKit apps never give anything back after their window closes
(Tools 50/43 fresh → 232/137 after one use), that every app drags a 17–23 MB AppImage helper, that Clipboard's Flutter engine alone is 156/73, that Wheel keeps a GL renderer in a hidden process, and that ~250 MB of the RSS is libLLVM/Mesa counted five times.

**What must change** (ranked, details §4): U1 unpacked runtime (−167/−203 fleet), W1 release-after-close for Tools/Box/Look (−479/−233), W0 trim after init/hide (≈ −150/−72 for the five non-WebKit apps; the WebKit apps' share is inside W1), WH1 Wheel software renderer (−72/−24), W3b page-out of hidden WebKit heaps (−60/−60), CB1a Clipboard trim (−60/−13), OPEN1 open-window trim (−203…−236 / −91…−109 with a window open); optional: CB1b Clipboard daemon (−90/−74), compositing off (−50/−40, not UX-neutral).

**Projected fleet, all 8 in the background: 427 RSS / 313 PSS** (337/239 with the Clipboard daemon) → **under 500 on both, no UX loss.** Reopen/cold-start latency is equal or better everywhere (unpacked runtime: Tools 0.68 → 0.31 s, Box 0.92 → 0.46, Look 0.67 → 0.32 cold; in-place reopen 0.30/0.25/0.20 s).
**With one window open:** non-WebKit windows ≈ 440/325 (under 500). A **WebKit window (Tools/Box/Look): PSS ≈ 465–502 (at the limit), RSS ≈ 650–675 (not reachable without a UX trade-off)** — a single WebKit window costs 240–290 RSS even trimmed, an empty page is 456/229 (§6.3). The option with no visible UX cost is W1 + OPEN1 + W3b (§6.4).

## 1. How the numbers were taken (every number in this file carries one of these labels)

- **RSS** = `VmRSS`: all RAM pages a process has mapped, shared libraries counted in full in every process that maps them. This is what htop/btop and Tools' Home dashboard show; sums over apps double-count.
  **PSS** = `smaps_rollup Pss`: each shared page divided among the processes mapping it; the sum over processes is (about) the RAM the apps really occupy. Both are given everywhere, as **RSS/PSS MB**.
- **App tree** = the app process plus its children (WebKitWebProcess, WebKitNetworkProcess). The **AppImage FUSE stub** is a separate process (the runtime forks a squashfuse daemon that is re-parented to init, so it is *not* a child of the app);
  it is reported as "stub" and included in every fleet total ("stubs included" as requested).
- **real** = the owner's CachyOS/Hyprland desktop, AMD GPU, read from `/proc` (`fleet.py`, `appmaps.py`), or an isolated instance on the real Wayland session. **lab** = private HOME/XDG/dbus on Xvfb with llvmpipe
  (`lab.py`). Xvfb is a faithful stand-in for WebKit/Qt/egui apps (Tools open: real 523/302 vs lab 529/312; lab all-8 AppImage fleet 767 PSS == real aged 767 PSS) but **inflates Flutter** (Clipboard lab 321/256 vs real 181/109).
- **aged** = the app was used once and its window closed (what a normal day looks like); **fresh** = just started `--background`; **released** = after the app's own release logic plus the fixes below.
- Unpacked-payload tests live on btrfs (`/var/tmp`); an early run on tmpfs was invalid (tmpfs pages get swapped) and was redone. `trim+dontneed` in the tables is the shim in `memory/tools/trimshim.c`
  (`malloc_trim` + `madvise(MADV_DONTNEED)` on read-only executable mappings of the process; `+pageout+anon` adds `MADV_PAGEOUT` of anonymous memory, which moves it to swap).

## 2. Measured tables

### 2.1 Owner's real desktop (RSS/PSS MB; FUSE stub in brackets)
| app | fresh after the reboot | aged background (used, window closed) | window open (real GPU) | back to background after close |
|---|---|---|---|---|
| Tools 0.4.6 | 63/57 [23/22] | **253/146** [21/21] (22:13) | 523/302 | never (lab: 232/137 after 60 s) |
| Box 0.4.3 | not running | 192–209/106–126 [18/18] | 467/300, mapped 0.50 s | 196→191/113→108 |
| Look 0.4.3 | 60/54 [23/23] | 194–200/112–118 [17/17] | 438/266, mapped 0.37 s | 195/112 (webview released at 30 s, WebKit stays) |
| Clipboard 0.3.2 | not running | 175–182/103–111 [18/18] | 193/121 (Settings page), shown in 0.11 s | 193/121 (never) |
| Wheel 0.5.0 | 113/70 [22/22] | 114–143/57–86 [17/17] | +0 (overlay already resident) | — |
| Shelf 0.4.0 | 72/62 [23/23] | 60–62/49–51 [6–8] | +1.3 | — |
| Find 0.5.4 | 43/40 [16/16] | 51–53/46–48 [1–5, swapped] | +0 | — |
| Lens 0.4.2 | not running | 7–14/5–11 [14/14] | n/a | — |
| **fleet (all 8)** | 5 apps 351/282 + stubs 106/106 | **1084/655 + stubs 112/112 = 1196/767** (22:13); 907/585 + 121/121 at 23:00 (Tools fresh) | Box open adds +276/+192 | |

### 2.2 Lab (Xvfb), per app and state, RSS/PSS (stub separate); "unpacked" = btrfs copy run through `AppRun`
| app | fresh bg AppImage [stub] | fresh unpacked | unpacked + trim | open (AppImage) | closed +35 s (AppImage / unpacked) | **released** (unpacked, W1+W0) |
|---|---|---|---|---|---|---|
| Tools | 50/43 [22/22] | 17/10 | 12/9 | 532/324 | 232/137 / 203/104 | **60/38** (AppImage 75/55) |
| Box | 72/65 [21/21] | 76/69 (85/78 at +20 s) | 45/42 | 533/325 | 256/160 / 258/153 | **92/71** (AppImage 94/74) |
| Look | 51/44 [21/21] | 54/47 | 34/30 | 505/297 | 245/150 / 250/144 | **80/59** (AppImage 79/59) |
| Clipboard (llvmpipe) | 321/256 [19/18] | 325/250 | 222/195 | — | — | real GPU est 120/96 (probe) |
| Wheel | GL 142/83 [22/22]; software 70/59 | GL 148/65 | GL 62/34; software **42/26** | `--preview` GL 227/160, software 81/65 | — | — |
| Shelf | 72/60 [23/23] | 75/64 | 41/35 | — | — | — |
| Find (empty index; real index +25) | 21/16 [19/19] | 23/18 | 10/8 | — | — | — |
| Lens | 14/11 [18/17] | 15/12 | 6/4 | — | — | — |

### 2.3 Fleet totals (all 8 together)
| state | RSS | PSS |
|---|---|---|
| real, aged (22:13), apps + stubs | 1084 + 112 = **1196** | 655 + 112 = **767** |
| real, fresh after reboot, 5 apps + stubs | 351 + 106 = 457 | 282 + 106 = 388 |
| lab AppImage, bg +70 s (apps 753/599 + stubs 170/168) | **922** | **767** |
| lab AppImage, Tools window open / closed +45 s | 1386 / 1081 | 1028 / 821 |
| lab unpacked, bg +45–70 s (no stubs) | 734–755 | 564–570 |
| lab unpacked + one trim | 444–471 | 373–403 |
| lab unpacked + trim, Tools window open / open + trim of all processes | 958 / 716 (compositing off: 879 / 663) | 691 / 572 (641 / 532) |
| lab unpacked + trim, 30 s after close / released | 544 / 482 | 443 / 401 |
(Clipboard is llvmpipe 220–325 RSS here; on the owner's GPU subtract about 100 RSS / 97 PSS. The lab AppImage fleet PSS 767 equals the real aged PSS 767.)

### 2.4 Latency (seconds to window mapped; Xvfb unless noted)
| app | cold start from nothing AppImage → unpacked | in-place reopen after release AppImage → unpacked | hot re-show (webview alive) |
|---|---|---|---|
| Tools | 0.68 → **0.31** | 0.40–0.42 → 0.30 | — (destroyed on close) |
| Box | 0.92 → **0.46** | 0.38–0.46 → 0.24–0.26 | 0.37 (= after release 0.375) |
| Look | 0.67 → **0.32** | 0.32–0.34 → 0.20 | **0.15** (0.29 after the 30 s release) |
| Clipboard | 0.29 → 0.22 | real GPU: resident show 0.06–0.11 | — |
| Flutter probe, real GPU | exec → first frame 0.22 (5 runs 0.20–0.22) | | |
Real desktop: Box `--show` mapped 0.50 s, Look 0.37 s, Tools (isolated, real Wayland) 0.72–0.77 s.

## 3. Where the memory goes (PSS MB by mapping category; RSS-only effects in the last column)

| app (state, source) | biggest private/own PSS | shared-library PSS | RSS-only inflation |
|---|---|---|---|
| Tools fresh (real, 56) | anon 9 + heap 3 | bundled GTK/WebKit libs 19, binary 14, libwebkit 10 | stub 22; FUSE makes mapped files 4–7x more resident (binary 11.9 vs 1.7 MB) |
| Tools open (real, 302) | anon 101 + heap 45 | libwebkit/JSC 79, bundled 38, binary 17 | libLLVM 70 + Mesa 35 RSS for 13 PSS |
| Tools closed (lab, 137) | UI process 98 (anon 42) + **network process 39 that never exits** | | |
| Box aged (real, 106) | anon 33, heap 13.5 | libwebkit 22, binary 16 (64 MB executable), bundled 13 | libLLVM 35 + Mesa 15 RSS, 6 PSS |
| Look aged (real, 118) | anon 30, heap 28 | libwebkit 21, bundled 19, binary 11 | libLLVM 35 + Mesa 15 RSS |
| Clipboard (real, 111) | heap 30, anon 21, Dart `[anon]` 15 | libflutter+AOT 20, Rust core 6.5, system 6.5 | libLLVM 35 + Mesa 16 RSS; 76 threads (24 Mesa util-queue, 14 engine, 12 tokio, 4 Dart io) |
| Wheel (real, 84) | heap 25, anon 12 | Qt 29.5, bundled 5 | libLLVM 35 + Mesa 15 RSS (GL renderer in a hidden process) |
| Shelf (real, 51) | heap 16.5, anon 4 | Qt 22, binary 4 | none (already software renderer) |
| Find (real, 48) | anon 28 (index 25.3 MB, 302 671 entries), heap 7.5 | binary 10 | |
| Lens (real, 11) | heap 2, anon 2 | binary 7 | |

Fleet-wide: **libLLVM + Mesa are mapped by 5 processes (Box, Look, Clipboard, Wheel, and Tools after first use): ~250 MB of the fleet's 1084 MB RSS but only ~25 MB PSS.** A quarter of the number the owner reads is that double count;
it disappears only if background processes never load the GL driver (Wheel: software renderer; WebKit apps: release + trim; Clipboard: engine out of the resident process).
The 8 stubs are 112 MB (real, partly swapped) to 170 MB (lab, all active). Fonts ≤ 1.4 MB PSS per process, icon-theme caches ≤ 0.2 MB: not a lever. Fleet swap on the real desktop 128–140 MB.
glibc arenas / duplicate tokio runtimes (Box and Look run 24 tokio workers = 2 runtimes, Tools 29, Find 12): measured **≤ 3 MB** (arena=1, tokio=2: threads 39→19, RSS −3): not a lever.

## 4. Ranked work items (each: app · cause · evidence · MB now → after RSS/PSS · fix · UX risk · how UX stays equal)

Ranking = fleet RSS+PSS saved per unit of effort, with UX-neutral items first. "lab" numbers are measured on Xvfb with the installed 0.4.x builds; "real" on the owner's desktop. All source lines were re-read (Haiku collected them, 302/305 quoted lines re-verified mechanically, the rest read by me).

| # | id | apps | fix (one line) | measured effect | UX |
|---|---|---|---|---|---|
| 1 | **U1** | all 8 (Link runtime + packaging, Tools GC) | AppImage stays the single distributed file but its runtime extracts once to `$XDG_DATA_HOME/arcade/runtime/<app>-<ver>-<hash>/` and `execv`s `AppRun` with `APPIMAGE`+`APPDIR` set: no FUSE daemon, no stub | stubs 112/112 real (170/168 lab) → 0; Tools fresh 73/65 → 17/10; lab all-8 fleet bg **922/767 → 755/564** (−167/−203) | cold start **faster**: Tools 0.68→0.31 s, Box 0.92→0.46, Look 0.67→0.32, Clipboard 0.29→0.22 |
| 2 | **W1** | Tools, Box, Look | after the last window is destroyed: SIGTERM the child `WebKitNetworkProcess`, `malloc_trim`, `madvise(DONTNEED)` on own read-only executable mappings; one shared fn in Link | Tools aged 203/104 → **60/38**, Box 258/153 → **92/71**, Look 250/144 → **80/59** (unpacked; Σ −479/−233); AppImage mode Tools 232/137 → 75/55 | reopen unchanged or faster (Tools 0.30 s, Box 0.25, Look 0.20; AppImage 0.40/0.38–0.46/0.32) |
| 3 | **W0** | all 8 (Rust: Link fn; Qt/Flutter: C twin) | the same trim once after init and after each hide (no network-process step) | fresh, unpacked, lab: Box 85/78→45/42, Look 54/47→34/30, Shelf 74/62→41/35, Wheel(sw) 76/40→42/26, Find 23/18→10/8, Lens 15/12→6/4, Tools 17/10→12/9; Flutter probe on real GPU 156/73→95/60 | none measured (click→repaint 145→141 ms) |
| 4 | **WH1** | Wheel | `QQuickWindow::setGraphicsApi(Software)` exactly as Shelf does | lab 142/83 → **70/59**, 34 → 9 threads; first `--preview` draw +85 MB avoided (GL 227/160 vs software 81/65); unpacked+trim 42/26 | needs a pixel-diff of `--preview` and an F8→visible timing on the owner's session (QML uses no ShaderEffect/MultiEffect/layer effects) |
| 5 | **W3b** | Tools, Box, Look (optional) | also `MADV_PAGEOUT` the anonymous heap of a hidden WebKit app | Tools 75/55 → 59/38 (19 MB to swap), reopen 0.40→0.42 s | swap-backed: invisible unless the machine is under memory pressure |
| 6 | **OPEN1** | Tools, Box, Look (window-open case) | UI process self-trims 2 s after page ready and every 15–30 s; the WebKit helper processes get the same through an `LD_PRELOAD` trimmer set in the AppRun hook | Tools open 528/319 → **292–325 / 210–228** (−203…−236 / −91…−109); fleet open 958/691 → **716/572** | click→repaint 145 → 141 ms (probe resolution ~40 ms) |
| 7 | **CB1a** | Clipboard | in-process trim (W0) | real probe 156/73 → 95/60; Clipboard est 181/109 → **~120/96** | none |
| 8 | **OPEN2** | WebKit apps | `WEBKIT_DISABLE_COMPOSITING_MODE=1` (software compositing) | real: Look open 487/280 → 409/228, Tools 532/322 → 453/267; lab Box 531/315 → 432/251; threads 138 → 96 | stills identical; **motion needs proof** (Box's own `ARCADE_MOTION_PROBE`, see §6.4) |
| 9 | **CB1b** | Clipboard (optional, large) | capture + store in a small resident daemon, Flutter UI spawned on demand | 181/109 → est **30/22** (−150/−87) | first open after idle 0.11 s → ~0.25 s (cold 0.22–0.29 s) unless kept warm |
| 10 | CB2 | Clipboard | answer `--status/--version/--arcade-manifest/--restart` without starting Flutter | transient ~150–250 MB and ~0.5 s per CLI call → 0 | none |
| 11 | SH1 | Shelf | thumbnail `QCache` default 16 MiB, cap 64 MiB, clear on hide | up to −64 MB when thumbnails were shown | re-decode on reopen (2 pool threads) |
| 12 | CB3/CB4 | Clipboard | list rows without full `text`; `imageCache` ≤ 24 MiB; window the list; size-budget retention (owner decision) | keeps RAM bounded with Forever/256 MB history | none |
| 13 | FD1 | Find | `shrink_to_fit` after load/rescan (optional: page-out the index when idle) | −2…−5 (−20 optional) | optional: first search after idle faults pages |
| 14 | LK1 | Look | virtualize/truncate big text/markdown | 5 MB .md/.txt peak 880/654 → est −300 | none if the truncated banner already exists |
| 15 | BX1 | Box | bound the 500-item clipboard history and job results | few MB | none |
| 16 | TD1 | Tools | Home "memory" = tree PSS (children + stub), not RSS of one pid | metric only (847 MB → the real ~600) | none |

### Details and evidence

**U1 — unpacked runtime (all apps).** Cause: every app runs from a FUSE-mounted squashfs. (a) The runtime's squashfuse daemon lives for the app's whole life: 16–23 MB anon each, 8 of them (`Arcade-X.AppImage` processes; real 112 MB, partly swapped). (b) File pages mapped through FUSE are 4–7x more resident
than from btrfs (Tools binary 11.9 vs 1.7 MB, libwebkit 7.6 vs 0.6): fresh Tools 50/43 through FUSE, **17/10** unpacked. (c) Cold start is slower (+0.37 s Tools, +0.46 s Box, +0.35 s Look).
Fix: replace the AppImage runtime (all eight use the same type2 runtime, squashfs offset 944632, zstd 128 K blocks) by one that extracts to a per-version cache dir once and `execv`s `AppRun` with `APPIMAGE` and `APPDIR` exported, so everything that advertises the `.AppImage` path keeps working:
`arcade-link/crates/arcade-link/src/manifest.rs:486-504` (`current_executable` returns `$APPIMAGE` only when the exe is under `$APPDIR`: both must be set), `install.rs:263,323` (Exec= of launcher and autostart), `client.rs:372-393` (`spawn_detached(manifest.executable)`),
`childenv.rs:21-36` (APPIMAGE_ONLY env stripped for children), Tools `installation.rs:121-139` / `platform.rs:370-400` / `paths.rs:118-135` (install path `~/Applications/Arcade/<App>.AppImage`, chmod 755; add GC of old unpacked dirs here),
Clipboard `main.cc:49-90` (`release_mount_handles`, `wait_for_predecessor`). Cost: +1.5 GB uncompressed on disk for the 8 apps (btrfs zstd ≈ 0.6 GB; ext4 full), one extraction (~1 s) per app per version, hardlinking identical files (Tools and Look share 251 MB, F22) reduces it.
UX equal: same path, same env contract, faster starts. Verify: `--restart`, update-in-place (old dir still mapped while the new version is extracted), and the "installed copy" detection in Find `installer.rs:221` and Look `integration/mod.rs:389-395`.

**W1 — release after close (Tools, Box, Look).** Cause (measured, F1–F3, F21): (1) tauri-runtime-wry keeps the `WebContext` for the whole app life on Linux (`tauri-runtime-wry-2.11.4/src/lib.rs:2576-2590`, tauri#14626), so WebKit's `WebKitNetworkProcess` (60–69 RSS / 33–50 PSS) never exits;
(2) every library page the webview touched (GTK, WebKit, Mesa/LLVM: UI process RssFile ~146 MB) stays mapped; (3) glibc keeps ~40 MB of freed heap. Today's code: Tools `desktop.rs:253-262` (+ `resident/memory.rs:4-15`: `malloc_trim` and `MADV_PAGEOUT` of `.so` mappings — PAGEOUT skips pages also mapped by the network process, measured 227/132 vs 232/137, i.e. ineffective),
Box `lib.rs:1469,1506-1517` (only `malloc_trim`; island released 15 s after hide `lib.rs:1286`, immediately on close `lib.rs:1459`; dashboard 15 s `dashboard.rs:12`), Look `app.rs:340,725-760` (destroys the webview after 30 s, `app.rs:37`, nothing else). The three apps even carry copies of the same `release_webview_ipc_on_destroy`.
Fix: one function in Link, called 0.5 s after the webview is destroyed: SIGTERM children whose comm is `WebKitNetworkProcess` (delay ≥ 3 s; only data in that process is localStorage, used by Tools only for the onboarding flag, `Onboarding.svelte:56`), `malloc_trim(0)`, `madvise(MADV_DONTNEED)` over this process's `r-xp` file mappings only
(never `r--p`/RELRO or writable ones: DONTNEED would revert relocated pages; they are COW'd). Keep the 15 s/30 s keep-warm: it is what makes Look re-show 0.15 s (F25). UX equal: reopen measured 0.30/0.25/0.20 s unpacked, 0.40/0.38–0.46/0.32 s AppImage — not slower than before; respawned network process verified.

**W0 — trim after init and after hide.** Cause: fresh processes touch far more file pages than they keep using (Box maps 64 MB executable and keeps 31 MB resident after start). Fix: same function without the kill step, after startup init (all 8) and after each hide (Qt/Flutter apps). Evidence F16, F15. UX equal: refaults are minor faults from the page cache (microseconds); measured click→repaint unchanged.

**WH1 — Wheel software renderer.** Cause: `QQuickView overlay` is created at start (`main.cpp:416`) with the default GL scene graph, so a hidden process holds libgallium+libLLVM (RSS 49), `QSGRenderThread` and ~12 Mesa threads; real Wheel shows +0 MB when the wheel is displayed because it is already all resident.
Shelf (same QML kit) already forces software (`Shelf main.cpp:287-292`). Settings/Activity engines are already released after 10 s (`main.cpp:439-441,479-481`) — keep. Verify visually (pixel diff of `--preview`) and time F8→visible on the real session; I could not capture the overlay on Xvfb.

**OPEN1 / OPEN2 / W3b — window-open levers** are in §6.3–6.4.

**CB1 — Clipboard engine.** Cause (F10, F11, F15): capture is Dart-driven (`app_controller.dart:363-367,1410-1510`, watcher process started from Dart `desktop_adapter.dart:603-606`), so the Flutter engine must stay alive; the runner keeps a realized, never-mapped GTK window + GL context all day (`my_application.cc:65-75,128-141`).
Probe: a stock Flutter app, hidden, costs **156/73** on the owner's GPU; Clipboard is 181/109, so Clipboard's own code is only +25/+36. Destroying the view frees nothing (230/166 vs 233/166, `FlutterEngineRemoveView` refuses the implicit view), `gtk_widget_unrealize` is worse (306/228).
CB1a (trim) is free; CB1b (daemon) is the only way below ~95/60: the Rust core is already a library, the watcher already a separate process, what has to move is the Dart orchestration of `capture` (private mode, dedupe, size limits, rich formats). UX cost of CB1b: today the window shows in 0.06–0.11 s (real), a fresh engine needs 0.21 s to first frame on the real GPU (0.22–0.29 s Xvfb) — keep a 60 s keep-warm to hide it.

**CB2.** `--status`, `--version`, `--arcade-manifest`, `--restart` set `query` and still run the full activate path (`my_application.cc:171-181`; `main.cc:84-92` fast-paths only `--help`): every status call by Link/Tools/the owner boots a Flutter engine (my own `--status` printed a second GTK instance). Answer from C++ (version/manifest) or the Link socket (status) before `g_application_run`.

**CB3/CB4 (owner decision: history Forever, 256 MB budget, oldest unpinned pruned).** Today there is no size budget at all: `retention_hours` 24 / `max_items` 500, clamp `MAX_RETAINED_ITEMS` 10 000 (`store.rs:16,1005-1046`, `core.rs:1558-1567`). RAM is already bounded by paging: `history` returns ≤ `_historyLimit` (250, +250 per "load more", `app_controller.dart:82,862`)
and the list renders only `preview` (`history.dart:330`, `quick_overlay.dart:321`) but the payload still carries `text` (≤ 32 KiB) per row — drop it from list rows. `Image.memory(image.bytes!)` at `clipboard_widgets.dart:300` has no `cacheWidth`, and no `imageCache` limit exists (Flutter default 100 MiB): set 24 MiB.
`_items` (`app_controller.dart:61,606`) has no Dart-side cap: window it. Search decrypts every row (`store.rs:456-560`) but streams, so keep it streaming and move to keyset pagination when the 10 000 clamp is replaced by the byte budget (`SUM(length(ciphertext))` in `prune`).
Sqlite lives on disk (owner's real history: 3 items, 925 KB db): RAM does not grow with history if only previews cross the FFI boundary.

**SH1.** `ThumbnailProvider.cpp:22` `QCache` 64 MiB, user preference up to 1 GiB (`:121-125`), never cleared when the panel hides, no disk thumbnail cache. **FD1.** Find's index is 25.3 MB of heap (`indexBytes` 25 266 078 for 302 671 entries via `--status`; `index.rs:170` counts capacity; no `shrink_to_fit` anywhere).
**LK1.** Look on a 5 MB `.md`/`.txt`: 880/654 peak (prior audit data `memory/look.jsonl`), back to 180 after the 30 s release; `mediaserver.rs:35-50` spawns a thread per connection and is never stopped. **BX1.** Box keeps a 500-item clipboard history in RAM (`clipboard_history.rs:20,92`) and job results without a cap (`link.rs:183-189`).
**TD1.** The owner's "847 MB" (Tools Home, `src/dashboard.rs:47-82`) is `sysinfo` RSS of one pid per app, no children, no stub; show tree PSS. `Home.svelte:69-71` polls every 2 s while Home is visible (6 % CPU open): poll slower, not a memory item.

## 5. Architecture options and their measured latency cost

| option | memory | measured latency | verdict |
|---|---|---|---|
| **Release in place** (W1+W0, process stays) | aged closed state Tools 60/38, Box 92/71, Look 80/59 (unpacked) | reopen after release 0.30 / 0.25 / 0.20 s (AppImage 0.40 / 0.38–0.46 / 0.32); hot re-show while the webview is alive: Look **0.15 s**; Box 0.37 s hot and 0.375 s after release (no measurable keep-warm benefit through the CLI path); AppImage CLI start included | **do this** |
| **Tiny resident daemon, UI process on demand** (Tauri apps) | closed state → fresh-trimmed level: Tools 12/9, Box 45/42, Look 34/30 (Σ −141/−87 vs in-place) | cold process start, unpacked: Tools 0.31, Box 0.46, Look 0.32, Clipboard 0.22 s (AppImage 0.68/0.92/0.67/0.29); i.e. +0.01…+0.2 s vs in-place | not needed for Case A; per-app daemon/UI split is large; only worth it for the margin |
| **Prewarm the UI when the shortcut is pressed** | — | no gain: the shortcut press *is* the open request; only a keep-warm window (Look 30 s, Box 15 s today) removes latency | keep today's keep-warm |
| **One shared WebKit for all apps** | with W1 only one network process exists when one window is open (33 PSS / 60 RSS): gain 0. Two WebKit windows at once: shared payload files give −47 PSS, 0 RSS (F22) | — | reject (would merge three apps into one host); take the hardlinked payload (cheap, part of U1's store) |
| **Release other apps' webviews while one is open** | already true after W1 (hidden apps hold only 38–71 PSS) | shortening Look's 30 s loses the 0.15 s hot re-show for no steady-state gain (Box's 15 s buys nothing measurable) | do not shorten |

## 6. Per-app budgets, reachability, and the one-window-open options

How the projections were built: lab-measured "released" values for the three WebKit apps (unpacked, W1+W0: Tools 60/39, Box 93/72, Look 82/61), lab-measured trimmed values for Shelf 43/36, Wheel with software renderer 42/26, Lens 6/5,
Find 42/38 (lab 17/14 + the real 25 MB index, minus trim), and for Clipboard the **real-GPU** engine probe (156/73 → 95/60 trimmed ⇒ Clipboard 181/109 → ~120/96). W3b uses the measured Tools page-out (−16/−17) and scales Box/Look by their anon size (est). Numbers are RSS/PSS MB, stubs 0 (after U1).

### 6.1 Case A — all 8 apps in the background (aged, windows closed)
| app | expected, Tier 1 (U1+W1+W0+WH1+CB1a) | + W3b (page-out of hidden WebKit heaps) | CI budget (PSS/RSS) |
|---|---|---|---|
| Tools | 60/39 | 44/21 | 30/50 |
| Box | 93/72 | ~68/50 | 60/80 |
| Look | 82/61 | ~62/41 | 50/70 |
| Find | 42/38 | 42/38 | 42/48 |
| Lens | 6/5 | 6/5 | 8/8 |
| Shelf | 43/36 | 43/36 | 40/48 |
| Wheel (software) | 42/26 | 42/26 | 30/50 |
| Clipboard (engine resident, trimmed) | 120/96 | 120/96 | 105/130 |
| **fleet** | **488 / 373** | **427 / 313** | **sum 484 / 365** |
With CB1b (Clipboard engine out of the resident process, est 30/22): Tier 1 398/299, with W3b **337/239**. Today (real aged): 1196/767. So the fleet shrinks by ~60 % RSS / ~60 % PSS and is **under 500 on both metrics**; Tier 1 alone leaves only 12 MB of RSS margin, W3b (UX-neutral by measurement, costs ~60 MB of swap) or CB1b buys the margin.

### 6.2 Case B — all 8 in the background + one window open
| open window | UX-neutral set (Tier 1 + W3b + OPEN1) RSS/PSS | + compositing off | + CB1b | + page-out while open (OPEN3) |
|---|---|---|---|---|
| Tools (tree 292/210) | **675 / 502** | 625 / 462 | 585 / 428 | ~613 / 432 |
| Box (tree 295/207) | **654 / 470** | 602 / 428 | 564 / 396 | ~589 / 403 |
| Look (tree 285/193) | **650 / 465** | 601 / 427 | 560 / 391 | — |
| Wheel / Shelf / Find / Lens (+0…+2), Clipboard in-process (+12) | **≈ 440 / 325** | | | |
(Open-window trees from the fleet lab, §F29; "others" from §6.1; compositing off removes 50/40; CB1b 90/74; OPEN3 (anon page-out of the open window, swap 70–95 MB) about 60–120.) Today with Tools open: 1386/1028 (lab AppImage fleet).

### 6.3 Reachability, stated plainly
- **Case A: reachable on PSS and RSS without UX loss** — 427 RSS / 313 PSS (every item UX-neutral by measurement; WH1 still needs the visual check on the owner's session). 337/239 if the Clipboard engine moves out.
- **Case B with a non-WebKit window (Wheel, Shelf, Find, Lens, Clipboard): reachable**, ≈ 440 / 325.
- **Case B with a WebKit window (Tools, Box, Look): PSS is at the limit** (465–502) with the UX-neutral set; **RSS is not reachable (650–675)**. Reason: one WebKit window has a fixed floor of ~240–290 RSS even trimmed (an *empty* page in stock WebKitGTK is 456 RSS / 229 PSS untrimmed, F24), the other seven apps cannot go below ~250–380 RSS
  (Clipboard engine 120, Box/Look residual 68/62, Qt apps ~43 each, Find index 25), and what is left to remove costs visible UX. Even with every in-process lever (Clipboard daemon, process-exit WebKit apps) the others sit at ~250 and the window at 292: ~540 RSS.
  To get RSS under 500 you must additionally take one of: compositing off (−50; measured Island p95 15–17 → 19–22 ms on the real GPU), page-out of the open window (−60…−120; swap-in jitter risk), or a Clipboard cold start (+0.15 s). Under the owner's rule (UX must not get worse) none of these is acceptable by default,
  so the honest target for that case is **PSS ≈ 465–502 and RSS ≈ 650**, i.e. a 35–40 % cut from today's 1386/1028, not 500/500.

### 6.4 One WebKit window open: concrete options per app, with UX cost (measured where marked)
Common to all three: after W1 the *other* hidden WebKit apps hold only 39–72 PSS (no WebProcess, no network process); there is nothing left to release there. Candidates considered:
| candidate | effect | UX cost | verdict |
|---|---|---|---|
| Release the other hidden apps' webviews while one is open | already the case after W1; shortening Look's 30 s or Box's 15 s gains nothing in steady state | Look loses its 0.15 s hot re-show (0.29 s after release) | no |
| Shared WebKitNetworkProcess across apps | with W1 only one network process exists when one window is open: 0 gain; two windows at once: shared payload files −47 PSS / 0 RSS (F22) | none | no (take the hardlinked payload only) |
| Drop a hidden app's WebProcess right after it hides | already how the apps behave (destroy with the webview); leftover is the network process + pages = W1 | none | covered by W1 |
| Smaller WebKit caches / memory-pressure settings / JIT off | JIT off −8/−7 (measured, not a lever); caches/pressure settings unmeasured (needs code; apps load local assets so little to shrink; the WebProcess floor is ~100 MB private even for an empty page) est ≤ −20 | GC more often: possible jank | prototype only |
| **OPEN1 — open-window trim** (UI process self-trim + `LD_PRELOAD` trimmer for the helper processes) | Tools 528/319 → **292–325 / 210–228** (−203…−236 / −91…−109), Box → 295/207, Look → 285/193 | none measured (click→repaint 145 → 141 ms; probe resolution ~40 ms) | **recommended: no visible UX cost** |
| W3b — page-out of hidden apps' anon heaps | −16/−17 (Tools measured), ~−60 for the three | none measured (reopen 0.40 → 0.42 s); swap-backed | **recommended** (invisible while RAM is free; 7.5 GB free here) |
| Compositing off (`WEBKIT_DISABLE_COMPOSITING_MODE=1`) | open tree −50/−40 (Tools 242/170, Box 243/165, Look 236/155); real GPU −79/−55 | Box's own probe, real GPU: p95 frame 15–17 → 19–22 ms, same count of >25 ms frames; stills pixel-identical; Look's video/PDF untested | not by default; owner's eyes first |
| Page-out of the open window (OPEN3) | Tools open 207/111 right after, 285/179 after use; swap 67–94 MB | swap-in on interaction under memory pressure; repaint probe unchanged | no |
| Clipboard engine out of the resident process (CB1b) | −90/−74 on every WebKit case | first Clipboard open after idle +0.1–0.15 s unless kept warm | the cheapest *trade* to get PSS comfortably < 500; owner's call |
Per app: **Tools** — only a window, no hidden webview; Home polls every 2 s (`Home.svelte:69-71`, 6 % CPU, slow it down); OPEN1 + W1 + W3b. **Box** — Island + dashboard are two webviews (dashboard adds ~7 MB), Island must stay smooth: never compositing off by default; OPEN1 + W1 + BX1 (bound the 500-item history).
**Look** — highest peak risk is content, not the window: 5 MB text/markdown peaks at 880/654 (LK1); video/PDF make compositing off risky; keep the 30 s keep-warm (0.15 s re-show); OPEN1 + W1.
**Recommendation (no visible UX cost): W1 + OPEN1 + W3b**, which takes the fleet with a WebKit window open from 1386/1028 (lab, today) to ~650/470–500. If the owner wants PSS clearly < 500 in that case, add CB1b with a 60 s keep-warm before considering compositing off.

## 7. CI conformance check (`memory/tools/fleet_budget.py`, `budgets.json`, `wmclose.c`)

What it does: starts **all eight** installed AppImages (or unpacked equivalents) together in one private session on Xvfb (own HOME/XDG/dbus), waits 75 s, measures every app tree from `/proc` (`smaps_rollup` PSS + `VmRSS`, FUSE stub attributed by exe + HOME because it is not a child of the app),
optionally opens one window (`--open tools|box|look`), closes it with `WM_DELETE_WINDOW` (`wmclose.c`; `xdotool windowclose` destroys the X window and the app exits, `windowquit` needs a window manager), settles 30–45 s, measures again, and **exits 1** if any per-app or fleet budget in `budgets.json` is exceeded. ~4 min, no GPU, no root.

```
python3 fleet_budget.py --apps ~/Applications/Arcade --budgets budgets.json --section xvfb --open tools --settle 70 --close-wait 30 --json out.json
```

Run #1 on today's builds (`memory/logs/ci-run.log`): background fleet **922 RSS / 750 PSS** (apps + stubs) vs Xvfb budget 680/540, Tools window open **1391 / 1014**, 30 s after close **847 / 683** (Tools row 0 because of the close bug above, now fixed) — 22 failures, exit 1, as expected before any fix.
Run #2 after the close fix (`memory/logs/ci-run2-fixed.log`, `memory/ci-run-fixed.json`): background **921 / 765**, Tools window open **1389 / 1028**, 30 s after a graceful close **1080 / 834** with Tools still at **240/145** (the resident process now survives the close, and shows the problem W1 fixes: it never returns), 25 budget failures, exit 1. The check therefore works as a gate; it only goes green once U1, W1, W0, WH1, W3b ship (and Clipboard moves out for the `target` section).

Budgets (MB, PSS/RSS, apps + stub, in `budgets.json`): sections `target` (the owner's goal on a real GPU: per app (PSS/RSS) Tools 30/50, Box 60/80, Look 50/70, Find 42/48, Lens 8/8, Clipboard 105/130, Shelf 40/48, Wheel 30/50, **fleet 500/500**) and `xvfb` (what CI can enforce: identical except
**Clipboard 260/335 (PSS/RSS) because llvmpipe makes the Flutter engine 2.3x larger than on a GPU; Flutter has no software-render switch (probe ignored `enable-software-rendering`) and softpipe is worse (443/382)**, hence fleet 525/690). Once the Clipboard engine is out of the resident process (CB1b) the Clipboard allowance disappears and CI enforces `target` unchanged.
The `open`/`closed` fleet caps are **ratchets** (set to today's achievable value, lowered as items land); the one-window RSS goal is not reachable (see §6.3), so CI must not claim it.
Per-app budgets sum to 484 RSS / 365 PSS (< 500 on both); they are the expected values of §6.1 plus 10–15 %, and the RSS ones only hold with W3b (anon page-out of hidden WebKit apps) or CB1b (§6.1).
Where to wire it: after the packaging job of each app repo (so an app that regresses its own budget fails its PR) plus a nightly fleet job that downloads the eight newest artifacts. Also keep the Tools Home number honest (TD1) so the owner and CI read the same metric.

## 8. Not worth doing / measured negatives and minor facts
- `MALLOC_ARENA_MAX=2` saves 0–2 MB; `MALLOC_ARENA_MAX=1 TOKIO_WORKER_THREADS=2` saves 1–3 MB (threads 39→19 Box, 35→15 Look, 30→20 Tools, 27→17 Find). Duplicate tokio runtimes (24 workers in Box/Look = 2 runtimes) are a thread-count issue, not memory.
- Flutter: destroying the view frees nothing; `unrealize` costs more; `enable-software-rendering` ignored; `GALLIUM_DRIVER=softpipe` 443/382 (worse), `LP_NUM_THREADS=1` 321/239 (−2 %).
- `MADV_PAGEOUT` alone (what Tools 0.4.6 does) is ineffective while another process maps the same pages; `DONTNEED` on executable mappings is what works. `PAGEOUT` of anon memory works but moves it to swap (Tools 19 MB, open window up to 94 MB): use only for hidden apps.
- Fonts ≤ 1.4 MB PSS per process, icon-theme caches ≤ 0.2 MB. Fleet swap 128–140 MB on the real desktop.
- Link serves each connection on its own thread (`server.rs:445-452`; 8 `arcade-link-conn` threads in an aged Tools): small, but a source of thread growth.
- A shared, hardlinked payload (Tools and Look ship byte-identical libwebkit/JSC/GTK/ICU, 251 MB; Box has a different WebKit build) saves PSS only when two WebKit apps are open together (−47 PSS in the measured two-window case); aim all three apps at one WebKit build.
- Disk cost of U1: +1.5 GB uncompressed (`/var/tmp/arcade-audit-ext` = 1471 MB for 8 apps; Look 437, Box 365, Tools 277, Shelf 151, Wheel 144, Clipboard 38, Lens 36, Find 26).


---

# Appendix A. Findings log (measurements as they were taken)

### A.0 Baseline of the owner's desktop when I started (restored at the end)
All 8 apps running in background (`--background`/`--service`), no Arcade window open except Shelf's edge tab (30x92 at right edge); only other window T3 Code.
Clipboard Private mode ON (untouched). Real Tools had just been relaunched fresh by the earlier agent.

### A.1 Measured: owner's real fleet (read-only /proc), 2026-11-06 ~23:00, all apps "background"
| app | procs | RSS MB | PSS MB | FUSE stub RSS/PSS | threads |
|---|---|---|---|---|---|
| Tools (fresh restart) | 1 | 63.4 | 55.9 | 23.0 / 22.9 | 44 |
| Box (used earlier, webview released) | 2 | 192.2 | 105.6 | 18.5 / 18.5 | 61 |
| Look (used earlier, webview released) | 2 | 199.9 | 117.8 | 17.5 / 17.5 | 63 |
| Clipboard | 1 | 181.7 | 110.7 | 18.3 / 18.3 | 76 |
| Wheel | 1 | 140.7 | 84.0 | 16.7 / 16.7 | 24 |
| Shelf | 1 | 62.3 | 51.5 | 7.6 / 7.6 | 8 |
| Find | 1 | 53.3 | 48.2 | 5.1 / 5.1 | 31 |
| Lens | 1 | 13.9 | 11.3 | 14.5 / 14.5 | 10 |
| **FLEET** | | **907** | **585** | **121 / 121** | 317 |
| **FLEET + stubs** | | **1028** | **706** | | |
Earlier (22:13, Tools had been opened+closed): fleet RSS 1084 / PSS 655 (+112 stubs) — Tools 253/146.

### A.2 Findings log (measured by this agent; consolidated into §3/§4 at the end)
Method: isolated labs on Xvfb :95 (private HOME/XDG/dbus, installed AppImages), harness `memory/tools/{lab,scenario,exp_rel,appmaps}.py`, raw `memory/lab-*.jsonl`.
Real-desktop cross-check: Tools window open on the real Hyprland/AMD GPU = RSS 523 / PSS 302 (Xvfb llvmpipe: 532/324) → Xvfb numbers are representative (±7 %).

**F1. Closed webview apps stay ~3x bigger than fresh.** Tools fresh bg 50/43 (RSS/PSS MB) → open 532/324 → closed+60 s 232/137 (never returns).
 Remaining after "release in place": UI process 172/98 (libs touched by WebKit/GTK/GL stay mapped, heap 42 MB anon) + **WebKitNetworkProcess 60/39 that never exits**.
**F2. Why the NetworkProcess never exits:** tauri-runtime-wry 2.11.4 `lib.rs:2575-2590` deliberately keeps the `WebContext` alive on Linux (tauri#14626), so WebKit keeps its network process.
 Killing it (SIGTERM, from the app, after the webview is destroyed) is harmless: next open respawns it, reopen 0.40 s vs 0.70 s first-ever open (measured, Tools).
**F3. File-backed pages can be dropped with no latency cost.** `madvise(MADV_DONTNEED)` on read-only executable mappings (never COW'd; refault from page cache) + malloc_trim + killing the net process:
 Tools closed 232/137 → **75/55** (A: trim+dontneed) or **59/38** (B: +MADV_PAGEOUT of anon, 19 MB goes to swap). Reopen after release 0.40 / 0.42 s (A / B) = same as before (0.40).
 Tools 0.4.6 already has `release_unused_runtime_pages` (src-tauri/src/resident/memory.rs) but it only does PAGEOUT of `.so` mappings; PAGEOUT skips pages mapped by another process (the NetworkProcess maps the same libs) → ineffective (measured: after-pageout 227/132).

**F4. The FUSE-mounted AppImage costs far more than its stub.** Same Tools build, fresh `--background`, 20 s (btrfs-backed unpacked copy vs AppImage), RSS/PSS MB:
 AppImage 50/43 + FUSE stub 23/22 = **73/65**;  unpacked `AppRun` **17/10** + no stub. Mapped file pages are 4–7x more resident through FUSE
 (binary 11.9 MB vs 1.7; libwebkit 7.6 vs 0.6). All 8 stubs = 111 MB RSS/PSS (they hold squashfuse caches, 16–21 MB anon each; swapped out when idle).
 (An earlier "ext" run from a tmpfs path was invalid: tmpfs pages were swapped out. Re-done on btrfs `/var/tmp`.)
**F5. Negative results (measured, bg fresh):** `MALLOC_ARENA_MAX=2` saves 0–2 MB; `MALLOC_ARENA_MAX=1 TOKIO_WORKER_THREADS=2` saves 1–3 MB and drops threads 39→19 (Box), 35→15 (Look), 30→20 (Tools).
 So glibc arenas and duplicate tokio runtimes are NOT a memory lever (only a thread-count/cleanliness one).
**F6. Real-desktop cycles (owner's GPU), RSS/PSS MB** (`memory/real-cycle-*.jsonl`): Box before 209/126 → open 467/300 (window mapped 0.50 s) → hidden 191/108.
 Look before 194/111 → open 438/266 (0.37 s) → +40 s released 195/112 (WebKit stays: network proc + libs). Clipboard before 181/109 → open (Settings page) 193/121 → hidden 193/121 (never gives back).
 Wheel `--preview` shown: 114/57 → 114/57 (+0: QML overlay + engine already resident while invisible). Find `--show`: 51.7/46.7 → same. Shelf `--show`: 61/50 → 62/51.
 Tools open (real GPU) 523/302; I opened it by accident with `--status` (Tools has no such flag, unknown flag opens the window — T4); closed by `--quit` and relaunched `--background`.

**F7. Trimming while the window is OPEN (Tools, Xvfb; shim runs in UI + WebKitWebProcess + WebKitNetworkProcess):** idle open 528/319 →
 `trim+dontneed` (drop executable-text pages, kernel refaults the working set): 325/228 (→ 364/254 after exercising every tab) →
 + `PAGEOUT` of anon: 207/111 but 94 MB sits in swap (285/179, swap 67 after use). Click→repaint over 8 tab switches: 145 ms before, 141 ms after, 140 ms after anon-pageout
 (probe resolution ~40 ms) → **no measurable UX change**. Per process: UI 201/113 → 102/63; WebProcess 267/173 → 181/140; NetworkProcess 60/33 → 42/26.
 Only the UI process can be trimmed by the app itself; the WebKit helpers need an `LD_PRELOAD` trimmer inherited through the AppRun environment (no WebKit API for it).
**F8. Shelf already forces the Qt Quick software renderer** (`src/main.cpp:287-292`, so no Mesa/LLVM; Shelf bg PSS 50). Wheel does not (GL → libgallium+libLLVM in a hidden process, QSGRenderThread, 12 Mesa threads).
 Neither app uses ShaderEffect/MultiEffect/DropShadow/layer effects (grep), and the shared kit's `Shadow.qml` is written to "work on the software renderer too".
**F9. Shelf's in-RAM thumbnail cache** (`src/ThumbnailProvider.cpp:22,121`): `QCache` 64 MiB default, user preference `thumbnailQuotaBytes` up to **1 GiB**, never cleared when the panel is hidden, no disk thumbnail cache.
**F10. Clipboard:** capture is driven from Dart (`app_controller.dart:363-367`, `_captureFromDesktop` → `_core.invoke('capture')`) with the native Wayland watcher in the Flutter plugin (`platform/desktop/arcade_desktop_bridge/linux/clipboard_bridge.cc`),
 so the Flutter engine/isolate must stay alive for capture; the runner keeps a *realized but unmapped* GTK window + GL context for the whole background life (`my_application.cc:62-72`).
 History is not held in RAM beyond one page: `history` returns <= `_historyLimit` (250, +250 per "load more") items, each with `text` (<= 32 KiB) + `preview` (500 chars) — bounded, but the list payload carries the full text field (see item CB-3).
 `--status/--version/--arcade-manifest` start a complete Flutter engine in a throw-away process (observed: my `--status` printed a second `dev.arcade.Clipboard` GTK instance) — ~0.5 s and ~150–250 MB transient per CLI call (item CB-4).
 Flutter 3.47.2 SDK on this machine ships `fl_engine_new_headless()` and `fl_view_new_for_engine()` (`/opt/flutter/bin/cache/artifacts/engine/linux-x64/flutter_linux/`), but "the engine must not be headless" for a view.

**F11. Flutter engine cannot shed its view (probe: stock `flutter create` app, release build, Flutter 3.47.2, Xvfb/llvmpipe so absolute MB are high, deltas matter):**
 hidden-but-realized window (what Clipboard does now) 233/166; destroy the GtkWindow+FlView and keep the engine alive: **230/166** (no change; the engine logs
 `FlutterEngineRemoveView ... The implicit view cannot be removed`); `gtk_widget_unrealize`: **306/228** (worse). Running Dart keeps ticking in all modes.
 → destroying/unrealizing the view does not free anything; the only ways to lower Clipboard are a smaller engine footprint or taking the engine out of the always-resident process (item CB-1).
**F12. Cold start of a UI process from nothing, Xvfb, warm page cache, window mapped (s), median of 4:** Tools AppImage **0.68** vs unpacked **0.31**; Box AppImage **0.92**.
 In-place reopen of an already-resident Tools after release is 0.40 s (F3). So a UI process spawned on demand from an UNPACKED payload (0.31 s) is not slower than today's in-place reopen,
 while the same spawn through the FUSE AppImage (0.68 s) is slower. (remaining apps in `res/coldstart.log`, §4.)
**F13. Tools "memory" number the owner sees** (Home dashboard, "7 running, 847 MB"): `src/dashboard.rs:47-82` sums sysinfo `process.memory()` (RSS) of ONE pid per app (from its endpoint file), no children, no stub.
 So the 800+ MB figure is a sum of main-process RSS (which double-counts the Mesa/LLVM and GTK libs mapped by every app), not what the machine pays; fleet PSS was 585–655 MB.
**F14. Haiku-collected, grep-verified source references** are in `memory/refs-tauri-qt.md` and `memory/refs-clipboard.md` (copied at the end). Notable: Box also keeps a 500-item clipboard history fully in RAM
 (`apps/desktop/src-tauri/src/clipboard_history.rs:20,92`); Look's media HTTP server thread-per-connection is never stopped (`mediaserver.rs:35-50`); Clipboard's `_items` has no Dart-side cap
 (`app_controller.dart:61,606`) beyond the 250-per-page request; there is NO size-budget pruning in the Clipboard core today (only `retention_hours` 24 and `max_items` 500, clamp 10 000: `store.rs:16,1005-1046`).

**F15. Bare Flutter engine on the owner's GPU** (stock app, hidden realized window, Wayland): **156/73** (RSS/PSS, anon 47, 38 threads). Clipboard's own Dart/Rust/plugins add only +25/+36
 (Clipboard bg 181/109). So even a perfectly lean Clipboard stays >= 156/73 while the Flutter engine lives in the resident process.
 With the trim shim on that probe: `dontneed` 95/60; + anon page-out 68/33 (18 MB to swap).
**F16. Startup-only trim of unpacked Rust apps (one `trim+dontneed` after init; stays down 30 s idle):** Box 85/78 → **45/42**, Look 54/47 → **34/30**, Tools 17/10 → 12/9.
 (Unpacked `Box` is bigger than Tools at bg because its 64 MB executable keeps 31 MB of pages resident after start; the trim removes them.)
**F17. WebKit software compositing (`WEBKIT_DISABLE_COMPOSITING_MODE=1`) on the owner's real Wayland session** (isolated instance, window open idle, RSS/PSS):
 Look **487/280 → 409/228**, Tools **532/322 → 453/267** (threads 138 → 96); window mapped 0.66→0.57 s (Look), 0.77→0.72 s (Tools). Static frames are pixel-identical (`memory/shot-real-look-{comp,nocomp}.png`).
 Not verifiable by screenshot: scroll/animation smoothness and video playback (Look), so this lever needs the owner's eyes before it ships.
**F18. Wheel software renderer (Xvfb):** GL (current) 142/83, 34 threads → `QT_QUICK_BACKEND=software` **70/59, 9 threads** (RSS −72, PSS −24; on the owner's GPU the PSS gain is smaller because radeonsi is lighter than llvmpipe: real Wheel 114–143 / 57–86).
 Unpacked Wheel (GL): 148/65 (stub gone: −22 RSS/PSS from the stub only).

### A.3 Session 2 findings (2026-11-07), appended as measured
**Machine rebooted between sessions** (uptime 2 min at resume): scratchpad (tmpfs) wiped, harness rebuilt from `memory/tools`, `/var/tmp/arcade-audit-ext` survived. After the reboot only Tools, Find, Look, Shelf, Wheel had autostarted (Box, Lens, Clipboard were not running; I did not start them: that is the owner's post-reboot state).
**F19. Fresh-boot real fleet (5 apps, ~2 min after login):** RSS 351 / PSS 282 + FUSE stubs 106/106 (Find 43/40, Look 60/54, Shelf 72/62, Tools 63/57, Wheel 113/70; stubs 16–23 each). Freshly started apps are small; the aged state (used once, closed) is what reaches 1084/655.
**F20. All 8 apps together on Xvfb, installed AppImages, stub attribution fixed** (`fleet-image2.log`): bg+70 s apps 753/599 + stubs 170/168 = **922 / 767**; Tools window open **1386 / 1028**; Tools closed +45 s **1081 / 821** (Tools 218/122 stays, never returns).
 (The lab PSS total 767 equals the real desktop's aged 767, so the lab fleet is a faithful stand-in for the CI check; Xvfb makes Clipboard 249 PSS instead of ~110.)
**F21. Aged release, unpacked payload (Xvfb, `exp_rel.py APP trim+dontneed --ext`), RSS/PSS:**
 Tools bg 17/10 → open 540/320 (mapped 0.45 s) → closed+35 s 203/104 (UI 133/54 + net proc 69/50) → net-proc kill + trim + dontneed **60/38**; reopen **0.30 s**; open again 466/294.
 Box bg 76/69 → open 541/324 (0.47 s) → closed+35 s 258/153 → released **92/71** (anon 34, thr 69); reopen **0.24–0.26 s**, open 446/280. (AppImage Box was 94/74, reopen 0.38–0.46 s: unpacked is faster.)
**F22. Two WebKit windows at once (Tools + Look, unpacked, Xvfb):** separate copies **1035 / 535**; Look's byte-identical libs hardlinked to Tools' inodes (121 files, 251 MB, one page cache) **1027 / 488** → shared payload saves **47 MB PSS, 0 RSS** (PSS only, and only when two WebKit apps are open together).
**F23. CI check run #1 (current installed builds, Xvfb, `memory/ci-run-current.json`, log `memory/logs/ci-run.log`):** the script runs end to end (8 apps together, window open, close, settle) and exits 1 with 22 budget failures, as it should today:
 background fleet 922 RSS / 750 PSS (apps + stubs), Tools window open 1391 / 1014, 30 s after close 847 / 683 (target budgets 680/540 under Xvfb). It also exposed a bug in my close step: `xdotool windowclose` destroys the X window and the app exits (Tools row read 0),
 `windowquit` does nothing without a window manager; fixed by shipping `wmclose.c` (sends WM_DELETE_WINDOW, what a title-bar button sends) next to the script. Re-run after the fix: see §7.
**F24. WebKit window floor (probe: python + system WebKitGTK 2.54, 940x780, 300 rows of static text, Xvfb):** UI process 191/88 + WebKitWebProcess 200/109 + WebKitNetworkProcess 65/32 = **456 RSS / 229 PSS** for an EMPTY page (compositing off: 452/227).
 Tools open (528/319) is only +72/+90 above that floor, trimmed Tools (325/228) is at the floor. So the fixed cost of one WebKit window is ~450 RSS / ~230 PSS before the app adds anything; RSS < 500 with a WebKit window open cannot be reached by trimming the app, only by moving pages out of RSS (trim/page-out).
**F25. Look hot re-show (AppImage, Xvfb):** first show (cold webview) 0.65 s; hide → show within 30 s (webview alive) **0.15 s** (3 runs: 0.154/0.151/0.152); after the 30 s release **0.29 s**. The keep-warm window is worth 0.14 s on every quick re-open: do not shorten Look's 30 s / Box's 15 s.
**F26. Fleet, Tools as the open window, unpacked + shim, Xvfb (`fleet-neutral-tools.log`; Clipboard here is llvmpipe 220/193, substitute ~120/96 for the real GPU):** bg 734/570 → after trim+dontneed **471/403** → Tools window open **958/691** → open + trim of all processes **716/572** → closed+30 s 544/443 → released **482/401**.
**F27. Box's own motion probe (`ARCADE_MOTION_PROBE`, `lib.rs:1236`, `motion-probe.js`: requestAnimationFrame intervals for 700 ms after every Island show), compositing on vs off** (raw: `memory/logs/box-motion-*.txt`):
 *Xvfb:* on avg 16.1–17.3 ms, max 31–69, 1 frame >25 ms per show; off avg 15.9, max 17–19, 0 frames >25 ms (off is smoother there: software-timed).
 *Real Wayland session, owner's GPU:* **on** avg 14.5–16.2 ms, p95 15–17, max 45–46 (first 113), 1 frame >25 ms per show; **off** avg 15.0–15.4, p95 **19–22**, max 31–40, 1 frame >25 ms per show.
 → on the real GPU, software compositing makes the Island animation slightly less even (p95 +4–5 ms, average +0.5 ms) with the same count of slow frames. Not identical, so OPEN2 is **not** UX-neutral for Box; unverified for Tools (no animations measured) and Look (video/PDF).
**F28. JavaScript JIT off (`JSC_useJIT=false`) on an open Tools window (Xvfb):** 513/298 → 505/291 (−8/−7): not a lever (and it slows heavy JS).
**F29. WebKit app as the open window, fleet lab (unpacked + shim, all others at trimmed-fresh level, Clipboard = llvmpipe 220/193; `fleet-{neutral,nocomp}-{tools,box,look}.log`), app tree RSS/PSS:**
 open+trimmed (UX-neutral): Tools **292/210**, Box **295/207**, Look **285/193** (fleet 716/572, 695/549, 689/537); + compositing off: Tools **242/170**, Box **243/165**, Look **236/155** (fleet 663/532, 638/499, 645/505).
 closed + released in place: Tools 60/39, Box 93/72, Look 82/61 (fleet 481/401, 491/410, 487/408).


## Appendix B. Evidence index
- Raw samples: `memory/lab-*.jsonl` (per scenario), `memory/fleet-*.jsonl` (all-8 lab fleets), `memory/real-cycle-*.jsonl` (real desktop show/hide cycles), `memory/{box,look,tools}.jsonl` + `memory/real/*.txt` (first session), `memory/ci-run-current.json` (CI run).
- Logs: `memory/logs/*.log` (queue outputs: hot re-show, two-window, release runs, fleet runs, CI run), `memory/logs/box-motion.txt`. Screenshots: `memory/shot-*.png` (incl. real-Wayland Look compositing on/off).
- Source references: `memory/refs/refs-tauri-qt.md`, `refs-clipboard.md`, `refs-launch.md` (302 of 305 quoted lines re-verified mechanically against the source; the rest read by hand). Flutter probe sources: `memory/probe/`.
- Harness: `memory/tools/` — `lab.py` (isolated lab on Xvfb; `LAB_DISPLAY` picks the display), `scenario.py`, `exp_rel.py` (release experiments, `--ext` for the unpacked payload), `fleet_lab.py` (all 8 together), `bg_trim.py`, `bgenv.py`, `coldstart.py`, `hot_reshow.py`, `twowin.py`, `box_motion.py`, `real_cycle.py`, `real_nocomp.py`, `appmaps.py`, `fleet.py`, `trimshim.c` (the `LD_PRELOAD` release shim used for every "trim" number), `wmclose.c`, **`fleet_budget.py` + `budgets.json` (the CI check)**. Scripts keep the first session's scratchpad path in a constant `S`; change it before reuse.

## Appendix C. Session log, incidents, cleanup, desktop check, handoff notes (historical)


> # HANDOFF — read this first (written 2026-11-06 end of session 1; resume from here)
> Task (unchanged): research only, NO code changes. Find what must change in each Arcade app so ALL apps together stay < 500 MB (PSS **and** RSS, FUSE stubs included) with no UX loss
> (owner rule: UX must not get worse). Deliverable = this file: measured table, ranked work items (app, cause, evidence file:line, MB now→after, fix, UX risk), per-app budgets for
> (A) all 8 in background and (B) all 8 + one window open, a plain reachability statement, CI conformance check. Final message = compact ranked list + projected fleet totals.
>
> **PSS vs RSS:** RSS = RAM pages a process has mapped, shared libraries counted in full in every process (what htop/btop and Tools' dashboard show; sums double-count).
> PSS = shared pages split among the processes sharing them (sum ≈ RAM really used). The owner's "800 MB+" is RSS-like: Tools Home sums sysinfo RSS of ONE pid per app (`src/dashboard.rs:47-82`), no children, no stubs.
>
> ## Status
> **Measured & logged (do not redo):** §2 findings F1–F18 below, raw data `memory/lab-*.jsonl`, `memory/fleet-*.jsonl`, `memory/real-cycle-*.jsonl`, `memory/logs/*.log`, screenshots `memory/shot-*.png`,
> Haiku source refs `memory/refs/refs-{tauri-qt,clipboard,launch}.md` (302 of 305 quoted lines re-verified against source), Flutter probe `memory/probe/`, harness `memory/tools/` (scripts still point at the old scratchpad path `S=`; change it).
> Additional results NOT yet written into F-log (all measured):
> - **Unpacked Tools, aged cycle (Xvfb, `exp_rel.py tools trim+dontneed --ext`):** bg 17/10 → open 540/320 (window mapped 0.45 s) → closed+35 s **203/104** (UI 133/54 + net proc 69/50) → kill net proc + trim + dontneed **60/38** → reopen **0.30 s**, open again 466/294. (RSS/PSS MB)
> - **Fleet, all 8 together on Xvfb, unpacked** (`fleet-ext.log`): bg 755/564; **after one trim+dontneed 448/372** (Clipboard alone 220/193 because llvmpipe; other 7 apps total 228/179). Tools window open adds ~+515/+305 (532/320).
>   Per app unpacked bg → trimmed (RSS/PSS): Tools 34/27→17/13, Box 85/78→48/44, Look 60/53→37/33, Find 31/26→17/14 (small lab index; real index adds ~25 MB), Lens 12/10→6/5, Shelf 75/63→43/36, Wheel(GL) 132/58→62/34, Clipboard 326/249→220/193.
> - **Fleet AppImage (stub attribution BUG, stubs read 0):** apps only 753/586 bg; Tools window open 1215/844; closed+45 s 912/656. Re-run needed for stubs (see TODO 3). Lab stubs when active: Tools 22, Box 21, Look 22, Find 19, Lens 17, Clipboard 19, Shelf 23, Wheel 22 MB (real: 112 total, partly swapped).
> - **Cold start from nothing, Xvfb, median s (AppImage → unpacked):** Tools 0.68→0.31, Box 0.92→0.46, Look 0.67→0.32, Clipboard 0.29→0.22. Real-GPU stock Flutter exec→first frame 0.22 s (5 runs). In-place reopen after release: Tools 0.40/0.30(unpacked), Box 0.38–0.46, Look 0.32–0.34.
> - **Compositing off, Xvfb, open idle:** Tools 529/312→420/238, Box 531/315→432/251, Look 507/291→401/221. **Open + trim + compositing off (Tools, Xvfb):** 415/233 → 259/168 (→297/201 after exercising tabs) → with anon page-out 174/86 (68 MB swap).
> - **Wheel `--preview` first draw on Xvfb:** GL 142/83 → 227/160 (+85 MB, never released); software backend 70/59 → 81/65. Wheel unpacked+software+startup-trim 42/26.
> - **Xvfb inflates Flutter** (llvmpipe): Clipboard 321/256 bg vs 181/109 on the real GPU; `GALLIUM_DRIVER=softpipe` worse (443/382), `LP_NUM_THREADS=1` 321/239; Flutter has no software-render switch (probe ignored `enable-software-rendering`). CI needs a separate Clipboard allowance until its engine leaves the resident process.
> - Fonts/icons: ≤1.4 MB PSS fonts per process, icon-theme caches ≤0.2 MB (negligible). Fleet swap on the real desktop 128–140 MB.
>
> **NOT done (the interruptions killed the queued lab scripts; run only these):**
> 1. `exp_rel.py box trim+dontneed --ext` and `... look ...` (aged unpacked release numbers for Box/Look; Tools done above). Optional: `scenario.py tools|box --cycles 3 --env MALLOC_ARENA_MAX=1 --env TOKIO_WORKER_THREADS=2` (arena/tokio effect with window open; bg effect already ≤3 MB, safe to skip).
> 2. `hot_reshow.py look` / `box` (re-show latency while webview still alive vs after release) and `twowin.py ext` / `twowin.py hl` (Tools+Look windows both open: separate unpacked copies vs Look's identical libs hardlinked to Tools' inodes; Tools & Look ship byte-identical libwebkit/JSC/GTK/ICU, 221 MB; Box has a different WebKit build).
> 3. `fleet_lab.py image2 --open tools` (AppImage fleet with fixed stub attribution; script already fixed).
> 4. Write-up steps: consolidate §3/§4 (below, from this handoff), **run the CI script once** (`memory/tools/fleet_budget.py --apps ~/Applications/Arcade --open tools --json out.json`; stub attribution fixed, never executed yet; expect budget failures with a budgets file), cleanup, desktop check, final ranked list.
> Resume: `lab.py` starts Xvfb :95 and private dbus by itself; unpacked payloads must still exist in `/var/tmp/arcade-audit-ext/<App>/squashfs-root` (needed for items 1–2). Scratchpad is on tmpfs `/tmp` (lost on reboot): if gone, re-extract with `Arcade-X.AppImage --appimage-extract` into /var/tmp (NOT /tmp: tmpfs pages get swapped and give invalid numbers).
>
> ## Draft ranked work items (fill numbers from F-log; verify file:line in source; orchestrator verifies too)
> | id | app | fix | now → after (RSS/PSS MB, measured unless "est") | UX risk / keep UX equal |
> |---|---|---|---|---|
> | U1 | all 8 (Link + packaging) | **Unpacked runtime**: AppImage keeps single-file distribution but its runtime extracts once to `$XDG_DATA_HOME/arcade/runtime/<app>-<ver>-<hash>/` then `execv`s AppRun with `APPIMAGE`+`APPDIR` set (no FUSE daemon, no stub). Touch points: `Link manifest.rs:486-504` (`current_executable` needs exe under `$APPDIR`), `install.rs:263,323` (Exec=), `client.rs:372-393` (spawn), Tools `installation.rs:121-139`/`platform.rs:370-400`/`paths.rs:118-135`, Clipboard `main.cc:49-90` (release_mount_handles), `childenv.rs:21-36`. | stubs 112/112 → 0 (lab 166); Tools fresh bg 73/65 → 17/10; cold start −0.37/−0.46/−0.35 s | disk +1.5 GB uncompressed (btrfs zstd ≈ 0.6 GB; ext4 full), first launch extracts (once per version), GC old dirs. Same entry path/env contract, starts faster. |
> | W1 | Tools, Box, Look | After last window is destroyed: SIGTERM child `WebKitNetworkProcess` (kept alive by tauri-runtime-wry `lib.rs:2575-2590`, tauri#14626), `malloc_trim`, `madvise(DONTNEED)` on read-only executable mappings. Tools `desktop.rs:253-262`+`resident/memory.rs:4-15` (today PAGEOUT only, skipped for pages shared with the net process), Box `lib.rs:1469,1506-1517` (malloc_trim only), Look `app.rs:340,725-760` (nothing). Put one shared fn in Link. | Tools 232/137 → 75/55 (AppImage), **60/38 unpacked**; Box 256/160 → 94/74; Look 245/150 → 79/59; real aged: Box 191/108, Look 195/112 | reopen unchanged (Tools 0.40/0.30 s, Look 0.32 s). Only data in network process is localStorage onboarding flag (Tools `Onboarding.svelte:56`): delay kill ≥3 s, use SIGTERM. |
> | W0 | Tools, Box, Look, Find, Lens, Shelf, Wheel, Clipboard | One `trim+dontneed` after init (and each hide) | Box 85/78→45/42, Look 54/47→34/30, Shelf 74/62→41/35, Wheel 76/40→42/26, Find 23/18→10/8, Lens 15/12→6/4, Tools 17/10→12/9 | none measured |
> | WH1 | Wheel | `QQuickWindow::setGraphicsApi(Software)` exactly like Shelf `main.cpp:287-292` (no ShaderEffect/MultiEffect in Wheel/Shelf QML; kit `Shadow.qml` is software-safe). Wheel `main.cpp:416` creates the overlay at start; software also removes GL first-draw (+85 MB). | Xvfb 142/83 → 70/59; real 114–143/57–86 → est 70/45 | visual diff screenshot of `--preview` still to be taken on real session; no effects used |
> | CB1 | Clipboard | Flutter engine cannot shed its view (probe: destroy view 230/166 = no change, unrealize worse; `FlutterEngineRemoveView` refuses implicit view). Bare engine on real GPU 156/73; Clipboard 181/109. Options: (a) in-process trim only: est 120/96 (probe 156/73→95/60); (b) move capture + store to a small resident daemon (capture already out of process: `arcade_clipboard_wl_capture` watcher started from Dart `desktop_adapter.dart:603-606`, Dart orchestration `app_controller.dart:363-367,1410-1510`, Rust core already a library) and spawn Flutter UI on demand: est 30/22 | (a) −60/−13; (b) −150/−87 | (b) first open after idle 0.11 s → ~0.25 s (cold start 0.22–0.29 s) — breaks "same latency" unless a keep-warm window (e.g. 60 s) is used; large engineering. Recommend (a) now, (b) optional for scenario-B headroom. |
> | CB2 | Clipboard | `--status/--version/--arcade-manifest/--restart` boot a full Flutter engine in a throw-away process (`my_application.cc:171-181`, `main.cc:84-92` only `--help` fast-paths) → answer from C++ or the Link socket | transient ~150–250 MB and ~0.5 s per CLI call → 0 | none |
> | CB3 | Clipboard | list rows carry `text` (≤32 KiB) + `preview` (500 chars) but UI renders only `preview` (`history.dart:330`, `quick_overlay.dart:321`): drop `text` from list payload; `Image.memory` without `cacheWidth` (`clipboard_widgets.dart:300`) and no `imageCache` limit (default 100 MiB): set 24 MiB; window the `_items` list (`app_controller.dart:61,606`, `_historyLimit` +250) | bounded, only matters with big clips/Forever | none |
> | CB4 | Clipboard | owner decision (Forever, 256 MB budget, prune oldest unpinned) does not exist yet: only `retention_hours` 24 and `max_items` 500 clamp 10 000 (`store.rs:16,1005-1046`, `core.rs:1558-1567`); search decrypts every row (`store.rs:456-560`) — streaming, not retained; keep it that way, keyset-paginate, prune by `SUM(length)` | RAM stays bounded if only previews cross FFI | none |
> | SH1 | Shelf | thumbnail `QCache` 64 MiB default, preference up to 1 GiB, never cleared on hide (`ThumbnailProvider.cpp:22,121-125`): default 16 MiB, hard cap 64 MiB, `clear()` on hide | up to −64 MB when thumbnails shown | re-decode on reopen (2 pool threads) |
> | FD1 | Find | index 25.3 MB heap (`indexBytes` 25266078 for 302 671 entries; `index.rs:170` counts capacity, no `shrink_to_fit`); optional mmap + page-out when idle | −2…−5 (shrink), −20 optional | optional: first search after idle faults pages |
> | TD1 | Tools | dashboard "847 MB" = RSS of main pids; show tree PSS (incl. children/stub) so the owner sees what the machine pays (`dashboard.rs:47-82`); Home polling 2 s (`Home.svelte:69-71`, 6 % CPU open) | metric only | none |
> | LK1 | Look | big text/markdown peak: 5 MB .txt/.md → 880/654 (prior notes `memory/look.jsonl`), returns to 180 after 30 s; virtualize/truncate; `mediaserver.rs:35-50` thread-per-connection never stopped (minor) | peak −300 est | none if truncated banner already exists |
> | BX1 | Box | 500-item clipboard history fully in RAM (`clipboard_history.rs:20,92`), job results kept (`link.rs:183-189` no cap), 24 tokio workers | few MB | none |
> | NEG | all | glibc arenas, tokio worker caps: measured ≤3 MB (threads 39→19) — do not prioritise | | |
>
> ## Draft projection (estimates marked; recompute from final data)
> * Baseline real aged fleet: RSS 1084 / PSS 655 + stubs 112/112 = **1196 / 767** (22:13 snapshot; at 23:00 with Tools fresh 907/585 + 121).
> * **Case A, all 8 background**, after U1+W1+W0+WH1+SH1 (+CB1a): per app RSS/PSS ≈ Tools 45/35, Box 70/55, Look 55/45, Clipboard 120/96, Wheel 45/27, Shelf 43/36, Find 40/38, Lens 6/4 = **≈ 424 / 336** (<500 both, no UX loss). With CB1b (Clipboard daemon): ≈ 334 / 262.
> * **Case B, + one window open** (PSS fits; RSS does not for WebKit): PSS ≈ 336 − bg + open(168–228) ≈ 470–490 (A) or ~410 (CB1b). RSS: UX-neutral set (W1 + open-time trim, Tools open 325/228) ≈ 424−45+325 ≈ **700**; + compositing off (259/168) ≈ **640**; + anon page-out to swap (174–233/86–141) ≈ **550–610**; + Clipboard daemon on top ≈ **460–520**. => **RSS < 500 with a WebKit window open is not reachable without a visible trade-off** (compositing off needs owner's eyes; swap-assisted page-out risks swap-in jank; CB1b adds 0.15 s). If the open window is Wheel/Shelf/Find/Lens/Clipboard(in-process) it IS reachable (+0…+12 MB).
>
> ## One-window-open options per WebKit app (analysis so far; owner asked for a concrete list, recommend the one with no visible UX cost)
> 1. *Release the other hidden WebKit apps' webviews while one is open* — already true after W1: hidden apps destroy their webview (Tools on close; Box immediately on window close but 15 s on Escape/shortcut hide, `lib.rs:1286,1459`; Look 30 s, `app.rs:37`). Shortening Box's 15 s / Look's 30 s only helps if two apps are used within that window and removes the hot re-show (0.05–0.1 s → 0.3–0.4 s): visible cost, gain 0 in steady state → do not shorten. (hot re-show numbers: TODO 2)
> 2. *Shared WebKitNetworkProcess* — WebKit runs one network process per UI process; sharing across apps would need one host process for all three apps (architecture rewrite). With W1 only one network process (33 PSS/60 RSS) exists when one window is open → gain 0. Reject.
> 3. *Smaller WebKit caches / memory-pressure settings* — `WebKitCacheModel`, `webkit_web_context_set_memory_pressure_settings` (WebKitGTK ≥2.34) to cap the WebProcess (anon ≈100 MB: JSC + bmalloc): needs a prototype in each app (apps load local assets, so cache shrink has ~no UX effect; GC pressure may cost jank). **Unmeasured** — the one candidate still needing a measurement (est −20…−40 RSS/PSS).
> 4. *Drop a hidden app's WebProcess right after hide* — same as 1 (already destroyed with the webview; leftover is the net process + library pages, covered by W1).
> 5. Measured levers for the open window itself: **open-time trim** (UI process self-trim + `LD_PRELOAD` trimmer for the WebKit helper processes; Tools 528/319 → 325/228, click→repaint unchanged 145→141 ms) = **no visible UX cost → recommended**; compositing off (−79/−55 on the real GPU, pixel-identical stills, motion/video unverified) = needs owner's eyes; anon page-out (−120/−120 but 70–95 MB to swap) = reject unless RSS target is strict.
>
> ## Cleanup still to do (nothing cleaned yet)
> `/var/tmp/arcade-audit-ext` (1.5 GB; keep until TODO 1–2 done), 50 run dirs `/tmp/arl-*`, scratchpad `lab/` (129 MB), `flt/probe` build, copies are already in `memory/tools`. Xvfb :95 and lab dbus daemons are NOT running (killed by the interruptions); Xvfb :93/:92 belong to earlier agents — leave them.
> **Real desktop now == baseline:** windows = Shelf edge tab + T3 Code; all 8 apps running in background; Clipboard Private mode untouched; clipboard content never changed. Tools was restarted once by me (accidental window via `--status`, closed with `--quit`, relaunched with the mandated systemd-run command, now pid 339405). Lens has a pre-existing defunct child.
>
> ## Gotchas learned (save time)
> Tools has NO `--status` (unknown flag opens the window, T4) — never call it. `pkill -f '<pattern>'` can match and kill your own shell. /tmp is tmpfs (unpacked tests must live in /var/tmp). Lab `XDG_RUNTIME_DIR` must be short (sockets <108 bytes; Wheel failed on a long path; `lab.py` now uses `/tmp/arl-<hash>`). AppImage FUSE stub is NOT a child of the app (find by exe `*.AppImage` + HOME env). Overlay windows of Wheel/Shelf/Find/Lens are not detectable on Xvfb. Real-desktop apps that touch Hyprland rules (Box `hyprland.rs:70`) must not be run against the real compositor. Do not run an isolated Clipboard on the real session (its watcher would copy the owner's clipboard).



