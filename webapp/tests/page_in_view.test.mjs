// "Page X of Y" while scrolling names the page most in view (audit CANVAS-09).
import assert from "node:assert/strict";
import test from "node:test";

import { dominantSheet } from "../src/page_in_view.mjs";

test("the sheet with the largest visible area is the page in view", () => {
  const view = { left: 0, top: 100, right: 1000, bottom: 700 };
  const sheets = [
    { left: 100, right: 900, top: -900, bottom: 150 }, // 50px showing
    { left: 100, right: 900, top: 170, bottom: 1220 }, // 530px showing
    { left: 100, right: 900, top: 1240, bottom: 2290 }, // below
  ];
  assert.equal(dominantSheet(sheets, view), 1);
  assert.equal(dominantSheet([], view), -1);
  assert.equal(dominantSheet([{ left: 0, right: 10, top: 900, bottom: 950 }], view), -1);
});
