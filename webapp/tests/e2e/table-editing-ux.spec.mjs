import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

async function insertTwoByTwoTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
}

async function tablePropertyValues(page) {
  return page.evaluate(() => ({
    alignment:
      document.querySelector('#tableAlign button[aria-pressed="true"]')?.dataset.talign,
    width: document.getElementById("tableWidth").value,
    indent: document.getElementById("tableIndent").value,
    fixed: document.getElementById("tableFixedLayout").checked,
    header: document.getElementById("tableHeaderRow").checked,
    columnWidth: document.getElementById("tableColumnWidth").value,
    rowHeight: document.getElementById("tableRowHeight").value,
    rowRule: document.getElementById("tableRowHeightRule").value,
    cellMargin: document.getElementById("tableCellMargin").value,
    cellSpacing: document.getElementById("tableCellSpacing").value,
    caption: document.getElementById("tableCaption").value,
    description: document.getElementById("tableDescription").value,
  }));
}

test("the contextual Table ribbon exposes complete core commands and bounded formatting", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);

  const ribbon = page.locator(".table-ribbon");
  await expect(ribbon).toBeVisible();
  await expect(ribbon.locator("[data-table-select]")).toHaveCount(3);
  await expect(ribbon.locator("[data-table-action]")).toHaveCount(7);
  for (const action of [
    "insert-row-above",
    "insert-row-below",
    "insert-column-left",
    "insert-column-right",
    "delete-row",
    "delete-column",
    "delete-table",
  ]) {
    await expect(ribbon.locator(`[data-table-action="${action}"]`)).toBeEnabled();
  }
  await expect(ribbon.locator('[data-table-distribute="columns"]')).toBeEnabled();
  await expect(ribbon.locator('[data-table-distribute="rows"]')).toBeDisabled();
  await expect(ribbon.locator('[data-table-sort="ascending"]')).toBeEnabled();
  await expect(ribbon.locator('[data-table-sort="descending"]')).toBeEnabled();
  await ribbon.locator('[data-table-distribute="columns"]').click();
  await expect(ribbon.locator('[data-table-distribute="columns"]')).toBeEnabled();
  await expect(page.locator("#splitCellBtn")).toBeEnabled();
  await expect(page.locator("#mergeCellsBtn")).toBeDisabled();
  await expect(page.locator("#tableContext")).toContainText("2×2 table");
  await page.locator("#tableStyleBtn").click();
  await expect(page.locator("#tableStyleMenu")).toBeVisible();
  await expect(page.locator("#tableStyleMenu [data-table-style]")).toHaveCount(1);
  await page.locator("#tableStyleMenu [data-table-style]").click();

  await ribbon.locator('[data-table-action="insert-row-below"]').click();
  await expect(page.locator("#tableContext")).toContainText("3×2 table");
  await ribbon.locator('[data-table-action="insert-column-right"]').click();
  await expect(page.locator("#tableContext")).toContainText("3×3 table");

  await ribbon.locator('[data-table-select="row"]').click();
  await expect(page.locator(".table-cell-selection")).toHaveCount(3);
  await expect(page.locator("#mergeCellsBtn")).toBeEnabled();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");
  await page.locator("#splitCellBtn").click();
  await expect(page.locator("#splitCellDialog")).toBeVisible();
  await page.locator("#splitCellRows").fill("1");
  await page.locator("#splitCellColumns").fill("3");
  await page.locator("#splitCellConfirm").click();
  await expect(page.locator("#tableContext")).not.toContainText("merged/spanned");
  await expect(
    ribbon.locator('[data-table-action="insert-column-right"]'),
  ).toBeEnabled();
  const keyboardInsert = ribbon.locator('[data-table-action="insert-row-above"]');
  await keyboardInsert.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#tableContext")).toContainText("4×3 table");

  await page.locator("#tableBtn").click();
  const formatMenu = page.locator("#tableMenu");
  await expect(formatMenu).toBeVisible();
  const bounds = await formatMenu.boundingBox();
  const viewport = page.viewportSize();
  expect(bounds.y).toBeGreaterThanOrEqual(8);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height - 8);
  expect(bounds.height).toBeLessThan(500);
  await expect(formatMenu.locator("[data-table-action]")).toHaveCount(0);
  await expect(formatMenu.locator("#cellVAlign")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("table properties commit live, undo per interaction, and restore focus", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);

  const trigger = page.locator("#tablePropertiesBtn");
  const panel = page.locator("#tablePropertiesPanel");
  await trigger.click();
  await expect(panel).toBeVisible();
  await expect(page.locator("#tablePropertiesContext")).toContainText("2×2 table");
  await expect(page.locator("#viewport")).toBeVisible();
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
  await expect(page.locator("#tablePropertiesApply")).toHaveCount(0);
  await expect(page.locator("#tablePropertiesReset")).toHaveCount(0);
  const surface = await panel.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      radius: style.borderRadius,
      borders: [
        style.borderTopWidth,
        style.borderRightWidth,
        style.borderBottomWidth,
        style.borderLeftWidth,
      ],
      shadow: style.boxShadow,
    };
  });
  expect(surface.radius).toBe("10px");
  expect(surface.borders).toEqual(["1px", "1px", "1px", "1px"]);
  expect(surface.shadow).not.toBe("none");
  const original = await tablePropertyValues(page);

  await page.locator('#tableAlign button[data-talign="center"]').click();
  await page.locator("#tableWidth").fill("4");
  await page.locator("#tableWidth").press("Tab");
  await page.locator("#tableIndent").fill("0.25");
  await page.locator("#tableIndent").press("Tab");
  await page.locator("#tableFixedLayout").check();
  await page.locator("#tableHeaderRow").check();
  await page.locator("#tableColumnWidth").fill("1.25");
  await page.locator("#tableColumnWidth").press("Tab");
  await page.locator("#tableRowHeightRule").selectOption("exact");
  await page.locator("#tableRowHeight").fill("0.5");
  await page.locator("#tableRowHeight").press("Tab");
  await page.locator("#tableCellMargin").fill("0.1");
  await page.locator("#tableCellMargin").press("Tab");
  await page.locator("#tableCellSpacing").fill("0.06");
  await page.locator("#tableCellSpacing").press("Tab");
  await page.locator("#tableCaption").fill("Sales");
  await page.locator("#tableCaption").press("Tab");
  await page.locator("#tableDescription").fill("Quarterly sales table");
  await page.locator("#tableDescription").press("Tab");
  await expect(panel).toBeVisible();
  expect(await tablePropertyValues(page)).toEqual({
    alignment: "center",
    width: "4",
    indent: "0.25",
    fixed: true,
    header: true,
    columnWidth: "1.25",
    rowHeight: "0.5",
    rowRule: "exact",
    cellMargin: "0.1",
    cellSpacing: "0.06",
    caption: "Sales",
    description: "Quarterly sales table",
  });

  // Each completed control interaction is its own undo action. The last undo
  // clears only cell spacing; the earlier live changes remain committed.
  await page.locator('[data-tab="home"]').click();
  await page.locator("#undoBtn").click();
  await page.locator("#tabTable").click();
  expect(await tablePropertyValues(page)).toEqual({
    alignment: "center",
    width: "4",
    indent: "0.25",
    fixed: true,
    header: true,
    columnWidth: "1.25",
    rowHeight: "0.5",
    rowRule: "exact",
    cellMargin: "0.1",
    cellSpacing: "0.06",
    caption: "Sales",
    description: "",
  });

  await page.locator("#tableWidth").focus();
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(trigger).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("live table properties remain reachable on a narrow viewport", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await insertTwoByTwoTable(page);
  // The Table ribbon's Properties group collapses into the "⋯" overflow menu at
  // this narrow width (docs/64 — no horizontal scrollbar); open it to reach it.
  const tableProps = page.locator("#tablePropertiesBtn");
  if (!(await tableProps.isVisible())) await page.locator("#ribbonOverflowBtn").click();
  await tableProps.click();

  const panel = page.locator("#tablePropertiesPanel");
  await expect(panel).toBeVisible();
  await expect(panel.locator(".table-properties-actions")).toHaveCount(0);
  await expect(page.locator("#tablePropertiesClose")).toBeVisible();
  await expect(page.locator("#viewport")).toBeVisible();
  await expect(page.locator("body")).not.toHaveClass(/modal-open/);
  const bounds = await panel.boundingBox();
  expect(bounds.x).toBeGreaterThanOrEqual(34);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(382);
  expect(bounds.y).toBeGreaterThanOrEqual(0);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(692);
  expect(bounds.width).toBeLessThan(390);
  await expect(panel).toHaveCSS("border-radius", "10px");
  await expect(panel).not.toHaveCSS("box-shadow", "none");
  expect(consoleErrors).toEqual([]);
});

// Reads the model-synced off-screen accessibility tree to return the rows of
// the table that contains `marker`, each row as an array of cell texts. This is
// the reliable way to observe table content because the document is painted to
// an opaque canvas.
async function a11yTableRows(page, marker) {
  return page.evaluate((needle) => {
    const tables = [...document.querySelectorAll("#a11yDocument table")];
    const target = tables.find((t) => t.textContent.includes(needle));
    if (!target) return null;
    return [...target.querySelectorAll("tr")].map((tr) =>
      [...tr.querySelectorAll("td")].map((td) => td.textContent),
    );
  }, marker);
}

test("table sort keys off the column containing the caret, not always the first", async ({
  page,
  consoleErrors,
}) => {
  // Insert a 2×2 table; the caret is left in its first cell with the editor
  // surface focused, so typing lands directly in the cells (no extra click that
  // could miss the cell on the opaque canvas).
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();

  // Fill the 2×2 grid in Tab (row-major) order so the first column's order
  // (ALPHA, BETA) is the OPPOSITE of the second column's order (TWO, ONE):
  //   row 0 = [ALPHA, TWO]
  //   row 1 = [BETA,  ONE]
  // A first-column ascending sort would leave the rows unchanged (ALPHA < BETA),
  // so only a correct second-column sort can reverse them.
  await page.keyboard.type("ALPHA");
  await page.keyboard.press("Tab");
  await page.keyboard.type("TWO");
  await page.keyboard.press("Tab");
  await page.keyboard.type("BETA");
  await page.keyboard.press("Tab");
  await page.keyboard.type("ONE");

  await expect
    .poll(() => a11yTableRows(page, "ALPHA"))
    .toEqual([
      ["ALPHA", "TWO"],
      ["BETA", "ONE"],
    ]);

  // The caret is now in the SECOND column (the cell holding "ONE"). Sorting
  // ascending from the ribbon must key off that column, moving ONE above TWO and
  // carrying each row's first column with it.
  const ribbon = page.locator(".table-ribbon");
  await page.locator("#tabTable").click();
  await ribbon.locator('[data-table-sort="ascending"]').click();

  await expect
    .poll(() => a11yTableRows(page, "ALPHA"))
    .toEqual([
      ["BETA", "ONE"],
      ["ALPHA", "TWO"],
    ]);

  expect(consoleErrors).toEqual([]);
});

test("split an ordinary cell into a rows x columns grid (Word Split Cells)", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);
  // Caret sits in a plain (unmerged) cell. Split it into 2 rows x 2 columns —
  // the case the old column-only split could not do.
  await expect(page.locator("#splitCellBtn")).toBeEnabled();
  await page.locator("#splitCellBtn").click();
  await expect(page.locator("#splitCellDialog")).toBeVisible();
  await page.locator("#splitCellRows").fill("2");
  await page.locator("#splitCellColumns").fill("2");
  await page.locator("#splitCellConfirm").click();

  // The dialog closes, the document is dirty, and the split committed as a single
  // undoable table action (the Undo control is enabled and labeled for it). The
  // engine-level undo/round-trip of the grid split is covered by the Rust tests.
  await expect(page.locator("#splitCellDialog")).toBeHidden();
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Undo .*Table/i);

  expect(consoleErrors).toEqual([]);
});
