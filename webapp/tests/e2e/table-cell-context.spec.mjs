// Table cells participate in the same model-owned editing contract as every
// other text container. These tests use the engine-drawn active-cell outline as
// geometry, so pointer assertions do not guess where an opaque canvas laid out
// a regular or merged cell (docs/58, P1G-CONTEXT-04).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
} from "./fixtures.mjs";

async function insertTwoByTwo(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator(".cell-outline")).toBeVisible();
}

async function tableRows(page) {
  return page.evaluate(() => {
    const table = document.querySelector("#a11yDocument table");
    if (!table) return null;
    return [...table.querySelectorAll("tr")].map((row) =>
      [...row.querySelectorAll("td")].map((cell) => cell.textContent),
    );
  });
}

async function copySelection(page) {
  return page.evaluate(() => {
    const data = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
    return data.getData("text/plain");
  });
}

async function activeCellBox(page) {
  const outline = page.locator(".cell-outline");
  await expect(outline).toBeVisible();
  const box = await outline.boundingBox();
  expect(box).not.toBeNull();
  return box;
}

async function clickEmptySide(page, box) {
  await page.mouse.click(box.x + box.width * 0.85, box.y + box.height * 0.75);
  await expect(page.locator(".cell-outline")).toBeVisible();
}

test("an empty-area click stays in the intended ordinary cell", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwo(page);
  const first = await activeCellBox(page);
  await page.keyboard.type("FIRST");
  await page.keyboard.press("Tab");
  await page.keyboard.type("SECOND");

  await clickEmptySide(page, first);
  const active = await activeCellBox(page);
  expect(Math.abs(active.x - first.x)).toBeLessThan(2);
  expect(Math.abs(active.width - first.width)).toBeLessThan(2);
  await page.keyboard.type("_CELL");

  await expect
    .poll(() => tableRows(page))
    .toEqual([
      ["FIRST_CELL", "SECOND"],
      ["", ""],
    ]);
  expect(consoleErrors).toEqual([]);
});

test("Select All and replacement stay inside one table cell", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwo(page);
  await page.keyboard.type("FIRST");
  await page.keyboard.press("Enter");
  await page.keyboard.type("MORE");
  await page.keyboard.press("Tab");
  await page.keyboard.type("SECOND");
  await page.keyboard.press("Shift+Tab");

  await page.keyboard.press(`${MOD}+a`);
  await expect(page.locator("#status")).toHaveText(
    "Cell contents selected — choose Select All again to select the document",
  );
  await page.keyboard.type("ONLY");

  await expect
    .poll(() => tableRows(page))
    .toEqual([
      ["ONLY", "SECOND"],
      ["", ""],
    ]);
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  expect(consoleErrors).toEqual([]);
});

test("a repeated Select All explicitly escalates from cell to document", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwo(page);
  await page.keyboard.type("CELLONLY");
  await page.keyboard.press("Tab");
  await page.keyboard.type("NEIGHBOR");
  await page.keyboard.press("Shift+Tab");

  await page.keyboard.press(`${MOD}+a`);
  await expect(page.locator("#status")).toContainText("Cell contents selected");
  expect(await copySelection(page)).toBe("CELLONLY");

  await page.keyboard.press(`${MOD}+a`);
  await expect(page.locator("#status")).toHaveText("Document selected");
  const documentText = await copySelection(page);
  expect(documentText).toContain("CELLONLY");
  expect(documentText).toContain("NEIGHBOR");
  expect(documentText.length).toBeGreaterThan("CELLONLY".length);
  expect(consoleErrors).toEqual([]);
});

test("a merged cell owns its full spanned empty area", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwo(page);
  await page.keyboard.type("LEFT");
  await page.keyboard.press("Tab");
  await page.keyboard.type("RIGHT");

  await page.locator("#tabTable").click();
  const ribbon = page.locator(".table-ribbon");
  await ribbon.locator('[data-table-select="row"]').click();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");

  const merged = await activeCellBox(page);
  await clickEmptySide(page, merged);
  const active = await activeCellBox(page);
  expect(Math.abs(active.x - merged.x)).toBeLessThan(2);
  expect(Math.abs(active.width - merged.width)).toBeLessThan(2);
  await page.keyboard.type("_MERGED");

  const rows = await tableRows(page);
  expect(rows).toHaveLength(2);
  expect(rows[0]).toHaveLength(1);
  expect(rows[0][0]).toContain("_MERGED");
  expect(rows[1]).toHaveLength(2);
  expect(consoleErrors).toEqual([]);
});

test("clicking body text exits the active cell without mutating the table", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwo(page);
  await page.keyboard.type("TABLESAFE");
  const rowsBefore = await tableRows(page);
  const cell = await activeCellBox(page);
  const canvas = page.locator(".page-wrap .page").first();
  const pageBox = await canvas.boundingBox();
  expect(pageBox).not.toBeNull();

  // Search below the live table outline for ordinary body text. The active-cell
  // outline disappearing proves the click resolved outside the table; a caret
  // proves it landed on editable body content rather than page whitespace.
  let exited = false;
  for (
    let y = cell.y + cell.height * 2;
    y < pageBox.y + pageBox.height * 0.9;
    y += 24
  ) {
    await page.mouse.click(pageBox.x + pageBox.width * 0.5, y);
    if (
      (await page.locator(".cell-outline").count()) === 0 &&
      (await page.locator(".overlay .caret").count()) === 1
    ) {
      exited = true;
      break;
    }
  }
  expect(exited, "a body caret must be reachable below the table").toBe(true);
  await page.keyboard.type("BODYAFTERTABLE");
  expect(await tableRows(page)).toEqual(rowsBefore);
  await expect(page.locator("#a11yDocument")).toContainText("BODYAFTERTABLE");
  expect(consoleErrors).toEqual([]);
});
