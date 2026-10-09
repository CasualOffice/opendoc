// Every box read in the browser suite waits for the element it is about to
// measure, enforced across the whole suite.
//
// `boundingBox()` returns `null` for an element that has no box yet, so a spec
// that reads it before layout has happened does not fail with "the sheet never
// appeared" — it fails with `TypeError: Cannot read properties of null (reading
// 'x')`, which reads like a broken canvas and sent me looking at the renderer.
// That is what turned `main` red on #782: `review-comment-actions.spec.mjs`
// waited for `.page-wrap` to EXIST and then measured `.page-wrap .page`, a
// different element. The wait proved the wrapper was there and said nothing
// about the sheet inside it, so under four-way load the box came back null —
// one spec failed and two more in the same file went flaky.
//
// `stableBox(locator)` in `fixtures.mjs` polls the element it is handed until
// that element has a real width, so the wait and the measurement are the same
// element by construction and cannot drift apart again. It already existed, and
// its own doc comment already named this error string — 149 call sites used it
// and 118 did not. A helper that half the suite reaches is the recurring
// single-surface defect (`SKILL.md` §10) in test code.
//
// So: a raw `.boundingBox()` in a spec fails the build. The exception list is a
// ratchet — it may shrink, never grow.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const E2E = join(dirname(fileURLToPath(import.meta.url)), "e2e");

/** Sites that read a box WITHOUT wanting the wait, each with its reason. A spec
 *  may only be here because a missing box is the expected answer.
 *
 *  Empty: its last entry, `caret-alignment.spec.mjs`, now reads the caret's box
 *  in one page task — its locator round trip raced the overlay repaint and
 *  reported "no caret" over a painted one. */
const TOLERATES_NO_BOX = new Map([]);

const specs = () => readdirSync(E2E).filter((n) => n.endsWith(".spec.mjs"));

/** The names a module imports, from BOTH import shapes.
 *
 *  This repo writes `import { a, b } from "x";` on one line and, when prettier
 *  breaks it, as a block with one name per line. A reader that only looks at
 *  lines starting with `import` sees the first shape and is blind to the second
 *  — which is exactly the bug that produced this change: the codemod behind it
 *  used that test, so eleven specs got a rewritten call site and no import, and
 *  would have thrown `stableBox is not defined` on whichever shard ran them. */
function importedNames(src) {
  const names = [];
  for (const m of src.matchAll(/^import\s+\{([\s\S]*?)\}\s+from\s+"[^"]+";/gm)) {
    for (const n of m[1].split(",")) {
      const name = n.trim().replace(/\s+as\s+\w+$/, "");
      if (name) names.push(name);
    }
  }
  return names;
}

test("no spec reads boundingBox() directly — the wait and the measurement must be one element", () => {
  const offenders = [];
  for (const name of specs()) {
    const src = readFileSync(join(E2E, name), "utf8");
    const lines = src.split("\n");
    for (let i = 0; i < lines.length; i++) {
      if (!/\.boundingBox\(\)/.test(lines[i])) continue;
      if (TOLERATES_NO_BOX.has(name)) continue;
      offenders.push(`${name}:${i + 1}  ${lines[i].trim()}`);
    }
  }
  assert.deepEqual(
    offenders,
    [],
    `${offenders.length} raw box read(s). Use \`stableBox(locator)\` from ` +
      `fixtures.mjs, which waits for the element it measures:\n  ${offenders.join("\n  ")}`,
  );
});

test("the exception list is a ratchet: every entry still exists and still tolerates no box", () => {
  // An exception that has been fixed must leave the list, or the list stops
  // describing the suite and starts excusing it.
  for (const [name, reason] of TOLERATES_NO_BOX) {
    const path = join(E2E, name);
    assert.ok(specs().includes(name), `${name} is listed as an exception but no longer exists`);
    const src = readFileSync(path, "utf8");
    assert.match(
      src,
      /\.boundingBox\(\)/,
      `${name} no longer reads a box directly — remove it from TOLERATES_NO_BOX`,
    );
    assert.match(
      src,
      /\.catch\(/,
      `${name} is excused because it handles a missing box (${reason}) — but it no ` +
        `longer catches, so it is not handling one`,
    );
  }
});

test("stableBox is the one implementation, and it lives in the shared fixture", () => {
  // The guard above is only worth anything if the thing it redirects to exists
  // and is reachable from every spec. Pinning both halves here means deleting
  // the helper fails this test rather than silently making the suite raw again.
  const fixture = readFileSync(join(E2E, "fixtures.mjs"), "utf8");
  assert.match(fixture, /export async function stableBox\(/, "fixtures.mjs must export stableBox");
  assert.match(
    fixture,
    /\.boundingBox\(\)/,
    "stableBox is the only place allowed to read a box directly, so the read must be here",
  );

  // ONE mechanism, not two. A spec that rolls its own retry-until-it-has-a-box
  // is the same defect wearing a local name, and the next person to move the
  // wait has two places to find. (The first version of this assertion pinned a
  // COUNT of specs using the helper — a circumstance that a merge changes and
  // that says nothing about whether a second mechanism exists.)
  const rivals = [];
  for (const name of specs()) {
    const src = readFileSync(join(E2E, name), "utf8");
    if (/(?:function|const)\s+\w*[sS]tableBox\w*\s*[=(]/.test(src)) rivals.push(name);
  }
  assert.deepEqual(rivals, [], `spec(s) define a second box-waiting helper: ${rivals.join(", ")}`);
});

test("every spec that calls stableBox imports it", () => {
  // A codemod that rewrites call sites and forgets the import produces a
  // `stableBox is not defined` at run time, in a spec that may not run on every
  // shard. This is the same shape as the import that dropped two siblings and
  // broke every click in #774, and it is cheap to pin.
  const missing = [];
  for (const name of specs()) {
    const src = readFileSync(join(E2E, name), "utf8");
    if (!/\bstableBox\(/.test(src)) continue;
    if (!importedNames(src).includes("stableBox")) missing.push(name);
  }
  assert.deepEqual(missing, [], `spec(s) use stableBox without importing it: ${missing.join(", ")}`);
});

test("no spec imports the same name twice", () => {
  // The dedupe bug in the codemod that produced this change: six files ended up
  // with `stableBox, stableBox` and failed to parse. `node --check` caught them,
  // but only because every file happened to be checked — this asserts it.
  const dupes = [];
  for (const name of specs()) {
    const seen = new Set();
    for (const n of importedNames(readFileSync(join(E2E, name), "utf8"))) {
      if (seen.has(n)) dupes.push(`${name}: ${n}`);
      seen.add(n);
    }
  }
  assert.deepEqual(dupes, [], `duplicate import name(s): ${dupes.join(", ")}`);
});
