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
  backgroundProgressMayPaint,
  documentStateBadge,
  documentTabTitle,
  isObjectSelectionStatus,
  needsToast,
  statusClassName,
  toastDuration,
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

// `109` UX-017. The whole defect was a channel that disappeared: below 620px the
// status line and both live regions were inside a `display: none` container, so
// the editor refused and said nothing anywhere. These two rules are what decides
// that a message reaches a second surface, and they are the half that can be
// answered without a browser.
test("a message the status line cannot show always gets the second channel", () => {
  for (const kind of ["", "ok", "error", undefined]) {
    assert.equal(
      needsToast(kind, false),
      true,
      `"${kind}" must still reach the user when the status line is not on screen`,
    );
  }
});

test("a refusal is escalated even when the status line IS on screen", () => {
  assert.equal(needsToast("error", true), true);
});

// The converse, and the reason this is a rule rather than "always toast": a card
// over the document for every toolbar press would be worse than the defect.
test("a confirmation the status line is already showing is not escalated", () => {
  for (const kind of ["", "ok", undefined]) {
    assert.equal(needsToast(kind, true), false, `"${kind}" must not throw a card`);
  }
});

test("a refusal stays on screen longer than a confirmation, because it has to be read", () => {
  assert.ok(toastDuration("error") > toastDuration(""));
  // Long enough to read a sentence with a way out in it, short enough not to be
  // litter — both inside the range a snackbar uses.
  for (const kind of ["error", "", "ok"]) {
    assert.ok(toastDuration(kind) >= 2000, `${kind} is too brief to read`);
    assert.ok(toastDuration(kind) <= 10000, `${kind} outstays its message`);
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

// A background task's progress and a reader's action feedback are not the same
// message, and one channel carried both. The font-provisioning
// pass wrote through `setStatus`, so it announced itself into a live region, it
// toasted wherever the footer is hidden, it reported itself to an embedding host
// as editor status, and it held the line for as long as the download took — on
// the default editor page 31.40 MB of coverage-driven CJK and colour-emoji
// faces, measured 2026-10-06.
test("background progress may have the status line only when nobody else wants it", () => {
  // Free: nothing there, or the line is already this task's own.
  assert.equal(backgroundProgressMayPaint("", false), true);
  assert.equal(backgroundProgressMayPaint("Fetching fonts for sample.docx…", true), true);
  // Taken: a reader's message, whatever kind it is. A confirmation counts as
  // much as a refusal — "Footnote added — type the note text" is an instruction
  // the reader is still following.
  assert.equal(backgroundProgressMayPaint("Footnote added — type the note text", false), false);
  assert.equal(
    backgroundProgressMayPaint("Viewing mode is read-only; switch to Editing", false),
    false,
  );
  // An absent flag is not a progress line. `delete`ing a `data-` attribute
  // leaves `undefined`, and treating that as "mine" would hand every reader's
  // message straight back to the next background pass.
  assert.equal(backgroundProgressMayPaint("Saved Q3 Report.docx", undefined), false);
  assert.equal(backgroundProgressMayPaint("Saved Q3 Report.docx", "1"), false);
});
