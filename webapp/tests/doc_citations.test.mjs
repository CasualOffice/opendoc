// Every `docs/NNN` citation in the tree names a numbered document that exists.
//
// This repository cites design docs from code — well over a thousand references
// across the crates, the webapp and the docs — and a citation is load-bearing: it
// is how a reader finds out WHY a control is disabled, why an operation refuses,
// why a default is what it is. `docs/99` §9.2 records the class this belongs to:
// prose describing something that was never armed. A comment pointing at a
// document nobody wrote is that defect with the evidence removed, and it is worse
// than no comment, because the reader spends time looking for the file.
//
// It happened on the branch that added this test. A module comment cited
// `docs/127` for the Link-to-Previous design while that document did not exist —
// the lane that wrote the citation intended to write the doc and stopped first. So
// this guard exists.
//
// WHAT IT DOES NOT CATCH, stated because a guard cited as evidence for more than
// it checks is the thing this repository keeps getting burnt by:
//
//   * A citation naming a doc that exists but is about something ELSE. That is
//     exactly how the `docs/127` case ended: while the citation sat unresolved,
//     `main` landed `127-FIELD-REFERENCES-DESIGN.md` under the number, so the
//     reference resolved to a real file about the wrong subject. Nothing
//     mechanical can read a citation's intent. The mitigation is a habit, not a
//     test: WRITE THE DOC IN THE SAME COMMIT AS THE CITATION, so the number is
//     claimed in the tree the moment it is claimed in a comment.
//   * An AMBIGUOUS number. `docs/` currently holds two documents numbered 83, 86,
//     94, 95, 105 and 114, one of which (105) is among the most-cited numbers in
//     the tree. Such a citation resolves to two files and the reader has to guess
//     which. This test
//     deliberately accepts them rather than reddening on arrival — renaming a
//     document is a separate change, and a guard that has to be suppressed to
//     land is a guard nobody trusts. Reported, not enforced.
//
// Buildless, in the existing `npm run test:unit` lane, the same as
// `tracker_counts.test.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** Directories that hold build output, dependencies or fixtures rather than
 *  source. `target` and `node_modules` are the expensive ones; `pkg` is generated
 *  wasm glue and `test-results` is Playwright's. */
const SKIP_DIRS = new Set([
  "node_modules",
  "target",
  "pkg",
  "test-results",
  ".git",
]);

/** The trees a citation can live in. Everything a human writes prose in. */
const ROOTS = ["crates", "docs", "webapp/src", "webapp/tests"];

/** Plus the hand-written pages at the webapp root. `editor.html` is one of them,
 *  and it is where the Link-to-Previous citation that prompted this test lives, so
 *  leaving the root out would have left the original defect uncovered. */
const ROOT_FILES = "webapp";

const SOURCE_EXT = /\.(rs|mjs|js|html|md|css|py|json|toml)$/;

function walk(dir, out) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      walk(join(dir, entry.name), out);
    } else if (SOURCE_EXT.test(entry.name)) {
      out.push(join(dir, entry.name));
    }
  }
  return out;
}

function sourceFiles() {
  const files = [];
  for (const root of ROOTS) {
    const abs = join(repoRoot, root);
    try {
      if (statSync(abs).isDirectory()) walk(abs, files);
    } catch {
      // A root that is not in this checkout is not this test's business.
    }
  }
  // The webapp's own root, files only — its subdirectories are covered above or
  // are build output.
  const webappRoot = join(repoRoot, ROOT_FILES);
  for (const entry of readdirSync(webappRoot, { withFileTypes: true })) {
    if (!entry.isDirectory() && SOURCE_EXT.test(entry.name)) {
      files.push(join(webappRoot, entry.name));
    }
  }
  return files;
}

/** Every number `docs/` actually holds. A document is `NNN-TITLE.md`, and the
 *  number is compared NUMERICALLY so `docs/5`, `docs/05` and `05-FOO.md` are one
 *  answer — both spellings are in use in the tree. */
function numbersOnDisk() {
  const numbers = new Set();
  for (const name of readdirSync(join(repoRoot, "docs"))) {
    const match = /^(\d+)-/.exec(name);
    if (match) numbers.add(Number(match[1]));
  }
  return numbers;
}

test("every docs/NNN citation names a document that exists", () => {
  const present = numbersOnDisk();
  assert.ok(
    present.size > 100,
    `only ${present.size} numbered docs found — the scan is looking in the wrong place, ` +
      `and a citation guard that finds no docs would pass by failing to look`,
  );

  const dangling = new Map();
  let citations = 0;
  for (const file of sourceFiles()) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(/docs\/(\d{1,3})/g)) {
      citations += 1;
      const number = Number(match[1]);
      if (present.has(number)) continue;
      const where = relative(repoRoot, file);
      if (!dangling.has(number)) dangling.set(number, new Set());
      dangling.get(number).add(where);
    }
  }

  // If the scan stops finding citations, it has stopped testing anything. This is
  // the half of the guard that fails when the guard breaks rather than when the
  // tree does — the shape `no_unrouted_strings` uses for the same reason.
  assert.ok(
    citations > 500,
    `only ${citations} docs/NNN citations found across the tree; this repository cites ` +
      `design docs from code constantly, so a count this low means the walk is broken`,
  );

  const report = [...dangling.entries()]
    .sort(([a], [b]) => a - b)
    .map(
      ([number, files]) =>
        `  docs/${number} — cited in ${[...files].sort().join(", ")}`,
    )
    .join("\n");
  assert.equal(
    dangling.size,
    0,
    `${dangling.size} citation(s) name a document that does not exist. Either write the ` +
      `document or remove the reference — a comment pointing at a missing design doc is ` +
      `the docs/99 §9.2 defect class:\n${report}`,
  );
});
