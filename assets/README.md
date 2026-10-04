# Assets

Monochrome glyphs (`glyphs/<arcade-id>.svg`, 16 × 16, `currentColor`) badge
entries that a peer app contributes ("Quick Look" with Look's eye, "Convert to
WebP" with Box's cube). `tokens.json` holds the accents, neutrals, radii and
motion used only by integration surfaces.

Each app copies what it needs into its own resources (the apps never depend
on this repository at build time). After changing a glyph, copy it again:

| App | Copy to |
|---|---|
| Box | `apps/desktop/frontend/src/lib/arcade-glyphs.ts` (inlined SVG strings) |
| Lens | `crates/arcade-lens/src/gui/glyphs.rs` (drawn as vector paths) |
| Look | `src/lib/arcade-glyphs.ts` |
| Wheel | `assets/arcade/` (Qt resources) |
| Clipboard | `apps/flutter_app/assets/arcade/` |
