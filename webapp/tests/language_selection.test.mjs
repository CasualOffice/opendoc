// The language of the selected text, as the status bar and Set Language read it
// (`docs/109` RM-42), against a stand-in engine: which span a caret belongs to,
// when a range is "mixed", and that the dialog's list carries a document's own
// tags.
import assert from "node:assert/strict";
import test from "node:test";
import {
  LANGUAGE_CHOICES,
  choiceMatches,
  languageChoices,
  selectionLanguage,
  spanLanguageAt,
} from "../src/language_selection.mjs";

const SPANS = [
  { start: 0, end: 11, language: "" },
  { start: 11, end: 18, language: "fr-FR" },
];

test("a caret belongs to the text before it, and the paragraph start to the first span", () => {
  assert.equal(spanLanguageAt(SPANS, 0), "");
  assert.equal(spanLanguageAt(SPANS, 11), "", "at the boundary the caret continues the English");
  assert.equal(spanLanguageAt(SPANS, 12), "fr-FR");
  assert.equal(spanLanguageAt(SPANS, 18), "fr-FR");
  assert.equal(spanLanguageAt([], 3), "");
  assert.equal(spanLanguageAt(SPANS, 99), "fr-FR", "past the end reads the last span");
});

function engine({ spans = SPANS, format = null } = {}) {
  const asked = [];
  return {
    asked,
    languageSpans: (node) => {
      asked.push(["languageSpans", node]);
      return JSON.stringify(spans);
    },
    selectionCharacterFormat: (...args) => {
      asked.push(["selectionCharacterFormat", ...args]);
      return JSON.stringify(format);
    },
    selectionEdge: (an, ao, fn, fo, end) => ({ node: end ? fn : an, offset: end ? fo : ao, free() {} }),
  };
}
const at = (node, offset) => ({ node, offset });

test("a range inside one paragraph is one language or mixed, read from the spans alone", () => {
  const doc = engine();
  assert.deepEqual(selectionLanguage(doc, { anchor: at("p", 12), focus: at("p", 18) }), { tag: "fr-FR", mixed: false });
  assert.deepEqual(selectionLanguage(doc, { anchor: at("p", 18), focus: at("p", 12) }), { tag: "fr-FR", mixed: false }, "backwards too");
  assert.deepEqual(selectionLanguage(doc, { anchor: at("p", 2), focus: at("p", 14) }), { tag: "", mixed: true });
  assert.deepEqual(selectionLanguage(doc, { anchor: at("p", 13), focus: at("p", 13) }), { tag: "fr-FR", mixed: false });
  assert.equal(doc.asked.some(([name]) => name === "selectionCharacterFormat"), false, "O(paragraph): no selection-wide read");
});

test("a range across paragraphs asks the character state once, and honours its mixed list", () => {
  const uniform = engine({ format: { values: { language: "de-DE" }, mixed: [] } });
  assert.deepEqual(selectionLanguage(uniform, { anchor: at("a", 0), focus: at("b", 3) }), { tag: "de-DE", mixed: false });
  const mixed = engine({ format: { values: {}, mixed: ["language"] } });
  assert.deepEqual(selectionLanguage(mixed, { anchor: at("a", 0), focus: at("b", 3) }), { tag: "", mixed: true });
});

test("the list is the curated set plus a document's own tags, searchable by name and tag", () => {
  const choices = languageChoices(["x-klingon", "fr-FR"], "en");
  assert.equal(choices.filter((c) => c.tag === "fr-FR").length, 1, "no duplicate for a tag already listed");
  assert.ok(choices.some((c) => c.tag === "x-klingon"), "a document's unknown tag is shown, not dropped");
  assert.equal(choices.length, new Set([...LANGUAGE_CHOICES, "x-klingon"]).size);
  const french = choices.find((c) => c.tag === "fr-FR");
  assert.equal(choiceMatches(french, "fren"), true);
  assert.equal(choiceMatches(french, "FR-fr"), true);
  assert.equal(choiceMatches(french, "german"), false);
  assert.equal(choiceMatches(french, ""), true);
});
