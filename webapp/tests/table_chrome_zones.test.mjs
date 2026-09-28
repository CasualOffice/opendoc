import assert from "node:assert/strict";
import test from "node:test";

import { boundaryAt } from "../src/table_chrome_zones.mjs";

// 96 CSS px to the inch, 1440 twips to the inch.
const PX = 1440 / 96;

/** A one-column table whose rows are `heights` px tall, starting at y = 0. */
function tableWithRows(heights) {
  let y = 0;
  const rowEdges = [];
  for (const [i, h] of heights.entries()) {
    y += h * PX;
    rowEdges.push({ i, x: 0, y, w: 400 * PX, height: h * PX, outer: i === heights.length - 1 });
  }
  return {
    tables: [
      {
        node: "t1",
        regular: true,
        x: 0,
        y: 0,
        w: 400 * PX,
        h: y,
        rowEdges,
        colEdges: [],
      },
    ],
    rowEdges,
  };
}

test("a click inside an ordinary row is a click in the cell, not a grab at its border", () => {
  // THE REGRESSION. A table inserted at default height paints rows about 18px
  // tall, and the zone was a flat +/-5px about every boundary. Ten of those
  // eighteen pixels therefore belonged to a boundary, so a click anywhere in the
  // bottom quarter of a normal row started a row resize instead of putting the
  // caret where the person clicked. `table-cell-context.spec.mjs` clicks at 75%
  // of the cell height — 4.6px from the edge of an 18.4px row — and that is what
  // reddened main.
  const { tables } = tableWithRows([18.4, 18.4]);
  const firstEdgeY = 18.4 * PX;

  const inTheCell = boundaryAt(tables, 200 * PX, firstEdgeY - 4.6 * PX, 5 * PX, 5 * PX);
  assert.equal(
    inTheCell,
    null,
    "a point 4.6px above an 18.4px row's lower boundary is inside the cell; the caret belongs there",
  );

  // And the boundary is still grabbable from where a person actually aims at it.
  const onTheEdge = boundaryAt(tables, 200 * PX, firstEdgeY - 1.5 * PX, 5 * PX, 5 * PX);
  assert.equal(onTheEdge?.kind, "row", "a point 1.5px off the boundary must still arm it");
});

test("a tall row keeps the full forgiving zone", () => {
  // The clamp is a proportion, not a smaller constant: on a row with room to
  // spare the zone stays at the +/-5px this repo deliberately ships (ONLYOFFICE
  // uses +/-3px, and being more forgiving is one of the few places we lead it).
  const { tables } = tableWithRows([100, 100]);
  const edgeY = 100 * PX;

  for (const offset of [4.9, 2, 0.1]) {
    assert.equal(
      boundaryAt(tables, 200 * PX, edgeY - offset * PX, 5 * PX, 5 * PX)?.kind,
      "row",
      `a point ${offset}px from a 100px row's boundary must arm it`,
    );
  }
  assert.equal(
    boundaryAt(tables, 200 * PX, edgeY - 5.6 * PX, 5 * PX, 5 * PX),
    null,
    "and 5.6px away is still outside the zone",
  );
});

test("the zone is clamped by the SHORTER of the two bands a boundary separates", () => {
  // A tall row above a short one: the zone must not eat the short row just
  // because the row it closes is generous.
  const { tables } = tableWithRows([100, 18.4]);
  const edgeY = 100 * PX;

  assert.equal(
    boundaryAt(tables, 200 * PX, edgeY + 4.6 * PX, 5 * PX, 5 * PX),
    null,
    "4.6px BELOW the boundary is inside the 18.4px row underneath it",
  );
});
