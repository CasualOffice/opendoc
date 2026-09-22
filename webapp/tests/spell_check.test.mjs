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

const DICTIONARY = ["and", "correct", "is", "line", "of", "one", "page", "prose", "sentence", "the", "this", "word", "wrong"]
  .join("\n")
  .concat("\n---\n");

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
      return DICTIONARY;
    },
    ...overrides,
  });

  return {
    checker,
    placed,
    statuses,
    fetched,
    flags: () => {
      placed.length = 0;
      checker.paint();
      return placed.map((el) => el.dataset.spellWord);
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

/** Runs the scan and waits for the lazy dictionary fetch to land. */
async function settle(h) {
  h.checker.refresh();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
  h.checker.refresh();
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
  assert.deepEqual(h.fetched, ["fake://en-US"], "one fetch per language, ever");
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
  assert.deepEqual(h.fetched, [], "and no word list is fetched for a language we do not have");
});

test("the document's own w:lang wins over the host default", async () => {
  const engine = fakeEngine(2, { language: "en-GB" });
  const h = harness(engine);
  await settle(h);
  assert.deepEqual(h.fetched, ["fake://en-GB"], "not the en-US default");
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
