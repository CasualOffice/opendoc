# Schema v1 Design Reference

**Status:** Consolidated reference — accepted designs, mixed implementation state
(per-construct status below).
**Decision basis:** ADR-027 (`36-ADR-027-ACCEPTANCE-RECORD.md`), ADR-014 (grapheme
positions), ADR-018 (strict bounded JSON), disposition taxonomy
(`35-DISPOSITION-TAXONOMY.md`).
**Supersedes for import:** `22-NORMALIZED-SCHEMA-V0.md` is retained as the v0
baseline; v1 is a superset reached by a deterministic, total v0→v1 migration.

## About this document

This is the single consolidated design record for the Phase 1A semantic DOCX
import and the normalized schema v1 model. It merges what were previously
sixteen separate numbered docs (32 and 38–53); those files were deleted and
their content preserved here, one section per construct. The tracker
(`14-EXECUTION-TRACKER.md`) references sections of this doc by anchor, e.g.
`38-SCHEMA-V1-DESIGN-REFERENCE.md#tables`.

Every section preserves the original model shape (types), key invariants,
deferred/reported items, and acceptance/implementation status with its tracker
id. Nothing was dropped in consolidation; prose was condensed.

**Merged sources (old number → section):** 32 → [Import architecture](#import-architecture);
38 → [Base schema v1](#base-schema-v1); 39 → [Tables](#tables); 40 → [Fields](#fields);
41 → [Text boxes](#text-boxes); 42 → [Footnotes and endnotes](#footnotes-and-endnotes);
43 → [Headers and footers](#headers-and-footers); 44 → [Legacy VML pictures](#vml-pictures);
45 → [Extra-part media and hyperlinks](#extra-part-media); 46 → [Ruby](#ruby);
47 → [Comments](#comments); 48 → [Tracked changes](#tracked-changes);
49 → [Run properties](#run-properties); 50 → [Paragraph properties](#paragraph-properties);
51 → [Table properties](#table-properties); 52 → [Bookmarks](#bookmarks);
53 → [Content controls](#content-controls).

## Shared design rules (all sections inherit)

- **Typed, first-class values** — no OOXML attribute bags in the model. OOXML
  element names, relationship ids, prefixes, and part paths are *provenance*,
  never model identity.
- **Additive + backward-compatible** — every extension is a new `Option`/`Vec`/
  variant with `skip_serializing_if`. Existing v0 and v1 snapshots load and
  serialize byte-identically; the v0→v1 migration and its byte-exact golden are
  unchanged (v0 carries none of these constructs).
- **Deterministic** — identical input + config → byte-identical snapshot; map
  keys serialize in lexical order; arrays preserve document order; ids are
  import-generated in canonical document order.
- **Strict on load** — unknown object fields rejected; `schema_version > 1`
  rejected; every invariant validated first-failure-wins with typed, text-free
  errors.
- **No silent data loss** — anything not modeled is dispositioned in the
  compatibility report (`35-DISPOSITION-TAXONOMY.md`) and, in Retention mode,
  preserved verbatim. Unmapped `apply_*_property` arms return `false`
  (→ reported); unmapped elements hit the generic `report(local)` arm.
- **Bounded** — every recursion (table/text-box/revision/sdt nesting) is
  depth-capped so adversarial JSON cannot exhaust the stack.

---

## Import architecture

*(was `32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md`)*
**Status:** Accepted (architecture-level) — 2026-07-24, ADR-027. **Tracker:** P1A-001.

### Outcome

Load an admitted DOCX package into the normalized model and emit an atomic,
deterministic import bundle: (1) a semantic JSON snapshot; (2) an immutable
bounded source-package snapshot; (3) source-to-model provenance; (4) a typed
preservation ledger; (5) a complete compatibility report carrying a
`model_outcome` and a `retention_outcome` (per `35-…`) for every
non-fully-mapped construct.

End-to-end path: `.docx bytes → bounded package reader → content types +
relationship graph → main document part → styles/themes/numbering/sections/media
references → paragraphs/runs/basic properties → mapping registry → normalized
model + provenance + preservation ledger → deterministic semantic JSON + source
snapshot + compatibility report`.

Phase 1A validates whether the normalized model can represent useful
WordprocessingML semantics before typography/pagination/rendering make model
changes expensive. It does not implement DOCX writing, but each accepted mapping
defines its reverse strategy and edit-invalidation scope before import code is
accepted.

**Evidence:** ECMA-376 (WordprocessingML, OPC, Markup Compatibility,
Transitional), ISO/IEC 29500-1:2016, Microsoft WordprocessingML + OPC docs.
Competitor study and fidelity boundaries: `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`,
`34-OOXML-FIDELITY-ARCHITECTURE.md`. The importer follows relationship *types*
and resolved targets, never conventional ZIP paths.

### Why v0 is insufficient

Schema v0 represents only document/paragraph identity, runs, four inline marks
(bold/italic/underline/strike), and an inert bounded extension map. It has no
first-class representation for paragraph/run property values, styles, numbering,
sections, themes, relationships, or media references. The v0 extension map is
not an OOXML round-trip mechanism (no typed source ownership, ordering,
provenance, edit invalidation, conflict handling, save disposition). The first
accepted Phase 1A slice therefore defines schema v1 with deterministic v0→v1
migration and strict v1 JSON validation.

### In scope / out of scope

**In scope:** package graph (content types, office-document + part-level
relationships, internal target resolution + normalized part names, external
target identification without fetch, missing/duplicate/cyclic/invalid reporting);
semantic parts (main body, paragraphs/runs, text/tabs/breaks, basic para/run
properties, paragraph+character style definitions and inheritance, document
defaults, theme color/font references, abstract numbering + instances + levels +
references, body-level section properties, media relationships without image
decoding); artifacts (schema v1 snapshot, bounded source snapshot,
source-to-model provenance, preservation-ledger schema, compatibility report
schema v1, versioned import/reverse-mapping registry, semantic golden fixtures,
stable errors/warnings, correctness + bounded-work benchmarks).

**Out of scope:** font resolution/shaping/bidi/line-breaking, layout/metrics,
pagination, display lists, native/WASM rendering, hit testing, UI/Tauri hosts,
DOCX writer, image decoding, external-resource fetch, complete
table/drawing/field/note/comment/tracked-change/embedded-object semantics.
Out-of-scope content must still be dispositioned in the report. Writer
implementation is Phase 2; reverse mapping / dirty scope / invalidation /
unsupported-save disposition are in scope as *design requirements* because
importer choices must not discard information a future writer needs.

### Import pipeline

1. Admit the package under `PackageLimits` (no XML read before admission).
2. Parse `[Content_Types].xml` + package relationships under XML/relationship
   limits.
3. Locate the main document via its relationship type.
4. Resolve the main document's relationships; classify internal/external targets.
5. Parse theme/styles/numbering before resolving effective references.
6. Process markup compatibility; stream the main document in source order as
   bounded decoder events.
7. Apply versioned mapping rules → semantic state, provenance, typed preservation
   entries, diagnostics.
8. Validate style/numbering/section/media references.
9. Normalize ids, property values, maps, source-order arrays.
10. Validate the v1 model + every retained source artifact.
11. Emit the bundle atomically.

No partially valid `DocumentSession` is returned. Inspection diagnostics may
accompany a failed import but cannot expose document text.

### Fidelity artifacts (follow `34-…`)

- **Source snapshot:** admitted parts, content types, relationships, hashes,
  retained safe bytes, explicit non-retention dispositions.
- **Provenance:** connects semantic owners/properties to source regions and
  mapping-rule versions via a *content-relative offset-span anchor* (source part
  + document-order block path + grapheme-offset span over normalized paragraph
  text + source `w:r` index), captured *before* run normalization — never a
  mutable model node id (ADR-027 D5).
- **Preservation ledger:** each entry has typed owner, anchor, source order,
  namespace context, byte accounting, invalidation scope, conflict policy, and
  planned save disposition.
- **Mapping registry:** source vocabulary, semantic target, unconsumed
  preservation, reverse mapping, dirty scope, security policy, fixtures, support
  state per feature.

### Compatibility report

Versioned independently from the schema. Every entry: stable code, severity,
dual-axis disposition (`model_outcome` ∈ mapped/degraded/omitted;
`retention_outcome` ∈ preserved/not-retained/blocked/rejected/not-applicable),
feature id, source part, structural location (no text), bounded occurrence
count, optional relationship/namespace id, remediation/support-phase reference.
Completeness = every admitted part and traversed unsupported
element/attribute/relationship/MC-branch has an explicit disposition on both
axes. Entries ordered by part, document order, code, feature id. No timestamps,
local paths, random values, or document text. A `preserved` outcome is valid
only when the report references a validated snapshot/ledger record; a warning
without retention is `not-retained`, not `preserved`.

### Determinism & security

For identical package bytes + options + engine version: stable relationship
traversal order, node/definition ids, lexical map key order, document-order
arrays, stable warning aggregation, stable source-snapshot manifests + retained
hashes; semantic JSON / provenance / ledger / compatibility JSON byte-identical
across native + WASM. ZIP entry order must not change equivalent results.

Security: DTDs/custom/external entities rejected; XML parsing namespace-aware,
streaming, depth-limited, cancellable; relationship targets resolved as OPC part
URIs under path-safety rules; external targets never fetched; every count has a
secure default + non-bypassable ceiling; image bytes referenced not decoded;
errors/reports omit text and host paths; a limit/structural failure creates no
session. (The XML parser dependency required a separate dependency ADR — ADR-011,
`quick-xml`.)

### Open decisions (resolved via ADR-027)

Exact v1 shape + migration API; Strict vs Transitional profile (D8: accept both,
normalize at decode); snapshot/provenance/ledger schemas + byte budgets;
mapping-registry format/ownership/versioning; MC branch selection + retention;
XML parser dependency; tables rejected vs preserved (R4: hybrid
flatten-then-preserve, later promoted to first-class — see [Tables](#tables));
`ImportBundle` + SDK inspection API; warning-code registry ownership. Each is
tracked to a chosen/pending option in `36-ADR-027-ACCEPTANCE-RECORD.md`.

---

## Base schema v1

*(was `38-NORMALIZED-SCHEMA-V1-DESIGN.md`)*
**Status:** Accepted 2026-07-24; implemented. **Tracker:** P1A-008.

### Versioned envelope

Every snapshot carries `schema_version` (`0` or `1`). A v0 document loads
unchanged; a v1 loader rejects `schema_version > 1`. v1 adds a `definitions`
section alongside `body`; v0 has none and migration synthesizes an empty one.

```text
Document {
  schema_version: 1,
  id: DocumentId,
  body: [BlockNode],          // ordered
  definitions: Definitions,   // new in v1
}
```

### Body: block and inline nodes

Block nodes (ordered in `body`): `Paragraph { id, properties: ParagraphProperties,
inlines: [InlineNode] }` (plus `Table` and `Sdt`, added by later sections).

Inline nodes (replacing v0's flat run list): `Run { id, properties: RunProperties,
text: String }` (grapheme sequence; offsets are grapheme indices); `Tab { id }`
(`w:tab`); `Break { id, kind: Line | Page | Column }` (`w:br`). Later sections
add `Drawing`, `Hyperlink`, `Field`, `TextBox`, `NoteReference`,
`CommentReference`, `Revision`, `BookmarkStart`/`BookmarkEnd`, `Sdt`.

Originally v1 kept a flat body (nested paragraphs from tables/text boxes/SDT
flattened per R4, geometry in the ledger). Later slices promote tables, text
boxes, and sdt to first-class recursive containers (see those sections).

### Property model

Typed structs of optional supported fields. Unsupported source properties are
not stored on the node — dispositioned in the report, and when retained recorded
in the ledger. Property *values* are enumerations or measured units, never raw
OOXML strings.

- `ParagraphProperties { style_ref: Option<StyleId>, numbering: Option<NumberingRef>,
  alignment: Option<Alignment>, indentation: Option<Indentation>, spacing:
  Option<Spacing>, … }`
- `RunProperties { style_ref: Option<StyleId>, bold, italic, underline, strike:
  Option<bool>, color: Option<ThemeColorRef | RgbColor>, size_half_points:
  Option<u32>, font_ref: Option<ThemeFontRef | FontName>, … }`

The v0 marks become the corresponding `RunProperties` booleans.

### Definitions

- **Styles** — `Style { id: StyleId, kind: Paragraph | Character, based_on:
  Option<StyleId>, paragraph: Option<ParagraphProperties>, run:
  Option<RunProperties> }` + document defaults. `based_on` forms an inheritance
  chain; **cycles are rejected** (typed error, dispositioned; cycle fixture
  required).
- **Numbering** — `AbstractNumbering { id, levels: [NumberingLevel] }` +
  `NumberingInstance { id, abstract_ref, overrides }`; paragraphs reference an
  instance + level.
- **Sections** — ordered `SectionBoundary` values capturing supported
  page/column metadata (size, margins, columns) without layout; body +
  per-paragraph `sectPr` normalize into one ordered sequence.
- **Theme references** — `ThemeColorRef`, `ThemeFontRef` retain semantic
  color/font intent without embedding the theme.
- **Media references** — `MediaReference { id, relationship_id, media_type,
  part_name }` identifying the package part without decoding bytes.

Referential integrity: every StyleId / numbering / media / section reference
must resolve within `definitions`; a dangling reference is a typed load error.

### Identity, determinism, migration

Node ids and definition ids are import-generated, stable, assigned in canonical
document order. The import namespace/documentId seed is input-derived and
independent of ZIP/relationship enumeration order (R3, open). Grapheme offsets
use the same segmentation as the transaction layer (native = WASM).

**v0 → v1 migration** (deterministic + total): `schema_version` 0→1; add empty
`definitions`; each v0 paragraph → v1 `Paragraph` with default properties; each
v0 run → v1 `Run` (marks → `RunProperties` booleans; text + ids preserved).
Lossless; golden vectors assert byte-identical output.

### Strict validation

On load v1 rejects: unknown object fields; `schema_version > 1`; duplicate ids;
zero/degenerate ids; dangling style/numbering/section/media references;
`based_on` cycles; grapheme offsets outside a run's text; any property value
outside its declared domain. Errors are typed and carry no document text.

### Open items

Exact unit encodings (twips vs EMUs vs points) per measured property; the
concrete `NumberingLevel` field set; whether document defaults are a distinct
definition or a synthetic style id; R3 seed derivation.

### Inline drawings + hyperlinks (first additive slices)

**Tracker:** P1A-021 (in review). The first constructs promoted beyond the
originally-flat v1 body, establishing the additive pattern every later section
follows. `InlineNode::Drawing { media: MediaId, extent: Option<Extent> }`
(embedded-picture only; `MAX_EMU` public) and `InlineNode::Hyperlink { target:
HyperlinkTarget, tooltip: Option<String>, inlines }` with `HyperlinkTarget ∈
External{url} | Internal{anchor}`. `ModelError::EmptyHyperlink`,
`NestedHyperlink`. `validate_body` → recursive `validate_inlines(in_hyperlink)`;
id-uniqueness + snapshot limits recurse into hyperlink children. Import builds
media before body → `MediaId` index; `w:drawing` → Drawing (embed-less/dangling/
degraded reported); `w:hyperlink` → Hyperlink via `push_segment` router +
`hyperlink_depth` nesting. Strictly additive (existing v1 byte-identical).

### Document properties (docProps core/app/custom)

**Tracker:** P1A-DOCPROPS (done). Closes the semantic metadata-loss gap: the flat
v1 `Document` had no properties field, so the import→edit→save path dropped
title/author/dates/company/counts/custom properties. Additive
`DocumentProperties { core: CoreProperties, app: AppProperties, custom:
[CustomProperty] }` hung off `Document` (private `Option`, attached via
`Document::with_properties`, read via `Document::properties()`); all-empty
metadata collapses to `None`.

- **Core** (`docProps/core.xml`, Dublin Core + `cp:`) — title, subject, creator,
  keywords, description, last_modified_by, revision, created, modified,
  last_printed, category, content_status, language, version. All `Option`.
- **App** (`docProps/app.xml`) — application, app_version (verbatim token),
  company, manager, template, total_time, pages, words, characters,
  characters_with_spaces, lines, paragraphs, doc_security, hyperlink_base,
  scale_crop, links_up_to_date, shared_doc, plus the `titles_of_parts` and
  `heading_pairs` (`vt:vector`) groups. Counts are integers; all `Option`/empty.
- **Custom** (`docProps/custom.xml`) — ordered `[CustomProperty { name, value }]`
  with `CustomValue ∈ Text | I4 | R8 | Bool | FileTime | Other{kind}`. The
  `fmtid`/`pid` bookkeeping is regenerated on write, not modeled.

**Dates and `r8`/`filetime` are stored verbatim as strings** so bytes round-trip
without a lossy parse and the model stays `Eq`. Validation bounds text/token
lengths and rejects negative counts (`ModelError::PropertyValueOutOfDomain`).
Import discovers the parts through the **package-root** relationships
(`/metadata/core-properties`, `/extended-properties`, `/custom-properties`) with
a well-known part-name fallback, bounded namespace-agnostic parse; unrecognized
leaf fields are reported. The semantic writer emits each non-empty group as a
`docProps/*` part with its `[Content_Types]` override and a root `_rels` entry,
omitting empty groups (matching producer behavior). Round-trip: semantic fixed
point (`import → model → write → reopen` = identical `DocumentProperties`) plus a
Retention byte-survival check on a real corpus file; `synthetic-rich-metadata.docx`
exercises every field. Strictly additive (existing v1 byte-identical).

---

## Tables

*(was `39-SCHEMA-V1-TABLES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-022 (in review),
first structural slice of P1A-019. **Supersedes** the base-v1 "tables flattened
(R4)" note.

Models table **structure and cell-merge geometry**. Styling (borders/shading/
alignment/row height/table-style ref/widths) is a later additive slice — see
[Table properties](#table-properties).

### Model

```text
BlockNode { Paragraph(Paragraph)  Table(Table) }   // Table new, tag "table"

Table    { id: NodeId, grid: [GridColumn], rows: [TableRow] }   // rows >= 1
GridColumn { width_twips: Option<i32> }            // w:gridCol@w (dxa); optional
TableRow { id: NodeId, cells: [TableCell] }        // cells >= 1
TableCell { id: NodeId, properties: TableCellProperties, blocks: [BlockNode] }  // blocks >= 1
TableCellProperties {
  grid_span: Option<u32>,                 // w:gridSpan (horizontal merge span)
  vertical_merge: Option<VerticalMerge>,  // w:vMerge
  width_twips: Option<i32>,               // w:tcW@w when @type is dxa
}
VerticalMerge { Restart, Continue }       // "restart" => Restart; else Continue
```

Cell content is recursive `[BlockNode]` (nested tables representable). A cell
always holds ≥1 block; import synthesizes an empty paragraph for a degenerate
empty cell.

**Cell-merge geometry:** horizontal = `grid_span` on the origin cell (absent =
1); vertical = `Restart` on the top cell (`w:vMerge w:val="restart"`), `Continue`
on continued cells. The model records OOXML roles faithfully; it does **not**
collapse merged cells (a layout concern). Grid/merge consistency (spans summing
to grid width) is **not** enforced — real producers emit locally-inconsistent
grids and the model must not reject what a word processor accepts.

### Constants / validation

`MAX_TABLE_DEPTH = 32` (root table = depth 1); `grid_span ∈ 1..=16384`;
`width_twips ∈ 0..=31_680` (page-geometry twip ceiling). One recursive walk
validates: non-empty rows/cells/blocks (`EmptyTable`/`EmptyTableRow`/
`EmptyTableCell`), depth ≤ bound (`TableNestingTooDeep`), domains
(`PropertyValueOutOfDomain{"table.cell.grid_span"|"table.cell.width"|
"table.grid.column.width"}`). Id-uniqueness and snapshot text/block limits
recurse through tables (table/row/cell ids join the global id set; nested nodes
count against `max_blocks`, text against scalar/run-byte ceilings). New
`ModelError`: `EmptyTable`, `EmptyTableRow`, `EmptyTableCell`,
`TableNestingTooDeep` (all `NodeId`).

### Import

`tables.rs` `TableStack` builder holds the open table/row/cell stack; the flat
body parser drives it. `w:tbl`/`w:tr`/`w:tc` open (ids allocated on the opening
tag, document order); a finished paragraph routes to the innermost open cell
else body (replacing the R4 flatten); `w:tblGrid`/`w:gridCol@w` populate the
grid; `w:tcPr` → `grid_span`/`vertical_merge`/`width_twips` (dxa or absent only;
pct/auto reported). Closing tags finalize (empty row/row-less table reported +
dropped). Every other table construct (`w:tblPr`, `w:trPr`, unmapped `w:tcPr`
children) hits the report arm — never silently dropped, Retention-preserved.
Over-`MAX_TABLE_DEPTH` nesting reported + transparent.

**Review-found defect (fixed):** an over-depth table refused by `open_table` left
`is_active()` true, so its subtree mutated the parent (silent corruption). Fixed
with a context-independent, always-balanced `suppressed_tbl_depth` counter
(every `<w:tbl>` opens-or-suppresses, every `</w:tbl>` balances); a residual
start/end-asymmetry desync on malformed `<w:tbl>`-inside-`<w:p>` was closed too.

### Round-trip / deferred

Retention unchanged (byte-exact). Fidelity harness recurses tables→cells→blocks.
**Deferred (reported + Retention-preserved):** table/row/cell styling (borders,
shading, vAlign, row height, header marker, table-style ref, `w:tblW`, cell
margins), `w:tblGrid` change tracking, grid/span consistency normalization — see
[Table properties](#table-properties).

---

## Fields

*(was `40-SCHEMA-V1-FIELDS-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-023 (in review).

Models a field's **instruction** + **cached-result inlines**. Does not evaluate
fields (a layout/runtime concern) or parse instruction grammar; the instruction
is retained opaque + bounded.

Two source forms → one node: **simple** `<w:fldSimple w:instr=" PAGE ">` wrapping
result runs; **complex** `fldChar` run sequence
(`begin` → `instrText`… → `separate` → result runs → `end`; no `separate` = empty
result).

### Model

```text
InlineNode { …, Field(Field) }   // tag "field"
Field {
  id: NodeId,
  instruction: String,        // opaque field code; non-empty, <= 4096 bytes
  inlines: Vec<InlineNode>,   // cached result (MAY be empty); leaf inlines only
}
```

**Wrapper nesting rule (bounds inline recursion):** `Hyperlink` and `Field` are
the two inline **wrappers**; a wrapper may contain only **leaf** inlines
(Run/Tab/Break/Drawing), never another wrapper. Max inline nesting stays at one
wrapper level. Consequently a hyperlink cannot contain a field and vice versa; a
HYPERLINK-instruction complex field is modeled as a `Field` (URL in instruction,
display runs as result), not converted to `Hyperlink` in this slice.

### Validation

`validate_inlines` gains `in_wrapper` (= `in_hyperlink || in_field`, replacing
the `in_hyperlink` flag). `Field` reject if `in_wrapper` (`NestedField`); check
`instruction` domain (`PropertyValueOutOfDomain{"field.instruction"}`); validate
`inlines` with `in_wrapper = true`; hard run-merge boundary; empty inlines
allowed. `Hyperlink` unchanged (`NestedHyperlink`). Id-uniqueness + limits
recurse into `Field.inlines`; instruction bytes bounded. New `ModelError`:
`NestedField(NodeId)`.

### Import

Field state machine reusing the segment pipeline. Simple: `<w:fldSimple>` opens
an accumulator seeded with `w:instr`; child runs become result; commit on close
(empty/oversize instruction → reported, runs flatten). Complex: `fldChar`
machine — `begin` opens (collecting instruction), `instrText` appends,
`separate` switches to collecting result, `end` commits; `fldChar` runs emit no
text; `begin` without `end` flushed at paragraph close; nested complex fields
bounded by a depth counter (inner flattens, reported). Routing reuses
`push_segment`: while a field is open, segments route into its result (including
pre-`separate` ones — preserves malformed content). A hyperlink inside a field
(and vice versa) reported + flattened. Orphaned `w:instrText` reported.

**Review-found defects (fixed):** (major) `push_segment` dropped display runs
arriving while a field collected its instruction — now captured (lossless);
(minor) `instrText` with no field collecting it dropped — now reported.

**Deferred (reported + Retention-preserved):** field evaluation; instruction
grammar; form fields (`w:ffData`), `w:fldLock`/`w:dirty`; HYPERLINK→`Hyperlink`
conversion; wrapper-in-wrapper as model structure.

---

## Text boxes

*(was `41-SCHEMA-V1-TEXTBOXES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-024 (in review);
fixes P1A-025 audit clusters 1–2.

A `w:txbxContent` (DrawingML `wps:txbx` or legacy VML `v:textbox`) holds block
content but the flat parser could not open its inner paragraph — the box's inner
`</w:p>` fired `finish_paragraph()` on the **enclosing** paragraph, truncating
it, mis-capturing boxed text, and resetting `drawing_depth` so the enclosing
drawing's **image was silently dropped** (a blocker, corrupting even ordinary
main-body documents). The audit also found `mc:AlternateContent` walked **both**
branches (duplicated content).

### Model

```text
InlineNode { …, TextBox(TextBox) }   // tag "text_box"
TextBox { id: NodeId, blocks: Vec<BlockNode> }   // non-empty; paragraphs + nested tables
```

`TextBox.blocks` reuses the recursive block model validated for table cells. A
text box may contain a table and a cell may contain a text box (bounded by table
depth + a new text-box bound). `MAX_TEXTBOX_DEPTH = 8`. A `TextBox` is a
block-container, not an inline wrapper, so `in_wrapper` does not forbid it; it
may appear inside a hyperlink/field run stream, its blocks validated
independently.

### Validation

`validate_inlines` `TextBox` arm: reject empty `blocks` (`EmptyTextBox`);
validate each block, incrementing a text-box-depth counter, reject past bound
(`TextBoxNestingTooDeep`); hard run-merge boundary. Id-uniqueness + limits
recurse. New `ModelError`: `EmptyTextBox`, `TextBoxNestingTooDeep`.

### Import

Block-sink stack via a suspended `ContentFrame` (save/restore ~28 content
fields). Entering `w:txbxContent` suspends the current paragraph/run context and
pushes a text-box sink; inner content builds normally into it (inner `</w:p>`
finishes the *inner* paragraph); leaving builds a `TextBox` inline (empty →
reported + dropped) routed via the segment router. Because the inner context is
suspended/restored, the enclosing drawing's `drawing_depth`/`blipFill`/
`pending_embed` are preserved — fixing the silent image drop.
`mc:AlternateContent`: descend only the **first** `mc:Choice`; `mc:Fallback` and
later choices skipped + reported (`mc_skip_depth`) — neither duplicated nor lost.

**Review-found defects (fixed):** (major) the importer gave each box a fresh
table stack but the model threaded the enclosing table depth into box blocks →
deep tables + a box of tables aborted the whole import; the model now restarts
the table budget per box. (minor) a `w:sectPr` inside a box pushed a phantom
section; now guarded to true body level.

**Deferred (reported + Retention-preserved):** shape geometry/anchoring/wrapping/
fill; linked text boxes (`wps:linkedTxbx`); VML→`Drawing` conversion; extra-part
parsing; `w:ruby` ordering. Accepted minor: text-box id allocated eagerly
(consistent with tables).

### Current anchored-frame extension (2026-07-27)

The later floating-object slice extended `TextBox` with optional
`anchor`/`extent`/`relative_height`/`fill`/`border`, and introduced the shared
`DrawingAnchor` used by floating pictures, text boxes, and groups. The float
reflow increment adds:

```text
WrapDistances {
  top_emu, bottom_emu, start_emu, end_emu: i64
}
DrawingAnchor {
  horizontal, vertical, wrap,
  wrap_distances: WrapDistances,  // default zero; omitted from JSON when all zero
  behind_doc
}
```

Each distance is non-negative and bounded by `MAX_EMU`. Semantic DOCX import and
export preserve `wp:anchor@distT/distB/distL/distR` for DrawingML anchored
pictures, text boxes, and groups. This is additive schema-v1 data with a default,
so existing snapshots remain valid. Layout currently consumes `distB` only for
the paragraph/line-relative `wrapTopAndBottom` flow barrier; `distT/distL/distR`
remain preserved for the later side/cross-paragraph reflow slices.

### Current text-body box-model extension (2026-07-27)

`TextBox` and `GroupTextBox` now carry a defaulted
`TextBoxBodyProperties { insets, vertical_anchor, horizontal_overflow,
vertical_overflow, auto_fit }`. Insets are signed `ST_Coordinate32` EMU values
with DrawingML's asymmetric defaults (91,440 left/right; 45,720 top/bottom).
Autofit distinguishes no-autofit, shape growth, and normal-autofit's bounded
`font_scale`/`line_spacing_reduction` percentages. The percentage domain is
validated, while the `i32` inset representation provides the coordinate bound by
construction.

The importer attaches `wps:bodyPr` after the suspended `w:txbxContent` frame
restores its open shape builder; semantic export emits the attributes and
mutually-exclusive `a:*AutoFit` child. Layout resolves one content origin and
clip policy for inline and anchored/grouped boxes, so body, nested cell,
header/footer, and group rendering cannot diverge. Exact vertical ellipsis,
rotation/vertical writing, `anchorCtr`, and VML-specific `inset` remain outside
this bounded extension.

---

## Footnotes and endnotes

*(was `42-SCHEMA-V1-NOTES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-026 (in review);
closes the P1A-025 note-part silent-drop + the last LibreOffice-visible text gap.

`word/footnotes.xml` / `word/endnotes.xml` were never read in Semantic mode: all
note body text/images/tables/text-boxes silently absent; the in-body
`w:footnoteReference`/`w:endnoteReference` reported by name only with its `w:id`
dropped (footnotes fixture scored 93%).

### Model

```text
Definitions { …, footnotes: DefinitionMap<NoteId, Note>, endnotes: DefinitionMap<NoteId, Note> }  // empty-omitted
Note { blocks: Vec<BlockNode> }   // may be empty
InlineNode { …, NoteReference(NoteReference) }
NoteReference { id: NodeId, kind: NoteKind, note: NoteId }
NoteKind { Footnote, Endnote }
```

A note's `blocks` reuse the recursive block model (paragraphs, tables, text
boxes, images). `NoteId` is a new definition id. A `NoteReference` is a leaf
inline + hard run-merge boundary.

### Validation

Every `NoteReference.note` resolves in the map named by its `kind`
(`footnotes`/`endnotes`), else `DanglingNoteRef(NodeId)`. Each note's blocks
validated recursively with a **fresh** table/text-box depth budget. Id-uniqueness
+ snapshot limits include every `NoteId` and note-internal ids/text. A note's
blocks may be empty (OOXML separator notes skipped at import, but the model does
not forbid an empty note).

### Import

`import_package` resolves `/footnotes` + `/endnotes` and parses each with the
body parser in **note-container mode**: `w:footnote`/`w:endnote` are block
containers keyed by `w:id`, returning `(w:id, blocks)`; separator/continuation
notes (`w:type` present and not `normal`) skipped. Each note part resolves its
**own** relationships (`DocxPackage::part_relationships`) so note-internal images/
hyperlinks are modeled. `w:id` strings → deterministic `NoteId`; maps built
before the body so a reference resolves (dangling → reported + dropped). Fidelity
harness appends each note's block text in id order (strips footnote auto-number
markers).

**Review-found defects (fixed):** (major) `close_note` finished the paragraph
before unwinding text-box frames, dropping an outer paragraph a box restored —
now unwinds frames first (mirrored in body `parse`); (minor) a stray `<w:body>`
and a body-level `w:sectPr` inside a notes part modeled-then-discarded — now
gated to body mode + reported.

**Deferred (reported + Retention-preserved):** note **properties**
(`w:footnotePr`: numbering format, restart, position); the rendered
auto-numbered mark; comments/tracked changes inside notes; headers/footers.

---

## Headers and footers

*(was `43-SCHEMA-V1-HEADERS-FOOTERS-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-027 (in review);
closes the P1A-025 header/footer silent-drop.

### Model

```text
Definitions { …, headers: DefinitionMap<HeaderFooterId, HeaderFooter>, footers: DefinitionMap<…> }  // empty-omitted
HeaderFooter { blocks: Vec<BlockNode> }   // may be empty
SectionBoundary { …, headers: Vec<HeaderFooterRef>, footers: Vec<HeaderFooterRef> }  // additive; empty-omitted
HeaderFooterRef { kind: HeaderFooterKind, reference: HeaderFooterId }
HeaderFooterKind { Default, First, Even }
```

`HeaderFooterId` is a new definition id; a section references at most one
header/footer of each kind.

### Validation

Each `HeaderFooterRef.reference` resolves in the map named by its position (a
`SectionBoundary.headers` entry → `headers` map, etc.), else
`DanglingHeaderFooterRef(NodeId)`. Blocks validated recursively (fresh depth,
like notes). Id-uniqueness + limits include every `HeaderFooterId` and internal
ids/text. Blocks may be empty.

### Import

`import_package` resolves each `/header` + `/footer` part (keyed by `r:id`),
parsed in **single-container mode** (`w:hdr`/`w:ftr` root = document+body),
returning one block list → a deterministic `HeaderFooterId`; an `r:id →
HeaderFooterId` index is built for headers and footers. Body `w:sectPr`
`w:headerReference`/`w:footerReference` (`r:id`, `w:type`) resolve → add a
`HeaderFooterRef` (kind from `w:type`: first/even, else default); unresolved
`r:id` reported. **Id order:** document → styles → numbering → media → footnotes
→ endnotes → headers → footers → body. LibreOffice txt export ignores
header/footer text, so correctness is covered by unit tests, not the fidelity
proxy.

**Review-found defect (fixed):** the note-mode `w:body`/`w:sectPr`
no-silent-loss guards weren't extended to header/footer (`hf_root`) mode, so a
stray `w:body`/`w:sectPr` in a header/footer was silently consumed (phantom
section burned an id) — fixed by also requiring `hf_root.is_none()`.

**Deferred (reported + Retention-preserved):** header/footer-part media +
external hyperlinks (shared follow-up — now closed by
[Extra-part media](#extra-part-media)); `w:titlePg`/even-odd settings that select
which header applies; VML images.

---

## VML pictures

*(was `44-SCHEMA-V1-VML-IMAGES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-028 (in review);
closes the P1A-025 VML-image reported-but-dropped finding.

Only DrawingML pictures (`w:drawing` → `a:blip@r:embed`) were modeled; a legacy
VML picture (`w:pict` → `v:imagedata@r:id`) had its image reference reported but
dropped. **No model change** — a VML picture reuses `Drawing { id, media:
MediaId, extent: None }` (VML sizes shapes in CSS, not captured, so `extent` is
always `None`).

### Import

`w:pict` (inside a run) opens a picture context (`pict_depth`); `v:imagedata@r:id`
resolves through the **same** media index as DrawingML; on `</w:pict>` a
resolvable id → `Drawing` segment, else (unresolved id, or an image-less shape =
a VML text box, handled by the text-box slice) reported. `pict_depth` + pending
id saved/restored across text-box frames. A VML picture in a header/footer/note
part resolves against that part's media index.

**Review-found defect (fixed):** `finish_paragraph` reset drawing counters but
not `pict_depth` (a latent leak, unreachable under `check_end_names` but a
data-loss trap once extra-part media lands) — one-line reset added.

**Deferred (reported):** VML shape geometry, CSS sizing, wrap/anchor, OLE
objects (`w:object`), VML fills/strokes.

---

## Extra-part media

*(was `45-EXTRA-PART-MEDIA-AND-HYPERLINKS-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-029 (in review);
closes the P1A-025 extra-part image/link silent-drop. **No model change** —
reuses `MediaReference`, `Drawing`, `Hyperlink`.

Notes/headers/footers were modeled, but images and external hyperlinks *inside*
them resolved against an empty index. Those relationships live in each part's own
`_rels` (`part_relationships`).

### Import

The media table is built into **one shared table** across the whole package
(main → footnotes → endnotes → headers → footers). `media::build_into` allocates
**one `MediaId` per relationship** (no dedup), so a main-document-only file's
media table + id sequence are byte-identical to before, and returns that part's
`relationship_id → MediaId` index. Relationship ids are **per-part**: each part's
parser receives its own media index + hyperlink map (a header's `rId1` and the
main document's `rId1` resolve independently); `import_package` builds a
`PartSources { xml, images, hyperlinks }` per extra part via `part_relationships`.
`MediaReference.relationship_id` records the first source that introduced an image
part (main preferred, aggregated first). Unresolved image/hyperlink relationships
reported. Internal hyperlinks (`w:anchor`) need no relationship.

**Review-found defect (fixed):** the initial design de-duped images by part,
which also collapsed a main-doc-only file's two-refs-to-one-image case (shifting
ids). Fixed by one `MediaId` per relationship — main-doc behavior byte-identical.

**Deferred:** VML images inside extra parts (now depend on that part's populated
index); OLE objects; linked (non-embedded) images.

---

## Ruby

*(was `46-RUBY-PHONETIC-GUIDES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-030 (in review);
closes the P1A-025 ruby reorder/merge finding. **No model change** — import-only.

A `w:ruby` (East-Asian phonetic guide) had both its annotation (`w:rt`) and base
(`w:rubyBase`) captured in raw document order — the annotation appears *before*
the base, reordering/merging the reading text with the pronunciation guide.

### Import

`ruby_annotation_depth` counter: entering `w:rt` increments; while > 0 a run's
`w:t` text is **not** emitted (annotation dropped); leaving `w:rt` reports `rt`
(dispositioned, not silently lost) and decrements. `w:rubyBase` runs captured
normally, so the base reads in document order at the ruby's position. The counter
is saved/restored across text-box frames and reset at paragraph close, so a
malformed unclosed `w:rt` cannot suppress later text.
`w:ruby`/`w:rubyPr`/`w:rubyBase` reported.

**Review-found defect (fixed):** a bool flag let a nested `w:rt` (valid OOXML)
clear the outer annotation early, re-merging a fragment — fixed by making it a
depth counter.

**Deferred:** modeling the annotation text (a `Ruby { base, annotation }` node)
and ruby alignment/positioning.

---

## Comments

*(was `47-SCHEMA-V1-COMMENTS-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-031 (in review);
closes the P1A-025 comment-part silent-drop. Reuses the note-part machinery
wholesale.

`word/comments.xml` was never read: comment body text/images/tables/text-boxes
absent; in-body `w:commentReference` reported by name with `w:id` dropped;
author/date/initials lost.

### Model

```text
Definitions { …, comments: DefinitionMap<CommentId, Comment> }   // empty-omitted
Comment {
  blocks: Vec<BlockNode>,          // may be empty
  author: Option<String>,          // <= 255 bytes, non-empty
  initials: Option<String>,        // <= 255 bytes, non-empty
  date: Option<String>,            // <= 64 bytes, non-empty (ISO-8601 as written)
}
InlineNode { …, CommentReference(CommentReference) }
CommentReference { id: NodeId, comment: CommentId }
```

Blocks reuse the recursive block model. `CommentId` is a new definition id;
`CommentReference` is a leaf inline + hard run-merge boundary. Author/date/
initials retained as written — opaque, bounded, never parsed/reformatted (date
kept as its original string so no timezone/precision is lost).

### Validation

Every `CommentReference.comment` resolves in `Definitions::comments`, else
`DanglingCommentRef(NodeId)`. Blocks validated recursively (fresh depth).
`author`/`initials` non-empty ≤255 (`comment.metadata`); `date` non-empty ≤64
(`comment.date`). Id-uniqueness + limits recurse. Blocks may be empty.

### Import

`import_package` resolves the main `/comments` relationship — **only** the
canonical type (`commentsExtended`/`commentsIds`/`commentsExtensible` end with
different suffixes, not matched) — via `resolve_part_sources` so comment-part
images/hyperlinks resolve. `parse_comments` runs the body parser in
note-container mode (`note_container == Some(b"comment")`): each `w:comment` a
block container keyed by `w:id` → a `CommentId` in document order; `open_note`
reads `w:author`/`w:date`/`w:initials` into `CommentMeta` (dropped, not
truncated, when empty/oversized); `close_note` unwinds text-box frames before
finishing the paragraph. In-body `w:commentReference` (in a run) →
`CommentReference` (dangling reported). **Id order:** document → styles →
numbering → main media → footnotes → endnotes → headers → footers → comments →
body.

**Review-found defect (fixed):** a table left open by EOF-truncated markup in a
comment was stranded in the shared `TableStack` and dropped (and could have bled
into the next comment absent the XML layer's `MalformedXml` rejection); fixed with
`TableStack::flush_open` at every container boundary (`close_note`, main `parse`,
`parse_header_footer`) + a `suppressed_tbl_depth` reset — closing the pre-existing
main-body/notes EOF gap too.

**Deferred (reported, not modeled):** `w:commentRangeStart`/`End` (the anchored
range — the reference-to-definition link is modeled; the span is a follow-up; no
content lost, range runs are normal body content); `commentsExtended`/`Ids`/
`Extensible` parts (threading, durable ids, resolved state); a
`w:commentReference` **inside** a footnote/endnote/header/footer part (reported
not modeled — those parts are parsed before the comment index exists, same
established pattern as note-references in secondary parts; the comment body is
still fully modeled, only the in-part anchor link dropped + reported).

---

## Tracked changes

*(was `48-SCHEMA-V1-TRACKED-CHANGES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented. **Tracker:** P1A-032 (in review),
final slice of P1A-019.

`w:ins`/`w:del`/`w:delText` were reported by name only, metadata lost, and
`w:delText` not routed like `w:t` (deleted text could be dropped).

### Model — a wrapper `InlineNode`, not a `Run` property

```rust
pub const MAX_REVISION_DEPTH: u32 = 8;
pub enum RevisionKind { Insertion, Deletion }   // w:ins / w:del
pub struct Revision {
    pub id: NodeId,                    // this inline's own id (document order)
    pub kind: RevisionKind,
    pub author: Option<String>,        // <= 255 bytes, non-empty
    pub date: Option<String>,          // <= 64 bytes, ISO-8601 as written
    pub revision_id: Option<String>,   // w:id as written, <= 64 bytes; opaque,
                                       // non-unique grouping key (NOT a NodeId)
    pub inlines: Vec<InlineNode>,      // non-empty; may hold a nested Revision
}
InlineNode { …, Revision(Revision) }   // new internally-tagged variant
```

**Why a wrapper, not a `Run` flag:** a `w:ins`/`w:del` wraps a contiguous *range*
(runs, hyperlinks, drawings, tabs, breaks) under a single (author, date, id)
triple; a per-run flag would duplicate metadata and cannot express "this whole
range including its hyperlink was inserted", nor compose for nested
inserted-then-deleted. A wrapper is a hard merge boundary (adjacent-run
normalization unaffected). `w:delText` is captured into `Run.text` like `w:t`;
the enclosing `Revision{Deletion}` marks it deleted — no new `Run` field, no
loss. **No new id newtype and no `Definitions` field** (serialization
byte-identical): the producer's `w:id` is the opaque `revision_id`, not a
resolvable `NodeId` (tracked-change ids are producer-local grouping keys;
multiple `w:ins` legitimately share one).

### Validation

`validate_inlines` threads `revision_depth`. Revision arm: empty →
`EmptyRevision`; past `MAX_REVISION_DEPTH` → `RevisionNestingTooDeep`; metadata
domains (author/`revision_id` ≤255 `revision.metadata`, date ≤64
`revision.date`). Transparent to `in_wrapper` (may wrap a hyperlink/field and sit
inside one). Resets adjacent-run tracking. Id-uniqueness + limits recurse;
`revision_depth` restarts at 0 per block and inside a text box. New `ModelError`:
`EmptyRevision`, `RevisionNestingTooDeep`.

### Import — unified innermost-wins wrapper stack

Lives in the main body (and notes/headers/footers/comments, which reuse
`BodyParser`) — no new part. The two mutually-exclusive singleton wrappers
(`hyperlink`, `field`) are **replaced by one innermost-wins stack** so a revision
nests with a hyperlink in **both** directions (revision-in-link and
link-in-revision) and within itself (`ins>del`):

```rust
enum OpenWrapper { Hyperlink(HyperlinkAccumulator), Field(FieldAccumulator), Revision(RevisionAccumulator) }
// on BodyParser and ContentFrame: wrappers: Vec<OpenWrapper>  (+ field_depth kept)
```

`push_segment` routes to `wrappers.last_mut()` else the paragraph. Same-kind
nesting still reported+flattened by the open guard; on close each wrapper pops and
re-emits its `Segment` via `push_segment` (landing in the enclosing wrapper or
paragraph). `on_start` `b"ins"|b"del"` requires `paragraph_open && !run_open &&
ppr_depth == 0 && rpr_depth == 0` — the guard routes **only** run-range
revisions; a `w:ins`/`w:del` in `w:pPr>w:rPr` (paragraph-mark) or `w:r>w:rPr`
(property marker) falls through to report arms. `w:delText` gets `w:t`-parallel
arms → `Run.text` (a stray `w:delText` outside `w:del` still a normal run).
`finish_paragraph`/`exit_textbox`/`close_note` drain wrappers innermost-first.

**Review-found defects (fixed):** (major, one root cause) the `on_end`
`w:ins`/`w:del` arm had no close-side counter, so an *excluded* inner range's
close (a property-context `w:rPr>w:ins` marker, or an over-depth range)
prematurely committed the *enclosing* real revision — desyncing the stack and
misplacing/dropping tracked content. Fixed with a `suppressed_revision_depth`
close-side counter (mirroring `field_depth`/`hyperlink_depth`/
`suppressed_tbl_depth`): `on_start` unconditional (open a real range or
report-and-count), `on_end` balances before any commit. (minor) the model
validated `revision_id` at ≤255 but the contract bound is ≤64 — fixed to ≤64.

**Deferred (reported, not modeled):** paragraph-mark insertion/deletion
(`w:pPr>w:rPr>w:ins`/`del`); property-change revisions (`w:rPrChange`,
`w:pPrChange`, `w:tblPrChange`, `w:tcPrChange`, `w:trPrChange`, `w:sectPrChange`,
`w:numberingChange`); move revisions (`w:moveFrom`/`w:moveTo` + range markers);
custom-XML and cell revisions (`w:customXmlIns/DelRangeStart/End`, `w:cellIns`,
`w:cellDel`, `w:cellMerge`).

---

## Run properties

*(was `49-SCHEMA-V1-RUN-PROPERTIES-DESIGN.md`)*
**Status:** Accepted 2026-07-25; implemented (this slice's scope). **Tracker:**
P1A-033 (in review); prioritized by the P1A-0AA coverage audit (rFonts #1,
highlight #4, vertAlign #5, caps #8).

`RunProperties` mapped only bold/italic/underline/strike/color/size. A key
finding: `font_ref` (and the `FontRef`/`FontName`/`ThemeFont`/`ThemeFontRef`
types) already existed and were validated, but `apply_run_property` never
populated them, so `w:rFonts` was reported-only despite the field existing.

### Scope (shipped: A + D + B + typed underline)

- **A — toggle marks** (`w:caps`, `w:smallCaps`, `w:vanish`, `w:webHidden`,
  `w:dstrike`): additive `Option<bool>` fields `all_caps`, `small_caps`,
  `hidden`, `web_hidden`, `double_strike`, via the `is_true` toggle helper.
- **D — fonts** (`w:rFonts`): populate `font_ref` as the `ascii` slot + add
  `font_ref_h_ansi`/`font_ref_cs`/`font_ref_east_asia`. Each slot prefers its
  `*Theme` attribute (`major*`→`ThemeFontRef::Major`, `minor*`→`Minor`) then its
  named attribute (bounded 255). Consumed only when a slot resolves; an `rFonts`
  carrying only unmodeled detail (e.g. just `@hint`) resolves nothing and is
  reported — **no silent loss**.
- **B — named vocabularies** (`w:vertAlign`, `w:highlight`, `w:em`): closed enums
  `VerticalAlignment`, `HighlightColor`, `EmphasisMark`; unknown `@val` reported.
- **Typed underline** (`w:u@val`/`@color`, tracker P1F-38): the on/off bit
  remains `underline: Option<bool>` while `underline_style:
  Option<UnderlineStyle>` carries the closed single/double/thick/dotted/dashed/
  dot-dash/wavy/words vocabulary and `underline_color: Option<RgbColor>` carries
  an independent sRGB color. `None` is canonical single-line/automatic color;
  unrecognized producer styles degrade visibly to single and are reported.
  Import/export, cascade, layout, paint, editor commands, selection/caret mixed
  state, armed typing, and the internal rich clipboard all consume these fields.
  Editor set/clear uses nested delta options so "leave unchanged" cannot be
  confused with "clear direct style/color". Suggesting mode rejects typed
  underline edits until tracked-format authoring can carry the fields.

### Validation / compat

Each font slot bounded like `font_ref` (`run.font_ref.name`, non-empty ≤255);
`check_run_property_refs` iterates all four. Enums type-safe. Every new field
`Option<_>` + `skip_serializing_if = "Option::is_none"` (no `#[serde(default)]` —
serde auto-defaults missing Option); default `RunProperties` serializes to `{}`.
`migration.rs` `run_properties_from_marks` ends with `..RunProperties::default()`
so adding fields cannot break its total-struct literal; migration golden
unchanged.

**Deferred (design captured, lower priority):** C — typographic metrics
(`w:spacing` char / `w:kern` / `w:position`); E — language (`w:lang`).
**Stays reported-only (no silent loss):** text effects
(`w:outline`/`emboss`/`imprint`/`shadow`/`effect`), color theme tint/shade,
`w:rtl`, `w:bdr`, `w:fitText`.

---

## Paragraph properties

*(was `50-SCHEMA-V1-PARAGRAPH-PROPERTIES-DESIGN.md`)*
**Status:** Designed 2026-07-25 (coverage workflow; adversarially reviewed,
sound-with-fixes). **Wave 1 implemented** (tracker P1A-034); waves 2+ pending.
Full multi-slice design below; fold review fixes at implementation.

`ParagraphProperties` modeled only `style_ref`, `numbering`, `alignment`,
`indentation`, `spacing`; everything else in `w:pPr` reaches the report arm
(`_ if self.ppr_depth > 0 => report`). All additions are `Option`/`Vec` +
`#[serde(default, skip_serializing_if …)]` (empty still serializes to `{}`,
migration golden byte-identical); bounded values validated via `check_domain(…,
"paragraph.…")` reusing `PropertyValueOutOfDomain` (**no new `ModelError`
variant**); producer-specific token vocabularies (border art styles, shading
patterns) retained as bounded strings, structurally load-bearing vocabularies
(tab alignment/leader, vertical text alignment) typed closed enums; unmapped →
`apply_paragraph_property` returns `false` and is reported.

**Implementation status:** Wave 1 (P1A-034) shipped the toggle flags `keepNext`/
`keepLines`/`pageBreakBefore`/`widowControl`/`contextualSpacing`/
`suppressLineNumbers` + `outlineLvl` (0..=9), mapped through the shared
`apply_paragraph_property` so they work in **both** the body and styles pPr state
machines. Waves 2+ (shading, borders, tabs) pending. **Review fixes to fold at
impl:** reuse the Border/Shading value type from [Table properties](#table-properties);
the `styles.rs` **second** pPr state machine must also map new props; cap `tabs`
at import to match validation; filter `w:pos`.

### Slicing

| Slice | Constructs | Import surface |
|---|---|---|
| **A — Flow control & levels** | `keepNext`, `keepLines`, `pageBreakBefore`, `widowControl`, `suppressLineNumbers`, `contextualSpacing`, `bidi`, `outlineLvl`, `textAlignment` | `properties.rs` only — flat elements |
| **B — Paragraph shading** | `w:shd` | `properties.rs` only; introduces reusable `Shading` |
| **C — Paragraph borders** | `w:pBdr` (top/bottom/left/right/between/bar) | container → new `pbdr_depth` in `body.rs`; reusable `BorderSide` |
| **D — Custom tab stops** | `w:tabs` → repeated `w:tab` | container → new `tabs_depth` in `body.rs` |

Slices C/D introduce **container** elements (children carry data), so they add a
depth counter to the flat quick-xml state machine, mirroring `numPr`/`numpr_depth`.
`Shading` (B) and `BorderSide` (C) are producer-neutral so the table-property and
run-property tails reuse them.

### Slice A — model

`TextAlignment { Auto, Baseline, Bottom, Center, Top }` (`w:textAlignment`,
ST_TextAlignment). Fields appended to `ParagraphProperties` (all `Option`,
`skip_serializing_if`): `keep_next`, `keep_lines`, `page_break_before`,
`widow_control`, `suppress_line_numbers`, `contextual_spacing`, `bidi` (all
`Option<bool>`, `CT_OnOff`: absent `w:val`→true, `0`/`false`/`off`→false);
`outline_level: Option<u8>` (`w:outlineLvl`, `0..=9` → `"paragraph.outline_level"`);
`text_alignment: Option<TextAlignment>` (unknown token → report). Import adds
`apply_paragraph_property` arms (reuse `is_true`; bind `let value =
attribute_value(element, b"val")` at the top); out-of-domain/unparseable
`outlineLvl`/`textAlignment` → `return false`. Validation: `check_domain(level <=
9, "paragraph.outline_level")`.

### Slice B — paragraph shading (`w:shd`)

Flat element `<w:shd w:val="clear" w:color="auto" w:fill="D9D9D9"/>`. Fill color
modeled regardless of pattern; the ST_Shd pattern token (~40 values) retained.

```rust
pub struct Shading {              // Clone-only (Other(String) forbids Copy)
    pub pattern: ShadingPattern,  // <= 32 bytes when Other
    pub fill: Option<RgbColor>,   // w:fill; auto/absent omitted
    pub color: Option<RgbColor>,  // w:color; auto/absent omitted
}
pub enum ShadingPattern { Clear, Solid, Other(String) }  // Other serializes {"other":"pct25"}
```

Field `shading: Option<Shading>`. Import: build from `val`/`fill`/`color`
(`parse_rgb` returns `None` for `auto`/malformed); a no-op shd (Clear, no colors)
→ `return false` (reported not modeled). Validation: `Other` token non-empty ≤32
(`paragraph.shading.pattern`).

### Slice C — paragraph borders (`w:pBdr`)

Container; each side is a `CT_Border` child.

```rust
pub struct BorderSide {
    pub style: String,               // ST_Border token as written, <= 32 bytes
    pub size_eighths: Option<u16>,   // w:sz, eighths of a point, 0..=1020
    pub space_points: Option<u16>,   // w:space, points, 0..=31
    pub color: Option<RgbColor>,     // w:color; auto/absent omitted
}
pub struct ParagraphBorders { top, bottom, start, end, between, bar: Option<BorderSide> }
```

Field `borders: Option<ParagraphBorders>`. Sides map `left→start`, `right→end`
(transitional/strict alias). Import adds `pbdr_depth: u32` (BodyParser +
ContentFrame save/restore). The `pbdr_depth > 0` guard is **load-bearing**:
`top`/`bottom`/`left`/`right` also name cell-margin/border children (under
`tcPr`/`tblPr`), so scoping by `pbdr_depth` prevents cross-family misrouting;
these arms must sit **above** the generic `_ if self.ppr_depth > 0` arm. A side
with no usable `w:val` dropped + reported. Validation bounds style ≤32
(`paragraph.border.style`), size ≤1020 (`.size`), space ≤31 (`.space`).

### Slice D — custom tab stops (`w:tabs`)

Container of repeated `w:tab` (`CT_TabStop`).

```rust
pub enum TabAlignment { Start, Center, End, Decimal, Bar, Clear }   // ST_TabJc
pub enum TabLeader { None, Dot, Hyphen, Underscore, Heavy, MiddleDot }  // ST_TabTlc
pub struct TabStop { alignment: TabAlignment, position_twips: i32 /* -31_680..=31_680 */, leader: Option<TabLeader> }
```

Field `tabs: Vec<TabStop>` (`skip_serializing_if = "Vec::is_empty"`, ≤64,
document order). `val="left"→Start`, `right→End`; `num` (legacy list tab) /
unknown → skip-and-report that stop. Import adds `tabs_depth: u32`.
**Disambiguation hazard:** `w:tab` is overloaded — a *tab character* (in a run →
`InlineNode::Tab`, under `run_open`) vs a *tab stop* (under `pPr>tabs`, run
closed); contexts are disjoint (`tabs_depth > 0` vs `run_open`) and the arm
ordering enforces it. Validation bounds count ≤64 (`paragraph.tabs.count`) and
position (`paragraph.tab.position`).

**Deferred (reported, not modeled):** paragraph-mark run properties (`w:pPr>w:rPr`
— needs a paragraph-node field); `w:pPr>w:sectPr`; frames (`w:framePr`);
East-Asian/typographic toggle tail (`w:snapToGrid`, `w:wordWrap`,
`w:overflowPunct`, `w:topLinePunct`, `w:autoSpaceDE`/`DN`, `w:kinsoku`,
`w:suppressAutoHyphens`, `w:mirrorIndents`, `w:adjustRightInd`,
`w:suppressOverlap`); `w:cnfStyle`, `w:divId`; `w:pPrChange`; `w:tab val="num"`.

**Owner-flagged notes:** `ShadingPattern::Other(String)` forces `Shading` to
`Clone`-only (not `Copy`) — model `pattern` as a bounded `String` if a future
table tail wants `Copy`. Border `size_eighths` bound `0..=1020` is generous
(Word UI caps ~6pt); tighten to `0..=255` if the corpus shows nothing larger.

---

## Table properties

*(was `51-SCHEMA-V1-TABLE-PROPERTIES-DESIGN.md`)*
**Status:** Designed 2026-07-25 (coverage workflow; adversarially reviewed,
sound-with-fixes). **Wave 1 implemented** (tracker P1A-035, attribute-based);
wave 2 (borders + margins, `P1A-035b`) pending. The audit's #2/#3 priority. Full
multi-slice design below.

`w:tblPr` / `w:trPr` and the `w:tcPr` long tail were the last table constructs
reported-not-modeled: `Table`/`TableRow` carried **no properties field**, and
`TableCellProperties` mapped only `gridSpan`/`vMerge`/`tcW`.

**Implementation status:** Wave 1 (P1A-035) shipped the **attribute-based**
properties — no child-element capture, avoiding the border-container collision
hazard: `TableProperties{alignment, width_twips, layout, look, shading}` on
`Table`, `TableRowProperties{height, cant_split, header}` on `TableRow`, extended
`TableCellProperties` with `shading`/`vertical_alignment`/`no_wrap`/
`text_direction`. New parser states `tblpr_depth`/`trpr_depth` (saved/restored
across text-box frames, reset defensively on `tr`/`tc` open). Note: the shipped
cell vAlign enum is named `CellVerticalAlignment` (to avoid the run
`VerticalAlignment`); the design below names it `VerticalAlignment`. **Wave 2
(`P1A-035b`, pending):** borders (`tblBorders`/`tcBorders`) + margins
(`tblCellMar`/`tcMar`) — the nested edge-child capture.

### Attachment points

| Level | OOXML | Model target | Shape |
|---|---|---|---|
| Table | `w:tbl > w:tblPr` | new `TableProperties` on `Table` | `skip_serializing_if = "TableProperties::is_empty"` |
| Row | `w:tr > w:trPr` | new `TableRowProperties` on `TableRow` | `skip_serializing_if = "TableRowProperties::is_empty"` |
| Cell | `w:tc > w:tcPr` | extend `TableCellProperties` | new `Option<…>`, `skip_serializing_if` |

`TableCell.properties` stays always-serialized (`{}` when empty); `Table`/
`TableRow` gain a **skipped** properties field (empty = omitted → existing
snapshots byte-identical), same shape as `Table.grid`.

### Shared value types (Slice A, reused by Slice C)

```rust
pub struct BorderEdge {          // w:top/start/bottom/end/insideH/insideV
    pub style: String,           // ST_Border token, lowercased, 1..=32 bytes (opaque tail)
    pub size_eighth_points: Option<u32>,  // w:sz, 0..=1024
    pub color: Option<RgbColor>,          // w:color explicit sRGB only; auto reported
    pub space_points: Option<u32>,        // w:space, 0..=31
}
pub struct TableBorders { top, start, bottom, end, inside_h, inside_v: Option<BorderEdge> }
pub struct Shading { fill: Option<RgbColor> }  // w:shd; only background fill; non-clear/nil pattern or non-auto color ALSO reported
pub struct CellMargins { top_twips, start_twips, bottom_twips, end_twips: Option<i32> }  // w:tblCellMar/w:tcMar, dxa 0..=31_680; non-dxa reported
```

Each has an `is_empty()` for `skip_serializing_if`. `RgbColor` reused from
`properties.rs`.

### Slice A — table properties (`w:tblPr`)

```rust
pub enum TableLayout { Fixed, Autofit }   // w:tblLayout/@type
pub struct TableLook { first_row, last_row, first_column, last_column, no_h_band, no_v_band: bool }  // all-false omitted
pub struct TableProperties {
    alignment: Option<Alignment>,   // w:jc; start/center/end (justify reported)
    width_twips: Option<i32>,       // w:tblW dxa, 0..=31_680
    layout: Option<TableLayout>,
    look: TableLook,                // skip if empty
    borders: TableBorders,          // skip if empty
    shading: Shading,               // skip if empty
    cell_margins: CellMargins,      // w:tblCellMar; skip if empty
}
// Table gains: properties: TableProperties (after grid, before rows; skip if empty)
```

Import (`body.rs` + `tables.rs`): `tblpr_depth` guarded by `tables.is_active() &&
suppressed_tbl_depth == 0 && <no open row>`; property arms call `TableStack`
setters writing `self.stack.last_mut().properties`. `jc` → `alignment_from`
(else report); `tblW` dxa clamped (pct/auto report, mirroring `tcW`);
`tblLayout` fixed/autofit; `tblLook` explicit attrs or legacy hex mask; `shd`
fill via `parse_rgb` (non-clear/nil `@val` or non-auto `@color` also report);
`tblCellMar` child edges (dxa). Border-set capture: `border_scope:
Option<BorderTarget>` set on `tblBorders`/`tcBorders`, each edge child builds a
`BorderEdge` (empty `@val` → dropped + reported), committed on container `on_end`.
Validation domains: `table.width`, `table.borders.{size,style,space}`,
`table.cell_margins` (shared `check_borders`/`check_margins` helpers, prefix per
level).

### Slice B — row properties (`w:trPr`)

```rust
pub enum HeightRule { Auto, AtLeast, Exact }   // w:trHeight/@hRule
pub struct RowHeight { value_twips: Option<u32> /* 0..=31_680 */, rule: Option<HeightRule> }
pub struct TableRowProperties { height: RowHeight, cant_split: bool, header: bool }
// TableRow gains: properties: TableRowProperties (after id, before cells; skip if empty)
```

Import `trpr_depth` (guarded row-open, no-cell-open): `trHeight` → value + rule;
`cantSplit`/`tblHeader` via `is_true`. Validation: `table.row.height`.

### Slice C — cell property long tail (`w:tcPr`)

```rust
pub enum VerticalAlignment { Top, Center, Bottom }        // w:vAlign (shipped as CellVerticalAlignment)
pub enum TextDirection { LrTb, TbRl, BtLr }               // w:textDirection
// TableCellProperties gains (beyond existing grid_span/vertical_merge/width_twips):
//   borders: TableBorders, shading: Shading, margins: CellMargins,
//   vertical_alignment: Option<VerticalAlignment>, no_wrap: bool, text_direction: Option<TextDirection>
```

Adding `TableBorders`/`CellMargins` (which own a `String` via `BorderEdge.style`)
**drops `Copy`** from `TableCellProperties` (Clone/Eq/etc. remain; verified no
`Copy` dependency in `tables.rs`). Import extends the `tcpr_depth > 0` arms:
`tcBorders` (shared capture, `BorderTarget::Cell`), `shd`, `tcMar`, `vAlign`
(top/center/bottom else report), `noWrap` (`is_true`), `textDirection`
(lrTb/tbRl/btLr; legacy `tbLrV`/`lrTbV`/`tbRlV` reported); diagonal
`tl2br`/`tr2bl` reported.

**Slice sequencing:** A lands first (introduces shared types + border-set
capture); B and C independent after A. Recommended A → B → C.

**Deferred (reported, not modeled):** `w:tblPrEx` (per-row exceptions);
table-style conditional formatting (`w:tblStylePr`, `w:cnfStyle`); diagonal cell
borders; `w:shd` pattern + pattern color (only fill modeled); theme fills/colors
(`themeFill*`/`themeColor`); `w:tblInd`, `w:tblOverlap`, `w:bidiVisual`,
`w:tblCaption`/`Description`, `w:tblpPr` (floating positioning), `w:hidden`/
`w:fitText` cell flags, row `w:jc`/`w:wBefore`/`w:wAfter`/`w:gridBefore`/
`w:gridAfter`/`w:divId`; property-change revisions on tables.

---

## Bookmarks

*(was `52-SCHEMA-V1-BOOKMARKS-DESIGN.md`)*
**Status:** Implemented — 2026-07-25 (coverage workflow; adversarially reviewed,
review fixes folded). **Tracker:** P1A-036 (done). First of the "range markup"
family (bookmarks → comment ranges → move ranges).

`w:bookmarkStart{w:id,w:name}` / `w:bookmarkEnd{w:id}` fell through to the report
arm: the name, position, and any internal-hyperlink anchor target were lost.

### Model — definition + paired range markers

```rust
id_newtype!(BookmarkId);                    // ids.rs
pub struct Bookmark { pub name: String }    // definitions.rs; non-empty, <= 255 bytes
// Definitions gains: bookmarks: DefinitionMap<BookmarkId, Bookmark>  (empty-omitted)
pub struct BookmarkStart { pub id: NodeId, pub bookmark: BookmarkId }  // w:bookmarkStart
pub struct BookmarkEnd   { pub id: NodeId, pub bookmark: BookmarkId }  // w:bookmarkEnd
InlineNode { …, BookmarkStart(BookmarkStart), BookmarkEnd(BookmarkEnd) }
```

**Why definition + independent markers (not a wrapper, not name-on-marker):**
(1) bookmarks *overlap* and are not well-nested — a wrapper requires strict XML
nesting and cannot represent them, so markers are paired independent zero-width
points and the "range" is the span between two markers sharing a `BookmarkId`;
(2) bookmarks span block boundaries — independent leaf markers each carrying the
`BookmarkId` pair by shared id with no same-paragraph requirement; (3) the name
is declared once (on the start) — storing it on a keyed *definition* gives one
authoritative storage + domain-check site, follows the `comments`/`notes`/`media`
pattern, and leaves a hook for a future name→bookmark index. Ids per bookmark:
one `NodeId` for the `BookmarkId` (allocated when the start tag is first seen, so
it exists before a later end can reference it) + one per marker = 3 distinct ids.

### Anchor resolution — two references, opposite treatment

- **Marker → definition** (`BookmarkStart/End.bookmark` → `Definitions::bookmarks`):
  **STRICT.** An importer-controlled invariant (the importer always inserts the
  definition when allocating the `BookmarkId`), mirroring `DanglingCommentRef`/
  `DanglingNoteRef`. Unresolved → `DanglingBookmarkRef(NodeId)`.
- **Internal-hyperlink `anchor`** (a bookmark *name*) → **LAX.** No fatal error,
  not even a soft report this slice. Justification: forward references (target
  often follows the link); cross-part targets (anchor in body → bookmark in a
  header/note, parsed in separate passes); well-known anchors (`_top`, `_GoBack`,
  TOC/heading auto-bookmarks) need no matching start; and correctness/no-reject of
  valid documents outranks strictness (AGENTS.md). A name→bookmark reverse index
  (enabling an optional deferred non-fatal dangling-anchor report) is a recorded
  future enhancement.

### Validation

`validate_bookmarks`: each name non-empty ≤255 (`bookmark.name`).
`validate_unique_ids`: each `bookmarks` key `node_id()` joins the id set.
`validate_inlines`: two arms resolve `marker.bookmark` in `definitions.bookmarks`
(else `DanglingBookmarkRef`) and reset `previous_run_properties = None` (a hard
merge boundary — two equal-property runs separated by a marker must **not** merge
or the marker position is destroyed). Markers are transparent to `in_wrapper`,
`textbox_depth`, `revision_depth` (inert leaves like `Tab`). New `ModelError`:
`DanglingBookmarkRef(NodeId)`.

### Import — self-closing markers, source-`w:id` pairing

Lives in the main body and every `BodyParser`-reusing part (notes, headers,
footers, comments) — no new part. New state: `bookmark_ids: BTreeMap<String,
BookmarkId>` (part-scoped, **not** swapped in `ContentFrame`, so a bookmark
opened in body flow and closed inside a text box still pairs). A `&mut
DefinitionMap<BookmarkId, Bookmark>` accumulator is threaded into every part
parser (the `media::build_into` pattern) so bookmarks from all parts land in one
`Definitions::bookmarks`. Markers are self-closing → quick-xml `Event::Empty`
(dispatched `on_start` then `on_end`); handled entirely in `on_start` (unique
local names → their `on_end` falls to `_ => {}`); **no depth/suppression counter
needed** (no XML content between open and close).

**Balancing guarantee (why no reference ever dangles):** a `BookmarkId` enters
the map + id-registry **only** when a start is fully modeled inline; an end is
modeled **only** when its id already resolves. A missing/oversized name, a
duplicate `w:id`, an orphan end, or a block-level marker is reported + dropped
(and a block-level start never registers its id). Therefore every modeled
marker's `bookmark` resolves — `DanglingBookmarkRef` can never fire on importer
output (like `DanglingCommentRef`). `bookmark_ids` is cleared between reused-parser
notes/comments in `close_note`.

**Review fixes folded:** split match arms (distinct start/end types); column
bookmarks reported; fidelity-tool exhaustive `InlineNode` match extended; owned
`&mut DefinitionMap` accumulator; self-closing Empty-event `w:id` balancing via
the source-id→`BookmarkId` map.

**Deferred (reported, never silently dropped):** block-level markers (between
paragraphs, `!paragraph_open`) — follow-up: attach to the adjacent paragraph's
leading/trailing inlines; column bookmarks (`w:colFirst`/`colLast`) — attributes
ignored, bookmark still modeled by name/range; strict internal-hyperlink anchor
resolution (name→bookmark index + deferred report); other range markup
(`commentRangeStart/End`, `moveFrom/ToRangeStart/End`, `customXml*RangeStart/End`)
— separate slices reusing this marker-pair pattern.

---

## Content controls

*(was `53-SCHEMA-V1-CONTENT-CONTROLS-DESIGN.md`)*
**Status:** Designed 2026-07-25 (coverage workflow; adversarially reviewed,
sound-with-fixes). **Ready, pending implementation.** **Tracker:** P1A-037.

A `w:sdt` (structured document tag) wraps content with a stable identity
(`w:tag`), friendly name (`w:alias`), numeric id (`w:id`), and editing behaviour
(`w:richText`/`w:text`/`w:dropDownList`/`w:date`/`w:checkbox`/…). Today the
wrapper is reported-not-modeled: inner paragraphs/runs still parse (no text lost)
but the control's identity/name/tag/type are dropped. A `w:sdt` is
context-polymorphic: `w:sdtContent` holds **block** content (paragraphs/tables),
**inline** content (runs), or, rarely, whole **rows**/**cells**. This slice
models the block and inline forms; the two structural forms are deferred
(reported).

### Model — a wrapper `BlockNode` and a wrapper `InlineNode`, shared props

```rust
pub const MAX_SDT_DEPTH: u32 = 8;
pub enum SdtControlKind {  // w:sdtPr type marker; None = no marker (rich text default) or unmapped (also reported)
    RichText, PlainText, ComboBox, DropDownList, Date, Picture, Checkbox, Group,
    BuildingBlockGallery /* docPartObj/docPartList */, RepeatingSection, Citation, Bibliography,
}
pub struct SdtProperties {   // empty serializes {}
    control_kind: Option<SdtControlKind>,
    alias: Option<String>,       // w:alias, non-empty <= 255
    tag: Option<String>,         // w:tag, non-empty <= 255
    control_id: Option<String>,  // w:id as written, <= 64; opaque, non-unique grouping key (NOT a NodeId)
}
pub struct BlockSdt  { id: NodeId, properties: SdtProperties, blocks: Vec<BlockNode> }   // non-empty
pub struct InlineSdt { id: NodeId, properties: SdtProperties, inlines: Vec<InlineNode> } // non-empty
BlockNode  { …, Sdt(BlockSdt) }    // tag "sdt"
InlineNode { …, Sdt(InlineSdt) }   // tag "sdt"
```

The block/inline split is mandatory (each enum recurses into its own node type);
the shared `SdtProperties` matches the "typed properties, empty is `{}`" pattern
and is always present (like `Run.properties`). `w:id` is opaque (the
`revision_id` decision) — producer-local, may repeat, so not a `NodeId`. No new
id newtype, no `Definitions` field (content, not a cross-referenced definition).
A content control is a *transparent* wrapper (like `Revision`) — it may wrap
leaf inlines, a hyperlink/field, or a nested inline sdt, and may itself sit
inside one.

### Validation

Both walkers thread `sdt_depth: u32`. `validate_block` `Sdt` arm: over-bound →
`SdtNestingTooDeep`; empty → `EmptySdt`; `check_sdt_properties`; recurse with
`sdt_depth + 1` (a block sdt does **not** restart table/text-box budgets — a
transparent wrapper; but see the HARD review fix). `validate_inlines` `Sdt` arm
(modeled on `Revision`, transparent to `in_wrapper`): same bounds, recurse with
`sdt_depth + 1` and `previous_run_properties = None`. `check_sdt_properties`:
alias/tag non-empty ≤255 (`sdt.alias`/`sdt.tag`), `control_id` non-empty ≤64
(`sdt.id`). The `TextBox` arm passes `sdt_depth` through (does not reset).
Id-uniqueness + limits recurse. New `ModelError`: `EmptySdt(NodeId)`,
`SdtNestingTooDeep(NodeId)`. Every existing `validate_block`/`validate_inlines`
call site passes initial `sdt_depth = 0`.

### Import — flat state machine

Lives in the main body and `BodyParser`-reusing parts + text boxes
(`ContentFrame`) — no new part. Two well-worn mechanisms reused: **inline sdt = a
fourth wrapper on `wrapper_order`** (like `Revision`); **block sdt = a
`ContentFrame`** (like a text box, generalized to also emit a block). New state:
`sdts: Vec<SdtAccumulator>` (open inline controls; suspended in frame),
`WrapperKind::Sdt`, `sdt_scopes: Vec<SdtScope>` (`Inline`/`Block`/`Passthrough`),
`pending_block_sdt_props: Vec<SdtProperties>`, `sdt_prop_depth: u32` (inside
`w:sdtPr`/`w:sdtEndPr`), `sdt_depth: u32` (**path** counter for `MAX_SDT_DEPTH`,
**not** suspended, matching the model), `ContentFrame.kind: FrameKind`
(`TextBox`/`BlockSdt`) + `sdt_properties`. `Segment` gains `Sdt { properties,
children }`.

**Context detection at `<w:sdt>`:** **Inline** when `paragraph_open && !run_open
&& sdt_prop_depth == 0 && drawing_depth == 0 && pict_depth == 0`; **Block** when
`in_body && !paragraph_open && !run_open && ppr_depth == 0 && rpr_depth == 0 &&
sdt_prop_depth == 0` and not in a table's structural gap (no table active, or a
cell open via `TableStack::in_cell()`); **Passthrough** (reported, transparent —
inner content still parses) for every other position (inside a run;
row/cell-structural; over `MAX_SDT_DEPTH`). Only Inline/Block bump `sdt_depth`.

Property markers routed (only while `sdt_prop_depth > 0`) to the innermost open
sdt's slot: `alias`/`tag`/`id`; the type markers → `control_kind`; **any other
element while `sdt_prop_depth > 0`** (lock, placeholder, dataBinding, listItem,
`w:sdtEndPr` `w:rPr`, …) → `report(local)`, placed **before** the generic
`rPr`/`pPr`/block arms so a `w:sdtPr` `w:rPr` can never leak into flow.
`b"sdtContent"` (block) → `enter_sdt_block` (allocate id, move pending props into
the frame, suspend context like `enter_textbox`); `on_end` builds
`BlockNode::Sdt` from the frame's blocks routed via `self.tables.push_block`
(into the enclosing cell or body); empty/over-depth reported + dropped. Inline
sdt commits on `</w:sdt>` like a hyperlink; `commit_top_wrapper` drains one left
open by malformed input.

**Review fixes (HARD, fold at impl):** block-sdt `validate_block` must restart
the table budget (like text boxes) or deep tables abort; text-box `frames.len()`
depth must not conflate with sdt frames; an unclosed inline sdt must drain via
`wrapper_order` without cross-paragraph desync; `enter_sdt_block` must
`unwrap_or_default()` not `.expect()` (no panic on malformed markup);
`w:docPartObj`/`w:docPartList` collapsing to one kind must still report the lost
distinction.

**Deferred (reported, not modeled):** the `w:sdtPr` long tail (`w:lock`,
placeholder/`showingPlcHdr`, `dataBinding`, `sdtEndPr` run props, and per-type
detail — `listItem` entries, `w:date` format/calendar, `w14:checkbox` state/
glyphs, gallery/category, `repeatingSection` items); row-/cell-structural
content controls (an sdt wrapping `w:tr`/`w:tc` — needs the `Table` model to
carry a row/cell wrapper).
