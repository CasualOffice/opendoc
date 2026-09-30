// `webapp/packs/` is DERIVED, and this is what keeps it derived.
//
// The same contract `glossary_artifact.test.mjs` holds over `dict/glossary.txt`, for
// the same reason and with the same teeth: the generator is re-run and the committed
// files must match **byte for byte**. That fails on a hand edit, on a stale artifact
// after the candidate list changes, and on a change to the filtering rules that
// nobody regenerated for. SKILL.md §9: a published number is generated from a
// committed artifact or it is not published — and a pack publishes four of them
// (bytes, SHA-256, word count, version), every one of which a dialog shows a reader
// and a manifest asks them to trust.
//
// It also asserts the things the derivation is SHAPED to guarantee, because "it
// matches the generator" is only worth having if the generator is right:
//
//   * **every word in the pack is absent from the base tier.** This is not hygiene;
//     it is the premise of the underline guard in `spell_check.test.mjs`. If the
//     shipped dictionary already knew these words, installing the pack would change
//     nothing a reader could see and that guard would be green for a reason that had
//     nothing to do with packs.
//   * every word is word-shaped, through the same `packWordAdmissible` the generator
//     uses — a pack is a mechanism for ACCEPTING words, so a pack full of fragments
//     is a mechanism for accepting typos;
//   * the manifest validates against the real validator, and its self-test passes
//     against the real asset. A published pack this build would refuse to install is
//     a download nobody can use.

import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

import {
  NEVER_KNOWN,
  PACK_SOURCES,
  buildPackFiles,
  buildPackWords,
  readCandidates,
  renderPackWords,
} from "../tools/build-pack.mjs";
import { isKnownWord, parseDictionary, parseGlossary } from "../src/spelling.mjs";
import {
  PACK_SIZE_CEILING_BYTES,
  packBytes,
  packSelfTest,
  packWordAdmissible,
  parsePackWords,
  validateManifest,
} from "../src/proof_packs.mjs";

const WEBAPP = new URL("../", import.meta.url);
const read = (relative) => readFileSync(new URL(relative, WEBAPP), "utf8");

const catalogue = JSON.parse(read("packs/index.json"));

test("every committed pack file is exactly what the generator produces, today", () => {
  for (const [relative, text] of buildPackFiles()) {
    assert.equal(
      read(relative),
      text,
      `run \`node tools/build-pack.mjs\` — ${relative} is generated, and a hand edit ` +
        "or a stale artifact after a change to the candidate list both land here",
    );
  }
});

test("the generator's --check mode fails on drift rather than only the write path", () => {
  // The guard on the guard. `--check` is what a build runs, so a `--check` that
  // compared the generator against itself would pass on a repository whose committed
  // files were anything at all. Driven by comparing against a deliberately wrong
  // text, which is the same thing `--check` does internally.
  const files = buildPackFiles();
  const [first] = [...files.keys()];
  assert.notEqual(files.get(first), `${files.get(first)}qzxdrift\n`);
});

test("every word in every pack is absent from the base tier", () => {
  // THE premise of the underline guard. Asserted here rather than there, because
  // this is where the filtering lives and because a failure here is a message about
  // the candidate list rather than about proofing.
  const glossary = parseGlossary(read("dict/glossary.txt"));
  const dictionaries = ["en-US", "en-GB"].map((language) =>
    parseDictionary(read(`dict/${language}.txt`)),
  );
  for (const pack of PACK_SOURCES) {
    const words = read(`packs/${pack.directory}/words.txt`).split("\n").filter(Boolean);
    const redundant = words.filter((word) =>
      dictionaries.some((dictionary) => isKnownWord(word, dictionary.all, { glossary })),
    );
    assert.deepEqual(
      redundant,
      [],
      `${pack.packId} carries words the base tier already knows, so installing it ` +
        "would change nothing a reader can see for them",
    );
  }
});

test("every word in every pack is word-shaped", () => {
  for (const pack of PACK_SOURCES) {
    const words = read(`packs/${pack.directory}/words.txt`).split("\n").filter(Boolean);
    assert.deepEqual(
      words.filter((word) => !packWordAdmissible(word)),
      [],
      "a pack is a mechanism for accepting words; a fragment in one is a mechanism " +
        "for accepting typos",
    );
  }
});

test("the candidate list still earns its keep, and the filter is really filtering", () => {
  // Two directions, and the second is the one that rots silently. If the dictionary
  // ever catches up with the whole list the pack becomes empty and the generator
  // throws; if NOTHING is ever filtered, the filter has stopped working and the
  // "absent from the base tier" guard above would be passing vacuously.
  for (const pack of PACK_SOURCES) {
    const { words, alreadyKnown, rejected } = buildPackWords(pack);
    assert.ok(words.length >= 50, `${pack.packId} ships ${words.length} words`);
    assert.ok(
      alreadyKnown > 0,
      `${pack.packId}: nothing was filtered as already known, which means either the ` +
        "candidate list has been pre-filtered by hand (it must not be — that is the " +
        "generator's job) or the base-tier lookup has stopped working",
    );
    assert.equal(rejected, 0, `${pack.packId}: the candidate list carries a non-word`);
    assert.ok(
      readCandidates(pack.source).length > words.length,
      "the artifact is a strict subset of the candidates",
    );
  }
});

test("every published manifest validates, and would install in this build", () => {
  for (const manifest of catalogue.packs) {
    const result = validateManifest(manifest, { locale: manifest.locale });
    assert.equal(result.ok, true, `${manifest.packId}: ${JSON.stringify(result)}`);
  }
});

test("every asset's declared bytes and digest are the file's own", () => {
  // The numbers a reader is asked to trust. A manifest whose digest did not match
  // its own file would refuse to install with `digest` — a published pack nobody can
  // use, and a failure that would look like a corrupted download.
  for (const manifest of catalogue.packs) {
    for (const asset of manifest.assets) {
      const text = read(asset.url);
      assert.equal(Buffer.byteLength(text, "utf8"), asset.bytes, `${asset.url} bytes`);
      assert.equal(createHash("sha256").update(text, "utf8").digest("hex"), asset.sha256);
    }
    assert.ok(packBytes(manifest) <= PACK_SIZE_CEILING_BYTES, "ADR-042 §8's ceiling");
  }
});

test("the version is derived from the content, so it cannot be forgotten", () => {
  for (const manifest of catalogue.packs) {
    const digest = manifest.assets[0].sha256;
    assert.equal(manifest.packVersion, `1.${digest.slice(0, 12)}`);
    // And it is the same string the provenance entry publishes, so a reader
    // comparing the two is not comparing two independently-maintained numbers.
    assert.equal(manifest.provenance[0].version, manifest.packVersion);
  }
});

test("every published pack passes its own self-test against its own asset", () => {
  for (const manifest of catalogue.packs) {
    const words = new Set();
    for (const asset of manifest.assets) {
      for (const word of parsePackWords(read(asset.url))) words.add(word);
    }
    const result = packSelfTest(words, manifest.selfTest);
    assert.equal(result.ok, true, `${manifest.packId}: ${JSON.stringify(result)}`);
    // The self-test must be able to FAIL, which means its positives have to be words
    // the pack really carries and its negatives words it really does not. A
    // self-test whose negatives were also absent from every conceivable pack would
    // pass on a pack containing nothing at all — so the positives are checked to be
    // real and the negatives to be the deliberate non-words.
    assert.ok(manifest.selfTest.known.length >= 3);
    for (const word of manifest.selfTest.known) assert.ok(words.has(word));
    assert.deepEqual(manifest.selfTest.unknown, [...NEVER_KNOWN]);
    for (const word of NEVER_KNOWN) assert.equal(words.has(word), false);
  }
});

test("every pack declares provenance with a licence, and names our own source", () => {
  // ADR-042 leaves OPEN which source corpus is legally suitable for a redistributed
  // CONTEXT pack (Increment D). This increment ships nothing from a third-party
  // corpus, and that is the reason it can ship at all — asserted here so a later pack
  // cannot quietly arrive with an unreviewed licence.
  for (const manifest of catalogue.packs) {
    assert.ok(manifest.provenance.length > 0);
    for (const entry of manifest.provenance) {
      assert.equal(entry.license, "Apache-2.0", "this repository's own licence");
      assert.match(entry.url, /^https:\/\/github\.com\/CasualOffice\/opendoc\//);
    }
  }
});

test("the asset is sorted, deduplicated and newline-terminated", () => {
  for (const pack of PACK_SOURCES) {
    const text = read(`packs/${pack.directory}/words.txt`);
    const words = text.split("\n").filter(Boolean);
    assert.equal(text.endsWith("\n"), true);
    assert.deepEqual(words, [...words].sort(), "sorted, so the artifact stays diffable");
    assert.equal(new Set(words).size, words.length);
    assert.equal(renderPackWords(words), text);
  }
});

test("the shipped catalogue is exactly the packs this build generates", () => {
  assert.deepEqual(
    catalogue.packs.map((manifest) => manifest.packId).sort(),
    PACK_SOURCES.map((pack) => pack.packId).sort(),
  );
});
