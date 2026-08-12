# Color glyph rendering design

## Decision

Color emoji are rendered by the document engine, not by a DOM overlay or a
font-only web fallback. The display list remains the source of truth for native,
WASM, browser, and headless output.

## Scope

The first bounded slice supports color glyphs whose source font exposes either
`COLR/CPAL` layered outlines or embedded `CBDT/CBLC`/`SBIX` bitmap strikes.
Palette selection is deterministic: the default font palette is used unless a
future document/theme policy explicitly selects another palette. Unsupported
formats, malformed tables, and missing strikes fall back to the existing
monochrome outline path and emit a compatibility finding; they never drop the
character or abort the document.

## Display-list contract

Shaping already owns Unicode segmentation and glyph positioning. Color lookup is
performed by shaped glyph ID, so no new source-scalar metadata is required for
the renderer path. This does not alter advances, hit testing, selection offsets,
or export bytes.

## Renderer contract

The renderer probes color data before outlining a glyph. Layered outlines are
painted in palette order into the same `tiny-skia` surface and clip. Bitmap
strikes are decoded into premultiplied RGBA and composited at the glyph's
metrics-derived origin and scale. All paths are bounded by the existing font,
glyph, pixel, and document limits.

## Implementation status

The renderer now probes `COLR/CPAL` and embedded `CBDT/CBLC`/`SBIX` PNG strikes
before the monochrome outline path. Solid COLR layers are painted in palette
order on the shared surface; bitmap strikes use metrics-derived placement.
COLR gradient brushes and complex paint transforms remain explicit follow-up
work and safely fall back to monochrome outlines.

## Acceptance gates

1. A native raster fixture proves a multicolor emoji has at least two distinct
   non-background colors and remains positioned at the shaped advance.
2. The same fixture passes through WASM and browser canvas without a DOM overlay.
3. A monochrome font and an unsupported/malformed color table retain the current
   output and do not panic.
4. Selection rectangles, copy text, pagination, DOCX export, and headless render
   remain unchanged.
5. Full Rust, WASM, frontend, browser, fuzz, and platform CI gates pass.
