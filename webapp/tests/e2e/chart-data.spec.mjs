// A chart's data is editable, from the moment it is inserted.
//
// The defect this guards, in the owner's words: "its static, how can i add
// data … what does adding static chart achieve". Insert ▸ Chart put Word's
// sample numbers on the page and nothing could change them. Every assertion
// below is about what a READER sees and gets — the panel opening on its own,
// a typed number moving the painted bars, one undo taking it back, a pasted
// spreadsheet block growing the grid — never about which element holds what.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, expectEditorFocused, setReviewMode, runAppMenuCommand, stableBox } from "./fixtures.mjs";

/** A fingerprint of the first page's pixels, so "the chart repainted" is a
 *  thing the test can see rather than infer from the panel. */
async function pagePixels(page) {
  return page.evaluate(() => {
    const canvas = document.querySelector(".page-wrap canvas.page");
    const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
    let hash = 0;
    for (let i = 0; i < data.length; i += 7) hash = (hash * 31 + data[i]) | 0;
    return hash;
  });
}

const panel = (page) => page.locator("#chartDataPanel");
const cell = (page, row, col) => panel(page).locator(`input[data-row="${row}"][data-col="${col}"]`);

async function insertChart(page) {
  await page.locator("#tabInsert").click();
  await page.locator("#insertChartBtn").click();
  await expect(panel(page)).toBeVisible();
}

test("Insert ▸ Chart opens the chart's data, and a typed number repaints the chart and undoes in one step", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);

  // Word's sequence: the data is open and the first value is ready to type over.
  await expect(cell(page, 0, 0)).toBeFocused();
  await expect(panel(page).locator('[role="radio"][aria-checked="true"]')).toHaveAttribute("data-kind", "column");
  const original = await cell(page, 0, 0).inputValue();
  expect(original).not.toBe("");

  const before = await pagePixels(page);
  await cell(page, 0, 0).fill("42");
  await cell(page, 0, 0).press("Enter");
  // Enter commits and moves down a row, as in a spreadsheet.
  await expect(cell(page, 1, 0)).toBeFocused();
  // The value shown is re-read from the ENGINE after the write, so this is the
  // document's number, not the text the test typed.
  await expect(cell(page, 0, 0)).toHaveValue("42");
  await expect.poll(() => pagePixels(page)).not.toBe(before);

  // One undo step for one cell, taken from the document, reflected in the panel.
  await page.locator("#pages").focus();
  await page.keyboard.press("ControlOrMeta+z");
  await expect(cell(page, 0, 0)).toHaveValue(original);
  await expect.poll(() => pagePixels(page)).toBe(before);
});

test("rows and series can be added, removed and named, and the chart follows", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const rows = () => panel(page).locator("tbody tr").count();
  const series = () => panel(page).locator('input[data-row="-1"]').count();
  const startRows = await rows();
  const startSeries = await series();

  await panel(page).getByRole("button", { name: "Add row" }).click();
  await expect.poll(rows).toBe(startRows + 1);
  // The new row's label field takes focus, ready to be named.
  await expect(cell(page, startRows, -1)).toBeFocused();
  await cell(page, startRows, -1).fill("Q5");
  await cell(page, startRows, -1).press("Tab");
  await expect(cell(page, startRows, -1)).toHaveValue("Q5");

  await panel(page).getByRole("button", { name: "Add series" }).click();
  await expect.poll(series).toBe(startSeries + 1);
  await cell(page, -1, startSeries).fill("Forecast");
  await cell(page, -1, startSeries).press("Enter");
  await expect(cell(page, -1, startSeries)).toHaveValue("Forecast");

  await panel(page).getByRole("button", { name: "Remove series Forecast" }).click();
  await expect.poll(series).toBe(startSeries);
  await panel(page).getByRole("button", { name: "Remove row Q5" }).click();
  await expect.poll(rows).toBe(startRows);
});

test("a block pasted from a spreadsheet fills the grid from the cell it lands in, growing it to fit", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const startRows = await panel(page).locator("tbody tr").count();
  // The exact text Excel and Sheets put on the clipboard: tab-separated rows,
  // a trailing newline, and a displayed grouping comma. Pasted on the first
  // row's LABEL, so column one of the block is the category names.
  const tsv = "Jan\t1,200\t3\nFeb\t4\t5\nMar\t6\t7\nApr\t8\t9\nMay\t10\t11\n";
  await cell(page, 0, -1).focus();
  await page.evaluate((text) => {
    const data = new DataTransfer();
    data.setData("text/plain", text);
    document.activeElement.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  }, tsv);
  await expect(cell(page, 0, -1)).toHaveValue("Jan");
  await expect(cell(page, 0, 0)).toHaveValue("1200");
  await expect(cell(page, 4, -1)).toHaveValue("May");
  await expect(cell(page, 4, 1)).toHaveValue("11");
  expect(await panel(page).locator("tbody tr").count()).toBe(Math.max(startRows, 5));
});

test("the chart type, title and legend are one click each, and the type is a real family change", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const before = await pagePixels(page);
  await panel(page).getByRole("radio", { name: "Pie" }).click();
  await expect(panel(page).getByRole("radio", { name: "Pie" })).toHaveAttribute("aria-checked", "true");
  await expect.poll(() => pagePixels(page)).not.toBe(before);

  const pie = await pagePixels(page);
  await panel(page).getByLabel("Title").fill("Revenue by quarter");
  await panel(page).getByLabel("Title").press("Enter");
  await expect.poll(() => pagePixels(page)).not.toBe(pie);
  await expect(panel(page).getByLabel("Title")).toHaveValue("Revenue by quarter");

  await panel(page).getByLabel("Legend").selectOption("bottom");
  await expect(panel(page).getByLabel("Legend")).toHaveValue("bottom");
});

test("a value that is not a number is refused at the cell, saying which cell and what to type", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const original = await cell(page, 1, 0).inputValue();
  await cell(page, 1, 0).fill("lots");
  await cell(page, 1, 0).press("Enter");
  await expect(cell(page, 1, 0)).toHaveAttribute("aria-invalid", "true");
  await expect(panel(page).locator(".chart-data-note")).toContainText("“lots” in row 2");
  await expect(panel(page).locator(".chart-data-note")).toContainText("is not a number");
  // Escape abandons the bad entry and puts the document's value back.
  await cell(page, 1, 0).press("Escape");
  await expect(cell(page, 1, 0)).toHaveValue(original);
  await expect(cell(page, 1, 0)).not.toHaveAttribute("aria-invalid", "true");
});

test("Edit data is reachable from the chart's chip, its right-click menu and a double-click", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await panel(page).getByRole("button", { name: "Close chart panel" }).click();
  await expect(panel(page)).toBeHidden();
  await expectEditorFocused(page);

  // The chart is still selected; its chip carries Edit data.
  const chip = page.locator(".object-context-bar");
  await expect(chip).toBeVisible();
  await chip.getByRole("button", { name: /Edit the chart's data/ }).click();
  await expect(panel(page)).toBeVisible();
  await panel(page).getByRole("button", { name: "Close chart panel" }).click();

  // Right-click on the chart itself.
  const box = await stableBox(chip);
  const target = { x: box.x + 40, y: box.y + box.height + 60 };
  await page.mouse.click(target.x, target.y, { button: "right" });
  await page.getByRole("menuitem", { name: /Edit data/ }).click();
  await expect(panel(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(panel(page)).toBeHidden();

  // Double-click, Word's and Docs' gesture for "edit this chart".
  await page.mouse.dblclick(target.x, target.y);
  await expect(panel(page)).toBeVisible();
});

test("in Viewing mode the data is shown and nothing can be changed", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const original = await cell(page, 0, 0).inputValue();
  await setReviewMode(page, "viewing");
  await cell(page, 0, 0).fill("99");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue(original);
});

test("an edited chart survives Save as DOCX and reopens still editable, with its workbook", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await cell(page, 0, 0).fill("77.5");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue("77.5");
  await cell(page, -1, 0).fill("Revenue");
  await cell(page, -1, 0).press("Enter");

  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.docx");
  const bytes = await readFile(await (await download).path());
  // A chart the editor wrote names an embedded workbook, so Word's own Edit
  // Data opens the numbers the chart shows instead of "linked file not
  // available".
  expect(bytes.includes(Buffer.from("word/embeddings/Microsoft_Excel_Worksheet_"))).toBe(true);

  // Reopen the saved file: the chart is the one we edited, and it is still
  // editable — the "static chart" defect must not come back one save later.
  await page.locator("#file").setInputFiles({
    name: "chart.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: bytes,
  });
  await expect(panel(page)).toBeHidden();
  // Select the chart by double-clicking where it sits: it is the first thing on
  // the first page of this document.
  const sheet = page.locator(".page-wrap canvas.page").first();
  await expect(sheet).toBeVisible();
  const box = await stableBox(sheet);
  await page.mouse.dblclick(box.x + box.width * 0.3, box.y + box.height * 0.12);
  await expect(panel(page)).toBeVisible();
  await expect(cell(page, 0, 0)).toHaveValue("77.5");
  await expect(cell(page, -1, 0)).toHaveValue("Revenue");
  await expect(cell(page, 0, 0)).toBeEnabled();
  // It came from a file and names a workbook, so the panel says what an edit
  // will do to that workbook before the reader makes one.
  await expect(panel(page).locator(".chart-data-reason")).toContainText("embedded workbook");
  await cell(page, 0, 0).fill("80");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue("80");
});
