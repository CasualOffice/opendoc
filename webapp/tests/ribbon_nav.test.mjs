import assert from "node:assert/strict";
import test from "node:test";

import { rovingIndex, tabStopIndex } from "../src/ribbon_nav.mjs";

test("arrow keys move one control at a time and wrap at both ends", () => {
  assert.equal(rovingIndex("ArrowRight", 0, 4), 1);
  assert.equal(rovingIndex("ArrowRight", 3, 4), 0);
  assert.equal(rovingIndex("ArrowLeft", 3, 4), 2);
  assert.equal(rovingIndex("ArrowLeft", 0, 4), 3);
});

test("Home and End jump to the ends of the band", () => {
  assert.equal(rovingIndex("Home", 2, 4), 0);
  assert.equal(rovingIndex("End", 2, 4), 3);
});

test("entering the band with no current item lands on the matching edge", () => {
  assert.equal(rovingIndex("ArrowRight", -1, 4), 0);
  assert.equal(rovingIndex("ArrowLeft", -1, 4), 3);
});

test("keys the toolbar does not own are left to the caller", () => {
  for (const key of ["ArrowUp", "ArrowDown", "Tab", "Enter", " ", "Escape", "a"]) {
    assert.equal(rovingIndex(key, 1, 4), null, key);
  }
});

test("an empty band claims nothing, so a collapsed ribbon cannot trap the arrows", () => {
  for (const key of ["ArrowRight", "ArrowLeft", "Home", "End"]) {
    assert.equal(rovingIndex(key, -1, 0), null, key);
  }
});

test("the band keeps its Tab stop where focus last sat", () => {
  const items = ["a", "b", "c"];
  assert.equal(tabStopIndex(items, "b"), 1);
});

test("a remembered control that overflow removed falls back to the first, never to no stop", () => {
  const items = ["a", "b", "c"];
  assert.equal(tabStopIndex(items, "gone"), 0);
  assert.equal(tabStopIndex(items, undefined), 0);
});

test("an empty band reports no Tab stop rather than index 0", () => {
  assert.equal(tabStopIndex([], "a"), -1);
});
