// Overlay chrome is read in one page task, across the whole browser suite.
//
// `.overlay` children — the caret, the object outline, the grips — are torn down
// and redrawn on every overlay paint, and paints land on their own schedule
// after an interaction (proofing results, a font arriving, a relayout; one click
// on the demo picture paints its outline five times). `locator(...).evaluate()`
// resolves the element in one round trip and evaluates against it in a second,
// so a repaint in between hands the callback the REMOVED node: its rect is all
// zeros and its computed transform is empty.
//
// That is what "object-geometry: an object resizes by 16px in Suggesting mode,
// CI only" was. The 16x16 demo picture never moved; one of the two reads
// measured a detached outline as 0. It was quarantined once, lifted with a
// diagnostic, and came back 3/3 on CI when a layout change shifted when
// proofing repainted. Fixing that one spec would leave the same race in every
// other spec that reads the caret or an outline this way, so this test fails
// the build if one comes back.
//
// Use `overlayBox(page, selector)` from `fixtures.mjs` instead. `evaluateAll`
// is not banned: it queries and evaluates in the same page call.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const E2E = join(dirname(fileURLToPath(import.meta.url)), "e2e");

/** `locator(".overlay …")`, optionally `.first()` / `.nth(n)` / `.last()`, then
 *  `.evaluate(` — across line breaks, since prettier splits the chain. */
const OVERLAY_EVALUATE =
  /\.locator\(\s*(["'`])\.overlay\b[^"'`]*\1\s*\)(?:\s*\.(?:first|last)\(\s*\)|\s*\.nth\(\s*\d+\s*\))?\s*\.evaluate\(/g;

/** Every offending chain in `text`, as `line: source`. Exported for the self-test. */
export function overlayEvaluateOffenders(text) {
  const offenders = [];
  for (const match of text.matchAll(OVERLAY_EVALUATE)) {
    const line = text.slice(0, match.index).split("\n").length;
    offenders.push(`${line}: ${match[0].replace(/\s+/g, " ")}`);
  }
  return offenders;
}

test("the pattern recognises the racy read in each shape the suite used", () => {
  for (const sample of [
    'page.locator(".overlay .caret").evaluate((el) => el)',
    'page.locator(".overlay .object-outline").first().evaluate((el) => el)',
    'page\n    .locator(".overlay .object-outline")\n    .first()\n    .evaluate((element) => element)',
    "page.locator('.overlay .object-handle').nth(2).evaluate((el) => el)",
  ]) {
    assert.equal(overlayEvaluateOffenders(sample).length, 1, sample);
  }
  for (const sample of [
    'page.locator(".overlay .spell-error").evaluateAll((els) => els)',
    'overlayBox(page, ".overlay .object-outline")',
    'page.locator("#viewport").evaluate((v) => v.scrollTop)',
  ]) {
    assert.deepEqual(overlayEvaluateOffenders(sample), [], sample);
  }
});

test("no spec reads overlay chrome through locator.evaluate", () => {
  const offenders = [];
  for (const name of readdirSync(E2E)) {
    if (!name.endsWith(".mjs")) continue;
    const text = readFileSync(join(E2E, name), "utf8");
    for (const offender of overlayEvaluateOffenders(text)) offenders.push(`${name}:${offender}`);
  }
  assert.deepEqual(
    offenders,
    [],
    "these read overlay chrome (redrawn on every overlay paint) in two round trips, " +
      "so a repaint in between measures the removed node as zeros; use " +
      "overlayBox(page, selector) from fixtures.mjs:\n  " +
      offenders.join("\n  "),
  );
});
