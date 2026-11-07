# Real-desktop release audit — 2026-11-06

Owner desktop: CachyOS, Hyprland 0.56.2 (Lua), 2560×1440 at scale 1. Installed AppImages; sources read only. Exact-address `hl.dsp.send_shortcut` and the existing `hk.sh` virtual pointer method were used. No source edits, installs, uninstalls, commits or branch changes.

Evidence: `real/<app>/`. Private state snapshots and disposable fixtures: session scratchpad `real-audit/`. Baseline captured in `baseline-procs-round3.txt` and `baseline-clients-round3.json`. Existing Box dashboard, Nautilus AUDIT window and Shelf right-edge tab were present before this pass.

Portal testing limitation: `hl.dsp.global` sends Activated only (no Deactivated). Hold/release and repeat toggles cannot be judged from it alone. Existing owner keybindings were not changed. The owner’s no-close-on-pointer-leave rule overrides the older focus-loss wording in the specs.

Results below are updated during testing.

## Clipboard 0.3.2

### RD-CLIPBOARD-1 — UX — Picker closes merely on pointer leave
Steps: focus a disposable `kitty` running `cat`; open `Arcade-Clipboard.AppImage --overlay` (the installed Ctrl+Shift+Space path); move the pointer inside (1200,580), then send a real virtual-pointer motion -450,0 into the terminal. No click and no key. With `input:follow_mouse=1`, focus moves to the terminal and the 480×520 pinned floating picker disappears after the 400 ms grace period. A cursor warp alone did not transfer focus, so that alone was insufficient to reproduce. Clicking the Box window also dismissed it.
Expected: pointer leave and transient focus changes leave the picker open; only explicit dismissal/selection closes it. Actual: blur dismisses it. Evidence: `real/clipboard/picker-before-leave.json`, `picker-after-real-motion.json`, `picker-after-focus-click.json`, `picker-header-before-leave.png`. Confirms CB-3 / C0. Suspected source: `App dev/Arcade-clipboard/apps/flutter_app/lib/platform/desktop_adapter.dart:248-258` (`onWindowBlur`) → `services/app_controller.dart` (`onOverlayFocusLost`/`dismissOverlay`).

### RD-CLIPBOARD-2 — polish — Shortcut recorder chips stretch into full-width rows
Steps: Settings → Shortcuts on the real 1228×693 tiled window. Expected: compact Ctrl / Shift / Space key chips as used in the History footer. Actual: three ~900 px bordered bars stack vertically, making the shortcut look like three separate fields and consuming 80 px. Evidence: `real/clipboard/settings-real.png`. Suspected source: `apps/flutter_app/lib/design/arcade_widgets.dart` shortcut recorder layout under the desktop Settings constraints.

Verified working:
- Picker is pinned/floating, centred at (1065,460), 480×520; it receives keyboard focus. Search filters the audit clip, Enter closes and pastes the exact 29-character marker into a disposable GTK TextView. (`gtk-sink-text.txt` private scratch evidence.) The custom-class test terminal receives Ctrl+V rather than its Ctrl+Shift+V; native GTK is the valid paste check.
- Runtime Hyprland Lua bind exists for the retained owner shortcut Ctrl+Shift+Space (modmask 5, description `Arcade Clipboard ACBMESH_SIGNAL_…`); no portal shortcut is expected for this backend.
- Pin/unpin and the Pinned filter work. Settings shows the real compositor supports shortcut/paste (the isolated unsupported warning is absent). Start at login is OFF, consistent with null receipt autostart. No change made to it.

## Look 0.4.3

### RD-LOOK-1 — broken — Escape becomes trapped after closing the connected-actions menu
Steps: Space-preview a text file from Nautilus → A → Escape (menu closes, focus returns to its A button) → F → Escape. Expected: Escape exits fullscreen, then another Escape closes the preview. Actual: the fullscreen 2560×1440 window stays fullscreen; more Escape presses are swallowed by the now-closed menu’s button. Click the preview body first and Escape works again. Evidence: `real/look/actions.png`, `fullscreen-before-escape.json`, `fullscreen-after-escape.json` (both fullscreen=2). Reproduced twice; F still toggles. Source: `Rust/arcade-look/src/lib/actions-strip.ts:66-70`: the root handles Escape even when `closed`, stops propagation and refocuses the button. `src/app.ts:784-787` has the correct fullscreen-back behaviour but never receives it.

Verified working:
- Nautilus selects the audit text file → Space → floating 1040×720 preview at (785,360), proper filename title and readable text. No tiling. `real/look/space-preview.png` (crop was offset 25 px; actual geometry from clients).
- Pointer moves out via virtual-pointer motion and focus follows into the harmless file manager; Look remains mapped (`real/look/after-pointer-leave.json`).
- A shows live actions from Box, Clipboard, Shelf and Wheel, with app glyphs and number-key hints. First Escape closes only this menu.
- F enters/exits compositor fullscreen. With focus in the preview body, Escape exits fullscreen one level before closing.

## Round 4 (continuation) — method notes

Disposable windows: `kitty --class arcade-audit-term` (floated/moved by exact address with `hl.dsp.window.float/resize/move({window='address:…'})`), a GTK drop target and drag source (scratchpad `rd4/`). Keys to layer-shell surfaces (Find overlay has no window address) were sent with `hl.dsp.send_shortcut` *without* a window only after checking that the layer exists and that the focused window was my own terminal, so a miss could only hit the disposable terminal. Baseline process list: `rd4/baseline-ps.txt`; clipboard was empty at start.

## Find 0.5.4

### RD-FIND-1 — UX — Escape with text in the search closes the overlay and discards the query in one press
Steps: `Arcade-Find.AppImage --show readme` (the installed Ctrl+Alt+F bind runs `--toggle`), results listed, press Escape once.
Expected (owner rule): search with text → first Escape clears the text, only an empty search closes. Actual: one Escape hides the overlay and the query is gone (`--status` shows `visible:false, query:""`). Footer says "Esc Close", consistent with the code, but not with the fleet rule. Tab (actions) and Ctrl+I (details) each go back exactly one step to results; that part is right.
Evidence: `real/find/f-results-footer.png`. Source: `crates/arcade-find/src/ui/model.rs:984-990` (Results mode Escape: clears only the range anchor, else `Effect::Hide`). Needs "if input non-empty → clear text and re-search".

### RD-FIND-2 — polish — "Start at login" is ON while the helper text says Hyprland ignores autostart entries
Steps: Settings → General on Hyprland. Actual: the switch is on, and the row reads "Hyprland doesn't run autostart entries itself: add exec-once = arcade-find --background to your Hyprland config." The toggle therefore claims a state that does nothing here (Tools starts the apps instead). Expected: show it off/disabled or reword ("saved, but Hyprland needs…"). Evidence: `real/find/s-general.png`. Source: `crates/arcade-find/src/settings_ui.rs` (Start at login row).

### RD-FIND-3 — polish — Other drives are listed by raw mount path/UUID
Steps: Settings → Locations. Actual: "Other drives" shows `/mnt/c18bd31c-8ecf-4eb1-9c8c-97e27314658f` as the title with no label/size. Expected: volume label or device name. Evidence: `real/find/s-Locations.png`.

### RD-FIND-4 — polish — Index page says "Took 0 s." for a 500 ms scan
Steps: Settings → Index. `--status` has `lastScanMs: 500`. The row rounds down to 0 s. Show ms below 1 s. Evidence: `real/find/s-Index.png`.

### FD-2 re-assessed on the real desktop (not reproduced by pointer motion)
Find on Hyprland is a wlr-layer-shell surface (`hyprctl layers`: level 3 `dev.arcade.Find`, 760×436 at (900,502), `KeyboardInteractivity::Exclusive`, `ui/wayland.rs:297`). With `follow_mouse=1`, a real virtual-pointer motion from the overlay into the disposable terminal and even over T3 did NOT move keyboard focus and the overlay stayed open (status `visible:true`, query kept). A real click on another (floating) window while the overlay was open did not close it either; Hyprland keeps the exclusive keyboard focus. So FD-2 (`ui/wayland.rs:565-592`, keyboard `leave` → hide after 150 ms) only fires when something else takes the keyboard (another layer surface, workspace switch, notification focus). The owner's pointer-leave rule is satisfied on this desktop; the leave handler is still wrong in principle for those other focus changes but I could not trigger it safely. Evidence: `real/find/f-overlay` sequence (statuses recorded in this section).

### F1 re-assessed — not reproducible, shutdown noise
`~/.local/share/arcade/find/logs/arcade-find.log` line: `tray: failed to register to the StatusNotifierWatcher: NoReply: Remote peer disconnected` + `other error during loop operation` appears once, 2026-11-03 17:26:42, immediately after `0.5.3 stopped` (session/bus going away). Current process (pid 24736) reports `tray:"hosted"` and its menu is served by Tools. No evidence of a register failure while running. Downgrade to log-noise (the message is printed after "stopped").

Verified working (Find):
- Tray menu (hosted by Tools): Open Arcade Find opens the empty overlay (136 px); Settings opens the settings window; Rescan index runs and updates `lastScan` in <1 s. Menu has Open/Settings/Rescan/Restart/Quit (Restart/Quit not clicked).
- Quick Look action (Enter on a result, or Tab → first action) opens Arcade Look floating at (785,360) 1040×720 with the file; Escape on Look returns focus to the previous window.
- Tab shows 13 actions: Quick Look, Open, Show in folder, Copy path, Copy file, More in Arcade Box…, Send to my devices, Add to Shelf, Add to Wheel, Rename, Pin, Details, Move to Trash. Copy path lands on the Wayland clipboard (`wl-paste` = exact path, UTF8_STRING/text/plain); Copy file lands as `text/uri-list` `file:///…/real-desktop.md` (no `x-special/gnome-copied-files`; Nautilus paste-as-copy not tried).
- Escape goes back one step from Actions and from Details to Results.
- Settings pages General, Shortcuts (shows runtime Hyprland bind `CTRL ALT, F, exec, …Arcade-Find.AppImage --toggle`, toggle ON, matches `hyprctl binds`: modmask 12 key F), Locations, Index, Connected apps (all six peers "Connected"), About all render; Escape closes Settings from any page. Light theme re-themes Settings and overlay (earlier screenshot `real/find/light-overlay.png`); System theme follows `prefer-dark`.
- After Tools was restarted (it hosts the tray), Find's `--status` still reported `tray:"hosted"` and the Tools menu again listed Find: Find re-registers with a returning host, so F1 does not break tray hosting.
- Index: 302k files, 51,053 watched folders, 0 unwatched; idle 0.1 % CPU, 32 threads, 62 MB.
- Fixed a leftover from the previous pass: Find's `theme` setting had been left at `dark`; restored to the owner's original `system` (settings file now identical to `real-audit/settings-find-before.json`).

Not tested (Find): drag of a result row to a peer. A control with a GTK drag source → GTK drop target using the same virtual-pointer `drag` also produced no drag-motion on the target, so this driver cannot start Wayland DnD and the Find result is unknown. Add to Shelf/Add to Wheel/Send to my devices actions not clicked (they would modify owner data).

## Lens 0.4.2

On Hyprland the capture overlay is an ordinary fullscreen (fs=2) 2560×1440 xdg-toplevel `dev.arcade.Lens` (not layer-shell), so keys can be sent to its exact address and pointer-leave is impossible (it covers the whole output). Capture uses the screenshot portal; no portal prompt appeared. Test subject: a floated kitty (`arcade-audit-ocr`) with large text.

### RD-LENS-1 — broken — Ctrl+C does nothing in the capture overlay, annotator and result panel (documented as "copy")
Steps: `arcade-lens --capture`, drag a region over the test text, press Ctrl+C (sent to the overlay's address with `hl.dsp.send_shortcut({mods='CTRL',key='c'})`; same keystroke path that Ctrl+Enter etc. use fine elsewhere). Also Annotate → Ctrl+C (footer of the annotator shows "Ctrl+C Copy").
Expected: README/shortcuts say "Ctrl+C copies it as an image". Actual: nothing happens, no toast, overlay stays open, clipboard untouched (`wl-paste` → "Nothing is copied", checked after `wl-copy --clear`). Using the visible toolbar "Copy" button in the annotator, or Tab → filter "copy" → Region "Copy" → Enter, does copy a correct 860×260 `image/png` (verified, `file` reports PNG 860×260), and it survives the capture process exiting.
Cause (confirmed in dependency source): egui-winit 0.33.3 `~/.cargo/registry/src/*/egui-winit-0.33.3/src/lib.rs:820-822` turns Ctrl+C into `egui::Event::Copy` and returns without a `Key` event; Lens only looks for `Key::C` with ctrl/command (`gui/overlay.rs:980` primary panel, `:1017` result panel, `gui/annotate.rs:348` `consume_key(cmd, Key::C)`) and has no `Event::Copy` handler anywhere. Needs: handle `egui::Event::Copy` in those three places. (The isolated Xvfb run in lens.md reported Ctrl+C closing the overlay; the real Wayland path does not.)
Evidence: `real/lens/ln-ctrlc.png`, `ln-annotate.png`, `ln-filter-copy2.png`.

### RD-LENS-2 — UX — Escape in the actions list with a typed filter leaves the list instead of clearing the filter
Steps: select a region, Tab, type `image` (list narrows to "Open in Image Editor"/"Web Image"), Escape.
Expected (owner rule): first Escape clears the typed text. Actual: back to the primary bar in one press, filter gone. The footer says "Esc Back", which is true but skips a level. Evidence: `real/lens/ln-filter-image.png`, `ln-esc1.png`. Source: `gui/overlay.rs:932-938` (`_ => self.panel = Panel::Primary` for every panel including `Expanded{filter}`).

### RD-LENS-3 — UX — Escape on a pin with its context menu open closes the whole pin
Steps: capture → Tab → filter "pin" → Enter (a 520×200 floating "Pin — Arcade Lens" window appears), right-click it (menu opens), press Escape once.
Expected: Escape closes the menu only. Actual: the pin window disappears in one press (menu and pin). Evidence: `real/lens/ln-pin-menu.png`, `ln-pin-esc1.png`. Source: `gui/pins.rs` root Escape handler runs although the egui popup is open.

### RD-LENS-4 — polish — The pin context menu is clipped by the small pin window
Steps: same pin (520×200), right-click. Actual: menu shows Copy, Save…, Annotate, Opacity ▸, Actual size and the rest is cut off at the window's bottom edge; "Close" and "Close all pins" (`gui/pins.rs:223-227`) are unreachable from a small pin (the hover × button still closes). Evidence: `real/lens/ln-pin-menu.png`. Fix: open the menu in its own surface or keep a minimum pin height.

### RD-LENS-5 — UX — Programmatic focus change closes the capture overlay and drops the selection (LN-4 confirmed on Hyprland)
Steps: `--capture`, drag a 500×180 region, then `hyprctl dispatch hl.dsp.focus({window='address:<disposable terminal>'})` (a focus change, no pointer). Within 1.2 s `--status` → `overlay:false`; selection lost. Expected: a transient focus change must not dismiss. Pointer-leave itself is not reproducible (the overlay is fullscreen). Source: `gui/mod.rs:650-677` `close_overlay_on_focus_loss`.

### RD-LENS-6 — wrong result — OCR of three plain lines is classified as a "Table"
Steps: region over three lines of monospace text (`ARCADE LENS OCR 4821` / `Region capture test line` / `#FF8800 colour`). Actual: the primary bar leads with a "Table" row ("ARCADE   LENS OCR 4821 …", default action "Copy Table"), and the expanded list pre-highlights "Copy as CSV" in the unfiltered view but "Copy Table" once filtered. Plain "Copy Text" is only in the second section. Expected: the common case of text lines defaults to Copy Text; table detection should need column structure (the wide gaps here are monospace spacing). Evidence: `real/lens/ln-selected.png`, `ln-expanded2.png`. Source: `lens-actions` table recogniser (not read in depth).

### LN-3 confirmed on the real desktop (zombies)
Owner's resident Lens (pid 27008) holds a defunct child `[arcade-lens]` 27116 (3 h old). Every capture/settings child leaves another one: after each action that closed a capture overlay, an Annotate/Pin, or Settings, `ps --ppid 27008` showed a new `Z<` entry (217008, 219430, 219895, 220142, 223656). `arcade-lens --status` reaps only the capture slot (27116 and the capture zombies vanished after it), but the Settings child 223656 stayed `Z<` through two `--status` calls and is reaped only when Settings is opened again, so a zombie can live for hours exactly as the owner's 27116 did. Source as in lens.md (`gui/wayland.rs:377-405`).

### LN-6 / LN-7 downgraded for Hyprland
Lens on Wayland has no window list (`lens-platform/src/lib.rs:160-166` returns empty unless X11), so window selection, "Record" and Live pins do not exist here: clicking a window in the overlay selects nothing, filter "record" shows "No actions match", the pin menu has no Live (by design, `docs/user/faq.md:21`). They are X11-only findings. Enhancement: Hyprland exposes `hyprctl clients -j` and Lens already shells out to hyprctl (`lens-platform/src/hyprland.rs`), so a window list is possible; right now the main desktop gets neither window capture nor recording. Pin placement is also compositor-chosen (centre) on Wayland; the pin did not appear over its source region.

### LN-2 not reproduced on Wayland
Copy Text (key `T` on OCR result) puts the exact recognised text on the clipboard (`wl-paste` → `ARCADE LENS OCR 4821\nRegion capture test line\n#FF8800 colour`, UTF8_STRING/text/plain). Region image copy via the Copy action gives a valid PNG. Colour: Copy HEX placed `#0E1116` (pixel under the loupe; grim says `0E1216`, one level apart because the terminal background is translucent over a live blur), Capture text from the tray also copied text and closed.

Verified working (Lens):
- `--capture` / tray Capture / Pick color / Capture text all open the overlay; drag-select shows size badge `860 × 260`, primary action bar, "Enter Run / Tab More actions / Esc Close" footer that matches behaviour; Esc from the expanded list goes back one level, a second Esc closes (the overlay then reports `overlay:false` and the capture child exits).
- Colour mode loupe with HEX/RGB; Copy HEX works; Annotate window opens at the region (1040×360 floating), toolbar Copy writes `image/png`; Done closes; Settings opens (760×560 floating) and every page (General, Shortcuts, Actions, Text recognition, Chains, Services, Plugins, Privacy, Connected apps, About) renders; Escape closes it from any page. Shortcuts page shows Ctrl Alt Shift L and the Hyprland note; `hyprctl binds` has a matching modmask 13 L bind. History is off, 0 items.
- Pointer motion over/out of the fullscreen overlay never changes anything (nothing to leave).

## Shelf 0.4.0

The owner's Shelf is docked on the right edge: a floating, pinned (always-on-top) 30×92 tab at (2520,475), a normal Wayland toplevel (not layer-shell). Its address changes when it is re-created (0x…126cc0 → …f3140 → …17b5a0 during this pass).

### RD-SHELF-1 — UX — Hover-opened docked panel collapses when the pointer leaves (SH-8 confirmed on Hyprland)
Steps: real virtual-pointer motion onto the tab (to 2530,520): panel grows to 640×490 at (1910,475) and takes keyboard focus. Then warp the pointer into the disposable terminal and do one real motion (`pointer move 5 0`). Within 1.5 s the panel is back to the 30×92 tab. Repeated: identical.
Expected (owner rule): a popup never closes only because the pointer left. A deliberate `arcade-shelf --show` (and the global shortcut `hl.dsp.global('dev.arcade.Shelf:arcade-shelf-show')`) did stay open at 640×490 across the same pointer-leave and a focus change, so only the hover-peek path collapses. If hover-peek is meant to be a peek, a click or keystroke inside should promote it to "explicitly opened"; otherwise it contradicts the rule and the Settings text. Evidence: `real/shelf/peek-open.png`. Source: `ui/Main.qml:198-201` (`peekDismissTimer`), `src/Controller.cpp:508-514`.

### RD-SHELF-2 — polish — Settings say hover does not take keyboard focus; on Hyprland it does
Steps: Settings → General "Open docked Shelf on hover — Hovering the tab shows the panel without taking keyboard focus. Click it to type." On the real desktop (`follow_mouse=1`) the hovered panel becomes the focused client (`hyprctl clients` focusHistoryID 0) and an Escape sent to it collapses it, i.e. keys are accepted without a click. Not harmful but the text is wrong for focus-follows-mouse setups. Evidence: `real/shelf/s-general.png`, `peek-open.png`.

### RD-SHELF-3 — wrong result — "Backdrop blur" is ON while saying the desktop doesn't offer one (SH-4 confirmed on Hyprland)
Steps: Settings → General on the owner's desktop. Actual: switch ON; text "Your desktop doesn't offer one here, so Shelf stays opaque." Hyprland has `decoration:blur:enabled = true` (`hyprctl getoption`), so the message is wrong for this compositor, and the switch should not show ON when unsupported. Evidence: `real/shelf/s-general.png`. Source: Shelf blur capability probe (Settings page General).

### RD-SHELF-4 — polish — `--status` reports mode "background" while the panel is open
Steps: `arcade-shelf --show` (panel 640×490 visible and focused), then `--status`. Actual: `"mode":"background"` both when only the tab is visible and with the panel open. The mode field cannot tell scripts whether the panel is showing. (isolated SH-7 saw "foreground" after restart, so the value is not consistent.)

Verified working (Shelf):
- Hover-peek (docked tab → panel) opens quickly; panel layout (Quick Shelf header, filter, Undo/Actions, footer hints "Open / Space Preview in Look / Del Remove / Esc Close") renders cleanly.
- Global shortcut: Settings → Shortcuts shows "Ready · Ctrl+Alt+S"; `hyprctl globalshortcuts` lists `dev.arcade.Shelf:arcade-shelf-show`; dispatching it opens the panel (Activated only; toggling-off not judged); `hyprctl binds` has the matching modmask 12 S bind. Portal registration works on the real compositor (the isolated "No global shortcut" caption does not occur).
- Always on top: the tab and panel are `floating` + `pinned` above T3 and the floating terminal.
- Fullscreen guard: with my disposable terminal set fullscreen by exact address (`fs=2`), the Shelf client disappeared (no mapped Shelf window); after unsetting fullscreen the tab returned at (2520,475).
- `--add <file>` adds a text item; Space on the selected item opens the file in Arcade Look (floating 1040×720) with its name; Esc on Look returns; Delete removes the item (`items` back to 0). Settings (Shortcuts, Storage, Backup and restore, Connected apps, About screenshots in `real/shelf/s-*.png`) open from the tray and close with Escape.
- Escape on the focused panel collapses it to the tab; keys sent by address to an unfocused Shelf are ignored (Qt needs focus), so that is not a bug.
- Idle: 0 CPU ticks in 10 s, 8 threads.

Not tested (Shelf): dragging a real file from Nautilus into/out of the panel (the virtual-pointer drag cannot start Wayland DnD — see the Find note), docking to the other three edges, tray "New shelf…", backdrop blur rendering itself.

## Wheel 0.5.0

Owner config: trigger is bare `Tab` held (Hyprland global shortcut `dev.arcade.Wheel:arcade-wheel-trigger`, `hyprctl binds`: modmask 0 key Tab), hold threshold 0 ms, one deck "Applications" with six real application actions (Zen Browser, kitty, Files, T3 Code Nightly, ANM Player, Calculator). The overlay is a layer-shell surface (`hyprctl layers`: level 3 `arcade-wheel`, 0,0 2560×1440) with `KeyboardInteractivityNone` (`src/platform/linux/LinuxBackend.cpp:978`), so keyboard focus stays on whatever window had it (T3 here) while the wheel is up; I therefore sent no keys while it was visible.

### RD-WHEEL-1 — wrong result — Qt writes a second cache tree `~/.cache/Arcade Wheel/Arcade Wheel/` (W1 root cause)
Evidence: `~/.cache/Arcade Wheel/Arcade Wheel/qtpipelinecache-x86_64-little_endian-lp64/qqpc_opengl` (98 KB, rewritten 2026-11-06 16:31) next to the proper `~/.cache/arcade/wheel/{logs,qmlcache,qtpipelinecache-…}` (last written Nov 3). Cause: `src/main.cpp:120-122` sets `QCoreApplication::setOrganizationName("Arcade Wheel")` and `setApplicationName("Arcade Wheel")`; Qt's scenegraph pipeline cache (and anything else using `QStandardPaths::CacheLocation`) therefore resolves to `~/.cache/<org>/<app>` = the doubled "Arcade Wheel/Arcade Wheel". `WheelPaths::cacheDir()` (`src/core/WheelPaths.cpp:16`) is correct but not used for Qt's own caches. Fix: set the organisation name empty/"arcade" and application name "wheel" (or redirect `QSG_RHI_PIPELINE_CACHE`/`XDG_CACHE_HOME` to `~/.cache/arcade/wheel`), and delete the old tree on migration. The older `~/.cache/arcade/wheel/qtpipelinecache…` copy shows the earlier build wrote there, so the 0.5.0 org/app names moved it.

### RD-WHEEL-2 — UX — Escape does nothing at the top of Settings (WH-1 confirmed on the real desktop)
Steps: tray → Arcade Wheel → Settings (opens tiled 1228×1398 on "Your wheel"); send Escape to its exact address. Actual: window stays. Ctrl+W closes it (`qml/settings/SettingsWindow.qml:59`, `StandardKey.Close`). Expected: Escape closes the window at the top view (the Find, Lens and Shelf settings windows tested in this pass all close on Escape).

### RD-WHEEL-3 — UX — `--press` then moving 150 px then `--release` executes the slice immediately; no harmless way to test "release in the middle"
Not a defect in itself (it is the product): `--press` shows the wheel centred and warps the real pointer to the screen centre (1280,720; `LinuxBackend.cpp:1174-1196`, from (400,1200)). A real pointer motion `pointer move 0 -150` selected slice 0 (`selectedIndex:0`) and `--release` launched Zen Browser within a second. My test thus started a Zen process by mistake (pid 225626, started fresh by the Wheel); I closed that process by PID. The pointer is not returned to where it was after the wheel closes (after right-click cancel and after release the cursor remained at the centre). For a user who holds Tab while working with the mouse elsewhere this means every use jumps the cursor to the middle of the output; check whether that is wanted (spec decision), otherwise restore the cursor on dismiss.

Verified working (Wheel):
- `--preview` and `--show` draw the full-screen layer with the owner's six-slice wheel (icons, "Applications" centre label); the preview/show stays up after a real pointer motion out of the area and over T3 (no pointer-leave dismissal); right-click at the centre cancels it (layer disappears from `hyprctl layers`, status `overlayVisible:false`). `--hide` removes the layer.
- Slice dispatch on the real desktop: pointer move up from centre selects slice 0, release launches the configured application (Zen Browser appeared as a new tiled window).
- Settings page set: General, Shortcuts ("Ready · hold Tab to open", hold threshold 0 ms), Your wheel (live preview, six actions listed clockwise), Actions, Center gestures, Appearance, Behaviour, Installed applications, Connected apps, About all reachable; Ctrl+W closes; resident keeps running afterwards (`--status` running, tray hosted). Screenshots `real/wheel/s-*.png`.
- `--status` JSON is valid and consistent (trigger text, platform wayland, tray hosted).

Not tested (Wheel): the genuine held-key path (global shortcut Activated+Deactivated cannot be generated by `hl.dsp.global`, and I would not press physical keys); wheel on multiple monitors (single monitor); editing/importing a separate profile (not done: the owner's live config was not modified).

## Tools 0.4.6

Tools (the tray host for all apps) was restarted once with its own `--restart` (see T1); the successor runs in the same systemd unit (`app-arcade\x2dtools@autostart.service`) as the old one and as Box/Lens, so Box and Lens were not killed, and every app (Find, Shelf, Wheel) reported `tray:"hosted"` again after the restart. Tools' window class is `arcade-tools`; its overlay is a layer-shell surface (`arcade-tools-overlay`, exclusive keyboard).

### RD-TOOLS-1 — wrong result — Shortcuts → Conflicts shows 6 false conflicts: every Arcade shortcut "conflicts" with its own Hyprland binding
Steps: Tools → Shortcuts → Conflicts (badge "6"). Actual: "Two bindings on the same keys: only one of them works." lists Show or hide the Arcade Island ⟷ Arcade Box, Open the clipboard picker ⟷ "Arcade Clipboard ACBMESH_SIGNAL_24757_517403", Show or hide Find ⟷ "Lua function (added at runtime)", Start a screen capture ⟷ "Lua function (added at runtime)", Show or toggle Arcade Shelf ⟷ "arcadeShelf_2215da1615a84c6284a6f02f51b3322e", Open the wheel… ⟷ "Arcade Wheel trigger". Each pair is the app's own registered bind (same chord on both sides). Nothing actually clashes (`hyprctl binds` has exactly one bind per chord). Also leaks internal bind descriptions (signal id, hash) as labels.
Expected: an Arcade shortcut matched by the Hyprland bind that the same app created is not a conflict; the badge should be 0 unless two different owners share a chord. Evidence: `real/tools/shortcuts-conflicts.png`, `t-Shortcuts.png`. Source: `crates/arcade-shortcuts/src/index.rs:723-742` (Arcade key vs every `wm.groups` bind via `keys::conflicts`, no check that the WM bind belongs to / was registered by that app), labels from the compositor description.

### RD-TOOLS-2 — broken — Engines page: every installed engine "Check failed … version check timed out" (0 of 11 found)
Steps: Tools → Engines (also after "Check again"). Actual: Tesseract, ripgrep, FFmpeg, Poppler, qpdf, eSpeak NG, libvips, ImageMagick, Ghostscript… all red "Check failed — /usr/bin/<x> version check timed out", Tesseract even offers "Download with Arcade"; after a second "Check again" (spinner ≈ 20 s, i.e. 11 serial 2 s timeouts) 1 of 11 was found (the last row, v15.2.0), the rest still failed. From a terminal the same commands answer in 1–8 ms (`tesseract --version` 8 ms, `rg --version` 1 ms), and Lens/Box/Find use these binaries successfully. Expected: versions listed, nothing "failed".
Evidence: `real/tools/t-Engines.png`, `engines-recheck.png`, `engines-final.png`, `engines-first.png`. Suspected source: `src/engines.rs:212-240` `version_output` (`child.wait_timeout(VERSION_TIMEOUT=2 s)` inside the Tauri/GTK process; a SIGCHLD/`waitpid` race with GLib's child reaping would make `wait_timeout` never observe the exit), engine probing `src/engines.rs:110-130`. Not reproduced under the isolated Xvfb run (Tools 0.4.6 there never opened this page). Related: Box's Engines page misses tools (BR-8) but that is a different list.

### RD-TOOLS-3 — wrong result — App details shows "Latest: Not checked" and an enabled green "Update" while Home says "Updates checked 1 min ago"
Steps: Apps → Arcade Clipboard → Details. Actual: Installed v0.3.2, Latest "Not checked", primary "Update" button; Updates tab says "You're up to date · Last checked 5 min ago" and Home "Updates checked 1 min ago". Expected: the detail page shows the last checked latest version and no Update button when already current. Evidence: `real/tools/detail-after-esc.png` (top part), `t-Updates.png`. Update not clicked.

### RD-TOOLS-4 — UX — Escape does not go back from the app details page (TL-3 confirmed)
Steps: Apps → Details on a card → Escape (sent to the window): stays on "← Back to apps". Back works by click. At the top-level tabs Escape (correctly) leaves the main window open. Evidence: `real/tools/detail.png`, `detail-after-esc.png`. Source: `src-ui/components/AppDetail.svelte` (no Escape handler), `src-ui/App.svelte`.

### RD-TOOLS-5 — UX — Shortcut overlay Escape closes everything even with a filter typed (TL-6 confirmed on Wayland)
Steps: `Arcade-Tools.AppImage --show-shortcuts` (this Wayland session opens a 2560×1440 layer-shell surface with a 1066 px panel, `KeyboardInteractivity::Exclusive`), type `clip` (list narrows to Arcade Clipboard / 37 WM entries), press Escape once. Actual: overlay disappears (process exits) and the query is lost. Footer says "Esc Close". Expected (owner rule): first Escape clears the typed text. Source: `crates/arcade-tools-overlay/src/ui/mod.rs:60`. Evidence: `real/tools/overlay-wide.png`.

### TL-2 / TL-5 / TL-1 re-assessed on the real Wayland desktop
- TL-2 (focus loss closes the overlay): not reproduced. With the exclusive layer, real pointer motion over and out of the panel, over my disposable terminal, and an explicit `hl.dsp.focus` of another window left the overlay open. A real click on another window's area (outside the panel) did close it, which is the allowed "explicit outside click".
- TL-5 (typing crashes in libxkbcommon): not reproduced on Wayland; typed `clip` without any crash (no new coredump). It is an X11/bundled-xkb problem only.
- TL-1 (CLI `--show-shortcuts` SIGBUS): not reproduced here; the CLI exited 0 and the overlay (running from the CLI's own `.mount_…`) stayed up for >10 s, and `coredumpctl` shows no new core. Toggling with a second `--show-shortcuts` closed it. Risk remains if the CLI AppImage's mount goes away while the helper is alive.

### RD-TOOLS-6 — polish — Window class/app_id is `arcade-tools`, every other Arcade app is `dev.arcade.<App>`
`hyprctl clients`: Tools = `arcade-tools`; Box/Look/Find/Lens/Shelf/Wheel/Clipboard = `dev.arcade.<Name>`. STANDARD §10 says the OS identifier is `dev.arcade.*` (Wheel's main.cpp comment cites it). Window rules, icon matching and `hyprctl` scripts for the fleet break for Tools. Evidence: `hyprctl clients` output in this session.

### T4 / TL-4 confirmed: `--status` opens the GUI, prints nothing
`Arcade-Tools.AppImage --status` (real install): exit 0, empty stdout, the Tools window (71,21 1228×1398) opened/focused.

### T1 (idle CPU) — NOT reproduced; likely cause identified
- Original resident Tools (pid 1355, 5 h uptime): 60 s sample → 0.02 % CPU (1 tick), 67 threads (26 tokio workers, 8 tools-tray, 8 arcade-link-con, 6 arcade-tools, 4 notify-rs), 91 fds, 102 MB RSS. The 8.9 % seen earlier is gone.
- After `--restart` (pid 228175): first 60 s after start 1.22 % (startup scan/catalog work), next 60 s 0.00 %, threads 44 (12 tokio workers), 61 fds, 66 MB.
- With the window open on Home, idle: 6.1 % CPU for 30 s (183 ticks; three tokio workers busy ~60 % each), threads 44→78, fds 95, RSS 214 MB. Source: `src-ui/tabs/Home.svelte:68-73` samples `api.dashboard()` every 2 s "only while Home is visible"; each sample opens a Link connection to all seven apps (`src/dashboard.rs:70`, 200 ms connect timeout each) and refreshes sysinfo. After closing the window: 0.00 % for 60 s again, 65 threads, RSS 187 MB (not released).
- Hypothesis for the owner's 9 %: a Tools window left open/hidden (minimised or on another workspace) on the Home tab keeps that 2 s poll running because `active` is the tab state, not window visibility. I did not minimise/move the window to confirm. Fix: sample only while the window is visible/focused, reuse the Link connection, and back off when nothing changed.
- No growth ratchet observed: thread count rises with window open (44→78→65) but does not climb across the idle/closed periods measured.

Verified working (Tools):
- Tray (hosted by Tools via one StatusNotifierItem): per-app submenus for Box (Open, Settings, Open pipelines, Restart, Quit), Clipboard (Open, Settings, Private mode, Restart, Quit), Find (Open, Settings, Rescan index, Restart, Quit), Lens (Open, Settings, Capture, Capture text, Pick color, Restart, Quit), Look (Open, Settings, Restart, Quit), Shelf (Open, Settings, New shelf…, Show shelf, Restart, Quit), Wheel (Open, Settings, Open wheel, Restart, Quit), plus Start all / Quit all / Quit Arcade Tools. Clicked and verified: Find Open / Settings / Rescan, Look Settings, Clipboard Settings, Box Settings (opens the dashboard on Settings), Lens Settings / Capture text / Pick color, Shelf Settings, Wheel Settings, Tools Open. Not clicked: every Restart/Quit, Start all, Quit all, Private mode toggle.
- Tools window pages Home (7 installed, 7 running, 847 MB), Apps (Start-at-login toggles per app), Updates, Shortcuts (Arcade/Hyprland/Apps tabs), Learn, Connections, Engines, Settings all render; the shortcuts overlay (Super+/ bind exists: `hyprctl binds` modmask 64 key slash) opens as a layer surface and its filter works.
- Escape closes the settings windows of Look and (earlier) Find/Lens/Shelf; it does not close Wheel's, Clipboard's or the Box dashboard (top view). Clipboard `Esc` handling for its picker is covered in RD-CLIPBOARD-1.

## Clipboard 0.3.2 (continuation: C1 and CB-4)

### C1 not reproduced — a normal `--quit` does not core dump
Steps: record `coredumpctl`, run `Arcade-Clipboard.AppImage --quit` on the real install (resident pid 24757, scope `arcade-clipboard-175236.scope`), wait 4 s. Actual: process exits, `coredumpctl list --since -1min` empty; diagnostics log: `[lifecycle] quitting`. The earlier SIGBUS was the AppImage mount going away under a live process (scope teardown), not the quit path. Relaunched per rule 5.

### RD-CLIPBOARD-3 — broken — `--restart` leaves a live but unreachable successor running from an unmounted AppImage (CB-4 confirmed on the real install)
Steps: `Arcade-Clipboard.AppImage --restart` on the freshly relaunched resident; wait 10–20 s. Actual: a process `/tmp/.mount_ArcadeolgIJc/usr/lib/arcade-clipboard/clipboard --background` (pid 239853) is alive, owns the D-Bus name `dev.arcade.Clipboard`, but `/tmp/.mount_ArcadeolgIJc` no longer exists (`mount | grep` empty, `ls` ENOENT), and `Arcade-Clipboard.AppImage --status` reports `{"running":false,"runningVersion":null,"link":…}` with no Link endpoint; no coredump. The app is effectively gone (no tray menu, no picker). Expected: the successor must start from the AppImage file itself, or the old process must stay until the new one is up.
Source: `apps/flutter_app/lib/platform/desktop_adapter.dart:~195-205` `restart()` uses `Platform.resolvedExecutable` (inside the old mount) with `ARCADE_CLIPBOARD_RESTART_AFTER=<pid>`, then `_quit()`; when the old AppImage runtime exits, FUSE unmounts and the successor loses its files. Fix: relaunch via `$APPIMAGE` (env) when set, as other apps do.
Recovery used: killed pid 239853 and relaunched with the rule-5 `systemd-run --user --scope` command; `--status` then `running:true`.

### Private mode is ON on the owner's Clipboard (observed, not changed by this pass)
The tray item ("Private mode", toggle-state 1), the Settings → Privacy switch and the sidebar footer ("Private mode", amber dot) all show it ON. `~/.local/share/arcade/clipboard/logs/diagnostics.log` has `paused=false` capture events up to 19:13:31 and none afterwards: nothing I copied during this pass (Lens text/image, Find path/file, colour) was captured. Either the owner set it or an earlier audit pass left it on (Box's "Clipboard history" connected-app experiments in `box-real.md` toggled the peer); I did not touch it. The owner should confirm and turn it off if unintended.

## Box (continuation)

### BR-9 re-test (Window Pin): still broken
Steps: disposable floating terminal focused by pointer; `arcade-desktop --show`; type "window pin", Enter, Ctrl+Enter ("Ready to pin the active window"), click "Keep active window on top". Actual: red card "Couldn't finish — No other window is active" together with the green "Done · 1 result"; the terminal's `pinned` stays `false` (`hyprctl clients`), the Island stays open and pinned itself. Evidence: `real/box/wp-search.png`, `wp-tool.png`, `wp-run.png`, `wp-after.png`. Same cause as BR-9 (`hyprland.rs` `foreign_active_window` right after the Island hides). Recent page was already confirmed in round 3 (`real/box/page-recent.png`).

## Fleet-wide Escape / pointer-leave summary (real desktop, this pass)

| App / surface | Pointer leave closes it? | Focus change closes it? | Escape |
|---|---|---|---|
| Find overlay (layer-shell, exclusive kb) | no | not by pointer/click (keeps exclusive focus) | text typed → closes and discards (RD-FIND-1) |
| Lens capture overlay (fullscreen toplevel) | n/a (covers output) | yes (RD-LENS-5) | Esc steps back once; with filter text goes back without clearing (RD-LENS-2) |
| Lens pin menu | no | – | closes whole pin (RD-LENS-3) |
| Shelf docked panel, hover-opened | yes (RD-SHELF-1) | – | closes when focused |
| Shelf panel, explicit `--show` / global shortcut | no | no | closes when focused |
| Wheel preview/show (layer-shell, no kb) | no | n/a | n/a (no keyboard); right-click cancels |
| Tools shortcuts overlay (layer-shell, exclusive kb) | no | no (explicit outside click closes) | text typed → closes (RD-TOOLS-5) |
| Look preview (earlier) | no | no | one step back (RD-LOOK-1 trap) |
| Clipboard picker (earlier) | yes (RD-CLIPBOARD-1) | yes | – |
| Settings windows | – | – | close on Esc: Find, Lens, Shelf, Look; do NOT: Wheel (RD-WHEEL-2), Clipboard, Box dashboard (top view), Tools main window |

## Final state

- Clipboard (Wayland) restored to empty (`wl-copy --clear`; originally "Nothing is copied").
- Disposable windows closed/killed: audit terminals (kitty), OCR kitty, GTK DnD target/source, Zen Browser instance accidentally started by Wheel (killed by PID). Screenshots under `real/<app>/`; helper scripts and logs in the session scratchpad `rd4/`.
- Settings restored: Find `theme` back to `system` (file identical to `settings-find-before.json`). Nothing else changed: Box Start at login untouched (off), Lens history off, no installs/uninstalls/commits, no Hyprland config/bind change.
- Running state: Tools restarted (new pid 228175, tray host, window closed), Clipboard restarted twice (current pids 240250/240257, relaunched in a `systemd-run --user --scope` unit, `running:true`), Find, Look, Lens, Box (background), Shelf (docked tab at (2520,475)), Wheel all still running, as at the start. Lens still has one defunct Settings child (223656) which only goes away when Settings is reopened (LN-3).
- One heads-up for the owner: Clipboard "Private mode" is ON (see above) and was not set by this pass.
