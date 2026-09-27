// What the editor says when a structured paste carried the content but not every
// reference (`docs/129` §2).
//
// These are the assertions that make the report worth having. The engine drops
// ten markers in five families because a second copy of each is a defect rather
// than a copy; the one thing the host must never do is drop them QUIETLY, and the
// second is name a family a given paste did not degrade. Both are decisions, and
// both are testable in node because `paste_loss.mjs` is pure.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const { pasteLossFamilies, pasteLossMessage } = await import("../src/paste_loss.mjs");
const { resetI18n, setCatalogue, setLocale } = await import("../src/i18n.mjs");

const catalogue = (tag) => JSON.parse(readFileSync(join(WEBAPP, "locales", `${tag}.json`), "utf8"));

test("a faithful paste says nothing at all", () => {
  for (const nothing of [[], undefined, null, "bookmark"]) {
    assert.equal(pasteLossMessage(nothing), null, `${JSON.stringify(nothing)} is not a report`);
  }
});

test("the families are named in a fixed order, whatever order the engine reported", () => {
  assert.deepEqual(pasteLossFamilies(["trackedMove", "bookmark", "comment"]), [
    "bookmark",
    "comment",
    "trackedMove",
  ]);
  // A repeat is one mention: the engine counts markers, the sentence names kinds.
  assert.deepEqual(pasteLossFamilies(["comment", "comment"]), ["comment"]);
});

test("a family this host has no wording for is left out rather than printed raw", () => {
  // The engine may grow a sixth family before this module does. Printing
  // `hyperlinkAnchor` at a user is worse than naming only what is understood.
  assert.deepEqual(pasteLossFamilies(["bookmark", "somethingNewInTheEngine"]), ["bookmark"]);
  assert.equal(pasteLossMessage(["somethingNewInTheEngine"]), null);
});

test("the sentence is the catalogue's, in the active locale, with the list punctuated for it", () => {
  resetI18n();
  setCatalogue("en", catalogue("en"));
  setLocale("en");
  const english = pasteLossMessage(["bookmark", "comment", "fieldRange"]);
  // The serial comma is `Intl.ListFormat`'s for `en`, not a choice made here —
  // which is the point: the punctuation is the locale's answer, not the editor's.
  assert.equal(
    english,
    "Pasted, without bookmarks, comments, and field codes — those cannot be duplicated inside one document.",
  );
  // Not a key, and not a raw engine identifier.
  assert.ok(!english.includes("paste.loss"));
  assert.ok(!english.includes("fieldRange"));

  resetI18n();
  setCatalogue("de", catalogue("de"));
  setLocale("de");
  const german = pasteLossMessage(["bookmark", "comment"]);
  assert.ok(german.includes("Textmarken"), german);
  assert.ok(german.includes("Kommentare"), german);
  // German joins with "und", which is the whole reason `Intl.ListFormat` is used
  // instead of a hard-coded ", ".
  assert.ok(german.includes(" und "), german);
  assert.ok(!german.includes(" and "), german);
  resetI18n();
});

test("every shipped locale answers all five families and the frame", () => {
  const keys = [
    "paste.loss",
    "paste.loss.bookmark",
    "paste.loss.comment",
    "paste.loss.note",
    "paste.loss.fieldRange",
    "paste.loss.trackedMove",
  ];
  const english = catalogue("en");
  for (const tag of ["ar", "de", "es", "fr", "ja", "ru", "zh-Hans"]) {
    const entries = catalogue(tag);
    for (const key of keys) {
      assert.equal(typeof entries[key], "string", `${tag} is missing ${key}`);
      assert.notEqual(entries[key], english[key], `${tag}'s ${key} is still the English string`);
    }
    // The frame must keep its placeholder, or the list of families vanishes.
    assert.ok(entries["paste.loss"].includes("{items}"), `${tag} dropped {items}`);
  }
});
