import { test, expect } from "./fixtures.mjs";

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
  await expect(rowHeaders).toHaveCount(19);

  // Honest cells are actually present in the DOM (not just in the data file).
  await expect(rowHeaders.filter({ hasText: "Images & inline drawings" })).toHaveCount(1);
  await expect(rowHeaders.filter({ hasText: "Math (OMML)" })).toHaveCount(1);
  await expect(rowHeaders.filter({ hasText: "Charts" })).toHaveCount(1);

  // The Images row's Editable cell must read "Not yet" (no insert/edit).
  const imagesRow = table.locator("tbody tr", { has: page.getByRole("rowheader", { name: /Images & inline drawings/ }) });
  await expect(imagesRow.locator("td.cell").nth(2)).toContainText("Not yet");

  await expect(consoleErrors).toEqual([]);
});
