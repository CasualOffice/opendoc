# Normalized Schema v1: Content Controls (Structured Document Tags) Design

**Status:** Implemented — 2026-07-25 (multi-agent coverage workflow; adversarially reviewed, verdict sound-with-fixes; all HARD review fixes folded in at implementation).
**Tracker:** P1A-037

> Produced by the parallel model-coverage design workflow. The adversarial review flagged concrete implementation fixes (see the tracker entry); fold them in at implementation time.



**Status:** Proposed — 2026-07-25
**Tracker:** P1A-0xx — schema v1 semantic extension (family: content controls / `w:sdt`)
**Decision basis:** ADR-027, schema v1 (`38-…`), text boxes (`41-…`, the block-container analog), tracked changes (`48-…`, the innermost-wins wrapper analog), fields/hyperlinks (`40-…`, the leaf-wrapper analog)

## Why

A content control (`w:sdt` — "structured document tag") is a wrapper Word places around content to give it a stable identity (`w:tag`), a friendly name (`w:alias`), a numeric id (`w:id`), and an editing behaviour (`w:richText`, `w:text`, `w:dropDownList`, `w:date`, `w:checkbox`, …). Today the wrapper is **reported-not-modeled**: the inner paragraphs and runs still parse (they route into the body / paragraph flow like any other content, so **no text is lost**), but the control's identity, name, tag, and type are dropped, and there is no first-class node an editor can attach to, re-bind, or round-trip. This slice models the `w:sdt` wrapper — both its **block-level** and **inline-level** forms — as a first-class, editable node carrying its properties, and continues to report (never silently drop) the `w:sdtPr` long-tail and the rarer row/cell-structural forms.

## The construct

```xml
<w:sdt>
  <w:sdtPr>
    <w:alias w:val="Full name"/>
    <w:tag w:val="fullName"/>
    <w:id w:val="1553275"/>
    <w:richText/>                 <!-- type marker; absent ⇒ rich text -->
    <w:lock .../><w:placeholder>…</w:placeholder><w:dataBinding .../>  <!-- long tail -->
  </w:sdtPr>
  <w:sdtEndPr>…</w:sdtEndPr>       <!-- end-mark run props; ignored -->
  <w:sdtContent>
    …paragraphs/tables (block)  OR  …runs (inline)  OR  …w:tr (row)  OR  …w:tc (cell)…
  </w:sdtContent>
</w:sdt>
```

A `w:sdt` is context-polymorphic. Its `w:sdtContent` holds **block** content (paragraphs/tables — when the `w:sdt` sits where a block goes), **inline** content (runs — when it sits inside a paragraph where a run goes), or, rarely, whole **rows** or **cells** (when it sits in a table's structure). This slice models the block and inline forms and defers the two structural forms (reported).

## Model — a wrapper `BlockNode` **and** a wrapper `InlineNode`, sharing typed properties

A content control is a *transparent* wrapper (like `Revision`, unlike leaf-only `Hyperlink`/`Field`): it wraps ordinary flow content and can nest. Because the same `w:sdt` element appears in both a block and an inline position, we add **both** a `BlockNode::Sdt` and an `InlineNode::Sdt`, each carrying a shared `SdtProperties`. This mirrors the block/inline split already present for tables (block) vs runs (inline) and keeps each enum's recursion type-correct.

`crates/casual-doc-model/src/v1/body.rs` (additive):

```rust
/// Maximum content-control (structured document tag) nesting depth (an `sdt`
/// inside an `sdt` inside …). Block and inline sdt nesting share this budget.
pub const MAX_SDT_DEPTH: u32 = 8;

/// The editing behaviour of a content control (`w:sdtPr` type marker). `None`
/// means the producer wrote no type marker — the OOXML default, rich text — or a
/// marker this slice does not map (then also reported). Producer-specific detail
/// of each type (list entries, date format, checkbox glyphs) is deferred.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SdtControlKind {
    RichText,             // w:richText
    PlainText,            // w:text
    ComboBox,             // w:comboBox
    DropDownList,         // w:dropDownList
    Date,                 // w:date
    Picture,              // w:picture
    Checkbox,             // w14:checkbox
    Group,                // w:group
    BuildingBlockGallery, // w:docPartObj / w:docPartList
    RepeatingSection,     // w:repeatingSection
    Citation,             // w:citation
    Bibliography,         // w:bibliography
}

/// Typed content-control properties (`w:sdtPr`). An empty value serializes to
/// `{}`. Everything else in `w:sdtPr` (lock, placeholder, data binding, list
/// entries, date/checkbox detail) is retained-and-reported, not modeled here.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtProperties {
    /// Editing behaviour, if a recognized type marker was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_kind: Option<SdtControlKind>,
    /// Friendly name (`w:alias@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Programmatic tag (`w:tag@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The producer's `w:id@w:val` as written, if declared (<= 64 bytes). Opaque
    /// and non-unique across controls — a grouping key, NOT a node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_id: Option<String>,
}

/// A block-level content control (`w:sdt` around paragraphs/tables). Its content
/// reuses the recursive block model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// An inline-level content control (`w:sdt` around runs). A transparent inline
/// range wrapper (like `Revision`): it may wrap leaf inlines, a hyperlink/field,
/// or a nested inline sdt, and may itself appear inside a hyperlink/field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped inline content (non-empty).
    pub inlines: Vec<InlineNode>,
}

// added variants (internally tagged; existing snapshots never contain them):
enum BlockNode  { Paragraph(Paragraph), Table(Table), Sdt(BlockSdt) }   // tag "sdt"
enum InlineNode { …, Sdt(InlineSdt) }                                   // tag "sdt"
```

`InlineNode::id()` gains `Self::Sdt(sdt) => sdt.id`.

### Modeling decisions

- **Two variants, one property struct.** The block/inline split is mandatory (each enum must recurse into its own node type); the shared `SdtProperties` avoids duplicating four fields and matches the `RunProperties`/`ParagraphProperties`/`TableCellProperties` "typed properties, empty is `{}`" pattern. `properties` is always present (not skipped), like `Run.properties`.
- **`w:id` is opaque, not a `NodeId`.** Exactly the `revision_id` decision (`48-…`): sdt ids are producer-local, may be absent, and may repeat across controls; promoting one to a resolvable `NodeId` would break `DuplicateNodeId` uniqueness. Retained as bounded `control_id: Option<String>`.
- **No new id newtype, no `Definitions` field.** A content control is inline/block *content*, not a cross-referenced definition (unlike comments/notes/headers). `ids.rs` and `definitions.rs` are untouched and serialize byte-identically.
- **Control kind is a bounded enum, absent ⇒ rich text.** `None` covers both "no marker" (the default) and "marker we do not yet map" (also reported at import), so the domain stays closed and additive.

## Strict validation (additive)

`crates/casual-doc-model/src/v1/document.rs`. Both walkers gain an `sdt_depth: u32` parameter threaded like `textbox_depth`:

- `validate_block(block, table_depth, textbox_depth, sdt_depth)` gains a `BlockNode::Sdt` arm:
  - `sdt_depth + 1 > MAX_SDT_DEPTH` → `SdtNestingTooDeep(sdt.id)` (parallel to `TableNestingTooDeep`).
  - `sdt.blocks.is_empty()` → `EmptySdt(sdt.id)` (parallel to `EmptyTextBox`).
  - `check_sdt_properties(&sdt.properties)`.
  - recurse: `validate_block(nested, table_depth, textbox_depth, sdt_depth + 1)` — a block sdt does **not** restart the table/text-box budgets (it is a transparent wrapper, not a fresh container in the sense a text box is), but it does advance `sdt_depth`.
- `validate_inlines(inlines, owner, in_wrapper, textbox_depth, revision_depth, sdt_depth)` gains an `InlineNode::Sdt` arm, modeled on `Revision` (transparent):
  - bound → `SdtNestingTooDeep(sdt.id)`; empty → `EmptySdt(sdt.id)`; `check_sdt_properties`.
  - recurse with `in_wrapper` **unchanged** (transparent to the leaf-only rule — an inline sdt may wrap a hyperlink/field and may itself sit inside one), `sdt_depth + 1`, and `previous_run_properties = None` (a hard merge boundary).
- `check_sdt_properties`: `alias`/`tag` non-empty and ≤ 255 (`"sdt.alias"`, `"sdt.tag"`); `control_id` non-empty and ≤ 64 (`"sdt.id"`), all via `check_domain` + `PropertyValueOutOfDomain`.
- **Text box restart interaction.** The `InlineNode::TextBox` arm passes the current `sdt_depth` through (does not reset it), matching the importer's single non-suspended path counter (below). `MAX_SDT_DEPTH = 8` is generous enough that this never rejects real documents.
- **Unique-id** (`record_block_ids`/`record_inline_ids`) and **snapshot-limit** (`accumulate_block_limits`/`accumulate_inline_limits`) accounting gain `Sdt` arms that recurse into `blocks`/`inlines` (mirroring `Table`/`TextBox` and `Revision`/`Hyperlink`), so nested ids are checked for duplicates and nested content counts against the block/text bounds.

`ModelError` (appended, additive) — `crates/casual-doc-model/src/error.rs`:

```rust
/// A content control (w:sdt) had no content (v1).
EmptySdt(NodeId),
/// A content control nested deeper than the supported bound (v1).
SdtNestingTooDeep(NodeId),
```

with `Display` strings paralleling `EmptyTextBox` / `TextBoxNestingTooDeep`.

Every existing `validate_block` / `validate_inlines` call site (body, table cells, notes, headers/footers, comments, text box) passes an initial `sdt_depth` of `0`.

## Import — flat state machine changes

`crates/casual-doc-import/src/body.rs`. Content controls live in the main body and inside notes/headers/footers/comments (which reuse `BodyParser`) and text boxes (`ContentFrame`), so **no new part** — `lib.rs`, `ParseInputs`, and the resolution indices are unchanged. Two well-worn mechanisms are reused:

- **Inline sdt = a fourth wrapper on the existing `wrapper_order` stack** (exactly like `Revision`).
- **Block sdt = a `ContentFrame`** (exactly like a text box, generalized to also emit a block).

### New parser state

| Field | Role | Suspended in `ContentFrame`? |
|---|---|---|
| `sdts: Vec<SdtAccumulator>` | open **inline** controls, innermost last (mirrors `revisions`); each holds `properties: SdtProperties` and `segments: Vec<Segment>` | yes |
| `WrapperKind::Sdt` on `wrapper_order` | routes segments into the innermost inline sdt | yes (order stack already suspended) |
| `sdt_scopes: Vec<SdtScope>` | scope (`Inline`/`Block`/`Passthrough`) of every open `w:sdt`, so `</w:sdtContent>`/`</w:sdt>` route by scope | yes |
| `pending_block_sdt_props: Vec<SdtProperties>` | properties of open **block** controls awaiting their `w:sdtContent` | yes |
| `sdt_prop_depth: u32` | inside a `w:sdtPr`/`w:sdtEndPr` subtree (guards its inner `w:rPr` etc. out of flow) | yes |
| `sdt_depth: u32` | **path** nesting counter for the `MAX_SDT_DEPTH` guard; **not** suspended, so it matches the model (a text box does not reset it) | **no** |
| `ContentFrame.kind: FrameKind` (`TextBox`/`BlockSdt`) + `ContentFrame.sdt_properties: SdtProperties` | lets `exit_frame` emit either an inline `TextBox` segment or a `BlockNode::Sdt` | (frame itself) |

`Segment` gains `Sdt { properties: SdtProperties, children: Vec<Segment> }`; `SdtAccumulator { properties, segments }` mirrors `RevisionAccumulator`.

### Context detection (at `<w:sdt>`)

Scope is decided from the surrounding parser state, exactly as `b"p"` vs `b"r"` vs `b"tbl"` are today:

- **Inline** when `paragraph_open && !run_open && sdt_prop_depth == 0 && drawing_depth == 0 && pict_depth == 0` — the position where a run/hyperlink goes.
- **Block** when `in_body && !paragraph_open && !run_open && ppr_depth == 0 && rpr_depth == 0 && sdt_prop_depth == 0` **and** not in a table's structural gap — i.e. either no table is active, or a cell is open (`TableStack::in_cell()`, a small new predicate). Block content flows into the cell after `exit_frame` via the restored `self.tables.push_block`.
- **Passthrough** (reported, transparent — inner content still parses, wrapper identity lost) for every other position:
  - inside a run (`run_open`) — invalid nesting;
  - **row-structural** (`tbl` active, no row open) and **cell-structural** (`tbl` active, row open, no cell open) sdt — deferred (see below);
  - over `MAX_SDT_DEPTH` (`sdt_depth >= MAX_SDT_DEPTH`).

In all three scopes we push `sdt_scopes`; only `Inline`/`Block` bump `sdt_depth`. `Passthrough` reports `b"sdt"` once and does nothing structural, so its `w:sdtPr`/`w:sdtContent` are inert markers and its inner rows/cells/paragraphs flow to the current container unchanged — **no data loss**.

### Open / close arms

`on_start`:
- `b"sdt"` → decide scope; push `sdt_scopes`. **Inline:** `sdt_depth += 1`; push `SdtAccumulator` onto `sdts`; `wrapper_order.push(WrapperKind::Sdt)`. **Block:** `sdt_depth += 1`; `pending_block_sdt_props.push(SdtProperties::default())`. **Passthrough:** `reporter.report(b"sdt")`.
- `b"sdtPr" | b"sdtEndPr"` (when an sdt is open) → `sdt_prop_depth += 1`.
- **Property markers, only while `sdt_prop_depth > 0`**, routed to the innermost open sdt's property slot (`sdts.last_mut().properties` if `sdt_scopes.last() == Inline`, else `pending_block_sdt_props.last_mut()`; `Passthrough` discards):
  - `b"alias"` → `alias` (filter non-empty, ≤ 255); `b"tag"` → `tag`; `b"id"` → `control_id` (≤ 64).
  - `b"richText" | b"text" | b"comboBox" | b"dropDownList" | b"date" | b"picture" | b"checkbox" | b"group" | b"docPartObj" | b"docPartList" | b"repeatingSection" | b"citation" | b"bibliography"` → set `control_kind`.
  - **any other element while `sdt_prop_depth > 0`** (lock, placeholder, showingPlcHdr, dataBinding, listItem, date/checkbox detail, `w:sdtEndPr` `w:rPr`) → `reporter.report(local)`. This arm is placed **before** the generic `rPr`/`pPr`/block arms so a `w:sdtPr` `w:rPr` can never leak into run/paragraph flow.
- `b"sdtContent"` → **Block scope:** `enter_sdt_block` — allocate the node id (document order), move `pending_block_sdt_props.pop()` into `frame.sdt_properties`, then suspend context (`FrameKind::BlockSdt`) exactly as `enter_textbox` (the `sdt_scopes` `Block` entry rides into the frame; the frame gets a fresh `sdts`/`sdt_scopes`, so nested controls start clean). **Inline/Passthrough:** inert (inline segments already route via `wrapper_order`).

`on_end`:
- `b"sdtContent"` → **Block:** `exit_frame` — build `BlockNode::Sdt(BlockSdt { id, properties, blocks })` from the frame's accumulated blocks and route via the restored `self.tables.push_block` (into the enclosing cell, or the body root); an empty or over-depth frame is reported and dropped (parallel to the empty-text-box path). **Inline/Passthrough:** inert.
- `b"sdtPr" | b"sdtEndPr"` → `sdt_prop_depth = sdt_prop_depth.saturating_sub(1)`.
- `b"sdt"` → pop `sdt_scopes`. **Inline:** `sdt_depth -= 1`; `commit_sdt` — pop the `WrapperKind::Sdt` marker and the `sdts` accumulator, `normalize_segments`, and (non-empty) `push_segment(Segment::Sdt { properties, children })` into the enclosing wrapper or paragraph (empty → report + drop, like `commit_revision`). **Block:** `sdt_depth -= 1` (defensively drain a still-pending `pending_block_sdt_props` / open frame if `w:sdtContent` never arrived). **Passthrough:** inert.

`push_segment` gains a `Some(WrapperKind::Sdt) => sdts.last_mut().segments.push(segment)` arm; `commit_top_wrapper` gains `Some(WrapperKind::Sdt) => self.commit_sdt()`, so `finish_paragraph`'s drain flushes an inline sdt left open by malformed input. `segment_to_inline` gains a `Segment::Sdt` arm allocating the wrapper id first, then recursing into children (document order, mirroring `Hyperlink`/`Revision`).

`ContentFrame` save/restore (`enter_*`/`exit_*`) swaps in the new suspended fields (`sdts`, `sdt_scopes`, `pending_block_sdt_props`, `sdt_prop_depth`) alongside `revisions`/`wrapper_order`, and `exit_frame` branches on `frame.kind` to emit a `Segment::TextBox` (unchanged) or a `BlockNode::Sdt`. `sdt_depth` is intentionally **not** saved/restored (it is a true path counter).

### Silent-data-loss risks and mitigations

| Risk | Mitigation |
|---|---|
| Wrapper identity/props dropped | modeled as `BlockSdt`/`InlineSdt` with `alias`/`tag`/`id`/`control_kind` |
| Block sdt content lost | `ContentFrame` collects its blocks; routed into cell/body on `</w:sdtContent>` |
| Inline sdt content lost | wrapper accumulator routes its runs like a hyperlink; committed on `</w:sdt>` |
| `w:sdtPr` inner `w:rPr` leaking into a run | `sdt_prop_depth` guard + a dedicated report arm before the flow arms |
| Row/cell-structural sdt | `Passthrough` — reported; inner `w:tr`/`w:tc` parse into the table unchanged |
| sdt inside a hyperlink/field/revision (and vice-versa) | `wrapper_order` innermost-wins already composes all four wrapper kinds |
| Nested / over-deep sdt | stack entry + `sdt_depth`/`MAX_SDT_DEPTH` guard → `Passthrough` beyond bound |
| Empty / unclosed sdt | empty → reported+dropped; unclosed → drained at paragraph/frame/note close |
| Unmapped `w:sdtPr` long-tail | reported via the `sdt_prop_depth` report arm |

## Backward-compatibility

Strictly additive. `BlockNode::Sdt` and `InlineNode::Sdt` are new internally-tagged variants (tag `"sdt"`); no existing variant's bytes change and existing snapshots never contain them. `SdtProperties`/`SdtControlKind` are new; all `SdtProperties` fields are `#[serde(default, skip_serializing_if …)]`. `definitions.rs` and `ids.rs` are untouched (no new id newtype, no `Definitions` field), so they serialize byte-identically. The `validate_*` and accounting walkers gain a threaded `sdt_depth` parameter (internal, not serialized). The v0→v1 migration and its byte-exact golden are unchanged (v0 has no content controls). `ModelError` variants are appended.

## Explicitly deferred (reported, not modeled)

Each reaches an existing report path (the new `sdt_prop_depth` report arm, the `Passthrough` `report(b"sdt")`, or the final `_ if self.in_document => report`), so it surfaces in the compatibility report — no silent drop. Follow-up slices:

- **`w:sdtPr` long tail:** `w:lock`, `w:placeholder`/`w:showingPlcHdr`, `w:dataBinding`, `w:sdtEndPr` run props, and per-type detail — `w:comboBox`/`w:dropDownList` `w:listItem` entries, `w:date` (`w:dateFormat`, `w:calendar`, `w:storeMappedDataAs`), `w14:checkbox` checked state and checked/unchecked glyphs, `w:docPartObj`/`w:docPartList` gallery/category, `w:repeatingSection`/`w:repeatingSectionItem`.
- **Row-level and cell-level (structural) content controls** — an `sdt` wrapping `w:tr`/`w:tc`; needs the `Table` model to carry a row/cell wrapper.
- **Bookmarks** (`w:bookmarkStart`/`End`) — the sibling remaining gap; separate slice.

## Test plan

- **Model** (`crates/casual-doc-model/src/v1/tests.rs`, extend the `any_inline`/block walkers): block and inline sdt round-trip with each `SdtControlKind` and with `alias`/`tag`/`control_id`; empty block sdt → `EmptySdt`; empty inline sdt → `EmptySdt`; nested sdt within bound accepted, over `MAX_SDT_DEPTH` → `SdtNestingTooDeep`; oversized `alias`/`tag`/`control_id` → `PropertyValueOutOfDomain`; sdt ids unique across nesting; inline sdt as a transparent wrapper inside a hyperlink **and** wrapping a hyperlink both validate; `{}` `properties` serialization is minimal; v0→v1 golden byte-identical.
- **Import** (`crates/casual-doc-import/src/tests.rs`): block content control around paragraphs/tables modeled (id/alias/tag/kind); inline content control around runs modeled; block sdt inside a table cell lands in the cell; inline sdt inside a hyperlink and hyperlink inside an inline sdt (innermost-wins regression guard); nested sdt; `w:sdtPr` long-tail (`lock`/`dataBinding`/`listItem`) reported not modeled; a `w:sdtPr` `w:rPr` does not leak into flow; row/cell-structural sdt reported with inner cells intact; empty content control dropped+reported; over-depth → passthrough; unclosed `w:sdt` flushes its content; content control inside a text box round-trips.
- **Walkers:** add `Sdt` arms recursing into `blocks`/`inlines` to the fidelity walker (`tools/opendoc-fidelity/src/main.rs`) and the export presence walker (`crates/casual-doc-export/src/lib.rs`).
- **Fixtures** (`docs/23-DOCX-FIXTURE-CORPUS.md`): add a block content control and an inline content control fixture; assert round-trip.

All gates (fmt, clippy, unit, doctest, wasm, MSRV 1.85, doc) as in prior slices.

## CHANGELOG (Unreleased → Added)

> Content controls in schema v1: block-level (`w:sdt` around paragraphs/tables) and inline-level (`w:sdt` around runs) structured document tags are modeled as additive `BlockNode::Sdt` / `InlineNode::Sdt` wrappers carrying typed properties (control kind, alias, tag, retained `w:id`) and wrapping their content. Row/cell-structural controls and the `w:sdtPr` long tail (locks, placeholders, data binding, per-type detail) remain reported (not yet modeled). Additive: existing snapshots and the v0→v1 migration golden are byte-identical.

---

**Files this slice touches:** `crates/casual-doc-model/src/v1/body.rs` (types, variants, `MAX_SDT_DEPTH`, `InlineNode::id`), `crates/casual-doc-model/src/v1/document.rs` (`validate_block`/`validate_inlines`/`check_sdt_properties`/id+limit walkers, threaded `sdt_depth`), `crates/casual-doc-model/src/error.rs` (`EmptySdt`, `SdtNestingTooDeep`), `crates/casual-doc-import/src/body.rs` (state, `on_start`/`on_end` arms, `enter_sdt_block`/`exit_frame`/`commit_sdt`, `Segment::Sdt`), `crates/casual-doc-import/src/tables.rs` (`in_cell()` predicate), `crates/casual-doc-import/src/properties.rs` (optional: an `apply_sdt_property` helper), plus the two walkers and tests/fixtures. `definitions.rs`, `ids.rs`, `migration.rs`, and `lib.rs`/`ParseInputs` are untouched.