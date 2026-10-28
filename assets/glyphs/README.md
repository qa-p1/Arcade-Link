# Shared tray glyphs

The SVGs here are generated copies of [`design/glyphs/`](../../design/glyphs)
(`python3 design/generate.py`); edit those. All seven apps and Tools have monochrome SVGs on the family's 16-unit grid:
Box, Lens, Look, Wheel, Clipboard, Find, Shelf and Tools. SVG `currentColor`
follows the caller's foreground color. Keep the original SVG for themed trays.

`png/{16,22,24,32,48}/` contains transparent black tray renders for each ID.
Generate them with `python3 tools/render_glyphs.py` (librsvg `rsvg-convert`),
without changing the established glyph geometry. Use the nearest native pixel
size for raster trays; recolor the foreground for a dark background.
