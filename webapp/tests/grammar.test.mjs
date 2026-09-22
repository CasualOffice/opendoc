// The grammar rules, each with the sentence it exists for AND the sentence it
// must leave alone.
//
// The second half is the point. A grammar checker's failure mode is not missing
// an error, it is marking correct prose: the user stops trusting the marks and
// then the real ones are invisible too. So every rule here is tested in both
// directions, and the "clean" cases are the ones that were actually hard —
// `It had had two owners`, `An hour later`, `a university`, `e.g. this`,
// `fig. 2`, `Version 3.5`, `J. Smith`.
//
// Each was driven RED by mutating `src/grammar.mjs`; the mutations and their
// output are in the commit message.

import assert from "node:assert/strict";
import test from "node:test";

import { GRAMMAR_RULES, findGrammarIssues, grammarSupports } from "../src/grammar.mjs";

/** `[ruleId, matchedText, firstReplacement]` for each finding. */
function found(text) {
  return findGrammarIssues(text).map((issue) => [
    issue.ruleId,
    text.slice(issue.start, issue.end),
    issue.replacements[0],
  ]);
}

test("the rule set is the one the docs describe", () => {
  assert.deepEqual(
    GRAMMAR_RULES.map((rule) => rule.id),
    [
      "doubled-word",
      "article-agreement",
      "subject-verb-agreement",
      "modal-of",
      "space-before-punctuation",
      "missing-space",
      "repeated-punctuation",
      "sentence-capitalisation",
    ],
    "docs/114 §11 lists exactly these; a rule added without a row there is a " +
      "rule nobody can find out about",
  );
});

test("a doubled word is caught, and the two English ones that are not errors are not", () => {
  assert.deepEqual(found("This is is a doubled word."), [["doubled-word", "is is", "is"]]);
  assert.deepEqual(found("It had had two owners."), [], "`had had` is ordinary English");
  assert.deepEqual(found("I know that that is true."), [], "and so is `that that`");
  assert.deepEqual(found("One, one more."), [], "separated by punctuation, not doubled");
});

test("a / an follows the SOUND, not the letter", () => {
  assert.deepEqual(found("I ate a apple."), [["article-agreement", "a", "an"]]);
  assert.deepEqual(found("An dog barked."), [["article-agreement", "An", "A"]]);
  assert.deepEqual(
    found("An hour later a university opened a one-off an FBI office."),
    [],
    "hour and FBI take `an` despite the consonant letter; university and " +
      "one take `a` despite the vowel letter — the four cases the letter test " +
      "gets wrong, and the four people actually write",
  );
});

test("pronoun-verb agreement, from the closed table", () => {
  assert.deepEqual(found("He have a plan."), [
    ["subject-verb-agreement", "He have", "He has"],
  ]);
  assert.deepEqual(found("they was late"), [
    ["subject-verb-agreement", "they was", "they were"],
  ]);
  assert.deepEqual(
    found("He has a plan and they were late and the reports was filed."),
    [],
    "the correct forms pass — and `the reports was` is NOT caught, because a " +
      "noun subject needs a tagger (docs/114 §11, stated gap)",
  );
});

test("could of / would of", () => {
  assert.deepEqual(found("We could of gone."), [["modal-of", "of", "have"]]);
  assert.deepEqual(found("We could have gone. She thought of it."), []);
});

test("spacing around punctuation", () => {
  assert.deepEqual(found("Wait , please ."), [
    ["space-before-punctuation", " ,", ","],
    ["space-before-punctuation", " .", "."],
  ]);
  assert.deepEqual(found("One,two,three."), [
    ["missing-space", ",", ", "],
    ["missing-space", ",", ", "],
  ], "every comma, not just the first — the rule must not consume its neighbours");
  assert.deepEqual(
    found("Version 3.5 costs 4,50 and see fig. 2; it works."),
    [],
    "a decimal point, a thousands comma and an abbreviation are not spacing errors",
  );
});

test("repeated punctuation, but not an ellipsis", () => {
  assert.deepEqual(found("Really!! Sure??"), [
    ["repeated-punctuation", "!!", "!"],
    ["repeated-punctuation", "??", "?"],
  ]);
  assert.deepEqual(found("Well... maybe -- perhaps."), [], "... and -- are deliberate");
});

test("a sentence starts with a capital, and an abbreviation does not end one", () => {
  assert.deepEqual(found("This ends. next one starts."), [
    ["sentence-capitalisation", "next", "Next"],
  ]);
  assert.deepEqual(
    found("Dr. smith arrived. e.g. this is fine. J. Smith wrote it."),
    [],
    "Dr. does not end a sentence; `e.g.` and an initial legitimately start " +
      "one in lower case",
  );
});

test("an address is never grammar-checked", () => {
  assert.deepEqual(
    found("See http://example.com/a.b and qzx@docscentre.example now."),
    [],
    "a URL has no grammar, and the corpus really does contain an email address",
  );
  // A probe a rule WOULD fire on: `missing-space` sees a colon between two
  // letters in `mailto:someone`. Without the address filter this is marked, so
  // the assertion above — which happens to trip no rule — is not on its own
  // evidence that the filter exists.
  assert.deepEqual(
    found("Write to mailto:someone@x.example now."),
    [],
    "the colon in a mailto: is not a missing space",
  );
});

test("two rules never mark the same span twice", () => {
  const issues = findGrammarIssues("He have have a plan.");
  for (let i = 1; i < issues.length; i += 1) {
    assert.ok(
      issues[i].start >= issues[i - 1].end,
      `overlapping marks: ${JSON.stringify(issues)}`,
    );
  }
});

test("the word being typed is not marked", () => {
  const text = "This is is a doubled word.";
  assert.equal(findGrammarIssues(text).length, 1);
  assert.deepEqual(
    findGrammarIssues(text, { caretOffset: 7 }),
    [],
    "a phrase still being typed is not yet a phrase with a mistake in it",
  );
});

test("an ignored rule produces nothing while the others still do", () => {
  const text = "This is is repeated , badly";
  assert.equal(findGrammarIssues(text).length, 2);
  const rest = findGrammarIssues(text, { ignored: new Set(["doubled-word"]) });
  assert.deepEqual(rest.map((issue) => issue.ruleId), ["space-before-punctuation"]);
});

test("the rules are English-only, and say so", () => {
  assert.equal(grammarSupports("en-US"), true);
  assert.equal(grammarSupports("en-GB"), true);
  assert.equal(
    grammarSupports("fr-FR"),
    false,
    "French puts a space before ; : ! ? — running these rules on it would " +
      "mark correct French as wrong on every sentence",
  );
});

test("ordinary correct prose produces nothing at all", () => {
  for (const sentence of [
    "Perfectly ordinary English prose with nothing wrong in it.",
    "The quick brown fox jumps over the lazy dog; it does so twice.",
    "She has a plan, and they were ready for it.",
    "An honest answer: a European union of one user, i.e. me.",
  ]) {
    assert.deepEqual(found(sentence), [], sentence);
  }
});
