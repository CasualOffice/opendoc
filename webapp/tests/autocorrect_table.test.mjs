// The editable AutoCorrect lists, per language (`docs/109` RM-19): the replace
// table and the first-letter exceptions, stored as the user's DIFFERENCE from
// the built-in lists, so Reset is "forget the difference" and a later built-in
// improvement still reaches a user who customised.
import assert from "node:assert/strict";
import test from "node:test";
import { admissibleEntry, createAutoCorrectTables, languageKey } from "../src/autocorrect_table.mjs";

function store(initial = {}) {
  const prefs = { ...initial };
  const tables = createAutoCorrectTables({
    readPref: (key) => prefs[key] ?? null,
    writePref: (key, value) => {
      prefs[key] = value;
    },
  });
  return { prefs, tables };
}

test("a language is its primary subtag; nonsense falls back to English", () => {
  assert.equal(languageKey("en-GB"), "en");
  assert.equal(languageKey("pt_BR"), "pt");
  assert.equal(languageKey("sr-Latn-RS"), "sr");
  assert.equal(languageKey(""), "en");
  assert.equal(languageKey("12"), "en");
});

test("English has the typo list, every language has the symbols", () => {
  const { tables } = store();
  assert.equal(tables.replacements("en-US").get("teh"), "the");
  assert.equal(tables.replacements("de-DE").has("teh"), false, "a typo in English is a word in German");
  assert.equal(tables.replacements("de-DE").get("(c)"), "©");
  assert.ok(tables.exceptions("en").has("e.g."));
  assert.ok(tables.exceptions("de").has("z.b."));
});

test("an added entry is stored as a difference, per language, and survives a new store", () => {
  const { prefs, tables } = store();
  assert.equal(tables.setReplacement("en-GB", "omw", "on my way"), true);
  assert.equal(tables.replacements("en-US").get("omw"), "on my way", "one table for every English region");
  assert.equal(tables.replacements("fr").has("omw"), false);
  assert.deepEqual(JSON.parse(prefs["opendoc.autocorrect.replace.en"]), { add: [["omw", "on my way"]], remove: [] });
  // A fresh store over the same prefs reads it back: it is remembered.
  assert.equal(store(prefs).tables.replacements("en").get("omw"), "on my way");
  assert.ok(store(prefs).tables.languages().includes("en"));
});

test("deleting a built-in entry is remembered as a removal, and Reset forgets both lists' differences", () => {
  const { prefs, tables } = store();
  tables.deleteReplacement("en", "teh");
  assert.equal(tables.replacements("en").has("teh"), false);
  tables.deleteException("en", "e.g.");
  tables.addException("en", "Approx.");
  assert.equal(tables.exceptions("en").has("e.g."), false);
  assert.equal(tables.exceptions("en").has("approx."), true, "stored lowercase");
  assert.equal(tables.customised("en"), true);
  tables.reset("en");
  assert.equal(tables.customised("en"), false);
  assert.equal(tables.replacements("en").get("teh"), "the");
  assert.equal(tables.exceptions("en").has("e.g."), true);
  assert.equal(tables.exceptions("en").has("approx."), false);
  assert.equal(prefs["opendoc.autocorrect.replace.en"], "");
});

test("an entry that cannot be typed on one line, or is empty, is refused", () => {
  const { tables } = store();
  assert.equal(admissibleEntry(""), false);
  assert.equal(admissibleEntry("  "), false);
  assert.equal(admissibleEntry("a\nb"), false);
  assert.equal(admissibleEntry("x".repeat(256)), false);
  assert.equal(admissibleEntry("x".repeat(255)), true);
  assert.equal(tables.setReplacement("en", "", "x"), false);
  assert.equal(tables.setReplacement("en", "a", "b\nc"), false);
  assert.equal(tables.addException("en", "two words."), false);
});

test("a corrupt stored list reads as the built-in one rather than failing to start", () => {
  const { tables } = store({ "opendoc.autocorrect.replace.en": "{not json", "opendoc.autocorrect.exceptions.en": "null" });
  assert.equal(tables.replacements("en").get("teh"), "the");
  assert.ok(tables.exceptions("en").has("e.g."));
});
