# Arcade standard (UI + project)

The one standard every Arcade app follows, existing and new. Each app's spec in this folder says what has to
change in that app to meet it. Arcade Tools 0.3.8 is the visual reference: where this document and Tools
agree, Tools is already correct; where they differ, this document wins and Tools changes too.

Agreed with the owner on 2026-10-27:
- Every app keeps its own toolkit; the design is shared, and implemented once per toolkit.
- Shared neutrals and status colours everywhere; each app's own accent colours its primary actions,
  selection and focus.
- Two templates: the **window shell** for main/settings/dashboard windows, the **overlay** for quick panels.
- Themes follow the system, with a Light / Dark / System choice in each app.
- Inter is bundled in every app.
- The design kit lives in Arcade Link (`design/`), next to `tokens.json` and `NEW_APP_SPEC.md`.

**Keep what's useful.** The standard changes how *shared* things look and behave (colours, type, controls,
settings layout, wording, project setup). It never removes or flattens a useful app-specific feature,
behaviour or surface. Examples: Look's frameless title bar, Shelf's dock modes and backdrop, Wheel's ring
geometry settings, Lens's annotate/pins/measure, Find's opaque fallback, Box's pipelines and plugins.
Each app spec has a **Keep** list. When the standard and a Keep item collide, the Keep item stays and gets
kit styling; the spec records it as a documented exception. Nothing is removed only to make apps look
alike.

---

## 1. Design kit (single source of truth)

`Arcade-Link/design/tokens.json` holds every value in this document. A generator in Link
(`design/generate.py`, stdlib only) writes one file per toolkit; the generated files are committed and a
Link CI check fails when they're out of date.

| Toolkit | Apps | Generated file(s) | Delivered how |
|---|---|---|---|
| CSS (Svelte / plain TS) | Tools, Box, Look | `design/css/arcade.css` (custom properties + base + components) | apps copy it via a `scripts/sync-design.*` step pinned to the Link tag, with a `VENDORED` hash check |
| QML | Shelf, Wheel | `design/qml/Arcade/` module: `Theme.qml` singleton + components | vendored next to the existing vendored Link Qt module, same `VENDORED.json` pin |
| Flutter | Clipboard | `design/dart/arcade_theme.dart` (`ThemeData` light + dark, `ArcadeColors` extension) | vendored file, hash-pinned |
| Rust | Find, Lens (and Rust parts of others) | `arcade-link` crate feature `design`: `arcade_link::design` (colour/size constants, `egui::Visuals` + `Style` builders behind an `egui` feature, plain RGBA for softbuffer/tiny-skia) | the existing git-tag dependency |

Also in `design/`:
- `fonts/` with Inter (variable, `.ttf` + `.woff2`, OFL licence file), loaded by every toolkit;
- `icons/`, the Arcade outline icon set (24×24, 1.5 stroke, `currentColor`), exported as SVG and
  generated as a Svelte/TS path map, a QML `Icon` component, a Dart `IconData`-free painter map and a Rust
  path table, so each toolkit can draw the same glyphs. Tools' current icon set (`src-ui/components/Icon.svelte`)
  is the seed; new glyphs follow the same grid;
- `glyphs/arcade.<app>.svg`, the app marks (already used by Tools);
- `STANDARD.md`, a copy of this document, kept identical.

Integration surfaces that already use `assets/tokens.json` (badges, Look actions strip, Connected apps pages)
move to the new tokens; `assets/tokens.json` stays as a generated, backwards-compatible subset.

## 2. Colour

### Neutrals (identical in every app)

| Token | Light | Dark | Use |
|---|---|---|---|
| `bg` | `#f6f7f9` | `#111317` | window background, sidebar |
| `panel` | `#ffffff` | `#16181d` | cards, lists, dialogs, overlays |
| `sunken` | `#f1f3f5` | `#1b1e24` | inputs inside panels, code, tiles, segmented track |
| `hover` | `#eceef1` | `#22262d` | hover fill |
| `line` | `#e3e6ea` | `#262a31` | borders, dividers |
| `line-strong` | `#d3d7dd` | `#323741` | control borders, scrollbar thumb |
| `text` | `#16191f` | `#e6e8ec` | primary text |
| `muted` | `#5f6672` | `#9097a2` | secondary text |
| `faint` | `#8b929c` | `#656b76` | placeholders, meta, disabled labels |
| `warn` / `warn-soft` | `#a8700a` / `#fbf1dc` | `#e8b04b` / `#2d2617` | warnings |
| `danger` / `danger-soft` | `#c43d3d` / `#fbe9e9` | `#f07a7a` / `#2f1c1e` | errors, destructive |
| `ok` / `ok-soft` | `#168a4c` / `#e5f4ec` | `#5fd08f` / `#1b2d23` | success, "running", "up to date" (always green, in every app) |
| `scrim` | `#0a0d1266` | `#0a0d1299` | dialog backdrop |
| `shadow` | `0 1px 2px #10141a0d` | none | resting cards |
| `shadow-pop` | `0 12px 40px #10141a24` | `0 16px 48px #0008` | dialogs, toasts, overlays, menus |

### App accents

Each app sets `accent`, `accent-text` (text on an accent fill), `accent-soft` (tinted background) for both
themes. Light accents give white text ≥ 4.5:1; dark accents reach ≥ 7:1 on `bg` and take dark text.
The generator checks these ratios and fails if a value drifts.

| App | Light `accent` | Dark `accent` | Notes |
|---|---|---|---|
| Tools | `#168a4c` | `#5fd08f` | unchanged (green) |
| Box | `#724fff` | `#a58fff` | violet |
| Lens | `#1e7e72` | `#2bb3a3` | teal |
| Look | `#1268f4` | `#69a0f8` | blue |
| Wheel | `#9e6506` | `#f59e0b` | amber |
| Clipboard | `#d91676` | `#f176b3` | pink |
| Shelf | `#bf13d8` | `#e273f3` | **changes** from periwinkle `#94A8FF` (too close to Look's blue) to fuchsia |
| Find | `#517e0e` | `#84cc16` | **changes** from green `#22C55E` (clashed with Tools) to lime |

`accent-soft` = accent at 12 % over `panel` (light) / 16 % over `bg` (dark); `accent-text` = `#ffffff` in
light, `#0b0d10` in dark. Brand marks and glyph tiles stay neutral, as in Tools (sidebar brand mark: `text`
fill with a `bg` glyph; glyph tiles: `sunken` fill with a `text` glyph). Another app's accent appears only
in that app's Link badge, never as decoration.

Rules: no hard-coded colours in app code. Every colour comes from the kit; app-specific content colours
(syntax highlighting, chart series, annotation pens in Lens, wheel slice colours) live in a named,
documented palette inside the app, built from kit values where possible.

## 3. Type

Inter everywhere (bundled), `font-synthesis: none`, tabular numbers for counts, sizes, times and progress.
Monospace: `ui-monospace, "JetBrains Mono", "Cascadia Mono", Menlo, Consolas, monospace` (system).

| Token | Size / line | Weight | Use |
|---|---|---|---|
| `display` | 26 / 32 | 600 | onboarding hero only |
| `title` | 21 / 26, −0.015em | 600 | page title |
| `title-sm` | 16 / 22 | 600 | dialog title, overlay header |
| `heading` | 14 / 20 | 600 | section heading, card title emphasis |
| `body` | 13 / 20 | 400 / 500 | default text, row titles (500) |
| `control` | 12.5 / 16 | 500 | buttons, inputs, segmented, toasts |
| `small` | 12 / 18 | 400 | row subtitles, section notes, field labels |
| `caption` | 11.5 / 16 | 500 | badges, key chips, counts, sidebar meta |
| `mono` | 12 / 18 | 400 | paths, hashes, code |

Nothing smaller than 11.5 px. Sentence case everywhere (titles, buttons, menu items).

## 4. Space, size, shape, motion

- Spacing scale (px): 2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 32, 40, 48. Nothing off-scale.
- Radii: `xs` 5 (key chips), `sm` 7 (buttons, inputs, nav items), `md` 10 (cards, lists, notices, toasts),
  `lg` 12 (dialogs, overlays), `tile` 9 (36 px glyph tile; 13 for 52 px, 7 for 28 px), `pill` 999.
- Control heights: 28 (small), 32 (default button/input/select), 34 (nav item), 36 (search field),
  44 (overlay search field; 56 for a launcher). Row min-height 52 (list), 46 (dialog list), 40 / 52 (overlay
  result, one / two lines).
- Icons: 16 px default (1.5 stroke), 15 in small buttons, 18 in glyph tiles, 26 in large tiles.
- Motion: `fast` 120 ms (hover, press), `base` 180 ms (toasts, panels, overlays in), `slow` 260 ms (page-level),
  easing `cubic-bezier(0.2, 0, 0, 1)`. Overlays appear in ≤ `base` and close instantly. Respect
  reduced-motion (all durations → ~0).
- Window minimum size: 720 × 520 for shell windows; below 760 px wide the sidebar collapses to 64 px icons.

## 5. Components

Every toolkit implements the same set, with the same names, states and measurements as Tools' `style.css`
(the reference implementation):

| Component | Variants / parts | States that must exist |
|---|---|---|
| Button | default, `primary` (accent), `ghost`, `danger` (outline), `destructive` (filled), `sm`, `icon-only` | hover, pressed, focus ring, disabled, busy (spinner replaces icon, width kept) |
| Link button | inline accent text | hover underline, disabled |
| Input / Select / Search field | default, `sm`; search has leading icon + clear | focus (accent border + 3 px `accent-soft` ring), invalid (`danger` border + message below), disabled |
| Switch, Checkbox, Radio | switch = boolean setting; checkbox = selection in lists/dialogs | focus, disabled |
| Segmented control | filter/view switching, optional count | active, hover, focus |
| Card | glyph tile + title + subtitle + actions | hover only when the whole card is clickable |
| List + Row | title, subtitle, leading glyph, trailing control; `row-extra` for expanded content | hover when clickable |
| Glyph tile | 28 / 36 / 52 | — |
| Badge, Dot | neutral, `ok`, `warn`, `danger`, accent | — |
| Key chip | single key, combo, "or" separator | — |
| Notice | neutral, `ok`, `warn`, `danger`, with optional action | — |
| Empty state | icon, heading, text, actions (dashed border) | — |
| Skeleton | line, row | shimmer (static under reduced motion) |
| Progress | bar (determinate/indeterminate), operation bar with cancel | — |
| Toast | info/success/error, auto-dismiss 5 s (errors stay until closed), bottom-right | raised above the operation bar |
| Dialog | title, body, options, actions right-aligned (primary last) | Esc + backdrop close unless destructive/busy |
| Menu / context menu | items with icon + shortcut chip, separators, danger item | keyboard navigation |
| Tooltip | delay 500 ms, `caption` text | — |
| Tabs (sidebar nav) | icon + label + count | active (panel fill + line ring + accent icon), roving tabindex, arrow keys |
| Markdown body | release notes, guides | — |

### 5.1 Standard controls: one behaviour everywhere, not just one look

Each of these is built once per toolkit kit and is the only way an app does that job.

- **Shortcut recorder:**
  - Shows the current combo as key chips. Click or Enter starts recording ("Press keys…", accent ring); Esc
    cancels, Backspace clears where "no shortcut" is allowed.
  - Requires a modifier, except F1–F24. Keys come out in platform order and names (Ctrl/Alt/Shift/Super;
    ⌘⌥⇧⌃ on macOS).
  - Checks conflicts against Link's shortcut registry and the window manager's binds. A conflict shows a
    `warn` notice "Used by <app/action>" with "Use anyway" / "Choose another".
  - Applies as soon as a valid combo is captured. "Reset to default" sits beside it as a ghost link.
  - Unsupported-desktop (some Wayland) and Hyprland notes share one text from Link.
  - Used by every app's Shortcuts section, the onboarding steps, and Tools' rebind dialog.
- **Settings row:**
  - Label (`body` 500) and an optional one-line description (`small` muted) on the left; the control on the
    right.
  - A switch row is clickable across its whole width.
  - Rows are grouped in a list under a section heading. No per-setting cards or icon tiles; no
    "Saved"/"Theme set" confirmation notices (the control shows the state).
- **Theme picker:** always a segmented control, System / Light / Dark, first row of General.
- **Slider row:** label + description, value with its unit right-aligned, slider full width below,
  arrow-key steps.
- **Select vs segmented:** up to 4 short options → segmented; more → select.
- **Search field:**
  - Ctrl/⌘+K focuses it and filters live.
  - Esc clears first, then closes or leaves.
  - The result count is announced to screen readers, and a no-results state names the query.
- **File, folder and output pickers:** one dashed drop zone ("Drop files or choose…") plus a list of chosen
  items with remove buttons. The output folder is a row with the path (mono, middle-ellipsis) and "Change…".
- **Result panel:**
  - Actions in this fixed order: Copy, Save…, Open, Show in folder, Send to… (Link).
  - Text results are selectable; long output scrolls inside the panel, never the window.
- **Context menus:** Open first, then the item's actions, then Copy/Copy path, separator, then
  destructive items last in `danger`.
- **Confirm before destroying:**
  - A kit dialog that names the thing: "Delete collection "Work"?".
  - One sentence on what is lost and what stays; buttons "Keep" / "Delete …" (destructive).
  - Never used for reversible actions; offer Undo instead.
- **Onboarding/welcome:**
  - The same hero (52 px mark, `display` title, one sentence) and at most 3 steps.
  - Usually shortcut → connect apps → done. "Skip" is always visible, and it can be re-run from
    Settings › General.
- **Notifications (system):** only for things that finish or need the user while the app is hidden, with the
  app name as title and one sentence. Every app has one switch in General.
- **Keyboard conventions** (every app):
  - Ctrl/⌘+, settings; Ctrl/⌘+K search; Esc close/back; F1 user guide; Ctrl/⌘+Q quit (where the app has
    a window); arrows + Enter in lists.
  - Overlays list their keys in the footer hint bar.
- **Window behaviour:**
  - Settings windows are 760×560 by default, minimum 720×520, native decorations.
  - Every window remembers its size and position per monitor, and opens on the monitor with the cursor.

States are not optional: every list or page that loads data has a loading (skeleton), empty, error (notice
with Retry) and populated state.

## 6. Template A: the window shell

For every app window that isn't a transient overlay: settings, dashboards, managers, history, about.

```
┌──────────────┬────────────────────────────────────────────────┐
│ [■] App name │  Page title                     [sec] [primary]│
│              │  One-line description / status                 │
│ ⌂ Section    │                                                │
│ ▣ Section  3 │  Section heading                       meta    │
│ ...          │  ┌──────────────────────────────────────────┐  │
│              │  │ Row title                         [ctrl] │  │
│              │  │ Subtitle                                 │  │
│              │  ├──────────────────────────────────────────┤  │
│──────────────│  │ ...                                      │  │
│ footer slot  │  └──────────────────────────────────────────┘  │
│ v1.2.3    ⟳  │                                   [toast]      │
└──────────────┴────────────────────────────────────────────────┘
```

- Sidebar 212 px on `bg`, right border `line`; brand = 28 px accent mark tile + app name (14/600); nav items
  34 px; footer slot for one global switch or status, then "Version x.y.z".
- Content column max 980 px, centred, padding 32/36/72; page head with title, subtitle, actions right.
- Sections 32 px apart; section head (heading + meta) 10 px above its list or card grid.
- Card grids: `repeat(auto-fill, minmax(240px, 1fr))`, 12 px gap. Never a fixed-column grid without
  `minmax(0, …)` (the Box overflow bug).
- Text never overflows its container: long names ellipsize with a tooltip, paths wrap anywhere.
- Native title bar/decorations stay native (no custom title bars) unless the app already ships a frameless
  window that the owner relies on.

### Standard settings sections (every app, in this order)
1. **General**: Appearance (System / Light / Dark), Start at login, app-specific basics.
2. **Shortcuts**: the app's global shortcuts with rebind + conflict warning, and a read-only list of in-app
   keys (the same data Tools' shortcut sheet shows).
3. App-specific sections (named for what they do, not "Advanced").
4. **Connected apps**: the Link page (shared component, same copy everywhere).
5. **About**: version, channel, "Updates are managed by Arcade Tools" (or the app's own updater when Tools
   isn't installed), Open user guide, Open logs folder, Copy diagnostics, licences.

### Shared patterns (identical copy and layout in every app)

- **Connected apps page:**
  - Notice at the top when this app's connection is off: "Connections are off in this app", with a Turn
    on switch.
  - Then one list row per other Arcade app:
    - glyph tile;
    - name;
    - one-line description of what the two apps do together (from Link's peer descriptions);
    - status badge: `ok` Connected / neutral Not running / Not installed;
    - trailing action: "Open" / "Install" (asks Tools) / nothing.
  - A collapsed "Recent activity" list (the Link activity log) and a "How it works" link to the guide come
    last.
- **About section**, in this order:
  1. version + channel;
  2. "Updates are managed by Arcade Tools" with Open Tools (or "Check for updates" when Tools isn't
     installed);
  3. User guide;
  4. Open logs folder;
  5. Copy diagnostics;
  6. Licences.
- **Self-install prompt** (first run of a downloaded build):
  - Title "Install Arcade <App>?"; one sentence on where it goes.
  - Options "Start at login" and "Remove the downloaded file".
  - Buttons: "Don't ask again for this file" (ghost), "Just run it" (secondary), "Install" (primary). This is
    Tools' dialog, rendered by each toolkit's kit.
- **Tray menu:**
  1. the app's main actions ("Open …" first);
  2. "Settings";
  3. separator;
  4. app-level actions ("Restart …");
  5. "Quit Arcade <App>" last.

  Use sentence case and no "Open Settings"/"Preferences" variants. When Tools hosts the tray, the same
  items appear in that app's submenu. The overhaul only renames and reorders an app's existing tray items, plus
  merges duplicates. It adds none and removes none; the item names in each spec are a guide, and the app's
  real items win.

Apps with a single small settings window use the same shell; with ≤ 3 sections the sidebar may become a
segmented control under the page title.

## 7. Template B: overlays

For transient surfaces: Find launcher, Lens capture toolbar/result panel, Wheel, Box Island, Look preview,
Shelf panel, Clipboard popup.

- Surface: `panel`, 1 px `line` border, radius `lg` 12, `shadow-pop`; opaque (no translucency/blur, which
  breaks on some compositors).
- Header (optional): 44 px search field or title (`title-sm`) with the app mark at 16 px. A launcher
  (Find) may use the large field: 56 px, 18 px input text.
- Content rows 40 px (one line) or 52 px (title + subtitle): leading icon/thumbnail, title (`body` 500,
  14 in a launcher), subtitle (`small` muted), trailing key chip or meta. Selected row = `accent-soft` fill + accent leading bar 2 px; hover = `hover`.
- Footer hint bar (28 px, `caption`, `faint`): key chips for the 2–4 main actions ("↵ Open", "⌘K Actions",
  "Esc Close").
- Keyboard first: opens with focus in the field, arrows move, Enter acts, Esc closes, Tab cycles; mouse
  works the same.
- Appear within 100 ms of the shortcut (perf budget in each spec); theme follows the app setting.
- Canvas-style overlays (Lens selection, Wheel ring, Box screen ruler) keep their shape but take colour,
  type, radius and chip styles from the kit.

## 8. Copy and behaviour

- **Names**: "Arcade Box" is plain text in one weight everywhere. No split styling like "Arcade **Box**".
  Inside the app's own window, the short name ("Box") is fine in running text.
- **One tagline per app**: the README one-liner, e.g. "Press Space, see anything." It is used unchanged
  as the Tools card summary, the About section, the onboarding hero and the tray tooltip. Today there are
  up to four different ones per app: README; the catalog summary ("Run quick actions on files, media, and
  text"); in-app lines such as "Find a tool. Get it done. Get back to your day.", "Your desktop, closer."
  and "Select anything. Do anything useful with it.". The long catalog `description` stays, for Tools'
  app detail. The catalog gets a `tagline` field and Tools reads it.
- **Brand mark and app icons:** today the launcher icons are eight unrelated styles. Tools is a flat
  slate tile with sliders; Box is a dark tile with a game controller, though its catalog glyph is a cube;
  Look and Lens are purple gradients; Find, Shelf and Wheel are dark tiles in three different styles; and
  Clipboard's macOS icon is still Flutter's default logo (`AUDIT/app-icons-today.png`). One family:
  - **Launcher icon**:
    - rounded square, radius 22.5 % of the size, with the platform safe area (macOS 10 % inset);
    - background = the app's light accent with a soft vertical gradient to the base accent;
    - the app's catalog glyph centred in white at 56 % of the size, stroke scaled from the 1.5 px/24 px
      grid;
    - no other art, text or 3D.
  - The kit's `design/app-icons/make.py` (needs `rsvg-convert`) renders every size each platform needs:
    PNG 16–1024, `.ico`, `.icns`, Android adaptive foreground/background, iOS set and Tauri's `icons/`
    set, into `design/app-icons/<app>/`. Apps copy their set with `sync-design`.
  - **Tray icon**: the same glyph, monochrome (template image on macOS, white/black per panel on Linux,
    per theme on Windows).
  - **Sidebar brand tile and Tools' glyph tiles**: the same glyph, as in §2.
  - **README logo**: the launcher icon at 112 px.
- **Window titles**: "<Thing> — Arcade <App>" (e.g. "report.pdf — Arcade Look", "Settings — Arcade Box",
  "Pinned reference — Arcade Box"). A main window with nothing open is titled just "Arcade <App>".
- **Versions**: users see "Version 0.4.5"; build numbers and commits (e.g. Wheel's `+build.48`) appear only
  in diagnostics.
- **Tooltips**: the action name, plus its key as a key chip ("Open with default app  ↵"), not
  "(Enter)" in brackets.
- Plain, calm, second person; say what happens ("Removes Arcade Box and its shortcuts. Your files stay.").
- Buttons are verbs ("Install", "Open", "Check for updates"); no "OK" / "Yes"; destructive actions name the
  thing.
- Errors say what went wrong and what to do next, never a raw stack or Rust error string; details go behind
  "Copy details".
- No placeholder UI: no dead buttons, fake menus, "coming soon", "0 planned" counters or lorem ipsum.
- Every action that takes > 300 ms shows progress; > 2 s shows cancel where possible.
- Settings apply at once (no Save button). Fields that need validation apply on Enter/blur and show an
  inline error; nothing else the user changed is lost.

### 8.1 Words, formats and behaviour that must match

- **Fixed words:**
  - "Settings" (never "Preferences" or "Options");
  - "Start at login" (not "Start on login" or "Launch at login");
  - "Quit Arcade <App>" (never "Exit");
  - "Install" (never "Get");
  - "Show in folder" (not "Reveal" or "Reveal original");
  - "Copy path";
  - "Open with default app";
  - "Connected apps";
  - "Check for updates";
  - "Not running" / "Not installed" / "Connected".
- **Error wording:** errors read "Couldn't <verb> <thing>. <Reason in plain words>. <What to do>.". Use
  contractions, as Tools does; "Could not" and "Failed to" are not used. Technical detail goes behind
  "Copy details".
- **Sizes:** decimal units with one decimal place, the way GNOME, KDE and macOS show them: "940 kB",
  "12.4 MB", "3.1 GB". KiB/MiB/GiB are not shown in the UI; the apps mix them today. Size settings use MB.
- **Times and dates:**
  - Relative within a day: "Just now", "5 min ago", "2 h ago".
  - Then "Yesterday 14:05", then the locale date ("12 Oct", or "12 Oct 2025" for another year).
  - Durations read "1:05:09" for media and "3 s" / "2 min" for jobs.
  - Numbers use locale grouping.
- **Shared helpers:** the kit ships one set of formatting helpers per toolkit (`formatSize`, `formatRelative`,
  `formatDuration`, `formatCount`) in `design/`, so no app formats these by hand.
- **"Copied" feedback:** every copy action shows the same 1.5 s toast, "Copied" or "Copied 3 paths".
- **Closing windows:** closing a window never quits an app that keeps running in the background. The first
  time, a toast says "Arcade <App> is still running. Quit it from the tray." Quit is always in the tray and
  in About.
- **Start at login:** the app starts in the background (tray only), with no window.
- **Tray clicks:** a left click opens the app's main surface and a right click opens the menu. On platforms
  that only show menus, the first item is "Open …".
- **Overlays:** they close on Esc and when they lose focus. Surfaces the user has pinned or set to stay on
  top are the exception, e.g. Shelf's always-on-top panel and Lens pins, which are on the Keep lists.
- **Opening a window**: a window appears only once its first frame is ready (Tools' `window_ready`
  pattern). No blank, white or black flash, in either theme.
- **Drag and drop**: a drop target shows `accent-soft` fill with a dashed accent border and a one-line label
  ("Drop to collect", "Drop files to use them"). Files dragged out of an app use the real file and show its
  icon or thumbnail. Every app that accepts or gives files does both the same way.
- **Menus**: in-app context menus use the kit menu. Tray menus stay native.
- **Empty states**: "No <things> yet" or "Nothing matches "<query>"", then one sentence on how to get some,
  then at most two actions.
- **Text selection and cursors**: selected text uses `accent-soft`. Clickable things show the pointer
  cursor. Read-only values that people copy (paths, hashes, versions) are selectable.
- **Second launch:** launching the app again brings its main surface to the front (§10 Single instance).

## 9. Accessibility

- Visible focus ring (2 px accent, 2 px offset) on every interactive element; full keyboard operation.
- Contrast: text ≥ 4.5:1, large text and UI parts ≥ 3:1, in both themes.
- Every icon-only control has an accessible name/tooltip; hit targets ≥ 28 px.
- Respect reduced motion and system font scaling up to 125 % without clipping.

## 10. Project (dev) standard

Each repo, regardless of language, has the same shape and the same commands:

- **Top level**: `README.md` (standard README with logo + screenshot), `CHANGELOG.md` (dated, Keep a
  Changelog style), `LICENSE`, `docs/` (`STATUS.md`, `user/` guide, `dev/` notes), `scripts/`, `.github/`,
  `.editorconfig`, `.gitignore` covering every build output, `VENDORED`/`VENDORED.json` for vendored files.
- **One entry point per task**, same names in every repo, documented in README "Development":
  `scripts/dev` (run locally), `scripts/check` (format check + lint + tests + UI checks, exactly what CI runs),
  `scripts/package` (build installers), `scripts/sync-design` (refresh the design kit from the pinned Link tag).
- **Formatting and lint**, enforced in CI with zero warnings: `rustfmt` + `clippy -D warnings`;
  `prettier` + `svelte-check`/`tsc --noEmit` (web); `clang-format` + compiler `-Wall -Wextra -Werror`
  in CI (C++); `qmllint` (QML); `dart format` + `dart analyze --fatal-infos` (Flutter).
- **Toolchains pinned**: `rust-toolchain.toml`, `.nvmrc`/`engines`, Qt version in CI, Flutter version file.
- **CI**: same workflow names and job names in every repo: `ci.yml` (check on Linux/Windows/macOS),
  `release.yml` (package + publish with `arcade-release.json` + `SHA256SUMS.txt`); version read from one
  file per repo.
- **Link** pinned by tag everywhere (git dependency or vendored with hash); the design kit uses the same tag.
- **Command line**: besides Link's shared install set (`--install`, `--silent`, `--repair`, `--uninstall`,
  `--remove-data`, `--channel`, `--keep-download`, `--no-launch`, `--integration-status`, `--quit`,
  `--restart`, `--background`), every app answers `--version`, `--help`, `--settings` (open settings),
  `--status` (one-line JSON: running, version, Link state) and, where it has a main surface, `--show`,
  `--hide` and `--toggle`. App-specific flags stay. Today `--status` and `--show`/`--toggle` are missing in
  some apps, and Look's `--service` and Clipboard's `--overlay` keep working as aliases.
- **Logs and diagnostics**: logs go where Link's `logs` module puts them, with the same rotation. "Copy
  diagnostics" produces the same header in every app (app, version, channel, OS/desktop, Link protocol and
  peers, engines), then app-specific lines. No secrets.
- **Single instance**: launching a running app shows its main surface (or settings when it has none)
  instead of starting a second copy. Every app does this through Link.
- **Developer docs**: user docs already share one layout (`docs/user/` with the same 8 files), so keep it.
  Developer docs move to `docs/dev/` with lowercase names: `architecture.md`, `link.md`, `packaging.md`,
  `testing.md` and `decisions.md`, plus any app-specific topics (e.g. Box `tool-api.md`, Clipboard
  `protocol.md`/`security.md`). Finished plans, audits and research notes (e.g. Box
  `implementation-plan.md`, `recovery-audit.md`, `research/`; Look `PLAN.md`) move to `docs/dev/history/`.
  They are kept, not deleted. Links are updated.
- **User guide links**: files in `docs/user/` link only to other files in `docs/user/` or to absolute
  URLs, because Tools shows them offline. The docs check fails on any relative link that leaves the folder.
- **Workflows**: today each repo releases differently (`build.yml`, `tag.yml`, CI-triggered releases,
  auto-tagging). Target: `ci.yml` (checks on all 3 OSes, PRs and main) and `release.yml` (build, package,
  publish), started the same way everywhere, as Tools does it: manual `tag.yml` (tag + sha) → `release.yml`
  on the tag. Assets, names, `arcade-release.json` and `SHA256SUMS.txt` stay byte-for-byte compatible with
  what Tools and the catalog expect. The switch is verified with one release on the `beta` channel before
  the old workflow is removed.
- **Identifiers and folders:** today these vary per app. Config is in `~/.config/arcade-find`, `arcadelens`,
  `arcade-look` and `Arcade Wheel`. Data is in `~/.local/share/app.arcadelook`, `app.arcadetools`,
  `arcadelens`, `dev.arcadebox.app` and `dev.arcade.clipboard`. Desktop files and app ids are
  `arcade-find.desktop`, `arcade.shelf.desktop`, `com.arcadewheel.ArcadeWheel.desktop` and
  `dev.arcade.clipboard.desktop`, and Box's window class is `arcade-desktop`. Target:
  - Link id `arcade.<app>` (unchanged).
  - OS identifier `dev.arcade.<App>`, used for the bundle id, Windows AppUserModelID, the desktop file
    `dev.arcade.<App>.desktop` and the Wayland app_id / X11 WM_CLASS, e.g. `dev.arcade.Box`.
  - Folders from Link's paths module: config `~/.config/arcade/<app>/settings.json`, data
    `~/.local/share/arcade/<app>/`, cache `~/.cache/arcade/<app>/`. On Windows that is
    `%APPDATA%\Arcade\<App>` (+ `%LOCALAPPDATA%` for cache); on macOS
    `~/Library/Application Support/Arcade/<App>` (+ `~/Library/Caches/Arcade/<App>`).
  - Migration, which must not lose anything:
    1. On first start of the new version, copy the old folders to the new ones and verify.
    2. Only then remove the old ones. A failed copy leaves everything in place and logs it.
    3. Rewrite the autostart entry and the desktop entry under the new id, and remove the old ones.
  - Tools' backup, restore, uninstall and "focused app" detection (shortcut sheet) learn both the old and
    new locations and classes.
  - Check by installing the previous release, setting things up, then updating through Tools. Settings,
    history, shortcuts, autostart, tray hosting and uninstall must all still work.
- **Desktop entry contents:**
  - `Name=Arcade <App>`;
  - `Comment=` the tagline;
  - `Keywords=` 4–8 words;
  - `Categories=Utility;` + one specific category;
  - `StartupWMClass=` the OS identifier;
  - `Icon=` the app icon;
  - a "Settings" desktop action.
- **Default global shortcuts:**
  - `Ctrl+Alt+<key>`, the same chord on Linux and Windows, `⌃⌥<key>` on macOS. Shift is added only where
    the plain chord is commonly taken.
  - Documented exceptions: Wheel's hold key (F8, since hold-to-open needs one key) and Tools'
    `Super+/` (the cheat-sheet convention).
  - The table below is checked for clashes between Arcade apps in Link's CI.
  - Existing users keep the shortcut they have; new defaults only apply to new installs.

  | App | Default (Linux/Windows) | macOS | Change |
  |---|---|---|---|
  | Box | Ctrl+Alt+Space | ⌃⌥Space | macOS was ⇧⌘Space |
  | Look (preview selection) | Ctrl+Alt+Shift+Space | ⌃⌥⇧Space | — |
  | Find | Ctrl+Alt+F | ⌃⌥F | — |
  | Lens | Ctrl+Alt+Shift+L (Ctrl+Alt+L locks the screen on several desktops) | ⌃⌥⇧L | — |
  | Shelf | Ctrl+Alt+S | ⌃⌥S | — |
  | Clipboard | Ctrl+Alt+V | ⌃⌥V | Linux was Ctrl+Shift+Space, macOS ⇧⌘V |
  | Wheel | hold F8 | hold F8 | — |
  | Tools (cheat sheet) | Super+/ | ⌘/ | — |
- **Tags and branches:**
  - Release tags are `v<version>` in every repo. Wheel's `v0.4.5+build.73` style moves the build number
    into `arcade-release.json` and the release notes; Tools already requires a stable tag equal to
    `v{version}`.
  - Branches are `<type>/<topic>` (`fix/…`, `feature/…`, `release/x.y.z`, `overhaul/standard`) and are
    deleted after merging. Today every repo carries 5–12 old `arcade/*` and `claude/*` branches.
  - Cleanup deletes remote branches already merged into main. Unmerged ones are listed for the owner and
    left alone.
- **Clean code**: no commented-out code, no debug prints, no TODO without an issue link, no unused
  dependencies, files or assets, no committed build output, no one-off scripts outside `scripts/` and none
  that nothing uses. Tests test behaviour; no empty or permanently skipped tests.
- **Cleanup rule for the overhaul**: remove only what is provably unused (no references anywhere, not
  built, not used by CI, packaging, Link, docs or another app) and whose removal keeps every CI job, the
  packaged app and its Link contracts working. Anything less certain is left in place.

## 11. Done means

An app meets the standard when:
1. Every window uses Template A and every transient surface uses Template B, with kit tokens only (a grep
   for hex colours / px font sizes outside the kit and the app's documented content palette finds nothing).
2. Light, dark and system themes all work and switch live; Inter is used; no text overflows at the minimum
   size or at 125 % font scale.
3. Loading, empty, error and populated states exist wherever data loads.
4. Every feature listed in the app's spec works when used like a real user, with real inputs.
5. `scripts/check` passes locally and CI is green on all three OSes; the release is published and Tools
   installs/updates it.
6. README, user guide, `docs/STATUS.md`, CHANGELOG and screenshots match the new UI.
