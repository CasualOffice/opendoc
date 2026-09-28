// The proofing contract, in node, with no browser and no worker.
//
// `docs/146` §4/§5, ADR-042. Everything that decides a finding is here rather
// than in the worker precisely so these questions can be asked cheaply — and
// so each of them can be driven RED by mutating one line of production code
// (SKILL.md §4). The mutations that were actually run are named beside the
// assertions they break.
//
// The rule these tests are shaped by, from `docs/114` §7: assert the POSITIVE
// case first. "A failed dictionary flags nothing" passes when the checker does
// not work at all, so every such test also asserts that something IS found.

import assert from "node:assert/strict";
import test from "node:test";

import {
  BASIC_PACK_VERSION,
  DEEP_SUGGESTION_BUDGET,
  PROOF_MESSAGE,
  PROOF_PROTOCOL_VERSION,
  UNAVAILABLE,
  createProofResponder,
  isFreshResult,
  proofCacheKey,
  proofParagraph,
} from "../src/proof_protocol.mjs";
import { parseDictionary, parseGlossary } from "../src/spelling.mjs";
import { byteOffsetToStringIndex, stringIndexToByteOffset } from "../src/text_rules.mjs";

const WORDS = [
  "a", "and", "are", "beside", "café", "correct", "emoji", "family", "good",
  "hello", "here", "is", "line", "of", "one", "ours", "page", "prose", "real",
  "repeated", "sentence", "the", "this", "too", "word", "wrong",
];
const DICTIONARY = parseDictionary(`${WORDS.join("\n")}\n---\n`);
const GLOSSARY = parseGlossary("opendoc\nqzxbrand\n");

/** The resources shape `proofParagraph` reads, with every tier separable. */
function resources({
  dictionary = DICTIONARY,
  glossary = GLOSSARY,
  personal = new Set(),
  ignoredWords = new Set(),
  ignoredRules = new Set(),
  locale = "en-US",
  deep = true,
} = {}) {
  return {
    dictionaries: new Map([[locale, dictionary]]),
    glossary,
    personal,
    ignoredWords,
    ignoredRules,
    packVersion: BASIC_PACK_VERSION,
    settingsRevision: 1,
    suggest: () => ({ suggestions: [], complete: deep }),
  };
}

const check = (text, options = {}, overrides = {}) =>
  proofParagraph(
    { text, locale: "en-US", spelling: true, grammar: true, ...overrides },
    resources(options),
  );

// ---- The cache key -------------------------------------------------------------

test("every part of the cache key changes the key", () => {
  const base = {
    documentId: 1,
    paragraphId: "p1",
    paragraphRevision: 3,
    locale: "en-US",
    packVersion: BASIC_PACK_VERSION,
    settingsRevision: "1:sg",
    profileRevision: 0,
  };
  const key = proofCacheKey(base);
  // The positive case first: the same inputs must give the same key, or the
  // cache never hits and the assertions below are vacuous.
  assert.equal(proofCacheKey({ ...base }), key);
  for (const [field, value] of Object.entries({
    documentId: 2,
    paragraphId: "p2",
    paragraphRevision: 4,
    locale: "en-GB",
    packVersion: "enhanced-1",
    settingsRevision: "2:sg",
    profileRevision: 1,
  })) {
    assert.notEqual(
      proofCacheKey({ ...base, [field]: value }),
      key,
      `${field} must be in the key: \`docs/146\` §4 names it, and a key that ignores ` +
        "it serves an answer computed under different conditions",
    );
  }
});

// ---- Staleness ------------------------------------------------------------------

test("a reply about a replaced document, or an edited paragraph, is rejected", () => {
  const revisions = new Map([["p1", 7]]);
  const context = { documentId: 5, revisionOf: (id) => revisions.get(id) ?? null };
  const reply = { documentId: 5, paragraphId: "p1", paragraphRevision: 7 };

  // Positive first. Without this the three rejections below prove nothing: a
  // predicate that always returns false rejects stale replies perfectly.
  assert.equal(isFreshResult(reply, context), true);

  assert.equal(
    isFreshResult({ ...reply, documentId: 4 }, context),
    false,
    "node ids restart per import, so the previous document's findings would " +
      "land on this document's paragraphs",
  );
  revisions.set("p1", 8);
  assert.equal(
    isFreshResult(reply, context),
    false,
    "the paragraph was edited while the check was in flight",
  );
  revisions.delete("p1");
  assert.equal(
    isFreshResult(reply, context),
    false,
    "the paragraph is no longer tracked at all",
  );
});

// ---- The defect this increment exists to prevent ---------------------------------

test("an UNAVAILABLE dictionary yields NO spelling findings — not every word", () => {
  const text = "This word qzxtypo is is wrong";
  // Positive: with a real dictionary exactly the invented word is flagged.
  const found = check(text);
  assert.deepEqual(
    found.filter((f) => f.kind === "spelling").map((f) => f.word),
    ["qzxtypo"],
  );

  const failed = check(text, { dictionary: UNAVAILABLE });
  assert.deepEqual(
    failed.filter((f) => f.kind === "spelling"),
    [],
    "a word list that could not be loaded must contribute NOTHING. It used to " +
      "be stored as an EMPTY dictionary, and an empty dictionary means no word " +
      "is known — so a failed fetch underlined every word in the document " +
      "(`docs/146` §2 CORRECTION). MUTATION: make `spellable` accept " +
      "`UNAVAILABLE` and this goes red with 5 findings.",
  );
  // ...and grammar is untouched by the spelling failure. This is the pair that
  // matters: the document is still being proofed, it is just not being spelled.
  assert.deepEqual(
    failed.filter((f) => f.kind === "grammar").map((f) => f.ruleId),
    ["doubled-word"],
  );
});

test("a dictionary that has not arrived yet is also not an empty one", () => {
  const found = proofParagraph(
    { text: "This word qzxtypo is wrong", locale: "fr-FR", spelling: true, grammar: false },
    resources(),
  );
  assert.deepEqual(found, [], "no dictionary for the locale means no spelling opinion");
});

test("grammar is decided from the text alone — no dictionary, no glossary", () => {
  const found = proofParagraph(
    { text: "This line has has a doubled word", locale: "en-US", spelling: false, grammar: true },
    {
      // Deliberately hostile: nothing a spelling pass would need exists.
      dictionaries: new Map(),
      glossary: null,
      personal: new Set(),
      ignoredWords: new Set(),
      ignoredRules: new Set(),
      suggest: () => assert.fail("grammar must never ask for a suggestion"),
    },
  );
  assert.deepEqual(
    found.map((f) => f.ruleId),
    ["doubled-word"],
    "MUTATION: gate the grammar block on `spellable` and this goes red — which " +
      "is the coupling `docs/146` §2 asked to be removed",
  );
});

// ---- The three tiers stay three tiers ---------------------------------------------

test("the glossary, the personal dictionary and the word list are separate tiers", () => {
  const text = "The word opendoc and qzxmine and qzxnobody are here";
  const flagged = (options) =>
    check(text, options)
      .filter((f) => f.kind === "spelling")
      .map((f) => f.word);

  assert.deepEqual(flagged({}), ["qzxmine", "qzxnobody"], "the glossary term is not flagged");
  assert.deepEqual(
    flagged({ personal: new Set(["qzxmine"]) }),
    ["qzxnobody"],
    "the user's word is accepted without the glossary absorbing it",
  );
  assert.deepEqual(
    flagged({ glossary: new Set() }),
    ["opendoc", "qzxmine", "qzxnobody"],
    "and clearing the glossary does not take the other tiers with it",
  );
  assert.deepEqual(
    flagged({ ignoredWords: new Set(["qzxnobody"]) }),
    ["qzxmine"],
    "the session ignore set is a fourth, and is independent of all three",
  );
});

// ---- UTF-16 on the wire, and the round trip to UTF-8 -------------------------------
//
// `docs/146` §9's first correctness bullet. The worker speaks JS string indices
// and the engine speaks UTF-8 byte offsets, and the conversion happens at one
// boundary — so the thing that must hold is that a finding's offsets survive the
// trip in both directions for text that is NOT one code unit per character.

const OFFSET_CASES = [
  // A combining acute: `e` + U+0301 is two code units, one grapheme, three bytes.
  ["combining marks", "A café and qzxtypo here"],
  // A surrogate pair: one code point, two code units, four bytes.
  ["an astral emoji", "Good \u{1F44D} and qzxtypo here"],
  // A ZWJ sequence: several code points, one perceived character.
  ["a ZWJ emoji sequence", "Family \u{1F468}‍\u{1F469}‍\u{1F467} and qzxtypo here"],
  // Right-to-left, with the marks the shaper inserts.
  ["RTL text", "Hello שלום and qzxtypo here"],
  // Two scripts in one paragraph, neither of them Latin-1.
  ["mixed scripts", "日本語 कर and qzxtypo here"],
];

for (const [name, text] of OFFSET_CASES) {
  test(`a finding's offsets survive UTF-16 to UTF-8 and back: ${name}`, () => {
    const found = check(text).filter((f) => f.kind === "spelling");
    // Positive case first: the misspelling AFTER the awkward text is found at
    // all. A tokenizer that gave up at the first astral character would make
    // every assertion below vacuously true.
    assert.deepEqual(
      found.map((f) => f.word),
      ["qzxtypo"],
      "the word after the non-Latin text must still be found",
    );
    const finding = found[0];
    // The offsets are JS string indices into the text that was sent.
    assert.equal(
      text.slice(finding.start, finding.end),
      "qzxtypo",
      "MUTATION: return a code-POINT index from the tokenizer instead of a " +
        "string index and this goes red on every case but the Latin one",
    );
    // …and the conversion the document adapter does is lossless in both
    // directions at exactly those positions.
    for (const index of [finding.start, finding.end]) {
      const bytes = stringIndexToByteOffset(text, index);
      assert.equal(
        byteOffsetToStringIndex(text, bytes),
        index,
        `${index} must round-trip through the engine's byte offsets`,
      );
    }
    // The byte range really is the word, as the engine would read it.
    const encoded = new TextEncoder().encode(text);
    const slice = encoded.slice(
      stringIndexToByteOffset(text, finding.start),
      stringIndexToByteOffset(text, finding.end),
    );
    assert.equal(new TextDecoder().decode(slice), "qzxtypo");
  });
}

test("an offset inside a surrogate pair is never produced", () => {
  const text = "qzxtypo \u{1F44D} qzxother";
  for (const finding of check(text)) {
    for (const index of [finding.start, finding.end]) {
      const code = text.charCodeAt(index);
      assert.ok(
        !(code >= 0xdc00 && code <= 0xdfff),
        `offset ${index} splits a surrogate pair, which the engine cannot address`,
      );
    }
  }
});

// ---- The responder, which is what BOTH transports run -------------------------------

test("the responder loads resources, answers a check, and names its protocol", () => {
  const responder = createProofResponder();
  const ready = responder.handle({
    type: PROOF_MESSAGE.resources,
    dictionaries: [{ locale: "en-US", text: `${WORDS.join("\n")}\n---\n` }],
    glossaryText: "opendoc\n",
    settingsRevision: 1,
  });
  assert.deepEqual(ready, { type: PROOF_MESSAGE.ready, version: PROOF_PROTOCOL_VERSION });

  const reply = responder.handle({
    type: PROOF_MESSAGE.check,
    requestId: 9,
    documentId: 2,
    spelling: true,
    grammar: true,
    paragraphs: [
      { paragraphId: "p1", paragraphRevision: 1, locale: "en-US", text: "This is is qzxtypo" },
    ],
  });
  assert.equal(reply.type, PROOF_MESSAGE.findings);
  assert.equal(reply.requestId, 9);
  assert.equal(reply.documentId, 2);
  assert.deepEqual(reply.results[0].findings.map((f) => f.kind), ["grammar", "spelling"]);
  // The reply carries back what identifies the paragraph, or the coordinator
  // cannot tell a fresh answer from a stale one.
  assert.equal(reply.results[0].paragraphId, "p1");
  assert.equal(reply.results[0].paragraphRevision, 1);
  assert.equal(responder.handle({ type: "something:else" }), null);
});

test("a word list that arrived as unavailable stays unavailable in the responder", () => {
  const responder = createProofResponder();
  responder.handle({
    type: PROOF_MESSAGE.resources,
    dictionaries: [{ locale: "en-US", unavailable: true }],
    glossaryText: "",
  });
  assert.equal(responder.state.dictionaries.get("en-US"), UNAVAILABLE);
  const reply = responder.handle({
    type: PROOF_MESSAGE.check,
    requestId: 1,
    documentId: 1,
    spelling: true,
    grammar: true,
    paragraphs: [
      { paragraphId: "p1", paragraphRevision: 1, locale: "en-US", text: "This is is qzxtypo" },
    ],
  });
  assert.deepEqual(
    reply.results[0].findings.map((f) => f.kind),
    ["grammar"],
    "grammar survives a failed word list; spelling says nothing rather than everything",
  );
});

test("suggestions arrive with the finding, and the deep search is budgeted honestly", () => {
  const responder = createProofResponder();
  responder.handle({
    type: PROOF_MESSAGE.resources,
    dictionaries: [{ locale: "en-US", text: `${WORDS.join("\n")}\n---\n` }],
    glossaryText: "opendoc\n",
  });
  const run = (text) =>
    responder.handle({
      type: PROOF_MESSAGE.check,
      requestId: 1,
      documentId: 1,
      spelling: true,
      grammar: false,
      paragraphs: [{ paragraphId: "p1", paragraphRevision: 1, locale: "en-US", text }],
    }).results[0].findings;

  const [near] = run("The sentance is here");
  assert.deepEqual(
    near.replacements.map((r) => r.text),
    ["sentence"],
    "the suggestion is computed while checking now, not on right-click " +
      "(`docs/114` §5.4 correction) — MUTATION: return `[]` from `suggest` and " +
      "this goes red",
  );
  assert.equal(near.suggestionsComplete, true);

  // Past the budget, a finding with nothing to offer must not CLAIM there is
  // nothing to offer: the deep search was never run for it.
  // DISTINCT invented words — a repeated one would be answered from the memo
  // and never spend a second unit of the budget, which is the whole point of
  // the memo and would make this test unable to reach the budget at all.
  const letters = "abcdefghijklmnopqrstuvwxyz";
  const many = Array.from(
    { length: DEEP_SUGGESTION_BUDGET + 8 },
    (_, i) => `qzx${letters[Math.floor(i / 26)]}${letters[i % 26]}zzq`,
  ).join(" ");
  const beyond = run(many);
  assert.ok(beyond.length > DEEP_SUGGESTION_BUDGET);
  assert.equal(
    beyond.at(-1).suggestionsComplete,
    false,
    "a word the budget never searched for must report that, or the menu says " +
      "'No spelling suggestions' about a search it did not do",
  );
});
