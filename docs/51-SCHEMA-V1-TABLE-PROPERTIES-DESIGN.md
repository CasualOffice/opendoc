# Normalized Schema v1: Table-Property Long Tail Design

**Status:** Designed — 2026-07-25 (multi-agent coverage workflow; adversarially reviewed, verdict sound-with-fixes). Pending implementation.
**Tracker:** P1A-035

> Produced by the parallel model-coverage design workflow. The adversarial review flagged concrete implementation fixes (see the tracker entry); fold them in at implementation time.



**Status:** Proposed — 2026-07-25
**Tracker:** P1A-033 / P1A-034 / P1A-035 (three slices under P1A-019 schema-v1 semantic extension)
**Decision basis:** ADR-027, schema v1 (`38-…`), tables structure (`39-…`), tracked changes (`48-…`, closest recent analog), importer no-skip audit
**Files:** `crates/casual-doc-model/src/v1/table.rs`, `.../properties.rs`, `.../document.rs`, `.../error.rs`; `crates/casual-doc-import/src/{tables.rs,body.rs,properties.rs}`

## Why

`w:tblPr` / `w:trPr` and the `w:tcPr` long tail are the last table constructs that are reported-not-modeled. Today `Table` and `TableRow` carry **no properties field at all**, and `TableCellProperties` maps only `gridSpan` / `vMerge` / `tcW`. Every other property element (`tblBorders`, `shd`, `tblCellMar`, `jc`, `tblLayout`, `tblLook`, `trHeight`, `cantSplit`, `tblHeader`, `tcBorders`, `tcMar`, `vAlign`, `noWrap`, `textDirection`) falls through `body.rs` to `_ if self.in_document => self.reporter.report(local)` — surfaced in the compatibility report, byte-preserved in Retention, but not editable in the model. This design makes table/row/cell formatting first-class and editable while preserving the no-silent-loss and byte-identical-backward-compat contracts.

## Model overview — where the new fields attach

Three attachment points, mirroring the OOXML nesting:

| Level | OOXML | Model target | Field shape |
|---|---|---|---|
| Table | `w:tbl > w:tblPr` | **new** `TableProperties` on `Table` | `#[serde(default, skip_serializing_if = "TableProperties::is_empty")] properties: TableProperties` |
| Row | `w:tr > w:trPr` | **new** `TableRowProperties` on `TableRow` | `#[serde(default, skip_serializing_if = "TableRowProperties::is_empty")] properties: TableRowProperties` |
| Cell | `w:tc > w:tcPr` | **extend existing** `TableCellProperties` | new `Option<…>` fields, each `skip_serializing_if = "Option::is_none"` |

`TableCell.properties` is already always-serialized (`{}` when empty) and stays that way — its bytes are unchanged when the new options are `None`. `Table` and `TableRow` gain a **skipped** properties field so existing snapshots (which have no such key) round-trip byte-identically (empty = omitted). This is the same additive shape used for `Table.grid` (`skip_serializing_if = "Vec::is_empty"`).

### Shared value types (introduced in Slice A, reused by Slice C)

Borders, shading, and cell margins appear at both TABLE and CELL level, so they are modeled once as shared types in `table.rs`:

```rust
/// One border edge (`w:top`/`w:start`/`w:bottom`/`w:end`/`w:insideH`/`w:insideV`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BorderEdge {
    /// ST_Border line style token (`w:val`), lowercased; `1..=32` bytes.
    /// Retained opaquely: the ST_Border list (~180 incl. art borders) is a
    /// producer-facing enum, so the token is kept verbatim rather than
    /// promoted to a closed Rust enum (the codebase's opaque-when-producer-
    /// specific convention). Editable size/color/space are typed.
    pub style: String,
    /// Line width in eighth-points (`w:sz`); `0..=1024`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_eighth_points: Option<u32>,
    /// Line color (`w:color`), explicit sRGB only; `auto` reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<RgbColor>,
    /// Padding between border and text in points (`w:space`); `0..=31`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_points: Option<u32>,
}

/// A border set (`w:tblBorders` / `w:tcBorders`). Any subset of edges.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBorders {
    #[serde(skip_serializing_if = "Option::is_none")] pub top: Option<BorderEdge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub start: Option<BorderEdge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub bottom: Option<BorderEdge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub end: Option<BorderEdge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub inside_h: Option<BorderEdge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub inside_v: Option<BorderEdge>,
}
impl TableBorders { pub fn is_empty(&self) -> bool { *self == Self::default() } }

/// Cell shading (`w:shd`). Only the background fill is modeled; a non-`clear`/
/// `nil` pattern or a non-`auto` pattern color is *also reported* (degraded, so
/// no silent loss of visible shading semantics).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shading {
    /// Background fill (`w:fill`), explicit sRGB; `auto` yields `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<RgbColor>,
}
impl Shading { pub fn is_empty(&self) -> bool { self.fill.is_none() } }

/// Cell content margins (`w:tblCellMar` / `w:tcMar`), dxa (twips); `0..=31_680`.
/// Non-`dxa` (`pct`/`nil`) margin types are reported.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMargins {
    #[serde(skip_serializing_if = "Option::is_none")] pub top_twips: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub start_twips: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub bottom_twips: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub end_twips: Option<i32>,
}
impl CellMargins { pub fn is_empty(&self) -> bool { *self == Self::default() } }
```

`RgbColor` is reused from `properties.rs` (already exported via `mod.rs` glob). `mod.rs` re-exports `table::*`, so every new type is public automatically — no export edits.

---

## Slice A — Table properties (`w:tblPr`) [P1A-033]

New `TableProperties` on `Table`, plus the shared types above.

```rust
/// Table layout algorithm (`w:tblLayout/@w:type`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableLayout { Fixed, Autofit }

/// Conditional-formatting look flags (`w:tblLook`). All-false = omitted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableLook {
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub first_row: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub last_row: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub first_column: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub last_column: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub no_h_band: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")] pub no_v_band: bool,
}
impl TableLook { pub fn is_empty(&self) -> bool { *self == Self::default() } }

/// Typed table properties (`w:tblPr`). An empty value is omitted.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableProperties {
    /// Table alignment (`w:jc`); start/center/end (justify reported).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Preferred table width, dxa only (`w:tblW/@w:type="dxa"`); `0..=31_680`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_twips: Option<i32>,
    /// Layout algorithm (`w:tblLayout`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<TableLayout>,
    /// Conditional-format look flags (`w:tblLook`).
    #[serde(skip_serializing_if = "TableLook::is_empty")]
    pub look: TableLook,
    /// Table borders (`w:tblBorders`).
    #[serde(skip_serializing_if = "TableBorders::is_empty")]
    pub borders: TableBorders,
    /// Table background shading (`w:shd`).
    #[serde(skip_serializing_if = "Shading::is_empty")]
    pub shading: Shading,
    /// Default cell margins (`w:tblCellMar`).
    #[serde(skip_serializing_if = "CellMargins::is_empty")]
    pub cell_margins: CellMargins,
}
impl TableProperties { pub fn is_empty(&self) -> bool { *self == Self::default() } }
```

`Alignment` (import mapper `alignment_from`) is reused; `w:jc="both"/"distribute"` would map to `Justify`, which is meaningless on a table — the import arm maps only `start/left/center/end/right` and reports anything else.

**`Table` struct change** (`table.rs`):
```rust
pub struct Table {
    pub id: NodeId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid: Vec<GridColumn>,
    #[serde(default, skip_serializing_if = "TableProperties::is_empty")]
    pub properties: TableProperties,   // NEW — placed after grid, before rows
    pub rows: Vec<TableRow>,
}
```

### Bounded domains (in `validate_table`, `document.rs`)
| Property | Domain | property-name string |
|---|---|---|
| `tblW` twips | `0..=31_680` | `table.width` |
| border `sz` | `0..=1024` | `table.borders.size` |
| border `style` | non-empty, `≤32` bytes | `table.borders.style` |
| border `space` | `0..=31` | `table.borders.space` |
| cell margins | `0..=31_680` each | `table.cell_margins` |

A shared `check_borders(&TableBorders, prefix)` and `check_margins(&CellMargins, prefix)` helper is added and called for both table and cell (Slice C), taking the property-name prefix so the domain string is stable per level.

### Import mapping (`body.rs` + `tables.rs`)
- New parser state `tblpr_depth: u32` (on `BodyParser` **and** `ContentFrame`, initialized 0, saved/restored — exactly like `tcpr_depth`).
- Open guard: `b"tblPr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 && <no open row>` → `self.tblpr_depth += 1`. (A `w:tblPrEx` on a row is **not** matched here → reported; see Deferred.) `on_end` `b"tblPr"` → `saturating_sub(1)`.
- Property arms guarded by `self.tblpr_depth > 0`, each calling a new `TableStack` setter that writes into `self.stack.last_mut().properties`:
  - `jc` → `alignment_from(val)` → `set_table_alignment`; else `report(b"jc")`.
  - `tblW` → dxa `w` clamped `0..=31_680` → `set_table_width`; `pct`/`auto` → `report(b"tblW")` (mirrors the existing `tcW` arm).
  - `tblLayout` → `fixed`/`autofit` → `set_table_layout`; else report.
  - `tblLook` → parse `firstRow`/`lastRow`/`firstColumn`/`lastColumn`/`noHBand`/`noVBand` attrs, else decode legacy hex `@w:val` bitmask → `set_table_look`.
  - `tblBorders` → enter a small nested-edge capture (see below) → `set_table_borders`.
  - `shd` → fill from `@w:fill` hex via `parse_rgb`; `auto`/theme → `fill=None`; a non-`clear`/`nil` `@w:val` **or** non-`auto` `@w:color` → also `report(b"shd")` (degraded, fill still captured).
  - `tblCellMar` → capture `top`/`start`|`left`/`bottom`/`end`|`right` child `w:w`(dxa) → `set_table_cell_margins`.

**Border-set capture.** `tblBorders`/`tcBorders` are containers whose children are the six edges. Add `tblbordersscope` handling with a tiny depth flag on the parser: on `b"tblBorders"`/`b"tcBorders"` (guarded by `tblpr_depth`/`tcpr_depth`) set a `border_scope: Option<BorderTarget>`; each edge child (`top`/`start`/`left`/`bottom`/`end`/`right`/`insideH`/`insideV`) builds a `BorderEdge` from `@w:val`(→`style`, lowercased, dropped+reported if empty), `@w:sz`, `@w:color`, `@w:space`, routed by `border_scope`; on the container `on_end`, commit the accumulated `TableBorders` to the table or cell. Diagonal edges `tl2br`/`tr2bl` (cell only) are reported (Deferred). This mirrors how `w:ind` builds a composite `Indentation` in `properties.rs`, just spread over child elements instead of attributes.

`TableStack` gains: `set_table_alignment`, `set_table_width`, `set_table_layout`, `set_table_look`, `set_table_borders`, `set_table_shading`, `set_table_cell_margins`, each mutating `self.stack.last_mut()?.properties`. `TableBuilder` gains `properties: TableProperties` (defaulted in `open_table`), committed in `close_table` (`properties: table.properties`).

---

## Slice B — Row properties (`w:trPr`) [P1A-034]

New `TableRowProperties` on `TableRow`.

```rust
/// Row-height rule (`w:trHeight/@w:hRule`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]     // "auto" | "atLeast" | "exact"
pub enum HeightRule { Auto, AtLeast, Exact }

/// Row height (`w:trHeight`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowHeight {
    /// Height in twips (`w:val`); `0..=31_680`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_twips: Option<u32>,
    /// Height rule (`w:hRule`); absent = `auto` in OOXML.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<HeightRule>,
}
impl RowHeight { pub fn is_empty(&self) -> bool { *self == Self::default() } }

/// Typed table-row properties (`w:trPr`). An empty value is omitted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableRowProperties {
    /// Row height (`w:trHeight`).
    #[serde(skip_serializing_if = "RowHeight::is_empty")]
    pub height: RowHeight,
    /// Keep row on one page (`w:cantSplit`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cant_split: bool,
    /// Repeat as header row across pages (`w:tblHeader`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
}
impl TableRowProperties { pub fn is_empty(&self) -> bool { *self == Self::default() } }
```

**`TableRow` struct change:**
```rust
pub struct TableRow {
    pub id: NodeId,
    #[serde(default, skip_serializing_if = "TableRowProperties::is_empty")]
    pub properties: TableRowProperties,   // NEW — after id, before cells
    pub cells: Vec<TableCell>,
}
```

### Bounded domains (in `validate_table`, per row)
| Property | Domain | property-name string |
|---|---|---|
| `trHeight` twips | `0..=31_680` | `table.row.height` |

`cant_split`/`header` are booleans — no domain check.

### Import mapping
- New parser state `trpr_depth: u32` (parser + frame, like `tcpr_depth`).
- Open guard: `b"trPr" if self.tables.is_active() && self.suppressed_tbl_depth == 0 && <row open, no cell open>` → `trpr_depth += 1`; `on_end` decrements.
- Arms under `trpr_depth > 0`:
  - `trHeight` → `value_twips` from `@w:val` (clamped `0..=31_680`), `rule` from `@w:hRule` (`atLeast`/`exact`/`auto`) → `set_row_height`.
  - `cantSplit` → `set_row_cant_split(is_true(val))` (OOXML on/off; reuse `is_true`).
  - `tblHeader` → `set_row_header(is_true(val))`.
- `RowBuilder` gains `properties: TableRowProperties` (defaulted in `open_row`), committed in `close_row` (`properties: row.properties`).

---

## Slice C — Cell property long tail (`w:tcPr`) [P1A-035]

Extend the existing `TableCellProperties`, reusing `TableBorders` / `Shading` / `CellMargins` from Slice A.

```rust
/// Cell vertical text alignment (`w:vAlign`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment { Top, Center, Bottom }

/// Cell text flow direction (`w:textDirection`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]     // OOXML tokens lrTb/tbRl/btLr
pub enum TextDirection { LrTb, TbRl, BtLr }

pub struct TableCellProperties {
    // ---- existing (unchanged bytes) ----
    #[serde(skip_serializing_if = "Option::is_none")] pub grid_span: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] pub vertical_merge: Option<VerticalMerge>,
    #[serde(skip_serializing_if = "Option::is_none")] pub width_twips: Option<i32>,
    // ---- NEW ----
    #[serde(skip_serializing_if = "TableBorders::is_empty")] pub borders: TableBorders,
    #[serde(skip_serializing_if = "Shading::is_empty")]      pub shading: Shading,
    #[serde(skip_serializing_if = "CellMargins::is_empty")]  pub margins: CellMargins,
    #[serde(skip_serializing_if = "Option::is_none")]        pub vertical_alignment: Option<VerticalAlignment>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]     pub no_wrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]        pub text_direction: Option<TextDirection>,
}
```

`TableCellProperties` is currently `Copy`; adding `TableBorders`/`CellMargins` (which own a `String` via `BorderEdge.style`) **drops `Copy`** (and `Eq` stays, `Clone`/`Debug`/`Default`/`PartialEq`/`Serialize`/`Deserialize` remain). `tables.rs` moves the cell `properties` value in `close_cell` (already a move into `TableCell`), and the `current_cell()` setters take `&mut`, so no `Copy` dependency exists — verified against `tables.rs` usage. Same applies to `Table`/`TableRow` (already non-`Copy`).

### Bounded domains (in `validate_table`, per cell — reuse Slice-A helpers with a `table.cell.*` prefix)
| Property | Domain | property-name string |
|---|---|---|
| cell border size/style/space | as Slice A | `table.cell.borders.{size,style,space}` |
| cell margins | `0..=31_680` | `table.cell.margins` |

`vertical_alignment`, `no_wrap`, `text_direction` are closed enums / bool — validated by type.

### Import mapping (extends the existing `tcpr_depth > 0` arms in `body.rs`)
- `tcBorders` → shared border-set capture (Slice A) with `BorderTarget::Cell`.
- `shd` (under `tcpr_depth`) → `set_cell_shading` (same fill/degraded-report rule).
- `tcMar` → capture edges → `set_cell_margins`.
- `vAlign` → `top`/`center`/`bottom` → `set_cell_vertical_alignment`; else report.
- `noWrap` → `set_cell_no_wrap(is_true(val))`.
- `textDirection` → `lrTb`/`tbRl`/`btLr` → `set_cell_text_direction`; other legacy tokens (`tbLrV`, `lrTbV`, `tbRlV`) reported.
- `tl2br` / `tr2bl` (diagonal cell borders) → reported (Deferred).

The current `gridSpan`/`vMerge`/`tcW` arms are untouched; the new arms slot in beside them under the same `tcpr_depth > 0` guard.

---

## Backward-compatibility (byte-identical)

- **Additive only.** Every new field uses `skip_serializing_if` (`Option::is_none`, `<Type>::is_empty`, or `std::ops::Not::not` for bools). An empty value serializes to nothing, so a table/row/cell with no long-tail properties is byte-identical to today.
- `Table` / `TableRow` gain `#[serde(default, skip_serializing_if = …)]` properties fields → absent in existing snapshots is accepted (default) and re-emitted as absent (skip). `deny_unknown_fields` is unaffected (a defaulted, absent field is legal).
- `TableCell.properties` byte layout is unchanged: existing keys stay in order; new keys only appear when set.
- **v0→v1 migration golden unchanged.** `migration.rs` constructs no `Table` (v0 has no tables — confirmed: it only builds paragraphs/runs), so the migration output and its byte-exact golden are untouched regardless of these additions.
- **No new ids, no new `Definitions` field, no new id newtype.** Table properties are inline value objects, so `validate_unique_ids`, `record_block_ids`, and `accumulate_*_limits` need no changes (they already recurse table rows/cells for ids/limits; the new fields carry none). `ids.rs` / `definitions.rs` serialize byte-identically.

## No silent data loss

Every degraded or unmodeled sub-case reaches the Reporter, never a silent consume:
- Non-`dxa` widths/margins (`pct`/`auto`/`nil`) → `report` (mirrors the existing `tcW` precedent).
- `w:shd` with a real pattern or pattern color beyond a plain fill → `report` (fill still captured — partial, flagged).
- Border edge with empty `@w:val` → edge dropped + `report`.
- `w:jc="both"/"distribute"` on a table, unknown `tblLayout`/`vAlign`/`textDirection` tokens → `report`.
- Anything not matched by a new arm still hits `_ if self.in_document => self.reporter.report(local)` — the default no-loss backstop is preserved.

## Explicitly deferred (reported, not modeled)

Each reaches a report arm today and continues to, so it surfaces in the compatibility report:

- **`w:tblPrEx`** (per-row table-property exceptions) — a row-scoped override of table properties; needs override-merge semantics. Reported.
- **Table-style conditional formatting** (`w:tblStylePr`, `w:cnfStyle` on rows/cells) — table-style band/first-row overrides; belongs with a table-styles slice.
- **Diagonal cell borders** (`w:tl2br` / `w:tr2bl`).
- **`w:shd` pattern + pattern color** (only `fill` modeled; pattern reported).
- **Theme fills / `themeFill*`** on shading and `themeColor` on borders (only explicit sRGB modeled).
- **`w:tblInd`, `w:tblOverlap`, `w:bidiVisual`, `w:tblCaption`/`w:tblDescription`, `w:tblpPr`** (floating-table positioning), **`w:hidden`/`w:fitText`** cell flags, row **`w:jc`/`w:wBefore`/`w:wAfter`/`w:gridBefore`/`w:gridAfter`/`w:divId`**.
- **Property-change revisions** on tables (`w:tblPrChange`/`w:trPrChange`/`w:tcPrChange`) — already deferred by the tracked-changes slice (`48-…`).

## Test plan

**Model (`v1/tests.rs`):**
- Round-trip each new type: table with `jc`/`tblW`/`tblLayout`/`tblLook`/`tblBorders`/`shd`/`tblCellMar`; row with `trHeight`(each `hRule`)/`cantSplit`/`tblHeader`; cell with `tcBorders`/`shd`/`tcMar`/`vAlign`/`noWrap`/`textDirection`.
- **Empty-omission (byte-compat guard):** a `Table`/`TableRow`/`TableCell` with default properties serializes to the *exact* pre-change JSON (assert no `"properties"` key). This is the load-bearing backward-compat test.
- Domain rejection: over-range `tblW`/`trHeight`/margins/border `sz`, empty border `style`, oversized style token → `PropertyValueOutOfDomain { property: … }` with the expected stable string.
- Extend the `any_table`/table-walker test helper to populate every new field.

**Import (`import/src/tests.rs`) — one fixture per slice + a combined fixture:**
- `tblPr` with borders/shd/jc/layout/look/cellMar → asserted on the model; `tblW pct` and `jc both` reported.
- `trPr` with `trHeight atLeast`/`cantSplit`/`tblHeader` → modeled.
- `tcPr` long tail incl. diagonal border + patterned shd → modeled fields present, diagonal + pattern reported.
- Degraded cases: `tcW pct` still reported (regression guard on the pre-existing arm); malformed border edge dropped+reported.
- Balanced-depth guard: `tblPr`/`trPr`/`tcPr` open/close keep `tblpr_depth`/`trpr_depth`/`tcpr_depth` balanced across nested tables (extends the existing suppression/nesting tests).
- Fidelity walker (`tools/opendoc-fidelity`) and any export-presence walker gain table/row/cell-properties arms so the new fields are exercised end-to-end.

**Corpus:** add a `table-formatting` DOCX fixture (borders + shading + header row + merged-cell margins) to `docs/23-DOCX-FIXTURE-CORPUS.md`, with a round-trip assertion like the existing `table-with-merged-cells` fixture.

**Gates:** fmt, clippy (`-D warnings`), unit, doctest, wasm build, MSRV 1.85, doc — per prior slices.

## CHANGELOG (Unreleased → Added)

> Table, row, and cell formatting in schema v1: table properties (`w:tblPr` — alignment, dxa width, layout, look flags, borders, shading, default cell margins), row properties (`w:trPr` — height/rule, cantSplit, header), and the cell-property long tail (`w:tcPr` — borders, shading, margins, vertical alignment, noWrap, text direction) are modeled as additive `TableProperties`/`TableRowProperties` and new `TableCellProperties` fields, with shared `TableBorders`/`Shading`/`CellMargins` value types. Non-dxa widths/margins, patterned shading, diagonal borders, `tblPrEx`, and table-style conditional formatting remain reported (not yet modeled). Additive: existing snapshots and the v0→v1 migration golden are byte-identical.

---

**Slice sequencing / dependency:** Slice A **must land first** (it introduces the shared `TableBorders`/`Shading`/`CellMargins` types and the border-set capture machinery that Slice C reuses). Slices B and C are independent of each other and can land in either order after A. Recommended order: A (P1A-033) → B (P1A-034) → C (P1A-035).

**Key source anchors reviewed:** `table.rs:33-81` (existing `TableCellProperties`/`Table`/`TableRow`, the `skip_serializing_if` grid precedent); `document.rs:418-455` (`validate_table` + `check_domain`); `properties.rs:58-84,138-167` (composite `Indentation`/`Spacing` build + `parse_rgb`/`is_true`); `body.rs:960-1015` (the `tcpr_depth` dispatch and `tcW` degraded-report precedent); `tables.rs` `TableBuilder`/`RowBuilder`/`CellBuilder` (where the new `properties` accumulators attach). `migration.rs` builds no tables → migration golden is safe.