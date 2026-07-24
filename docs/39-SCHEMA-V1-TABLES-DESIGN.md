# Normalized Schema v1: Semantic Tables Design

**Status:** Accepted — 2026-07-25 (repository owner directive: proceed and complete
the semantic model)
**Tracker:** P1A-019 (schema v1 semantic extension), first structural slice
**Decision basis:** ADR-027 (`36-ADR-027-ACCEPTANCE-RECORD.md`, R4), schema v1
design (`38-NORMALIZED-SCHEMA-V1-DESIGN.md`), disposition taxonomy
(`35-DISPOSITION-TAXONOMY.md`)
**Supersedes for tables:** `38-…` §"Out of scope for v1" listed tables as
flattened structure (R4). This document promotes tables to a first-class,
additive v1 block value, following the same additive pattern already used for
inline drawings and hyperlinks (P1A-021). Retention round-trip is unchanged.

## Why now

Schema v1 flattened table-cell paragraphs into the body (R4) and reported the
`w:tbl`/`w:tr`/`w:tc` structure as unmapped. Everything still round-tripped via
Retention, but tables were not editable model values and lost their grid/merge
geometry in the semantic model. Drawings and hyperlinks have since been added as
additive v1 nodes without breaking v0/v1 compatibility (byte-identical existing
snapshots, unchanged migration golden). Tables are the next and largest such
slice, and are prerequisite structure for every later table feature
(borders/shading, cell content editing, layout).

This slice models table **structure and cell-merge geometry**. Table/row/cell
*styling* (borders, shading, alignment, row height, table style reference,
widths beyond the shared grid and cell width) remains reported and
Retention-preserved; it is a later additive slice.

## Design rules (inherited from doc 38)

- Typed, first-class values — no OOXML attribute bags in the model.
- Additive and backward-compatible: existing v0 and v1 snapshots load and
  serialize byte-identically; v0→v1 migration is unchanged (v0 has no tables).
- Deterministic: identical input + config → byte-identical snapshot; arrays
  preserve document order; ids are import-generated in canonical document order.
- Strict on load: unknown object fields rejected; every invariant validated
  first-failure-wins with typed, text-free errors.
- No silent data loss: unmapped table constructs are dispositioned in the
  compatibility report; in Retention mode they are preserved verbatim.
- Bounded: table nesting is depth-capped so validation recursion and adversarial
  JSON cannot exhaust the stack.

## Model

New block variant on the existing `v1::BlockNode` (tagged `"type"`, snake_case,
so a table serializes as `{"type":"table",…}` — additive, existing paragraph
snapshots unaffected):

```text
BlockNode {
  Paragraph(Paragraph)   // unchanged
  Table(Table)           // new
}

Table {
  id: NodeId,
  grid: [GridColumn],    // the shared column grid (w:tblGrid); may be empty
  rows: [TableRow],      // >= 1, document order
}

GridColumn { width_twips: Option<i32> }   // w:gridCol@w (dxa); optional

TableRow {
  id: NodeId,
  cells: [TableCell],    // >= 1, document order
}

TableCell {
  id: NodeId,
  properties: TableCellProperties,   // always present; empty is {}
  blocks: [BlockNode],   // >= 1 nested block (paragraphs and nested tables)
}

TableCellProperties {
  grid_span: Option<u32>,             // w:gridSpan@w (horizontal merge span)
  vertical_merge: Option<VerticalMerge>,  // w:vMerge
  width_twips: Option<i32>,           // w:tcW@w when @type is dxa (twips)
}

VerticalMerge { Restart, Continue }   // w:vMerge@val ("restart" => Restart; else Continue)
```

Cell content is a recursive `[BlockNode]`, so nested tables are naturally
representable. A cell always holds at least one block; OOXML requires a cell to
contain at least one paragraph, and import synthesizes an empty paragraph for a
degenerate empty cell so the invariant holds.

### Cell-merge geometry

- **Horizontal merge** is `grid_span` on the origin cell: the number of grid
  columns the cell spans (`w:gridSpan`). Absent means 1.
- **Vertical merge** is `vertical_merge`: `Restart` on the top cell of a merged
  run (`w:vMerge w:val="restart"`), `Continue` on each continued cell
  (`<w:vMerge/>` or `w:val="continue"`). The model records the OOXML roles
  faithfully; it does not itself collapse merged cells (a rendering/layout
  concern), preserving round-trip meaning.

Grid/merge consistency (e.g. that spans sum to the grid width) is **not**
enforced: real producers emit locally-inconsistent grids and the model must not
reject a document a word processor accepts. Structural invariants below are the
only hard rules.

## Constants and domains

- `MAX_TABLE_DEPTH = 32` — maximum table nesting depth (a root table is depth 1).
  Validation rejects deeper nesting; import caps at this depth (see below).
- `grid_span` ∈ `1..=16384` when present (0 is invalid).
- `width_twips` (cell and grid column) ∈ `0..=31_680` when present — the same
  twip ceiling used for page geometry in doc 38.

## Strict validation (additive to `Document::validate`)

Body and cell blocks are validated by one recursive walk. For every `Table`:

- `rows` is non-empty, else `EmptyTable(table.id)`.
- each `TableRow.cells` is non-empty, else `EmptyTableRow(row.id)`.
- each `TableCell.blocks` is non-empty, else `EmptyTableCell(cell.id)`.
- nesting depth ≤ `MAX_TABLE_DEPTH`, else `TableNestingTooDeep(table.id)`.
- `grid_span` and every `width_twips` in domain, else
  `PropertyValueOutOfDomain{property}` (`"table.cell.grid_span"`,
  `"table.cell.width"`, `"table.grid.column.width"`).
- each nested paragraph and nested table is validated by the same rules
  (property refs, adjacent-run normalization, ids), recursively.

Id-uniqueness (`validate_unique_ids`) and the snapshot text/block limits
(`validate_snapshot_limits`) both recurse through tables so that no id collision
and no text volume can be smuggled inside a cell. Table, row, and cell ids join
the single global id set with document, definition, paragraph, and inline ids.
Each `Table`, `TableRow`, and `TableCell`, and every nested `Paragraph`, counts
against `max_blocks`; nested run text counts against the scalar-value and
run-byte ceilings.

New `ModelError` variants (typed, carry only ids): `EmptyTable(NodeId)`,
`EmptyTableRow(NodeId)`, `EmptyTableCell(NodeId)`, `TableNestingTooDeep(NodeId)`.

## v0 → v1 migration

Unchanged. v0 has no tables, so migration still produces only paragraphs and the
byte-exact golden vector is unaffected. Tables enter the model only through
import (or authored v1 JSON).

## Import (`casual-doc-import`)

A dedicated `tables.rs` `TableStack` builder holds the open table/row/cell stack;
the body parser drives it, keeping the flat paragraph/run/segment machinery.

- `w:tbl` (block level, in body, not inside a run/pPr/rPr) opens a table and
  allocates its id in document order. Nesting beyond `MAX_TABLE_DEPTH` is
  reported (`tbl`) and treated transparently (inner content flattens into the
  enclosing cell) — unreachable in practice given the XML depth ceiling, but
  fail-safe.
- `w:tr` opens a row; `w:tc` opens a cell. A finished paragraph is routed to the
  innermost open cell, else to the body root — this replaces the R4 flatten:
  cell paragraphs now nest instead of appending to the body.
- `w:tblGrid`/`w:gridCol@w` populate the shared grid.
- `w:tcPr` scopes cell properties: `w:gridSpan@val` → `grid_span` (≥1),
  `w:vMerge` → `vertical_merge`, `w:tcW@w` → `width_twips` **only when @type is
  `dxa` or absent** (pct/auto widths are reported, not silently coerced).
- A closing `w:tc` finalizes the cell (synthesizing an empty paragraph if it has
  no blocks); `w:tr` finalizes the row (an empty row is reported and dropped);
  `w:tbl` finalizes the table (a row-less table is reported and dropped) and
  pushes a `BlockNode::Table` into its container.
- Every other table-scoped construct (`w:tblPr` and its children — table style,
  borders, shading, layout, width; `w:trPr`; unmapped `w:tcPr` children) falls
  through to the existing report arm: **reported, never silently dropped**, and
  Retention-preserved.

Determinism: ids are allocated on the opening tag (table, row, cell, paragraph),
inline ids at paragraph close, exactly as the existing machinery does — a fixed,
input-derived order independent of map/relationship enumeration.

## Round-trip and fidelity

- **Retention** is unchanged: the source package is retained and reconstructed
  byte-for-byte, so an unedited table document round-trips exactly regardless of
  semantic modeling. The re-import → identical-model check now also exercises the
  table structure.
- The LibreOffice differential fidelity harness recurses tables → cells → blocks
  when extracting text, so table cell text counts toward the text-fidelity proxy.

## Acceptance evidence

- Model: unit tests for a valid table (grid + gridSpan + vMerge), each structural
  rejection (`EmptyTable`/`EmptyTableRow`/`EmptyTableCell`/`TableNestingTooDeep`),
  domain rejections, nested-table id-uniqueness, and a JSON round-trip golden.
- Import: `real-producer-table-merges.docx` imports to a body containing a
  `Table` whose cells carry the expected `grid_span` and `vertical_merge`, with
  table structure removed from the flat body.
- Round-trip: `real-producer-table-merges.docx` import(Retention) → write →
  reopen yields an identical model and byte-identical parts.
- All workspace gates (fmt, clippy `-D warnings`, tests, doctests, wasm, MSRV,
  doc, deny, fixture checksums) green.

## Out of scope for this slice (still reported + Retention-preserved)

Table/row/cell styling (borders, shading, vertical alignment, row height,
header-row marker, table style reference, `w:tblW` table width, cell margins),
`w:tblGrid` change tracking, and any grid/span consistency normalization. Each is
an additive follow-up on this structure.
