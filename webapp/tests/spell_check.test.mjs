// The checker's own behaviour, against a fake engine, in node.
//
// This file exists because of a guard that could not fail. The browser spec
// asserted "page 5's misspelling has no marker before you scroll there" and
// called that proof the scan is windowed — and it is not: `place()` returns
// null for a page with no sheet, so nothing paints outside the window whether
// the scan walked three paragraphs or three hundred thousand. Removing the
// page bound from the walk left that spec green (SKILL.md §4: a test that
// cannot fail is not a test). The extent of the scan is only observable from
// inside, so it is measured here, by counting what the engine is asked.
//
// `createSpellChecker` takes every engine and DOM collaborator through `io`,
// which is what makes this possible with no browser and no wasm.

import assert from "node:assert/strict";
import test from "node:test";

import { createSpellChecker, spellingContextCommands } from "../src/spell_check.mjs";
import { createProofResponder } from "../src/proof_protocol.mjs";

const DICTIONARY = ["and", "are", "badly", "both", "correct", "fine", "is", "line", "of", "one",
  "ours", "page", "prose", "repeated", "sentence", "the", "this", "too", "word", "wrong"]
  .join("\n")
  .concat("\n---\n");

/** The shipped glossary tier: not in any dictionary, not the user's. */
const GLOSSARY = ["OpenDoc", "opendoc", "qzxbrand"].join("\n").concat("\n");

/** `1 -> "aa"`, `2 -> "bb"`, … — a per-page invented word with no digits. */
const letter = (page) => String.fromCharCode(96 + page).repeat(2);

/** A document of `pagesCount` pages, two paragraphs each, one of them wrong. */
function fakeEngine(pagesCount = 6, { language = "" } = {}) {
  const paragraphs = [];
  for (let page = 1; page <= pagesCount; page += 1) {
    paragraphs.push({ node: `p${page}a`, page, text: "This page is one line of correct prose" });
    // No digit in the invented word: a token containing one is skipped by rule,
    // so a fixture that numbered its typos would test nothing.
    paragraphs.push({ node: `p${page}b`, page, text: `This word qzx${letter(page)} is wrong` });
  }
  const index = new Map(paragraphs.map((p, i) => [p.node, i]));
  const asked = { copyText: [], languageAt: [] };

  const cursor = (node, offset) => ({ node, offset, free() {} });

  const doc = {
    hitTest(page) {
      const found = paragraphs.find((p) => p.page === page);
      return found ? cursor(found.node, 0) : null;
    },
    moveCaret(node, offset, direction) {
      const at = index.get(node);
      if (direction === "left") {
        if (at === 0) return cursor(node, 0);
        const previous = paragraphs[at - 1];
        return cursor(previous.node, previous.text.length);
      }
      if (at === paragraphs.length - 1) return cursor(node, offset);
      return cursor(paragraphs[at + 1].node, 0);
    },
    paragraphLength(node) {
      return paragraphs[index.get(node)].text.length;
    },
    copyText(node) {
      asked.copyText.push(node);
      return paragraphs[index.get(node)].text;
    },
    caretRect(node) {
      return [paragraphs[index.get(node)].page, 0, 0, 2, 20];
    },
    selectionRects(node) {
      return [paragraphs[index.get(node)].page, 100, 200, 40, 14];
    },
  };
  if (language) {
    doc.languageAt = (node) => {
      asked.languageAt.push(node);
      return language;
    };
  }
  return { doc, paragraphs, asked };
}

/** A checker wired to a fake engine, with a fake overlay and fake timers. */
function harness(engine, overrides = {}) {
  const placed = [];
  const statuses = [];
  let windowFirst = 1;
  let windowLast = 2;
  let caret = null;
  const fetched = [];

  const checker = createSpellChecker({
    getDoc: () => engine.doc,
    windowPages: () =>
      Array.from({ length: windowLast - windowFirst + 1 }, (_, i) => ({
        pageNumber: windowFirst + i,
        wTwip: 12240,
        hTwip: 15840,
      })),
    place: (flat, kind) => {
      const el = { className: kind, dataset: {} };
      placed.push(el);
      return el;
    },
    caret: () => caret,
    enabled: () => true,
    defaultLanguage: () => "en-US",
    status: (text) => statuses.push(text),
    repaint: () => {},
    openWords: async () => null,
    dictionaryUrl: (language) => `fake://${language}`,
    fetchText: async (url) => {
      fetched.push(url);
      return url === "fake://glossary" ? GLOSSARY : DICTIONARY;
    },
    ...overrides,
  });

  return {
    checker,
    placed,
    statuses,
    fetched,
    /** The word lists actually requested, without the glossary — which every
     *  check loads once regardless of language and would otherwise mask what a
     *  test is really asking about. */
    dictionariesFetched: () => fetched.filter((url) => url !== "fake://glossary"),
    flags: () => {
      placed.length = 0;
      checker.paint();
      return placed
        .filter((el) => el.className === "spell-error")
        .map((el) => el.dataset.spellWord);
    },
    /** Every mark the paint pass placed, with its class and (for grammar) its
     *  rule — so a test can tell a spelling squiggle from a grammar one. */
    placedMarks: () => {
      placed.length = 0;
      checker.paint();
      return placed.map((el) => ({
        className: el.className,
        word: el.dataset.spellWord,
        rule: el.dataset.grammarRule,
      }));
    },
    setWindow(first, last) {
      windowFirst = first;
      windowLast = last;
    },
    setCaret(next) {
      caret = next;
    },
  };
}

/** Runs the scan and waits for everything it can set in motion to land: the
 *  two lazy asset fetches, the worker round trip for each, and the re-check
 *  each of those triggers.
 *
 *  A loop rather than a fixed pair of ticks, because the checker now COALESCES:
 *  at most one check is in flight and a scan that arrives while one is
 *  outstanding is deferred until the reply lands (`docs/146` §4, "budget and cap
 *  the worker queue"). So "everything has settled" genuinely takes several
 *  rounds, and a helper that hides the ordering is better than every test
 *  guessing how many. */
async function settle(h) {
  for (let round = 0; round < 6; round += 1) {
    h.checker.refresh();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

test("the checker flags the wrong word and leaves the right ones alone", async () => {
  const engine = fakeEngine();
  const h = harness(engine);
  await settle(h);
  assert.deepEqual(h.flags(), ["qzxaa", "qzxbb"], "one per page, for the two in the window");
});

test("the scan asks the engine about the pages in the window and NO others", async () => {
  const engine = fakeEngine(20);
  const h = harness(engine);
  h.setWindow(1, 2);
  await settle(h);

  const visited = [...new Set(engine.asked.copyText)];
  assert.deepEqual(
    visited,
    ["p1a", "p1b", "p2a", "p2b"],
    "the walk must stop at the window's last page. This is the assertion the " +
      "browser spec could not make: nothing paints outside the window either " +
      "way, so only the engine can say how far the scan went",
  );

  // And moving the window moves what is asked — it is not a one-shot prefix.
  engine.asked.copyText.length = 0;
  h.setWindow(9, 10);
  await settle(h);
  assert.deepEqual([...new Set(engine.asked.copyText)], ["p9a", "p9b", "p10a", "p10b"]);
});

test("a re-scan of an unchanged window asks the engine for no text at all", async () => {
  const engine = fakeEngine();
  const h = harness(engine);
  await settle(h);
  const first = engine.asked.copyText.length;
  assert.ok(first > 0);
  h.checker.refresh();
  assert.ok(
    engine.asked.copyText.length > first,
    "the text is still read — it is how the cache knows the paragraph is unchanged",
  );
  // What the cache saves is the CHECK, not the read. Proven by the word list
  // being fetched exactly once however many times the window is rescanned.
  h.checker.refresh();
  h.checker.refresh();
  assert.deepEqual(h.dictionariesFetched(), ["fake://en-US"], "one fetch per language, ever");
  assert.equal(
    h.fetched.filter((url) => url === "fake://glossary").length,
    1,
    "and exactly one for the glossary, however many times the window is rescanned",
  );
});

test("the word under the caret is not flagged, and reappears when the caret leaves", async () => {
  const engine = fakeEngine();
  const h = harness(engine);
  await settle(h);
  assert.deepEqual(h.flags(), ["qzxaa", "qzxbb"]);

  // "This word qzxaa is wrong" — the typo starts at index 10.
  h.setCaret({ node: "p1b", offset: 12 });
  await settle(h);
  assert.deepEqual(h.flags(), ["qzxbb"], "Word does not squiggle the word being typed");

  h.setCaret({ node: "p1b", offset: 0 });
  await settle(h);
  assert.deepEqual(h.flags(), ["qzxaa", "qzxbb"]);
});

test("Ignore all clears the word everywhere; Ignore once clears only that occurrence", async () => {
  const engine = fakeEngine();
  const h = harness(engine);
  await settle(h);

  const flagged = h.checker.misspellingAt({ node: "p1b", offset: 12 });
  assert.equal(flagged?.word, "qzxaa");
  h.checker.ignoreOnce(flagged);
  assert.deepEqual(h.flags(), ["qzxbb"]);

  h.checker.ignoreAll("qzxbb");
  await settle(h);
  assert.deepEqual(h.flags(), [], "and the other one goes too");
});

test("a paragraph in a language with no dictionary is reported by name, not skipped quietly", async () => {
  const engine = fakeEngine(2, { language: "fr-FR" });
  const h = harness(engine);
  await settle(h);
  assert.deepEqual(h.flags(), [], "nothing is checked, because nothing can be");
  assert.ok(
    h.statuses.includes("No spelling dictionary for fr-FR"),
    `the user must be told which language: got ${JSON.stringify(h.statuses)}`,
  );
  assert.equal(
    h.checker.statusNote(),
    "No spelling dictionary for fr-FR",
    "and the note survives a status line that gets cleared by something else",
  );
  assert.deepEqual(
    h.dictionariesFetched(),
    [],
    "and no word list is fetched for a language we do not have",
  );
});

test("the document's own w:lang wins over the host default", async () => {
  const engine = fakeEngine(2, { language: "en-GB" });
  const h = harness(engine);
  await settle(h);
  assert.deepEqual(h.dictionariesFetched(), ["fake://en-GB"], "not the en-US default");
  assert.ok(engine.asked.languageAt.length > 0, "asked once per paragraph, not per word");
});

test("turning it off clears every marker and stops the scan", async () => {
  const engine = fakeEngine();
  let on = true;
  const h = harness(engine, { enabled: () => on });
  await settle(h);
  assert.equal(h.flags().length, 2);

  on = false;
  h.checker.setEnabled(false);
  assert.deepEqual(h.flags(), []);
  engine.asked.copyText.length = 0;
  h.checker.refresh();
  assert.deepEqual(engine.asked.copyText, [], "an off checker does not read the document");

  on = true;
  h.checker.setEnabled(true);
  await settle(h);
  assert.equal(h.flags().length, 2, "and it comes straight back");
});

// ---- The context-menu rows -----------------------------------------------------

test("zero suggestions still renders a row, disabled, saying so", () => {
  const rows = spellingContextCommands({ word: "qzxtypo" }, [], {});
  assert.equal(rows[0].id, "spell.noSuggestions");
  assert.equal(rows[0].enabled, false);
  assert.match(rows[0].disabledReason, /qzxtypo/, "and it names the word");
  assert.deepEqual(
    rows.slice(1).map((r) => r.id),
    ["spell.ignoreOnce", "spell.ignoreAll", "spell.addToDictionary"],
    "an empty menu is never the answer (SKILL.md §10)",
  );
});

test("the suggestions lead the menu, and only they are gated by the review mode", () => {
  const rows = spellingContextCommands(
    { word: "recieve" },
    ["receive", "relieve"],
    {},
    "Switch to Editing mode to correct the spelling",
  );
  assert.deepEqual(
    rows.map((r) => r.id),
    [
      "spell.suggestion.0",
      "spell.suggestion.1",
      "spell.ignoreOnce",
      "spell.ignoreAll",
      "spell.addToDictionary",
    ],
  );
  assert.equal(rows[0].label, "receive");
  assert.equal(rows[0].enabled, false);
  assert.match(rows[0].disabledReason, /Editing mode/);
  for (const row of rows.slice(2)) {
    assert.notEqual(
      row.enabled,
      false,
      `${row.id} is a host-side decision, not a document mutation — a reader ` +
        "looking at a false positive can still dismiss it",
    );
  }
});

test("a suggestion row replaces through the caller's own path, with the flagged range", () => {
  const calls = [];
  const flagged = { node: "p1b", word: "recieve", start: 10, end: 17 };
  const rows = spellingContextCommands(flagged, ["receive"], {
    replace: (which, word) => calls.push([which, word]),
  });
  rows[0].run();
  assert.deepEqual(calls, [[flagged, "receive"]], "no mutation path of its own");
});

// ---- The glossary tier ----------------------------------------------------------

test("a product name from the shipped glossary is not flagged, and is not a personal word", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word opendoc is ours and qzxbrand is too";
  const h = harness(engine);
  h.setWindow(1, 1);
  await settle(h);
  assert.deepEqual(h.flags(), [], "our own names are not misspellings");
  assert.deepEqual(
    h.checker.personalWords(),
    [],
    "and they are NOT reported as words the user added — the glossary ships " +
      "with the product and the personal dictionary belongs to the user, and " +
      "confusing the two would make one look like the other's doing",
  );
  // `glossaryTerms()` used to be asserted here. It is gone: the glossary now
  // lives in the worker, with the dictionary and the rules, and an accessor
  // that answered from the coordinator would be a second copy of it. The tier
  // separation is asserted where the tiers now are — `proof_protocol.test.mjs`
  // — and here by the pair of facts a user can actually observe: the name is
  // not flagged, and it is not reported as a word they added.
});

test("the glossary and the personal dictionary are independent tiers", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word opendoc and qzxmine are both fine";
  const h = harness(engine);
  h.setWindow(1, 1);
  await settle(h);
  assert.deepEqual(h.flags(), ["qzxmine"], "one is glossary, the other is not yet known");

  await h.checker.addToDictionary("qzxmine");
  await settle(h);
  assert.deepEqual(h.flags(), []);
  assert.deepEqual(h.checker.personalWords(), ["qzxmine"], "only the user's own word");
  assert.ok(
    !h.checker.personalWords().includes("opendoc"),
    "adding a personal word must not absorb the glossary into it",
  );
});

test("a mistyped product name suggests the product name", async () => {
  // Through the REAL path now, not a hand-built `flagged`: suggestions are
  // computed in the worker while checking and arrive with the finding, so the
  // only honest way to ask this question is to put the typo in the document
  // and right-click it (`docs/146` §4, and the correction in `docs/114` §5.4).
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word opendco is wrong";
  const h = harness(engine);
  h.setWindow(1, 1);
  await settle(h);
  const flagged = h.checker.misspellingAt({ node: "p1b", offset: 12 });
  assert.equal(flagged?.word, "opendco");
  assert.ok(
    h.checker.suggestions(flagged).includes("opendoc"),
    "the glossary is searched for near matches, or the feature the owner asked " +
      "for stops at 'not flagged' and never helps anyone fix a typo in it",
  );
});

// ---- Grammar --------------------------------------------------------------------

test("grammar findings are painted with their own marker and carry the rule", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated wrong";
  const h = harness(engine, { grammarEnabled: () => true });
  h.setWindow(1, 1);
  await settle(h);
  const marks = h.placedMarks();
  assert.ok(
    marks.some((mark) => mark.className === "grammar-error" && mark.rule === "doubled-word"),
    `expected a grammar mark, got ${JSON.stringify(marks)}`,
  );
});

test("grammar runs with spelling OFF — they are two independent switches", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated and qzxtypo too";
  let spelling = true;
  const h = harness(engine, {
    enabled: () => spelling,
    grammarEnabled: () => true,
  });
  h.setWindow(1, 1);
  await settle(h);
  assert.ok(h.placedMarks().some((m) => m.className === "spell-error"));
  assert.ok(h.placedMarks().some((m) => m.className === "grammar-error"));

  spelling = false;
  h.checker.setEnabled(false);
  await settle(h);
  const marks = h.placedMarks();
  assert.deepEqual(
    marks.filter((m) => m.className === "spell-error"),
    [],
    "spelling off means no spelling marks",
  );
  assert.ok(
    marks.some((m) => m.className === "grammar-error"),
    "...and grammar keeps running, because the owner made it the more " +
      "important of the two and it does not depend on the dictionary",
  );
});

test("turning grammar on re-checks paragraphs the cache already holds", async () => {
  // The cache is keyed by node + language + WHICH PASSES ARE ON. Without the
  // last part, a paragraph already checked with grammar off is served straight
  // back from the cache when grammar is switched on, and the marks never appear
  // — and `setGrammarCheckEnabled` deliberately does NOT clear the cache
  // (nothing about the text changed), so this is the real path, not a contrived
  // one.
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated wrong";
  let grammar = false;
  const h = harness(engine, { grammarEnabled: () => grammar });
  h.setWindow(1, 1);
  await settle(h);
  assert.deepEqual(h.placedMarks().filter((m) => m.rule), [], "grammar is off");

  grammar = true;
  // Exactly what the command does — `refresh()` and no cache clear. It is
  // awaited now because the check happens in a worker: `settle` runs the same
  // `refresh()` and then lets the reply land.
  await settle(h);
  assert.ok(
    h.placedMarks().some((m) => m.rule === "doubled-word"),
    "the paragraph must be re-checked, not served from the grammar-off entry",
  );
});

test("ignoring a grammar rule silences that rule and nothing else", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated , badly";
  const h = harness(engine, { grammarEnabled: () => true });
  h.setWindow(1, 1);
  await settle(h);
  const rules = () => [...new Set(h.placedMarks().filter((m) => m.rule).map((m) => m.rule))].sort();
  assert.deepEqual(rules(), ["doubled-word", "space-before-punctuation"]);

  h.checker.ignoreRule("doubled-word");
  await settle(h);
  assert.deepEqual(rules(), ["space-before-punctuation"], "only the named rule goes");
});

test("a grammar menu explains the rule, offers its correction, and can turn it off", () => {
  const calls = [];
  const flagged = {
    kind: "grammar",
    ruleId: "doubled-word",
    message: "\u201cis\u201d is repeated",
    replacements: ["is"],
  };
  const rows = spellingContextCommands(flagged, ["is"], {
    replace: (which, text) => calls.push(["replace", text]),
    ignoreRule: (rule) => calls.push(["ignoreRule", rule]),
  });
  assert.deepEqual(
    rows.map((r) => r.id),
    ["grammar.explain", "grammar.suggestion.0", "grammar.ignoreRule"],
  );
  assert.equal(rows[0].enabled, false, "the explanation is a label, not an action");
  assert.equal(rows[0].label, flagged.message);
  rows[1].run();
  rows[2].run();
  assert.deepEqual(calls, [["replace", "is"], ["ignoreRule", "doubled-word"]]);
});

test("a grammar correction is gated by the review mode, like a spelling one", () => {
  const rows = spellingContextCommands(
    { kind: "grammar", ruleId: "doubled-word", message: "m", replacements: ["is"] },
    ["is"],
    {},
    "Switch to Editing mode to correct the spelling",
  );
  const suggestion = rows.find((r) => r.id === "grammar.suggestion.0");
  assert.equal(suggestion.enabled, false);
  assert.notEqual(
    rows.find((r) => r.id === "grammar.ignoreRule").enabled,
    false,
    "turning a rule off is a host decision, not a document mutation",
  );
});

// ---- Increment A: the decoupling, and the failure that used to underline
// ---- everything (`docs/146` §2, ADR-042) ----------------------------------------

/** A transport that HOLDS each findings reply until the test releases it —
 *  which is exactly what a busy worker does, and is not otherwise observable.
 *  Every other message (the `ready` handshake, resources) goes straight
 *  through, so only the thing under test is delayed. */
function heldTransport(state) {
  return (onReply) => {
    const responder = createProofResponder();
    return {
      post(message) {
        const reply = responder.handle(message);
        if (!reply) return;
        if (reply.type === "proof:findings") state.held = () => onReply(reply);
        else onReply(reply);
      },
      dispose() {},
    };
  };
}

test("a GRAMMAR-ONLY scan asks the network for nothing at all", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated wrong";
  const h = harness(engine, { enabled: () => false, grammarEnabled: () => true });
  h.setWindow(1, 1);
  await settle(h);

  // The positive case first: grammar really is running, so "no fetches" is not
  // "nothing happened".
  assert.ok(
    h.placedMarks().some((mark) => mark.rule === "doubled-word"),
    "grammar must run with spelling off, or this test proves nothing",
  );
  assert.deepEqual(
    h.fetched,
    [],
    "a grammar pass consults neither the word list nor the GLOSSARY, and must " +
      "therefore request neither. `docs/146` §2 named the dictionary; the " +
      "glossary was the one actually being fetched. MUTATION: move " +
      "`loadGlossary()` back above the `if (spelling)` and this goes red.",
  );
});

test("a dictionary that fails to load flags NOTHING, and grammar keeps running", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated and qzxtypo too";
  const h = harness(engine, {
    grammarEnabled: () => true,
    fetchText: async (url) => {
      if (url === "fake://glossary") return GLOSSARY;
      throw new Error("offline");
    },
  });
  h.setWindow(1, 1);
  await settle(h);
  await settle(h);

  const marks = h.placedMarks();
  assert.deepEqual(
    marks.filter((mark) => mark.className === "spell-error"),
    [],
    "a word list that could not be loaded used to be stored as an EMPTY one, " +
      "and an empty word list means no word is known — so every word in the " +
      "document was underlined while the status line said the dictionary had " +
      "failed. MUTATION: install `emptyDictionary()` on the catch path instead " +
      "of `unavailable` and this goes red with eight marks.",
  );
  assert.ok(
    marks.some((mark) => mark.rule === "doubled-word"),
    "...and the document is still being proofed: grammar does not depend on it",
  );
  assert.ok(
    h.statuses.some((text) => /could not be loaded/.test(text)),
    "the user is told, by name",
  );
  assert.match(
    h.checker.statusNote(),
    /could not be loaded/,
    "and it is a STANDING condition, so it survives a status line that " +
      "something else clears (`docs/114` §10.2)",
  );
});

test("a reply about text that has since been edited is discarded", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word qzxstale is wrong";
  const state = {};
  const h = harness(engine, { proofTransport: heldTransport(state) });
  h.setWindow(1, 1);
  await settle(h);

  // The request is out and the reply is held. Now the paragraph changes.
  assert.ok(state.held, "a check must have been posted");
  // Captured BEFORE the edit: the next check overwrites `state.held` with its
  // own reply, and delivering that one would be testing nothing.
  const stale = state.held;
  state.held = null;
  engine.paragraphs[1].text = "This word qzxfresh is wrong";
  h.checker.refresh();
  stale();
  await new Promise((resolve) => setTimeout(resolve, 0));

  assert.deepEqual(
    h.flags(),
    [],
    "the held reply is about `qzxstale`, which is not in the document any " +
      "more. MUTATION: drop the `isFreshResult` guard in `onReply` and this " +
      "goes red by painting `qzxstale` over text that now reads `qzxfresh`.",
  );

  // And the fresh answer, when it comes, IS painted — so the guard rejects
  // staleness rather than everything.
  state.held?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(h.flags(), ["qzxfresh"]);
});

test("a reply about the previous document is discarded", async () => {
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word qzxone is wrong";
  const state = {};
  const h = harness(engine, { proofTransport: heldTransport(state) });
  h.setWindow(1, 1);
  await settle(h);
  assert.ok(state.held);
  const stale = state.held;
  state.held = null;

  // A new document is opened while the check is in flight. Node ids come from a
  // counter that restarts per import, so `p1b` exists in both AT THE SAME
  // REVISION — which is precisely why the revision alone cannot answer this and
  // the document id has to.
  h.checker.reset();
  engine.paragraphs[1].text = "This word qzxtwo is wrong";
  h.checker.refresh();
  state.held?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(h.flags(), ["qzxtwo"], "the new document is checked");

  stale();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(
    h.flags(),
    ["qzxtwo"],
    "MUTATION: stop bumping `documentId` in `reset()` and this goes red with " +
      "`qzxone` — the previous document's findings land on the new document's " +
      "paragraph, at the same node id and the same revision",
  );
});

test("the in-process fallback is the worker's own responder, not a second checker", async () => {
  // `docs/146` §4 and ADR-042: the fallback must be a TRANSPORT. The only way
  // to assert that from outside is that routing every message through a
  // hand-built `createProofResponder` gives identical marks.
  const build = (overrides) => {
    const engine = fakeEngine(2);
    engine.paragraphs[1].text = "This word qzxtypo is is wrong";
    const h = harness(engine, { grammarEnabled: () => true, ...overrides });
    h.setWindow(1, 1);
    return h;
  };
  const viaDefault = build({});
  await settle(viaDefault);
  const viaExplicit = build({
    proofTransport: (onReply) => {
      const responder = createProofResponder();
      return {
        post(message) {
          const reply = responder.handle(message);
          if (reply) queueMicrotask(() => onReply(reply));
        },
        dispose() {},
      };
    },
  });
  await settle(viaExplicit);
  assert.deepEqual(viaExplicit.placedMarks(), viaDefault.placedMarks());
  assert.ok(viaDefault.placedMarks().length > 0, "and they both found something");
});

test("a reply that predates the word list does not poison the cache", async () => {
  // A real ordering, not a contrived one, and the reason the freshness test is
  // the KEY rather than the text: the first check of a paragraph is posted
  // before the word list has arrived, so it comes back with grammar findings
  // only — correct at the time. If that reply is written into the cache under
  // the key the coordinator computes AFTER the list arrives, the re-check hits
  // it and the document is never spell-checked at all. No text changed, so
  // nothing about the document can see it; what changed was the resources.
  const engine = fakeEngine(1);
  engine.paragraphs[1].text = "This word is is repeated and qzxtypo too";
  const h = harness(engine, { grammarEnabled: () => true });
  h.setWindow(1, 1);
  await settle(h);

  const marks = h.placedMarks();
  assert.ok(
    marks.some((mark) => mark.rule === "doubled-word"),
    "the grammar finding, which arrives before the word list does",
  );
  assert.ok(
    marks.some((mark) => mark.word === "qzxtypo"),
    "and the spelling finding, which cannot arrive until it has. MUTATION: " +
      "make `packVersionNow()` return the bare constant so the key stops " +
      "naming which resources produced the answer, and this goes red with the " +
      "grammar mark alone — a document that is silently never spell-checked.",
  );
});
