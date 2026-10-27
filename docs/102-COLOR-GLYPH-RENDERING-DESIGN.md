# Color glyph rendering and picker design

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
Nested COLRv1 affine transforms are composed and applied to layer outlines.
Linear and radial COLRv1 gradient brushes now map to bounded tiny-skia shaders;
sweep gradients remain an explicit follow-up and safely fall back to
monochrome outlines.

## Acceptance gates

1. A native raster fixture proves a multicolor emoji has at least two distinct
   non-background colors and remains positioned at the shaped advance.
2. The same fixture passes through WASM and browser canvas without a DOM overlay.
3. A monochrome font and an unsupported/malformed color table retain the current
   output and do not panic.
4. Selection rectangles, copy text, pagination, DOCX export, and headless render
   remain unchanged.
5. Full Rust, WASM, frontend, browser, fuzz, and platform CI gates pass.

## Glyph picker panel UX (former doc 103)

### Decision

Symbol and emoji insertion use a contextual supporting pane, not a dialog.
The pane is part of the editor shell, sits after the document viewport in the
right contextual column, and keeps the document visible while the user inserts
multiple glyphs.

### Design rules

- The pane is coplanar with the editor: no backdrop, modal lock, centered card,
  dialog shadow, or `aria-modal`.
- Header hierarchy matches the existing inspector panels: icon, title, short
  description, and a compact close button.
- Search and category tabs stay in the scrollable panel body; the repeated-
  insertion affordance stays in a small footer.
- Opening Symbol closes Emoji and vice versa. The editor caret remains the
  insertion target; opening the pane must not create a second editing surface.
- At compact widths the pane becomes a fixed right overlay with the same panel
  chrome, not a centered modal.
- Escape and Close dismiss the pane and return focus to the prior editor target.

This follows established supporting-pane patterns: Carbon describes right
panels as optional content associated with a shell action, while Material's
standard side sheet keeps the primary content visible and lays out beside it.
References: https://carbondesignsystem.com/components/UI-shell-right-panel/usage/
and https://m2.material.io/components/sheets-side.

### Verification contract

Browser coverage must assert the pane has `panel-head` and `panel-body`, has no
`dialog-card` or `aria-modal`, follows the viewport in the shell, remains open
after insertion, and closes cleanly with Escape.
