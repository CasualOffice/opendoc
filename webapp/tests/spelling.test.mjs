// The spelling rules, against the dictionary that actually ships.
//
// `docs/114` §7 asks for three things these tests are shaped by:
//
//   * **assert the positive case first.** "No squiggle on a correctly spelled
//     word" passes when the feature does not exist at all, so the first thing
//     asserted anywhere is that a misspelling IS found.
//   * **prove the cost claim, do not state it.** The keystroke-path claim
//     (`docs/107` §4: per-keystroke work is O(1) in document size) is
//     falsifiable, so `SpellScheduler` is driven with injected timers and the
//     number of checks is counted.
//   * **never use `@` as a probe marker** — the fidelity corpus contains
//     `info@docscentre.com`. `QZX` is the marker here.
//
// Every one of these was driven red by mutating `src/spelling.mjs`; the
// mutations and their output are in the commit message.

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  DEFAULT_SPELL_LANGUAGE,
  ParagraphCache,
  SpellScheduler,
  addressSpans,
  boundedEditDistance,
  dictionaryForLanguage,
  findMisspellings,
  isKnownWord,
  parseDictionary,
  skipReason,
  suggestionsFor,
  tokenizeWords,
  unsupportedLanguageMessage,
} from "../src/spelling.mjs";

const enUS = parseDictionary(
  readFileSync(new URL("../dict/en-US.txt", import.meta.url), "utf8"),
);
const enGB = parseDictionary(
  readFileSync(new URL("../dict/en-GB.txt", import.meta.url), "utf8"),
);

// ---- The positive case, first ------------------------------------------------

test("a misspelled word is found, with its exact span", () => {
  const text = "This sentance is wrong.";
  const found = findMisspellings(text, enUS.all);
  assert.deepEqual(
    found.map((m) => [m.word, m.start, m.end]),
    [["sentance", 5, 13]],
    "the checker must FLAG something before any 'and this is clean' test means anything",
  );
  assert.equal(text.slice(found[0].start, found[0].end), "sentance");
});

test("correctly spelled prose is left alone", () => {
  assert.deepEqual(findMisspellings("This sentence is spelled correctly.", enUS.all), []);
});

// ---- Capitalization: Hunspell's rule, not a lowercase compare -----------------

test("a lower-case entry accepts lower, Title and ALL CAPS", () => {
  for (const word of ["apple", "Apple", "APPLE"]) {
    assert.equal(isKnownWord(word, enUS.all), true, word);
  }
});

test("a Title-case entry does NOT accept the lower-case form", () => {
  assert.equal(isKnownWord("London", enUS.all), true);
  assert.equal(isKnownWord("LONDON", enUS.all), true);
  assert.equal(
    isKnownWord("london", enUS.all),
    false,
    "a lowercase comparison would accept this, and Word flags it — including " +
      "SCOWL's proper names only pays off if case still carries information",
  );
});

test("possessives are handled by rule, not by 29,500 extra lines in the file", () => {
  assert.equal(enUS.all.has("dog's"), false, "the file does not carry possessives");
  assert.equal(isKnownWord("dog's", enUS.all), true, "straight apostrophe");
  assert.equal(isKnownWord("dog’s", enUS.all), true, "curly apostrophe");
  assert.equal(isKnownWord("sentance's", enUS.all), false, "a wrong stem stays wrong");
});

test("a hyphenated compound is correct when both halves are", () => {
  assert.equal(isKnownWord("well-known", enUS.all), true);
  assert.equal(isKnownWord("well-knowne", enUS.all), false);
});

// ---- Dialects ------------------------------------------------------------------

test("the two dictionaries really do disagree about colour", () => {
  assert.equal(isKnownWord("colour", enUS.all), false);
  assert.equal(isKnownWord("colour", enGB.all), true);
  assert.equal(isKnownWord("color", enUS.all), true);
});

test("a language tag maps to the list that checks it, or to nothing at all", () => {
  assert.equal(dictionaryForLanguage("en-US"), "en-US");
  assert.equal(dictionaryForLanguage("en-GB"), "en-GB");
  assert.equal(dictionaryForLanguage("en-AU"), "en-GB", "Commonwealth shares the GB list");
  assert.equal(dictionaryForLanguage("en"), "en-US");
  assert.equal(dictionaryForLanguage(""), DEFAULT_SPELL_LANGUAGE);
  assert.equal(
    dictionaryForLanguage("fr-FR"),
    null,
    "a language with no list must resolve to NOTHING, so the caller has to say " +
      "so — silently checking French against English would flag every word, and " +
      "silently checking nothing looks exactly like a clean document",
  );
});

test("an unsupported language is named, not described generically", () => {
  assert.equal(unsupportedLanguageMessage("fr-FR"), "No spelling dictionary for fr-FR");
});

// ---- Tokenizing and the skip rules ---------------------------------------------

test("a token keeps an internal apostrophe and drops the quoting ones", () => {
  assert.deepEqual(
    tokenizeWords("don’t 'quoted' well-known"),
    [
      { start: 0, end: 5, word: "don’t" },
      { start: 7, end: 13, word: "quoted" },
      { start: 15, end: 25, word: "well-known" },
    ],
  );
});

test("digits, ALL CAPS and single characters are skipped, and each says why", () => {
  assert.equal(skipReason("QZX7"), "contains a digit");
  assert.equal(skipReason("QZXQZX"), "all caps");
  assert.equal(skipReason("q"), "single character");
  assert.equal(skipReason("qzxqzx"), "");
  assert.equal(skipReason("QZX7", { ignoreNumbers: false }), "all caps");
  assert.equal(skipReason("QZXQZX", { ignoreUpper: false }), "");
});

test("a URL, an email and a path are addresses, and nothing inside them is a word", () => {
  // The design proposed testing this on the TOKEN. It cannot work: `:`, `/`,
  // `@` and `.` are not word characters, so none of these is ever one token.
  const text = "See http://qzx.example/a-b and qzx@docscentre.example and docs/114.md";
  assert.ok(addressSpans(text).length >= 3, "the addresses are found in the text");
  assert.deepEqual(
    findMisspellings(text, enUS.all),
    [],
    "not one of qzx / example / docscentre / md may be squiggled",
  );
});

test("the word the caret is inside is not flagged while it is being typed", () => {
  const text = "A sentance here";
  assert.equal(findMisspellings(text, enUS.all).length, 1);
  assert.deepEqual(
    findMisspellings(text, enUS.all, { caretOffset: 8 }),
    [],
    "Word does not squiggle the word under the caret, and neither do we",
  );
  assert.equal(
    findMisspellings(text, enUS.all, { caretOffset: 10 }).length,
    0,
    "the caret at the very end of the word still counts as inside it",
  );
  assert.equal(
    findMisspellings(text, enUS.all, { caretOffset: 15 }).length,
    1,
    "...but a caret elsewhere in the paragraph does not suppress it",
  );
});

test("the personal dictionary and the session ignore set both suppress a word", () => {
  const text = "The qzxword is fine.";
  assert.equal(findMisspellings(text, enUS.all).length, 1);
  assert.deepEqual(findMisspellings(text, enUS.all, { personal: new Set(["qzxword"]) }), []);
  assert.deepEqual(findMisspellings(text, enUS.all, { ignored: new Set(["qzxword"]) }), []);
});

// ---- Suggestions ------------------------------------------------------------------

test("the named cases from the design come first, not fourth", () => {
  assert.equal(suggestionsFor("recieve", enUS)[0], "receive");
  assert.equal(
    suggestionsFor("teh", enUS)[0],
    "the",
    "tea, tee, ten and the are all one edit away, all common and all start " +
      "with t — only ranking a transposition above a substitution gets this right",
  );
  assert.equal(suggestionsFor("seperate", enUS)[0], "separate");
  assert.equal(suggestionsFor("occurence", enUS)[0], "occurrence");
  assert.equal(suggestionsFor("definately", enUS)[0], "definitely");
  assert.equal(
    suggestionsFor("acomodate", enUS)[0],
    "accommodate",
    "two edits away — this one only works because the second stage exists",
  );
});

test("a suggestion is offered in the case the typist used", () => {
  assert.equal(suggestionsFor("Recieve", enUS)[0], "Receive");
  assert.equal(suggestionsFor("RECIEVE", enUS)[0], "RECEIVE");
});

test("a word nothing is close to yields no suggestions, and does so quickly", () => {
  assert.deepEqual(suggestionsFor("qzxqzxqzxqzx", enUS), []);
});

test("bounded edit distance counts a transposition as one, and gives up at the bound", () => {
  assert.equal(boundedEditDistance("teh", "the", 2), 1, "a transposition is ONE edit");
  assert.equal(boundedEditDistance("teh", "the", 1), 1, "even at a bound of one");
  assert.equal(boundedEditDistance("acomodate", "accommodate", 2), 2);
  assert.equal(
    boundedEditDistance("kitten", "sitting", 2),
    3,
    "past the bound it returns max + 1 rather than the true distance (3 here)",
  );
});

// ---- The cost claim ------------------------------------------------------------

test("typing N characters runs the check ZERO times before the debounce elapses", () => {
  let now = 0;
  const timers = [];
  let checks = 0;
  const scheduler = new SpellScheduler({
    check: () => {
      checks += 1;
    },
    quiesceMs: 400,
    setTimer: (fn, ms) => {
      const handle = { fn, at: now + ms, live: true };
      timers.push(handle);
      return handle;
    },
    clearTimer: (handle) => {
      if (handle) handle.live = false;
    },
  });
  const advance = (ms) => {
    now += ms;
    for (const handle of timers) {
      if (handle.live && handle.at <= now) {
        handle.live = false;
        handle.fn();
      }
    }
  };

  for (let i = 0; i < 200; i += 1) {
    scheduler.noteDirty();
    advance(20); // a fast typist: 50 characters a second
  }
  assert.equal(
    checks,
    0,
    "a keystroke must never reach the dictionary — 200 of them while typing " +
      "continuously must not produce one check (docs/107 §4)",
  );
  advance(400);
  assert.equal(checks, 1, "and exactly one once the typing stops");
});

test("a scroll may ask for a shorter wait without becoming a second scheduler", () => {
  let now = 0;
  const timers = [];
  let checks = 0;
  const scheduler = new SpellScheduler({
    check: () => {
      checks += 1;
    },
    quiesceMs: 400,
    setTimer: (fn, ms) => {
      const handle = { fn, at: now + ms, live: true };
      timers.push(handle);
      return handle;
    },
    clearTimer: (handle) => {
      if (handle) handle.live = false;
    },
  });
  const advance = (ms) => {
    now += ms;
    for (const handle of timers) {
      if (handle.live && handle.at <= now) {
        handle.live = false;
        handle.fn();
      }
    }
  };
  scheduler.noteDirty(90);
  advance(100);
  assert.equal(checks, 1);
  scheduler.noteDirty();
  advance(100);
  assert.equal(checks, 1, "the default is unchanged by the override");
  advance(300);
  assert.equal(checks, 2);
});

// ---- The cache -------------------------------------------------------------------

test("a cached paragraph is reused while its text is unchanged and dropped when it is not", () => {
  const cache = new ParagraphCache(3);
  cache.set("p1", "hello", ["a"]);
  assert.deepEqual(cache.get("p1", "hello"), ["a"]);
  assert.equal(cache.get("p1", "hello there"), null, "an edited paragraph is re-checked");
});

test("the cache is bounded, and evicts the least recently used", () => {
  const cache = new ParagraphCache(2);
  cache.set("p1", "a", []);
  cache.set("p2", "b", []);
  cache.get("p1", "a"); // p1 is now the most recent
  cache.set("p3", "c", []);
  assert.equal(cache.size, 2, "a long scroll cannot grow the cache without bound");
  assert.notEqual(cache.get("p1", "a"), null, "the recently used entry survives");
  assert.equal(cache.get("p2", "b"), null, "the least recently used went");
});

// ---- The shipped artifact ----------------------------------------------------------

test("the dictionary files are the shape the parser and the ranking depend on", () => {
  for (const [name, dictionary] of [["en-US", enUS], ["en-GB", enGB]]) {
    assert.ok(dictionary.all.size > 80_000, `${name} has a plausible word count`);
    assert.ok(dictionary.common.size > 30_000, `${name} has a common tier`);
    assert.ok(
      dictionary.common.size < dictionary.all.size,
      `${name}'s common tier is a subset, not the whole file`,
    );
    for (const word of dictionary.all) {
      assert.ok(!/[\s/]/.test(word), `${name} contains a non-word entry: ${JSON.stringify(word)}`);
    }
    assert.equal(
      dictionary.all.has(""),
      false,
      `${name} must not carry an empty entry — it would accept every skipped token`,
    );
  }
});
