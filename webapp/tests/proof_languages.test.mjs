// "Proofing languages": the parts of the dialog that are decisions rather than DOM.
//
// The rendering is a browser's business (`tests/e2e` reaches the real dialog), but
// three things here are not, and all three have been the shape of a real defect in
// this repository:
//
//   1. **Every refusal has a sentence.** `proof_packs.mjs` returns a CODE, on
//      purpose, so the decision can be translated and tested. A code with no key
//      reaches a user as nothing at all — the dialog closes having said nothing,
//      which is the failure SKILL.md §10 names. So the map is asserted TOTAL over
//      `PACK_REFUSAL_CODES`, and every key is asserted to exist in `EN_STRINGS` and
//      in all eighteen catalogues.
//   2. **The sentence resolves.** `t()` returns the KEY on a miss, deliberately, so
//      it is visible in a screenshot — and `dialog-i18n-keys.spec.mjs` exists
//      because a dialog once painted `captionDialog.headingLevel` for the life of
//      the tab. A refusal sentence that came back as its own key would be that
//      defect in the one place a reader is already upset.
//   3. **The numbers.** Megabytes are rounded here rather than in the sentence, so
//      every locale rounds the same way, and a pack under 100 KB still reads as a
//      size rather than as `0 MB`.

import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";

import { PACK_REFUSAL, PACK_REFUSAL_CODES, PACK_SIZE_CEILING_BYTES } from "../src/proof_packs.mjs";
import {
  PACK_REFUSAL_KEYS,
  languageName,
  megabytes,
  refusalKeyGaps,
  refusalSentence,
} from "../src/proof_languages.mjs";
import { PROOF_SDK_REFUSAL_KEYS } from "../src/proof_sdk.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";

const LOCALES = new URL("../locales/", import.meta.url);
const catalogue = (tag) => JSON.parse(readFileSync(new URL(`${tag}.json`, LOCALES), "utf8"));
const tags = readdirSync(LOCALES)
  .filter((name) => name.endsWith(".json"))
  .map((name) => name.replace(/\.json$/, ""))
  .sort();

test("every pack refusal code has a sentence, and the map is total", () => {
  assert.deepEqual(refusalKeyGaps(), [], "a refusal with no sentence says nothing to a reader");
  for (const code of PACK_REFUSAL_CODES) {
    const key = PACK_REFUSAL_KEYS[code];
    assert.ok(key, `${code} has no key`);
    assert.ok(EN_STRINGS[key], `${key} is not declared in EN_STRINGS`);
  }
});

test("every refusal sentence is translated in all eighteen locales", () => {
  // The coverage gate already requires every EN_STRINGS key in every locale; this
  // asserts it for THESE keys by name, so the day one is added without a translation
  // the failure names the refusal rather than a count.
  const keys = new Set([...Object.values(PACK_REFUSAL_KEYS), ...Object.values(PROOF_SDK_REFUSAL_KEYS)]);
  const gaps = [];
  for (const tag of tags) {
    if (tag === "en") continue;
    const local = catalogue(tag);
    for (const key of keys) if (!local[key]) gaps.push(`${tag}: ${key}`);
  }
  assert.deepEqual(gaps, []);
});

test("a refusal's sentence resolves to prose, not to its own key", () => {
  setCatalogue("en", catalogue("en"));
  setLocale("en");
  for (const code of PACK_REFUSAL_CODES) {
    const sentence = refusalSentence({ code, detail: {} });
    assert.notEqual(sentence, PACK_REFUSAL_KEYS[code], `${code} painted its key`);
    assert.ok(sentence.length > 10, `${code}: ${JSON.stringify(sentence)}`);
  }
});

test("the size-limit refusal states the real ceiling, in megabytes", () => {
  setCatalogue("en", catalogue("en"));
  setLocale("en");
  const sentence = refusalSentence({ code: PACK_REFUSAL.tooLarge, detail: {} });
  assert.match(sentence, /\b50\b/, `ADR-042 §8's ~50 MB, from the constant: ${sentence}`);
  assert.equal(PACK_SIZE_CEILING_BYTES / (1024 * 1024), 50);
});

test("an unmapped code still says something rather than nothing", () => {
  // Defence in depth for the one case the totality guard cannot cover: a code added
  // in a later increment before its sentence lands. Ugly beats silent.
  assert.equal(refusalSentence({ code: "brand-new" }), "brand-new");
  assert.equal(refusalSentence(null), "");
});

test("megabytes never rounds a real pack down to nothing", () => {
  assert.equal(megabytes(50 * 1024 * 1024), 50);
  assert.equal(megabytes(1_572_864), 1.5);
  assert.equal(megabytes(1793), 0.1, "the shipped supplement is ~1.8 KB and must not read as 0 MB");
  assert.equal(megabytes(0), 0.1);
  assert.equal(megabytes("nonsense"), 0.1);
});

test("a language's name comes from the platform, and falls back to its tag", () => {
  // `Intl.DisplayNames` rather than eighteen catalogues of language names: the
  // platform already knows them, and a hand-maintained table of them is exactly the
  // hand-maintained list this repository derives instead (SKILL.md §8).
  assert.match(languageName("en-GB", "en"), /English/);
  assert.notEqual(languageName("en-GB", "de"), languageName("en-GB", "en"));
  // `Intl` answers for a tag it has never heard of rather than throwing, so the
  // fallback is reached by a locale it cannot parse at all — and what matters either
  // way is that the cell is never blank.
  assert.ok(languageName("qzx-ZZ", "en").includes("qzx"), "an unknown tag still names itself");
  assert.equal(languageName("en-US", "not a locale"), "en-US", "an unusable locale falls back");
  assert.ok(languageName("en-US", "").length > 0);
});
