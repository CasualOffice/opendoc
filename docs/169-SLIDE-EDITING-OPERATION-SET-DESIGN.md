# 169 — Slide editing: the operation set, the envelope it shares, and the order it is built in

**Status:** Proposed — for the owner's review before any editing code is written.
**Opened:** 2026-10-09. **Decision:** ADR-067 (proposed). **Tracker:** `109` PRES-02.
**Programme:** `168` lane D1. **Inputs:** ADR-005, ADR-033, ADR-043, ADR-055, ADR-056,
ADR-057, `107` §4 (the editing budgets), `156` §3.3 and §8.

A deck can be opened, rendered, read aloud and saved, but nothing can change it. This document
decides **how a change to a deck is expressed**: which operations exist, how they are
addressed, what they share with the document editor's operations, and in what order they are
built. Implementation is a separate programme and starts only once the owner accepts ADR-067.

The rule `168` was opened to enforce applies here too. **Name what is consumed before
building anything.** Below, every part of the design either reuses something the document
editor already has, by name, or says why it cannot.

## 1. What the tree has today

All of this was re-read from source on 2026-10-09, not taken from earlier documents. Earlier
documents disagree with the source in two places (§1.4).

### 1.1 The transaction crate is wired to one document type

- `casual-doc-transaction` declares no operations of its own. It re-exports
  `casual_doc_edit::Operation` (`lib.rs:87-89`), and its own comment says why: "a second enum
  over the same document is the defect doc 147 exists to remove".
- `RevisionLog::apply` takes `&mut casual_doc_model::v1::Document` (`lib.rs:75`, `:848`).
- The following are all typed over `v1::Document` or `Operation` too:
  - `BlockIndex::of` (`transform.rs:368`);
  - `wire.rs`'s `document_space`, `localise` and `held_by`;
  - `ClientSession::joined` and `receive`;
  - `combine`;
  - `access::refuse_if_not_permitted` and `protection::refuse_if_protected`.
- **Nothing in the crate is generic over the document class.**

### 1.2 The operation set is closed, and its matches are exhaustive

- **Size.** `casual_doc_edit::Operation` has the number of variants that
  `crates/casual-doc-edit/tests/operation_count.rs` derives. That test is why no count is
  written here.
- **Addressing.** Every operation addresses one of:
  - a `Pos { node: NodeId, offset: u32 }`, a paragraph id plus a UTF-8 byte offset
    (`casual-doc-edit/src/lib.rs:277`);
  - a `Range` within one paragraph;
  - a node id;
  - a sibling index;
  - a document-scope registry key.
- **Exhaustive matches, no wildcard.** Eight matches in the transform classify every
  operation: `tier`, `variant_name`, `coordinates`, `set_coordinates`, `anchors`,
  `anchor_key`, `footprint` and `effect_of` (`transform/classify.rs:1-14`). Three more sit
  outside it:
  - `WireOperation::introduces`;
  - `access::admitted_by`;
  - the protection exemption.
- **What this buys.** Adding an operation is a compile error in every one of those places,
  which is `107` exit gate 2.

### 1.3 What the slide model already gives an editor

- **One id space.** Every slide node is a `casual_doc_model::NodeId`, in the same space as
  the document model's:
  - paragraph, run, break, field, shape tree, drawing, table, row and cell;
  - slide, layout and master, as typed newtypes over it.
- **Shape identity.** A slide shape's id is its drawing's id (`SlideNode::id`,
  `shape_tree.rs:229-238`).
- **Z-order.** Child index is both paint order and z-order (`shape_tree.rs:243-246`).
- **Text uses the document editor's anchor space already.**
  - Laid-out glyph clusters index the paragraph's `NodeId` plus a UTF-8 byte offset into its
    plain text (`casual-pres-layout/src/text.rs:233-248`).
  - `TextRun::text()` counts an `a:br` as `"\n"` and a field as its cached text, mirroring
    the DOCX `inline_text_len`.
  - So `Pos`, `Range`, `Affinity`, `MappingStep` and `PositionMap` carry over unchanged.
- **Validation.** `Presentation::validate` is O(deck) and documented "never per keystroke"
  (`presentation.rs:170-173`). `ShapeTree::validate`, `TextBody::validate` and
  `TextParagraph::validate` are the local checks.

### 1.4 What is missing or in the way

- **Nothing in the tree is a slide operation.** There is no `SlideOperation`, no
  `casual-pres-edit`, and no dependency of `casual-pres-wasm` on either transaction crate.
- **`Presentation::slides_mut()` and `definitions_mut()` are public.** Only tests call them
  today. Left public, they are the hole ADR-005 was written to close.
- **Text inside a grouped shape is not in the model at all.** `168` lane G is closing this.
- **The authored `p:cNvPr@id` is not kept** (`casual-pres-import/src/ids.rs`).
  - PowerPoint matches animation targets (`p:timing`'s `spid`) by that id.
  - A deck whose shapes are edited and re-saved must keep it, or every retained animation
    loses its target.
- **No hit-testing, caret or selection API on the slide path.** Neither `casual-pres-layout`
  nor `casual-pres-wasm` has one.
- **Stale counts in earlier documents.** ADR-055's rationale cites a fixed operation count
  and "five exhaustive `transform` matches", and `168` cited another fixed count. Both were
  hand-maintained, and neither phrasing was in the shape `operation_count.rs` scans for. This
  change rewords both so they state no number.

## 2. The decision

Four parts.

1. **A slide edit is a second closed operation set, `SlideOperation`, in a new crate
   `casual-pres-edit`, over `casual_pres_model::Presentation`.** It never touches
   `v1::Document`, as ADR-055 requires.
2. **It travels in the same transaction envelope, through the same revision log, undo
   projection, codec frame and OT pipeline the document editor uses.** These are published in
   place in `casual-doc-transaction` as generic over an `OperationSet`, with the document
   class as the default type argument. No DOCX call site, literal or behaviour changes.
3. **What is document-agnostic is shared verbatim.**
   - Identity: `Mint`, `IdSpace`.
   - Positions: `Pos`, `Range`, `Affinity`, `PositionMap`.
   - Envelope fields: `Intent`, `Coalesce`, `Origin`.
   - The log, the undo projection, the frame codec, and the positional rebase kernel.

   **What depends on a document's shape is written per class:**
   - apply and inverse;
   - the eleven exhaustive classifications;
   - access admission.

   Those must exist per class, because a DrawingML run is not a `w:r` (§6).
4. **Editing is built in phases, each shipping something a user can do** (§12). Collaboration
   on a deck comes last, but every slide operation is OT-classified from its first commit,
   because ADR-033 makes an unclassified operation a build failure.

## 3. Where the operations live, and the seam they need

### 3.1 The seam: one envelope, generic over the operation set

`casual-doc-transaction` gains one trait. It is the contract a document class signs to use
the shared envelope:

```rust
pub trait OperationSet {
    /// The document class this set changes (`v1::Document`, `Presentation`).
    type Document;
    /// The closed operation enum. Serde, because ADR-057's frame carries it.
    type Operation: Clone + Debug + Eq + Serialize + DeserializeOwned;
    /// Applies one operation; returns its inverse. Validates before it mutates.
    fn apply(doc: &mut Self::Document, mint: Mint, op: &Self::Operation)
        -> Result<Self::Operation, EditError>;
    /// The position-map steps an operation produces (text insert/delete/split/join).
    fn mapping_steps(op: &Self::Operation, inverse: &Self::Operation) -> Vec<MappingStep>;
    /// …and the exhaustive classifications the transform reads (§10).
}
```

`Transaction`, `Commit`, `RevisionLog`, `WireOperation` and the sessions become generic over
it, with the document class as the **default type argument**. `Transaction` is
`Transaction<O = casual_doc_edit::Operation>`, and so on. Every existing DOCX call site,
struct literal and test therefore compiles unchanged and monomorphises to the same code.
That is the route ADR-056 took for the same reason: an additive change to the envelope is
free, and a change to an operation's shape breaks every literal (E0063).

**This is not a second implementation.** The log, undo, coalescing, eviction bounds
(`DEFAULT_MAX_UNDO_GROUPS`, `MAX_COMMITS_PER_GROUP`), codec and ordering logic stay one body
of code. The generic parameter is the whole cost.

**Proof that the DOCX path did not move** is part of the change that publishes the seam.
That change must show, with the evidence in its commit message:

- the full Rust test suite passes unchanged;
- the editor's full browser suite passes;
- `casual_doc_wasm`'s code section is byte-identical before and after, which
  `wasm_size.test.mjs` can measure.

### 3.2 `casual-pres-edit`, a sibling of `casual-doc-edit`

`casual-pres-edit` holds `SlideOperation`, its `apply` and inverses, and its access
admission. It depends on:

- `casual-pres-model`;
- `casual-doc-edit`, for `Pos`, `Range`, `Mint` and `EditError`, which ADR-055 publishes in
  place rather than extracting;
- `casual-doc-transaction`, for the trait.

`casual-pres-wasm` gains the log and **one choke point**, `apply_group`. A source guard,
`every_presentation_mutation_is_a_transaction`, mirrors the document facade's (`casual-doc-wasm`
`lib.rs`, `every_document_mutation_is_a_transaction`) and requires exactly that one point.
`Presentation::slides_mut` and `definitions_mut` become `pub(crate)` plus a `test-support`
feature, so a mutation outside a transaction stops compiling rather than being forbidden by
review.

## 4. The operation set

**Tier** is ADR-033's:

| Tier | Meaning | Concurrency rule |
| --- | --- | --- |
| PL | paragraph-local | positional rebase |
| SI | sibling-indexed, through an `Intent` anchor | anchor, as ADR-056 |
| NA | node-addressed | liveness plus per-aspect contention |
| DS | deck scope | last writer wins |

**Phase** is §12's. Every operation returns its inverse, using the patterns `casual-doc-edit`
uses: a retained previous value, retained content, or a snapshot vehicle.

| Operation | What it does | Addressing | Tier | Inverse | Phase |
| --- | --- | --- | --- | --- | --- |
| `InsertSlide` | a new slide from a layout, with its placeholders, or a duplicate | `Intent` anchor `Before(SlideId)` / `AtEnd`; content minted | SI | `DeleteSlide` | 1 |
| `DeleteSlide` | remove a slide | `SlideId` | NA | `InsertSlide` carrying the slide | 1 |
| `MoveSlide` | reorder | `SlideId` + `Intent` anchor | SI | `MoveSlide` to the old anchor | 1 |
| `SetSlideLayout` | change a slide's layout; placeholders re-bind by `(kind, index)` | `SlideId` | NA | previous layout and the shapes re-bound | 2 |
| `SetSlideHidden`, `SetSlideName`, `SetSlideBackground` | slide attributes | `SlideId` | NA | previous value | 1 / 1 / 2 |
| `InsertShape` | a shape, picture, text box, group or table, or a placeholder materialised (§5.3) | `SlideId` + `Intent` anchor for z-order; content minted | SI | `DeleteShape` | 1 |
| `DeleteShape` | remove a shape and its subtree | `NodeId` | NA | `InsertShape` carrying it | 1 |
| `MoveShapeInZOrder` | bring forward, send back | `NodeId` + `Intent` anchor | SI | the old anchor | 1 |
| `SetShapeTransform` | move, resize, rotate, flip (`a:xfrm`) | `NodeId` | NA | previous transform | 1 |
| `SetShapePaint` | fill and outline, **both places at once** (§7) | `NodeId` | NA | previous paint pair | 2 |
| `SetShapeGeometry` | preset and adjust values (the 187-preset table) | `NodeId` | NA | previous geometry | 2 |
| `SetObjectDescr` | name, alt text, title | `NodeId` | NA | previous | 1 |
| `SetImageCrop` | `a:srcRect` | `NodeId` | NA | previous crop | 2 |
| `GroupShapes` / `UngroupShapes` | group and ungroup, recomputing child transforms | node ids + minted group id | NA | each other | 3 |
| `SetTextBodyProperties` | anchor, insets, wrap, autofit, columns, vertical | shape `NodeId` | NA | previous | 2 |
| `InsertText` | type at a caret | `Pos` | PL | `DeleteText` | 1 |
| `DeleteText` | delete within one paragraph | `Range` | PL | `InsertText`, or a paragraph snapshot | 1 |
| `SplitParagraph` | Enter | `Pos` + minted id | PL | `JoinParagraphs` | 1 |
| `JoinParagraphs` | Backspace at a paragraph's start | two paragraph ids | PL | `SplitParagraph` | 1 |
| `InsertLineBreak` | Shift+Enter (`a:br`) | `Pos` + minted id | PL | `DeleteText` over it | 1 |
| `FormatText` | run properties over a range (§6) | `Range` | PL | per-run previous properties | 1 |
| `SetParagraphProperties` | level, alignment, bullet, numbering, indent, spacing | paragraph `NodeId` | NA | previous | 1 |
| `SetParagraphRuns` | replace one paragraph's runs; the snapshot vehicle, never on the typing path (B4) | paragraph `NodeId` | NA | itself | 1 |
| `InsertTableRow` / `DeleteTableRow` / `InsertTableColumn` / `DeleteTableColumn` / `SetTableCellProperties` | the document editor's table operations, over `SlideTable` | table or cell `NodeId` + index | SI / NA | as the document's | 3 |
| `SetSlideSize` | `p:sldSz` | deck | DS | previous | 4 |

Speaker notes need **no operations of their own**. A notes body is a text body, so once `168`
lane E4 puts notes in the model, the text operations above address its paragraphs by id.

Out of scope, and not built: master and layout editing (`168` §5), animation and transition
editing (retained verbatim; §5.2 keeps their targets valid), 3-D, and SmartArt editing.

## 5. Addressing and identity

### 5.1 Every target is a `NodeId`

A slide is its `SlideId`. A shape is its drawing's id. A paragraph is its own id. A position
is a `Pos`, the same type as the document editor's. **A paragraph lookup by id must be O(1)
on the typing path.** The presentation therefore needs the cached route the document model
has (`v1/locate.rs`, `paragraph_mut`) rather than walking slides. The layout and master
lookups that are linear scans today (`layout_of`, `master_of`) are fine off the typing path
and are named in the operations that use them.

### 5.2 The authored shape id is a prerequisite, not a nicety

Editing makes the missing `p:cNvPr@id` load-bearing:

- **A shape that survives an edit must keep its authored id.** Then the retained `p:timing`
  still names it on save.
- **A deleted shape** must take its animation entries with it, and the loss report must say
  so.
- **A duplicated or pasted shape** gets a fresh authored id, unique within its slide. It is
  assigned with the node ids, before the operation is built (§5.4).

Phase 0 therefore carries the authored id on `SlideNode` (it is not a `v1` field, so it does
not touch the shared drawing types) and writes it back. This closes the "later" row of `168`
§2 (`cNvPr/@id`) first, because editing depends on it.

### 5.3 Placeholders are materialised, not edited in place

An empty slide placeholder ("Click to add title") is inherited from the layout and has no
node on the slide. In PowerPoint and in Google Slides, typing into it creates the slide's own
copy. The same happens here, and it is **two operations in one transaction**:

1. `InsertShape`, carrying a shape bound to the placeholder's `(kind, index)` with an empty
   text body;
2. the first `InsertText`.

Undo removes both. Nothing ever edits a layout through a slide.

### 5.4 Duplicate, copy and paste re-mint

`InsertSlide` and `InsertShape` carry their content with **fresh ids already assigned**, as
`casual-doc-edit`'s `InsertBlocks` requires of structured paste ("the caller assigns fresh ids
before inserting"). The ids come from the facade's generator in the identity space the
transaction reserves (ADR-051), and the inverse records them, so undo and redo re-insert the
same nodes. A duplicate is therefore never two nodes with one id. Media references are shared
through `Definitions` and are never duplicated.

## 6. Text: the same anchors, a different run model

- **What carries over unchanged.** `Pos`, `Range`, `Affinity`, `MappingStep`,
  `PositionMap`, the caret rules, coalescing per run (B3), and the positional half of the
  transform (§10).
- **What cannot be reused, and why.** The *apply* kernels cannot be reused as they stand.
  - `casual_doc_edit::insert_text`, `delete_text` and `ensure_run_boundary` operate on
    `Vec<InlineNode>`.
  - `FormatDelta` is shaped like `w:rPr`: half-points, `w:rFonts`, highlight.
  - A DrawingML run measures size in hundredths of a point, has `a:latin`, `a:ea`, `a:cs` and
    `a:sym`, and is a `TextRun` enum with `Run`, `LineBreak` and `Field` arms
    (`text_paragraph.rs:270-279`). `text_run.rs:7-16` already warns that the units differ by
    a factor of 50.
- **The plan.** `casual-pres-edit` therefore has its own run kernels and a `TextFormatDelta`
  in DrawingML terms. The kernels are small: split a run at a byte offset, merge equal
  neighbours, and keep a paragraph non-empty.
- **The guard against drift.** A shared property test drives the same edit script through
  both kernels and requires the same plain text and the same `PositionMap`. That is what
  keeps "different data" from becoming "different behaviour".

Rejected, with reasons, in §13: mapping slide text into `v1::Paragraph` to borrow the
document kernels.

Autofit (`normAutofit`'s font scale and line reduction) is a **layout** result, not a model
edit, as PowerPoint's own file shows. An edit never writes it. Layout recomputes it.

## 7. One operation, two places: a shape's paint

A slide shape's fill and outline are stated twice:

- on the drawing (`GroupShape.fill` / `stroke`);
- on the slide wrapper (`SlideNode.fill` / `outline`, `SlidePaint::{Inherited, Authored,
  Suppressed}`).

`validate_stated_paint` refuses the two to disagree. `SetShapePaint` therefore carries the
pair and writes **both** atomically, and its inverse carries both previous values. There is
deliberately no operation that writes one side. That would be a model the validator rejects,
reachable by one keystroke.

## 8. Undo, redo and coalescing

These are unchanged, because they are the shared log's:

- undo is a read of the log (ADR-043);
- groups are bounded (`DEFAULT_MAX_UNDO_GROUPS`, `MAX_COMMITS_PER_GROUP`);
- typing coalesces per run;
- a drag is one coalesced group, so moving a shape across the slide is one undo step, as in
  both reference products.

A slide-level operation is its own group. The facade's undo is the document facade's
`apply_group` with `Origin::Undo` (`casual-doc-wasm` `lib.rs`, around `:14000`).

## 9. Validation and the budgets

`107` §4's budgets, applied to a deck:

| Budget | Rule on a deck |
| --- | --- |
| **B1** | A keystroke is O(1) in deck size: one paragraph found by cached route, one run kernel, `TextParagraph::validate` on that paragraph only. `Presentation::validate` never runs per operation. Slide-level operations validate the slide they touch (O(slide)). |
| **B2** | Transform cost is O(concurrent operations since base), as for documents. |
| **B3 / B4** | Typing coalesces; `SetParagraphRuns` is never on the typing path. |
| **B6** | **A slide is the natural invalidation unit.** An edit re-lays out and re-rasterises the slide it touched and that slide's thumbnail, and nothing else. This is easier to hold than on the document path, where a paragraph can reflow every later page. |
| **B5 / B7** | Unchanged; the log is the shared one. |

The doubling guard is the same as the document editor's. Typing into slide 1 of a 2n-slide
deck costs the same as in an n-slide deck. That is measured as n versus 2n, never against a
millisecond threshold.

## 10. Concurrency: classified now, collaborative later

Every `SlideOperation` implements the eleven exhaustive classifications from its first
commit:

- the eight transform matches;
- `introduces`;
- access admission;
- the protection exemption.

The rules by tier:

- **PL** operations reuse the positional rebase kernel (`rebase_coordinates`, `transform.rs`
  around `:1395`). It is lifted into a module both operation sets call, because its input is
  a `Pos` and an effect (Text / Split / Join), not a document.
- **SI** operations carry ADR-056 `Intent` anchors. Slide order and z-order are sibling lists
  exactly as blocks are, and a destroyed anchor is a reported loss, never a fallback to the
  old index. `resolve_anchor` is generalised over a sibling list. That is the second, small
  part of the seam.
- **NA** operations contend through `footprint`'s `(Target, Aspects)` bitmask. A shape's
  aspects are transform, paint, geometry, text-body properties and description. So one
  participant moving a shape while another recolours it both win, and two participants
  moving it is last writer wins.
- **Deleting a slide or shape** tombstones concurrent operations inside it (`Rebase::Tombstoned`)
  and reports them, as deleting a table does today.

**What is deliberately later.** The relay carries a document-class tag at join, which bumps
`PROTOCOL_VERSION`; a session's frames are then of that class. The TP1 property tests for
slide operations are a precondition for that phase, not for phases 1–3. Nothing about
single-user editing waits on the relay.

## 11. Surfaces

- **The engine first.** `casual-pres-layout` gains hit-testing:
  - point → shape;
  - point → `Pos`, from the cluster index it already builds;
  - `Pos` → caret rectangle.

  `casual-pres-wasm` gains selection, `apply_group`, undo and redo, mirroring the document
  facade's names.
- **The page.** `slides.html` gains the editing-mode control it deliberately lacks today
  (`168` §4a), and the slide commands become real commands on the same registry. Every
  capability is reachable from at least two surfaces, as SKILL §10 requires: ribbon, context
  menu, shortcut.
- **Text input is the hard part, and it should not be written twice.** The document editor's
  input host (hidden field, IME composition, the `beforeinput` contract, the
  `keyboard.type` traps SKILL §4 lists) lives inside `main.js`, which has no mount seam
  (HF-109). There are two ways to give slides text input:
  - **(a)** lift the input host out of `main.js` into a module both pages mount;
  - **(b)** write a second input host for slides.

  This design takes **(a)**. It costs an HF-109 increment first, and it is the only answer
  that does not ship two IME implementations.

## 12. Phases

Each phase ships something a user can do, and each is a separate change with its own gates.

| Phase | Delivers | Needs |
| --- | --- | --- |
| **0 — ground** | Nothing visible. The seam (§3.1) with the DOCX path proven unmoved; `casual-pres-edit` with an empty set; the facade choke point and its source guard; mutators made private; authored shape ids kept and written back (§5.2); the cached paragraph route | ADR-067 accepted |
| **1 — a usable editor** | Add, duplicate, delete, reorder and hide slides; move, resize, rotate, delete and z-order shapes; type, delete, Enter, Shift+Enter; bold, italic, underline, size, colour and font; paragraph level, alignment and bullets; alt text; undo and redo | Phase 0; lane G (group text in the model); the input host lifted out of `main.js` |
| **2 — the formatting people expect** | Fill, outline and geometry from the existing pickers; insert picture, shape and text box; crop; text-box properties; change a slide's layout; slide background; speaker-notes text (with lane E4) | Phase 1 |
| **3 — structure** | Tables (the document editor's table operations over `SlideTable`); group and ungroup; copy and paste within a deck and between decks | Phase 2 |
| **4 — together** | Co-editing a deck: the class-tagged relay, TP1 property tests for every slide operation, presence on a slide | Phase 3, and ADR-033's gates for slides |

## 13. Alternatives rejected

- **Slide arms inside `casual_doc_edit::Operation`, resolved through a new `Surface` arm**
  (`156` §3.3's sketch).
  - Every one of the eleven exhaustive matches on the DOCX editor's path would grow a slide
    case.
  - `v1::Document` would have to hold a deck.
  - Every slide increment would bump the DOCX protocol.
  - ADR-055 rejects exactly this, and this ADR supersedes `156` §3.3's sketch rather than
    contradicting it silently.
- **A `casual-pres-transaction` crate with its own envelope, log and undo.** This is the
  second copy `168` exists to prevent. Two logs would answer "what does undo undo" two ways.
- **Slide text mapped onto `v1::Paragraph` so the document kernels apply.** The mapping is
  lossy in both directions: DrawingML size, fonts, `a:sym`, and fields with their own types.
  Per edit, it is either a conversion (breaking B1) or a persistent shadow model, which means
  two sources of truth. The shared property test in §6 gets the benefit without the mapping.
- **Editing slide text in the DOM** (a `contenteditable` laid over the canvas).
  - It violates ADR-005.
  - The DOM would become the source of truth for a run's formatting.
  - The canvas and the overlay would disagree about every glyph the shaper places
    differently from the browser.
- **Index-addressed slide and z-order operations.** Concurrent reorders resolve to the wrong
  slide. ADR-056 already measured why anchors travel on the envelope.
- **A CRDT for decks.** ADR-033 closed this for both classes; one collaboration model across
  the product.

## 14. Questions for the owner

1. **The seam.** Making the log and envelope generic touches `casual-doc-transaction`, which
   the document editor runs on. The proposal keeps it behaviour-neutral by default type
   arguments and proves it with an identical code section (§3.1). Is that acceptable, or should
   slide editing wait for a third consumer, as ADR-055's "no extraction before a third
   consumer" would read on its strictest reading?
2. **Phase 1's scope.** The table in §12 is the proposal. Is anything there that should wait,
   or missing that a first editor must have?
3. **The input host.** Lift the document editor's input handling out of `main.js` first
   (an HF-109 increment), so slides and documents share one IME implementation? Or accept a
   second input host to start sooner? The design recommends the first.
4. **Placeholder behaviour.** Typing into an empty layout placeholder creates the slide's own
   copy, as PowerPoint and Google Slides do (§5.3). Confirm.
