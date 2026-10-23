// The table gutter's zone geometry (`docs/141` D-2).
//
// Every assertion here is on a GUARANTEE the design makes, not on a shape it
// happens to have:
//
//   * the zone reaches the table's own leading BORDER and a clamped hair past
//     it — the border is the affordance — while the rest of every cell still
//     places the caret at every band height;
//   * the leading borders carry no resize boundary, so the selection gesture and
//     the resize gesture never contest a pixel;
//   * the strip resolves the band a user is pointing BESIDE;
//   * the `+` insert zone cannot eat the strip segment that selects the band,
//     however short the band is;
//   * the review mode decides what ARMS rather than what fails on release;
//   * a hover stays linear in the size of the table.
import test from "node:test";
import assert from "node:assert/strict";

import {
  COARSE_TARGET_PX,
  INSIDE_PX,
  TARGET_PX,
  armTableGutter,
  dropBoundaryAt,
  gutterAt,
  gutterLanes,
  moveTargetIndex,
  stripRect,
  tableBands,
  targetPxFor,
} from "../src/table_gutter_zones.mjs";
import { boundaryAt } from "../src/table_chrome_zones.mjs";

/** A `tableChromeOnPage`-shaped table: `rows` rows of `rowH` twips and `cols`
 *  columns of `colW`, with its top-left corner at (x, y). */
function chromeTable({ rows = 3, cols = 2, rowH = 288, colW = 1440, x = 1440, y = 1440, regular = true, node = "t1" } = {}) {
  return {
    node,
    regular,
    rows,
    columns: cols,
    x,
    y,
    w: cols * colW,
    h: rows * rowH,
    rowEdges: Array.from({ length: rows }, (_, i) => ({
      i,
      x,
      y: y + (i + 1) * rowH,
      w: cols * colW,
      h: 0,
      width: 0,
      height: rowH,
      outer: i === rows - 1,
      anchor: `r${i}`,
    })),
    colEdges: Array.from({ length: cols }, (_, i) => ({
      i,
      x: x + (i + 1) * colW,
      y,
      w: 0,
      h: rows * rowH,
      width: colW,
      height: 0,
      outer: i === cols - 1,
      anchor: `c${i}`,
    })),
  };
}

/** The tolerances a 100%-zoom page gives: twips per CSS px is 15. */
const OPTS = {
  targetX: TARGET_PX * 15,
  targetY: TARGET_PX * 15,
  insideX: INSIDE_PX * 15,
  insideY: INSIDE_PX * 15,
};
/** One target, in twips at 100%. */
const T = TARGET_PX * 15;

test("the zone reaches the table's own leading border, and stops a hair past it", () => {
  // The owner's correction: the BORDER is the affordance, not a bar beside it.
  // ONLYOFFICE's `private_CheckHitInBorder` reports `RowSelection` for
  // `X <= X_cell_start` and `ColumnSelection` for `Y <= Y_cell_start + nRadius`,
  // so the border line itself belongs to the selection there. This is that.
  const table = chromeTable();
  const middleRow = table.y + 144;
  const middleCol = table.x + 720;

  // ON the border, to the twip.
  assert.equal(gutterAt([table], table.x, middleRow, OPTS)?.axis, "row");
  assert.equal(gutterAt([table], middleCol, table.y, OPTS)?.axis, "column");
  // And a hair past it, which is what makes a 1px-wide painted border aimable.
  assert.equal(gutterAt([table], table.x + INSIDE_PX * 15 - 1, middleRow, OPTS)?.axis, "row");

  // The load-bearing half is unchanged: the REST of every cell still places a
  // caret. Sampled from beyond the inside reach to the table's far side.
  for (let x = table.x + INSIDE_PX * 15; x < table.x + table.w; x += 120) {
    for (let y = table.y + INSIDE_PX * 15; y < table.y + table.h; y += 60) {
      assert.equal(gutterAt([table], x, y, OPTS), null, `(${x}, ${y}) is inside a cell`);
    }
  }
});

test("no leading border is a resize boundary, and a resize is reachable the moment the zone ends", () => {
  // The tension the border-as-affordance design has to answer. It is answered
  // structurally rather than by tuning: `tableChromeOnPage` reports one boundary
  // per band at its CLOSING edge (`hittest.rs` — row edges at
  // `row.top + row.height`), so the table's top and left borders are not
  // boundaries at all and no resize can ever begin ON one.
  const table = chromeTable();
  const tol = 5 * 15;
  // Along each leading border, at the middle of every band it passes — the
  // whole of it except where a perpendicular inner boundary crosses, which is
  // the next paragraph. Nothing arms a resize on any of it.
  for (const y of [table.y + 144, table.y + 432, table.y + 720]) {
    assert.equal(
      boundaryAt([table], table.x, y, tol, tol),
      null,
      `the left border at y=${y} must not arm a resize`,
    );
    assert.equal(gutterAt([table], table.x, y, OPTS)?.axis, "row");
  }
  for (const x of [table.x + 720, table.x + 2160]) {
    assert.equal(
      boundaryAt([table], x, table.y, tol, tol),
      null,
      `the top border at x=${x} must not arm a resize`,
    );
    assert.equal(gutterAt([table], x, table.y, OPTS)?.axis, "column");
  }

  // The two DO overlap where an inner row boundary crosses the left border —
  // every row's leading edge except the first is the previous row's trailing
  // edge, which is a legitimate resize — and there the gutter wins: it is
  // consulted first in the press path (`main.js`, before `tableChrome`) and its
  // strip element sits over the page and carries the `cell` cursor. That
  // precedence is not new; the boundary zone already reached 5px outside the
  // table and the gutter already went first. What must survive is that the
  // resize stays reachable, and it does: one twip past the zone the same
  // boundary arms.
  const crossing = table.y + 288;
  assert.equal(boundaryAt([table], table.x, crossing, tol, tol)?.kind, "row");
  assert.equal(gutterAt([table], table.x, crossing, OPTS)?.axis, "row");
  const zone = stripRect(table, "row", T, INSIDE_PX * 15);
  assert.equal(gutterAt([table], zone.x + zone.w, crossing, OPTS), null);
  assert.equal(boundaryAt([table], zone.x + zone.w, crossing, tol, tol)?.kind, "row");
  // The same on the other axis: a column boundary crossing the top border.
  const colCross = table.x + 1440;
  assert.equal(gutterAt([table], colCross, table.y, OPTS)?.axis, "column");
  const colZone = stripRect(table, "column", T, INSIDE_PX * 15);
  assert.equal(boundaryAt([table], colCross, colZone.y + colZone.h, tol, tol)?.kind, "column");

  // …and the TRAILING outer edges still belong to the resize, as they do in
  // ONLYOFFICE. Without this the guard above would pass on a payload that had
  // simply lost its boundaries.
  assert.equal(boundaryAt([table], table.x + 720, table.y + table.h, tol, tol)?.kind, "row");
  assert.equal(boundaryAt([table], table.x + table.w, table.y + 144, tol, tol)?.kind, "column");
});

test("the inside reach is clamped against the band it reaches INTO, not its own", () => {
  // The same rule, and the same number, as the boundary clamp next door: a PIXEL
  // distance measured into a band that is not measured in pixels. The band a
  // COLUMN zone eats into is the first ROW, so a short row is what has to clamp
  // it — clamping it against the first column's width would measure the wrong
  // dimension and let a 3px reach swallow a 10-twip row whole.
  const shortRows = chromeTable({ rows: 3, rowH: 10 });
  const colZone = stripRect(shortRows, "column", T, INSIDE_PX * 15);
  assert.ok(
    colZone.y + colZone.h <= shortRows.y + 10 * 0.2 + 0.001,
    "the column zone must not reach past a fifth of the first row",
  );

  // And the row zone's own clamp, against a narrow first column.
  const narrowCols = chromeTable({ cols: 2, colW: 100 });
  const rowZone = stripRect(narrowCols, "row", T, INSIDE_PX * 15);
  assert.ok(
    rowZone.x + rowZone.w <= narrowCols.x + 100 * 0.2 + 0.001,
    "the row zone must not reach past a fifth of the first column",
  );
  // …and the rest of that narrow column still places a caret.
  assert.equal(gutterAt([narrowCols], narrowCols.x + 50, narrowCols.y + 144, OPTS), null);
});

test("a table flush against the sheet's edge keeps a target, on its own border", () => {
  // A table indented to the sheet's edge has no free space beside it, so the
  // strip shrinks to the border and its inside reach: still a target, and still
  // nothing drawn off the sheet or over anything.
  const flush = chromeTable({ x: 0, y: 0 });
  for (const axis of ["row", "column"]) {
    const lanes = gutterLanes(flush, axis, T, INSIDE_PX * 15);
    assert.ok(lanes?.strip, `${axis} zone`);
    assert.equal(lanes.short, true, `${axis}: no room for a full target`);
    assert.equal(lanes.insert, null, `${axis}: and no discs`);
  }
  assert.equal(gutterAt([flush], 0, 144, OPTS)?.axis, "row");
  assert.equal(gutterAt([flush], 720, 0, OPTS)?.axis, "column");
  assert.equal(stripRect(flush, "row", T, INSIDE_PX * 15).x, 0);
});

test("the row strip resolves the band the pointer is beside", () => {
  const table = chromeTable({ rows: 4, rowH: 288 });
  for (let i = 0; i < 4; i++) {
    const middle = table.y + i * 288 + 144;
    const hit = gutterAt([table], table.x - 30, middle, OPTS);
    assert.equal(hit?.kind, "strip");
    assert.equal(hit.axis, "row");
    assert.equal(hit.index, i, `the strip beside row ${i} must select row ${i}`);
    assert.equal(hit.anchor, `r${i}`);
  }
});

test("the column strip resolves the band the pointer is above", () => {
  const table = chromeTable({ cols: 3, colW: 1440 });
  for (let i = 0; i < 3; i++) {
    const middle = table.x + i * 1440 + 720;
    const hit = gutterAt([table], middle, table.y - 30, OPTS);
    assert.equal(hit?.kind, "strip");
    assert.equal(hit.axis, "column");
    assert.equal(hit.index, i);
  }
});

test("the + disc has its own lane, so the strip selects every pixel of every band", () => {
  // An 18px row (270 twips). The disc used to share the strip and respond over
  // a fifth of the band, 7px, while painting 16 (HF-341). Now it lives in the
  // lane beyond the strip, a full target, and the strip keeps every band whole.
  const table = chromeTable({ rows: 3, rowH: 270 });
  const lanes = gutterLanes(table, "row", T, INSIDE_PX * 15);
  assert.ok(lanes.insert, "a 96px margin holds both lanes");
  const inStrip = lanes.strip.x + lanes.strip.w / 2;
  const inLane = lanes.insert.x + lanes.insert.w / 2;
  for (let y = table.y; y < table.y + table.h; y += 15) {
    const hit = gutterAt([table], inStrip, y, OPTS);
    assert.equal(hit?.kind, "strip", `the strip at y=${y} must select, not insert`);
    assert.equal(hit.index, Math.floor((y - table.y) / 270));
  }
  // In the lane, the NEAREST boundary within half a target, and its disc box.
  const boundary = table.y + 270;
  const onIt = gutterAt([table], inLane, boundary + 100, OPTS);
  assert.equal(onIt?.kind, "insert");
  assert.equal(onIt.index, 1, "boundary 1 inserts before row 1");
  assert.equal(onIt.after, true);
  assert.equal(onIt.anchor, "r0", "which is `insertRow(row 0, below)`");
  assert.deepEqual(onIt.disc, { x: lanes.insert.x, y: boundary - T / 2, w: T, h: T });
  const leading = gutterAt([table], inLane, table.y - 30, OPTS);
  assert.equal(leading?.kind, "insert");
  assert.equal(leading.after, false);
  assert.equal(leading.anchor, "r0");
});

test("the painted disc is its own hit box: every pixel it paints inserts at it", () => {
  // The defect: painted 16px, live over 7. The guarantee: while a disc is
  // shown, its whole box answers for it, even where a neighbouring boundary is
  // nearer (rows here are shorter than a target).
  const table = chromeTable({ rows: 4, rowH: 270 });
  const lane = gutterLanes(table, "row", T, INSIDE_PX * 15).insert;
  const shown = gutterAt([table], lane.x + 10, table.y + 2 * 270, OPTS);
  assert.equal(shown.index, 2);
  const sticky = { table: table.node, axis: "row", index: 2, disc: shown.disc };
  for (let y = shown.disc.y; y < shown.disc.y + shown.disc.h; y += 15) {
    for (let x = shown.disc.x; x < shown.disc.x + shown.disc.w; x += 30) {
      const hit = gutterAt([table], x, y, { ...OPTS, sticky });
      assert.equal(hit?.kind, "insert", `(${x}, ${y}) is painted disc`);
      assert.equal(hit.index, 2, `(${x}, ${y}) must insert at the disc it is on`);
    }
  }
  // Without the sticky disc the same corner resolves to the nearer boundary,
  // which is what made the painted box and the live box disagree.
  const corner = gutterAt([table], shown.disc.x + 10, shown.disc.y + 5, OPTS);
  assert.equal(corner.index, 1);
});

test("every lane and disc is a full target where the free space allows it, 44px under a finger", () => {
  const table = chromeTable({ rows: 3, rowH: 270, cols: 3, colW: 1440 });
  for (const axis of ["row", "column"]) {
    const { strip, insert } = gutterLanes(table, axis, T, INSIDE_PX * 15);
    assert.equal(axis === "row" ? strip.w : strip.h, T, `${axis} strip is one target thick`);
    assert.equal(axis === "row" ? insert.w : insert.h, T, `${axis} disc lane is one target thick`);
  }
  assert.equal(targetPxFor("mouse"), 24);
  assert.equal(targetPxFor("pen"), 24);
  assert.equal(targetPxFor("touch"), COARSE_TARGET_PX);
  assert.equal(COARSE_TARGET_PX, 44);
});

test("no lane reaches over a line of text: it fits the free space or shrinks", () => {
  // `clearAbove` is the gap layout measured up to the line above (HF-344). The
  // column strip used to reach 14px up whatever was there.
  for (const gapPx of [0, 4, 10, 21, 30, 60]) {
    const table = { ...chromeTable({ cols: 3 }), clearAbove: gapPx * 15, clearBefore: gapPx * 15 };
    for (const axis of ["row", "column"]) {
      const lanes = gutterLanes(table, axis, T, INSIDE_PX * 15);
      const free = gapPx * 15;
      for (const rect of [lanes?.strip, lanes?.insert].filter(Boolean)) {
        const start = axis === "row" ? rect.x : rect.y;
        const border = axis === "row" ? table.x : table.y;
        assert.ok(start >= border - free, `${axis} lane at a ${gapPx}px gap starts ${(border - start) / 15}px out`);
      }
    }
    // And the line above is still the line: nothing arms there.
    assert.equal(gutterAt([table], table.x + 720, table.y - gapPx * 15 - 15, OPTS), null);
  }
});

test("no pixel is both a gutter target and a boundary target", () => {
  // Two targets contesting one pixel is the overlap WCAG 2.5.8 counts against
  // both (HF-342). The boundary zones now start where the strip's inside reach
  // ends, and the corner where the leading borders meet belongs to one strip.
  const table = chromeTable({ rows: 3, rowH: 600, cols: 3, colW: 1440 });
  const tol = 12 * 15;
  const inset = INSIDE_PX * 15;
  let both = 0;
  for (let x = table.x - 50 * 15; x < table.x + table.w + 20 * 15; x += 15) {
    for (let y = table.y - 50 * 15; y < table.y + table.h + 20 * 15; y += 15) {
      if (gutterAt([table], x, y, OPTS) && boundaryAt([table], x, y, tol, tol, inset)) both++;
    }
  }
  assert.equal(both, 0, `${both} points armed a gutter target AND a resize`);
});

test("the review mode decides what ARMS, not what fails on release", () => {
  const table = chromeTable();
  const strip = [table.x - 30, table.y + 144];
  const boundary = [table.x - T - 60, table.y + 288]; // in the disc lane, beyond the strip

  // Editing: everything arms.
  assert.equal(armTableGutter([table], ...strip, OPTS, {}).target?.kind, "strip");
  assert.equal(armTableGutter([table], ...boundary, OPTS, {}).target?.kind, "insert");

  // Suggesting: selecting is not an edit, so the strip still arms; inserting has
  // no tracked-change representation, so the disc does not, and a press says why.
  const suggesting = { geometryBlocked: true };
  assert.equal(armTableGutter([table], ...strip, OPTS, suggesting).target?.kind, "strip");
  const refused = armTableGutter([table], ...boundary, OPTS, suggesting);
  assert.equal(refused.target, null);
  assert.equal(refused.refusal, "table.reason.notTracked");

  // Viewing: the disc is not armed and nothing is said — a click there is an
  // ordinary caret placement, not a refusal worth a sentence.
  const viewing = { editsBlocked: true, geometryBlocked: true };
  assert.equal(armTableGutter([table], ...boundary, OPTS, viewing).target, null);
  assert.equal(armTableGutter([table], ...boundary, OPTS, viewing).refusal, "");
});

test("a column strip on a merged table arms like any other", () => {
  // `tableSelectionAnchorNodes(node, "column")` resolves the column through the
  // engine's grid map, so a merged table selects, inserts and moves columns
  // (`docs/171`). This strip used to refuse with "table.reason.merged".
  const merged = chromeTable({ regular: false });
  const column = armTableGutter([merged], merged.x + 720, merged.y - 30, OPTS, {});
  assert.equal(column.target?.kind, "strip");
  assert.equal(column.target?.axis, "column");
  assert.equal(column.refusal, "");
  assert.equal(armTableGutter([merged], merged.x - 30, merged.y + 144, OPTS, {}).target?.kind, "strip");
});

test("a hover stays LINEAR in the size of the table", () => {
  // Guard the complexity, not the milliseconds (SKILL §8). A hover fires on every
  // pointer move, so an accidental per-cell query here is the quadratic the
  // chrome layer just removed from the other axis — and a timing threshold could
  // not tell a slow constant from it.
  const reads = (rows) => {
    let count = 0;
    const watch = (o) => new Proxy(o, { get: (t, k) => (count++, t[k]) });
    const table = chromeTable({ rows, cols: 8 });
    const watched = {
      ...table,
      rowEdges: table.rowEdges.map(watch),
      colEdges: table.colEdges.map(watch),
    };
    // ONE hover, at the LAST band, so the whole band list is traversed — the
    // worst case for a single pointer position, which is what a pointer move
    // actually costs.
    gutterAt([watched], table.x - 30, table.y + table.h - 144, OPTS);
    return count;
  };
  const small = reads(32);
  const ratio = reads(64) / small;
  assert.ok(
    ratio < 3,
    `doubling the rows multiplied one hover's property reads by ${ratio.toFixed(2)}. ` +
      "One hover must be O(bands): ~2 on doubling. Anything near 4 means the band " +
      "list is being rebuilt or re-scanned inside a loop over boundaries, which is " +
      "the per-cell quadratic `tableChromeOnPage` was introduced to remove",
  );
});

test("tableBands reads a band as [end - extent, end], leading edge first", () => {
  const table = chromeTable({ rows: 3, rowH: 300, y: 1000 });
  assert.deepEqual(
    tableBands(table, "row").map((b) => [b.i, b.start, b.end]),
    [
      [0, 1000, 1300],
      [1, 1300, 1600],
      [2, 1600, 1900],
    ],
  );
});

// ---------------------------------------------------------------------------
// The reorder drop (`docs/141` §4.2.3).

test("the drop boundary flips at the MIDDLE of a band, not at its edge", () => {
  const bands = tableBands(chromeTable({ rows: 3, rowH: 300, y: 1000 }), "row");
  // Above the table, and just inside the first band: drop before row 0.
  assert.equal(dropBoundaryAt(bands, 900), 0);
  assert.equal(dropBoundaryAt(bands, 1100), 0);
  // Past the middle of row 0: drop between rows 0 and 1. A drop that flipped
  // only at the band EDGE would still say 0 here, and the indicator would lag
  // the pointer by most of a row.
  assert.equal(dropBoundaryAt(bands, 1160), 1);
  assert.equal(dropBoundaryAt(bands, 1400), 1);
  assert.equal(dropBoundaryAt(bands, 1460), 2);
  // Past the last band, and well past it: clamped to the trailing boundary,
  // because a drag that runs off the table has not stopped being a drag.
  assert.equal(dropBoundaryAt(bands, 1860), 3);
  assert.equal(dropBoundaryAt(bands, 9000), 3);
  assert.equal(dropBoundaryAt([], 500), 0);
});

test("the post-move index loses one slot only when the band travels forwards", () => {
  // Backwards: nothing above it has been vacated, so boundary == to.
  assert.equal(moveTargetIndex(2, 0), 0);
  assert.equal(moveTargetIndex(2, 1), 1);
  // A drop on either of its own boundaries is the index it already has, which
  // is what makes a no-op drag refusable rather than a silent undo entry.
  assert.equal(moveTargetIndex(2, 2), 2);
  assert.equal(moveTargetIndex(2, 3), 2);
  // Forwards: it vacates its own slot on the way past, so to == boundary - 1.
  // Getting this wrong moves the right row to the wrong place AND COMMITS —
  // no refusal catches it, which is why it is asserted directly.
  assert.equal(moveTargetIndex(0, 3), 2);
  assert.equal(moveTargetIndex(0, 2), 1);
});
