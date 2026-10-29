// The selection's word count, counted the way the engine counts (HF-051).
//
// The document's figures are the engine's; the selection's are counted in
// `word_count.mjs` from `copyText`'s output. Two counters of one quantity are
// two chances to disagree, so these cases pin the JS rule to the engine's —
// `split_whitespace` for words, `!is_whitespace` for characters, every char for
// characters with spaces — on the inputs where a naive rule differs.
import { test } from "node:test";
import assert from "node:assert/strict";

import { countText, selectionIsRange } from "../src/word_count.mjs";

test("words are maximal runs of non-whitespace, as split_whitespace counts them", () => {
  assert.deepEqual(countText("one two  three"), {
    words: 3,
    characters: 11,
    charactersWithSpaces: 14,
    paragraphs: 1,
  });
  // Leading, trailing and repeated whitespace makes no empty words.
  assert.equal(countText("   lead and trail   ").words, 3);
  assert.equal(countText("").words, 0);
});

test("whitespace is Unicode White_Space, not just ASCII space", () => {
  // No-break space, em space, ideographic space and a tab all separate words in
  // Rust's `split_whitespace`, so they must here too.
  const text = "a b c　d\te";
  assert.equal(countText(text).words, 5);
  assert.equal(countText(text).characters, 5);
  assert.equal(countText(text).charactersWithSpaces, 9);
});

test("a character is a code point, so an astral letter counts once", () => {
  const counts = countText("𠀀𠀁 😀");
  assert.equal(counts.words, 2);
  assert.equal(counts.characters, 3);
  assert.equal(counts.charactersWithSpaces, 4);
});

test("paragraph separators are not characters, and only paragraphs with text count", () => {
  // `copyText` joins paragraphs with `\n`. A selection from the end of one
  // paragraph to the start of the next holds no character of either.
  assert.deepEqual(countText("\n"), { words: 0, characters: 0, charactersWithSpaces: 0, paragraphs: 0 });
  assert.deepEqual(countText("end of one\nstart of two\n\nfour"), {
    words: 7,
    characters: 22,
    charactersWithSpaces: 26,
    paragraphs: 3,
  });
  // A word does not run across a paragraph boundary.
  assert.equal(countText("abc\ndef").words, 2);
});

test("a caret is not a selection; any covered position is", () => {
  const at = (node, offset) => ({ node, offset });
  assert.equal(selectionIsRange(null), false);
  assert.equal(selectionIsRange({ anchor: at("p1", 3), focus: at("p1", 3) }), false);
  assert.equal(selectionIsRange({ anchor: at("p1", 3), focus: at("p1", 4) }), true);
  assert.equal(selectionIsRange({ anchor: at("p1", 0), focus: at("p2", 0) }), true);
});
