import { readFileSync } from "node:fs";

import { test, expect } from "./fixtures.mjs";

/** The construct families the page is built from, read from the committed data
 *  rather than remembered here.
 *
 *  This count used to be the literal `26`, and #584 added a 27th family and
 *  left the number behind — so `main` went red on a spec that was measuring
 *  nothing but whether somebody had remembered to edit two files at once.
 *  `SKILL.md` §9: a published number is DERIVED from a committed artifact. The
 *  assertion worth keeping is that the page renders every family in the data
 *  and invents none, which is what this now says.
 *
 *  Loaded the same way `fidelity_data.test.mjs` does it: `fidelity.js` is a
 *  classic browser script, so it is evaluated with a fake `module` and no
 *  `document`, which runs its data block and skips its DOM render. */
/** `RegExp`-safe form of a family name — several carry `(`, `)` or `&`. */
function escapeForRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function fidelityFamilies() {
  const source = readFileSync(new URL("../../src/fidelity.js", import.meta.url), "utf8");
  const sandbox = { exports: {} };
  new Function("module", source)(sandbox);
  return sandbox.exports.FIDELITY.map((row) => row.family);
}

test("the fidelity matrix page renders an accessible, data-grounded table", async ({ page }) => {
  const consoleErrors = [];
  page.on("console", (m) => { if (m.type() === "error") consoleErrors.push(m.text()); });
  page.on("pageerror", (e) => consoleErrors.push(String(e)));

  await page.goto("/fidelity.html");
  await expect(page).toHaveTitle(/fidelity support matrix/i);

  const table = page.locator("table.fidelity-table");
  await expect(table).toBeVisible();
  // Real semantic table with column headers and row headers (scope) for AT.
  await expect(table.locator('thead th[scope="col"]')).toHaveCount(5);
  const rowHeaders = table.locator('tbody th[scope="row"]');
  const families = fidelityFamilies();
  await expect(rowHeaders).toHaveCount(families.length);
  // …and they are the SAME families, in the data's order. A count alone would
  // pass on a page that rendered the right number of the wrong rows.
  await expect(rowHeaders).toHaveText(families.map((family) => new RegExp(escapeForRegExp(family))));

  // Honest cells are actually present in the DOM (not just in the data file).
  await expect(rowHeaders.filter({ hasText: "Images & inline drawings" })).toHaveCount(1);
  await expect(rowHeaders.filter({ hasText: "Math (OMML)" })).toHaveCount(1);
  await expect(rowHeaders.filter({ hasText: "Charts" })).toHaveCount(1);

  // The Images row's Editable cell reads "Partial": insert, crop, alt text,
  // move/resize/wrap and delete land, but in-place replace, rotation and
  // picture styling do not — the page reflects the data file.
  const imagesRow = table.locator("tbody tr", { has: page.getByRole("rowheader", { name: /Images & inline drawings/ }) });
  await expect(imagesRow.locator("td.cell").nth(2)).toContainText("Partial");

  // Headers & footers are a FULL editing surface now. Pinning it here is what
  // stops the page drifting back to "renders but is not editable", which is what
  // it claimed for months after the capability shipped.
  const runningRow = table.locator("tbody tr", { has: page.getByRole("rowheader", { name: /Headers & footers/ }) });
  await expect(runningRow.locator("td.cell").nth(2)).toContainText("Full");

  // The honesty floor still renders: Math (OMML) stays a read-only "Not yet"
  // editable cell, so the ○ / Not-yet state is exercised on a real row.
  const mathRow = table.locator("tbody tr", { has: page.getByRole("rowheader", { name: /Math \(OMML\)/ }) });
  await expect(mathRow.locator("td.cell").nth(2)).toContainText("Not yet");

  await expect(consoleErrors).toEqual([]);
});
