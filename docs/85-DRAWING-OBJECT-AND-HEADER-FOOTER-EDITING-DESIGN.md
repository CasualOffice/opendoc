# 85 — Drawing-Object and Header/Footer Editing Design

**Status:** Proposed design for owner review. **Design-first per AGENTS.md — this
doc defines scope, ops, and phasing; it is explicitly NOT an implementation and
lands no code.**
**Date:** 2026-08-01
**Depends on (read first):** doc 58 (interaction/selection/editing pipeline —
this design is an *additive extension* of its `HitTarget`/`Selection`/`Command`
taxonomy), doc 59 (the live v1 edit op-set), doc 56 (shell/engine-drawn
selection), docs 47/48/51/52/53/54 (text-box + float + per-section render/layout,
already shipped), doc 84 (context-menu / shared command registry), doc 69
(competitive interaction grammar), doc 67 row 10 (the editing-scope gap this
closes).
**Owner decisions recorded (2026-08-01):** Q3, Q4, and Q5 in §9 are now
**DECIDED** and folded into the phasing, ops, and tracker below; the remaining
open questions (Q1, Q2, Q6, Q7, Q8, Q9) still gate implementation. **§10 is a
competitive comparison of how OnlyOffice, Microsoft Word, and Google Docs handle
object and header/footer editing, plus what we adopt vs. deliberately differ**
given our engine-drawn-selection / model-as-truth / canvas architecture — read it
alongside the interaction grammar in §4.

## 1. Purpose and phasing

Body and table-cell paragraph text is editable today; **headers/footers, notes,
text boxes, and drawing objects are not general editing surfaces** (doc 67 row
10). This design closes that gap in two phases, preceded by one prerequisite
model slice, in a deliberate order:

- **Phase 0 — prerequisite model slice (DECIDED, Q3):** add image **crop**
  (`a:srcRect`) and inline-`Drawing` **alt-text** (`descr`) to the model, with
  import/export/layout, *before* Phase A object editing — so crop and alt-text
  ship *with* Phase A's image ops rather than trailing them. This is the one item
  that is not a pure edit-layer change (§5.4).
- **Phase A — drawing-object editing:** insert / select / move / resize / delete
  for images, text boxes, and basic shapes — **including inserting new basic
  shapes**, not only editing existing ones (DECIDED, Q4); image replace / crop /
  alt-text / wrap-mode; text-box text editing and body properties; and the one
  *object interaction grammar* shared by every object kind. Object edits are
  **blocked (fail-closed) in Suggesting mode** because they have no OOXML revision
  markup (DECIDED, Q5; §6).
- **Phase B — headers/footers as editable surfaces:** entering/exiting the
  header/footer editing context; editing their paragraphs **and** their
  contained images/text-boxes (by reusing Phase A); first-page and even/odd
  variants; and section "same as previous" linkage.

**Why A before B (the load-bearing rationale).** Real-world DOCX headers and
footers are predominantly *images and text boxes* — logos, letterheads, banners —
not bare text. A header/footer editing surface that could edit only its
paragraphs would be unable to touch the logo that is the whole point of most
letterheads. Phase B therefore *depends on* Phase A: once a text box or anchored
image is a first-class editable object (Phase A), editing it inside a header is
"the same object, a different container" (Phase B), not a second implementation.
This ordering is also why Phase B is mostly a *context/addressing* problem rather
than new object machinery.

This is an **edit** design. Render and layout for all of these already ship —
inline and anchored drawings (`P1F-28`), text boxes and their body properties
(docs 47/52/54, `P1F-FLOAT-*`), groups/shapes, and per-section headers/footers
(`P1A-027`, `P1F-17`, `P1F-53`). The work here is making the already-rendered
constructs *mutable through the command/transaction contract* — never making the
browser DOM authoritative (doc 67 row 10; doc 56).

## 2. What already exists (grounding)

Verified against `crates/casual-doc-model/src/v1/{body.rs,definitions.rs}`,
`crates/casual-doc-edit/src/lib.rs`, and `crates/casual-doc-wasm/src/lib.rs`.

### 2.1 Model (all render + round-trip today; none editable)

| Construct | Type (`v1`) | Key fields |
| --- | --- | --- |
| Inline image | `InlineNode::Drawing` | `{ id, media: MediaId, extent: Option<Extent> }` — **no `descr`/alt-text, no crop, no rotation** |
| Floating image | `InlineNode::AnchoredDrawing` | `{ id, media, extent: Extent, anchor: DrawingAnchor, descr: Option<String>, relative_height: Option<u32> }` |
| Anchor geometry | `DrawingAnchor` | `{ horizontal: AnchorHorizontal, vertical: AnchorVertical, wrap: WrapMode, wrap_distances: WrapDistances, behind_doc: bool }` |
| Wrap | `WrapMode` | `Square \| Tight \| Through \| TopAndBottom \| None` (behind/in-front = `behind_doc` + `None`) |
| Text box | `InlineNode::TextBox` | `{ id, anchor: Option<DrawingAnchor>, relative_height, extent: Option<Extent>, fill, border, body_properties: TextBoxBodyProperties, blocks: Vec<BlockNode> }` (`anchor: None` = inline) |
| Text-box body | `TextBoxBodyProperties` | `{ insets, vertical_anchor, horizontal_overflow, vertical_overflow, auto_fit }` (doc 52) |
| Group / shapes | `InlineNode::Group(WordprocessingGroup)` | `{ id, anchor, extent, transform, children: Vec<GroupChild> }`; `GroupChild = Picture \| TextBox \| Shape \| Group`; `GroupShape.geometry: ShapeGeometry` (`Rectangle \| RoundRectangle \| Ellipse \| Line \| Other`) |
| Chart/SmartArt/OLE | `InlineNode::EmbeddedObject` | inline-only positioning; part bytes preserved out-of-model |
| Media | `Definitions.media: DefinitionMap<MediaId, MediaReference>` | `MediaReference { relationship_id, media_type, part_name }` — **image bytes are NOT in the model**; they live in the package/preservation side-table |
| Section | `Definitions.sections: Vec<SectionBoundary>` | `{ …, headers: Vec<HeaderFooterRef>, footers: Vec<HeaderFooterRef>, title_page: Option<bool> }` |
| H/F reference | `HeaderFooterRef` | `{ kind: HeaderFooterKind (Default \| First \| Even), reference: HeaderFooterId }` |
| H/F content | `Definitions.headers / footers: DefinitionMap<HeaderFooterId, HeaderFooter>` | `HeaderFooter { blocks: Vec<BlockNode> }` (two **separate** maps) |

Every content node carries a globally unique `NodeId` (`u128`, namespace+counter,
32-hex string). "Everything is a node" (doc 58 §1) already holds in the model.

**Model gaps this design must confront (see §5, §9):** inline `Drawing` has no
`descr`; **no crop (`a:srcRect`), rotation, or flip** on any picture/shape;
`ShapeGeometry` is a coarse 5-value enum (no custom geometry); `EmbeddedObject`
has no floating position; **"same as previous"/link-to-previous is not modeled**
(OOXML expresses it by *omitting* a `HeaderFooterRef`, i.e. implicit-by-absence).

### 2.2 Edit op-set (the live path)

The shipping op-set is `casual_doc_edit::Operation` applied to `v1::Document`.
(`casual-doc-transaction` is the older v0 grapheme substrate and is **not** wired
into the WASM editing path — doc 59 says it is paralleled, not reused.) The
existing variants are text/paragraph (`InsertText`, `DeleteText`,
`SplitParagraph`, `JoinParagraphs`, `FormatText`, `ClearFormatting`,
`SetHyperlink`, `SetInlines`, `SetParagraphProperties`), table
(`InsertRow`/`DeleteRow`/`InsertColumn`/`DeleteColumn`/`DeleteTable`/`InsertTable`/
`SetTableCellProperties`/`SetTableProperties`/`ReplaceTable`), and document/section
(`SetCoreProperties`, `UpdateReviewState`, `SetSectionGeometry`).

**No variant touches drawings, anchors, images, text boxes, or header/footer
content.** `SetSectionGeometry` explicitly leaves headers/footers untouched.

The choke point is `apply(doc, ids, op) -> Result<Operation /*inverse*/, EditError>`
(stateless, validate-then-mutate, returns the inverse). Undo/redo, the typed
`HistoryKind` labels, 256-entry history bound, and typing coalescing live on
`WasmDocument` in `casual-doc-wasm`; a group of ops undoes atomically via
`apply_group`. Public mutation satisfies AGENTS.md's "commands and transactions"
rule through the **closed op-set behind the single `apply` seam** (I1/I2): JS
never constructs an `Operation`; semantic `#[wasm_bindgen]` methods build ops
internally.

**The addressing constraint (critical).** Every op targets `document.body()`.
`find_paragraph`/`find_paragraph_mut` walk exactly `Paragraph`, `Table` (cell
blocks), and `Sdt` (SDT blocks) **within the body slice** — they do not descend
into headers, footers, text-box bodies, group text boxes, or note blocks.
`Pos { node, offset }` (paragraph-relative UTF-8 byte) is the only anchor; there
is **no object (non-text) selection** anywhere. So both features need a new
*container/addressing* concept and (for objects) a new *selection* concept — the
subjects of §3 and §4.

## 3. Object selection and hit-testing

Doc 58 already declares the taxonomy this design fills in; the arms exist as
*declared-but-not-shipped* and this is the doc that ships them for objects.

### 3.1 Hit targets (extend `HitTarget`, additive)

Doc 58 §2 already names the arms we need — implement them:

- `InlineObject { node, kind: ObjectKind, rect }` — an inline `Drawing`/`TextBox`/
  `EmbeddedObject` hit as a unit.
- `Float { node, rect, handle: Option<Handle> }` — an anchored drawing/text-box/
  group; `Handle` names a resize/move grip so a `Float` hit *is* a drag gesture
  with no new pipeline.
- `RunningZone { region: HeaderFooter, pos }` — a header/footer band (Phase B).

`ObjectKind` distinguishes image / text-box / shape / group / embedded so the
frontend `match` and the context bar can specialize without new hit paths.

### 3.2 Selection (extend `Selection`, additive)

Doc 58 §3 declares `Selection::Object(NodeId)`. Ship it: an image/float/shape/
group selected *as a unit* is `Object(node)`. A caret *inside* a text-box body or
a header paragraph remains an ordinary `Caret`/`Text` selection — its `Pos.node`
just happens to resolve to a paragraph inside a non-body container (§4.1).

### 3.3 Engine-drawn selection and handles

Objects cannot rely on the DOM (doc 56/58). The selection overlay we already draw
from `LayoutSnapshot` geometry gains **object outlines + resize/move handles**
painted from the object's placed rect (`Page.anchored` for floats;
`DisplayList`/inline box for inline objects). New WASM read methods, mirroring
`caretRect`/`selectionRects`:

- `objectAt(page, xTwip, yTwip) -> HitTarget` — the object/handle under a point.
- `objectRect(node) -> Rect` and `objectHandles(node) -> Handle[]` — the geometry
  **we** paint selection chrome from (exact raster match, zero overlay drift).

These are read-only (no transaction), exactly as `copyText`/`selectionRects` are.

**Implementation note (`P1G-OBJ-SELECT`, shipped).** The first slice resolves
**inline objects** — `w:drawing` pictures and inline text boxes in top-level body
paragraphs — by walking the paginated layout and correlating each placed box back
to its model node at query time (image by media part-name then document order;
text box by order), so no `NodeId` had to be threaded onto the layout structs
(`InlineImage`/`InlineTextBox` carry no id) and the shaper/flow pipeline is
untouched. `objectHandles` returns the eight grip centers `[page, cx, cy, kind]`
(NW,N,NE,E,SE,S,SW,W); the host paints a fixed screen-size grip at each. **Still
open for a follow-up selection slice:** anchored/floating objects need
`PlacedAnchor` to carry its source `NodeId` (a small `anchor.rs` thread), and
table-cell / header-footer objects need the walk to descend those containers.
Handle-grip *hit* resolution (the `Float { handle }` arm that turns a grip click
into a drag) lands with the move/resize op slice (`P1G-OBJ-GEOMETRY`); this slice
paints grips but does not make them draggable.

## 4. The universal object interaction grammar

One grammar for images, text boxes, shapes, groups, **and** tables (doc 69's
"disable, don't hide" contextual model; doc 84's command registry). Matches Word
/ Google Docs muscle memory:

1. **Single click** on an object → `Selection::Object(node)`; the engine draws the
   outline + handles and the host shows a **context bar** (doc 84
   `editorCommands(context)` with an `object` context) — e.g. Replace / Crop /
   Alt-text / Wrap / Delete for an image; Edit-text / Fill / Outline / Wrap /
   Delete for a text box.
2. **Drag a handle** → live move/resize (Phase A ops, §5); **Escape** during a
   drag cancels it.
3. **Enter or double-click** on a *container* object (text box, group text box)
   → *enters* its content: selection becomes a `Caret` inside the box's body;
   ordinary text editing applies (§4.1). A leaf object (image/shape) has no edit
   mode; Enter/double-click opens its primary context action (Replace for an
   image).
4. **Escape** from edit-mode → back to `Object(node)` selected (not yet to the
   page). **Escape again** → collapses to a `Caret` in the surrounding text at the
   object's anchor. This two-step exit is the Word/Docs convention and keeps a
   mis-click from ejecting the user all the way to the page.
5. **Delete/Backspace** with `Object(node)` selected → delete the object (§5).
6. **Tab / Shift+Tab** with an object selected → move to the next/previous object
   on the page (keyboard object traversal; optional, flagged in §9).

This grammar is defined once and is object-kind-agnostic; a new object kind adds
an `ObjectKind` + context-bar descriptors, never a new interaction path.

## 5. New model / transaction operations (Phase A)

All new variants are additive `casual_doc_edit::Operation` arms entering the same
`apply` choke point, each returning an exact inverse, each grouped into one
undoable `HistoryEntry` (new `HistoryKind` labels: `ObjectInsert`, `ObjectMove`,
`ObjectResize`, `ObjectDelete`, `ObjectProperties`). JS still never builds an
`Operation`; new semantic `#[wasm_bindgen]` methods (with the `*_inner`
`Result<_, String>` test pattern) compile to them.

### 5.1 Addressing: a `Location` for non-body containers

Because `apply` is body-only, introduce one additive addressing enum so an op can
name *which block sequence* it targets. `NodeId`s are globally unique, so a
paragraph `Pos` already identifies its paragraph uniquely — the locator just needs
to search more roots; `Location` is needed for **structural** ops (insert/delete
an object or a paragraph, cross-paragraph joins) that must know the container:

```
Location =
  | Body
  | Header(HeaderFooterId) | Footer(HeaderFooterId)   // Phase B
  | TextBoxBody(NodeId)                               // the box whose blocks we edit
  | GroupTextBoxBody(NodeId)
```

Minimal implementation change: generalize `find_paragraph`/`find_paragraph_mut`
to resolve a `NodeId` across a *set of roots* (body + every header/footer body +
every text-box/group-text-box body), so **all existing text ops work unchanged
inside text boxes and headers/footers once the paragraph is found** — this is the
single highest-leverage change and the reason text-box/header text editing is
mostly *reuse*, not new ops. Cross-paragraph and object-structural ops carry an
explicit `Location`.

### 5.2 Object structural ops

- `InsertObject { location: Location, index: u32, object: Box<InlineNode> }` —
  insert an inline `Drawing`/`TextBox`/`EmbeddedObject` at a paragraph position,
  or (for a float) attach an anchored object to a paragraph. Inverse:
  `DeleteObject`.
- `DeleteObject { object: NodeId }` — remove the object; **retained inverse** =
  `InsertObject` with the removed node (mirrors `DeleteRow` carrying its row).
- Mirrors the existing `InsertTable { container: Option<NodeId> }` precedent for
  a container-scoped structural op.

### 5.3 Geometry ops (move / resize / wrap)

- `SetAnchor { object: NodeId, anchor: DrawingAnchor }` — move a float, change its
  wrap mode / wrap distances / z-order (`behind_doc`, `relative_height`).
  Self-inverse carrying the previous `DrawingAnchor` (retained-value pattern, like
  `SetParagraphProperties`).
- `SetExtent { object: NodeId, extent: Extent }` — resize an image / text box /
  shape (inline or floating). Self-inverse with the previous `Extent`.
- A drag interaction issues **one** `SetAnchor`/`SetExtent` at drag-end (live
  preview is host-side chrome, not per-frame ops), so a move/resize is one undo
  step.

**Implementation note (`P1G-OBJ-GEOMETRY`, resize shipped).** `SetExtent` landed
as an additive `casual_doc_edit::Operation` arm (self-inverse with the previous
`Option<Extent>`), driven by `setObjectExtent(node, wEmu, hEmu)` and committed
once on handle-release; the eight selection grips from `P1G-OBJ-SELECT` are the
draggable handles (the grip div is the `Float { handle }` hit resolution — no
separate `objectHandleAt` needed), corner grips constrain aspect under Shift
(§10.3), and the op is blocked fail-closed in Suggesting/Viewing mode (§6, Q5).
The resized extent **round-trips through DOCX** (export writes `wp:extent`; a
regression asserts export→reopen preserves the new size).

**Implementation note (`P1G-OBJ-ANCHOR-SELECT`, move + wrap shipped).** The
follow-up landed floating-object identity and geometry. `PlacedAnchor` now carries
a `node: Option<NodeId>` (set in `anchor.rs` for top-level `AnchoredDrawing`/
floating `TextBox`; `None` for group children), so `object_boxes` resolves body
floats from `page.anchored` — no fragile correlation needed on the float side.
`SetAnchor { object, anchor }` (self-inverse with the previous `DrawingAnchor`)
carries move **and** wrap **and** z-order in one op; a free body-drag commits it as
a `page`-relative `posOffset` on release (`setObjectAnchorPosition`), and the
context bar's Wrap control commits it with a new `wrap`/`behind_doc`
(`setObjectWrap`). `SetExtent` now also resizes floats. All three round-trip
through DOCX and are blocked fail-closed in Suggesting/Viewing. **Still deferred:**
group children and header/footer floats (selection), and inline↔floating
conversion.

### 5.4 Image content ops

- `ReplaceMedia { object: NodeId, media: MediaId }` — swap the image; the new
  bytes enter the package/preservation side-table and a new `MediaReference` is
  registered in `Definitions.media` (a host-provided image → new `MediaId`).
  Self-inverse with the previous `MediaId`.
- `SetObjectDescr { object: NodeId, descr: Option<String> }` — alt-text. Inline
  `Drawing` gains a `descr` field in the **Phase 0** model slice (DECIDED, Q3);
  `AnchoredDrawing`/`GroupPicture` already carry `descr`. Self-inverse with the
  previous value.
- `SetImageCrop { object: NodeId, crop: CropRect }` — crop (`a:srcRect`). The
  `crop` field + import/export/layout land in the **Phase 0 prerequisite slice
  (DECIDED, Q3)** so this op ships *with* Phase A rather than trailing it (it is
  the one item that is not a pure edit-layer change — see the tracker in §11,
  where `P1G-OBJ-MODEL` runs *before* the Phase A object ops). Self-inverse with
  the previous crop.

### 5.5 Text-box ops

- Text-box *content* editing (typing, formatting, paragraphs, nested tables,
  even nested images) is **existing text/paragraph/table ops** targeting
  paragraphs inside the box's `blocks`, unlocked purely by the §5.1 locator
  change. No new content ops.
- `SetTextBoxBody { object: NodeId, properties: Box<TextBoxBodyProperties> }` —
  insets / vertical anchor / autofit / overflow (doc 52). Self-inverse with the
  previous value.
- `SetShapeFillStroke { object: NodeId, fill: Option<Rgba>, stroke: Option<ShapeStroke> }`
  — text-box / shape appearance (doc 47). Self-inverse.

### 5.6 Inserting new basic shapes (DECIDED, Q4)

Phase A includes **authoring new basic shapes**, not only editing existing ones.
This is a deliberately larger scope than "edit-existing", and it is honest about
what new-shape authoring needs that mutating an imported shape did not:

- **Op:** `InsertShape { location, index, geometry: ShapeGeometry, extent, anchor:
  Option<DrawingAnchor>, fill, stroke }` — a specialization of `InsertObject`
  (§5.2) that mints a `GroupShape`/`WordprocessingShape` node with a default
  extent at a default position, then the user drags/edits it. Inverse:
  `DeleteObject`.
- **Geometry is limited by the model, not by this op.** `ShapeGeometry` is a
  coarse 5-value enum (`Rectangle | RoundRectangle | Ellipse | Line | Other`), so
  the shape *gallery* offered at insert time is exactly those primitives —
  rectangle, rounded rectangle, ellipse/oval, line — plus any modeled `Other`
  presets that already round-trip. A richer preset gallery (Word's ~160 autoshape
  presets) or arbitrary custom geometry (`a:custGeom`) is **out of scope** until
  `ShapeGeometry` grows; the insert UI must only offer what the model + layout can
  faithfully render and export.
- **No rotation/flip at insert or edit** in this phase (the model carries a
  `transform` but rotation/flip authoring + hit-testing rotated handles is
  deferred — see §5.7). Inserted shapes are axis-aligned.
- **Text in a shape:** a shape that is a text box authors its text through the
  same text ops as any text-box body (§5.5); a plain shape has no text.

### 5.7 What is intentionally out of scope for Phase A

Rotation/flip authoring (and the rotated-handle hit-testing it implies), custom
`ShapeGeometry`/`a:custGeom` and the full preset autoshape gallery, floating
position for `EmbeddedObject`, and group re-parenting / child add-remove. Each
needs a model extension and is a later slice; naming them keeps "basic shapes"
honest.

## 6. Undo / transaction semantics and tracked changes

- **Undo:** every new op returns an exact inverse via the same `apply` contract;
  a move/resize/replace is one `HistoryEntry` (one undo step) with a new
  `HistoryKind` label for a readable Undo menu. No coalescing across distinct
  object gestures (unlike typing).
- **Selection remap:** after an object op the selection stays `Object(node)`
  (node identity is stable); a delete collapses to a `Caret` at the object's
  former anchor. No `PositionMap` byte-remap is needed for pure object ops (they
  don't shift paragraph text), which keeps them simpler than text edits.
- **Tracked changes (Suggesting mode) — DECIDED (Q5): block, fail-closed.** OOXML
  has **no revision markup for object move/resize/replace/insert/delete/property
  changes**, and Word itself does not track most object-geometry edits.
  Consistent with `REVIEW-GAP-009`'s structural-tracking backlog and the decision
  for the other untrackable ops (`P1G-REVIEW-042`), **every object op is blocked
  in Suggesting mode** via the existing shared `blockUntrackedInSuggesting()` gate
  and reports the existing "This command cannot be tracked yet; switch to Editing
  to apply it" status — it is never silently applied untracked. This gives a
  truthful "no silent untracked mutation" guarantee. (A future tracked-object
  representation, if one is ever designed, would revisit this — not this phase.)

## 7. Save / round-trip implications

- Edited anchors/extents/text-box bodies/appearance already have semantic export
  (docs 47/52/54, `P1F-28`); an edited value flows through the existing writer,
  so `import → edit → export → reopen` must reach a **semantic fixed point** for
  every new op (the standard gate used by the render slices).
- `ReplaceMedia` and any *new* image must register a package part + relationship
  and content-type override; the writer already emits media relationships for
  drawings (`P1B-005f`). New bytes come from the host (the engine owns no image
  decode/encode).
- `SetImageCrop` and alt-text on inline `Drawing` cannot round-trip until their
  model/export support lands (§5.4, §9-Q3) — a hard dependency, not a soft one.
- No new mutable layout tree is introduced (consistent with doc 53 decision 7):
  object ops mutate the model; the existing post-pagination anchor/float passes
  recompute placement.

## 8. Phase B — headers/footers as editable surfaces

Phase B is deliberately *thin* because Phase A did the object work and §5.1 did
the addressing.

### 8.1 Editing context: entering / exiting (not a document mutation)

Entering a header/footer to edit is a **selection/host-context** change, **not**
an `Operation`. The engine exposes which running-zone the caret is in; the host
UI dims the body and shows a "Header / Footer" affordance (Word convention:
double-click the top/bottom margin, or a ribbon entry point — doc 67 "Page/layout
controls"). `RunningZone` hit targets (§3.1) drive entry; **Escape** exits back to
the body caret. No `EnterHeaderFooter` op exists or is needed — editing header
content is ordinary text ops whose `Pos.node` resolves into a
`Header(id)`/`Footer(id)` `Location` (§5.1).

### 8.2 Editing header/footer content (reuse)

Once the §5.1 locator resolves paragraphs inside `Definitions.headers/footers`
bodies, **all** of Phase A + existing text/paragraph/table ops work inside a
header/footer verbatim — including the logos and banners (anchored images / text
boxes) that motivated the whole phase order. This is the payoff of A-before-B.

### 8.3 First-page and even/odd variants

- The variant a page shows is already selected at render time from
  `SectionBoundary.title_page` + `w:evenAndOddHeaders` (`P1F-17`). Editing targets
  the specific `HeaderFooter` body behind the `HeaderFooterRef` of the variant the
  user is viewing (`kind: Default | First | Even`).
- Creating a variant that does not yet exist (e.g. turning on a distinct
  first-page header) is a **structural section op**:
  `SetSectionRunningRef { section: SectionId, region: HeaderFooterRegion /*header|footer*/, kind: HeaderFooterKind, reference: Option<HeaderFooterId> }` —
  `Some(id)` links the section's variant to a (possibly newly created) body;
  `None` removes the ref. Plus `CreateHeaderFooterBody { region } -> HeaderFooterId`
  to mint an empty `HeaderFooter { blocks: [] }` in the right map. Both are
  additive ops with retained inverses. Toggling `title_page` reuses (or extends)
  a section-property op — §9-Q6.

### 8.4 "Same as previous" (link-to-previous) linkage

OOXML expresses inheritance by a section **omitting** a `HeaderFooterRef` (it then
inherits the previous section's). The model mirrors this implicitly-by-absence and
has **no `link_to_previous` field**. For editing we must decide (§9-Q7):

- **Unlink** a section's header (Word's "Link to Previous" off) = create a new
  `HeaderFooter` body (copying the inherited content) and add a `HeaderFooterRef`
  for that section/kind (`SetSectionRunningRef` with `Some`).
- **Re-link** (turn "Link to Previous" back on) = remove the section's
  `HeaderFooterRef` (`SetSectionRunningRef` with `None`) so it inherits again;
  the now-orphaned body is garbage-collected on export.
- This keeps linkage as pure ref presence/absence (faithful to OOXML) rather than
  adding a boolean the writer would have to reconcile — **recommended**, pending
  owner confirmation.

## 8b. Status reconciliation (2026-08-09)

This design was written 2026-08-01. Substantial parts of it have since shipped,
and several "open questions" were in practice answered by that code rather than
by a decision. Recorded here so the remaining decisions are only the ones that
are genuinely still open. Every claim below was checked against the tree at
`4754e1e`, not against memory.

**Shipped since the design was written**

- **Phase 0 is complete.** Image crop (`a:srcRect`) and `descr` alt-text are in
  the model (`casual-doc-model/src/v1/body.rs`), with import/export/layout.
- **Phase A object editing is largely in place**: selection, move, resize,
  anchoring, crop, alt-text and the object context menu, covered by
  `object-selection`, `object-geometry`, `object-anchor`, `object-edit` and
  `object-context-menu` browser specs.
- **Image insertion ships** (`insertImageFromFile` / `insertImageFromBlob`), from
  a file or a clipboard paste.
- **Object edits fail closed in Suggesting**, as Q5 decided.

**Questions the shipped code has already answered**

- **Q1 (Escape grammar) — answered for Escape.** `objectSelection` carries
  `mode: "selected" | "editing"` and the two-step Escape is implemented. The
  *Tab/Shift+Tab object traversal* half of Q1 is NOT implemented and remains a
  genuine choice.
- **Q2 (context bar vs contextual tab) — answered: a floating context bar.**
  `object-context-bar` is built and shipped. Note the shell has since grown two
  contextual/permanent ribbon tabs (Table, and Review as of #455), so if a
  "Picture Format" tab is still wanted it now has precedent to follow; the bar
  and a tab are not mutually exclusive.
- **Q8 (where image bytes come from) — answered by the shipped insert path.**

**Still genuinely open, and blocking Phase B**

- **Q1 (Tab/Shift+Tab object traversal)** — unimplemented, still a choice.
- **Q6 (`title_page` / section running toggles)** and **Q7 (link-to-previous
  semantics)** — untouched. Both are header/footer semantics, so both gate
  Phase B rather than Phase A.
- **Q9 (phasing granularity)** — worth re-asking now that Phase A has largely
  landed piecemeal rather than as one tracked phase.

**The keystone has not been started**

There is no `Location` (or equivalent sub-document address) type anywhere in the
workspace. Everything that needs a caret outside the body therefore remains
unavailable, and an audit of the editing surfaces on 2026-08-09 confirmed it from
the outside: the editor exposes **no command at all** for header/footer editing,
footnote/endnote insertion or bodies, text-box text editing, or shape drawing —
the palette returns nothing for "text box", "footnote" or "shape".

This is the single largest remaining gap in the editor, and it is one dependency,
not four: header/footer, note bodies and text-box text all need the same thing —
an address that says *which* sub-document a position is in, threaded through
selection, hit-testing, the edit ops, layout and the accessibility projection.
Inserting a footnote before that exists would create a note body the user cannot
type into, which is why note *insertion* is deliberately still not wired up even
though the engine ops for it exist.

## 8c. Competitive research round 2 — LibreOffice, and how the model addresses a sub-document (2026-08-09)

§10 compares OnlyOffice, Word and Google Docs on *interaction*. Two things it does
not cover turn out to matter more than any of the open questions: **LibreOffice
Writer**, which is this project's own layout oracle and was missing from the
survey, and **how any of these editors actually addresses a position inside a
header, note or text box** — the question the `Location` keystone exists to answer.

### 8c.1 LibreOffice Writer's header/footer affordance

Writer does not use Word's bare double-click. Clicking above the top margin
raises a **header marker with a `+` button**; clicking `+` creates and enters the
header. Once it exists the marker carries a **down-arrow** opening a menu to
format or delete it. Exit is `Esc` or a click in the body — matching Word and
Docs.

Worth adopting the marker: it makes an *absent* header discoverable, which the
pure double-click of Word/Docs does not. A user who has never been told that
double-clicking the margin does something will never find it. That is the same
"one-surface-only" reachability failure the command-surface audit kept finding,
in a different disguise.

### 8c.2 How LibreOffice addresses sub-document positions — and why it matters here

Writer keeps **one node array** (`SwNodes`) divided into fixed ranges, with
boundary accessors: `GetEndOfPostIts()`, `GetEndOfInserts()` (footnotes and
endnotes), `GetEndOfAutotext()` (**Flys — floating frames and text boxes — plus
headers and footers**), `GetEndOfRedlines()`, and `GetEndOfContent()` (body text).
Header, footer, footnote and text-box content live in the *same* array as body
text, in dedicated ranges.

The consequence is the important part: a cursor (`SwPaM`) is a node index plus an
offset, and that is the *whole* address. Writer has no "which sub-document am I
in" type threaded through its editing layer, because a node index already answers
it. The ranges are a property of *where a node sits*, looked up when needed — not
a coordinate the cursor carries.

### 8c.3 This model already has the same property

Checked against the tree at `4754e1e`:

- `HeaderFooter`, `Note` and `Comment` each hold `blocks: Vec<BlockNode>` — the
  *same* recursive block model as the body, not a parallel representation.
- `Document::validate` records the ids of every block inside every header,
  footer, note and comment into the **same document-wide uniqueness set** as body
  ids (`document.rs`, `record_block_ids` over `definitions.headers` /
  `footers` / `notes` / `comments`).

So `Pos { node, offset }` **already uniquely addresses a position inside a header,
footer, note or comment**, exactly as a node index does in Writer. The address
type this design has been waiting on largely already exists.

What is actually missing is narrower than "a new address threaded everywhere":

1. **Op resolution is hardcoded to the body.** 56 sites in `casual-doc-edit`
   resolve through `doc.body_mut()`, and the crate contains **zero** references to
   `definitions.headers`, `footers` or `notes`. The resolution helpers themselves
   are already surface-agnostic — `find_paragraph(blocks, id)` takes any block
   slice — so what is body-specific is the *entry point*, not the traversal.
2. **Hit-testing and layout** must map a click in the header band, a note, or a
   text box to those nodes and back.
3. **A "which surface does this node live in" lookup**, for gating and for the
   things that genuinely differ per surface (per-section headers, note numbering).

That is a materially smaller and better-founded piece of work than the phrasing
elsewhere in this document implies, and it removes the argument for inventing a
`Location` enum that every op signature has to carry. It should be re-scoped
before Phase B is planned, and Q9 (phasing granularity) should be answered with
this in hand.

**Sources.** LibreOffice help, "About Headers and Footers"
(help.libreoffice.org/6.2/en-US/text/swriter/guide/header_footer.html); LibreOffice
Getting Started Guide 26.2, ch. 2; LibreOffice `SwNodes` class reference
(docs.libreoffice.org/sw/html/classSwNodes.html) for the node-array ranges and
their accessors.

## 9. Open questions for owner review

Three questions are now **DECIDED** (recorded 2026-08-01) and folded into the
phasing/ops/tracker above; kept here for the decision record. The rest remain
open and gate implementation.

- **Q1 — Object interaction grammar exact keys.** *(open)* Confirm the two-step
  Escape (edit-mode → object-selected → text-caret) and whether Tab/Shift+Tab
  traverses objects on a page (§4). These set muscle-memory expectations. (§10
  shows all three reference editors use single-click→handles and double-click→
  enter-text; none has a strong Tab-traversal convention, so that part is a
  genuine choice.)
- **Q2 — Context bar vs. contextual ribbon tab.** *(open)* Does object selection
  surface a floating context bar (Google Docs), a contextual ribbon tab (Word
  "Picture Format" / OnlyOffice right sidebar), or both? doc 69 already chose
  "disable, don't hide" for contextual tabs — confirm the object case reuses
  that. (§10 synthesis leans to a floating context bar for common actions + a
  reusable properties surface for the long tail.)
- **Q3 — Model additions for image crop + inline alt-text. DECIDED: do the
  prerequisite model slice.** Add crop (`a:srcRect`) and inline-`Drawing` `descr`
  to the model with import/export/layout as **Phase 0**, *before* Phase A object
  editing, so `SetImageCrop`/`SetObjectDescr` ship with Phase A (§1, §5.4;
  `P1G-OBJ-MODEL` runs first in §11).
- **Q4 — "Basic shapes" scope. DECIDED: include inserting new basic shapes.**
  Phase A authors new shapes, not only edits existing ones (§5.6). The insert
  gallery is bounded to what `ShapeGeometry` + layout faithfully render/export
  (rectangle / rounded-rectangle / ellipse / line / modeled `Other` presets);
  rotation/flip, the full autoshape preset gallery, and custom geometry remain
  deferred (§5.7).
- **Q5 — Object edits under Suggesting mode. DECIDED: block, fail-closed.** Every
  object op is blocked in Suggesting mode with the existing "cannot be tracked
  yet" status, matching `P1G-REVIEW-042`; object changes have no OOXML revision
  markup and are never applied silently untracked (§6).
- **Q6 — `title_page` / section running toggles.** *(open)* Extend `SetSectionGeometry`
  (which today deliberately leaves headers/footers untouched) or add a dedicated
  section-running-properties op? A dedicated op is cleaner but adds surface.
- **Q7 — Link-to-previous semantics.** Approve modelling linkage purely as
  `HeaderFooterRef` presence/absence (§8.4, recommended), or add an explicit
  `link_to_previous` field?
- **Q8 — Where does a new image's bytes come from?** The engine owns no image
  decode/encode; `InsertObject`/`ReplaceMedia` require the host to supply bytes +
  content-type. Confirm the host-provides-bytes contract (consistent with
  AGENTS.md "hosts own resources").
- **Q9 — Phasing granularity.** Is the A-then-B split at the right grain, or
  should text-box *content* editing (pure locator reuse) ship as an early
  sub-slice ahead of object move/resize?

## 10. Competitive editing-experience comparison

The owner's chief concern is that we follow *proven* object- and header/footer-
editing patterns rather than inventing. This section surveys the three editors we
benchmark against — **OnlyOffice Document Editor** (closest architecturally: an
OOXML-native, canvas-rendered editor), **Microsoft Word** (the fidelity oracle;
doc 69 is our detailed Word competitive guide), and **Google Docs** (the browser
interaction baseline; doc 58) — then synthesizes what we adopt vs. deliberately
differ given our engine-drawn-selection / model-as-truth / canvas architecture.
Sources are cited at the end of this section.

### 10.1 Object editing (image / shape / text box)

| Aspect | OnlyOffice | Microsoft Word | Google Docs |
| --- | --- | --- | --- |
| **Insert** | Insert tab → Image (from file / URL / storage); Shape (autoshape gallery); Text box | Insert → Pictures / Shapes (≈160 preset gallery) / Text Box | Insert → Image; Insert → Drawing (a separate canvas dialog) for shapes/text boxes |
| **Selection + handles** | Square edge/corner resize handles + a **green circular rotation handle**; alignment guides while moving | Border with sizing handles + a rotation handle at top; Shift = keep proportions, Ctrl = keep center, Shift+rotate = 15° steps | Blue-dot handles; Shift = keep aspect ratio; rotation via Image options |
| **Resize modifier** | Shift + corner = proportional | Shift = proportional, Ctrl = from center | Shift = aspect ratio |
| **Context surface** | **Right-hand settings sidebar** (Size incl. Crop, Opacity, Rotation, Wrapping Style, **Replace Image**) + right-click "Advanced Settings" | **Contextual ribbon tab** ("Picture Format" / "Shape Format"), Arrange group (Rotate, Wrap Text, Position) + right-click Wrap Text menu | **Floating toolbar** under the image + "Image options" side panel (Size & Rotation, Text Wrapping, Recolor/Adjustments) |
| **Wrap-text modes** | inline, square, tight, through, top and bottom, in front, behind | In Line with Text, Square, Tight, Through, Top and Bottom, Behind Text, In Front of Text | Inline, Wrap text, Break text, Behind text, In front of text |
| **Enter text-box editing** | Double-click the box (border goes solid = selected; caret inside = editing) | Double-click the text box to edit its text | Text boxes live inside the Drawing dialog; edit re-opens that dialog |
| **Replace image** | Replace Image (sidebar or right-click) | Right-click → Change Picture | Right-click → Replace image |

**The three agree on the core grammar:** single-click selects the object and
shows handles; drag a handle resizes (Shift constrains); a rotation handle
rotates; double-click a container enters its text; a context surface exposes
Replace / Wrap / Crop / properties. Our §4 grammar matches this. The wrap-mode
vocabularies are near-identical and map **directly onto our `WrapMode` enum**
(`Square/Tight/Through/TopAndBottom/None` + `behind_doc`) — no invention needed.

### 10.2 Header/footer editing

| Aspect | OnlyOffice | Microsoft Word | Google Docs |
| --- | --- | --- | --- |
| **Enter** | Double-click the top/bottom page margin, or Insert → Header/Footer | Double-click the header/footer region, or Insert → Header/Footer → opens the "Header & Footer" contextual tab | Double-click the top/bottom of the page, or Insert → Headers & footers |
| **Exit** | Click back in the body | **Close Header and Footer** button, or **Esc** | Click back in the body |
| **First-page variant** | "Different first page" option | **Different First Page** toggle (Header & Footer tab) | **Different first page** checkbox |
| **Odd/even variant** | Different odd/even | **Different Odd & Even Pages** toggle | (not offered) |
| **Section linkage** | Per-section headers with linking | **Link to Previous** toggle; requires a section break; each variant (first/odd/even) links separately | **Link to previous** checkbox, appears once the doc has section breaks |

**All three** treat header/footer editing as an **editing-context switch** — a
double-click into the margin band, dim/de-emphasize the body, edit in place, exit
by Esc or clicking the body — **not** a modal dialog and **not** a document
mutation in itself. Word and Google Docs both gate distinct section headers behind
a **section break + a "Link to Previous"/"Different first page" toggle**, and both
model linkage as *inheritance you opt out of*. This is exactly the design in §8
(entering is selection/host state, not an op; linkage is `HeaderFooterRef`
presence/absence) — again, proven, not invented.

### 10.3 What we adopt vs. deliberately differ

**Adopt (proven, and our model already supports it):**

- The **object grammar**: single-click → handles + context surface; drag handle →
  resize (Shift constrains proportions); double-click container → enter text;
  Delete removes the object (§4).
- The **wrap-mode vocabulary** verbatim (it *is* our `WrapMode` enum) and a
  right-click / context-bar **Wrap** submenu (§5.3).
- **Crop, Replace Image, Alt-text** as first-class image context actions (all
  three editors expose these; Phase 0 + §5.4 make them real for us).
- Header/footer as an **in-place context switch** entered by double-clicking the
  margin band and exited by Esc/body-click; **Different First Page**, **Different
  Odd & Even**, and **Link to Previous** as the section-variant/linkage controls
  (§8) — Word's exact control names, since doc 69 already tracks Word as the
  fidelity oracle.

**Deliberately differ (forced by our architecture, doc 56/58):**

- **We draw every handle and outline ourselves** from engine geometry
  (`objectRect`/`objectHandles`, §3.3). We **cannot** reuse any of these editors'
  DOM/SVG-overlay handle implementations — Word/OnlyOffice are native canvas apps
  and Google Docs overlays DOM handles on its own layout box; our selection chrome
  is rastered from the same `LayoutSnapshot` the page came from, so it matches the
  glyphs exactly and never drifts. This is a strength (zero overlay-vs-content
  drift) but means handle hit-testing is our own `objectAt` math, not the
  browser's.
- **The model is the single source of truth, never the DOM** (doc 67 row 10). A
  drag-resize previews as host chrome and commits **one** `SetExtent`/`SetAnchor`
  op at drag-end (§5.3); we do not, like a DOM editor, mutate a live element and
  reconcile afterward.
- **Shape authoring is bounded to what we can faithfully render + round-trip**
  (§5.6): our insert gallery is the modeled `ShapeGeometry` primitives, not Word's
  ~160-preset gallery, and there is **no rotation/flip authoring** yet — we would
  rather ship a small, faithful set than fake presets that export as rectangles.
  This is a conscious fidelity-over-breadth difference from all three references.
- **Text boxes edit in place**, like OnlyOffice/Word (caret inside the box), *not*
  behind a separate Drawing dialog like Google Docs — our text-box body is
  ordinary block content flowed by the shared pipeline (docs 47/52), so in-place
  editing is the natural (and higher-fidelity) fit.
- **Object edits are blocked in Suggesting mode** (§6, Q5) — none of the three
  reference editors tracks object-geometry changes either, so blocking (rather
  than silently untracked mutation) is both faithful and safer.

**Sources:** ONLYOFFICE Document Editor Help — Insert images
(`helpcenter.onlyoffice.com/docs/userguides/document_editor/InsertImages.aspx`),
Insert autoshapes, Insert text objects; Microsoft Support — "Change the size of a
picture, shape, text box, or WordArt", "Rotate or flip a text box, shape, WordArt
or picture", "Link to previous", "Create different headers or footers for odd and
even pages"; Google Docs Editors Help — "Insert & edit images/drawings", "Wrap
text around images", "Use headers, footers, page numbers & footnotes". Cross-refs:
doc 69 (Word competitive guide) and the OnlyOffice capability analysis referenced
there.

## 11. Proposed tracker breakdown (design-first; all Not started / Designing)

Reordered to reflect the recorded decisions: the **Phase 0 model slice runs
first** (crop + inline alt-text, Q3), Phase A now includes **new-shape
insertion** (Q4), and every object op **fails closed in Suggesting mode** (Q5).

Phase 0 — prerequisite model slice (`P1G-OBJ-MODEL`, runs before Phase A object
ops):

- `P1G-OBJ-MODEL` — (DECIDED, Q3) model + import + export + layout for image crop
  (`a:srcRect`) and inline-`Drawing` alt-text (`descr`); the foundation
  `SetImageCrop`/`SetObjectDescr` build on.

Phase A (`P1G-OBJ-*`):

- `P1G-OBJ-DESIGN` — this doc; owner sign-off on the remaining open questions
  (Q1/Q2/Q6/Q7/Q8/Q9) gates the ops below.
- `P1G-OBJ-SELECT` — `HitTarget::{InlineObject,Float}` + `Selection::Object`,
  `objectAt`/`objectRect`/`objectHandles`, engine-drawn outline + handles (doc 58
  arms; read-only, no ops).
- `P1G-OBJ-GRAMMAR` — the §4 interaction grammar + object context bar via doc 84
  `editorCommands(object)` (§10 adopt list).
- `P1G-OBJ-GEOMETRY` — `SetAnchor` + `SetExtent` (move/resize/wrap/z-order) with
  undo + export fixed point; blocked in Suggesting mode (Q5).
- `P1G-OBJ-STRUCTURE` — `InsertObject`/`DeleteObject` + `InsertShape` (new basic
  shapes, Q4) + the §5.1 `Location` locator generalization.
- `P1G-OBJ-TEXTBOX` — text-box content editing (locator reuse) +
  `SetTextBoxBody` + `SetShapeFillStroke`.
- `P1G-OBJ-IMAGE` — `ReplaceMedia` + `SetImageCrop` + `SetObjectDescr` (on the
  Phase 0 model); host-bytes contract (Q8).

Phase B (`P1G-HF-*`, each depends on Phase A):

- `P1G-HF-CONTEXT` — `RunningZone` hit target, enter/exit header-footer editing
  context (selection/host state, no op) — the §10 in-place context switch.
- `P1G-HF-CONTENT` — edit header/footer paragraphs + contained objects (Phase A
  reuse via `Location::Header/Footer`).
- `P1G-HF-VARIANTS` — first-page / even-odd targeting; `SetSectionRunningRef` +
  `CreateHeaderFooterBody`; `title_page` toggle (Q6).
- `P1G-HF-LINK` — "same as previous" link/unlink via ref presence/absence (Q7).

**The recorded decisions (Q3/Q4/Q5) are final; the remaining open questions
(Q1/Q2/Q6/Q7/Q8/Q9) still gate the code.** Per AGENTS.md, this doc is the design
gate; code follows a finalized design.
