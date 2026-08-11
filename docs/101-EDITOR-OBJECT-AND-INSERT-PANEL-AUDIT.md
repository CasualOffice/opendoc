# 101 — Editor Object and Insert-Panel Audit

**Status:** Proposed design for owner review; documentation-only audit.

**Date:** 2026-08-12

**Priority:** document safety and editing correctness first; SDK and OT/CRDT are
explicitly outside this work.

**Depends on:** docs 52, 58, 63, 64, 67, 69, 84, 85, 86 (typed OMML), 87, and
99.

## 1. Executive conclusion

The current drawing-object experience is not production-ready. The first
problem is not missing visual polish: **the host offers mutations that the
engine cannot perform for the selected model node**. A newly inserted shape is
the clearest example. It is represented as a `WordprocessingGroup` containing a
`GroupChild::Shape`, but the layout exposes the child id as the selected object.
Fill and outline can target that child; resize, move, wrap, delete, crop, and alt
text cannot. The UI nevertheless displays handles, wrap choices, Alt text, and
Delete. A user can therefore perform a valid-looking gesture that fails after
the fact.

The second problem is interaction architecture. A large floating context bar
places all six wrap modes plus object actions over the page; symbol, emoji, and
alt-text editors are modal overlays; and there is no persistent object
inspector. This obscures the document precisely when the user needs to compare
an edit with its result. The existing Paragraph and Table properties panels
already establish the correct nonmodal, live-inspector pattern, but objects and
insert tools do not use it.

The required order is:

1. make selection identity and mutation capabilities truthful;
2. fix resize, surface addressing, and crop-state correctness;
3. add one contextual right-side Properties inspector;
4. migrate symbols and emoji to a nonmodal Insert panel;
5. design an authoritative equation-editing model before exposing equation
   authoring;
6. then add advanced selection, arrangement, grouping, and alignment.

This order is deliberate. Moving broken commands into a better panel would make
the product look more complete while preserving unsafe behavior.

## 2. Audit method and evidence boundary

This audit inspected the current `main` implementation rather than relying on
tracker labels:

- object selection, resize, move, crop, context-bar, alt-text, symbol, and emoji
  flows in `webapp/src/main.js` and `webapp/editor.html`;
- inspector and responsive behavior in `webapp/src/style.css`;
- layout-to-model object correlation and WASM reads/mutations in
  `crates/casual-doc-wasm/src/lib.rs`;
- command targets, inverses, and object resolvers in
  `crates/casual-doc-edit/src/lib.rs`;
- object browser coverage in `webapp/tests/e2e/object-*.spec.mjs`,
  `shape-editing.spec.mjs`, `insert-object.spec.mjs`, and
  `text-box-editing.spec.mjs`;
- the retained-OMML authority defined by doc 86;
- official product documentation listed in §6.

This is a code-path audit, not a claim that every imported DOCX producer variant
has been manually exercised. The first implementation slice must turn each P0
finding into a minimal browser reproduction and an engine regression before it
changes behavior.

Two disposable Playwright diagnostics were also run against the current built
WASM on 2026-08-12 (the temporary spec was removed after the run; 2/2 passed):

- Insert Rectangle produced a selected `shape` with eight handles; dragging SE
  yielded the visible “not supported for this selection yet” error and left the
  outline unchanged.
- On a valid top-level picture, dragging W left left the selected outline's left
  edge unchanged and moved its right edge farther right, confirming that the
  gesture changes the wrong edge rather than preserving the opposite edge.

## 3. Current capability truth table

Legend: **Yes** means a compatible read and command path exists for that exact
selected identity; **No** means the UI currently exposes or implies a capability
that cannot be committed; **N/A** means the operation is structurally
inapplicable. “Body” includes nested body blocks only where the resolver
actually descends into them.

| Selected model identity | Select | Resize | Move / wrap | Delete | Alt text | Kind properties |
| --- | --- | --- | --- | --- | --- | --- |
| Top-level body inline `Drawing` | Yes | Yes | N/A | Yes | Yes | Crop, but current-crop read is absent |
| Top-level body `AnchoredDrawing` | Yes | Yes | Yes | Yes | Yes | Crop, but current-crop read is absent |
| Body inline `TextBox` | Yes | Yes | N/A | Yes | **No, but shown** | Content editing yes; body properties absent |
| Body floating `TextBox` | Yes | Yes | Yes | Yes | **No, but shown** | Content editing yes; body properties absent |
| Body anchored group root | Layout may expose children instead | Group extent path incomplete | Root anchor exists | Root can be deleted | No | No root inspector contract |
| `GroupChild::Shape` | Yes | **No, handles shown** | **No, wrap/move shown** | **No, shown** | **No, shown** | Fill and stroke only |
| `GroupChild::Picture` | Yes when layout supplies child id | **No, handles shown** | **No, wrap/move shown** | **No, shown** | **No, shown** | **Crop shown but unsupported** |
| Header/footer drawing | Float selection deliberately filtered; inline correlation incomplete | Body-only | Body-only | Structural delete can resolve owning surface | Setter resolves; getter is body-only | Inconsistent |
| Object in a table-cell fragment | Model resolver may descend | Mutation may descend within body | Mutation may descend within body | Mutation may descend within body | Mutation may descend within body | Selection correlation does not cover the nested placed-fragment shape |

The table exposes the architectural defect: selection, reads, and mutations do
not share one target-resolution contract. The frontend guesses capability from
`kind` and an `anchored` boolean, although the actual answer also depends on the
root/child identity, owning surface, model fields, and command support.

## 4. Defect catalogue

### 4.1 P0 — controls can target an unmutable object

`object_boxes()` now admits a placed group child when
`body_contains_group_child()` finds its id. The returned object is marked
`anchored: true`. The host therefore paints eight handles, makes the body
draggable, shows all wrap choices, and adds Alt text and Delete.

The corresponding command resolvers have a different target set:

- `object_authored_extent` / `SetExtent` resolve `Drawing`,
  `AnchoredDrawing`, and `TextBox`, but not a group child;
- `object_anchor` / `SetAnchor` resolve a top-level anchored group, not its
  child;
- `DeleteObject` removes an `InlineNode`, not a `GroupChild`;
- `SetObjectDescr` and crop resolve drawing nodes, not `GroupPicture` or
  `GroupShape`;
- only the shape fill/stroke resolver intentionally traverses group children.

Inserted shapes reliably enter this state: insertion creates a group-of-one and
the host selects the new child id from `objectOrder()`. This is a release-blocking
capability-truth defect because a normal first-party creation flow produces it.

**Required correction:** introduce the target/capability contract in §7 before
adding any new object command. Until then, unsupported controls must not be
interactive. A temporary fail-closed UI is acceptable; a gesture that appears
to succeed and later warns is not.

### 4.2 P0 — four resize handles have incorrect geometry semantics

The host records `startX` and `startY`, but `updateObjectResize()` changes only
preview width and height. `finishObjectResize()` commits only `SetExtent`.
Consequences:

- W, NW, and SW change width while leaving the left edge fixed;
- N, NW, and NE change height while leaving the top edge fixed;
- the opposite edge is not preserved;
- the preview and committed layout can disagree about which edge the user
  dragged;
- an anchored object's required position and extent changes are not one atomic
  transaction.

The browser suite covers SE and E only. It proves size deltas, not edge
invariants. There is no regression for N, W, NW, NE, SW, or S, no minimum-size
crossing case, and no assertion that one undo restores both position and size.

**Required correction:** the engine must return or consume a per-handle resize
recipe. Every offered handle must keep its opposite edge/corner invariant after
commit. For floats, N/W handles require one atomic anchor-plus-extent action. If
an inline or nested object cannot honor a handle, that handle is not a declared
capability and must not be painted.

### 4.3 P0 — object operations are not surface-complete

`SetExtent`, `SetAnchor`, and `SetImageCrop` still start at `doc.body_mut()`.
Structural deletion and alt-text mutation were later generalized through
`on_owning_surface_mut`, but the alt-text read helper still starts at
`document.body()`. Selection also filters header/footer floats because the
geometry mutations cannot resolve them.

The result is a split contract: an object can render in a header/footer, be
editable for text or fill/stroke, and still be unavailable or incorrectly
prefilled for another property. “Surface-agnostic addressing” is therefore not
complete.

**Required correction:** every object read and mutation must resolve the same
stable object reference across body, table cell, SDT, text-box body,
header/footer, and supported note surfaces. One operation × surface matrix must
exercise the actual model carrier, not only a one-page top-level sample.

### 4.4 P0 — crop can overwrite existing document intent

The crop UI explicitly initializes every session to zero because WASM exposes a
setter but no crop getter. Applying a new crop therefore replaces any imported
`a:srcRect`; it cannot expand, restore, or refine the existing crop accurately.
For a group picture, the selected child can also receive a Crop button although
the mutation resolver does not support that carrier.

**Required correction:** add a bounded `imageCrop(object)` read using the same
resolver as `SetImageCrop`, initialize the session from it, and prove
imported-crop → adjust → undo → export/reopen. If source dimensions are required
for an operation, expose them explicitly; do not infer an uncropped source from
the displayed box.

### 4.5 P1 — the floating context bar obscures the document

For every anchored object the bar renders six textual wrap buttons, a hint,
divider, and all kind actions above the selected object. Positioning clamps only
the top and left to 8 px. It does not clamp the right/bottom, flip below the
object, or avoid the selected content. On a narrow viewport or an object near an
edge, the control can overflow or cover the document.

The quick bar should remain only as a small accelerator: kind label, primary
action, current wrap summary, and “Properties”. Long-lived settings belong in
the inspector. The quick bar must be viewport-clamped on all four edges and
place above/below according to available space.

### 4.6 P1 — properties are scattered and incomplete

- shape Fill and Outline live in transient popovers;
- image Crop is direct manipulation, but Replace, border/effects, actual size,
  rotation, and flip are absent;
- text-box body margins, vertical alignment, autofit, and overflow have no
  command or UI;
- exact W/H, aspect lock, X/Y position, reference frames, wrap distances,
  arrange, and accessibility are not available in one place;
- z-order exists in anchor data but has no complete UI;
- there is no selection/layer pane, multi-select, grouping, snapping, guides,
  or alignment/distribution workflow.

This is why incremental context-bar additions will not scale. The product needs
one typed inspector whose sections are derived from capabilities.

### 4.7 P1 — modal insertion interrupts visual comparison

Symbol and emoji pickers are modal overlays with a trapped tab sequence. They
keep the picker open for repeated insertion, which is good, but the modal makes
the document inert and visually obscures it. That conflicts with a repeated,
exploratory insert task. WAI-ARIA's modal dialog pattern correctly makes the
underlying window inert; the problem is choosing a modal for a task that does
not require exclusivity.

There is no equation insert/editor UI. Adding another modal would repeat the
same interaction problem and would also bypass the unresolved model-authority
problem in §9.

### 4.8 P1 — accessibility and keyboard reach are partial

Tab/Shift+Tab traverses objects after one is selected, but an object is not
generally reachable from an ordinary caret through one documented keyboard
path. Context controls depend on transient chrome. Group hierarchy, object
names, lock state, and z-order have no accessible tree. Status naming also must
come from the selected model target, not paint-kind heuristics.

The future Properties and Selection panels must use ordinary landmarks,
headings, form labels, disclosure controls, and roving list/tree navigation.
Focus entering a nonmodal panel must not clear the model selection. Escape
returns to the selected object; F6 cycles workspace regions. Destructive and
temporarily unavailable actions remain present with a discoverable reason;
structurally inapplicable actions are omitted.

## 5. Why the current tests remained green

The current suites verify narrow successful carriers:

- `object-geometry.spec.mjs` resizes a top-level picture from SE and E;
- `object-anchor.spec.mjs` moves/wraps/resizes one top-level floating picture;
- `shape-editing.spec.mjs` verifies child-shape selection and fill/stroke, not
  resize, move, wrap, delete, crop, or alt text;
- `text-box-editing.spec.mjs` verifies entry, typing, and Escape, not body
  properties or the advertised Alt text action;
- no object suite crosses body/table/header/footer carriers for the same op;
- crop tests do not begin with a non-identity imported crop.

The tests match implementation slices rather than the end-user capability
claim. Future gates must use the capability matrix in §11 so a new carrier
cannot inherit controls without inheriting their regressions.

## 6. Competitive analysis and adopted lessons

The objective is not visual imitation. These products validate the interaction
patterns users already understand; OpenDoc still needs stricter model and
round-trip contracts than a browser-only editor.

| Product evidence | Useful behavior | OpenDoc decision |
| --- | --- | --- |
| [Word: set text direction and position in a shape or text box](https://support.microsoft.com/en-US/Word/set-text-direction-and-position-in-a-shape-or-text-box-in-word) | Format Shape exposes text direction, vertical alignment, wrapping, internal margins, and autofit | Put text-box body properties in the contextual inspector after `SetTextBoxBody` exists |
| [Word: resize pictures, shapes, text boxes, or WordArt](https://support.microsoft.com/en-us/office/change-the-size-of-a-picture-shape-text-box-or-wordart-in-word-901ec456-33ce-40d3-828d-5552ee794aa1) | Direct handles coexist with exact dimensions and aspect controls | Keep direct manipulation; add exact model-owned fields and inverse-tested modifier behavior |
| [Word: wrap and move pictures](https://support.microsoft.com/en-US/Word/wrap-text-and-move-pictures-in-word) | Wrap, position, move-with-text/fix-on-page, and anchors are one geometry concept | Present a concise wrap summary near the object and the complete anchor contract in Properties |
| [Word Selection pane](https://support.microsoft.com/en-us/office/use-the-selection-pane-to-manage-objects-in-documents-a6b2fd3e-d769-46c1-9b9c-b94e04a72550) | Lists stacked/grouped objects; selection, ordering, visibility, locking, and keyboard operation are available | Add a later model-backed Selection mode; do not infer layers from DOM order |
| [Word for the web: Equation Tools](https://support.microsoft.com/en-us/education/onenote/create-equations-in-word-for-the-web) | A side panel provides a draft, Symbols, Structures, Recent, and “Insert on page” | Adopt for the bounded first equation-authoring slice, after the math authority design |
| [Google Docs image options](https://support.google.com/docs/answer/97447?hl=en-CA) | Layout plus Size & Rotation are exposed in a right sidebar | Adopt a contextual right inspector, while retaining our deterministic engine commands |
| [Google Docs emoji and special characters](https://support.google.com/docs/answer/3371015?hl=en-CA) | Searchable picker plus `@emoji` and `:query` in-document entry | Move the full picker to Insert; consider inline emoji completion after IME/shortcut collision review |
| [OnlyOffice image settings](https://helpcenter.onlyoffice.com/docs/userguides/document_editor/InsertImages.aspx) | Right sidebar includes size, crop, actual size, replace, and shape settings | Use object-specific sections in one inspector; expose only modeled commands |
| [OnlyOffice shape settings](https://helpcenter.onlyoffice.com/docs/userguides/document_editor/InsertAutoshapes.aspx) | Selection activates shape settings in the right sidebar | Context selects the inspector mode; panel focus does not clear object selection |
| [LibreOffice image properties](https://help.libreoffice.org/latest/en-US/text/swriter/01/05060000.html) | Position/size, options, wrap, crop, and borders are available together | Use the same complete property grouping, with simpler defaults and progressive disclosure |
| [WAI-ARIA modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) | A true modal makes the underlying window inert and traps focus | Reserve modal dialogs for exclusive decisions; insert browsing and live properties are nonmodal |

## 7. Target object architecture

### 7.1 Stable target identity

A `NodeId` plus host-inferred `kind`/`anchored` is insufficient. Introduce one
engine-owned reference returned by hit testing and object traversal:

```text
ObjectRef {
  surface,          // body, header, footer, note, nested text-box surface
  root,             // removable/movable OOXML object carrier
  subject,          // exact child whose kind properties are edited
  path,             // bounded group-child path when subject != root
  kind              // picture, text-box, shape, group, embedded
}
```

For a group-of-one shape, `root` is the `WordprocessingGroup` and `subject` is
the `GroupShape`. Geometry, anchor, and delete apply to the root; fill/stroke
apply to the subject. For a true multi-child group, initial selection targets the
group root. Enter/double-click can descend to a child only when the engine
returns a valid child target and capability set.

This reference is an internal engine/WASM contract first. It must not be frozen
as the public SDK until the editor proves it.

### 7.2 Engine-declared capabilities

Every selection/read response includes a bounded capability payload. Names are
illustrative; the design constraint is normative.

```text
ObjectCapabilities {
  resize_handles: [Handle],
  can_move, can_nudge, can_wrap, can_set_wrap_distances,
  can_delete, can_crop, can_replace_media, can_edit_alt_text,
  can_edit_text, can_set_text_body,
  can_fill, can_stroke, can_rotate, can_flip,
  can_arrange, can_group, can_ungroup,
  disabled_reasons: { capability -> reason }
}
```

Rules:

- capabilities are computed from the exact `ObjectRef`, owning surface, model
  support, current edit/review mode, and host policy;
- JS renders controls from this payload; it does not infer them from `kind`;
- structurally unsupported controls are omitted;
- temporarily blocked controls (Viewing, Suggesting, locked object, host policy)
  are disabled and expose the reason;
- a capability cannot become `true` without a command, inverse, read/reflection
  path, export/reopen proof where applicable, and browser test;
- an error returned after a declared-capable action is a product defect, not the
  normal unsupported path.

### 7.3 Geometry transaction

Direct manipulation remains preview-only in the host. Pointer-up commits one
engine command/transaction containing every changed property:

```text
ResizeObject { target, handle, start_rect, final_rect, preserve_aspect }
```

The engine validates the target and converts the final placed rectangle into
the carrier-specific extent, anchor, group transform, or supported combination.
Its inverse restores all values exactly. The engine may reject a handle before
the gesture begins; the host must not paint that handle.

Required invariants:

- E/W keep the opposite vertical edge fixed; N/S keep the opposite horizontal
  edge fixed; corners keep the opposite corner fixed;
- minimum and maximum sizes are deterministic and bounded;
- page/reference-frame bounds and negative offsets follow explicit policy;
- aspect behavior is kind-aware and keyboard modifiers are documented;
- cancel, pointer-cancel, blur, and Escape perform no mutation;
- pointer-up creates exactly one undo entry;
- undo restores position, size, crop/transform dependencies, selection, and
  export semantics.

### 7.4 One contextual Properties inspector

Add a right-side `workspace-inspector` with mutually exclusive modes:
Paragraph, Table, Picture, Shape, Text box, Group, and later Selection. Existing
Paragraph/Table panels migrate into this shell rather than remaining separate
panel managers.

Desktop behavior:

- docked right, 320 px default, bounded resizable range 288–420 px;
- document viewport reserves the width, so the inspector does not cover the
  page;
- selection changes switch the contextual inspector only if the user has not
  explicitly pinned another workspace tool;
- closing the inspector preserves selection and returns focus to the document;
- controls update live from engine reads after every transaction and undo/redo.

Narrow behavior:

- below the docking threshold, use a nonmodal right drawer or bottom sheet with
  an explicit close control;
- keep document scrolling and selection available where screen area permits;
- never claim `aria-modal=true` or trap focus unless the document is actually
  inert;
- at phone widths the sheet may cover part of the page, but it must be
  dismissible, restore focus, and retain the selected model target.

Common property sections:

1. **Identity and accessibility:** kind/name, alt text or decorative state only
   for carriers that model it.
2. **Size:** W/H, aspect lock, actual size where media metadata exists.
3. **Position:** X/Y, horizontal/vertical reference, move with text/fix on page,
   rotation/flip only when modeled.
4. **Layout:** wrap mode, wrap distances, overlap policy.
5. **Arrange:** forward/back, front/back, align/distribute when supported.

Kind sections:

- Picture: Replace, Crop, Reset crop, border/effects once authored.
- Shape: fill, stroke colour/weight/dash/ends, geometry, rotation/flip once
  authored.
- Text box: fill/stroke plus internal margins, vertical alignment,
  wrap-in-shape, autofit/overflow after `SetTextBoxBody`.
- Group: group-level geometry/arrangement and an explicit child-entry action;
  never blend root and child mutations silently.

Commit behavior follows the existing live inspector pattern: toggles/selects
commit on change; numeric inputs commit on Enter or blur and Escape restores the
reflected value; drags preview locally and commit once on release.

### 7.5 Compact object quick bar

Retain a small bar because it reduces travel for common actions, but limit it
to three or four controls:

- selected kind/name;
- one primary action (Crop, Edit text, or Properties depending on capability);
- current wrap summary as one menu button for floats;
- Properties / More.

Delete remains available from keyboard and the context menu; it does not need a
large permanent button above the content. The bar is placed using measured
geometry, clamps on all four viewport edges, flips above/below, and must not
overlap the selected object's center when another placement is available.

## 8. Target Insert panel

Add a nonmodal `Insert` mode to the same right workspace rail. It retains a
model `Pos` insertion target while focus moves into search/grid controls. A
document click updates the target; merely focusing the panel does not. Every
insert revalidates the target against the latest transaction mapping and fails
visibly if the position no longer exists.

### 8.1 Symbols and emoji

Migrate the existing data sets, search, categories, and roving grid into Insert
sections. Requirements:

- the page remains visible, scrollable, and selectable;
- selecting a glyph inserts at the current pinned caret and keeps the panel
  open;
- each glyph is one undoable transaction;
- Recent is model/host state with a documented bound, not an unbounded store;
- search includes name, code point for symbols, and category;
- Viewing disables insertion with a reason; Suggesting uses the tracked text
  path;
- native colour-emoji rendering remains a separate fidelity limitation. The
  picker must not imply that every emoji will render in colour.

Inline `:query` / `@emoji` completion is a later accelerator. It needs explicit
IME, bidi, autocomplete dismissal, and literal-colon behavior tests before it
becomes default.

### 8.2 Shapes and text boxes

The Insert panel can host the full shape gallery with search/categories and
recent shapes; the ribbon keeps a compact quick gallery. Insertion must select a
target whose capability payload is immediately truthful. A shape is not
considered inserted successfully if the resulting selection cannot at least be
deleted, undone, and resized through its declared handles.

Text-box insertion enters its text surface only after the root object and body
surface are both addressable. Empty text boxes must retain valid content and
remain removable/undoable.

## 9. Equation authoring design gate

Doc 86 intentionally treats raw imported OMML as the round-trip authority and
the typed expression as a bounded rendering projection. It explicitly excludes
editing and OMML synthesis. Therefore an Equation panel cannot safely ship as a
UI-only feature or as plain-text replacement.

Before implementation, approve a dedicated design/ADR that answers:

1. Is a newly authored equation's authority a typed `MathExpression`, retained
   OMML, or a dual representation with an explicit dirty state?
2. What deterministic OMML subset can the exporter synthesize?
3. How are stable ids, caret positions, placeholders, selection, and inverses
   represented inside fractions, scripts, radicals, delimiters, matrices, and
   future constructs?
4. What happens when editing an imported equation containing unsupported OMML?
   Preservation must be fail-closed; unsupported children cannot disappear.
5. Is linear input UnicodeMath, LaTeX, both, or an internal grammar, and which
   syntax subset is guaranteed?
6. How do copy/paste, accessibility math descriptions, limits, validation,
   Suggesting mode, and export/reopen work?

Recommended bounded first UX after that decision:

- Equation Tools in the Insert panel;
- a draft preview at the top;
- Symbols, Structures, and Recent tabs;
- typed placeholders and keyboard navigation in the draft;
- one “Insert on page” transaction;
- direct in-document equation caret editing deferred until stable position and
  replacement semantics exist.

Imported unsupported equations remain atomic and preserved. The UI must state
when an equation can be viewed but not safely edited.

## 10. Prioritized delivery plan

### Phase 0 — capability truth and document safety

| ID | Work | Exit condition |
| --- | --- | --- |
| UXOBJ-001 | `ObjectRef` + `ObjectCapabilities`; host renders from it | No selected carrier displays an action without a valid read/command/inverse path |
| UXOBJ-002 | Resolve group root vs child identity | Newly inserted and imported lone/grouped shapes can be selected, deleted, resized/moved only at the correct level, and undo exactly |
| UXOBJ-003 | Correct per-handle resize transaction | Every offered handle passes opposite-edge/corner, min-size, cancel, one-undo, export/reopen tests |
| UXOBJ-004 | Surface-complete object resolvers | Read/mutate matrix passes for body, table cell, nested text box, header, footer, and each supported note carrier |
| UXOBJ-005 | Current-crop read and preservation | Imported non-zero crop can be refined, reset, cancelled, undone, and round-tripped without losing prior intent |
| UXOBJ-006 | Truthful interim chrome and errors | Unsupported controls are absent; temporary policy blocks are disabled with reasons; no dead gesture remains |

### Phase 1 — contextual object inspector

| ID | Work | Exit condition |
| --- | --- | --- |
| UXOBJ-010 | Shared right inspector shell | Paragraph, Table, and object modes are mutually exclusive, responsive, focus-safe, and do not cover desktop pages |
| UXOBJ-011 | Common geometry/layout sections | Exact size/aspect, position/reference, wrap and distances reflect and mutate through transactions |
| UXOBJ-012 | Text-box body operation and UI | Insets, vertical anchor, autofit, and overflow pass semantic and visual round-trip gates |
| UXOBJ-013 | Picture properties | Replace-media contract, actual/reset size, crop state, and modeled border/effects are complete |
| UXOBJ-014 | Rotation/flip model and UI | Typed model, render, semantic round-trip, handles/fields, and inverses agree |
| UXOBJ-015 | Compact quick bar | Bounded actions, four-edge positioning, no narrow-screen overflow, keyboard parity |

### Phase 2 — insertion without document obstruction

| ID | Work | Exit condition |
| --- | --- | --- |
| UXINS-001 | Shared Insert panel + pinned model position | Panel focus preserves insertion target; document click updates it; stale positions fail visibly |
| UXINS-002 | Migrate symbol/emoji pickers | Nonmodal repeated insertion, search/categories/recent, keyboard and mobile coverage |
| UXINS-003 | Shape/text-box gallery | Newly created objects satisfy Phase-0 capability gates immediately |
| UXMATH-001 | Equation authority/operation design + ADR | Owner-approved model, preservation, syntax, selection, inverse, and export contract |
| UXMATH-002 | Bounded Equation Tools panel | Draft/structures/symbols/recent plus one safe Insert transaction for the approved subset |

### Phase 3 — advanced object parity

Selection/layer pane, multi-select, grouping/ungrouping, visibility/lock,
alignment/distribution, snapping/guides, connector editing, inline↔floating
conversion, richer picture effects, and custom geometry. These follow the stable
identity/capability contract; none should invent host-only object state.

## 11. Required acceptance matrix and CI gates

Every object mutation PR must identify rows from this matrix. “Not applicable”
needs an engine capability result, not an omitted test.

| Dimension | Required cases |
| --- | --- |
| Kind/carrier | inline picture, floating picture, inline text box, floating text box, group root, lone shape child, nested shape child, group picture |
| Surface | body paragraph, table cell, SDT, nested text-box body, header, footer, supported note surface |
| Geometry | all declared handles, move, nudge, exact field, aspect on/off, min/max, page/reference bounds |
| Lifecycle | create/import, select, mutate, cancel, undo, redo, save, reopen, delete, undo-delete |
| Modes | Editing success, Suggesting fail-closed unless tracked, Viewing disabled |
| Input | mouse, touch/pointer cancel, keyboard, zoom, scroll/virtualization |
| Viewport | desktop docked, tablet drawer, phone sheet, browser zoom 200% |
| Accessibility | labelled controls, focus return, F6/Tab order, status/error announcement, no false modal semantics |
| Fidelity | unknown data retained, existing crop/anchor/style fields preserved, semantic fixed point or explicit compatibility finding |

Minimum gates for Phase 0/1 behavior changes:

- edit-crate command/inverse tests and failed-operation no-mutation checks;
- WASM target/capability/read/reflection tests;
- import/export/reopen fixed points for every changed OOXML property;
- targeted Playwright reproductions for each defect and matrix carrier;
- frontend unit tests pinning capability-to-control rendering;
- keyboard and accessibility browser coverage;
- responsive measurements/screenshots for inspector and quick bar;
- formatting, strict Clippy, workspace tests/doc tests, rustdoc warnings denied,
  WASM check/build, MSRV check, and the complete frontend/browser gate required
  by docs 15.

No row becomes `Done` based only on a successful top-level body picture.

## 12. Explicit non-goals for the first implementation slice

- public SDK stabilization;
- OT/CRDT or collaborative object transforms;
- custom-shape geometry authoring;
- full OMML editing or arbitrary LaTeX compatibility;
- browser DOM/contenteditable as object or equation truth;
- AI-generated alt text or external media fetching in the runtime;
- host-only geometry, layers, properties, or recent-item state without an
  explicit ownership/persistence contract.

## 13. Recommended owner decisions

These recommendations let implementation proceed in reviewable increments:

1. **Approve** the capability-truth and group-identity Phase 0 before inspector
   visual work.
2. **Approve** one 320 px contextual right inspector, resizable on desktop and a
   nonmodal drawer/sheet on narrow screens.
3. **Retain** a compact quick bar, but remove the six-button wrap strip and all
   unsupported actions.
4. **Approve** migration of Symbol and Emoji to the Insert panel; keep existing
   insertion transactions and keyboard grid behavior.
5. **Require** UXMATH-001 design/ADR before any Equation insert command. Use the
   Word-for-web draft-panel pattern for the first bounded authoring slice after
   that approval.
6. **Defer** selection/layers, multi-select, grouping, alignment, and snapping
   until the stable target/capability contract is proven.
