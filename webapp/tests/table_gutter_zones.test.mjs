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
  INSIDE_PX,
  STRIP_PX,
  armTableGutter,
  dropBoundaryAt,
  gutterAt,
  moveTargetIndex,
  stripRect,
  tableBands,
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
  stripX: STRIP_PX * 15,
  stripY: STRIP_PX * 15,
  insideX: INSIDE_PX * 15,
  insideY: INSIDE_PX * 15,
  insertX: 8 * 15,
  insertY: 8 * 15,
};

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
  const zone = stripRect(table, "row", STRIP_PX * 15, INSIDE_PX * 15);
  assert.equal(gutterAt([table], zone.x + zone.w, crossing, OPTS), null);
  assert.equal(boundaryAt([table], zone.x + zone.w, crossing, tol, tol)?.kind, "row");
  // The same on the other axis: a column boundary crossing the top border.
  const colCross = table.x + 1440;
  assert.equal(gutterAt([table], colCross, table.y, OPTS)?.axis, "column");
  const colZone = stripRect(table, "column", STRIP_PX * 15, INSIDE_PX * 15);
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
  const colZone = stripRect(shortRows, "column", STRIP_PX * 15, INSIDE_PX * 15);
  assert.ok(
    colZone.y + colZone.h <= shortRows.y + 10 * 0.2 + 0.001,
    "the column zone must not reach past a fifth of the first row",
  );

  // And the row zone's own clamp, against a narrow first column.
  const narrowCols = chromeTable({ cols: 2, colW: 100 });
  const rowZone = stripRect(narrowCols, "row", STRIP_PX * 15, INSIDE_PX * 15);
  assert.ok(
    rowZone.x + rowZone.w <= narrowCols.x + 100 * 0.2 + 0.001,
    "the row zone must not reach past a fifth of the first column",
  );
  // …and the rest of that narrow column still places a caret.
  assert.equal(gutterAt([narrowCols], narrowCols.x + 50, narrowCols.y + 144, OPTS), null);
});

test("a table flush against the sheet's edge keeps a target, on its own border", () => {
  // The case the outside-only strip could not serve: a table indented to the
  // margin had no room beside it, so `stripRect` returned null and the only way
  // to select a row was the menu. The border is still there, so the zone is too.
  const flush = chromeTable({ x: 0, y: 0 });
  for (const axis of ["row", "column"]) {
    assert.ok(stripRect(flush, axis, STRIP_PX * 15, INSIDE_PX * 15, 0), `${axis} zone`);
  }
  assert.equal(gutterAt([flush], 0, 144, OPTS)?.axis, "row");
  assert.equal(gutterAt([flush], 720, 0, OPTS)?.axis, "column");
  // Nothing is drawn off the sheet: the zone starts at the page's own edge.
  assert.equal(stripRect(flush, "row", STRIP_PX * 15, INSIDE_PX * 15).x, 0);
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

test("the + zone sits ON a boundary and never swallows the band it borders", () => {
  // An 18px row — the height a default table paints at — is 270 twips, so a flat
  // ±120-twip insert zone would claim the whole strip segment and the row could
  // never be SELECTED from the gutter at all. That is the same failure a flat
  // ±5px boundary zone already caused on the other axis.
  const table = chromeTable({ rows: 3, rowH: 270 });
  const boundary = table.y + 270; // between rows 0 and 1
  const onIt = gutterAt([table], table.x - 30, boundary, OPTS);
  assert.equal(onIt?.kind, "insert");
  assert.equal(onIt.index, 1, "boundary 1 inserts before row 1");
  assert.equal(onIt.after, true);
  assert.equal(onIt.anchor, "r0", "…which is `insertRow(row 0, below)`");

  // …and the MIDDLE HALF of every band's strip segment still selects that band.
  // This is the guarantee, stated as a proportion rather than as a distance,
  // because the zone is a pixel measure and a row is not: at 8px flat the middle
  // of an 18px row is 1px from being claimed by the disc.
  for (let i = 0; i < 3; i++) {
    for (const share of [0.25, 0.35, 0.5, 0.65, 0.75]) {
      const at = table.y + i * 270 + 270 * share;
      const hit = gutterAt([table], table.x - 30, at, OPTS);
      assert.equal(
        hit?.kind,
        "strip",
        `${(share * 100).toFixed(0)}% down row ${i}'s strip must still SELECT row ${i}`,
      );
      assert.equal(hit.index, i);
    }
  }
  // And the leading boundary inserts BEFORE the first row, which is the one
  // position `insertRow(anchor, after)` cannot express with `after`.
  const leading = gutterAt([table], table.x - 30, table.y, OPTS);
  assert.equal(leading?.kind, "insert");
  assert.equal(leading.after, false);
  assert.equal(leading.anchor, "r0");
});

test("the review mode decides what ARMS, not what fails on release", () => {
  const table = chromeTable();
  const strip = [table.x - 30, table.y + 144];
  const boundary = [table.x - 30, table.y + 288];

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

test("a column strip on a merged table refuses with the sentence the product has", () => {
  // `tableSelectionAnchorNodes(node, "column")` returns nothing for a merged
  // table, so arming the strip would paint an empty selection. The ROW strip is
  // unaffected: a row of a merged table is still a row.
  const merged = chromeTable({ regular: false });
  const column = armTableGutter([merged], merged.x + 720, merged.y - 30, OPTS, {});
  assert.equal(column.target, null);
  assert.equal(column.refusal, "table.reason.merged");
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
