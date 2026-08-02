# Table Alignment and BiDi-Visual Layout Design

Status: accepted for `P1F-TBL-ALIGN-BIDI`

## Problem

The v1 model, DOCX importer, semantic writer, edit commands, and WASM surface
retain table `w:jc`, row `w:jc`, and `w:bidiVisual`. Layout currently ignores
all three: every row begins at the table indent and every grid column is placed
left-to-right. Narrow centered/end-aligned tables and visually RTL tables
therefore render in the wrong physical location even though their properties
round-trip.

## Contract

1. Table and row `Alignment::Start`/`End` are logical placements. In a normal
   table start is the physical left and end is the physical right; in a
   `tbl_bidi_visual` table those mappings reverse. Center is direction-neutral.
   An absent alignment defaults to start.
2. A row's direct `w:jc` overrides the table's direct `w:jc` for that row. All
   rows retain the one solved table grid and width; row alignment changes only
   the grid's physical origin within the containing block.
3. `w:tblInd` is a logical-start inset. It participates in available-width
   solving and start placement. Center/end placement is resolved against the
   containing width and does not add the start inset a second time.
4. `w:bidiVisual` mirrors the physical placement of logical grid ranges while
   preserving source/model cell order and IDs. A spanning cell occupies the
   same logical columns and width, but its physical left edge is reflected
   through the solved table box.
5. Logical start/end cell margins and resolved vertical borders swap on the
   physical canvas for a mirrored table. Independently resolved top/bottom
   border segments are reflected within their cell box. Content flow width is
   unchanged; its physical left inset uses the reflected margin.
6. `w:bidiVisual` does not force paragraph base direction or reorder characters
   inside a cell. Paragraph `w:bidi` and Unicode BiDi shaping remain the owners
   of cell text direction.
7. Pagination, repeated-header rows, row splitting, vertical merges, hit testing,
   anchor lookup, and composition continue to consume the physical `CellFragment`
   geometry; no browser-DOM state is introduced.

## Scope and deferrals

This increment consumes direct table/row alignment and direct table
`tbl_bidi_visual`. Style-provided table/row geometry, cell spacing, floating
tables, vertical cell text, and Word's legacy distinction between physical
`left`/`right` tokens and logical `start`/`end` are deferred. The v1 importer
already normalizes those legacy aliases, so layout cannot reconstruct their
original spelling.

## Oracle policy

LibreOffice Writer is the practical geometry reference, with ISO/IEC 29500's
table-justification and `bidiVisual` rules authoritative when the oracle diverges.
Controlled fixed-width tables cover start/center/end placement, row override,
visually RTL column order, a grid span, unequal logical margins, and asymmetric
borders. The invariants are physical ordering and twip-scale offsets, not exact
font pixels.

## Verification

- focused layout regressions for alignment mapping, row override, mirrored
  unequal columns/spans, margins, and border segments;
- existing import/export semantic fixed points for all modeled properties;
- a controlled DOCX rendered through LibreOffice and OpenDoc;
- formatting, strict Clippy, workspace tests/doc tests, WASM, Rustdoc, MSRV,
  fuzz-build, benchmark smoke, web gates, and diff validation before publishing.

## Oracle result

A controlled semantic DOCX was rendered to PDF by LibreOffice Writer 26.2.4.2.
Writer placed the 600-twip-indented start table, centered table, and end table at
distinct increasing physical positions and displayed an unequal 1000/2000/3000
twip `bidiVisual` grid in reversed cell order: the first logical cell was the
rightmost and the third logical cell the leftmost.

Two LibreOffice limitations were made explicit rather than copied into the
runtime. Writer did not apply a row-level `w:jc` override, and it treated the
Office-2010 `start`/`end` spellings as physical edges in the RTL probe. The
normative [table-justification rule](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tablejustification?view=openxml-3.0.1)
says the interpretation reverses for a `bidiVisual` parent and separately defines
row-level alignment; the [`bidiVisual` rule](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.bidivisual?view=openxml-3.0.1)
also requires the first logical cell and its logical sides to appear at the
physical right. OpenDoc follows that contract; the five layout regressions pin
the logical mapping, row override, unequal columns, a grid span, reflected
margins/vertical borders, and reflected horizontal-border segments.
