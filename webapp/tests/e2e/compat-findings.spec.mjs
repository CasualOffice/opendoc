// The "N import findings" chip opens the findings it counts, and the same
// report is reachable from the palette and the File page.
//
// The chip was a `<span role="status">`: it could not be clicked or focused and
// no command reached the report behind it (`desk-11-import-findings.png`), so a
// reader was told something had been lost and given no way to find out what.
// `sample.docx` is the document with findings to show: since FID-AT-08/09/10 the
// engine carries everything `?fixture=rich` used to report, so that fixture has
// none (its chip is hidden, which the no-findings test below covers).
//
// The second half is `109` FID-AT-05, on the document the owner opened:
// `sample.docx`, whose dialog listed `cNvPr/@name`, `fontScheme/@name` and
// `docx.rsid ×165` under a headline of 183 kept findings. Each row now reads as
// words with its id kept beside it, and Word's own bookkeeping is a collapsed,
// labelled disclosure that the headline does not count.
import {
  test,
  expect,
  MOD,
  gotoSampleDocument as openSample,
  openFilePage,
  runPaletteCommand,
  stableBox,
} from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

const chip = (page) => page.locator("#compatibilityStatus");
const dialog = (page) => page.locator("#compatibilityFindingsDialog");

/** The sum of the group headlines the dialog prints. */
async function headlineTotal(page) {
  const totals = await dialog(page)
    .locator(".findings-total")
    .evaluateAll((nodes) => nodes.map((node) => Number(node.textContent.replace(/[^\d]/g, ""))));
  return totals.reduce((sum, value) => sum + value, 0);
}

/** The count the chip prints, as a number. */
async function chipCount(page) {
  return Number((await chip(page).textContent()).replace(/[^\d]/g, ""));
}

test("the findings chip is a button that opens what it counts, grouped by what happened", async ({
  page,
  consoleErrors,
}) => {
  await openSample(page);
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
  await openSample(page);
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

test("sample.docx's findings read as words, and Word's bookkeeping is one collapsed, labelled group", async ({
  page,
  consoleErrors,
}) => {
  await openSample(page);
  await expect(chip(page)).toBeVisible();
  const counted = await chipCount(page);
  await chip(page).focus();
  await page.keyboard.press("Enter");
  await expect(dialog(page)).toBeVisible();

  // Words first, where it is, and the engine's id kept beside them for support.
  // The rows asserted are the ones the engine still reports for sample.docx;
  // the settings it used to (`decimalSymbol` and its siblings) are carried
  // since FID-AT-10 and no longer findings at all.
  const defaults = dialog(page).locator('.findings-row[data-feature="objectDefaults"]');
  await expect(defaults.locator(".findings-name")).toHaveText(
    "Theme's default look for new shapes, lines and text boxes",
  );
  await expect(defaults.locator(".findings-where")).toHaveText("in the theme");
  await expect(defaults.locator(".findings-feature")).toHaveText("objectDefaults");
  const theme = dialog(page).locator('.findings-row[data-feature="theme/@name"]');
  await expect(theme.locator(".findings-name")).toHaveText("Document theme name");
  await expect(theme.locator(".findings-where")).toHaveText("in the theme");

  // No row, open or folded, reads as its id or as a catalogue key.
  const rows = await dialog(page)
    .locator(".findings-row")
    .evaluateAll((nodes) =>
      nodes.map((row) => ({
        feature: row.dataset.feature,
        name: row.querySelector(".findings-name")?.textContent ?? "",
      })),
    );
  expect(rows.length, "sample.docx has findings for this to prove anything").toBeGreaterThan(4);
  expect(
    rows.filter((row) => row.name === row.feature || row.name === "" || row.name.startsWith("findings.")),
  ).toEqual([]);

  // The headline counts what a reader would miss: the chip and the group totals
  // agree, and the 165 revision-save ids are in neither.
  expect(await headlineTotal(page)).toBe(counted);
  expect(counted).toBeGreaterThan(0);
  expect(counted).toBeLessThan(165);

  // Word's own bookkeeping: a real disclosure, collapsed, naming its count.
  // Found by the row it holds, not by the group it sits in — which group that
  // is, is the engine's outcome and may change; that it is folded may not.
  const rsidRow = '.findings-row[data-feature="docx.rsid"]';
  const rsid = dialog(page).locator(rsidRow);
  const disclosure = dialog(page).locator(".findings-bookkeeping", { has: page.locator(rsidRow) });
  const toggle = disclosure.getByRole("button", { name: /Word's own bookkeeping/ });
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  const folded = await toggle.locator(".findings-bookkeeping-total").textContent();
  expect(Number(folded.replace(/[^\d]/g, ""))).toBeGreaterThanOrEqual(165);
  await expect(rsid).toBeHidden();
  const panel = page.locator(`#${await toggle.getAttribute("aria-controls")}`);
  await expect(panel).toBeHidden();

  // Keyboard: Enter opens it, and every entry is there, rsid as ONE line.
  await toggle.focus();
  await page.keyboard.press("Enter");
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(panel).toBeVisible();
  await expect(rsid).toHaveCount(1);
  await expect(rsid).toBeVisible();
  await expect(rsid.locator(".findings-count")).toHaveText("×165");
  await expect(rsid.locator(".findings-name")).toHaveText(/Word adds on every save/);
  await expect(
    panel.locator('.findings-row[data-feature="docProps/thumbnail.jpeg"] .findings-name'),
  ).toHaveText("Thumbnail preview of the first page");
  // Space, a button's own key, folds it again.
  await page.keyboard.press("Space");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(rsid).toBeHidden();

  await page.keyboard.press("Escape");
  await expect(dialog(page)).toBeHidden();
  await expect(chip(page)).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("the findings speak the interface's language, words and bookkeeping alike", async ({
  page,
  consoleErrors,
}) => {
  await openSample(page, "?lang=de");
  await chip(page).click();
  await expect(dialog(page)).toBeVisible();
  const theme = dialog(page).locator('.findings-row[data-feature="theme/@name"]');
  await expect(theme.locator(".findings-name")).toHaveText("Name des Dokumentdesigns");
  await expect(theme.locator(".findings-where")).toHaveText("im Design");
  await expect(theme.locator(".findings-feature")).toHaveText("theme/@name");
  await expect(dialog(page).getByRole("button", { name: /Words eigene Verwaltungsdaten/ })).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test.describe("with a finger", () => {
  test.use({ hasTouch: true, viewport: { width: 390, height: 844 } });

  test("the bookkeeping disclosure is a finger-sized target and a tap opens it", async ({
    page,
    consoleErrors,
  }) => {
    await openSample(page);
    // The chip is not shown on a phone; the report is a command away.
    await runPaletteCommand(page, "file.compatibilityReport", "compatibility");
    await expect(dialog(page)).toBeVisible();
    const toggle = dialog(page).locator(".findings-disclosure").first();
    await toggle.scrollIntoViewIfNeeded();
    const box = await stableBox(toggle);
    expect(box.height).toBeGreaterThanOrEqual(44);
    await toggle.tap();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(consoleErrors).toEqual([]);
  });
});
