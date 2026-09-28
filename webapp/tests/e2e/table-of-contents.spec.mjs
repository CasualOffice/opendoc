// Generating and updating a table of contents — References ▸ Table of contents
// and References ▸ Update table.
//
// Both controls shipped DISABLED carrying a reason for as long as there was no
// engine operation behind them. The operations exist now, so what has to be
// guarded is that a user can reach them and that the DOCUMENT changes: a guard
// that asserted the ribbon button is enabled would pass while the dialog
// inserted nothing.
//
// The observable is the accessibility mirror, built from the model — so "the
// contents entries are in the document, one per heading, with the page each
// heading is on" is read from the document rather than from the painted page.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  mirrorBlocks,
  openCommandPalette,
} from "./fixtures.mjs";

/** The whole document as one string, from the model-built mirror. */
const documentText = async (page) => (await mirrorBlocks(page)).join("\n");

/** Runs a registry command by id from the palette. */
async function runPaletteCommand(page, id) {
  await openCommandPalette(page);
  await page.locator("#cmdList .cmd-item").first().waitFor();
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${id}"]`);
  await expect(row, `no palette row for ${id}`).toHaveCount(1);
  await row.click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

test("the References button inserts a table of contents, and the caret lands in its first entry", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await documentText(page);
  await page.locator('[data-tab="references"]').click();
  const insert = page.locator("#refTocBtn");
  // It is no longer a permanently disabled control carrying an engine excuse.
  await expect(insert).toBeEnabled();
  await insert.click();
  await expect(page.locator("#tocDialog")).toBeVisible();

  // Word's "Show levels", defaulting to Word's 3.
  await expect(page.locator("#tocLevels")).toHaveValue("3");
  await page.locator("#tocLevels").fill("1");
  await page.locator('#tocDialog button[type="submit"]').click();
  await expect(page.locator("#tocDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("Table of contents inserted");

  // THE DOCUMENT. Counted, not merely contained: the heading this entry names
  // was already in the document, so "the text is there" is true before the
  // insert and would pass while nothing happened at all.
  const occurrences = (text, needle) => text.split(needle).length - 1;
  expect(occurrences(before, "Rich Document")).toBe(1);
  expect(occurrences(await documentText(page), "Rich Document")).toBe(2);

  // The caret is IN the first entry: typing lands at its start rather than
  // wherever the insert was started from.
  await page.keyboard.type("ZZ");
  expect(await documentText(page)).toContain("ZZRich Document");

  expect(consoleErrors).toEqual([]);
});

test("Update table is disabled until a contents field exists, then offers Word's two modes", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // With no contents field, Update is DISABLED WITH ITS REASON — never absent
  // and never a control that silently does nothing.
  await page.locator('[data-tab="references"]').click();
  const update = page.locator("#refUpdateFieldsBtn");
  await expect(update).toBeDisabled();
  await expect(update).toHaveAttribute("title", /no generated table of contents/i);

  // Insert one, and the same control comes alive.
  await page.locator("#refTocBtn").click();
  await page.locator('#tocDialog button[type="submit"]').click();
  await expect(page.locator("#tocDialog")).toBeHidden();
  await page.locator('[data-tab="references"]').click();
  await expect(update).toBeEnabled();

  // Word's dialog: update page numbers only, or update the entire table.
  await update.click();
  await expect(page.locator("#tocUpdateDialog")).toBeVisible();
  await expect(page.locator('input[name="tocUpdateMode"][value="pageNumbers"]')).toBeChecked();
  await page.locator('input[name="tocUpdateMode"][value="entire"]').check();
  await page.locator('#tocUpdateDialog button[type="submit"]').click();
  await expect(page.locator("#tocUpdateDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("rebuilt from the headings");

  // Each mode is also its own command, so neither is reachable only from inside
  // a modal — the >=2-surface rule (`docs/105` UX-004).
  await runPaletteCommand(page, "reference.updateToc.pageNumbers");
  await expect(page.locator("#status")).toContainText("Page numbers updated");

  expect(consoleErrors).toEqual([]);
});
