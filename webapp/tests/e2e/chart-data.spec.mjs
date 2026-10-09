// A chart is editable end to end, the way ONLYOFFICE and Word do it.
//
// The defect this guards, in the owner's words: "its static, how can i add
// data … what does adding static chart achieve". Every assertion is about what
// a READER sees and gets: the data opening on insert, a typed number moving the
// painted bars, one undo taking it back, a pasted spreadsheet block growing the
// grid, the Chart tab and the settings panel changing the chart, a Word chart
// opening editable, and an edited chart surviving a save.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, expectEditorFocused, setReviewMode, runAppMenuCommand, stableBox } from "./fixtures.mjs";
import { makeWordChartDocx } from "./large-docx.mjs";

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

const dialog = (page) => page.locator("#chartDataDialog");
const settings = (page) => page.locator("#chartPanel");
const cell = (page, row, col) => dialog(page).locator(`input[data-row="${row}"][data-col="${col}"]`);
const done = (page) => dialog(page).getByRole("button", { name: "Done" });

async function insertChart(page) {
  await page.locator("#tabInsert").click();
  await page.locator("#insertChartBtn").click();
  await expect(dialog(page)).toBeVisible();
}

/** Opens a Chart-tab menu and picks a row, through submenus by name. */
async function chartTabMenu(page, button, ...path) {
  await page.locator("#tabChart").click();
  await page.locator(button).click();
  for (const name of path) {
    // A row's accessible name is its label, then — after a space — its check
    // (✓) or, for a submenu, its caret (›). The label itself is matched exactly.
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await page.getByRole("menuitem", { name: new RegExp(`^${escaped}(\\s*[›✓])?$`) }).last().click();
  }
}

test("Insert ▸ Chart opens the chart's data, and a typed number repaints the chart and undoes in one step", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);

  // ONLYOFFICE's and Word's sequence: the data is open on the first value.
  await expect(cell(page, 0, 0)).toBeFocused();
  const original = await cell(page, 0, 0).inputValue();
  expect(original).not.toBe("");

  const before = await pagePixels(page);
  await cell(page, 0, 0).fill("42");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 1, 0)).toBeFocused();
  // Re-read from the ENGINE after the write: the document's number.
  await expect(cell(page, 0, 0)).toHaveValue("42");
  await expect.poll(() => pagePixels(page)).not.toBe(before);

  await done(page).click();
  await expect(dialog(page)).toBeHidden();
  await page.locator("#pages").focus();
  await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => pagePixels(page)).toBe(before);
});

test("rows and series can be added, removed and named", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const rows = () => dialog(page).locator("tbody tr").count();
  const series = () => dialog(page).locator('input[data-row="-1"]').count();
  const startRows = await rows();
  const startSeries = await series();

  await dialog(page).getByRole("button", { name: "Add row" }).click();
  await expect.poll(rows).toBe(startRows + 1);
  await expect(cell(page, startRows, -1)).toBeFocused();
  await cell(page, startRows, -1).fill("Q5");
  await cell(page, startRows, -1).press("Tab");
  await expect(cell(page, startRows, -1)).toHaveValue("Q5");

  await dialog(page).getByRole("button", { name: "Add series" }).click();
  await expect.poll(series).toBe(startSeries + 1);
  await cell(page, -1, startSeries).fill("Forecast");
  await cell(page, -1, startSeries).press("Enter");
  await expect(cell(page, -1, startSeries)).toHaveValue("Forecast");

  await dialog(page).getByRole("button", { name: "Remove series Forecast" }).click();
  await expect.poll(series).toBe(startSeries);
  await dialog(page).getByRole("button", { name: "Remove row Q5" }).click();
  await expect.poll(rows).toBe(startRows);
});

test("a block pasted from a spreadsheet fills the grid from the cell it lands in, growing it to fit", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const startRows = await dialog(page).locator("tbody tr").count();
  // Excel's and Sheets' clipboard: tab-separated rows, a trailing newline, a
  // displayed grouping comma — pasted on the first row's LABEL.
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
  expect(await dialog(page).locator("tbody tr").count()).toBe(Math.max(startRows, 5));
});

test("a value that is not a number is refused at the cell, saying which cell and what to type", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  const original = await cell(page, 1, 0).inputValue();
  await cell(page, 1, 0).fill("lots");
  await cell(page, 1, 0).press("Enter");
  await expect(cell(page, 1, 0)).toHaveAttribute("aria-invalid", "true");
  await expect(dialog(page).locator(".chart-data-note")).toContainText("“lots” in row 2");
  await cell(page, 1, 0).press("Escape");
  await expect(cell(page, 1, 0)).toHaveValue(original);
  await expect(dialog(page)).toBeVisible();
  // A second Escape on the untouched cell closes the window.
  await cell(page, 1, 0).press("Escape");
  await expect(dialog(page)).toBeHidden();
});

test("the Chart tab is contextual and changes the type, the elements and the style", async ({ page }) => {
  await gotoEditor(page);
  await expect(page.locator("#tabChart")).toBeDisabled();
  await insertChart(page);
  await done(page).click();
  await expect(page.locator("#tabChart")).toBeEnabled();

  const column = await pagePixels(page);
  await chartTabMenu(page, "#chartTypeBtn", "Pie");
  await expect.poll(() => pagePixels(page)).not.toBe(column);

  const pie = await pagePixels(page);
  await chartTabMenu(page, "#chartTypeBtn", "Stacked column");
  await expect.poll(() => pagePixels(page)).not.toBe(pie);

  const stacked = await pagePixels(page);
  await chartTabMenu(page, "#chartElementsBtn", "Gridlines", "Vertical gridlines");
  await expect.poll(() => pagePixels(page)).not.toBe(stacked);

  const gridded = await pagePixels(page);
  await chartTabMenu(page, "#chartElementsBtn", "Data labels", "Inside base");
  await expect.poll(() => pagePixels(page)).not.toBe(gridded);

  const labelled = await pagePixels(page);
  await page.locator("#tabChart").click();
  const mono = page.locator('#chartStyleGallery [data-command="chart.style.mono-2"]');
  await mono.click();
  await expect(mono).toHaveAttribute("aria-checked", "true");
  await expect.poll(() => pagePixels(page)).not.toBe(labelled);
});

test("the Chart settings panel sets the title, the legend and the axis bounds", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await done(page).click();
  await page.locator("#tabChart").click();
  await page.locator("#chartSettingsBtn").click();
  await expect(settings(page)).toBeVisible();

  const plain = await pagePixels(page);
  await settings(page).locator('[data-chart-control="title"]').selectOption("above");
  await expect(settings(page).locator('[data-chart-control="titleText"]')).toHaveValue("Chart Title");
  await settings(page).locator('[data-chart-control="titleText"]').fill("Revenue by quarter");
  await settings(page).locator('[data-chart-control="titleText"]').press("Enter");
  await expect(settings(page).locator('[data-chart-control="titleText"]')).toHaveValue("Revenue by quarter");
  await expect.poll(() => pagePixels(page)).not.toBe(plain);

  await settings(page).locator('[data-chart-control="legend"]').selectOption("bottom");
  await expect(settings(page).locator('[data-chart-control="legend"]')).toHaveValue("bottom");

  const before = await pagePixels(page);
  await settings(page).locator('input[data-axis="vertical.maximum"]').fill("20");
  await settings(page).locator('input[data-axis="vertical.maximum"]').press("Enter");
  await expect(settings(page).locator('input[data-axis="vertical.maximum"]')).toHaveValue("20");
  await expect.poll(() => pagePixels(page)).not.toBe(before);
  // A bound that is not a number is refused, saying so.
  await settings(page).locator('input[data-axis="vertical.minimum"]').fill("lots");
  await settings(page).locator('input[data-axis="vertical.minimum"]').press("Enter");
  await expect(settings(page).locator(".chart-data-note")).toContainText("not a number");
});

test("Edit data is reachable from the chip, the right-click menu, the palette and a double-click", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await done(page).click();
  await expectEditorFocused(page);

  const chip = page.locator(".object-context-bar");
  await expect(chip).toBeVisible();
  await chip.getByRole("button", { name: /Edit the chart's data/ }).click();
  await expect(dialog(page)).toBeVisible();
  await done(page).click();

  const box = await stableBox(chip);
  const target = { x: box.x + 40, y: box.y + box.height + 60 };
  await page.mouse.click(target.x, target.y, { button: "right" });
  await page.getByRole("menuitem", { name: /^Edit data/ }).click();
  await expect(dialog(page)).toBeVisible();
  await done(page).click();

  await page.keyboard.press("ControlOrMeta+Shift+P");
  await page.locator('#cmdList .cmd-item[data-command-id="chart.editData"]').first().click();
  await expect(dialog(page)).toBeVisible();
  await done(page).click();

  await page.mouse.dblclick(target.x, target.y);
  await expect(dialog(page)).toBeVisible();
});

test("in Viewing mode the data is shown and nothing can be changed", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await done(page).click();
  await setReviewMode(page, "viewing");
  await page.keyboard.press("ControlOrMeta+Shift+P");
  await page.locator('#cmdList .cmd-item[data-command-id="chart.editData"]').first().click();
  await expect(dialog(page)).toBeVisible();
  const original = await cell(page, 0, 0).inputValue();
  await cell(page, 0, 0).fill("99", { force: true }).catch(() => {});
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue(original);
});

test("a chart Word wrote opens editable, and an edit repaints it", async ({ page }) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles({
    name: "word-chart.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(makeWordChartDocx()),
  });
  await expect(page.locator(".page-wrap canvas.page").first()).toBeVisible();
  const box = await stableBox(page.locator(".page-wrap canvas.page").first());
  // The chart is the first thing on the page.
  await page.mouse.dblclick(box.x + box.width * 0.3, box.y + box.height * 0.12);
  await expect(dialog(page)).toBeVisible();
  await expect(cell(page, 0, 0)).toHaveValue("4.3");
  await expect(cell(page, 0, 0)).not.toHaveAttribute("readonly", "");
  const before = await pagePixels(page);
  await cell(page, 0, 0).fill("1");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue("1");
  await expect.poll(() => pagePixels(page)).not.toBe(before);
});

test("an edited chart survives Save as DOCX and reopens still editable, with its workbook", async ({ page }) => {
  await gotoEditor(page);
  await insertChart(page);
  await cell(page, 0, 0).fill("77.5");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue("77.5");
  await cell(page, -1, 0).fill("Revenue");
  await cell(page, -1, 0).press("Enter");
  await done(page).click();

  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.docx");
  const bytes = await readFile(await (await download).path());
  expect(bytes.includes(Buffer.from("word/embeddings/Microsoft_Excel_Worksheet_"))).toBe(true);

  await page.locator("#file").setInputFiles({
    name: "chart.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: bytes,
  });
  await expect(page.locator(".page-wrap canvas.page").first()).toBeVisible();
  const box = await stableBox(page.locator(".page-wrap canvas.page").first());
  await page.mouse.dblclick(box.x + box.width * 0.3, box.y + box.height * 0.12);
  await expect(dialog(page)).toBeVisible();
  await expect(cell(page, 0, 0)).toHaveValue("77.5");
  await expect(cell(page, -1, 0)).toHaveValue("Revenue");
  await expect(dialog(page).locator(".chart-data-reason")).toContainText("embedded workbook");
  await cell(page, 0, 0).fill("80");
  await cell(page, 0, 0).press("Enter");
  await expect(cell(page, 0, 0)).toHaveValue("80");
});
