# Text-box appearance and positioning design

Status: accepted for the bounded implementation described here.

## Problem

The importer already models a DrawingML text box's optional extent, fill, outline,
and floating anchor, but the inline import path discards those properties. Layout
then invents a black one-pixel outline and sizes every inline box to the available
column. The semantic writer also emits every text box as a minimal inline shape,
which silently loses floating anchors and authored appearance on save.

## Decisions and invariants

1. `TextBox::extent` is meaningful for both inline and floating DrawingML boxes.
   A missing or zero dimension is not guessed during import.
2. Layout honors a positive authored width and height. A missing or zero width
   falls back to the available flow width; a missing or zero height falls back to
   the flowed content height plus the existing internal inset.
3. Fill, outline color, and outline width travel from import through layout to
   paint. No outline is fabricated when the document does not provide one.
4. Floating text boxes continue through the established anchor placement layer.
   Inline text boxes remain paragraph flow items.
5. Semantic export emits `wp:inline` or `wp:anchor` from the model and preserves
   the extent, anchor, z-order key, fill, and outline. Import → export → import
   must therefore reach a semantic fixed point for these properties.
6. Existing resource and nesting bounds remain unchanged. Text-box content still
   uses the shared block-flow pipeline rather than a second renderer.

## Compatibility boundary

This slice did not add `wps:bodyPr` insets, vertical anchoring, rotation,
autofit, overflow clipping, or text wrapping around floating objects. Insets,
vertical anchoring, bounded autofit, and overflow policy are designed separately
in `docs/52-TEXT-BOX-BODY-PROPERTIES-DESIGN.md`; rotation and the deliberate
legacy-VML fallback remain later work.
