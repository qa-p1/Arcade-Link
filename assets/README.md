# Assets

Integration-surface assets that the apps copied before the design kit. The
canonical copies now live in [`design/`](../design/README.md):

- `glyphs/<arcade-id>.svg`: generated copies of `design/glyphs/` (16 × 16,
  `currentColor`), which badge entries a peer app contributes ("Quick Look" with
  Look's eye, "Convert to WebP" with Box's cube). `glyphs/png/` holds the tray
  renders (`python3 tools/render_glyphs.py`).
- `tokens.json`: the 0.3 integration-surface keys (accents, neutrals, radii,
  motion), generated from `design/tokens.json` with the same keys so existing
  consumers keep working. New code uses the kit.
- `strings/install.json`: the shared install prompt copy.

Run `python3 design/generate.py` after changing a glyph or token. Apps that
still copy these files by hand:

| App | Copy to |
|---|---|
| Box | `apps/desktop/frontend/src/lib/arcade-glyphs.ts` (inlined SVG strings) |
| Lens | `crates/arcade-lens/src/gui/glyphs.rs` (drawn as vector paths) |
| Look | `src/lib/arcade-glyphs.ts` |
| Wheel | `assets/arcade/` (Qt resources) |
| Clipboard | `apps/flutter_app/assets/arcade/` |
