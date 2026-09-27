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
//   * A citation naming a doc that exists but is about something ELSE — see
//     above; nothing mechanical reads intent.
//
// AMBIGUOUS NUMBERS ARE NOW ENFORCED, by the second test below. When this guard
// was written `docs/` held two documents numbered 83, 86, 94, 95, 105 and 114,
// and it deliberately accepted them, on the reasoning that renaming a document
// was a separate change and a guard that must be suppressed to land is a guard
// nobody trusts. That separate change has now happened: seven collisions were
// resolved (the six above plus three documents all numbered 128), the minority
// side of each pair moved to 129-138, and the number stayed with whichever
// document the citations in the tree actually meant — 129 of 129 bare `docs/105`
// citations mean the audit tracker, 54 of 54 `docs/114` mean the spell-check
// design, all 16 `docs/94` mean the oracle harness. So the exception is gone and
// the rule is a build failure.
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
    for (const match of text.matchAll(/docs\/(\d{1,3})(?!\d)/g)) {
      // An external URL that happens to contain `/docs/` is not a citation of
      // ours. This guard read the dated path segment in a
      // `modelcontextprotocol.io/docs/<date>/...` link as a three-digit document
      // number and failed on a document that cites the MCP spec correctly. The
      // `(?!\d)` above is the general half of the fix — a four-digit path segment
      // is not a three-digit doc number — and this is the other half, for a URL
      // that really does contain a short number. (Written without an example of
      // the bad citation on purpose: this guard scans its own source too, so an
      // illustrative `docs/NNN` in a comment here is itself a dangling one.)
      const token = /(\S*)$/.exec(text.slice(Math.max(0, match.index - 200), match.index))[1];
      if (token.includes("://")) continue;
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

// A document number is an ADDRESS, so two documents may not share one.
//
// The test above checks that a citation resolves to something. This one checks
// that it resolves to exactly ONE thing, which is the half that was missing and
// the reason it was missing is worth writing down: nothing checked, so seven
// collisions accumulated without anybody deciding to create one. Three documents
// ended up numbered 128 in a single day — one of them cited 33 times from Rust,
// one cited twice from the webapp, one cited nowhere — and a reader following
// `docs/128` §4 had no way to know which file that meant.
//
// It is not a cosmetic problem. `docs/105` was cited 129 times and resolved to
// both the audit tracker and a cancelled chrome design; `crates/.../lib.rs`
// pointed at "`docs/83`" meaning review projection while sixteen other citations
// of `docs/83` meant SDK packaging. A wrong address is worse than a missing one,
// because the reader finds a real file and believes it.
test("no two documents claim the same number", () => {
  const byNumber = new Map();
  for (const name of readdirSync(join(repoRoot, "docs"))) {
    const match = /^(\d+)-/.exec(name);
    if (!match) continue; // `PHASE-1A-…-TRACKER.md` is deliberately unnumbered.
    const number = Number(match[1]);
    if (!byNumber.has(number)) byNumber.set(number, []);
    byNumber.get(number).push(name);
  }

  // The same self-check the citation test carries: a walk that finds nothing
  // would otherwise pass by failing to look.
  assert.ok(
    byNumber.size > 100,
    `only ${byNumber.size} numbered docs found — the scan is looking in the wrong place`,
  );

  const collisions = [...byNumber.entries()]
    .filter(([, names]) => names.length > 1)
    .sort(([a], [b]) => a - b);
  assert.deepEqual(
    collisions.map(([number, names]) => `docs/${number}: ${names.sort().join(" + ")}`),
    [],
    "two or more documents claim the same number, so every `docs/NNN` citation of it is " +
      "ambiguous. Renumber the one the tree's citations do NOT mean — count them first, " +
      "because the number belongs to whichever document readers are already pointing at",
  );
});

// A document's own heading must agree with its number.
//
// The renaming that closed the collisions above moved ten files and left every
// one of their H1s stating the OLD number, so `docs/137` opened with
// "# 105 — Dual chrome" — the same wrong-address defect, inside the change that
// was fixing it. Nine of the ten, caught by looking rather than by any guard.
//
// This is the cheap half of "does a citation mean what it says": a reader who
// follows `docs/137` and finds a heading numbered 105 cannot tell whether they
// have the right file, the wrong file, or a file that was renamed and not
// finished. Not every document uses the `# NNN — Title` form, and one that does
// not is left alone; the assertion is only that a document which states a number
// states its OWN.
test("a document's heading states its own number", () => {
  const wrong = [];
  let checked = 0;
  for (const name of readdirSync(join(repoRoot, "docs")).sort()) {
    const file = /^(\d+)-.*\.md$/.exec(name);
    if (!file) continue;
    const first = readFileSync(join(repoRoot, "docs", name), "utf8").split("\n", 1)[0];
    const heading = /^#\s+(\d+)\s*[—–-]\s*(.*)$/.exec(first);
    if (!heading) continue; // A document that does not number its heading is fine.
    checked += 1;
    if (heading[1] !== file[1]) wrong.push(`${name} opens "# ${heading[1]} — ${heading[2]}"`);
  }

  // The half that fails when the guard breaks rather than when the tree does.
  // 68 documents use the `# NNN — Title` form today. The floor is set BELOW that
  // measurement rather than at a round guess: my first attempt asserted 80 and
  // failed on arrival, which is the cheap version of the mistake this file keeps
  // finding in other guards — a number chosen for how it reads instead of for
  // what was counted.
  assert.ok(
    checked > 60,
    `only ${checked} numbered headings found — the scan is looking in the wrong place, and ` +
      "a guard that finds no headings passes by failing to look",
  );
  assert.deepEqual(
    wrong,
    [],
    "a document's heading states a different number than its filename. Renaming a document " +
      "means renaming it in its own first line too, or a reader cannot tell whether they have " +
      "the right file",
  );
});

// An ADR number is an address too, and nothing was checking it.
//
// Two documents' decisions were published as ADR-034 and ADR-035. A branch
// developed in parallel drafted its own ADR-034, and when the two merged, git
// resolved the conflict by taking one side: `main` ended up with the register
// running 032, 033, 034 — the SDK host contract — and **both experimental ADRs
// gone**, while five documents went on citing ADR-034 and ADR-035 meaning the
// decisions that had been deleted. A reader following those citations found a real
// section about something else.
//
// This is the document-number collision one register down, and it is worse in one
// respect: a document that disappears leaves a dangling path a guard can see,
// whereas a deleted ADR section leaves the citations resolving to the WRONG
// decision, silently. So: numbers unique, and contiguous from 1, because a gap in
// this register is how a deletion looks.
test("every ADR number is unique, and the register has no gaps", () => {
  const register = readFileSync(join(repoRoot, "docs", "08-ADR-REGISTER.md"), "utf8");
  const numbers = [...register.matchAll(/^##\s+ADR-(\d+)\s*—/gm)].map((m) => Number(m[1]));

  // The half that fails when the guard breaks rather than when the tree does.
  assert.ok(
    numbers.length > 25,
    `only ${numbers.length} ADR headings found — the scan is looking in the wrong place, and a ` +
      "guard that finds no ADRs passes by failing to look",
  );

  const seen = new Set();
  const duplicated = numbers.filter((n) => (seen.has(n) ? true : (seen.add(n), false)));
  assert.deepEqual(
    [...new Set(duplicated)].map((n) => `ADR-${String(n).padStart(3, "0")}`),
    [],
    "two decisions claim the same ADR number, so every citation of it is ambiguous. Give the " +
      "newer one the next free number and update its citations",
  );

  // Contiguous from 1. A gap means a section was dropped — which is exactly what
  // a merge did here — and the citations that pointed at it now point at nothing
  // or, worse, at whatever took the number.
  const sorted = [...seen].sort((a, b) => a - b);
  const missing = [];
  for (let n = 1; n <= sorted[sorted.length - 1]; n += 1) if (!seen.has(n)) missing.push(n);
  assert.deepEqual(
    missing.map((n) => `ADR-${String(n).padStart(3, "0")}`),
    [],
    "the ADR register skips a number. If a decision was superseded it keeps its section and " +
      "says so; if a merge dropped one, restore it — a gap is how a deleted decision looks",
  );
});
