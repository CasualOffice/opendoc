# Table Cell-Spacing Layout Design

Status: accepted for `P1F-TBL-CELL-SPACING`

## Problem

The v1 model, DOCX importer, semantic writer, and edit surface preserve table-
and row-level `w:tblCellSpacing`, but layout ignores both. Cells therefore keep
collapsed geometry and border conflict resolution even when the document asks
for distinct cell boxes separated from one another and from the table perimeter.

## Contract

1. A row's direct `cell_spacing_twips` overrides the table's direct value. An
   absent value resolves to zero. Imported/model-validated values are
   non-negative; layout clamps defensively.
2. Non-zero spacing is split deterministically across both sides of each cell:
   floor-half on logical start/top and the remainder on logical end/bottom.
   Adjacent cell boxes therefore have the authored full gap. The outer cells
   retain half-gaps to the table perimeter, matching the separated-cell model.
3. Horizontal gaps are carved from the already solved logical grid tracks. They
   do not enlarge the table or change alignment. A pathological spacing value is
   reduced as needed to retain a one-twip cell box. `gridSpan` cells receive
   spacing only at the outside of their complete span.
4. Vertical spacing contributes to the row's minimum occupied height, while the
   bordered/shaded cell box is inset by the split top/bottom gap. Authored exact
   row heights remain exact and may clip their reduced cell box.
5. Zero-spacing rows keep the existing collapsed-border conflict algorithm.
   Non-zero rows resolve each cell edge independently, without comparing the
   abutting cell. Cell/style borders still fall back to the applicable table
   perimeter or `insideH`/`insideV` edge when omitted.
6. For non-zero rows, outer table borders are retained separately from cell
   borders and paint on the solved grid perimeter. Thus an authored outer table
   edge and an authored perimeter-cell edge remain simultaneously visible.
7. `bidiVisual` reflects the logical spacing halves, cell boxes, and both border
   layers into physical geometry. It does not change paragraph text direction.
8. Vertical merges use the starting row's top gap and closing row's bottom gap;
   the merged cell remains one box across its covered rows. Pagination, header
   repetition, splitting, hit testing, anchor lookup, and nested layout consume
   the same physical fragment geometry.

## Model and fragment changes

`CellFragment` gains a compact, serializable `CellBoxSpacing` value and a second
resolved `CellBorders` layer for the table perimeter. The ordinary `x` and
`width` remain the actual cell-border box, so existing horizontal consumers do
not need to reconstruct gaps. Vertical consumers add the recorded top offset
and use the reduced cell-box height. Composition derives the table-grid slot by
expanding the physical cell box by its start/end spacing halves.

The model/import/export contract is unchanged. Table-style-provided cell spacing
and row-property cascade remain deferred because the current conditional style
layer intentionally carries only shading, paragraph/run formatting, and borders.

## Oracle policy

ISO/IEC 29500's separated-cell and border rules are authoritative. Microsoft's
[`tblCellSpacing` contract](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.tablecellspacing?view=openxml-3.0.1)
states that row spacing overrides table spacing, lives inside the table's width,
and separates adjacent cells from the table edges. Its
[`tcBorders` interoperability note](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/7c791c5d-44c3-4fe8-abdd-8f136761bb93)
states that non-zero spacing keeps cell and outer table borders visible rather
than collapsing them. LibreOffice Writer 26.2.4.2 is still run as a practical
oracle, but the controlled DOCX currently renders identically to zero spacing;
that omission is recorded rather than copied. Word's special conflict behavior
for table-level-exception `insideH`/`insideV` borders is outside the direct-
property slice because the current model does not carry that exception layer.

## Verification

- focused layout regressions for precedence, fixed-width gap geometry, row
  height, independent/outer borders, BiDi reflection, and a grid span;
- composition coverage proving that the table perimeter and inset cell border
  produce distinct paint rectangles;
- existing import/export semantic fixed points for table and row spacing;
- formatting, strict Clippy, workspace tests/doc tests, WASM, Rustdoc, MSRV,
  fuzz-build, benchmark smoke, web gates, and diff validation before publishing.
