# Conditional Table-Style Text and Border Design

Status: accepted for `P1F-TBL-CNF-TEXT-BORDER`

## Problem

The DOCX model and semantic writer preserve every property container inside a
table style and its `w:tblStylePr` regions, but layout currently consumes only
cell shading. Header-row bold/font/paragraph formatting and conditional
table/cell borders therefore survive an edit round trip but do not render.

This is also an auto-fit correctness issue: applying a conditional font only
during final flow would measure columns with different glyph metrics from those
used to render them.

## Contract

1. The cascade resolves a typed, immutable table-style layer for each cell from
   the table style's `basedOn` chain, root first. Each style contributes its base
   paragraph, run, table, and cell properties, followed by `wholeTable`, active
   banding, first/last column, first/last row, and corner regions in increasing
   precedence.
2. `wholeTable` applies without a `w:tblLook` gate. All other conditional
   regions remain gated by the corresponding look option and the union of the
   row and cell `w:cnfStyle` selectors.
3. Effective paragraph order is `docDefaults -> table style -> paragraph style
   chain -> direct paragraph properties`. Effective run order is `docDefaults
   -> table style -> paragraph style chain -> character style chain -> direct
   run properties`.
4. The same resolved text layer is used for intrinsic-width measurement and
   final line flow. An SDT is transparent to the layer. Entering a nested table
   replaces the outer layer with that nested table cell's layer, including an
   empty layer when the nested table has no style; leaving restores the outer
   context.
5. Table-style table and cell borders merge edge by edge. Direct table borders
   overlay style table borders, and direct cell borders overlay style cell
   borders. The resulting per-cell border candidates feed the existing
   perimeter/interior selection, adjacent-cell conflict ranking, grid-span
   segmentation, and deterministic paint pipeline.
6. Layout resolution is read-only. It does not write effective properties back
   into the document model, so unsupported source detail and semantic round-trip
   behavior remain unchanged.

## Oracle policy

LibreOffice is the layout reference for this slice: conditional font metrics,
paragraph spacing/alignment, row heights, line breaks, and overall table
geometry are compared against a controlled DOCX. LibreOffice is not the exact
border-decoration oracle because it can overdraw shared borders; border
precedence and segmentation remain governed by OOXML rules and deterministic
engine regressions.

## Scope and deferrals

This increment consumes conditional paragraph/run formatting and table/cell
borders already represented by the v1 model. It does not add row-property
cascade, table alignment/bidi layout, non-zero cell-spacing conflict behavior,
floating-table placement, vertical cell text, no-wrap/fit-text, theme/automatic
border colors, art-border glyphs, or additional compound border patterns.

## Verification

- cascade unit tests for `wholeTable`, look gating, precedence, inheritance,
  direct formatting, and edge-wise border merging;
- layout regressions proving conditional text changes both intrinsic width and
  final glyph/row geometry, conditional borders reach resolved paint, and outer
  table styles do not leak into nested tables;
- a controlled DOCX rendered with LibreOffice for geometry comparison;
- formatting, strict Clippy, workspace tests, doc tests, WASM check,
  documentation generation, and diff validation.

## LibreOffice oracle result

A controlled one-page Letter DOCX was exported through LibreOffice Writer
26.2.4.2 and rendered through OpenDoc with system fonts. Both placed the 20pt
conditional header on two lines (`CONDITIONAL HEADER` / `METRICS`), followed by
the 10pt body on one line. LibreOffice's PDF text boxes placed the header at
`y=99.65..144.97pt` and the body at `y=155.65..166.81pt`; OpenDoc's raster showed
the same line distribution, paragraph-space separation, blue header text, and
green conditional top edge. Exact shared-border pixels were not compared, per
the oracle policy above.
