# Document-grid line-pitch design

Status: Accepted for the bounded body-flow slice (2026-08-12)

## 1. Problem

The semantic model and DOCX reader/writer preserve section `w:docGrid`,
paragraph `w:snapToGrid`, and run `w:snapToGrid`, but layout currently consumes
none of them. A positive section line pitch therefore has no effect on line
boxes, row geometry, or pagination. The private Medical corpus probe carries
`<w:docGrid w:linePitch="360"/>`; OpenDoc currently produces three pages while
the LibreOffice oracle produces four.

This is a general OOXML layout gap, not permission to tune a global line-height
constant for that document.

## 2. Normative behavior used by this slice

The ISO/IEC 29500 `docGrid` and `snapToGrid` rules establish these precedence
rules:

1. A positive `linePitch` on a section grid controls inter-line pitch for
   paragraphs which snap to the grid. An omitted grid `type` with a positive
   line pitch is the producer form used by Word and is treated as a line grid.
   Explicit `lines` and `linesAndChars` also enable the line grid; explicit
   `default` and `snapToChars` do not.
2. Paragraph `snapToGrid` is style-cascaded. When it is never specified, it is
   effectively on while a line grid exists. Explicit false disables the grid.
3. `lineRule="exact"` overrides the document-grid pitch.
4. Table-cell lines do not use the pitch by default. They use it only when
   `w:compat/w:adjustLineHeightInTable` is explicitly on.

Normative reference summaries: Microsoft Open XML SDK documentation for
[`DocGrid.LinePitch`](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.docgrid.linepitch?view=openxml-3.0.1)
and
[`AdjustLineHeightInTable`](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.adjustlineheightintable?view=openxml-3.0.1).

The resolved natural line height already contains run font metrics and an
authored `lineRule="auto"` multiple. An `atLeast` value raises that required
height. Grid application then rounds the required height upward to the next
positive pitch multiple. This keeps glyph ink contained when a line needs more
than one grid unit and makes every resulting advance an integral grid unit.
Additional space is placed below the existing baseline, preserving current
glyph baseline geometry while fixing block height and pagination.

## 3. Data and layout flow

`DocumentSettings` gains the additive boolean
`adjust_line_height_in_table`. Import consumes the compatibility child instead
of reporting it, and semantic export writes it before the existing
`compatSetting` tail. False remains the serialization default.

The section-aware document driver resolves a small private line-grid value from
each `SectionBoundary` and passes it with that section's block slice into flow.
`FlowCtx` owns that value and an explicit table nesting depth. Paragraph
constraints receive a pitch only when paragraph, grid-type, and table-policy
checks all pass. Because nested table cells and inline text boxes recurse through
the same context, the table policy cannot be bypassed accidentally.

The public whole-document galley helpers apply a grid only when the document has
exactly one section. Multi-section fidelity belongs to the canonical
section-aware `paginate_document` driver; applying the first section's grid to
the entire document would be wrong.

The canonical incremental document entry point sends every section-bearing
DOCX through fresh section runs. The lower-level public single-section galley
cache can also consume a section grid, so its paragraph geometry hash includes
the resolved grid pitch. Table blocks are always re-flowed by that cache and do
not reuse nested paragraph fragments. A future cache for canonical per-section
runs must likewise include resolved grid pitch and table compatibility in its
section geometry key.

## 4. Determinism and bounds

- Zero, absent, negative, or model-invalid pitches are inactive.
- Height rounding uses widened integer arithmetic and clamps to the representable
  twip domain; it never loops once per grid unit.
- Exact spacing remains byte-for-byte unchanged.
- With no active grid, every existing line box remains byte-for-byte unchanged.
- Table depth is restored after every recursive cell flow, including nested
  tables.

## 5. Tests and acceptance

The slice is accepted when tests prove:

- positive line pitch rounds natural and `atLeast` line boxes upward;
- exact spacing and paragraph `snapToGrid=false` bypass it;
- omitted/`lines`/`linesAndChars` grid types activate line pitch while
  `default`/`snapToChars` do not;
- two sections can carry different pitches without cross-section leakage;
- changing a single-section grid pitch invalidates cached paragraph geometry;
- table cells remain unchanged by default and opt in only through
  `adjustLineHeightInTable`;
- the compatibility flag survives import -> semantic write -> reopen and is no
  longer reported as unsupported;
- focused layout tests and the full CI-equivalent Rust/WASM gates pass.

The rights-restricted Medical probe may be recovered from repository history for
local measurement, but must not be re-added to the working tree or pull request.
Its page count is evidence, not a committed test fixture.

## 6. Explicit deferrals

- `charSpace` and run-level `snapToGrid` character-grid shaping;
- absolute page-origin baseline snapping beyond deterministic line advances;
- section-grid application to headers, footers, floating text boxes, and note
  stories, which need explicit story/section ownership;
- a per-section incremental galley cache;
- any global line-height tuning or corpus-specific exception.
