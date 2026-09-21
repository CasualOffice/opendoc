// The set Tab walks, asked directly.
//
// The defect was not in the wrap-around arithmetic, which was right; it was in
// WHICH LIST the arithmetic ran over. So the choice of set is what these test.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  OBJECT_LABELS,
  nextObjectIndex,
  traversalAnnouncement,
  traversalRoot,
} from "../src/object_traversal.mjs";

test("a selection inside a group traverses that group", () => {
  // The Medical-form case: the anchored group is the root, the shape within it
  // is the subject, and Tab must stay inside.
  assert.equal(traversalRoot({ root: "g1", subject: "s2", path: "1" }), "g1");
});

test("a top-level selection traverses the document", () => {
  // A selected group is its own root and subject; Tab must move to the NEXT
  // top-level object, not descend. Descending is Enter.
  assert.equal(traversalRoot({ root: "g1", subject: "g1", path: "" }), null);
  assert.equal(traversalRoot({ root: "img", subject: "img", path: "" }), null);
});

test("no selection traverses the document", () => {
  assert.equal(traversalRoot(null), null);
  assert.equal(traversalRoot(undefined), null);
  assert.equal(traversalRoot({}), null);
});

test("stepping wraps in both directions", () => {
  assert.equal(nextObjectIndex(3, 0, 1), 1);
  assert.equal(nextObjectIndex(3, 2, 1), 0);
  assert.equal(nextObjectIndex(3, 0, -1), 2);
  assert.equal(nextObjectIndex(3, 1, -1), 0);
});

test("an object that is not in the list starts from the end being moved toward", () => {
  // Nothing selected, or the selection was deleted out from under the
  // traversal. Refusing to move would strand a keyboard user with no object.
  assert.equal(nextObjectIndex(4, -1, 1), 0);
  assert.equal(nextObjectIndex(4, -1, -1), 3);
});

test("an empty list has nowhere to land", () => {
  assert.equal(nextObjectIndex(0, -1, 1), -1);
  assert.equal(nextObjectIndex(0, 2, -1), -1);
});

test("the announcement says which set is being walked", () => {
  // "3 of 9" means a different thing inside a group, and nothing else tells a
  // screen-reader user that Escape is what leaves it.
  assert.equal(
    traversalAnnouncement("shape", 2, 9, true),
    "Shape 3 of 9 in group",
  );
  assert.equal(traversalAnnouncement("shape", 2, 9, false), "Shape 3 of 9");
  assert.equal(
    traversalAnnouncement("textbox", 0, 1, false),
    "Text box 1 of 1",
  );
  // An unknown kind is still announced, not skipped.
  assert.equal(traversalAnnouncement("chart", 0, 2, false), "Object 1 of 2");
});

test("every kind the engine reports has a name", () => {
  for (const kind of ["image", "textbox", "shape", "group"]) {
    assert.ok(OBJECT_LABELS[kind], `${kind} has no label`);
  }
});
