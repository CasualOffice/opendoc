# Normalized Schema v1: Tracked Changes (Revisions) Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model;
100% of a round-trip must be convertible to the model)
**Tracker:** P1A-032 — final slice of P1A-019 (schema v1 semantic extension)
**Decision basis:** ADR-027, schema v1 (`38-…`), fields (`40-…`), text boxes
(`41-…`), comments (`47-…`, the closest analog), importer no-skip audit (`P1A-025`)

## Why

Tracked changes (`w:ins`, `w:del`, `w:delText`) are the last un-modeled construct
family in the semantic-import audit. Today an inserted/deleted run range is
reported by element name only, its author/date/id metadata is lost, and — the
worst gap — `w:delText` (deleted text) is not routed like `w:t`, so deleted text
can be dropped. This slice models an inserted (`w:ins`) or deleted (`w:del`) run
range as a first-class inline wrapper carrying its metadata, preserves `w:delText`
verbatim, and reports (never silently drops) the paragraph-mark, property-change,
and move revisions that this slice does not yet model.

## Model — a wrapper `InlineNode`, not a `Run` property

A revision is modeled as a wrapper `InlineNode`, analogous to `Hyperlink` and
`Field` (`crates/casual-doc-model/src/v1/body.rs`), not a flag on `Run`:

- **Range semantics.** `w:ins`/`w:del` wrap one or more runs (and hyperlinks,
  drawings, tabs, breaks) as a contiguous range with a single (author, date, id)
  triple. A per-run flag would duplicate the metadata and cannot express "this
  whole range, including its hyperlink, was inserted".
- **Nesting.** A `w:ins` can contain a `w:del` (inserted-then-deleted). A wrapper
  composes recursively; a run flag cannot carry two revision identities.
- **Merge boundary.** A wrapper is a hard boundary in `validate_inlines`, so
  adjacent-run normalization is unaffected — two equal-property runs on opposite
  sides of an insertion are correctly not merged.
- **Deleted text is ordinary run text.** `w:delText` is captured into `Run.text`
  like `w:t`; the enclosing `Revision{kind: Deletion}` marks it deleted. No new
  `Run` field, no data loss.

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

InlineNode { … , Revision(Revision) }  // new internally-tagged variant
```

**No new id newtype and no `Definitions` field.** A revision is not a
cross-referenced definition, so `ids.rs` and `definitions.rs` are untouched. The
producer's `w:id` is retained as the opaque bounded `revision_id`, *not* promoted
to a resolvable `NodeId` — tracked-change ids are producer-local grouping keys
(multiple `w:ins` legitimately share one `w:id`) and would break `DuplicateNodeId`
uniqueness if treated as identities.

## Strict validation (additive)

`validate_inlines` threads a `revision_depth: u32` (like `textbox_depth`) and gains
a `Revision` arm:

- **Empty range** → `EmptyRevision(NodeId)` (parallel to `EmptyHyperlink`).
- **Nesting bound** → `RevisionNestingTooDeep(NodeId)` at `MAX_REVISION_DEPTH`
  (parallel to `TextBoxNestingTooDeep`); admits `ins>del` (depth 2) comfortably.
- **Metadata domains** → author/`revision_id` ≤ 255 (`revision.metadata`), date
  ≤ 64 (`revision.date`), reusing `PropertyValueOutOfDomain` (same bounds as
  comments).
- A revision is a transparent range marker: it neither imposes nor clears the
  wrapper leaf-only rule (`in_wrapper` passes through unchanged), so it is allowed
  inside a hyperlink/field and may itself contain one at top level.
- Resets adjacent-run tracking (`previous_run_properties = None`).
- Unique-id (`record_inline_ids`) and snapshot-limit (`accumulate_inline_limits`)
  accounting recurse into `inlines` (mirroring `Hyperlink`), so nested ids and
  text are counted. `revision_depth` restarts at 0 per block and inside a text box.

New `ModelError` variants (additive, appended): `EmptyRevision(NodeId)`,
`RevisionNestingTooDeep(NodeId)`.

## Import — unified innermost-wins wrapper stack

Tracked changes live in the main body (and inside notes/headers/footers/comments,
which reuse the same `BodyParser`), so **no new part** — `lib.rs` and `ParseInputs`
are unchanged.

The parser today holds two mutually-exclusive singleton wrappers, `hyperlink:
Option<…>` and `field: Option<…>`, routed field→hyperlink→paragraph in
`push_segment`. A revision nests with a hyperlink in **both** directions:
- **Revision inside hyperlink** (`<w:hyperlink>…<w:ins><w:r>…` — inserting text
  into a link).
- **Hyperlink inside revision** (`<w:ins><w:hyperlink>…` — inserting a whole link;
  common when Word tracks an added link).

A revision-only stack checked before the hyperlink singleton models the first but
silently degrades the second (the link's runs route to the revision, the hyperlink
accumulator empties, the wrapper is dropped). To keep both directions lossless,
**replace the two singletons with one innermost-wins stack**:

```rust
enum OpenWrapper { Hyperlink(HyperlinkAccumulator), Field(FieldAccumulator), Revision(RevisionAccumulator) }
// on BodyParser and ContentFrame: wrappers: Vec<OpenWrapper>  (+ field_depth kept)
```

`push_segment` routes to `self.wrappers.last_mut()` else the paragraph. Existing
invariants are preserved by the open guards, not by singleton exclusivity:
- A hyperlink/field still never nests in another of the same kind — the open guard
  reports + flattens a same-kind wrapper exactly as today.
- A field's `fldChar` lifetime and `field_depth` balancing are unchanged.
- On close, each wrapper pops and re-emits its `Segment` via `push_segment`, so it
  lands in the enclosing wrapper (an outer revision) or the paragraph — replacing
  the one spot (`hyperlink` close) that pushes directly to `self.segments`.

`ContentFrame` save/restore swaps its `hyperlink`/`hyperlink_depth`/`field` fields
for the single `wrappers` Vec (plus `field_depth`).

### Open/close and `w:delText`

- `on_start`: `b"ins" | b"del"` with `paragraph_open && !run_open && ppr_depth == 0
  && rpr_depth == 0` → `open_revision`. The `ppr_depth == 0 && rpr_depth == 0`
  guard is load-bearing: it routes **only** run-range revisions. A `w:ins`/`w:del`
  inside `w:pPr>w:rPr` (paragraph-mark) or `w:r>w:rPr` (property marker) fails the
  guard and falls through to the existing property report arms (never silent).
- `open_revision` pushes `OpenWrapper::Revision`, filtering author ≤255 / date ≤64
  / `w:id` ≤64 exactly like comment attributes. Over-`MAX_REVISION_DEPTH`: report
  the element and push **no** accumulator, so children flatten into the encloser
  (runs preserved, not dropped).
- `on_end`: pop the revision; empty children → report + drop; else
  `push_segment(Segment::Revision{…})`.
- **`w:delText`** gets `on_start`/`on_end` arms identical to `w:t`, producing a
  `Segment::Run` that routes into the open `Revision{Deletion}` — deleted text is
  preserved in `Run.text`. The `Event::Text`/`CData` dispatch already keys on
  `in_text`, so no reader change. A stray `w:delText` outside `w:del` still yields
  a normal run (never dropped).
- `segment_to_inline`: allocate the wrapper id first (document order), then build
  children (mirroring `Hyperlink`).
- `finish_paragraph`/`exit_textbox`/`close_note` drain `self.wrappers` bottom-safe,
  so an unclosed `w:ins` at paragraph end still commits its runs.

### Silent-data-loss risks and mitigations

| Risk | Mitigation |
|---|---|
| `w:delText` dropped | `w:t`-parallel arm captures it into `Run.text` |
| Nested `ins>del` | stack entry; inner closes first, routes into outer; depth-bounded |
| Revision around drawing/hyperlink/text box | wrapped segment routes into the accumulator; model permits any inline inside a revision |
| Hyperlink/field opened inside a revision (and vice-versa) | innermost-wins stack keeps each wrapper's children in the correct container |
| Paragraph-mark / property-change / move revisions mis-modeled | `ppr_depth==0 && rpr_depth==0` guard excludes them → report arms |
| Empty / unclosed range | reported-and-dropped / flushed at paragraph close |

## Backward-compat

Strictly additive. `InlineNode::Revision` is a new internally-tagged variant; no
existing variant's bytes change and existing snapshots never contain it. No
`Definitions` field and no new id newtype, so `definitions.rs`/`ids.rs` serialize
byte-identically. The v0→v1 migration and its byte-exact golden are unchanged (v0
has no revisions). All `Revision` metadata is `skip_serializing_if`.

## Explicitly deferred (reported, not modeled)

Each reaches an existing report arm (property arms or the final
`_ if self.in_document => report`), so it surfaces in the compatibility report —
no silent drop. Modeling is a follow-up slice.

- **Paragraph-mark insertion/deletion** (`w:pPr>w:rPr>w:ins`/`w:del`) — paragraph
  state, not a run range; needs a paragraph-node field.
- **Property-change revisions** (`w:rPrChange`, `w:pPrChange`, `w:tblPrChange`,
  `w:tcPrChange`, `w:trPrChange`, `w:sectPrChange`, `w:numberingChange`) — carry a
  "before" formatting snapshot.
- **Move revisions** (`w:moveFrom`/`w:moveTo` + their range markers) — need
  cross-range pairing (like comment ranges).
- **Custom-XML and cell revisions** (`w:customXmlIns/DelRangeStart/End`,
  `w:cellIns`, `w:cellDel`, `w:cellMerge`).

## Test plan

- **Model:** insertion/deletion round-trip (metadata retained; deleted text lives
  in `Run.text`); empty revision rejected; nested `ins>del` within bound accepted;
  over-depth rejected; oversized author/date rejected; revision ids unique;
  revision wrapping a drawing round-trips. Extend the `any_inline` test walker.
- **Import:** inserted run modeled with metadata; `w:delText` preserved; nested
  `ins>del` modeled; revision around a drawing/text box; revision inside a
  hyperlink **and** hyperlink inside a revision (both preserve both wrappers —
  the innermost-wins regression guard); paragraph-mark/property-change/move
  revisions reported not modeled; empty range dropped+reported; oversized metadata
  dropped; unclosed revision flushes runs. Extend the fidelity walker
  (`tools/opendoc-fidelity/src/main.rs`) and the export presence walker with a
  `Revision` arm recursing into `inlines`.

All gates (fmt, clippy, unit, doctest, wasm, MSRV 1.85, doc) as in prior slices.

## CHANGELOG (Unreleased → Added)

> Tracked changes in schema v1: inserted (`w:ins`) and deleted (`w:del`) run
> ranges are modeled as an additive `InlineNode::Revision` wrapper carrying kind
> (insertion/deletion) plus retained author/date/id metadata and wrapping their
> content inlines; deleted text (`w:delText`) is preserved verbatim in the run
> text. Paragraph-mark, property-change, and move revisions remain reported (not
> yet modeled). Additive: existing snapshots and the v0→v1 migration golden are
> byte-identical.
