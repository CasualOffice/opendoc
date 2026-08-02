# Intrinsic Inline-Box Table Sizing Design

Status: accepted for `P1F-TBL-INTRINSIC-INLINE`

## Problem

The v1 model, semantic writer, and final layout retain and render inline pictures,
embedded-object previews, typed math, fields, and inline text boxes. Table auto-fit
still computes cell intrinsic widths through a text-only `collect_runs` path and
reduces each shaped line using glyph runs alone. A cell can therefore be measured
as narrow text and later paint a much wider modeled object outside that column.

## Contract

1. Cell intrinsic sizing flattens a paragraph through the same `FlowItem` mapping
   used by final flow. Wrapper visibility, run-property cascade, table-style
   context, media lookup, field fallback values, and typed-math layout therefore
   have one source of truth.
2. The minimum pass shapes at a one-twip measure and the preferred pass at the
   existing bounded unwrapped measure. Fixed-size inline pictures, embedded-object
   previews, typed math, and authored inline text boxes contribute their resolved
   outer width in both passes. Fields contribute the width of the same cached or
   deterministic placeholder value that final flow uses.
3. An inline text box without a positive authored width derives its pass-specific
   width recursively from the intrinsic width of its blocks plus start/end insets.
   Recursion uses the active table-style context and the existing model depth and
   numeric bounds. The final text-box flow then consumes that resolved width.
4. Anchored objects remain out of normal inline width. Float barriers/exclusions
   affect available layout space, not intrinsic demand. A full-width horizontal
   rule is likewise width-dependent and contributes only the minimum non-zero box,
   rather than making every auto-fit column request the unwrapped sentinel width.
5. The line-width reducer includes the right edges of glyph runs, images, inline
   text boxes, and rules. It uses saturating arithmetic and remains bounded by the
   existing twip/model limits.
6. Measurement uses a throwaway font-resolution report and numbering state and
   never mutates the document, live list counters, or compatibility report.

## Oracle policy

LibreOffice Writer is the geometry reference. A controlled auto-fit table must
assign enough width for authored inline boxes and keep the same relative column
ordering. Exact raster pixels, image interpolation, and font substitution are not
part of this slice.

## Scope and deferrals

This increment changes intrinsic measurement only; final inline-object painting
and the v1 schemas remain unchanged. It does not add natural-size media probing
when an extent is absent, parse embedded chart/OLE content, make anchored floats
part of table width, implement Word's complete preferred-width negotiation, or
replace the existing field/tab plus inline-object compatibility path.

## Verification

- semantic fixed-point coverage for the modeled inline extents and cached field
  content used by measurement;
- layout regressions for pictures/object previews, typed math, fields, authored
  and widthless text boxes, wrapper recursion, and nested table cells;
- a controlled DOCX rendered with LibreOffice and OpenDoc;
- formatting, strict Clippy, workspace tests, doc tests, WASM check, Rustdoc,
  MSRV, benchmark smoke, web gates, and diff validation before publication.

## LibreOffice oracle result

A controlled HTML auto-width table containing a 200 CSS-pixel image and a
one-character neighbor was opened and saved as DOCX by LibreOffice Writer
26.2.4.2, then exported to PDF. Writer authored the inline extent as
`cx=1905000` (exactly 3000 twips) and froze its resolved table geometry into grid
columns of 3161 and 271 twips. The object-bearing column therefore remained wider
than the object and roughly 11.7 times the text-only neighbor. The OpenDoc
end-to-end regression constructs the equivalent positive-extent model and proves
the auto-fit result keeps the first column at least 3000 twips and wider than the
neighbor. PDF text placement also leaves the `x` in the narrow second column.

LibreOffice saved the HTML-derived table with `w:tblLayout="fixed"`; its frozen
grid is used only as the expected geometry outcome, not as evidence that Writer's
post-save DOCX still performs auto-fit.
