// The editing-surface focus contract, enforced across the whole browser suite.
//
// `#pages` paints pixels and cannot own text input: a non-editable element
// raises no soft keyboard on touch and fires no composition events, so focus is
// owned by the editable proxy `#editorTextInput` (docs/105 UX-001).
//
// Landing that change broke six specs, and I found them one CI failure at a
// time because each asserted the MECHANISM ("`#pages` is focused") rather than
// the GUARANTEE ("the editing surface is focused"). Patching them individually
// is how the same class comes back the next time focus ownership moves — so
// this test fails the build if any spec names the element directly again.
//
// Use `expectEditorFocused(page)` from `fixtures.mjs` instead.
import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const E2E = join(dirname(fileURLToPath(import.meta.url)), "e2e");

/** Assertions that pin focus to a specific element id instead of asking whether
 *  the editing surface has focus. */
const BANNED = [
  // expect(page.locator("#pages")).toBeFocused()
  /locator\(\s*["']#(?:pages|editorTextInput)["']\s*\)\s*\)\s*\.toBeFocused\(/,
  // expect(await page.evaluate(() => document.activeElement?.id)).toBe("pages")
  /\.toBe\(\s*["'](?:pages|editorTextInput)["']\s*\)/,
];

test("no spec asserts focus on a named editing-surface element", () => {
  const offenders = [];
  for (const name of readdirSync(E2E)) {
    if (!name.endsWith(".spec.mjs")) continue;
    const text = readFileSync(join(E2E, name), "utf8");
    text.split("\n").forEach((line, index) => {
      // The helper's own definition and doc comment legitimately mention both.
      if (line.includes("expectEditorFocused")) return;
      if (BANNED.some((pattern) => pattern.test(line))) {
        offenders.push(`${name}:${index + 1}  ${line.trim()}`);
      }
    });
  }
  assert.deepEqual(
    offenders,
    [],
    "these assertions pin focus to a named element; use expectEditorFocused(page) " +
      "from fixtures.mjs so the contract survives focus ownership moving:\n  " +
      offenders.join("\n  "),
  );
});
