// No browser spec may assert that a proofing mark is ABSENT with a bare zero
// count (`docs/109` HF-417).
//
// `expect(squiggle).toHaveCount(0)` retries until the first moment the count is
// zero, and the overlay reaches zero three ways that are not the feature
// working: before the checker has run, between two repaints (it is rebuilt from
// nothing each time), and whenever the caret is inside the word (`paint` never
// marks the word under the caret, and the right-click that opens a suggestion
// menu puts the caret there). `text-language.spec.mjs` found the first and the
// third; an audit then found eleven more of the same in `spell-check.spec.mjs`.
// Fixing them one by one is how the class comes back, so this fails the build
// if a spec writes one again.
//
// Use `expectMarkAbsent` (absence read beside a positive control, in one
// overlay snapshot), `expectNoMarksWithProofingOff` or
// `expectNoMarksWithCheckerStatus` from `e2e/proofing-marks.mjs`.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const E2E = join(dirname(fileURLToPath(import.meta.url)), "e2e");

/** What names a proofing mark in a spec: the overlay classes, the spec-local
 *  `marker(`/`grammarMark(` builders, the shared ones, or a variable a spec
 *  conventionally calls `squiggle`. */
const MARK = /spell-error|grammar-error|\bmarker\(|\bgrammarMark\(|\bspellMark\(|\bgrammarMarkFor\(|\bsquiggle\b|\bdoubled\b|\binvented\b|ANY_SPELL_MARK|ANY_GRAMMAR_MARK|\bGRAMMAR\b/;
/** A zero count, however it is spelled across lines. */
const ZERO = /\.toHaveCount\(\s*0\s*[,)]/;

/** One entry per `;`-terminated statement, with the line it starts on. */
function statements(source) {
  const out = [];
  let start = 0;
  let line = 1;
  let startLine = 1;
  for (let i = 0; i < source.length; i += 1) {
    if (source[i] === "\n") line += 1;
    if (source[i] === ";") {
      out.push({ text: source.slice(start, i + 1), line: startLine });
      start = i + 1;
      startLine = line;
    }
  }
  return out;
}

/** The offending statements of one spec's source. Exported shape for the
 *  self-test below. */
function offences(source) {
  return statements(source)
    .filter(({ text }) => ZERO.test(text) && MARK.test(text))
    .map(({ text, line }) => `${line}: ${text.trim().replace(/\s+/g, " ")}`);
}

test("no spec asserts a proofing mark is absent with a bare zero count", () => {
  const specs = readdirSync(E2E).filter((name) => name.endsWith(".spec.mjs"));
  assert.ok(specs.length > 50, `expected to read the browser specs, found ${specs.length}`);
  const found = [];
  for (const name of specs) {
    for (const offence of offences(readFileSync(join(E2E, name), "utf8"))) {
      found.push(`${name}:${offence}`);
    }
  }
  assert.deepEqual(
    found,
    [],
    "a zero count of proofing marks passes before the checker runs, between repaints and " +
      "with the caret in the word: use expectMarkAbsent / expectNoMarksWithProofingOff / " +
      "expectNoMarksWithCheckerStatus from e2e/proofing-marks.mjs",
  );
});

// The guard has to recognise the shapes the audit actually found, or it reads
// nothing and passes for that reason.
test("the guard recognises every shape the audit fixed, and not a non-zero count", () => {
  const caught = [
    'await expect(page.locator(".overlay .spell-error")).toHaveCount(0);',
    "await expect(page.locator(marker(typo))).toHaveCount(0, { timeout: 20_000 });",
    "await expect(squiggle).toHaveCount(0, { timeout: 20_000 });",
    'await expect(\n  page.locator(".overlay .spell-error"),\n  "message",\n).toHaveCount(0);',
    'await expect(page.locator(marker("Qzxworden"))).toHaveCount(\n  0,\n  "reason",\n);',
    'await expect(page.locator(grammarMark("doubled-word"))).toHaveCount(0, { timeout: 20_000 });',
  ];
  for (const source of caught) assert.equal(offences(source).length, 1, source);
  const allowed = [
    "await expect(squiggle).toHaveCount(1, { timeout: 20_000 });",
    'await expect(page.locator("#dialog")).toHaveCount(0);',
    "await expect(page.locator(grammarMark(rule))).toHaveCount(before - 1);",
  ];
  for (const source of allowed) assert.deepEqual(offences(source), [], source);
});
