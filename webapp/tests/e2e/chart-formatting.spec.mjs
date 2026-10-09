// A chart's formatting is editable, the way Word and ONLYOFFICE do it: axis
// titles, trendlines, error bars, one series' colour and line, combination
// charts on a secondary axis, number formats and text fonts.
//
// What the owner asked for was "complete it": these were the things a chart
// kept and saved but could not change. Every assertion is about what a READER
// sees and gets — the page repainting, the panel reading the engine's own value
// back, one undo taking a change away, and the change surviving Save as DOCX.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, runAppMenuCommand, stableBox } from "./fixtures.mjs";

/** A fingerprint of the first page's pixels. */
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
const control = (page, name) => settings(page).locator(`[data-chart-control="${name}"]`);

/** Inserts a chart, closes its data, and opens the settings panel. */
async function chartWithSettings(page) {
  await gotoEditor(page);
  await page.locator("#tabInsert").click();
  await page.locator("#insertChartBtn").click();
  await expect(dialog(page)).toBeVisible();
  await dialog(page).getByRole("button", { name: "Done" }).click();
  await page.locator("#tabChart").click();
  await page.locator("#chartSettingsBtn").click();
  await expect(settings(page)).toBeVisible();
}

/** Opens a Chart-tab menu and picks a row, through submenus by name. */
async function chartTabMenu(page, button, ...path) {
  await page.locator("#tabChart").click();
  await page.locator(button).click();
  for (const name of path) {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await page.getByRole("menuitem", { name: new RegExp(`^${escaped}(\\s*[›✓])?$`) }).last().click();
  }
}

test("Chart elements ▸ Axis titles adds a vertical title the panel then edits, and one undo removes it", async ({ page }) => {
  await chartWithSettings(page);
  const plain = await pagePixels(page);
  await chartTabMenu(page, "#chartElementsBtn", "Axis titles", "Vertical axis");
  await expect(control(page, "verticalAxisTitle")).toHaveValue("Axis Title");
  await expect.poll(() => pagePixels(page)).not.toBe(plain);

  const titled = await pagePixels(page);
  await control(page, "verticalAxisTitle").fill("Revenue (€)");
  await control(page, "verticalAxisTitle").press("Enter");
  await expect(control(page, "verticalAxisTitle")).toHaveValue("Revenue (€)");
  await expect.poll(() => pagePixels(page)).not.toBe(titled);

  await page.locator("#pages").focus();
  await page.keyboard.press("ControlOrMeta+z");
  await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => pagePixels(page)).toBe(plain);
});

test("a trendline and error bars from Chart elements reach every series and show in the Series section", async ({ page }) => {
  await chartWithSettings(page);
  const plain = await pagePixels(page);
  await chartTabMenu(page, "#chartElementsBtn", "Trendline", "Linear");
  await expect(control(page, "trendline")).toHaveValue("linear");
  await expect.poll(() => pagePixels(page)).not.toBe(plain);
  // Every series got one, not only the one the picker shows.
  await control(page, "series").selectOption("1");
  await expect(control(page, "trendline")).toHaveValue("linear");

  const trended = await pagePixels(page);
  await settings(page).getByLabel("Display equation").check();
  await expect(settings(page).getByLabel("Display equation")).toBeChecked();
  await expect.poll(() => pagePixels(page)).not.toBe(trended);

  const withEquation = await pagePixels(page);
  await chartTabMenu(page, "#chartElementsBtn", "Error bars", "Percentage");
  await expect(control(page, "errorBars")).toHaveValue("percentage");
  await expect(control(page, "errorBarValue")).toHaveValue("5");
  await expect.poll(() => pagePixels(page)).not.toBe(withEquation);
});

test("one series' colour and line style change only that series", async ({ page }) => {
  await chartWithSettings(page);
  await control(page, "type").selectOption("line");
  await expect(control(page, "seriesLineWidth")).toBeVisible();
  const before = await pagePixels(page);
  await control(page, "seriesColor").evaluate((input) => {
    input.value = "#ff0000";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await expect(control(page, "seriesColor")).toHaveValue("#ff0000");
  await expect.poll(() => pagePixels(page)).not.toBe(before);

  const red = await pagePixels(page);
  await control(page, "seriesDash").selectOption("dash");
  await expect(control(page, "seriesDash")).toHaveValue("dash");
  await control(page, "seriesLineWidth").fill("4");
  await control(page, "seriesLineWidth").press("Enter");
  await expect(control(page, "seriesLineWidth")).toHaveValue("4");
  await expect.poll(() => pagePixels(page)).not.toBe(red);

  // The second series kept its automatic colour.
  await control(page, "series").selectOption("1");
  await expect(control(page, "seriesColor")).not.toHaveValue("#ff0000");
  await expect(control(page, "seriesDash")).toHaveValue("solid");
});

test("a combination chart plots its last series as a line on a secondary axis, and a series can be moved back", async ({ page }) => {
  await chartWithSettings(page);
  const plain = await pagePixels(page);
  await chartTabMenu(page, "#chartTypeBtn", "Clustered column – line on secondary axis");
  await expect.poll(() => pagePixels(page)).not.toBe(plain);
  const last = await control(page, "series").locator("option").count();
  await control(page, "series").selectOption(String(last - 1));
  await expect(control(page, "seriesKind")).toHaveValue("line");
  await expect(settings(page).getByLabel("Plot on secondary axis")).toBeChecked();

  // A combo's data is still editable.
  await page.locator("#chartEditDataBtn").click();
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).locator('input[data-row="0"][data-col="0"]')).toBeEnabled();
  await dialog(page).getByRole("button", { name: "Done" }).click();

  await control(page, "seriesKind").selectOption("column");
  await settings(page).getByLabel("Plot on secondary axis").uncheck();
  await expect(control(page, "seriesKind")).toHaveValue("column");
  await expect.poll(() => pagePixels(page)).toBe(plain);
});

test("an axis number format and a text font repaint the chart, and a bad font size is refused", async ({ page }) => {
  await chartWithSettings(page);
  const plain = await pagePixels(page);
  await control(page, "verticalAxisNumberFormat").selectOption("0.00");
  await expect(control(page, "verticalAxisNumberFormat")).toHaveValue("0.00");
  await expect.poll(() => pagePixels(page)).not.toBe(plain);

  const formatted = await pagePixels(page);
  await control(page, "fontTarget").selectOption("chart");
  await control(page, "fontSize").fill("14");
  await control(page, "fontSize").press("Enter");
  await expect(control(page, "fontSize")).toHaveValue("14");
  await expect.poll(() => pagePixels(page)).not.toBe(formatted);

  const larger = await pagePixels(page);
  await control(page, "fontBold").click();
  await expect(control(page, "fontBold")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => pagePixels(page)).not.toBe(larger);

  await control(page, "fontSize").fill("9000");
  await control(page, "fontSize").press("Enter");
  await expect(settings(page).locator(".chart-data-note")).not.toBeEmpty();
  await expect(control(page, "fontSize")).toHaveValue("14");
});

test("axis titles, trendlines, a combination and fonts survive Save as DOCX", async ({ page }) => {
  await chartWithSettings(page);
  await chartTabMenu(page, "#chartElementsBtn", "Axis titles", "Vertical axis");
  await chartTabMenu(page, "#chartElementsBtn", "Trendline", "Exponential");
  await chartTabMenu(page, "#chartTypeBtn", "Clustered column – line");
  await control(page, "fontTarget").selectOption("chart");
  await control(page, "fontSize").fill("12");
  await control(page, "fontSize").press("Enter");
  await expect(control(page, "fontSize")).toHaveValue("12");

  const download = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.docx");
  const bytes = await readFile(await (await download).path());
  await page.locator("#file").setInputFiles({
    name: "formatted-chart.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: bytes,
  });
  await expect(page.locator(".page-wrap canvas.page").first()).toBeVisible();
  const box = await stableBox(page.locator(".page-wrap canvas.page").first());
  // The reopened file may show the compact toolbar, which has no ribbon tabs;
  // the chart's own right-click menu is on every layout.
  await page.mouse.click(box.x + box.width * 0.3, box.y + box.height * 0.12, { button: "right" });
  await page.getByRole("menuitem", { name: /^Chart settings…/ }).click();
  await expect(settings(page)).toBeVisible();
  await expect(control(page, "verticalAxisTitle")).toHaveValue("Axis Title");
  await expect(control(page, "trendline")).toHaveValue("exp");
  await expect(control(page, "fontSize")).toHaveValue("12");
  const last = await control(page, "series").locator("option").count();
  await control(page, "series").selectOption(String(last - 1));
  await expect(control(page, "seriesKind")).toHaveValue("line");
});
