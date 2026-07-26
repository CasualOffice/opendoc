# Text-box body properties design

Status: accepted for the bounded implementation described here.

## Problem

`wps:bodyPr` is currently discarded. Layout substitutes one 72-twip inset on
every side, always places content at the top, never applies autofit, and lets
nested content paint outside an authored text-box extent. The same hard-coded
behavior is used for inline boxes, floating boxes, grouped boxes, and boxes in
headers and footers.

## Model and import decisions

1. `TextBox` and `GroupTextBox` carry one `TextBoxBodyProperties` value.
2. The four DrawingML insets remain independent signed EMU coordinates. Omitted
   left/right values resolve to 91,440 EMU (0.1 inch); omitted top/bottom values
   resolve to 45,720 EMU (0.05 inch).
3. Vertical anchoring is modeled as top, center, or bottom; omitted means top.
4. Horizontal overflow is `overflow` or `clip`. Vertical overflow is
   `overflow`, `clip`, or `ellipsis`. Omitted means `overflow`.
5. Autofit is a choice:
   - absent/`noAutofit`: authored dimensions remain fixed;
   - `spAutoFit`: the shape grows vertically to contain its flowed content and
     insets, but never shrinks below a positive authored height;
   - `normAutofit`: the authored `fontScale` and `lnSpcReduction` percentages are
     retained and consumed while flowing the content.
6. Invalid enum tokens or percentages are reported through the compatibility
   reporter and fall back to their schema defaults. Signed inset coordinates are
   represented as `i32`, matching `ST_Coordinate32` without an unbounded integer
   path.
7. Semantic export emits the modeled `wps:bodyPr` attributes and autofit child.
   Import → write → reopen must reach a semantic fixed point for inline,
   floating, and grouped text boxes.

## Layout and paint decisions

1. A text box's inner width is `outer width - left inset - right inset`, clamped
   to one twip. Its content uses the existing recursive block flow; there is no
   text-box-only paragraph, table, image, or object renderer.
2. A fixed positive authored height remains the outer height except for
   `spAutoFit`. A missing/zero height resolves to content height plus top/bottom
   insets.
3. Top/center/bottom anchoring offsets the already-flowed block stack within the
   inner height. Negative free space never moves overflowing content upward.
4. `normAutofit@fontScale` scales run font size, letter spacing, and baseline
   shift while shaping. `lnSpcReduction` is applied only to percentage line
   spacing, as required by DrawingML.
5. `horzOverflow="clip"` and `vertOverflow="clip"` push a bounded paint clip on
   the selected axis around nested paragraphs, tables, images, and drawings.
   `vertOverflow="ellipsis"` currently uses the same safe clip but does not
   synthesize an ellipsis; the semantic value is preserved and this visual
   limitation remains explicit.
6. The resolved content origin and clip policy travel with both
   `InlineTextBox` and floating `AnchorContent::TextBox`. Composition therefore
   cannot accidentally fall back to a global inset.

## Context invariant

Body paragraphs, table cells (including nested tables), headers, and footers all
enter the same block-flow and float-placement functions. Tests must exercise an
inline body box, a nested cell box, a floating header box, a floating footer box,
and a grouped box. No feature is considered supported if it only works in the
main document body.

## Compatibility boundary

This slice does not implement text rotation/vertical writing, `anchorCtr`,
columns inside a text body, preset text warp, exact vertical ellipsis generation,
or automatic computation of a missing `normAutofit@fontScale`. Legacy VML
`inset`, CSS positioning, and VML-specific overflow remain in the later VML
positioning slice; existing VML boxes retain their prior uniform-inset fallback.
