import assert from "node:assert/strict";
import test from "node:test";

import { editRefusalMessage } from "../src/edit_errors.mjs";

// HF-045 made a refused undo restore the document and push the history entry
// back, so the edit genuinely leaves nothing half-applied. The status bar was
// still calling that "not supported for this selection", which is wrong twice:
// there is nothing wrong with the selection, and the user is not told the
// thing that matters — that their document is intact.
test("a refused history step says the document is unchanged, not that the selection is wrong", () => {
  for (const message of ["undo failed: Unsupported", "redo failed: CrossParagraph"]) {
    const shown = editRefusalMessage(new Error(message));
    assert.match(shown, /history step/);
    assert.match(shown, /unchanged/);
    assert.doesNotMatch(shown, /selection/);
  }
});

test("any other refused edit keeps the generic selection message", () => {
  for (const error of [new Error("Unsupported"), new Error("CrossParagraph"), "plain string"]) {
    assert.match(editRefusalMessage(error), /selection/);
  }
});

// The engine's own error names are internal vocabulary and must not reach the
// status bar (docs/67). This is the assertion that stops a future "helpful"
// change from interpolating the cause into the sentence.
test("no engine error name ever reaches the user", () => {
  const leaky = new Error("undo failed: Unsupported(DeleteText, AtomicLeaf)");
  const shown = editRefusalMessage(leaky);
  for (const internal of ["Unsupported", "DeleteText", "AtomicLeaf", "failed:"]) {
    assert.ok(!shown.includes(internal), `"${internal}" leaked into: ${shown}`);
  }
});

// `catch` receives whatever was thrown — including nothing at all.
test("a thrown null or undefined still produces a sentence", () => {
  for (const error of [null, undefined, {}]) {
    assert.match(editRefusalMessage(error), /^That edit/);
  }
});
