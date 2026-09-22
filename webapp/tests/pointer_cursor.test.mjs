// The guards on the pointer-cursor table (`docs/109` HF-179).
//
// The owner's report was "cursor is always in editor (I) but i think it should
// change based on where it is.. like for dragging, changing side, check box and
// other .. **im just giving example**". So the thing that has to stay fixed is
// not three cursors — it is the CLASS: every hit target on the editing surface
// says what it will do.
//
// The load-bearing test in this file is therefore
// `every interactive element on the page surface has a row`. A page sheet is
// one canvas, so a new hit target can only become clickable one of two ways:
// the router grows a branch, or an overlay element opts back into pointer
// events with `pointer-events: auto`. That second one is mechanically
// detectable, and this file detects it — adding an overlay control without a
// cursor fails here, which is what stops the surface drifting back.
//
// Everything else guards the table itself: no row without a cursor, no router
// row that cannot be reached, and no `owner: "css"` row whose claim about
// `style.css` is not actually true.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  CURSOR_TARGETS,
  TARGET_BY_ID,
  resizeCursorForHandle,
  resolvePointerCursor,
} from "../src/pointer_cursor.mjs";

// Comments stripped: every scan below is about what the stylesheet DOES, and a
// comment that discusses `pointer-events: auto` or names a retired class would
// otherwise read as the declaration itself. (Both happened while writing this.)
const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "",
);

const OWNERS = ["router", "css", "mode", "by-design", "unprobed"];

/** A probe that selects each router row, so the whole cascade is exercised
 *  rather than only the branches somebody remembered. Keyed by target id: a new
 *  routed row with no entry here fails `every routed row is reachable`. */
const PROBES = {
  "drag-object-resize": { drag: "object-resize", handle: 2 },
  "drag-object-crop": { drag: "object-crop", handle: 1 },
  "drag-object-move": { drag: "object-move" },
  "drag-table-column": { drag: "table-column" },
  "format-painter": { formatPainting: true },
  "drag-text": { drag: "text" },
  "object-locked": { object: { canMove: true }, geometryBlocked: true },
  "object-movable": { object: { canMove: true } },
  "object-selectable": { object: { canMove: false } },
  "form-checkbox": { formCheckbox: true },
  hyperlink: { link: true },
  "read-only-text": { editsBlocked: true },
  "running-content-band": { band: "header" },
  "body-text": {},
};

/** Declarations of `cursor` for `selector` in style.css, in source order.
 *  Deliberately a text scan rather than a CSS parser: the point is to read what
 *  the stylesheet actually says, with no dependency to keep current. */
function cursorsFor(selector) {
  const out = [];
  // Rule blocks are either `sel {\n … \n}` or a whole rule on one line; both
  // shapes are in this stylesheet, so match the block and then its property.
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const rule = new RegExp(`(^|[},\\s])${escaped}\\s*(,[^{]*)?\\{([^}]*)\\}`, "gm");
  for (const match of css.matchAll(rule)) {
    const decl = match[3].match(/(?:^|;)\s*cursor\s*:\s*([^;}]+)/m);
    if (decl) out.push(decl[1].trim());
  }
  return out;
}

test("every row is complete: an id, a cursor, a gesture and a reason", () => {
  // A null `cursor` is legitimate in exactly two places: a directional row,
  // where the handle decides, and the format-painter MODE, where the stylesheet
  // owns the shape and the router's job is to stand aside. Everywhere else a
  // missing cursor is the bug.
  const cursorOk = (row) =>
    row.directional || row.owner === "mode"
      ? row.cursor === null
      : typeof row.cursor === "string" && row.cursor.length > 0;
  const broken = CURSOR_TARGETS.filter(
    (row) => !row.id || !OWNERS.includes(row.owner) || !row.gesture || !row.why || !cursorOk(row),
  ).map((row) => row.id || "(unnamed)");
  assert.deepEqual(
    broken,
    [],
    "a hit target with no cursor, no stated gesture or no reasoning is the defect " +
      "this table exists to prevent",
  );
});

test("ids are unique", () => {
  const ids = CURSOR_TARGETS.map((row) => row.id);
  assert.equal(new Set(ids).size, ids.length, "a duplicate id silently shadows a row");
  assert.equal(TARGET_BY_ID.size, ids.length);
});

test("every routed row is reachable, and resolves to its own cursor", () => {
  const routed = CURSOR_TARGETS.filter((row) => row.owner === "router" || row.owner === "mode");
  const missing = routed.filter((row) => !(row.id in PROBES)).map((row) => row.id);
  assert.deepEqual(
    missing,
    [],
    "a routed row with no probe here has never been shown to be reachable at all",
  );
  for (const row of routed) {
    const { target, cursor } = resolvePointerCursor(PROBES[row.id]);
    assert.equal(target, row.id, `probe for ${row.id} resolved to ${target} instead`);
    if (row.directional) {
      assert.equal(cursor, resizeCursorForHandle(PROBES[row.id].handle));
    } else {
      assert.equal(cursor, row.cursor);
    }
  }
});

test("the cascade has a fallback and never answers with nothing", () => {
  assert.deepEqual(resolvePointerCursor({}), { target: "body-text", cursor: "text" });
  assert.deepEqual(resolvePointerCursor({ onPage: false }), {
    target: "page-background",
    cursor: "default",
  });
});

test("an overlay element is reported, not re-decided, and an unknown one throws", () => {
  // The pointer is over real chrome: CSS already drew the cursor, so the router
  // reports which row it is looking at rather than inventing an answer — that is
  // what lets the same table cover both halves of the surface.
  assert.deepEqual(resolvePointerCursor({ overlayTarget: "checklist-checkbox" }), {
    target: "checklist-checkbox",
    cursor: "pointer",
  });
  assert.deepEqual(resolvePointerCursor({ overlayTarget: "object-handle", handle: 3 }), {
    target: "object-handle",
    cursor: "ew-resize",
  });
  // Position must not override the element actually under the pointer.
  assert.equal(
    resolvePointerCursor({ overlayTarget: "comment-anchor", object: { canMove: true } }).target,
    "comment-anchor",
  );
  // And an id with no row is the failure this table exists to make loud, not a
  // silent fallback to the I-beam.
  assert.throws(
    () => resolvePointerCursor({ overlayTarget: "footnote-ref-marker" }),
    /every hit target must have a row/,
  );
});

test("precedence follows what the press will actually do", () => {
  // Pointer-down resolves the object BEFORE the link, so a linked picture gets
  // the object's cursor. This is the linked-picture case, and it is the one the
  // table would most plausibly get backwards.
  assert.equal(
    resolvePointerCursor({ object: { canMove: true }, link: true }).target,
    "object-movable",
  );
  // …and before the caret, so a form checkbox under an object loses too.
  assert.equal(
    resolvePointerCursor({ object: { canMove: false }, formCheckbox: true }).target,
    "object-selectable",
  );
  // A form checkbox outranks a link, matching `toggleFormCheckboxAt` returning
  // before the link chip is ever considered.
  assert.equal(resolvePointerCursor({ formCheckbox: true, link: true }).target, "form-checkbox");
  // An object you are editing is a text surface, not a target.
  assert.equal(
    resolvePointerCursor({ object: { canMove: true }, insideObject: true }).target,
    "body-text",
  );
  // Refusals demote: an object whose geometry cannot be changed must not offer
  // a move the release will reject.
  assert.equal(
    resolvePointerCursor({ object: { canMove: true }, geometryBlocked: true }).cursor,
    "default",
  );
  assert.equal(
    resolvePointerCursor({ formCheckbox: true, editsBlocked: true }).target,
    "read-only-text",
  );
  // A gesture in flight outranks position — the pointer outruns what it grabbed.
  assert.equal(
    resolvePointerCursor({ drag: "object-move", object: { canMove: false }, link: true }).cursor,
    "move",
  );
  // The format painter outranks a text drag (drag-select is how it is used over
  // a range) but not an object drag (a press on a drawing still moves it).
  assert.equal(
    resolvePointerCursor({ formatPainting: true, drag: "text" }).target,
    "format-painter",
  );
  assert.equal(
    resolvePointerCursor({ formatPainting: true, drag: "object-move" }).target,
    "drag-object-move",
  );
  // An open band is text you are typing in, not a band to enter.
  assert.equal(
    resolvePointerCursor({ band: "header", inRunningStory: true }).target,
    "body-text",
  );
});

test("the format painter hands the cursor back to CSS", () => {
  const { cursor } = resolvePointerCursor({ formatPainting: true });
  assert.equal(
    cursor,
    null,
    "the router must write NO inline cursor while the brush is armed — an inline " +
      "style beats the stylesheet rule that draws the brush",
  );
  const declared = cursorsFor("body.is-format-painting .page-wrap");
  assert.ok(
    declared.length > 0 && /url\(/.test(declared[0]),
    "style.css must still draw the brush for the mode the router stands aside for",
  );
});

test("resize handles map to their axis, and turn with the object", () => {
  // The engine's handle kinds: 0 NW, 1 N, 2 NE, 3 E, 4 SE, 5 S, 6 SW, 7 W.
  assert.deepEqual(
    [0, 1, 2, 3, 4, 5, 6, 7].map((h) => resizeCursorForHandle(h)),
    [
      "nwse-resize",
      "ns-resize",
      "nesw-resize",
      "ew-resize",
      "nwse-resize",
      "ns-resize",
      "nesw-resize",
      "ew-resize",
    ],
  );
  // Quarter turn: every grip moves two sectors, so N reads as E.
  assert.equal(resizeCursorForHandle(1, 90), "ew-resize");
  assert.equal(resizeCursorForHandle(0, 90), "nesw-resize");
  // Half a sector short of the switch, and half a sector past it.
  assert.equal(resizeCursorForHandle(1, 22), "ns-resize");
  assert.equal(resizeCursorForHandle(1, 23), "nesw-resize");
  // A full turn, and a negative one, are the identity.
  assert.equal(resizeCursorForHandle(4, 360), resizeCursorForHandle(4));
  assert.equal(resizeCursorForHandle(4, -360), resizeCursorForHandle(4));
  // Out of range is a shape, not a crash.
  assert.equal(resizeCursorForHandle(9), "default");
});

test("every css-owned row says something style.css actually says", () => {
  for (const row of CURSOR_TARGETS.filter((r) => r.owner === "css")) {
    if (row.directional) {
      for (let handle = 0; handle < 8; handle++) {
        const selector = row.selector.replace("%d", String(handle));
        assert.deepEqual(
          cursorsFor(selector),
          [resizeCursorForHandle(handle)],
          `${selector} must declare the axis cursor for handle ${handle}`,
        );
      }
    } else {
      assert.ok(
        cursorsFor(row.selector).includes(row.cursor),
        `${row.id}: style.css does not declare "cursor: ${row.cursor}" on ${row.selector}`,
      );
    }
  }
});

// ---------------------------------------------------------------------------
// The load-bearing guard.

/** Selectors in style.css that opt an element back into pointer events. An
 *  overlay child is `pointer-events: none` by inheritance, so opting back in is
 *  the one and only way to become a hit target on the page surface — which
 *  makes this list the complete set of them, mechanically. */
function interactiveSelectors() {
  const out = new Set();
  for (const match of css.matchAll(/([^{}]+)\{([^}]*pointer-events\s*:\s*auto[^}]*)\}/g)) {
    // Strip any comment that ran into the selector text, then split the list.
    const selectors = match[1].replace(/\/\*[\s\S]*?\*\//g, "").split(",");
    for (const raw of selectors) {
      const selector = raw.trim().replace(/\s+/g, " ");
      if (selector) out.add(selector);
    }
  }
  return out;
}

/** Interactive elements that are chrome floating OVER the page rather than
 *  targets painted into it. Each is a normal control with its own buttons, so
 *  it is not part of the document surface the router speaks for. Listed
 *  explicitly so that excluding something is as deliberate as including it. */
const NOT_A_PAGE_TARGET = new Set(["", ".object-context-bar"]);

test("every interactive element on the page surface has a row", () => {
  const covered = new Set(
    CURSOR_TARGETS.filter((row) => row.owner === "css").map((row) =>
      // Directional rows name one selector per handle; the base selector is
      // what the stylesheet's `pointer-events` rule is written on.
      row.selector.replace(/\[data-handle="%d"\]/, ""),
    ),
  );
  const uncovered = [...interactiveSelectors()].filter(
    (selector) => !NOT_A_PAGE_TARGET.has(selector) && !covered.has(selector),
  );
  assert.deepEqual(
    uncovered,
    [],
    "these elements take pointer events on the page surface but no row of " +
      "CURSOR_TARGETS says what cursor they get, so they will inherit the arrow " +
      "from the overlay layer and promise something they do not do. Add a row " +
      "(and its `cursor` declaration), or list it in NOT_A_PAGE_TARGET with a " +
      `reason: ${uncovered.join(" / ")}`,
  );
});

test("the canvas keeps its pre-router baseline and loses the class it replaced", () => {
  assert.ok(
    cursorsFor(".page").includes("text"),
    ".page must still declare `cursor: text` — it is what a sheet reads as " +
      "before the first probe of a frame resolves",
  );
  assert.ok(
    !/\.page\.link-hover/.test(css),
    "the link-hover class was the whole of the old one-target behaviour; the " +
      "router replaced it, and leaving it behind means two mechanisms for one cursor",
  );
});

test("the known gaps are the ones we think they are", () => {
  assert.deepEqual(
    CURSOR_TARGETS.filter((row) => row.owner === "unprobed").map((row) => row.id),
    ["object-rotate-handle", "table-row-boundary"],
    "an `unprobed` row is a target the engine cannot yet report. Wiring one up " +
      "means changing its owner here; adding one means the surface grew a target " +
      "we cannot see. Either way this list is meant to be edited deliberately.",
  );
});
