// The one-gesture paragraph-space rows, and what the line-spacing box says when
// it has nothing to say.
//
// Word and Google Docs both put "Add space before/after paragraph" in the same
// dropdown as the line-spacing presets; this editor had only two numeric point
// fields, so setting the standard 12 pt meant knowing it was 12. Docs' shape is
// followed (one row per side whose verb flips from the current state) with
// Word's amount (12 pt) — `spacing_menu.mjs` states the choice.
//
// The second half guards a smaller thing that was wrong for every paragraph of
// the shipped sample: `paragraphSpacing` reports DIRECT formatting only, so a
// paragraph taking its spacing from its style showed a blank box with no preset
// ticked and no explanation. Measured before this change: at the document start
// and six lines down, all four presets `aria-checked="false"` and the value
// empty.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  openCommandPalette,
} from "./fixtures.mjs";

const menu = "#spacingMenu";

async function openSpacing(page) {
  if (await page.locator(menu).isHidden()) await page.locator("#spacingBtn").click();
  await expect(page.locator(menu)).toBeVisible();
}

async function closeSpacing(page) {
  if (await page.locator(menu).isVisible()) {
    await page.keyboard.press("Escape");
    await expect(page.locator(menu)).toBeHidden();
  }
}

test("Add space after paragraph adds it, flips to Remove, and removes it again", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await openSpacing(page);

  const row = page.locator("#spaceAfterToggle");
  const field = page.locator("#spaceAfter");
  const startedWith = await field.inputValue();

  // Whichever way the sample's first paragraph starts, one press must change it
  // and the row must then offer the opposite. Drive it from the "no space"
  // state so the assertion is about the 12 pt Word writes.
  if (startedWith !== "0" && startedWith !== "") {
    await expect(row).toHaveText(/remove/i);
    await row.click();
    await openSpacing(page);
  }
  await expect(row).toHaveText(/add/i);

  await row.click();
  await openSpacing(page);
  await expect(field).toHaveValue("12");
  await expect(row).toHaveText(/remove/i);

  await row.click();
  await openSpacing(page);
  await expect(field).toHaveValue("0");
  await expect(row).toHaveText(/add/i);

  // One undoable action per press, not a silent pair.
  await closeSpacing(page);
  await expect(page.locator("#undoBtn")).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

test("the same capability is reachable from the palette, with the same flipped verb", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Drive the paragraph to a known "no space before" state through the popover.
  await openSpacing(page);
  await page.locator("#spaceBefore").fill("0");
  await page.locator("#spaceBefore").blur();
  await closeSpacing(page);

  await openCommandPalette(page);
  await page.locator("#cmdInput").fill("space before paragraph");
  const row = page.locator('#cmdList .cmd-item[data-command-id="paragraph.space.before"]');
  await expect(row).toContainText(/add/i);
  await row.click();

  await openSpacing(page);
  await expect(page.locator("#spaceBefore")).toHaveValue("12");
  await expect(page.locator("#spaceBeforeToggle")).toHaveText(/remove/i);
});

test("an inherited line spacing says so instead of showing an empty box", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await openSpacing(page);

  const value = page.locator("#lineSpacingValue");
  const note = page.locator("#lineSpacingFromStyle");
  // The sample's paragraphs carry no direct line spacing, so the box is empty —
  // and empty must not be the whole answer.
  await expect(value).toHaveValue("");
  await expect(note).toBeVisible();
  await expect(note).toHaveText(/style/i);

  // Setting one puts the explanation away, because there is now a real value.
  await value.fill("1.5");
  await value.blur();
  await openSpacing(page);
  await expect(value).toHaveValue("1.5");
  await expect(note).toBeHidden();
});
