# VML Text-Box Positioning Design

Status: accepted for the bounded P1F-VML-POS implementation slice.

## Problem

Legacy Word VML is already parsed into the shared drawing and text-box model,
but its positioning bridge is incomplete:

- `mso-position-horizontal` and `mso-position-vertical` alignments are ignored;
- left/right/inner/outer and top/bottom margin reference frames collapse to a
  smaller set of anchors;
- `w10:wrap`, `mso-wrap-mode`, and the four `mso-wrap-distance-*` values are
  discarded, so positioned VML pictures and shapes cannot participate in the
  existing float-reflow path;
- `v:textbox@inset`, `v-text-anchor`, and `mso-fit-shape-to-text` are parsed
  incompletely or replaced by one hard-coded inset;
- header/footer text boxes can be positioned, but body text boxes are forced
  inline unconditionally after a previous all-float implementation caused
  severe overlap in real VML-primary documents.

The last point is a correctness constraint, not merely a missing feature. The
body fallback must not be removed until the layout engine can honor the source
object's exclusion semantics.

## Source semantics

Microsoft's Office conformance notes define the Word-specific VML position
tokens:

- `mso-position-horizontal` selects absolute, left, center, right, inside, or
  outside placement within the frame named by
  `mso-position-horizontal-relative`;
- `mso-position-vertical` selects absolute, top, center, bottom, inside, or
  outside placement within the vertical relative frame;
- the relative-frame vocabularies include physical and mirrored margin areas;
- `margin-left` and `margin-top` supply offsets for absolute positioning.

Word always honors `v:textbox@inset`. VML also defines `v-text-anchor` for
vertical text placement and `mso-fit-shape-to-text` for growing a shape to its
content.

Primary references:

- [Microsoft Office notes for VML shape positioning](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/edfd844d-bf1a-4109-81cd-4caa788f1449)
- [Microsoft Office notes for VML text boxes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/f88c8cef-d1f4-42bf-b2da-6aa9f2dd128e)
- [VML text-box element and inset/property surface](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-textbox-element)
- [VML wrap modes](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-wrap-mode-attribute)
- [VML wrap-distance semantics](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-wrap-distance-left-attribute)
- [VML vertical text anchor](https://learn.microsoft.com/is-is/windows/win32/vml/msdn-online-vml-v-text-anchor-attribute)
- [VML shape-autofit behavior](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-fit-shape-to-text-attribute)

## Decisions

### 1. Keep VML parsing neutral

`vml.rs` remains a best-effort, model-independent parser. It gains neutral
position-alignment, wrap, distance, and text-body values. Mapping to
`DrawingAnchor` and `TextBoxBodyProperties` stays in `body.rs`.

Malformed or unknown tokens use the existing bounded fallback rather than
failing document import.

### 2. Preserve all anchor frames and alignments that the model can express

VML frames map onto the existing page, margin, column/character, physical
margin-strip, and mirrored-margin anchors. Relative alignment maps to the
existing horizontal and vertical alignment variants; absolute placement keeps
the signed twip offset.

Percentage offsets remain out of scope because the model has no percentage
position representation.

### 3. Carry VML wrap semantics through every positioned VML object

The parser records the common square, tight, through, top-and-bottom, and none
modes from `w10:wrap` or `mso-wrap-mode`, plus the four independent wrap
distances. A child shape inherits group-level wrap data when it does not
override it.

The importer maps these values onto `DrawingAnchor` for VML pictures, shapes,
and text boxes. This makes local top-and-bottom VML objects use the reflow path
already shared by body, table-cell, header, and footer content. Unsupported
square/tight/through page-level exclusion remains preserved in the model but is
still flow-neutral, as documented by the float-reflow design.

### 4. Restore body positioning only inside the proven-safe envelope

A body VML text box becomes a float only when all of the following hold:

1. it has a genuine absolute position and positive box dimensions;
2. its vertical frame is paragraph, text, or line;
3. its wrap mode is top-and-bottom.

That is exactly the local exclusion case the layout engine can honor today.
Page- or margin-relative body boxes, no-wrap overlays, and side-wrapped boxes
remain inline. This preserves readable document order and avoids restoring the
known overprint regression.

Header/footer text boxes retain their existing positioned behavior because
they are page furniture measured and repeated in a bounded band. Degenerate
header/footer boxes still fall back inline.

### 5. Defer every VML text box until its shape metadata is available

`w:txbxContent` closes before `w:pict`, so the current body path emits an inline
box before the VML shape has been parsed. Both body and running-content boxes
will instead queue their flowed blocks until `commit_pict`. This permits one
container-aware decision while preserving document order. An unmatched or
malformed shape drains to an inline box so text is never lost.

### 6. Preserve text-box body properties in both float and fallback paths

Explicit VML insets map independently to the shared body-property insets.
`v-text-anchor` maps top/middle/bottom to top/center/bottom placement, and
`mso-fit-shape-to-text` maps to shape autofit.

The safe floating path also retains the authored extent, fill, and stroke. The
inline safety fallback retains inset/alignment/autofit and appearance but does
not force the authored absolute extent; using a fixed, potentially undersized
height with overflow enabled can recreate the overlap this policy prevents.

## Coverage

Required regressions:

- parser coverage for relative alignment, physical/mirrored frames, wrap mode,
  wrap distances, non-uniform inset, vertical text anchor, and shape autofit;
- positioned VML image/shape carries wrap and alignment into its anchor;
- paragraph-relative top-and-bottom body text box becomes a positioned float;
- unsafe page-relative/no-wrap body text box remains inline while retaining its
  body properties and appearance;
- positioned header and footer text boxes retain their anchor, extent, inset,
  vertical alignment, fill, and stroke;
- a positioned VML text box inside a body table cell follows the same safe
  decision and remains discoverable by the nested-float path;
- malformed/unmatched VML text-box content drains inline without loss.

## Explicit follow-ups

- percentage offsets (`mso-left-percent` / `mso-top-percent`);
- exact inside/outside odd/even parity;
- square/tight/through and page-level fixed-point reflow;
- VML vertical writing/rotation and linked text-box chains;
- exact generic VML path geometry, gradients, and per-side strokes;
- emitting legacy VML rather than normalized DrawingML on semantic export.
