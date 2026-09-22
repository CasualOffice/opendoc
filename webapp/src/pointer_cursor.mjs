// What the mouse pointer looks like, and why — one table, one router.
//
// `docs/109` HF-179. Reported as "cursor is always in editor (I) but i think it
// should change based on where it is.. like for dragging, changing side, check
// box and other .. im just giving example". The "just giving example" is the
// whole row: the defect is not three missing cursors, it is that the pointer
// shape never says what the thing under it will do.
//
// ## Why it looked like an I-beam everywhere
//
// A page sheet is ONE `<canvas class="page">`. Every glyph, image, shape, text
// box, table rule and checkbox on it is painted pixels inside that single
// element, so `document.elementFromPoint` returns the same node wherever the
// pointer is and CSS has nothing to key a cursor off. `style.css` therefore
// declared `.page { cursor: text }` once, statically, and the only variation on
// the whole surface was one class (`.page.link-hover`) toggled by one rAF probe
// for hyperlinks. Measured before this change on a grid of 160 points across
// the demo document's first sheet: the computed cursor was `text` at every
// single point that was over paper.
//
// So the fix cannot be more CSS. A canvas surface needs a HOVER ROUTER — ask
// the engine what is under the point, decide from that, write the answer onto
// the canvas. This module is the decision half, kept free of the DOM so the
// whole mapping is unit-testable in node.
//
// ## The rule the table encodes
//
// **The pointer must predict the press.** Whatever `onPointerDown` will do at
// this point is what the cursor has to promise, so the priority order below is
// not a preference — it is read off the pointer-down path in `main.js`: object
// first (`doc.objectAt`, which returns early), then a form checkbox
// (`toggleFormCheckboxAt`, which returns early), then the caret, with the link
// chip resolved on pointer-UP. It follows that a cursor must also account for
// what the current MODE refuses: object geometry edits are blocked outside
// Editing (`startObjectResize`, `enterCropMode`, and `finishObjectMove`'s gated
// commit), so an object under the pointer in Viewing or Suggesting must not
// offer a move it will then refuse.
//
// ## Competitive reference
//
// Read from `ONLYOFFICE/sdkjs` at 72b0421 (AGPL — behaviour only, nothing
// copied). Their choke point is `DrawingDocument.SetCursorType`
// (`word/Drawing/DrawingDocument.js:2031`), fed by `CDocument.UpdateCursorType`
// (`word/Editor/Document.js:8464`) and, for drawings,
// `common/Drawings/CommonController.js` / `DrawingObjectsHandlers.js` — the
// same shape as this router. Where the three products disagree, the row's `why`
// names which was followed and what was given up. The divergences are
// deliberate and there are five of them: hyperlink text, inline-picture body,
// the format painter, resize-cursor rotation, and the cursor during a move.
//
// ## Four kinds of owner
//
// Not every target is painted into the canvas, and not every target needs a
// cursor of its own:
//
//  - `router`    this module decides it and `main.js` writes it on the canvas
//  - `css`       a real DOM element in the overlay/ruler layer; `style.css`
//                already dresses it, and `selector` says where
//  - `mode`      an armed whole-surface cursor CSS owns; the router stands aside
//  - `by-design` a distinct target deliberately left at the same cursor as body
//                text, with the reasoning recorded so it is a decision and not
//                an omission
//  - `unprobed`  a real target the engine cannot yet report. Recorded, not
//                omitted: `pointer_cursor.test.mjs` lists them back, so a gap
//                stays visible instead of being indistinguishable from a target
//                nobody thought of.

/** Outward direction of each resize handle, in degrees clockwise from North,
 *  indexed by the engine's handle kind (0 = NW, then clockwise). */
const HANDLE_BEARINGS = [315, 0, 45, 90, 135, 180, 225, 270];

/** Bearing sector (45 degrees each, from North) to the axis cursor that points
 *  along it. Opposite bearings share a cursor because a resize axis has no
 *  direction, only an orientation. */
const BEARING_CURSORS = [
  "ns-resize", // N
  "nesw-resize", // NE
  "ew-resize", // E
  "nwse-resize", // SE
  "ns-resize", // S
  "nesw-resize", // SW
  "ew-resize", // W
  "nwse-resize", // NW
];

/**
 * The directional cursor for resize handle `handle` on an object rotated
 * `rotationDegrees` clockwise.
 *
 * The rotation term is what makes the cursor tell the truth about a turned
 * object: the grip drawn at the top-left of a shape rotated 90 degrees drags
 * the shape's width, not its height. ONLYOFFICE turns it too, but only in 90
 * degree steps — `getNumByCardDirection`
 * (`common/Drawings/Format/GraphicObjectBase.js:2172`) picks whichever of the
 * four mid-edge handles has the smallest screen Y and calls that North, so a
 * shape rotated 30 degrees shows exactly the unrotated cursors. Bucketing the
 * real angle into eight 45 degree sectors, as here, is strictly closer and no
 * harder; that is the deliberate divergence.
 *
 * **This engine does not model object rotation yet** — nothing in `ObjectBox`
 * carries an angle and `objectHandles` emits kinds 0..7 only — so every call
 * site passes 0 today. The parameter is written now rather than later because
 * the mapping is the part that is easy to get wrong, and this way it is stated
 * and tested once instead of rediscovered.
 *
 * O(1).
 *
 * @param {number} handle engine handle kind, 0..7 (NW, N, NE, E, SE, S, SW, W)
 * @param {number} [rotationDegrees] clockwise rotation of the object
 * @returns {string} a CSS cursor keyword
 */
export function resizeCursorForHandle(handle, rotationDegrees = 0) {
  const bearing = HANDLE_BEARINGS[handle];
  if (bearing === undefined) return "default";
  // Round to the nearest sector rather than truncating: at 44 degrees the grip
  // is one degree short of the next axis, and truncation would keep showing the
  // old one right up to the boundary.
  const turned = (((bearing + rotationDegrees) % 360) + 360) % 360;
  return BEARING_CURSORS[Math.round(turned / 45) % 8];
}

/**
 * Every distinct thing the pointer can be over on the editing surface, the
 * cursor it gets, and who writes it.
 *
 * Ordered by router priority: the first row whose `when` accepts the probe
 * wins. Rows with no `when` are not the router's to decide (see the owner
 * kinds above), and the test suite holds each of them to its own contract.
 */
export const CURSOR_TARGETS = [
  // ---- A gesture already in flight outranks whatever is under the pointer ---
  // During a drag the pointer routinely outruns the thing it grabbed. Deciding
  // from position would flick the cursor back to `text` the moment it left the
  // handle, which reads as "the drag ended" while it has not.
  {
    id: "drag-object-resize",
    cursor: null,
    directional: true,
    owner: "router",
    selector: null,
    gesture: "Resizing an object",
    why:
      "The axis cursor is held for the whole drag, as in Word and in ONLYOFFICE " +
      "(`DrawingStates.js:2011`). Directional: the handle decides, through " +
      "`resizeCursorForHandle`.",
    when: (p) => p.drag === "object-resize",
  },
  {
    id: "drag-object-crop",
    cursor: null,
    directional: true,
    owner: "router",
    selector: null,
    gesture: "Cropping a picture",
    why:
      "Crop is a resize of the kept rectangle and reuses the handle numbering, " +
      "so it reuses the axis cursors — ONLYOFFICE's `hitToCropHandles` does the same.",
    when: (p) => p.drag === "object-crop",
  },
  {
    id: "drag-object-move",
    cursor: "move",
    owner: "router",
    selector: null,
    gesture: "Moving an object",
    why:
      "DIVERGENCE: ONLYOFFICE switches to `default` for the duration of a move " +
      "(`DrawingStates.js:2130`), which reads as though the object was dropped. " +
      "Word keeps the move arrow and so do we. `move`, not `grabbing`: " +
      "`grab`/`grabbing` is this product's ruler-tab idiom, and ONLYOFFICE reserves " +
      "it for the hand/pan tool.",
    when: (p) => p.drag === "object-move",
  },
  {
    id: "drag-table-column",
    cursor: "col-resize",
    owner: "router",
    selector: null,
    gesture: "Changing a column width",
    why: "Matches the handle's own hover cursor, so the shape does not change on press.",
    when: (p) => p.drag === "table-column",
  },

  // ---- Whole-surface modes ------------------------------------------------
  // Below the object and table drags, deliberately: with the painter armed a
  // press on a drawing still selects and moves it (`onPointerDown` reaches the
  // object path first, and the paint branch on pointer-UP only runs for a
  // gesture that produced one). Above the TEXT drag, equally deliberately: a
  // drag-select IS how the painter is used over a range, so the brush has to
  // survive it.
  {
    id: "format-painter",
    cursor: null,
    owner: "mode",
    selector: "body.is-format-painting .page-wrap",
    gesture: "Painting formatting onto the next thing clicked",
    why:
      "An armed mode owns the whole sheet — ONLYOFFICE does the same through " +
      "`LockCursorType`, with its own `text-copy`/`shape-copy` images. The brush " +
      "here is already a designed data-URI cursor in style.css, so the router's " +
      "job is to CLEAR its inline cursor: an inline style would beat that rule.",
    when: (p) => p.formatPainting === true,
  },
  {
    id: "drag-text",
    cursor: "text",
    owner: "router",
    selector: null,
    gesture: "Extending a selection",
    why: "Selecting stays an I-beam in all three products — the pointer is still in text.",
    when: (p) => p.drag === "text",
  },

  // ---- Overlay chrome: real DOM elements, dressed by CSS -------------------
  {
    id: "object-handle",
    cursor: null,
    directional: true,
    owner: "css",
    selector: '.overlay .object-handle[data-handle="%d"]',
    gesture: "Dragging a side or corner to resize",
    why: "Eight grips, eight axis cursors — the mapping every editor shares.",
  },
  {
    id: "object-crop-handle",
    cursor: null,
    directional: true,
    owner: "css",
    selector: '.overlay .object-crop-handle[data-handle="%d"]',
    gesture: "Dragging a crop edge",
    why: "Same eight axes as resize; crop differs in what moves, not in which way it moves.",
  },
  {
    id: "table-column-handle",
    cursor: "col-resize",
    owner: "css",
    selector: ".overlay .table-col-resize-handle",
    gesture: "Dragging a column boundary",
    why: "`col-resize`, the same keyword ONLYOFFICE uses for a vertical cell border.",
  },
  {
    id: "checklist-checkbox",
    cursor: "pointer",
    owner: "css",
    selector: ".overlay .checklist-marker",
    gesture: "Ticking a checklist item",
    why: "A control, not text. Same answer as the form checkbox below: one product, one checkbox.",
  },
  {
    id: "comment-anchor",
    cursor: "pointer",
    owner: "css",
    selector: ".overlay .review-comment-marker",
    gesture: "Opening the comment on this text",
    why:
      "Clicking activates the comment card, so it is a link-like affordance. " +
      "ONLYOFFICE gives a comment anchor NO hover affordance at all — there is no " +
      "comment member in its mouse-move type enum — so the bar here is Docs.",
  },
  {
    id: "tracked-change",
    cursor: "text",
    owner: "css",
    selector: ".overlay .review-revision-marker",
    gesture: "Placing the caret inside a tracked change",
    why:
      "The marker takes pointer events only so its attribution tooltip shows; the " +
      "pointerdown still falls through to caret placement. It inherited the ARROW " +
      "from the overlay layer, promising the one thing it does not do — found by " +
      "this row's own audit and fixed with it. ONLYOFFICE agrees: `text` plus a " +
      "Review tooltip (`word/Editor/Paragraph.js:12703`).",
  },
  {
    id: "running-content-marker",
    cursor: "pointer",
    owner: "css",
    selector: ".running-marker",
    gesture: "Entering the header or footer",
    why: "A real button floated over the band.",
  },
  {
    id: "ruler-indent-marker",
    cursor: "ew-resize",
    owner: "css",
    selector: ".ruler-marker",
    gesture: "Dragging a paragraph indent",
    why: "A horizontal-only drag, so a horizontal axis cursor.",
  },
  {
    id: "ruler-tab-glyph",
    cursor: "grab",
    owner: "css",
    selector: ".tab-glyph",
    gesture: "Dragging an existing tab stop",
    why: "A loose object picked up off the ruler, which is what `grab` is for.",
  },
  {
    id: "ruler-tab-corner",
    cursor: "pointer",
    owner: "css",
    selector: ".tab-corner",
    gesture: "Cycling the tab type",
    why: "A button.",
  },
  {
    id: "ruler-content",
    cursor: "copy",
    owner: "css",
    selector: ".ruler-content",
    gesture: "Adding a tab stop at this position",
    why: "A click CREATES something, which `copy` signals and `pointer` does not.",
  },
  {
    id: "page-background",
    cursor: "default",
    owner: "css",
    selector: ".pages",
    gesture: "Nothing — this is the desk, not the paper",
    why:
      "DIVERGENCE from ONLYOFFICE, which clamps an off-page coordinate back onto " +
      "the nearest sheet and so shows `text` out here. This editor does not: a " +
      "press on the grey surround resolves to no page and does nothing, so `text` " +
      "would be a lie. Docs shows the arrow.",
  },

  // ---- The canvas: everything below is decided by asking the engine --------
  {
    id: "object-locked",
    cursor: "default",
    owner: "router",
    selector: null,
    gesture: "Selecting the object; its geometry cannot be changed here",
    why:
      "Resize, crop and the move commit are all refused outside Editing mode, so " +
      "`move` would promise a gesture the release then rejects. ONLYOFFICE reaches " +
      "the same answer from the other end: read-only suppresses the handles and " +
      "drops a drawing to `default` (`GraphicObjects.js:285`).",
    when: (p) => !!p.object && !p.insideObject && p.geometryBlocked === true,
  },
  {
    id: "object-movable",
    cursor: "move",
    owner: "router",
    selector: null,
    gesture: "Dragging the object to a new position",
    why:
      "The press starts a move (`startObjectMove` when `canMove`), so the pointer " +
      "promises exactly that. This row also settles the linked-PICTURE case: " +
      "`objectAt` is resolved before the link on pointer-down, so the object cursor " +
      "wins and a linked picture does NOT get `pointer`. ONLYOFFICE lands in the " +
      "same place — `checkDrawingHyperlinkAndMacro` fires the tooltip but only " +
      "overrides the cursor inside the text rect, which a picture is not " +
      "(`CommonController.js:861`) — and so does Word.",
    when: (p) => !!p.object && !p.insideObject && p.object.canMove === true,
  },
  {
    id: "object-selectable",
    cursor: "default",
    owner: "router",
    selector: null,
    gesture: "Selecting the object",
    why:
      "DIVERGENCE: ONLYOFFICE shows `move` over an INLINE picture too, because it " +
      "can drag one. This engine cannot — an inline object reports `canMove: false` " +
      "and the click only selects — and a cursor that promises a drag which does " +
      "nothing is worse than the arrow. Docs also shows the arrow here.",
    when: (p) => !!p.object && !p.insideObject,
  },
  {
    id: "form-checkbox",
    cursor: "pointer",
    owner: "router",
    selector: null,
    gesture: "Ticking the box",
    why:
      "The press ticks it (`toggleFormCheckboxAt` returns early), so it is a " +
      "control, and ONLYOFFICE agrees (`Paragraph.js:12713`). One lookup covers " +
      "both the `w14:checkbox` content control and the legacy FORMCHECKBOX, so one " +
      "row covers both. Suppressed when edits are refused, because then it is not " +
      "a control, only text.",
    when: (p) => p.formCheckbox === true && p.editsBlocked !== true,
  },
  {
    id: "hyperlink",
    cursor: "pointer",
    owner: "router",
    selector: null,
    gesture: "Following the link",
    why:
      "DIVERGENCE, and the considered one. Word requires Ctrl+click and keeps the " +
      "I-beam; ONLYOFFICE copies that exactly, giving `pointer` only while Ctrl is " +
      "held (`Paragraph.js:12712`), with no plain-click path for a .docx. This " +
      "editor follows a link on an UNMODIFIED click — pointer-up opens the link " +
      "chip — so the hand is the truthful shape for this product, and it is what " +
      "was already shipped here (`.page.link-hover`).",
    when: (p) => p.link === true,
  },
  {
    id: "read-only-text",
    cursor: "text",
    owner: "router",
    selector: null,
    gesture: "Selecting text to copy",
    why:
      "Deliberately the SAME as ordinary body text. Viewing still selects and " +
      "copies, and all three products keep the I-beam; changing it would claim the " +
      "surface is inert when it is not. Its own row so the decision is visible, and " +
      "so the surface can be asserted to have REACHED this state rather than " +
      "having fallen through to the default.",
    when: (p) => p.editsBlocked === true,
  },
  {
    id: "running-content-band",
    cursor: "text",
    owner: "router",
    selector: null,
    gesture: "Clicking to put the caret in the nearest body line; double-clicking to edit the band",
    why:
      "Deliberately unchanged, against the temptation to make it an arrow. A single " +
      "click in the band DOES place a caret — `resolveClick` routes it into the body " +
      "— so the I-beam is accurate, and Word, Docs and ONLYOFFICE all leave it " +
      "alone (ONLYOFFICE's `UpdateCursorType` never branches on the band at all). " +
      "The band's own affordance is the `+ Header` button, which carries the hand.",
    when: (p) => !!p.band && !p.inRunningStory,
  },
  {
    id: "body-text",
    cursor: "text",
    owner: "router",
    selector: null,
    gesture: "Placing the caret / selecting",
    why: "The default, and the only one that was ever right before this change.",
    when: () => true,
  },

  // ---- Targets deliberately left at the body-text cursor ------------------
  {
    id: "table-cell",
    cursor: "text",
    owner: "by-design",
    selector: null,
    gesture: "Placing the caret in the cell",
    why:
      "A cell interior is text. ONLYOFFICE delegates a cell hit straight to its " +
      "content (`Table.js:3724`) and so reaches `text` as well. Only the BORDERS " +
      "differ, and those are separate rows.",
  },
  {
    id: "form-text-field",
    cursor: "text",
    owner: "by-design",
    selector: null,
    gesture: "Typing into the field",
    why:
      "A FORMTEXT result is text you type into, so the I-beam is already right. " +
      "DIVERGENCE: ONLYOFFICE turns the ENTIRE document `default` in form-filling " +
      "mode, the inside of the text fields included (`Paragraph.js:12714`), which " +
      "signals 'inert' over the one place the user is meant to type.",
  },
  {
    id: "content-control",
    cursor: "text",
    owner: "by-design",
    selector: null,
    gesture: "Placing the caret inside the control",
    why:
      "An inline SDT holds text; only its chrome would differ, and this editor " +
      "paints no SDT chrome. ONLYOFFICE agrees for the content (`Paragraph.js:12679`).",
  },
  {
    id: "footnote-reference",
    cursor: "text",
    owner: "by-design",
    selector: null,
    gesture: "Placing the caret beside the reference mark",
    why:
      "A reference mark is a run of text and clicking it does not navigate, so the " +
      "I-beam is correct and there is nothing to probe. ONLYOFFICE is the same: " +
      "`text` plus a Footnote tooltip, its cursor branch never looking at notes " +
      "(`Paragraph.js:12697`). It earns a row because the obvious guess is `pointer`.",
  },

  // ---- Known gaps, recorded so they cannot be mistaken for oversights -----
  {
    id: "object-rotate-handle",
    cursor: "crosshair",
    owner: "unprobed",
    selector: null,
    gesture: "Rotating the object",
    why:
      "There is no rotation handle because there is no rotation operation: " +
      "`objectHandles` emits kinds 0..7 (the eight resize grips) and `ObjectBox` " +
      "carries no angle. `crosshair` is what ONLYOFFICE uses " +
      "(`CommonController.js:1531`) and is recorded as the fallback; Word and Docs " +
      "both use a dedicated rotate glyph, which is the better answer when the " +
      "feature lands. `resizeCursorForHandle` already takes the angle, so the " +
      "resize grips will turn correctly on the same day.",
  },
  {
    id: "table-row-boundary",
    cursor: "row-resize",
    owner: "unprobed",
    selector: null,
    gesture: "Dragging a row boundary",
    why:
      "Only COLUMN resize handles exist (`tableColumnResizeHandles`), and only for " +
      "the table the caret is in — so a boundary is not a hit target until a handle " +
      "is painted on it. ONLYOFFICE hit-tests every border within 3 px and also " +
      "carries row/column/cell SELECT zones with custom image cursors, none of " +
      "which this editor has yet.",
  },
];

/** The cursor a directional row resolves to for `handle`, or the row's fixed
 *  cursor. Split out so the CSS guard and the router agree by construction. */
function cursorOf(row, probe) {
  if (!row.directional) return row.cursor;
  return resizeCursorForHandle(probe?.handle ?? 0, probe?.rotation ?? 0);
}

/** Rows the router itself decides, in priority order. */
const ROUTED = CURSOR_TARGETS.filter((row) => typeof row.when === "function");

/** Every row by id. */
export const TARGET_BY_ID = new Map(CURSOR_TARGETS.map((row) => [row.id, row]));

/**
 * The pointer target and cursor for one hover, from a plain description of what
 * is under the pointer.
 *
 * The probe is data, never a DOM node, which is what keeps the whole mapping
 * testable in node. Fields, all optional:
 *
 *  - `overlayTarget`  id of an overlay row when the pointer is over that
 *                     element; CSS has already decided and the router only
 *                     reports which row it is looking at
 *  - `drag`           `"object-resize" | "object-crop" | "object-move" |
 *                     "table-column" | "text"` while a gesture is in flight
 *  - `handle`         engine handle kind 0..7, for a directional row
 *  - `rotation`       the object's clockwise rotation in degrees
 *  - `formatPainting` the format painter is armed
 *  - `onPage`         the pointer is over a page sheet at all
 *  - `object`         `{ canMove }` for the object under the pointer
 *  - `insideObject`   the pointer is inside the object currently being EDITED,
 *                     which makes it a text surface rather than a target
 *  - `formCheckbox`   the point is inside a form checkbox control
 *  - `link`           a hyperlink is painted at the point
 *  - `band`           `"header" | "footer" | ""`
 *  - `inRunningStory` that band is already open for editing
 *  - `editsBlocked`   content edits are refused (Viewing, or an engine refusal)
 *  - `geometryBlocked` object geometry edits are refused (anything but Editing)
 *
 * O(1) in document size: it reads booleans the caller has already gathered.
 *
 * @param {object} probe
 * @returns {{ target: string, cursor: string | null }}
 */
export function resolvePointerCursor(probe = {}) {
  if (probe.overlayTarget) {
    const row = TARGET_BY_ID.get(probe.overlayTarget);
    if (!row) {
      throw new Error(
        `unknown overlay pointer target "${probe.overlayTarget}" — every hit target ` +
          "must have a row in CURSOR_TARGETS",
      );
    }
    return { target: row.id, cursor: cursorOf(row, probe) };
  }
  if (probe.onPage === false) {
    const row = TARGET_BY_ID.get("page-background");
    return { target: row.id, cursor: row.cursor };
  }
  for (const row of ROUTED) {
    if (row.when(probe)) return { target: row.id, cursor: cursorOf(row, probe) };
  }
  // Unreachable: `body-text` accepts everything. Thrown rather than defaulted so
  // that a reordering which strands the fallback fails loudly instead of
  // silently producing no cursor at all.
  throw new Error("no pointer target matched — CURSOR_TARGETS lost its fallback row");
}
