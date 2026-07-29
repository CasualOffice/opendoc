# 59 — Editing Operations on `v1::Document`

**Status:** Accepted architecture; implementation staged (text insert/delete first).
**Date:** 2026-07-27
**Related:** doc 45 (seams I1–I4), doc 58 (interaction pipeline), doc 56/57
(editor shell), `casual-doc-transaction` (the v0 op set this parallels).

## Why a new op set

The editing substrate built in Phase 0 (`casual-doc-transaction` /
`casual-doc-sdk`) operates on the **v0 normalized `Document`** — a minimal model
(paragraphs of `Text` runs, `{bold,italic,underline,strike}` marks). What the
viewer renders is **`v1::Document`** — runs with full properties, tables, images,
drawings, headers/footers, fields, content controls. The two are different types
with no v1→v0 mapping, so edits cannot be routed through `DocumentSession`: it
would mutate a model that is not what the user sees.

**Decision:** build the closed op set (I2) directly on `v1::Document`, so editing
mutates the *same model that is rendered*. The v0 layer is not reused; its
*design* — a closed op set, per-op inverses for undo, position mapping, one
choke-point entry (I1) — is carried onto v1. Delivery is **text-first**: the op
set starts with text edits and grows to structural/object edits additively.

## Anchor space

Edits use the **layout anchor space** (doc 58 §3): a position is
`(NodeId paragraph, u32 byte_offset)` — the *same* anchors `hitTest` returns, a
node-relative UTF-8 byte offset into the paragraph's shaped plain text
(`node_plain_text`). This deliberately avoids the v0 grapheme/affinity model and
the byte↔grapheme bridge: hit-testing, selection, and editing all speak byte
offsets. Offsets are validated to land on `char` boundaries; edits that would
split an extended grapheme cluster snap outward (never mid-cluster). Marks/format
edits, which the v0 model expresses as grapheme ranges, are a later concern.

## The op set (closed; I2)

```
Operation =
  | InsertText     { at: Pos, text: String }
  | DeleteText     { range: Range }                 // within one paragraph
  | SplitParagraph { at: Pos, new_id: NodeId }      // Enter
  | JoinParagraphs { first: NodeId, second: NodeId }// Backspace at ¶ start
  // later, additive: SetRunProps, InsertObject, DeleteObject, table ops, …
Pos   = { node: NodeId, offset: u32 }   // paragraph-relative UTF-8 byte offset
Range = { start: Pos, end: Pos }        // same node for DeleteText
```

`apply(&mut Document, &Operation) -> Result<Inverse, EditError>` mutates the
document in place and returns the **inverse** operation(s) that exactly undo it —
so undo/redo are just applying inverses. Every op is total or fails cleanly
(no partial mutation): it validates first (node exists, offset in range, char
boundary), then mutates.

Inverses:
- `InsertText{at,text}` → `DeleteText{at .. at+len}`.
- `DeleteText{range}` → `InsertText{range.start, removed_text}` (+ removed run
  properties for a faithful undo; first cut restores as one run with the
  boundary run's properties).
- `SplitParagraph{at,new_id}` → `JoinParagraphs{orig, new_id}`.
- `JoinParagraphs{a,b}` → `SplitParagraph{a, at=len(a)}` restoring b's id.

## Offset → run resolution

A paragraph's editable text lives in `InlineNode::Run{text,…}` items (and, for
recursion parity with `node_plain_text`, inside `Hyperlink`/`Revision`/`Sdt`
wrappers; `Tab`/`Symbol` contribute bytes but are not split). Resolving a
byte offset walks the paragraph's inlines accumulating each run's byte length
until the offset falls inside (or at the boundary of) a run, then splices that
run's `String`. Rules:

- Insertion at a run-interior offset splices into that run (inherits its props).
- Insertion at a run boundary appends to the preceding run (or, at paragraph
  start, prepends to the first run; an empty paragraph gets a new run whose props
  come from the paragraph's mark context — first cut: default props).
- Deletion spanning multiple runs removes whole covered runs and trims the
  partial end runs; runs emptied are dropped (`Run.text` is non-empty by
  invariant); adjacent runs with equal properties MAY be coalesced (deferred).
- Offsets over `Tab`/`Symbol`/object bytes snap to the nearest run boundary
  (first cut); precise editing across those is a later refinement.

**First implementation slice** handles **top-level runs** (no nested wrappers);
nested-wrapper and tab/symbol-boundary edits are follow-ups. The resolver reports
an `EditError` rather than corrupting when it cannot place an edit.

## Session, undo, and re-pagination

A thin `EditSession` owns the `v1::Document`, a monotonically increasing
`revision`, and two stacks of inverses (undo/redo). `apply` pushes the inverse to
undo and clears redo; `undo` pops undo→applies→pushes its inverse to redo.

After any apply the host **re-paginates** and repaints. The first cut re-paginates
the whole document (correct, simple); the incremental paginator
(`P1D-004`/`repaginate_with_stats`) already computes a reused-prefix/reflowed/
reused-tail split, so a later slice returns a **dirty-page set** from `apply` and
repaints only those pages (doc 57 §4.5). Selection is remapped by the same
byte-delta the op applied (insert shifts offsets ≥ `at` by `+len`; delete by
`−len`; split/join move across the paragraph boundary) — a `PositionMap` analogous
to the v0 one.

### User-action transaction refinement (2026-07-30)

The browser must not infer atomicity by issuing several public mutations and
hoping history later combines them. Plain-text insertion therefore crosses the
WASM boundary as one semantic command: it may delete the current selection,
insert text runs, and split paragraphs for normalized line breaks, but the
engine applies that closed operation group atomically and records one inverse
group. Paste, an IME commit, and replacing a selection with Enter all use this
path.

Typing remains incremental so every key immediately reflows and paints. The host
assigns a bounded typing-session id to a rapid adjacent burst; the engine may
prepend a new inverse group to the previous history entry only when both the id
and expected caret match. A pause, caret/selection movement, pointer action, or
non-typing command starts a new id. The engine still validates adjacency, so a
stale or reused host id cannot merge edits at unrelated positions.

History availability is engine truth (`canUndo` / `canRedo`), not inferred from
whether a document is open. Ordered selection edges are also resolved by the
engine because the frontend cannot compare anchors from different paragraphs.

## WASM surface (added to `casual-doc-wasm`)

- `insertText(node, offset, text) -> EditResult`
- `insertPlainText(selection, text) -> EditResult` (atomic paragraphs/paste)
- `typeText(selection, text, session) -> EditResult` (guarded history coalescing)
- `deleteRange(node, start, end) -> EditResult`
- `splitParagraph(node, offset) -> EditResult`  *(slice 2)*
- `joinParagraphs(first, second) -> EditResult` *(slice 2)*
- `undo() -> EditResult`, `redo() -> EditResult`
- `canUndo`, `canRedo`
- `selectionEdge(selection, towardEnd) -> Caret`
- `EditResult { revision, caret: Pos }` — the new revision and where the caret
  should land, so the frontend re-renders and re-places the caret. A dirty-page
  set is added when `apply` surfaces one.

Edits enter *only* through these semantic methods (I1): JS never constructs an
`Operation`, preserving the choke point.

## Frontend (webapp)

`input/` gains a keyboard handler: printable keys → `typeText` at the caret;
Backspace/Delete → `deleteRange` (or `joinParagraphs` at a ¶ boundary, slice 2);
Enter → `splitParagraph` (slice 2); ⌘Z/⌘⇧Z → undo/redo. After each edit the caret
advances, the affected page(s) re-render, and the caret rect is redrawn. IME
composition commits through the atomic plain-text command; its preedit remains a
host overlay until composition ends.

## Staging (prioritized pipeline)

- **Slice 1 — DONE:** `InsertText` + `DeleteText` on top-level runs, undo/redo,
  WASM `insertText`/`deleteRange`/`deleteBackward`/`deleteForward`/`undo`/`redo`,
  keyboard typing + backspace + undo, whole-document re-pagination.

- **Slice 2 — DONE (foundation): structural edits + navigation/selection.** The batch
  that makes editing feel complete, in priority order:
  1. `SplitParagraph` (Enter) + `JoinParagraphs` (Backspace at ¶ start) — includes
     content **reflowing across page boundaries**; the caret follows via
     `caret_rect` (already multi-page).
  2. **Cross-paragraph delete** — a selection that spans paragraphs deletes as
     `DeleteText` on the ends + `JoinParagraphs` (and type-over = delete + insert).
  3. **Caret navigation** — arrow keys: left/right by char (crossing ¶
     boundaries), up/down via `LayoutSnapshot::move_vertical` (crossing lines/
     pages). WASM `moveCaret(node, offset, dir)`.
  4. **Keyboard selection** — Shift+Arrow extends by moving the focus only.
  5. **Pointer selection** — Shift+Click extends to the clicked anchor;
     double-click selects the word (`wordAt` via Unicode word segmentation).

- **Slice 3 — in progress:** run/paragraph/table property edits, core table
  structural ops, dirty-page repaint, rich run clipboard, and IME preedit/commit
  are delivered through the same choke point. General object editing, structured
  table/list/image clipboard fragments, and additional editing surfaces remain.

- **Slice 4 — DONE:** user-action history refinement (`P1G-EDIT-CORRECTNESS-005`)
  adds atomic plain text, guarded typing-session coalescing, ordered range-edge
  collapse, and real history availability.

## Non-goals

Collaboration/OT (the closed op set + position map are the seam that keeps it
additive — I2/I3) and grapheme-range mark editing are out of scope here. Rich
run clipboard support is additive; complete structured table/list/image
clipboard fidelity remains outside this operation-set foundation.
