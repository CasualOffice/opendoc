// The link popover's one field (audit DIALOGS-11, DIALOGS-14): what a typed
// target means, and what is refused before anything is written.
import { test } from "node:test";
import assert from "node:assert/strict";
import { matchPlaces, resolveLinkInput, stepActiveOption } from "../src/link_popover_model.mjs";

const bookmarks = ["Intro", "Results", "results_2024"];

test("web addresses, with or without a scheme, are web links", () => {
  assert.deepEqual(resolveLinkInput("https://example.com/a?b=c"), { ok: true, target: "https://example.com/a?b=c", kind: "web" });
  assert.deepEqual(resolveLinkInput("  example.com/docs "), { ok: true, target: "https://example.com/docs", kind: "web" });
  assert.deepEqual(resolveLinkInput("localhost:8080"), { ok: true, target: "https://localhost:8080", kind: "web" });
  assert.deepEqual(resolveLinkInput("http://intranet.local"), { ok: true, target: "http://intranet.local", kind: "web" });
});

test("email addresses become mailto links", () => {
  assert.deepEqual(resolveLinkInput("ana@example.org"), { ok: true, target: "mailto:ana@example.org", kind: "email" });
  assert.deepEqual(resolveLinkInput("mailto:ana@example.org"), { ok: true, target: "mailto:ana@example.org", kind: "email" });
});

test("a place in this document must exist", () => {
  assert.deepEqual(resolveLinkInput("#Results", { bookmarks }), { ok: true, target: "#Results", kind: "place" });
  assert.deepEqual(resolveLinkInput("#Missing", { bookmarks }), { ok: false, why: "noBookmark" });
  assert.deepEqual(resolveLinkInput("#", { bookmarks }), { ok: false, why: "invalid" });
});

// Mutation: dropping the scheme check (`if (!EXTERNAL_LINK_SCHEMES.has(...))`)
// lets javascript: fall through to a generic "invalid", and this test goes red:
// the reader must be told WHY the target is refused, not that it is malformed.
test("text that is not a target is refused, and a scheme a link may not follow says so", () => {
  for (const text of ["", "   "]) assert.deepEqual(resolveLinkInput(text), { ok: false, why: "empty" });
  for (const text of ["not a url", "notes", "http://", "mailto:nobody"]) {
    assert.equal(resolveLinkInput(text).ok, false, text);
  }
  assert.deepEqual(resolveLinkInput("javascript:alert(1)"), { ok: false, why: "scheme" });
  assert.deepEqual(resolveLinkInput("file:///etc/passwd"), { ok: false, why: "scheme" });
});

test("places are matched by name, case-insensitively, in document order", () => {
  assert.deepEqual(matchPlaces(bookmarks, "res"), ["Results", "results_2024"]);
  assert.deepEqual(matchPlaces(bookmarks, "#intro"), ["Intro"]);
  assert.deepEqual(matchPlaces(bookmarks, ""), bookmarks);
  assert.deepEqual(matchPlaces(bookmarks, "", 1), ["Intro"]);
});

// The bookmark list is a listbox the keyboard walks while focus stays in the
// field (APG combobox). Mutation: making ArrowUp step forward like ArrowDown, or
// dropping the wrap, turns this red.
test("Down and Up walk the bookmark rows and wrap at both ends", () => {
  assert.equal(stepActiveOption(-1, 3, "ArrowDown"), 0);
  assert.equal(stepActiveOption(0, 3, "ArrowDown"), 1);
  assert.equal(stepActiveOption(2, 3, "ArrowDown"), 0);
  assert.equal(stepActiveOption(-1, 3, "ArrowUp"), 2);
  assert.equal(stepActiveOption(1, 3, "ArrowUp"), 0);
  assert.equal(stepActiveOption(0, 3, "ArrowUp"), 2);
  assert.equal(stepActiveOption(1, 3, "Home"), 1);
  assert.equal(stepActiveOption(0, 0, "ArrowDown"), -1);
});
