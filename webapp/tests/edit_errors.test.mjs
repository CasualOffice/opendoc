import assert from "node:assert/strict";
import test from "node:test";

import { editRefusalMessage, mutationBlockedMessage } from "../src/edit_errors.mjs";

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

// docs/113 §8.3/§8.5 — a document too large to lay out whole is opened one
// page-window at a time and is READ-ONLY, because re-paginating after a
// keystroke is the peak that path exists to avoid. Every edit is refused at the
// engine's atomic choke point, and the host was showing that refusal as "not
// supported for this selection", which is wrong in both halves: the selection
// is fine, and the reader is not told the thing that decides what to do next —
// that this document cannot be edited at any selection, and why.
const WINDOWED = [
  "This document is open one page-window at a time because it has more than 262,144",
  "paragraphs, which is the most the browser editor can lay out whole. Editing needs the",
  "whole document laid out at once, so it is not available here. Export the document, or",
  "split it into smaller ones.",
].join(" ");

test("a document that cannot be edited at all says so, instead of blaming the selection", () => {
  const shown = editRefusalMessage(new Error("Unsupported"), {
    editingUnavailableReason: WINDOWED,
  });
  assert.equal(shown, WINDOWED);
  assert.doesNotMatch(shown, /selection/);
});

// The document-level reason outranks every per-edit one: on a read-only
// document a refused undo is refused for the same single reason as everything
// else, and "that history step can no longer be applied" would imply the
// others could.
test("the document-level reason outranks the history-step message", () => {
  const shown = editRefusalMessage(new Error("undo failed: Unsupported"), {
    editingUnavailableReason: WINDOWED,
  });
  assert.equal(shown, WINDOWED);
  assert.doesNotMatch(shown, /history step/);
});

// An editable document must never be told it is read-only. The empty string is
// what the engine returns for one, and it must behave exactly as no context at
// all — this is the assertion that stops the new branch from swallowing the
// ordinary path.
test("an empty reason changes nothing", () => {
  for (const context of [{}, { editingUnavailableReason: "" }, { editingUnavailableReason: null }]) {
    assert.match(editRefusalMessage(new Error("Unsupported"), context), /selection/);
    assert.match(editRefusalMessage(new Error("undo failed: X"), context), /history step/);
  }
});

// The host blocks every mutation in Viewing mode BEFORE it reaches the engine,
// and that block is where a read-only document is actually met in the product;
// the engine refusal above is the second line of defence, not the first. Both
// go through this module so the two can never say different things about the
// same document.
test("a blocked mutation says how to unblock it, unless it cannot be unblocked", () => {
  assert.match(mutationBlockedMessage(), /switch to Editing/);
  assert.match(mutationBlockedMessage({ editingUnavailableReason: "" }), /switch to Editing/);
  const shown = mutationBlockedMessage({ editingUnavailableReason: WINDOWED });
  assert.equal(shown, WINDOWED);
  // The instruction a reader cannot follow: on a document the engine will not
  // let anyone edit, the mode buttons are disabled and there is nothing to
  // switch to.
  assert.doesNotMatch(shown, /switch to Editing/i);
});
