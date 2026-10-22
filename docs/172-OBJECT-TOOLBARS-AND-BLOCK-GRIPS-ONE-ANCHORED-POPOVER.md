# 172 — Object toolbars and block grips: one anchored-popover mechanism

**Status:** Design, with its first consumer built. 2026-10-22.
**Opened:** 2026-10-22, from the owner's report: "our hover action menus on selection of
things, like tables and images and others, need to be seriously looked into."
**Scope:** every selection and hover surface that floats beside a NON-table object (images,
shapes, text boxes, groups, charts, equations, links), the six-dot block grip the owner
asked for, and the shared contract table chrome must follow. Table chrome itself belongs to
the tables lane (its own design document, filed by that lane).
**Rows:** `docs/109` OL-02 (save validity, P0), OL-03 (inserted picture not selected),
OL-04 (wrap does not reflow), OL-05 (side grip too narrow), OL-06 (rotation undo label),
OL-07 (in-line group frame), OL-08 (the toolbar and the shared mechanism), OL-09 (the block
grip).
**Evidence base:** this tree, screenshots from both editors (§1, §6), and ONLYOFFICE Docs
10.0.0 driven locally at `http://localhost:8090` with Playwright. Its served content was
treated as data; **no AGPL code was read for this document or copied**. Word, Word for the
web, Google Docs, Notion and Loop were not available to drive: every claim about them is
marked **believed** and is recollection, not measurement.

> **What this concludes, before the evidence.**
>
> 1. The selection toolbar was not a design problem in one place but the same placement
>    arithmetic written several times (the object bar, the link card, and nothing yet for
>    the block grip), each wrong in its own way. The pattern that answers all of them is the
>    **anchored popover**: a surface placed against an anchor rectangle on a preferred side,
>    **flipped** when that side has no room, **shifted** along the cross axis to stay inside
>    the view, and **hidden** when its anchor is out of view (the model Floating UI and
>    Popper implement). It now lives once, in `webapp/src/object_anchor.mjs`.
> 2. The object bar is rebuilt on it (built, OL-08). The link card and the block menu adopt
>    it next; both need `main.js`, which another lane holds.
> 3. ONLYOFFICE 10.0.0 has **no** floating toolbar for pictures, shapes or text boxes, and
>    **no** block grip: it has a table move handle, a chart-elements button and an equation
>    pill. The owner's belief that ONLYOFFICE has a six-dot grip is not borne out. Notion has
>    one; Loop is believed to; Word and Google Docs are believed not to.

## 1. The inventory, before

Measured in this tree at commit `7ab5b24a` on the demo document (`?fixture=rich`) at
1440x900. Screenshots (lane scratchpad): `before-01-image-selected.png`,
`before-02-scrolled.png`, `before-04-textbox-selected.png`, `before-06-chart-selected.png`,
`before-09-image-dark.png`, `before-09-image-rtl.png`, `before-09-image-phone.png`.

| Surface | Appears / hides | Position | Overlap | Keyboard and screen reader | Touch | Defects |
| --- | --- | --- | --- | --- | --- | --- |
| **Object bar, image** | On selection; hides on deselect | ABOVE the object, left edge aligned to it, clamped to the view top | 1,220px wide over a **16px** picture; covered the heading above | No `role`, no name; reachable only by tabbing past the whole page; no Escape | Wrap chips 25px and absent from the 44px coarse-pointer list | Pinned to the view top over unrelated text when the object scrolled up under the ribbon; seven text chips; English-only Alt text, Properties, Crop, Delete |
| **Object bar, text box** | Hidden while editing inside; shown when selected | As image | 970px over a 288px box | As image | As image | No alt text control (Word has one) |
| **Object bar, chart** | On selection | As image | Covered the paragraph above | As image | As image | Named **"Object"** (`OBJECT_LABELS` had no chart); no wrap or position (a chart has no anchor in the model) |
| **Object bar, shape** | On selection | As image | — | As image | As image | Fill and Outline as text pills |
| **Link card** (`#linkChip`) | On a click on a link; hides on any pointerdown elsewhere or Escape | At the POINTER (+14px), never re-placed | Over the line below | `role="dialog"`, but focus never moves into it | 24px buttons | Does not follow a scroll: the card stays where the text was |
| **Equation** | No surface | — | — | — | — | No equation authoring (OMML renders; nothing edits it) |
| **Rotation grip** | With a rotatable selection | 24px above the top edge, on a tether | — | `role="slider"`, arrow keys | 24px target | — |
| **Resize grips** | With a resizable selection | On the perimeter | — | Not focusable (nudge and the inspector are the keyboard path) | ~24px target | A side grip was 9px across its edge: a press 5px off moved the picture (OL-05) |

Surfaces per capability (SKILL §10 asks for at least two):

| Capability | Bar | Context menu | Inspector | Ribbon / palette / key |
| --- | --- | --- | --- | --- |
| Wrap (7 modes) | yes | yes | — | — |
| Position, Arrange, Rotate | yes | yes | rotation field | — |
| Group / Ungroup | in Arrange | in Arrange | — | — |
| Alt text, Crop, Change picture, Picture border | yes | yes | — | — |
| Fill, Outline (shape) | yes | yes | yes | — |
| Properties | yes | yes | (is the inspector) | — |
| Delete | yes | yes | — | Delete key |
| Chart: Edit data | yes | yes | — | Chart tab, palette |
| **Shape: Add text** | **no** | yes | — | — |
| **In-line picture: Move to…** | **no** | yes | — | — |

Two capabilities sit on one surface only; both need a verb from `main.js` on the bar's `io`
(§7).

## 2. The competition

### ONLYOFFICE Docs 10.0.0 (verified locally)

- **No floating bar** for a picture, shape or text box. Settings go to a right-hand panel,
  collapsed by default to an icon rail; selecting an object highlights the rail icon, and
  an open panel switches between Paragraph, Image and Shape settings. Opening it reflows the
  page about 113px left.
- **A chart** gets a contextual "Chart design" tab, an auto-opened chart editor window, and
  one floating 40x24px **Chart elements** button 9px outside the chart's top-right corner,
  which follows scroll and zoom and hides on deselect.
- **An equation** gets a floating math-toolbar pill (~390x30px) about 20px above its line
  while the caret is inside it.
- **A link** gets a hover tooltip (the URL and "Press ⌘ and click link"), pointer-anchored,
  with no actions; editing is a modal dialog.
- **An inserted picture is selected**, with 8 handles and a rotation grip (OL-03).
- **Square wrap reflows the text in the same frame** (OL-04).
- **The Undo tooltip is the static "Undo (⌘+Z)"**: it names no action. Ours naming the
  wrong action is worse than naming none (OL-06).
- **Grip hit areas:** a side grip resizes about 9-11px along its edge and from 4-5px
  inside to 3-5px outside; corners at the corner point; the rotation grip is a ~10px circle
  about 24px above the top edge.
- **No per-block grip** beside a paragraph, heading or list item (the cursor stays `text`
  and no overlay appears). A table has a four-arrow **move handle** (~13x13px) just outside
  its top-left corner and a resize square just outside its bottom-right; the band left of a
  row shows a row-select cursor. Floating objects move by **body drag only**.

### Word and Google Docs (believed, not driven)

- **Word desktop:** the Layout Options button floats just outside a selected picture's top
  right and opens the wrap gallery; the ribbon gains Picture Format / Shape Format tabs;
  charts get three floating buttons (Elements, Styles, Filters). Alt+Shift+Up/Down moves a
  paragraph, a list item or selected rows. No block grip.
- **Google Docs:** a toolbar BELOW a selected image: In line, Wrap text, Break text, Behind
  text, In front of text, then the image's own actions. No block grip.

Our target takes **Docs' placement and contents** (below, the wrap modes as one icon row,
then the object's own actions) and **Word's keyboard and naming** (Alt+Shift+Up/Down, undo
labels that name the action). ONLYOFFICE's panel-only model is not adopted: it costs a page
reflow to open and leaves a touch user nothing beside the object.

## 3. The pattern: one anchored popover

`webapp/src/object_anchor.mjs`:

- `placeAnchored(anchor, size, view, {prefer, clearance, align, dir, gap})`, pure and O(1).
  Prefers `below`, flips to `above`, and when neither fits sits **inside** the view against
  the preferred edge (a reachable control over part of a huge object beats an unreachable
  one). `clearance` is how far the anchor's own grips reach on each side, so the surface
  never lands in a grip's target. `align` is `center` or `start`; `start` follows `dir`, so
  a right-to-left document aligns to the anchor's right edge. It returns `null` when the
  anchor is out of view on either axis: the surface hides rather than parking over
  unrelated text.
- `applyAnchoredPlacement(el, anchor, view, options)` measures, places, and reflects the
  side as `data-side`.
- `suspendDuringPointerGesture(surface, scope, onResume)` hides the surface (class
  `is-suspended`, not `hidden`) while a pointer gesture is in flight on the pages, and
  re-places it after, as Word and Docs do while a picture is dragged.
- `bindToolbarKeys(el, {onEscape, dir})` is the WAI-ARIA toolbar: one tab stop, arrows
  (reversed in RTL), Home and End, Escape out.

Every surface that floats beside something in the document uses these and nothing else. A
new surface states its anchor, its preferred side and its grip clearance; it does not write
placement arithmetic.

## 4. Target behaviour, for every object kind

1. **Appears** when an object is selected (click, Tab traversal, insert, paste, drop).
   **Hides** on deselect, while text inside the object is being edited, while a pointer
   gesture is in flight on the pages, and while the object is out of view. It comes back,
   re-placed, when the gesture ends or the object scrolls back.
2. **Placement:** below the object, clear of the south grips (20px); flipped above, clear
   of the rotation grip (44px); else inside the view. Centred on the object's TURNED
   bounds, kept 8px inside the page view. Scroll, zoom, resize and rotation re-place it from
   one `objectFrame` read, never an O(document) query.
3. **Contents:** the wrap modes as one row of seven icons (in-line first), then Position,
   Arrange, Rotate, the kind's own actions (Edit data; Alt text; Fill and Outline or Picture
   border; Crop; Change picture), Properties, Delete. Every control is an icon with its
   localised name on the tooltip and the accessible name. A control that does not apply is
   **absent** (a text box has no crop); one that applies but is refused is **disabled with
   its reason** and still focusable (`aria-disabled`).
4. **Keyboard:** Alt+F10 from the document focuses the toolbar's first control (the chord
   editors with a contextual toolbar use); arrows walk it (reversed in RTL), Home and End
   jump; Escape returns focus where it came from and leaves the object selected. One tab
   stop.
5. **Screen reader:** `role="toolbar"`, named for the object ("Image actions"); the wrap row
   is a `group` named "Text wrapping"; each wrap button reports `aria-pressed`.
6. **Touch:** under `(pointer: coarse)` every control is 44x44. The bar never sits over a
   grip, so a finger on a grip never hits the bar.
7. **Themes and RTL:** tokens only; RTL mirrors the order through flex and the arrows through
   `bindToolbarKeys`.
8. **Grips (OL-05):** a side grip's target grows along its edge (up to 8px each way) and into
   the object (up to 6px), MEASURED per object so that no band reaches a neighbouring
   corner's centre or the inner half of the object (`sideGripReach`); corners paint above
   sides. A turned object keeps outward-only growth, because its grip elements are not
   rotated.

### What this branch built

All of §3; §4 items 1-8 for the object bar; OL-02 (the save); OL-05 (the grips); the
engine half of OL-04 (`setObjectAnchorKind(node, "floating", wrap)` as one action, with
`runWrapPlan` as the host seam); and the Position gallery's share of the same defect class
(`combineEditResults`, so both engine results' pages repaint).

## 5. The six-dot block grip (designed, not built: OL-09)

### 5.1 Who has one

V = verified here, B = believed, U = unsure.

| Kind | ONLYOFFICE 10.0.0 | Word desktop | Word web | Google Docs | Notion | Loop |
| --- | --- | --- | --- | --- | --- | --- |
| Paragraph | none (V) | none; Alt+Shift+Up/Down (B) | none (U) | none; Ctrl/⌘+Shift+Up/Down (B) | grip, menu, drag (B) | grip (B) |
| Heading | none (V) | Navigation pane drags a heading with its content (B) | U | none (B) | grip; a toggle heading carries its children (B) | grip (U) |
| List item | none (V) | none; Alt+Shift+Up/Down (B) | U | none (B) | grip; carries nested children (B) | grip (U) |
| Table, whole | move handle top-left (V) | same (B) | same (U) | none (U) | block grip (B) | grip (U) |
| Table row | row-select cursor band, no grip (V) | Alt+Shift+Up/Down moves rows (B) | U | U | U | U |
| In-line image | none; body drag (V) | Layout Options button (B) | U | toolbar below (B) | block grip (B) | grip (U) |
| Floating image, shape, text box | body drag only (V) | body drag (B) | U | body drag (B) | n/a | n/a |
| Chart | Chart elements button, no grip (V) | three chart buttons, no grip (B) | U | an image (U) | block grip (B) | grip (U) |

The owner's hypothesis (a grip on images, objects, drawings, tables, list items and table
rows) matches **Notion**, and believably Loop, not ONLYOFFICE or Word. It is still the right
control for a block-structured document, and it does not conflict with Word's keyboard
model, so the design adopts it where it makes sense and says where it does not.

### 5.2 Where the grip appears, and where it does not

- **Yes:** paragraph, heading, list item, block quote, table (whole), an in-line image or
  chart alone in its paragraph (it IS the block), a table of contents.
- **Not for floating objects.** No product uses a separate move grip for a float; all three
  editors move one by dragging its body, and the body is already the move target here (with
  a touch move pad, `paintMovePad`). A separate grip would put a fourth kind of target
  beside eight resize grips and a rotation grip on a small object, where the grips are
  already arbitrated pixel by pixel (§4 item 8). Decision: body drag stays a float's move;
  the block grip of the float's ANCHOR paragraph moves the paragraph and its floats with it.
- **Not inside running stories:** headers, footers and notes are excluded. A header is not a
  sequence of user-arranged blocks, and moving content into or out of one is a different
  command. Text-box bodies are excluded in the first build.
- **Table rows and columns** belong to the tables lane; they follow §5.6.

### 5.3 The command: one move, one undo step, OT-safe

- **One engine command:** `moveBlocks(fromStart, fromEnd, to)` moves the contiguous run of
  top-level blocks `[fromStart, fromEnd]` to before block `to`. For a list item the run is
  the item plus every following item of greater level: its nested children travel with it.
  It is one transaction of `DeleteBlocks` + `InsertBlocks` with the moved blocks' NodeIds
  PRESERVED, so comments, bookmarks and tracked changes anchored in them travel too
  (invariant I3: anchors are NodeIds, never offsets). One transaction is one undo step,
  labelled "Move paragraph", "Move table" or "Move list item".
- **OT:** the op pair is already in the closed op set (I2). A concurrent edit inside a moved
  block must transform against the pair as a MOVE (identity preserved), not as a delete
  that drops the remote edit. That is the one engine question to settle in `docs/107`
  before building.
- **Suggesting mode:** a tracked move (`w:moveFrom`/`w:moveTo`), which the model already
  imports. Until the writer emits a move, the grip is disabled in Suggesting, with that
  reason.
- **Never a DOM move.** The canvas repaints from the engine's damage, like every edit.

### 5.4 The interaction

- **Hover:** a ⋮⋮ grip (Material Symbols `drag_indicator`) appears in the left gutter (the
  right gutter in RTL), centred on the block's first line, 150ms after the pointer enters
  the block or the gutter beside it. It stays while the pointer is in the block or on the
  grip and hides 300ms after it leaves. One grip at a time.
- **Click:** opens the block menu: Move up, Move down, Duplicate, Delete, Turn into
  (Paragraph, Heading 1-3, Bulleted list, Numbered list, Quote). It is placed by
  `placeAnchored(gripRect, size, view, {prefer: "below", align: "start", dir})`.
- **Drag:** the grip is the only drag start (text keeps its own selection drag). While
  dragging, a 2px accent drop line sits at the block boundary nearest the pointer, indented
  to the target's level for a list item; the source block is dimmed; auto-scroll at the view
  edges reuses `edge_scroll.mjs`. Escape cancels. The drop runs `moveBlocks`.
- **Keyboard:** Alt+Shift+Up/Down moves the block (or the selected blocks) by one block,
  Word's chord, in the keymap so the Shortcuts page lists it, with the same command and
  undo label.
- **Screen reader:** the grip is a button, "Move or edit paragraph"; after a move the live
  region says "Moved paragraph after" and the first words of the block now above it.
- **Touch:** a long-press (500ms) on the grip, or on the block's gutter, starts the drag; a
  tap opens the block menu; the grip's target is 44x44.

### 5.5 Page-flow constraints

- A drop across a page break moves the block in the flow; pagination follows. There is no
  "page" target.
- The drop line skips table interiors in the first build. Moving a paragraph INTO a cell is
  the job of "Move to".
- Dragging the last paragraph of a section is refused with the reason, because it would
  carry the section's properties with it.
- Headers, footers and notes are excluded (§5.2).
- In a protected range or a read-only session the grip is shown disabled, with the reason.

### 5.6 The shared grip contract (every grip, the tables lane's included)

1. Glyph `drag_indicator`, a 16px icon in a 24px box (44px under a coarse pointer); `--muted`
   at rest, `--ink` on hover, the accent while dragging; tokens only.
2. Cursor `grab` at rest and `grabbing` while dragging (`pointer_cursor.mjs` names both).
3. One drop indicator style: a 2px `--accent` line with 6px end caps.
4. Its menu is an anchored popover through `object_anchor.mjs`.
5. A move is one engine command, one undo step, labelled by what moved.
6. Every drag has a keyboard chord (Alt+Shift+arrows).
7. A disabled grip shows, and says why.

### 5.7 Which parts need `main.js` or `casual-doc-wasm/src/lib.rs`

- `lib.rs`: the `moveBlocks` facade and its history label (the transaction itself belongs in
  `casual-doc-transaction`/`casual-doc-edit`), and a `blockAt(page, x, y)` hit-test.
- `main.js`: the hover detection on the page surface, the keymap binding and the live-region
  message. The grip, its menu, the drag preview and the drop line can be one new module
  (`block_grip.mjs`) behind a narrow `io`.

## 6. The inventory, after

Screenshots (lane scratchpad): `after1-01-image-selected.png`, `after1-02-scrolled.png`.

| Surface | Now |
| --- | --- |
| Object bar | 505px for the demo's picture (was 1,220px), below and centred on it, clear of every grip; `role="toolbar"` named per kind; Alt+F10 in, arrows, Escape out with focus returned; hides during a drag and while the object is out of view; 28px icon controls, 44px under a coarse pointer; every label localised in all 19 catalogues; a chart is named "Chart" |
| Side grips | A press 6px off the East grip's centre and 4px inside the picture resizes it |
| Saved floats | Every `wp:` frame carries the schema's required attributes and children |

## 7. Deliberately not done here

- The link card's adoption of `object_anchor.mjs`, Add text and Move to on the bar, OL-03,
  the wrap-row half of OL-04, and OL-06: each needs `main.js` or `lib.rs`. The exact
  changes are in the lane's report and in the OL rows.
- A chart cannot be wrapped or positioned (the model gives an embedded chart no anchor), so
  the bar offers no wrap for a chart rather than a control that cannot work.
- An equation has no surface because there is no equation editor yet.
- The block grip (§5) is designed only.

## 8. Open questions

1. Below about 480px of view width, should the bar collapse to Word's single Layout Options
   button instead of wrapping to two rows?
2. Alt+F10 needs Fn on most Mac keyboards. Is a second chord wanted there?
3. §5.3: does the OT transform treat `DeleteBlocks` + `InsertBlocks` of the same NodeIds as
   a move today? If not, the grip waits on that.
