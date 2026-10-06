// The "N import findings" chip opens the findings it counts, and the same
// report is reachable from the palette and the File page.
//
// The chip was a `<span role="status">`: it could not be clicked or focused and
// no command reached the report behind it (`desk-11-import-findings.png`), so a
// reader was told something had been lost and given no way to find out what.
// `?fixture=rich` is the compatibility document with findings to show.
import { test, expect, MOD, gotoEditor, openFilePage } from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

const chip = (page) => page.locator("#compatibilityStatus");
const dialog = (page) => page.locator("#compatibilityFindingsDialog");

/** The count the chip prints, as a number. */
async function chipCount(page) {
  return Number((await chip(page).textContent()).replace(/[^\d]/g, ""));
}

test("the findings chip is a button that opens what it counts, grouped by what happened", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(chip(page)).toBeVisible();
  await expect(chip(page)).toHaveRole("button");
  const counted = await chipCount(page);
  expect(counted, "the fixture must have findings for this to prove anything").toBeGreaterThan(0);

  // Keyboard first: it takes focus and Enter opens it.
  await chip(page).focus();
  await page.keyboard.press("Enter");
  await expect(dialog(page)).toBeVisible();
  await expect(page.locator("#compatibilityFindingsDialogTitle")).toHaveText("Compatibility findings");

  // Grouped, and the groups account for exactly what the chip counted.
  const groups = dialog(page).locator(".findings-group");
  expect(await groups.count()).toBeGreaterThan(0);
  const totals = await dialog(page)
    .locator(".findings-total")
    .evaluateAll((nodes) => nodes.map((node) => Number(node.textContent.replace(/[^\d]/g, ""))));
  expect(totals.reduce((sum, value) => sum + value, 0)).toBe(counted);
  await expect(dialog(page).locator(".findings-row").first()).toBeVisible();
  await expect(dialog(page).locator(".findings-feature").first()).not.toHaveText("");

  // The shared dismissal: Escape closes it and puts the keyboard back on the chip.
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toBeHidden();
  await expect(chip(page)).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("the palette and the File page open the same findings", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("compatibility");
  const row = page.locator('#cmdList .cmd-item[data-command-id="file.compatibilityReport"]');
  await expect(row).toBeEnabled();
  await row.click();
  await expect(dialog(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toBeHidden();

  await openFilePage(page);
  await page.locator('#filePageBody .file-page-item[data-command="file.compatibilityReport"]').click();
  await expect(dialog(page)).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("a document with no findings shows no chip, and the command says why it is unavailable", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#status")).toContainText("Ready");
  await page.setInputFiles("#file", {
    name: "clean.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(makeLargeDocx(1)),
  });
  await expect(page.locator("body")).toHaveClass(/doc-loaded/);
  // Not painted at all. A `hidden` chip used to paint as an empty 16x4 box,
  // which as a button would be a focusable control with nothing to say.
  await expect(chip(page)).toBeHidden();
  expect(await chip(page).evaluate((el) => el.getClientRects().length)).toBe(0);

  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("compatibility");
  const row = page.locator('#cmdList .cmd-item[data-command-id="file.compatibilityReport"]');
  await expect(row).toBeDisabled();
  await expect(row).toContainText("This document has no compatibility findings");
  expect(consoleErrors).toEqual([]);
});
