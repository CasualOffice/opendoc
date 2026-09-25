import assert from "node:assert/strict";
import test from "node:test";

import { normalizeDropCap } from "../src/drop_cap.mjs";

test("drop-cap state accepts either host JSON or an object", () => {
  assert.deepEqual(normalizeDropCap('{"mode":"margin","lines":4}'), {
    mode: "margin",
    lines: 4,
  });
  assert.deepEqual(normalizeDropCap({ mode: "drop", lines: 2 }), {
    mode: "drop",
    lines: 2,
  });
});

test("drop-cap state fails closed and keeps line counts within Word's range", () => {
  assert.deepEqual(normalizeDropCap("not json"), { mode: "none", lines: 3 });
  assert.deepEqual(normalizeDropCap({ mode: "future", lines: 99 }), {
    mode: "none",
    lines: 10,
  });
  assert.deepEqual(normalizeDropCap({ mode: "drop", lines: 0 }), {
    mode: "drop",
    lines: 1,
  });
});
