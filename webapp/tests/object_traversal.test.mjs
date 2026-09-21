// The set Tab walks, asked directly.
//
// The defect was not in the wrap-around arithmetic, which was right; it was in
// WHICH LIST the arithmetic ran over. So the choice of set is what these test.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  OBJECT_LABELS,
  clickDescendsIntoGroup,
  escapeClimbsToGroup,
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

// --- clicking into a group (docs/117) ----------------------------------------

const groupHit = { kind: "group", root: "g1" };

test("the first click on a group does NOT descend", () => {
  // docs/117 §5 rule 1, and the half a naive fix breaks: a group has to stay
  // pickable, movable and deletable as one thing. Both Word and ONLYOFFICE
  // select the group on a first click.
  assert.equal(clickDescendsIntoGroup(null, groupHit, false), false);
  assert.equal(clickDescendsIntoGroup(undefined, groupHit, false), false);
  // A different object was held: this click is a first click on THIS group.
  assert.equal(
    clickDescendsIntoGroup(
      { root: "other", subject: "other" },
      groupHit,
      false,
    ),
    false,
  );
});

test("a click on a group already in hand descends", () => {
  // Word's second click — the gesture the owner was performing.
  assert.equal(
    clickDescendsIntoGroup({ root: "g1", subject: "g1" }, groupHit, false),
    true,
  );
});

test("a click elsewhere in a group whose child is held descends again", () => {
  // The third click, onto a different shape. Holding a child must not make the
  // group opaque again.
  assert.equal(
    clickDescendsIntoGroup({ root: "g1", subject: "s2" }, groupHit, false),
    true,
  );
});

test("a click on the child already held does not re-resolve", () => {
  // That click is the start of a drag. Re-resolving there fights the gesture.
  assert.equal(
    clickDescendsIntoGroup({ root: "g1", subject: "s2" }, groupHit, true),
    false,
  );
});

test("only a group is descended into", () => {
  for (const kind of ["image", "textbox", "shape", undefined]) {
    assert.equal(
      clickDescendsIntoGroup(
        { root: "g1", subject: "g1" },
        { kind, root: "g1" },
        false,
      ),
      false,
      `${kind} is not a group`,
    );
  }
});

test("Escape climbs out of a group, and only out of a group", () => {
  assert.equal(escapeClimbsToGroup({ root: "g1", subject: "s2" }), true);
  // A selected GROUP is its own root: Escape leaves, it does not climb to
  // itself and trap the user.
  assert.equal(escapeClimbsToGroup({ root: "g1", subject: "g1" }), false);
  assert.equal(escapeClimbsToGroup(null), false);
});
