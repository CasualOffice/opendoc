// The two decisions the wide-table scroller makes that can be answered in node
// (`docs/151` §6.3d): where the scroller goes when the caret moves (A-1), and
// what the always-visible bar looks like (A-3). The browser half — a real
// table, scrolled with the wheel, clicked and typed into — is
// `e2e/reflow-pageless.spec.mjs`.
import assert from "node:assert/strict";
import test from "node:test";

import {
  CARET_FOLLOW_EDGE_PX,
  followScrollLeft,
  thumbGeometry,
} from "../src/reflow_table_scroll.mjs";

const WIDTH = 848; // the Wide column's tile at 100%
const EDGE = CARET_FOLLOW_EDGE_PX;

test("a caret already in view does not move the table (A-1)", () => {
  // The rule that makes the table scrollable AT ALL with the caret in it: a
  // scroller that re-centred on every redraw could never be moved away.
  for (const x of [EDGE, WIDTH / 2, WIDTH - EDGE]) {
    assert.equal(followScrollLeft(x, 120, WIDTH), null, `a caret at ${x}px moved the table`);
  }
});

test("a caret past either edge brings the table just far enough to show it (A-1, A-4)", () => {
  // Right: the caret at 900px of an 848px strip needs 900 - (848 - 24) = 76 more.
  assert.equal(followScrollLeft(900, 100, WIDTH), 100 + 900 - (WIDTH - EDGE));
  // Left: `Home` puts the caret at the cell's start, off the left edge — the
  // table comes back to it (A-4), and never past zero.
  assert.equal(followScrollLeft(-200, 336, WIDTH), 336 - (EDGE + 200));
  assert.equal(followScrollLeft(-500, 100, WIDTH), 0, "never scrolled past the table's start");
  // And it lands exactly at the edge, so a second follow is a no-op.
  const next = followScrollLeft(900, 100, WIDTH);
  assert.equal(followScrollLeft(900 - (next - 100), next, WIDTH), null);
});

test("a strip too narrow to have edges never fights the reader", () => {
  assert.equal(followScrollLeft(5, 0, 2 * EDGE), null);
  assert.equal(followScrollLeft(5, 0, 0), null);
});

test("the bar is always there when the table overflows, never thinner than a finger (A-3)", () => {
  const start = thumbGeometry(800, 848, 1184, 0);
  assert.ok(start.width > 24 && start.width < 800, `thumb ${start.width}px`);
  assert.equal(start.left, 0);
  const end = thumbGeometry(800, 848, 1184, 336);
  assert.equal(Math.round(end.left + end.width), 800, "at the end the thumb meets the track's end");
  // A very wide table still leaves a thumb a finger can find.
  assert.equal(thumbGeometry(800, 390, 1_000_000, 0).width, 24);
  // Clamped: an overscrolled position does not push the thumb off the track.
  const over = thumbGeometry(800, 848, 1184, 9_999);
  assert.ok(over.left + over.width <= 800 + 1e-9);
  // Nothing to scroll: the thumb is the whole track.
  assert.deepEqual(thumbGeometry(800, 848, 848, 0), { width: 800, left: 0 });
});
