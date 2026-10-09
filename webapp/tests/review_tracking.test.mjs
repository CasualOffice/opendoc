// The document's Track Changes setting and the review mode (`review_tracking.mjs`,
// `docs/109` HF-283): open honours it, the reader's own switch writes it, and an
// edit that changes it moves the mode — only on a change.
import assert from "node:assert/strict";
import test from "node:test";

import { createReviewTracking } from "../src/review_tracking.mjs";

/** A stand-in engine: the setting, and a record of what the host asked of it. */
function fakeDoc(tracks) {
  const calls = [];
  return {
    calls,
    trackRevisions: tracks,
    setParagraphTracking(on) {
      calls.push(["paragraphTracking", on]);
    },
    setTrackRevisions(on, node, offset) {
      calls.push(["setTrackRevisions", on, node, offset]);
      this.trackRevisions = on;
      return { applied: on };
    },
  };
}
const runEditInto = (ran) => (thunk, options) => ran.push({ result: thunk(), options });

test("a document whose file tracks changes opens in Suggesting, never stronger than the session allows", () => {
  assert.equal(createReviewTracking().openMode("editing", true), "suggesting");
  assert.equal(createReviewTracking().openMode("editing", false), "editing");
  assert.equal(createReviewTracking().openMode("viewing", true), "viewing", "a reader stays a reader");
  assert.equal(createReviewTracking().openMode("suggesting", false), "suggesting");
});

test("the reader's own switch writes the setting, once, at the caret", () => {
  const tracking = createReviewTracking();
  const doc = fakeDoc(false);
  tracking.openMode("editing", false);
  const ran = [];
  tracking.apply({ doc, mode: "suggesting", byUser: true, caret: { node: "p1", offset: 3 }, runEdit: runEditInto(ran) });
  assert.deepEqual(doc.calls, [
    ["paragraphTracking", true],
    ["setTrackRevisions", true, "p1", 3],
  ]);
  assert.equal(ran.length, 1, "through the edit runner, so it is one undoable edit");
  assert.deepEqual(ran[0].options, { keepSelection: true }, "and the reader's selection survives it");

  // Asking again for what the document already says writes nothing.
  tracking.apply({ doc, mode: "suggesting", byUser: true, caret: null, runEdit: runEditInto(ran) });
  assert.equal(ran.length, 1);
  // And back to Editing writes `false`.
  tracking.apply({ doc, mode: "editing", byUser: true, caret: null, runEdit: runEditInto(ran) });
  assert.deepEqual(doc.calls.at(-1), ["setTrackRevisions", false, "", 0]);
});

test("a mode change that is not the reader's gesture, or into Viewing, writes nothing", () => {
  const tracking = createReviewTracking();
  const doc = fakeDoc(false);
  const ran = [];
  tracking.apply({ doc, mode: "suggesting", byUser: false, caret: null, runEdit: runEditInto(ran) });
  tracking.apply({ doc, mode: "viewing", byUser: true, caret: null, runEdit: runEditInto(ran) });
  assert.equal(ran.length, 0);
  assert.deepEqual(
    doc.calls.map(([name]) => name),
    ["paragraphTracking", "paragraphTracking"],
    "paragraph tracking still follows every mode change (HF-131)",
  );
});

test("an edit that CHANGES the setting moves the mode; a standing difference does not", () => {
  const tracking = createReviewTracking();
  tracking.openMode("editing", false);
  const switched = [];
  // A demo or a reviewer's grant: Suggesting over a document that does not track.
  // Every keystroke lands an edit; none of them may pull the reader out.
  assert.equal(tracking.follow(false, "suggesting", (m) => switched.push(m)), null);
  assert.equal(tracking.follow(false, "suggesting", (m) => switched.push(m)), null);
  // A co-author (or a Redo) turns tracking on: Editing follows into Suggesting.
  assert.equal(tracking.follow(true, "editing", (m) => switched.push(m)), "suggesting");
  // An Undo of it: back to Editing.
  assert.equal(tracking.follow(false, "suggesting", (m) => switched.push(m)), "editing");
  assert.deepEqual(switched, ["suggesting", "editing"]);
  // A reader in Viewing is never pulled out.
  assert.equal(tracking.follow(true, "viewing", (m) => switched.push(m)), null);
  assert.equal(switched.length, 2);
});
