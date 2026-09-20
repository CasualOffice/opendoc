// What the editor says about itself, checked without a browser.
//
// These decisions used to be inseparable from the DOM writes that acted on
// them inside `main.js` (`109` HF-085), so the only way to ask "does a refusal
// reach the assertive region?" was a Playwright run against a screen-reader
// surface. They are pure, and they are the policy ~117 `setStatus` call sites
// share.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DOCUMENT_STATE_BADGES,
  announcementRegion,
  documentStateBadge,
  documentTabTitle,
  isObjectSelectionStatus,
  statusClassName,
} from "../src/status_policy.mjs";

// A refusal the user cannot hear reads as the editor doing nothing at all —
// the worst failure shape available to an editor, and the reason errors are
// assertive while everything else waits its turn.
test("a refusal interrupts; nothing else does", () => {
  assert.equal(announcementRegion("error"), "assertive");
  for (const kind of ["", "ok", "warn", undefined]) {
    assert.equal(announcementRegion(kind), "polite", `"${kind}" must not interrupt`);
  }
});

test("the status class carries the kind", () => {
  assert.equal(statusClassName("error"), "status error");
  assert.match(statusClassName(""), /^status\s*$/);
});

test("an unknown document state falls back to Opened in BOTH the attribute and the text", () => {
  const badge = documentStateBadge("nonsense");
  assert.equal(badge.state, "opened", "the data attribute styles the pill; it must not name a state the text denies");
  assert.equal(badge.text, DOCUMENT_STATE_BADGES.opened.text);
  assert.equal(badge.icon, DOCUMENT_STATE_BADGES.opened.icon);
});

test("each real document state keeps its own icon and wording", () => {
  for (const [state, expected] of Object.entries(DOCUMENT_STATE_BADGES)) {
    const badge = documentStateBadge(state);
    assert.equal(badge.state, state);
    assert.equal(badge.icon, expected.icon);
    assert.equal(badge.text, expected.text);
  }
  assert.equal(new Set(Object.values(DOCUMENT_STATE_BADGES).map((b) => b.icon)).size, 3);
});

// A tab strip truncates from the RIGHT, so whatever is printed before the
// document name is what survives — and the name is what the user is scanning
// for. This is the assertion that stops a future "OpenDoc — Report.docx".
test("the tab names the document first, then the app", () => {
  const title = documentTabTitle({ name: "Q3 Report.docx" });
  assert.ok(title.startsWith("Q3 Report.docx"), title);
  assert.match(title, /OpenDoc/);
});

test("unsaved work shows the leading dot, saved work does not", () => {
  assert.equal(documentTabTitle({ name: "a.docx", dirty: true }), "• a.docx — OpenDoc");
  assert.equal(documentTabTitle({ name: "a.docx", dirty: false }), "a.docx — OpenDoc");
});

test("with no document the page's own title stands", () => {
  for (const name of ["", null, undefined]) {
    assert.equal(
      documentTabTitle({ name, dirty: true, fallback: "OpenDoc — editor" }),
      "OpenDoc — editor",
      "a dirty flag with no document must not invent a title",
    );
  }
});

// Clearing the status line when an object is deselected must not wipe someone
// else's message — a save confirmation, or the reason an edit was refused.
test("only the editor's own object-selection lines are treated as clearable", () => {
  for (const text of [
    "Image selected",
    "Shape selected — press Escape to leave",
    "Text box selected",
    "Object selected",
  ]) {
    assert.equal(isObjectSelectionStatus(text), true, text);
  }
  for (const text of [
    "Saved Q3 Report.docx",
    "That edit isn't supported for this selection yet",
    "Selected image",
    "",
    null,
    undefined,
  ]) {
    assert.equal(isObjectSelectionStatus(text), false, String(text));
  }
});
