# 58 — Interaction, Selection & Editing Architecture

**Status:** Accepted architecture; implementation staged (copy-in-view first).
**Date:** 2026-07-27
**Related:** doc 45 (extensibility seams I1–I4), doc 56 (editor shell decision),
doc 57 (Phase 1G plan), `casual-doc-layout::hittest` (P1E-002a/b),
`casual-doc-transaction` / `casual-doc-sdk` (the edit substrate).

## Why this doc exists

The browser viewer must gain selection and copy **now**, but the end goal is a
full editor: editable text, then tables, images, headers/footers, VML/legacy
drawings, and **positioned objects** (move/resize floats and anchors). If we wire
"select text → copy" as a bespoke path, we rebuild it for every object kind.

This document fixes the **one interaction pipeline** every present and future
action flows through, so that copy-in-view is the first *read-only* action over
it and every editing action is an *additive* extension — never a rework.

> **Design rule (normative).** No interaction is special-cased to text. Pointer
> and keyboard events resolve to a **hit target**, targets update a **selection**,
> selection drives a **command** (or a read-only action). New object kinds extend
> the `HitTarget` / `Selection` / `Command` enums; they do **not** add new
> pipelines.

### Editing-surface continuity contract

This contract is normative for every editable story/container: the document
body, a header or footer, a footnote or endnote, a text-box body (including a
box nested in a group), and text inside a table cell.

1. **The model selection owns the context.** The active surface is derived from
   the node that owns the caret/range, never inferred from `pages[0]`, a DOM
   element, a missing glyph hit, or host-maintained duplicate state.
2. **Entry is deliberate.** A pointer-down in another editable surface, the
   documented double-click/Enter gesture for a container, or an explicit command
   may change context. Repaint, virtualization, zoom, scrolling, ribbon/panel
   use, and focus restoration may not.
3. **Empty space is still inside its container.** A point inside the placed
   bounds of the active text box, running-content band, note, or cell remains in
   that surface even when glyph hit-testing returns no text target. It resolves
   to the nearest valid position in that surface.
4. **Pointer-down owns a drag.** A selection drag is resolved in the surface in
   which it began. Crossing into an incompatible surface clips the moving end to
   the nearest valid position in the starting surface; it never creates a
   cross-story range or silently retargets the gesture. A new pointer-down may
   enter the other surface.
5. **Geometry changes preserve semantic state.** Zoom, device-pixel-ratio
   changes, pagination repaint, and page mount/unmount retain the same model
   selection and editing context, then recompute only its visual geometry.
6. **Exit is predictable.** Escape and click-away follow the grammar of the
   active surface. Text-box editing exits to object selection, then to the
   surrounding text; running content exits to the document body; a direct click
   into another surface enters that surface exactly once.
7. **Commands are surface-neutral or explicit.** A command available for body
   text must use the owning surface for reads and mutation. If a surface cannot
   support it, the command returns an explicit unsupported result; it must not
   fall back to the body or report success without changing the intended node.

The browser regression matrix must exercise this contract with real pointer and
keyboard gestures. Test points come from engine/overlay geometry, not guessed
page fractions, whenever the product exposes that geometry.

For repeated running content, the active model node and offset remain unchanged
while scrolling. The visible caret projection follows the page nearest the
viewport midpoint, with updates bounded to one animation frame and geometry
recomputed by the engine. A projection may move only when that exact model focus
is placed on the candidate page; different-first/even/odd variants therefore do
not silently retarget the selection into another header or footer story.

## The pipeline

```
pointer / keyboard / IME / clipboard event
        │
        ▼
   hit-test  ────────────  LayoutSnapshot over PaginatedLayout (read-only geometry)
        │                  returns a HitTarget (§2), anchored on NodeId/ModelPos (I3)
        ▼
   update Selection (§3)   a MODEL concept, not a DOM concept
        │
        ├─► render overlay   caret_rect / selection_rects / object handles
        │                    → WE draw it (exact raster match; doc 56/57 decision)
        │
        ▼
   dispatch Action (§4)
        ├─ read-only  → copy / find / accessibility     (no transaction)
        └─ mutating   → Command → transaction (I1 choke point, I2 closed op set)
                          → PositionMap remaps selection → dirty pages → repaint
```

Two properties make this scale:

1. **Everything is a node.** A run, an image (`InlineNode::Drawing`), a table
   (`BlockNode::Table`), a cell, a footnote, a header/footer body, a VML picture,
   an anchored float — each already carries a stable `NodeId` (I3). So a selection
   can address *any* of them; only the hit-test needs to learn to *recognize* them.
2. **All mutation is one closed op set** behind the transaction choke point (I1/I2).
   "Edit text", "resize image", "insert table row", "move float" are all
   `Command`s that compile to that op set; the pipeline downstream of `dispatch`
   is identical for every one.

## 2. Hit targets (extensible taxonomy)

`hit_test(page, point)` returns **what** is under the point, not just a caret.
Today only the text-caret variant is produced; the enum is designed so later
variants are additive and the frontend `match` gets new arms, nothing else.

```
HitTarget =
  | TextCaret   { pos: ModelPos, zone }                 // v1 (exists)
  | InlineObject{ node: NodeId, kind: ObjectKind, rect }// image/drawing/eq
  | TableCell   { cell: NodeId, pos: ModelPos }         // caret inside a cell
  | Float       { node: NodeId, rect, handle: Handle? } // anchored/positioned obj
  | RunningZone { region: HeaderFooter, pos: ModelPos } // header/footer band
  | Outside     { nearest: ModelPos }                   // margin snap
```

- `ModelPos { node, offset }` is the **layout anchor** (offset = node-relative
  **UTF-8 byte**; P1E-002b). It addresses a caret slot inside a text node.
- `Handle` (later) names a resize/move grip on a positioned object; it is what
  turns a Float hit into a drag-to-resize gesture without a new code path.
- New object kinds (VML shapes, SmartArt, charts) add an `ObjectKind`, not a new
  `HitTarget`.

Only `TextCaret` and `Outside` ship first. The rest are declared now so the
frontend and WASM contract are shaped for them.

## 3. Selection (a model concept)

Selection lives in the **model space**, never as a browser DOM selection (I3;
doc 56). It is an enum so non-text selections are first-class:

```
Selection =
  | Caret   (ModelPos)                    // collapsed
  | Text    (ModelRange)                  // a run of text, possibly cross-node/page
  | Object  (NodeId)                      // an image/float/shape selected as a unit
  | Cells   (NodeId /*table*/, CellSpan)  // a rectangular block of table cells
```

Ships first: `Caret` and `Text`. The visible caret and highlight are drawn by us
from `LayoutSnapshot::{caret_rect, selection_rects}` — the **same geometry the
raster came from**, so there is zero overlay-vs-glyph drift (doc 57 §12 risk,
resolved by construction). The browser's own selection paint is suppressed; a
transparent per-run text layer exists **only** for native ⌘C / find / screen
readers.

### Two anchor spaces, one bridge

The engine deliberately has two position models; the interaction layer owns the
conversion so neither side leaks into the other:

| Space | Type | Offset unit | Used by |
| --- | --- | --- | --- |
| Layout | `hittest::ModelPos` | UTF-8 **byte** | hit-testing, caret/selection geometry, **copy** |
| Edit | `sdk::Position` | **grapheme** + affinity | transactions, undo, `PositionMap` |

- **Copy-in-view (now)** works entirely in **layout space** (byte offsets): hit →
  `ModelPos` → `ModelRange` → range-text extraction → clipboard. No edit space,
  no transaction — the read-only slice never touches I1/I2.
- **Editing (later)** converts the layout-space selection to `sdk::Position`
  (byte→grapheme, once) at the moment a `Command` is issued, applies through the
  transaction layer, and maps the selection forward with `PositionMap`. This
  byte↔grapheme bridge is a single documented seam (`selection_bridge`), not
  scattered arithmetic.

## 4. Actions and commands

- **Read-only actions** (copy, find highlight, a11y export) consume a `Selection`
  and never mutate. `copy_text(range)` walks the model between two `ModelPos` and
  concatenates run text with paragraph breaks. **This is the first action** — it
  exercises hit-test → selection → overlay → action end-to-end so the seams are
  proven before any mutation exists.
- **Mutating actions** are `Command`s (doc 45 I1). The closed op set (I2) already
  covers text (`insert`/`delete`/`split`/`join`); object and structural edits
  (resize/move a float, replace an image, insert/delete a table row, edit a
  header/footer, edit a VML shape's geometry) are **new `Command` variants that
  compile to the same ops or additive ops** — each still entering through the one
  choke point, still remapped by `PositionMap`, still yielding a dirty-page set.

Because copy is read-only, shipping it first adds **no** risk to the model while
laying every seam editing needs.

## 5. WASM contract (the first slice)

`casual-doc-wasm` gains, over the existing `open`/`render_page`:

- `textLayer(i) -> TextLayer` — per-run `{x,y,w,h,baseline,text,dir,node,offset}`
  from the page `DisplayList` (doc 57 §5.2); drives the transparent copy/find/a11y
  overlay.
- `hitTest(page, xTwip, yTwip) -> HitTarget` — §2 (TextCaret/Outside first).
- `caretRect(node, offset) -> CaretRect` and `selectionRects(range) -> Rect[]` —
  the geometry **we** paint the caret/highlight from.
- `copyText(range) -> string` — §4 read-only extraction.
- `linkAt(page, xTwip, yTwip) -> Hyperlink?` — click-to-open (hyperlinks are
  first-class `InlineNode`s).

`NodeId` (u128) crosses as a decimal **string**, compared/passed back only, never
arithmetic'd in JS (doc 57 §5.3). All geometry is page-local twips; the one units
module converts px↔twip.

Later milestones add `objectAt`, the `Command` methods, and `drainEvents` for the
dirty-page set — **additively**, against this same handle.

## 6. Frontend (webapp)

- `overlay/` gains: a transparent text layer (spans from `textLayer`) for native
  copy/find/AT, and a **selection layer we draw** (caret + highlight rects from
  `caretRect`/`selectionRects`; later, object outlines + resize handles).
- `input/` gains: pointer down/move/up → `hitTest` → update `Selection` → redraw
  overlay; ⌘C → `copyText`; click on a link → `linkAt` → open. Keyboard caret
  nav uses `moveVertical`/arrow logic (already in `LayoutSnapshot`).
- The selection layer is **object-agnostic**: it renders whatever geometry the
  current `Selection` yields, so adding `Object`/`Cells` selection later is new
  geometry, not new plumbing.

### 6.1 Platform keyboard contract

The host owns physical-key interpretation, but it emits semantic movement/edit
commands to the engine. Ctrl and Command are not aliases:

| Intent | macOS | Windows/Linux | Engine/host action |
| --- | --- | --- | --- |
| previous/next character | Left/Right | Left/Right | `left` / `right` |
| previous/next word | Option+Left/Right | Ctrl+Left/Right | `wordLeft` / `wordRight` |
| visual line start/end | Command+Left/Right or Home/End | Home/End | `lineStart` / `lineEnd` |
| previous/next paragraph | Command+Up/Down | Ctrl+Up/Down | `paragraphUp` / `paragraphDown` |
| document start/end | Command+Home/End | Ctrl+Home/End | `firstPosition` / `lastPosition` |
| one viewport upward/downward | Page Up/Down | Page Up/Down | host hit-tests one viewport-height away |
| delete previous/next word | Option+Backspace/Delete | Ctrl+Backspace/Delete | `deleteWordBackward` / `deleteWordForward` |

Select All is scoped to the active editing surface. In a table cell, the first
invocation selects the innermost cell's structural text flow using model-owned
cell bounds; invoking it again while that exact range remains selected escalates
to the whole document and reports that escalation in the editor status. A cell
whose contiguous range would cross an embedded text-box story returns an
explicit unsupported result instead of selecting across surfaces. This staged
rule prevents an ordinary replacement gesture from clearing neighboring cells
or unrelated document content.

Shift extends every navigation action from the existing model anchor. Plain
horizontal movement collapses a range to its ordered start/end before moving;
vertical, paragraph, page, and document movement moves the focus normally.
Page Up/Down uses the live viewport only to select the destination page-local
point, then resolves that point through `hitTest`; the resulting selection is
still a model anchor and browser scroll position never becomes document truth.

This contract follows the current Microsoft Word keyboard references for
[Windows](https://support.microsoft.com/en-us/accessibility/word/keyboard-shortcuts-in-word)
and
[macOS](https://support.microsoft.com/en-us/accessibility/word/keyboard-shortcuts-in-word).
The browser key values are the W3C UI Events
[`key` values](https://www.w3.org/TR/uievents-key/), including `PageUp`,
`PageDown`, `Home`, and `End`.

## 7. Delivery order

1. **P1G-003 (now):** text `Selection` (`Caret`/`Text`), custom-drawn caret +
   highlight, `copyText`, `linkAt`, transparent text overlay for ⌘C/find/AT.
   Proves the whole pipeline read-only.
2. **P1G-006:** the byte↔grapheme `selection_bridge` + editing `Command`s for
   text (insert/delete/split/join) through I1/I2; `PositionMap` selection remap.
3. **Later:** `HitTarget`/`Selection`/`Command` object variants — images, tables,
   headers/footers, VML/drawings, positioned-object move/resize — each an additive
   arm, no pipeline change.

## 8. Non-goals / open questions

- **Rich clipboard** (HTML/RTF) is deferred; the first `copyText` is plain text.
- **IME on canvas** (hidden contenteditable proxy vs composition API) is decided
  in P1G-006/007, not here.
- **Per-glyph vs per-run overlay boxes** for justified/complex lines: the visible
  selection is ours (exact), so overlay precision only affects native-selection
  copy granularity; start per-run, revisit with the corpus.
