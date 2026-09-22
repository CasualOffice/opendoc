# 117 — Selecting a shape inside a group

**Status:** accepted, implemented in `fix/group-click-descend`.
**Opened:** 2026-09-22. **Row:** `109` HF-170. **Related:** `115` (the same
rule applied to the Styles control), `101` UXOBJ-001 (object references).

## 1. What was shipped, and why it was not enough

The owner reported grouped drawings as uneditable three times. Twice the fix
addressed a real defect and still did not change what they experience:

| # | what it fixed | why it did not help |
| --- | --- | --- |
| #556 | grouped children got model identity at all | they still could not be reached |
| #574 | **Tab** walks a group's children, nested ones included | nobody reaches for Tab to click a picture |

The gesture a user actually performs is a **click**, and a click on a grouped
shape selected the whole group — every time, however many times they clicked.
`objectAt` answers with the group root by design, and nothing ever asked a
different question. Double-click jumped straight into typing, skipping child
selection entirely, so there was no gesture at all that selected one shape.

This document is the comparison that should have come before #574, not after.

## 2. ONLYOFFICE, read from source

`ONLYOFFICE/sdkjs@master`, read 2026-09-22 — `common/Drawings/States.js`,
`common/Drawings/CommonController.js`, `common/Drawings/DrawingObjectsHandlers.js`.
AGPL-3.0; read for **behaviour**, nothing copied.

Their controller carries an explicit *"the group I am currently inside"* —
`selection.groupSelection` — which is separate from "a group is selected".

- **A plain click on a grouped shape selects the GROUP.** `handleGroup` walks
  the children back to front and, on a hit in a child's shape area, calls
  `handleMoveHit(drawing, …)` where `drawing` is the **group**, not the child
  it hit. There is no path by which a first click selects a child.
- **A click on a grouped text box's TEXT goes straight into that text.** The
  `hit_in_text_rect` branch of `handleShapeImageInGroup` calls `handleTextHit`,
  which sets `group.selection.textSelection` and `selection.groupSelection`.
  This is how you get "inside" without a double-click.
- **Once inside, clicks resolve against the children.** `NullState.onMouseDown`
  tries `handleFloatObjects(…, selection.groupSelection.arrGraphicObjects, …,
  group)` *before* the top-level objects, and because a `group` is passed the
  selector becomes the group, so `selectObject` selects the CHILD.
- **Leaving clears it**: `resetInternalSelection` sets `groupSelection = null`.

The load-bearing fact: **a group is a wall to the first click and porous to
every click after you are inside it.** That is the shape of the fix.

## 3. Microsoft Word

Word's grammar for the same object, and the one most users have in their
fingers:

- click a grouped shape → the **group** is selected, with its own handles;
- click again on a child → that **child** is selected, handles on the child;
- click a third time on a different child → that child;
- double-click a child that holds text → its text, caret placed;
- **Esc** → back up one level, to the group. Esc again leaves entirely.

Word reaches the child with a second plain click. ONLYOFFICE requires the text
rect or a double-click first. Word's is the smaller gesture and the one that
answers the owner's complaint directly.

## 4. Google Docs

Not a counterpart: Docs has no in-document shape groups. A drawing is edited in
a separate drawing editor, where selection is PowerPoint-like. It contributes
nothing to this decision and is recorded here so the absence is not mistaken
for an oversight.

## 5. Decision

**Word's grammar, with ONLYOFFICE's "inside a group" state under it.**

1. **The first click on a group selects the group**, unchanged. A group has to
   be pickable, movable and deletable as one thing, and a descent on first
   click would make that impossible. Both competitors agree here.
2. **A click on a group already in hand selects the child under the pointer** —
   Word's second click. This is the whole of the defect: it is the gesture the
   owner was performing and the one that did nothing.
3. **A click on the group's own background re-selects the group**, because no
   child is under the pointer. Falling out of the group there would make the
   group unpickable once entered.
4. **Escape climbs one level**, child → group → document. Without this,
   descending was a one-way door: Escape from a child went all the way back to
   the text caret, so the group just picked up was gone.
5. **Double-click still enters text directly**, unchanged, and remains the
   fast path ONLYOFFICE's text-rect click serves.
6. **Nested groups are not a special case.** `objectDescendantAt` already
   resolves the deepest painted descendant, so a shape two levels down is
   reached by the same second click, and its reference path says `2.1` rather
   than `2`. The owner's Medical form has exactly this shape and it is why
   "nested" was called out separately in the report.

### Not adopted

- **ONLYOFFICE's text-rect click** (a first click on a grouped text box's text
  going straight into typing). It conflicts with rule 1: the same first click
  would select a group here and start editing there, depending on whether the
  pixel happens to be over text. Word does not do this, and a gesture whose
  meaning depends on sub-shape geometry is the kind of thing this repository
  has had to unpick before. Double-click remains the way in.
- **Ctrl/Shift-click multi-select inside a group.** Both products support it;
  neither the model nor the chrome carries a multi-object selection yet, so it
  is out of scope rather than half-built. Recorded as open.

## 6. Guards

A test asserting "a grouped shape can be selected" passed throughout, because
Tab could do it since #574. These are the ones that can fail on the defect:

1. **A second click selects the child under the pointer**, and a *third* click
   elsewhere in the group selects a different child. Reverting to one
   `objectAt` call fails it.
2. **The first click still selects the group.** A fix that always descended
   would pass (1) and break every group-as-a-unit gesture; this is the half
   that pins rule 1.
3. **A nested child is reached by the same click** — the path has a dot in it.
4. **Escape climbs to the group, then out**, rather than dropping straight to
   the caret.
5. **A click on the group's background re-selects the group** rather than
   deselecting.

Each was driven red by mutating `main.js`; the mutations and their output are
in the branch's commit message.
