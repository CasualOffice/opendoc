// The table gutter's zone geometry (`docs/141` D-2).
//
// Every assertion here is on a GUARANTEE the design makes, not on a shape it
// happens to have:
//
//   * a strip never takes any part of a cell, so clicking in a cell still places
//     the caret at every strip width (the defect the boundary zones had to be
//     clamped for);
//   * the strip resolves the band a user is pointing BESIDE;
//   * the `+` insert zone cannot eat the strip segment that selects the band,
//     however short the band is;
//   * the review mode decides what ARMS rather than what fails on release;
//   * a hover stays linear in the size of the table.
import test from "node:test";
import assert from "node:assert/strict";

import {
  STRIP_PX,
  armTableGutter,
  gutterAt,
  stripRect,
  tableBands,
} from "../src/table_gutter_zones.mjs";

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
const OPTS = { stripX: STRIP_PX * 15, stripY: STRIP_PX * 15, insertX: 8 * 15, insertY: 8 * 15 };

test("a strip lies entirely outside the table, so no cell loses a pixel to it", () => {
  const table = chromeTable();
  for (const axis of ["row", "column"]) {
    const strip = stripRect(table, axis, STRIP_PX * 15);
    assert.ok(strip, `${axis} strip should exist with room beside the table`);
    if (axis === "row") assert.equal(strip.x + strip.w, table.x);
    else assert.equal(strip.y + strip.h, table.y);
  }
  // The load-bearing half: every point INSIDE the table's own box resolves to no
  // gutter target at all. This is the property that makes the strip free where a
  // boundary zone had to be clamped to a fifth of its band.
  for (let x = table.x; x < table.x + table.w; x += 120) {
    for (let y = table.y; y < table.y + table.h; y += 60) {
      assert.equal(gutterAt([table], x, y, OPTS), null, `(${x}, ${y}) is inside a cell`);
    }
  }
});

test("a table with no room beside it reports no strip rather than one off the page", () => {
  // A table indented to the sheet's own edge. Drawing at a negative coordinate
  // would paint a strip nothing could ever hit; the row stays reachable from the
  // menu, the context menu and the palette.
  const flush = chromeTable({ x: 0, y: 0 });
  assert.equal(stripRect(flush, "row", STRIP_PX * 15, 60), null);
  assert.equal(stripRect(flush, "column", STRIP_PX * 15, 60), null);
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
