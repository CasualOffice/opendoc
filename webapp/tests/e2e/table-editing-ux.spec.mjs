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
  await expect(page.locator("#splitCellBtn")).toBeEnabled();
  await expect(page.locator("#mergeCellsBtn")).toBeDisabled();
  await expect(page.locator("#tableContext")).toContainText("2×2 table");

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
    cellSpacing: original.cellSpacing,
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
  await page.locator("#tablePropertiesBtn").click();

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
