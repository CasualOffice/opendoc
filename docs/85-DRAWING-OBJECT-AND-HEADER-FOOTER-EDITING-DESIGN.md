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
**Owner decision required:** the Open Questions in §9 gate implementation.

## 1. Purpose and phasing

Body and table-cell paragraph text is editable today; **headers/footers, notes,
text boxes, and drawing objects are not general editing surfaces** (doc 67 row
10). This design closes that gap in two phases, in a deliberate order:

- **Phase A — drawing-object editing:** insert / select / move / resize / delete
  for images, text boxes, and basic shapes; image replace / crop / alt-text /
  wrap-mode; text-box text editing and body properties; and the one *object
  interaction grammar* shared by every object kind.
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

### 5.4 Image content ops

- `ReplaceMedia { object: NodeId, media: MediaId }` — swap the image; the new
  bytes enter the package/preservation side-table and a new `MediaReference` is
  registered in `Definitions.media` (a host-provided image → new `MediaId`).
  Self-inverse with the previous `MediaId`.
- `SetObjectDescr { object: NodeId, descr: Option<String> }` — alt-text. **Blocked
  on a model change** for inline `Drawing` (it has no `descr` field today);
  `AnchoredDrawing`/`GroupPicture` already carry `descr`. See §9-Q3.
- `SetImageCrop { object: NodeId, crop: CropRect }` — **blocked on a model
  addition**: crop (`a:srcRect`) is not modeled on any picture type. This op is
  *designed here but gated* behind adding a `crop` field + import/export/layout
  support; it is the one Phase A item that is not a pure edit-layer change. See
  §9-Q3 and the phased split in §10 (`P1G-OBJ-MODEL` precedes it).

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

### 5.6 What is intentionally out of scope for Phase A

Rotation/flip, custom `ShapeGeometry` (beyond the 5-value enum), floating position
for `EmbeddedObject`, group re-parenting / child add-remove, and drawing new
shapes from scratch. Each needs a model extension and is a later slice; naming
them keeps "basic shapes" honest (§9-Q4).

## 6. Undo / transaction semantics and tracked changes

- **Undo:** every new op returns an exact inverse via the same `apply` contract;
  a move/resize/replace is one `HistoryEntry` (one undo step) with a new
  `HistoryKind` label for a readable Undo menu. No coalescing across distinct
  object gestures (unlike typing).
- **Selection remap:** after an object op the selection stays `Object(node)`
  (node identity is stable); a delete collapses to a `Caret` at the object's
  former anchor. No `PositionMap` byte-remap is needed for pure object ops (they
  don't shift paragraph text), which keeps them simpler than text edits.
- **Tracked changes (Suggesting mode):** OOXML has **no revision markup for
  object move/resize/replace/property changes**, and Word itself does not track
  most object-geometry edits. Consistent with `REVIEW-GAP-009`'s structural-
  tracking backlog, **object edits are not tracked in this phase.** Two safe
  options (owner decides, §9-Q5): (a) **block** object edits in Suggesting mode
  with the existing "cannot be tracked yet" status (fail-closed, matching
  `P1G-REVIEW-042`), or (b) **apply untracked** with a visible note. Default
  recommendation: **block in Suggesting mode** for a truthful "no silent
  untracked mutation" guarantee.

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

## 9. Open questions for owner review

- **Q1 — Object interaction grammar exact keys.** Confirm the two-step Escape
  (edit-mode → object-selected → text-caret) and whether Tab/Shift+Tab traverses
  objects on a page (§4). These set muscle-memory expectations.
- **Q2 — Context bar vs. contextual ribbon tab.** Does object selection surface a
  floating context bar (Google Docs), a contextual ribbon tab (Word "Picture
  Format"), or both? doc 69 already chose "disable, don't hide" for contextual
  tabs — confirm the object case reuses that.
- **Q3 — Model additions for image crop + inline alt-text.** Crop (`a:srcRect`)
  and inline-`Drawing` `descr` are not modeled. Approve a small model/import/
  export slice (`P1G-OBJ-MODEL`) as a prerequisite for `SetImageCrop` /
  `SetObjectDescr`-on-inline, or descope crop/inline-alt from Phase A?
- **Q4 — "Basic shapes" scope.** With `ShapeGeometry` a coarse 5-value enum and no
  rotation/flip/custom geometry, "shape editing" in Phase A means move/resize/
  delete/fill/outline of existing shapes only — **not** drawing new shapes or
  editing geometry. Confirm that boundary.
- **Q5 — Object edits under Suggesting mode.** Block (fail-closed, recommended)
  or apply-untracked-with-a-note? Object geometry/property changes have no OOXML
  revision markup.
- **Q6 — `title_page` / section running toggles.** Extend `SetSectionGeometry`
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

## 10. Proposed tracker breakdown (design-first; all Not started / Designing)

Phase A (`P1G-OBJ-*`):

- `P1G-OBJ-DESIGN` — this doc; owner sign-off on §9 gates everything below.
- `P1G-OBJ-SELECT` — `HitTarget::{InlineObject,Float}` + `Selection::Object`,
  `objectAt`/`objectRect`/`objectHandles`, engine-drawn outline + handles (doc 58
  arms; read-only, no ops).
- `P1G-OBJ-GRAMMAR` — the §4 interaction grammar + object context bar via doc 84
  `editorCommands(object)`.
- `P1G-OBJ-GEOMETRY` — `SetAnchor` + `SetExtent` (move/resize/wrap/z-order) with
  undo + export fixed point.
- `P1G-OBJ-STRUCTURE` — `InsertObject`/`DeleteObject` + the §5.1 `Location`
  locator generalization.
- `P1G-OBJ-TEXTBOX` — text-box content editing (locator reuse) +
  `SetTextBoxBody` + `SetShapeFillStroke`.
- `P1G-OBJ-IMAGE` — `ReplaceMedia`; host-bytes contract (Q8).
- `P1G-OBJ-MODEL` — (gated by Q3) model+import+export for crop + inline alt-text;
  then `SetImageCrop`/`SetObjectDescr`.

Phase B (`P1G-HF-*`, each depends on Phase A):

- `P1G-HF-CONTEXT` — `RunningZone` hit target, enter/exit header-footer editing
  context (selection/host state, no op).
- `P1G-HF-CONTENT` — edit header/footer paragraphs + contained objects (Phase A
  reuse via `Location::Header/Footer`).
- `P1G-HF-VARIANTS` — first-page / even-odd targeting; `SetSectionRunningRef` +
  `CreateHeaderFooterBody`; `title_page` toggle (Q6).
- `P1G-HF-LINK` — "same as previous" link/unlink via ref presence/absence (Q7).

**None of these are implementation-ready until the §9 owner decisions are
recorded.** Per AGENTS.md, this doc is the design gate; code follows a finalized
design.
