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
