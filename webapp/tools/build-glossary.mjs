#!/usr/bin/env node
// Generates `webapp/dict/glossary.txt` — the product and industry vocabulary the
// spell checker must never flag — from this repository's own committed sources.
//
// The owner's instruction, 2026-09-23: *"try to add industry glossary dictionary
// and industry names.. and more imp atlearn our product names .. fo it wont flag
// that as incorrect spelling"*, with the qualifier that it be derived from a
// committed source rather than hand-maintained, because a hand-maintained list
// rots (SKILL.md §8 — counts and lists in this repository are derived or they
// are not published).
//
// So nothing here is a curated list of words. Every term is EARNED from a file
// that is already in the repository and already reviewed:
//
//   1. **Our own names.** The workspace's crate names, the webapp package name,
//      and the site's own domain and titles. These are facts about the project
//      recorded in `Cargo.toml` / `package.json` / the site pages — the same
//      files that would have to change if a name changed.
//   2. **The vocabulary of our documentation.** Terms that appear often enough,
//      in enough separate documents, to be part of how this project talks —
//      `WordprocessingML`, `DrawingML`, `Hunspell`, `SCOWL`, `paginator`. A term
//      used once in one file is not vocabulary; it is a typo or a one-off.
//
// Text inside code spans and fenced blocks is EXCLUDED. Documentation here is
// full of identifiers (`effective_run_properties_in_range`, `w:lang`,
// `NodeId::from_parts`) and none of them is English a writer would type into a
// document. Including them would turn the glossary into a mechanism for
// accepting typos.
//
// Usage, from `webapp/`:
//   node tools/build-glossary.mjs            # write dict/glossary.txt
//   node tools/build-glossary.mjs --check    # verify only, exit 1 on drift
//
// The glossary is a SEPARATE TIER from the personal dictionary (`docs/114` §4):
// it ships with the product and is the same for everyone, while the personal
// dictionary belongs to the user and lives in their browser. They are different
// sets in `isKnownWord`, and clearing one has no effect on the other.

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { parseDictionary, skipReason } from "../src/spelling.mjs";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const WEBAPP = join(HERE, "..");
const REPO = join(WEBAPP, "..");

/** How often a term must appear across the documentation to count as
 *  vocabulary, and in how many separate files. Both, not either: a word
 *  repeated twenty times in one document is that document's subject, and a
 *  word appearing once each in three documents is probably a coincidence. */
export const MIN_OCCURRENCES = 4;
export const MIN_FILES = 2;

/** Anything shorter is noise, anything longer is not a word. */
export const MIN_TERM_LENGTH = 3;
export const MAX_TERM_LENGTH = 40;

/** Markdown with every code span and fenced block removed.
 *
 *  Exported because it is the rule that keeps identifiers out, and that rule is
 *  worth a test of its own: without it the glossary would accept
 *  `effective_run_properties_in_range` and, worse, every misspelling that ever
 *  appeared inside a quoted error message. */
export function stripCode(markdown) {
  return markdown
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/~~~[\s\S]*?~~~/g, " ")
    .replace(/`[^`\n]*`/g, " ")
    // Link targets and image sources are paths, not prose.
    .replace(/\]\([^)]*\)/g, "] ")
    // Bare URLs.
    .replace(/https?:\/\/\S+/g, " ");
}

/** Every candidate token in a stripped document.
 *
 *  Deliberately narrower than the checker's own tokenizer: a glossary term is
 *  ONE run of letters and digits. No apostrophes (a possessive is not a term)
 *  and — importantly — no hyphens.
 *
 *  Hyphens are excluded because `isKnownWord` already accepts a hyphenated
 *  compound whose every part is known, so `casual-doc-layout` and
 *  `byte-for-byte` need no entry of their own once their parts are covered.
 *  Admitting them produced fragments (`byte-for`, `all-target`, `build-vs`)
 *  that are not terms in any language, which is exactly the kind of junk a
 *  derived list has to be shaped to keep out. */
function candidates(text) {
  // Apostrophes are MATCHED and then rejected, rather than simply not matched.
  // Not matching them splits `didn't` into `didn` and `t`, and `didn` then
  // clears the occurrence threshold easily — the glossary would have shipped
  // `didn`, `doesn`, `isn` and `wasn` as accepted words.
  return (text.match(/\b\p{L}[\p{L}\p{Nd}'\u2019]*/gu) ?? []).filter(
    (term) => !/['\u2019]/.test(term),
  );
}

/** Files under `dir` matching `pattern`, one level deep. */
function filesIn(dir, pattern) {
  let entries = [];
  try {
    entries = readdirSync(dir);
  } catch {
    return [];
  }
  return entries
    .filter((name) => pattern.test(name))
    .map((name) => join(dir, name))
    .filter((path) => {
      try {
        return statSync(path).isFile();
      } catch {
        return false;
      }
    });
}

/**
 * Source 1 — our own names, from the files that define them.
 *
 * Crate names, the webapp package name, the site's own titles and its domain.
 * Split on hyphens, because that is how prose uses them: "the layout crate",
 * "casual-doc", "opendoc".
 */
export function ownNames() {
  const names = new Set();
  const add = (value) => {
    const term = String(value ?? "").trim();
    if (term) names.add(term);
  };

  for (const manifest of [
    ...readdirSync(join(REPO, "crates")).map((c) => join(REPO, "crates", c, "Cargo.toml")),
    ...readdirSync(join(REPO, "tools")).map((c) => join(REPO, "tools", c, "Cargo.toml")),
  ]) {
    let text = "";
    try {
      text = readFileSync(manifest, "utf8");
    } catch {
      continue;
    }
    const match = text.match(/^name\s*=\s*"([^"]+)"/m);
    if (!match) continue;
    // The parts, not the whole: `isKnownWord` splits a hyphenated compound and
    // accepts it when every part is known, so `casual-doc-layout` follows from
    // `casual`, `doc` and `layout` without an entry of its own.
    for (const part of match[1].split("-")) add(part);
  }

  const pkg = JSON.parse(readFileSync(join(WEBAPP, "package.json"), "utf8"));
  for (const part of String(pkg.name).split("-")) add(part);

  // The product and the organisation, as the site itself writes them. Read from
  // the pages rather than typed here, so a rename has one place to happen.
  for (const page of filesIn(WEBAPP, /\.page\.html$|^editor\.html$/)) {
    const html = readFileSync(page, "utf8");
    for (const [, title] of html.matchAll(/<title>([^<]+)<\/title>/g)) {
      for (const word of candidates(title)) add(word);
    }
    for (const [, host] of html.matchAll(/\b([a-z0-9-]+\.org|[a-z0-9-]+\.io)\b/g)) {
      add(host.split(".")[0]);
    }
  }
  return names;
}

/**
 * Source 2 — the fixture corpus's own vocabulary.
 *
 * `fixtures/manifest.json` is the committed description of every document this
 * project is tested against, and its feature tags are the names of the things
 * those documents contain (`round-trip`, `footnote`, `real-producer`). They are
 * read from the manifest rather than listed, so a new fixture brings its own
 * vocabulary with it.
 */
export function fixtureTerms() {
  const terms = new Set();
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(REPO, "fixtures", "manifest.json"), "utf8"));
  } catch {
    return terms;
  }
  for (const fixture of manifest.fixtures ?? []) {
    for (const field of [
      fixture.id,
      fixture.compatibilityProfile,
      fixture.limitProfile,
      fixture.performanceClass,
      ...(fixture.featureTags ?? []),
    ]) {
      for (const part of String(field ?? "").split("-")) {
        for (const term of candidates(part)) terms.add(term);
      }
    }
  }
  return terms;
}

/**
 * Source 3 — the vocabulary of our documentation.
 *
 * Returns `term -> { count, files }` for everything that cleared the code strip.
 */
export function documentationTerms() {
  const seen = new Map();
  const sources = [
    ...filesIn(join(REPO, "docs"), /\.md$/),
    ...filesIn(REPO, /^(README|AGENTS)\.md$/),
    ...filesIn(WEBAPP, /^README\.md$/),
  ];
  for (const path of sources) {
    const text = stripCode(readFileSync(path, "utf8"));
    const here = new Set(candidates(text));
    for (const term of candidates(text)) {
      const entry = seen.get(term) ?? { count: 0, files: new Set() };
      entry.count += 1;
      seen.set(term, entry);
    }
    for (const term of here) seen.get(term).files.add(path);
  }
  return seen;
}

/**
 * The glossary: our own names, plus documentation vocabulary the dictionary
 * does not already know, minus anything the checker would skip anyway.
 *
 * A term already in the en-US or en-GB list is left out: it is not a glossary
 * term, it is a word, and carrying it twice would only make the file bigger and
 * the diff noisier.
 */
export function buildGlossary() {
  const dictionaries = ["en-US", "en-GB"].map((language) =>
    parseDictionary(readFileSync(join(WEBAPP, "dict", `${language}.txt`), "utf8")),
  );
  const known = (term) =>
    dictionaries.some(
      (dictionary) =>
        dictionary.all.has(term) ||
        dictionary.all.has(term.toLowerCase()) ||
        dictionary.all.has(term[0].toUpperCase() + term.slice(1).toLowerCase()),
    );

  const glossary = new Set();
  const admit = (term) => {
    if (term.length < MIN_TERM_LENGTH || term.length > MAX_TERM_LENGTH) return;
    // Anything the checker skips by rule (ALL CAPS, digits) needs no entry —
    // and adding one would make the glossary look like it was doing work it is
    // not. The exception is that the user can turn those options off, so an
    // acronym IS admitted; a token with a digit is not.
    if (/\p{Nd}/u.test(term)) return;
    if (skipReason(term, { ignoreNumbers: true, ignoreUpper: false })) return;
    if (known(term)) return;
    glossary.add(term);
  };

  for (const name of ownNames()) admit(name);
  for (const term of fixtureTerms()) admit(term);
  for (const [term, { count, files }] of documentationTerms()) {
    if (count < MIN_OCCURRENCES || files.size < MIN_FILES) continue;
    admit(term);
  }
  return [...glossary].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

export function renderGlossary(terms) {
  return `${terms.join("\n")}\n`;
}

function main(argv) {
  const terms = buildGlossary();
  const text = renderGlossary(terms);
  const target = new URL("../dict/glossary.txt", import.meta.url);
  const digest = createHash("sha256").update(text, "utf8").digest("hex").slice(0, 16);
  const summary = `glossary: ${terms.length} terms, ${Buffer.byteLength(text, "utf8")} B, sha256 ${digest}…`;
  if (argv.includes("--check")) {
    let existing = "";
    try {
      existing = readFileSync(target, "utf8");
    } catch {
      existing = "";
    }
    if (existing === text) {
      console.log(`ok   ${summary}`);
    } else {
      console.error(`DIFF ${summary} — the committed glossary is not what this repository derives`);
      process.exitCode = 1;
    }
    return;
  }
  writeFileSync(target, text);
  console.log(`wrote ${summary}`);
}

if (process.argv[1] && process.argv[1].endsWith("build-glossary.mjs")) {
  main(process.argv.slice(2));
}
