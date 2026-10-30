# Arcade design kit

One design for every Arcade app, implemented once per toolkit. The rules are in
[`STANDARD.md`](STANDARD.md) (a copy of the overhaul standard, kept identical);
this folder holds the values and the code.

| Path | What |
|---|---|
| `tokens.json` | Every colour, type, space, radius, size, motion, shadow and icon value (STANDARD §2-4). The source of truth. |
| `generate.py` | Writes every file below from the tokens. Python stdlib only, deterministic. `--check` fails when a generated file is stale, an accent misses its contrast pair, two default shortcuts clash or the component CSS has a literal colour or px font size. |
| `css/arcade.css` | Custom properties for light and dark (`prefers-color-scheme` with an `html[data-theme]` override), `[data-app]` accents, and the shared base, shell and component rules (Tools 0.3.8's look). Generated from `css/components.css`. |
| `css/icons.ts`, `css/format.ts` | The icon path map and the shared format helpers (`formatSize`, `formatCount`, `formatDuration`, `formatRelative`). |
| `qml/Arcade/` | QML module: `Theme` (singleton: colours per theme, `accent(appId)`, type, space, radius, motion), `Fonts` (Inter via `FontLoader`), `Icon`, `Format.js`. Component sets are added by the first QML app spec. |
| `dart/` | `arcade_theme.dart` (`ArcadeTokens`, `ArcadeColors`, `arcadeTheme(Brightness, appId)`), `arcade_icons.dart` (`ArcadeIcon`, painter), `arcade_format.dart`, and the hand-written components in `arcade_widgets.dart` (`ArcadeButton`, `ArcadeSwitchRow`, `ArcadeListSection` + `ArcadeRow`, `ArcadeBadge`/`ArcadeDot`, `ArcadeKeyChip`/`ArcadeKeyCombo`, `ArcadeNotice`, `ArcadeEmptyState`, `showArcadeToast`, `ArcadeSidebar` + `ArcadeNavItem`, `ArcadePageHead`, `ArcadeSegmented`, `ArcadeSearchField`, `ArcadeOverlayRow`, `ArcadeHintBar`, `showArcadeDialog`, `confirmArcadeDestroy`, `ArcadeSkeletonRow`). |
| `../crates/arcade-link/src/design.rs` | Rust, behind the `design` feature: `palette(theme, app_id)`, type/space/radius/size consts, `ICONS`, `inter_font_bytes()`, `format`; `design-egui` adds `apply_egui` and the shared `egui::components` set. |
| `fonts/` | Inter 4.1 variable (`InterVariable.ttf`, `.woff2`) plus static 500/600 egui instances, under the SIL Open Font License (`OFL.txt`). |
| `icons/` | The outline icon set: `icons.json` (name → 24×24 path data) and one SVG per icon (generated). Stroke 1.5, round caps and joins, `currentColor`. |
| `glyphs/` | The app marks `arcade.<app>.svg` (16-unit grid). `assets/glyphs/` keeps generated copies for older consumers. |
| `app-icons/` | `make.py` (needs `rsvg-convert`) and every app's launcher icon set: PNG 16-1024, `.ico`, macOS `.icns` and AppIcon PNGs inside the 10 % safe area, iOS, Android adaptive, and Tauri's `icons/` names. |
| `shortcuts.json` | The default global shortcut of every app; clashes fail `generate.py --check`. |
| `format-cases.json` | The cases every toolkit's format helpers must pass. |

Change a value in `tokens.json` (or an icon in `icons/icons.json`, a component
in `css/components.css`), run `python3 design/generate.py`, commit the result,
and tag Link. CI runs `generate.py --check`, the Rust, TypeScript, QML and Dart
tests, and `make.py` is re-run by hand when a glyph or accent changes.

## Consuming the kit

Every app pins one Link tag for both its Link dependency and the kit.

### CSS (Tools, Box, Look)

`scripts/sync-design.py` in the app copies `design/css/arcade.css`,
`design/css/icons.ts`, `design/css/format.ts`, `design/fonts/InterVariable.woff2`,
`design/fonts/OFL.txt`, `design/glyphs/` and its own `design/app-icons/<app>/`
from a Link checkout at the pinned tag into the app's `design/` folder, and
records SHA-256 hashes in `VENDORED`; `scripts/verify-vendored.py` checks them
in CI:

```sh
git clone --depth 1 --branch v0.4.0 https://github.com/qa-p1/Arcade-Link /tmp/link
python3 scripts/sync-design.py /tmp/link
```

Load `arcade.css` before the app's own stylesheet, set `<html data-app="arcade.<app>">`
and, for the Appearance setting, `data-theme="light"` or `"dark"` (no attribute
follows the system). Declare Inter with `@font-face { font-family: Inter;
src: url(design/fonts/InterVariable.woff2) format("woff2"); font-weight: 100 900; }`.

### QML (Shelf, Wheel)

`tools/vendor_qt.py --destination <app>/vendor/arcade-link` vendors the Qt
module, `design/qml/Arcade` and `design/fonts/` under the same `VENDORED.json`
pin. Add `vendor/arcade-link/design/qml` to the QML import path, then:

```qml
import Arcade
Rectangle { color: Theme.panel; border.color: Theme.line; radius: Theme.radiusMd }
```

Set `Theme.appId` once (`"arcade.shelf"`) and bind `Theme.mode` to the app's
Appearance setting (`"system"`, `"light"`, `"dark"`). `Fonts.qml` loads
`../../fonts/InterVariable.ttf`; set `Fonts.source` if the app packs the font
elsewhere (for example in a resource file).

### Flutter (Clipboard)

Copy `design/dart/*.dart` into the app (for example `lib/design/`) and record
the hashes in the app's vendored-file list, then:

```dart
MaterialApp(theme: arcadeTheme(Brightness.light, 'arcade.clipboard'),
            darkTheme: arcadeTheme(Brightness.dark, 'arcade.clipboard'));
```

Bundle `design/fonts/InterVariable.ttf` as the `Inter` font family in
`pubspec.yaml`. `arcade_theme.dart` and `arcade_icons.dart` are generated, so
exclude `lib/design/` from `dart format`; `arcade_widgets.dart` is written by
hand here and changed only in Link. `tools/check_dart_kit.sh` renders every
component in light and dark and fails on overflow.

### Rust (Find, Lens, and the Rust parts of other apps)

```toml
arcade-link = { git = "https://github.com/qa-p1/Arcade-Link", tag = "v0.4.0", features = ["design", "design-egui"] }
```

```rust
arcade_link::design::egui::install_inter(&ctx);
arcade_link::design::apply_egui(&ctx, "system", "arcade.lens");
let p = arcade_link::design::palette(arcade_link::design::Theme::Dark, "arcade.find");
```

softbuffer / tiny-skia surfaces use `palette()`, `design::ICONS` (flat path
operations) and `inter_font_bytes()`.

## Shared conventions

- Format helpers: one implementation per toolkit with the same cases.
- `arcade_link::paths::{app_dirs, legacy_dirs, migrate_app_dirs}` (Qt:
  `ArcadePaths.h`): the `arcade/<app>` folders, every old location
  (`spec/app-dirs.json`) and a copy-verify-remove migration that is safe to run
  on every start.
- `arcade_link::diagnostics::Header` (Qt: `DiagnosticsHeader`): the first lines
  of every app's "Copy diagnostics".

### Readable foregrounds

Keep status and app accent tokens for fills. Use `--ok-ink`, `--warn-ink`,
`--danger-ink` and `--accent-ink` for text and action icons on neutral or
matching soft surfaces. Flutter exposes `okInk`/`warnInk`/`dangerInk`/`accentInk`,
QML exposes the same properties, and Rust exposes `design::ink(theme, app_id)`.
The generator derives these from the existing colours, moving toward the
theme's text only as far as necessary to meet 4.5:1 on panel, background,
sunken, hover and the matching soft fill. Fill colours and existing palette
APIs stay unchanged. Metadata, placeholders and hints use `muted`; `faint` is
reserved for disabled or decorative content.

### egui components

`design::egui::components::Kit::new(ui, app_id)` resolves the current theme
on every frame. Use `sidebar`, `page` (Template A content), `page_head`
(`page_head_with` for actions), `section_head` (`section_head_with`), `list`, `row`,
`switch_row`, `button`/`button_state`/`icon_button`, `text_input`, `select`,
`segmented`, `badge`, `notice`, `key_chip`, `empty_state`, `glyph_tile`,
`row_with_glyph` (Connected apps rows), `checkbox` (dialog options and list
selection) and `dialog_head` / `dialog_actions` (dialog title, body and
right-aligned actions, primary last) for shared UI.
Use `page_with_head` or `page_with_head_actions` for settings and data pages
whose heading must stay visible: the body owns the scroll area. `page` remains
available for existing callers. Row names ellipsize with their complete tooltip;
descriptions and paths wrap, and changes in their height never cross a divider.
Repeated row labels have independent cached measurements. Disabled controls use
readable muted inks without an additional opacity reduction. Enabled unchecked
checkbox boundaries retain at least 3:1 contrast before hover.
`Tone` selects readable status/accent inks; `ButtonKind` selects a button's
role. `path` wraps selectable paths and `ellipsis` retains the full tooltip.
`ShortcutRecorder::row` (with a `ShortcutSpec`) is the settings row; it validates keys and holds conflicting changes until the
user chooses; its callback must combine Link registry and desktop conflicts.
`Toast` keeps errors until closed, expires other messages after five seconds,
and uses 1.5 seconds for `Toast::copied()`.

Each data panel owns its own scroll area. Keep the page head outside it.
Install Inter once when the egui context is created, apply Appearance whenever
it changes, and rebuild `Kit` each frame to follow live system changes.

The egui kit uses `font(type_token)` / `font_weight(size, weight)` and real
Inter Medium/Semibold families. egui 0.33's `RichText.strong()` changes only
colour. `fonts/make-egui.py` (fonttools 4.61.1) creates deterministic instances
from the bundled variable font; `egui-weights.json` records the source and
output hashes. The design check verifies the hashes and SFNT weight tables.

The egui kit also provides `welcome_head` (52 px mark, display title and one
sentence) and `disclosure` for collapsible secondary information. Apps keep
Skip in their fixed page actions and use at most three welcome steps. Use
`manifest::peer_description(app, peer)` for Connected apps descriptions.
