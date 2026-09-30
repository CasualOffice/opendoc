#!/usr/bin/env node
// Generates the published language packs under `webapp/packs/` from their
// committed candidate lists (`docs/146` §6, ADR-042, Increment B).
//
// The rule this exists to obey is SKILL.md §9's first one: **a published number
// is generated from a committed artifact, or it is not published.** A pack
// publishes several — its size in bytes, its SHA-256, how many words it adds, and
// its version — and every one of them is a number a dialog shows a user and a
// manifest asks them to trust. So they are derived here, from a source that is in
// the repository, by a generator that is in the repository, with a `--check` mode
// that fails the build when the committed artifact is not what a fresh run
// produces. That is the same contract `build-dictionary.mjs` and
// `build-glossary.mjs` have, and `tests/pack_artifact.test.mjs` re-runs this the
// way `glossary_artifact.test.mjs` re-runs that one.
//
// Usage, from `webapp/`:
//   node tools/build-pack.mjs            # write packs/
//   node tools/build-pack.mjs --check    # verify only, exit 1 on drift
//
// ## What the generator actually decides, and why it is not a copy
//
// The candidate list is hand-reviewed prose (see its own header for the rule). The
// generator's job is the half a human cannot do reliably:
//
//   1. **Filter against the base tier.** Every candidate is checked against
//      `dict/en-US.txt`, `dict/en-GB.txt` AND `dict/glossary.txt`, through
//      `isKnownWord` — so the capitalization rule, the possessive rule and the
//      hyphen rule all apply, exactly as they will at check time. A word the base
//      tier already knows is dropped. This matters beyond file size: the PACK'S
//      WHOLE CLAIM is that it adds coverage, and a pack half full of words that
//      were already known cannot be told apart from one that does nothing.
//   2. **Reject anything not word-shaped**, through `packWordAdmissible`, which
//      the artifact guard also calls — one rule, two readers.
//   3. **Derive the version from the content.** `packVersion` is
//      `1.<first 12 hex of the asset's SHA-256>`, so it is immutable (`docs/146`
//      §5 asks for an immutable release identifier), it changes when and only when
//      the words change, and nobody has to remember to bump it. A version somebody
//      maintains by hand is a version that is wrong the first time two branches
//      touch the list.
//   4. **Derive the self-test.** The first, middle and last word of the artifact
//      are its positive examples; three strings that are not words are its
//      negative ones. Derived rather than listed, so the examples cannot go stale
//      when the list changes, and asserted here so a generator bug cannot ship a
//      self-test that passes vacuously.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { isKnownWord, parseDictionary, parseGlossary } from "../src/spelling.mjs";
import {
  PACK_ENGINE_API,
  PACK_SCHEMA_VERSION,
  packWordAdmissible,
} from "../src/proof_packs.mjs";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const WEBAPP = join(HERE, "..");
const PACKS = join(WEBAPP, "packs");
const SOURCES = join(PACKS, "sources");

/** The packs this build publishes. One row per pack, and every field a fact about
 *  where it comes from rather than about what it contains — the contents are the
 *  candidate file's business. */
export const PACK_SOURCES = Object.freeze([
  Object.freeze({
    packId: "casual-proof-en-US-supplement",
    locale: "en-US",
    source: "en-US-supplement.txt",
    directory: "en-US-supplement",
    /** The word lists this pack must ADD to, i.e. the base tier it is filtered
     *  against. Both English lists, because a pack installed for en-US is also
     *  offered to the Commonwealth dialects that share en-GB, and a word one list
     *  already has is not something this pack adds. */
    baseline: Object.freeze(["en-US", "en-GB"]),
  }),
]);

/** Strings the pack must NOT claim, for the self-test's negative half.
 *
 *  `QZX` rather than anything with an `@` in it: `docs/114` §5.3 records that the
 *  fidelity corpus really contains `info@docscentre.com`, so `@` is never a safe
 *  probe marker. These are shaped like words so that a pack which accepted
 *  everything — the failure the negative half exists to catch — would fail on
 *  them, which a string of punctuation would not prove. */
export const NEVER_KNOWN = Object.freeze(["qzxbletch", "qzxfrumious", "qzxwoggle"]);

/** The base tier, as the checker sees it. */
function baseTier(languages) {
  const dictionaries = languages.map((language) =>
    parseDictionary(readFileSync(join(WEBAPP, "dict", `${language}.txt`), "utf8")),
  );
  const glossary = parseGlossary(readFileSync(join(WEBAPP, "dict", "glossary.txt"), "utf8"));
  return (word) =>
    dictionaries.some((dictionary) => isKnownWord(word, dictionary.all, { glossary }));
}

/** Candidate lines: `#` comments and blank lines out, order preserved. */
export function readCandidates(name) {
  return readFileSync(join(SOURCES, name), "utf8")
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"));
}

/**
 * The words one pack ships: candidates that are word-shaped and that the base
 * tier does not already know, deduplicated and sorted.
 *
 * Returns the words plus the counts the summary prints, because "how many were
 * dropped as already known" is the number that says whether the candidate list is
 * still pulling its weight — a list that drops to nearly nothing is a list whose
 * dictionary caught up with it.
 */
export function buildPackWords(pack) {
  const known = baseTier(pack.baseline);
  const words = new Set();
  let rejected = 0;
  let alreadyKnown = 0;
  for (const candidate of readCandidates(pack.source)) {
    if (!packWordAdmissible(candidate)) {
      rejected += 1;
      continue;
    }
    if (known(candidate)) {
      alreadyKnown += 1;
      continue;
    }
    words.add(candidate.normalize("NFC"));
  }
  return {
    words: [...words].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)),
    rejected,
    alreadyKnown,
  };
}

/** The asset's text: one word per line, sorted, trailing newline — the same shape
 *  `dict/glossary.txt` has, which is why `words-plain-1` needs no new parser. */
export function renderPackWords(words) {
  return `${words.join("\n")}\n`;
}

function sha256(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/**
 * The manifest for one pack, as `docs/146` §5 shapes it.
 *
 * Every field is derived. The self-test's positives are the first, middle and
 * last word of the artifact — three points spread through the file, so a
 * truncated or half-written asset fails at least one of them — and the assertion
 * below is what stops a generator bug from writing a self-test that cannot fail.
 */
export function buildManifest(pack, words, text) {
  const digest = sha256(text);
  const packVersion = `1.${digest.slice(0, 12)}`;
  const known = [words[0], words[Math.floor(words.length / 2)], words.at(-1)];
  for (const word of known) {
    if (!words.includes(word)) throw new Error(`${pack.packId}: self-test positive ${word} absent`);
  }
  for (const word of NEVER_KNOWN) {
    if (words.includes(word)) throw new Error(`${pack.packId}: self-test negative ${word} present`);
  }
  return {
    schemaVersion: PACK_SCHEMA_VERSION,
    packId: pack.packId,
    locale: pack.locale,
    packVersion,
    engineApiRange: String(PACK_ENGINE_API),
    capabilities: ["spelling"],
    assets: [
      {
        id: "words",
        // RELATIVE, so the shipped pack resolves against whatever origin serves
        // the editor and `assetUrlAllowed`'s same-origin default admits it with no
        // host configuration at all. A host republishing these packs elsewhere
        // rewrites the url and names its origin; that is the seam, and it is why
        // the policy is a function of the caller's origin rather than a constant.
        url: `packs/${pack.directory}/words.txt`,
        bytes: Buffer.byteLength(text, "utf8"),
        sha256: digest,
        format: "words-plain-1",
      },
    ],
    provenance: [
      {
        name: "OpenDoc contemporary English supplement",
        version: packVersion,
        license: "Apache-2.0",
        url: "https://github.com/CasualOffice/opendoc/blob/main/webapp/packs/sources/en-US-supplement.txt",
      },
    ],
    selfTest: { known, unknown: [...NEVER_KNOWN] },
  };
}

/** Everything this build publishes, as `path -> text`. One function, so `--check`
 *  and the write path compare and emit exactly the same bytes. */
export function buildPackFiles() {
  const files = new Map();
  const catalogue = [];
  for (const pack of PACK_SOURCES) {
    const { words } = buildPackWords(pack);
    if (words.length === 0) throw new Error(`${pack.packId}: every candidate was filtered out`);
    const text = renderPackWords(words);
    const manifest = buildManifest(pack, words, text);
    files.set(join("packs", pack.directory, "words.txt"), text);
    catalogue.push(manifest);
  }
  // The catalogue the SDK's `catalogue()` reads: the manifests themselves, in one
  // file, so installing a pack is one fetch for the description and one per asset
  // rather than a fetch per pack to find out whether it exists.
  files.set(join("packs", "index.json"), `${JSON.stringify({ packs: catalogue }, null, 2)}\n`);
  return files;
}

function summarise() {
  const rows = [];
  for (const pack of PACK_SOURCES) {
    const { words, rejected, alreadyKnown } = buildPackWords(pack);
    const text = renderPackWords(words);
    rows.push(
      `${pack.packId}: ${words.length} words, ${Buffer.byteLength(text, "utf8")} B, ` +
        `sha256 ${sha256(text).slice(0, 16)}…, ${alreadyKnown} already in the base tier, ` +
        `${rejected} not word-shaped`,
    );
  }
  return rows;
}

function main(argv) {
  const files = buildPackFiles();
  const summary = summarise();
  if (argv.includes("--check")) {
    const drifted = [];
    for (const [relative, text] of files) {
      let existing = "";
      try {
        existing = readFileSync(join(WEBAPP, relative), "utf8");
      } catch {
        existing = "";
      }
      if (existing !== text) drifted.push(relative);
    }
    // A file under `packs/` that this build does NOT produce is drift too: a pack
    // whose source was deleted would otherwise stay published for ever, and its
    // manifest would go on offering a download of words nothing generates.
    for (const pack of PACK_SOURCES) {
      const directory = join(PACKS, pack.directory);
      let entries = [];
      try {
        entries = readdirSync(directory);
      } catch {
        entries = [];
      }
      for (const entry of entries) {
        if (!files.has(join("packs", pack.directory, entry))) {
          drifted.push(join("packs", pack.directory, entry));
        }
      }
    }
    if (drifted.length) {
      console.error(`DIFF packs: ${drifted.join(", ")} — run node tools/build-pack.mjs`);
      process.exitCode = 1;
      return;
    }
    for (const row of summary) console.log(`ok   ${row}`);
    return;
  }
  for (const [relative, text] of files) {
    const target = join(WEBAPP, relative);
    mkdirSync(join(target, ".."), { recursive: true });
    writeFileSync(target, text);
  }
  for (const row of summary) console.log(`wrote ${row}`);
}

if (process.argv[1] && process.argv[1].endsWith("build-pack.mjs")) {
  main(process.argv.slice(2));
}
