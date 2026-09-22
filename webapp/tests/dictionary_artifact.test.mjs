// The shipped word lists are a DERIVED artifact, and this is what keeps them
// derived (SKILL.md §8: a published number or file is generated from a
// committed generator, or it is not published).
//
// `tools/build-dictionary.mjs --scowl <dir> --check` is the full proof, and it
// needs the 2.5 MB SCOWL release, which is not committed and must not be
// fetched by a test. What is checked here is everything the artifact's SHAPE
// can be held to without it — and the shape is not cosmetic: the parse depends
// on the separator line, the suggestion ranking depends on the common tier
// being a real subset, the possessive rule depends on the file NOT carrying
// possessives, and the reproducibility claim depends on the ordering being
// codepoint order rather than whatever locale the generator ran under.
//
// The strongest of them is the re-render: parse the committed file, re-emit it
// with the generator's own `renderDictionary`, and require the bytes to be
// identical. That fails on an unsorted file, a duplicate, a stray blank line,
// a missing or doubled trailing newline, and a hand edit of any kind.

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  COMMON_LEVEL,
  LANGUAGE_SETS,
  LEVEL,
  SCOWL_RELEASE,
  SCOWL_TARBALL_SHA256,
  keepEntry,
  renderDictionary,
  sortWords,
} from "../tools/build-dictionary.mjs";
import { SPELL_LANGUAGES, parseDictionary } from "../src/spelling.mjs";

const files = new Map(
  SPELL_LANGUAGES.map((language) => [
    language,
    readFileSync(new URL(`../dict/${language}.txt`, import.meta.url), "utf8"),
  ]),
);

test("the generator ships a language set for every dictionary the app can load", () => {
  assert.deepEqual(
    Object.keys(LANGUAGE_SETS).sort(),
    [...SPELL_LANGUAGES].sort(),
    "a language the app offers with no generator entry is a file nobody can " +
      "regenerate; one the generator makes that the app never loads is dead weight",
  );
});

test("re-emitting a committed file reproduces it byte for byte", () => {
  for (const [language, text] of files) {
    const { common, all } = parseDictionary(text);
    const rest = [...all].filter((word) => !common.has(word));
    const round = renderDictionary({ common: sortWords(common), rest: sortWords(rest) });
    assert.equal(
      round,
      text,
      `${language}.txt is not what the generator would write: it is unsorted, ` +
        "carries a duplicate or a blank line, or has been edited by hand",
    );
  }
});

test("every entry is one the generator's own filter would keep", () => {
  for (const [language, text] of files) {
    const { all } = parseDictionary(text);
    const rejected = [...all].filter((word) => !keepEntry(word));
    assert.deepEqual(
      rejected.slice(0, 10),
      [],
      `${language}.txt carries ${rejected.length} entries the generator rejects`,
    );
  }
});

test("no possessive forms: they are handled by rule, and would be ~29,500 lines", () => {
  for (const [language, text] of files) {
    const { all } = parseDictionary(text);
    const possessives = [...all].filter((word) => /['’]s$/.test(word));
    assert.deepEqual(possessives.slice(0, 5), [], `${language}.txt carries possessives`);
  }
});

test("the common tier is a real, smaller subset — the suggestion ranking needs it", () => {
  for (const [language, text] of files) {
    const { common, all } = parseDictionary(text);
    assert.ok(common.size > 0, `${language} has no common tier`);
    assert.ok(
      common.size < all.size,
      `${language}'s common tier is the whole file, which ranks nothing`,
    );
    for (const word of common) {
      assert.ok(all.has(word), `${word} is in ${language}'s common tier but not in the file`);
    }
  }
});

test("the two dialect lists are neither identical nor unrelated", () => {
  const us = parseDictionary(files.get("en-US")).all;
  const gb = parseDictionary(files.get("en-GB")).all;
  assert.ok(us.size > 50_000 && gb.size > 50_000);
  const shared = [...us].filter((word) => gb.has(word)).length;
  assert.ok(shared / us.size > 0.9, "the two share the English core");
  assert.ok(
    [...gb].some((word) => !us.has(word)) && [...us].some((word) => !gb.has(word)),
    "and each carries spellings the other does not — otherwise one is pointless",
  );
});

test("the SCOWL licence ships beside the data, with the clause that permits it", () => {
  const notice = readFileSync(new URL("../dict/LICENSE.SCOWL.txt", import.meta.url), "utf8");
  assert.match(notice, /Copyright 2000-2018 by Kevin Atkinson/);
  assert.match(
    notice.replace(/\s+/g, " "),
    /the output created from the scripts/,
    "this exact clause is why a GENERATED word list is redistributable under " +
      "Apache-2.0 — if it ever leaves the notice, the licence analysis in " +
      "docs/114 §2.2 no longer holds",
  );
});

test("the generator states its provenance, so a regeneration is verifiable", () => {
  assert.equal(SCOWL_RELEASE, "scowl-2020.12.07");
  assert.match(SCOWL_TARBALL_SHA256, /^[0-9a-f]{64}$/);
  assert.equal(LEVEL, 60, "the level the official Hunspell en_US dictionary uses");
  assert.ok(COMMON_LEVEL < LEVEL, "the common tier is a cut of the same list");
});
