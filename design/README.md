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
| `dart/` | `arcade_theme.dart` (`ArcadeTokens`, `ArcadeColors`, `arcadeTheme(Brightness, appId)`), `arcade_icons.dart` (`ArcadeIcon`, painter), `arcade_format.dart`. |
| `../crates/arcade-link/src/design.rs` | Rust, behind the `design` feature: `palette(theme, app_id)`, type/space/radius/size consts, `ICONS`, `inter_font_bytes()`, `format`; `design-egui` adds `apply_egui`. |
| `fonts/` | Inter 4.1 variable (`InterVariable.ttf`, `.woff2`) under the SIL Open Font License (`OFL.txt`). |
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
`pubspec.yaml`. The files are generated, so exclude them from `dart format`.

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
