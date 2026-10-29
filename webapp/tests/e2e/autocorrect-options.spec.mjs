// AutoCorrect beyond smart quotes, with real keystrokes (`docs/109` RM-19;
// `docs/153` `text.autoformat-hyperlink`, `text.autocorrect-replace`,
// `review.autocorrect-options`, `text.autocapitalise`, `text.non-breaking-space`).
//
// Every AutoCorrect step is its OWN undo step after the typed character, so the
// next Cmd+Z reverts only the correction (Word, Docs and ONLYOFFICE agree). The
// assertions read the accessibility mirror (`#a11yDocument`), which is the text
// a screen reader gets, and the undo button's label, which names the step the
// next Cmd+Z will undo.
import {
  MOD,
  clickIntoFirstPage,
  expect,
  gotoEditor,
  runAppMenuCommand,
  runPaletteCommand,
  test,
} from "./fixtures.mjs";

/** A fresh empty paragraph at the end of the document, in Editing mode. */
async function freshParagraph(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
}

const mirror = (page) => page.locator("#a11yDocument").innerText();

test("a typed address becomes a link on the space, as its own undo step", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  const undo = page.locator("#undoBtn");
  await page.keyboard.type("QZlink https://example.org/qz ");
  // The newest step is the link, not the typing.
  await expect(undo).toHaveAttribute("aria-label", "Undo Link change");
  await page.keyboard.press(`${MOD}+z`);
  // One Cmd+Z removes only the link: the address and the space are still there,
  // and the step before it is the typing.
  await expect(undo).toHaveAttribute("aria-label", "Undo Typing");
  await expect.poll(() => mirror(page)).toContain("QZlink https://example.org/qz");
  expect(consoleErrors).toEqual([]);
});

test("Enter finishes a word: the correction lands on the paragraph left behind", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("QZent teh");
  await page.keyboard.press("Enter");
  await page.keyboard.type("QZnext");
  await expect.poll(() => mirror(page)).toContain("QZent the");
  // The caret stayed in the new paragraph: what was typed after Enter is not
  // glued onto the corrected one.
  await expect.poll(() => mirror(page)).not.toContain("theQZnext");
  await expect.poll(() => mirror(page)).toContain("QZnext");
  expect(consoleErrors).toEqual([]);
});

test("Ctrl/Cmd+Shift+Space types a non-breaking space", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("QZ10");
  await page.keyboard.press(`${MOD}+Shift+Space`);
  await page.keyboard.type("kg");
  await expect.poll(() => mirror(page)).toContain("QZ10\u00a0kg");
  expect(consoleErrors).toEqual([]);
});

test("the Symbol dialog's special characters insert a no-break space too", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("QZsym");
  await runPaletteCommand(page, "insert.symbol", "Symbol");
  await expect(page.locator("#symbolDialog")).toBeVisible();
  await page.locator("#symbolSearch").fill("no-break space");
  const cell = page.locator('#symbolGrid .glyph-cell[data-glyph="\u00a0"]');
  await expect(cell).toHaveAttribute("aria-label", "No-break space");
  await cell.click();
  await page.keyboard.press("Escape");
  await expect.poll(() => mirror(page)).toContain("QZsym\u00a0");
  expect(consoleErrors).toEqual([]);
});

test("the AutoCorrect options dialog: an added entry replaces as you type, and Reset removes it", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await runAppMenuCommand(page, "review", "tools.autoCorrectOptions");
  const dialog = page.getByRole("dialog", { name: "AutoCorrect options" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Replace", { exact: true }).fill("qzomw");
  await dialog.getByLabel("With", { exact: true }).fill("on my way");
  await page.locator("#autoCorrectAdd").click();
  await expect(page.getByRole("listbox", { name: "Replacements" }).locator('option[value="qzomw"]')).toHaveCount(1);
  await expect(page.locator("#autoCorrectReset")).toBeEnabled();
  await page.locator("#autoCorrectDone").click();
  await expect(dialog).toBeHidden();

  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.type(" QZx qzomw ");
  await expect.poll(() => mirror(page)).toContain("QZx on my way");
  // One Cmd+Z gives back what was typed.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => mirror(page)).toContain("QZx qzomw");

  // Reset forgets the entry; the same word now stays as typed.
  await runPaletteCommand(page, "tools.autoCorrectOptions", "AutoCorrect options");
  await expect(dialog).toBeVisible();
  await page.locator("#autoCorrectReset").click();
  await expect(page.locator('#autoCorrectList option[value="qzomw"]')).toHaveCount(0);
  await page.keyboard.press("Escape");
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.type(" qzomw ");
  await expect.poll(() => mirror(page)).toMatch(/QZx qzomw\s+qzomw/);
  expect(consoleErrors).toEqual([]);
});

test("a table cell's first letter follows the cell switch", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  // On by default: the cell's first word is capitalised.
  await page.keyboard.type("qzcellone ");
  await expect.poll(() => mirror(page)).toContain("Qzcellone");

  // Off in the dialog: the next cell's first word is left as typed, while the
  // sentence switch is still on.
  await runPaletteCommand(page, "tools.autoCorrectOptions", "AutoCorrect options");
  const cells = page.locator('#autoCorrectDialog [data-autocorrect-switch="capitaliseCells"]');
  await cells.uncheck();
  await expect(page.locator('#autoCorrectDialog [data-autocorrect-switch="capitalise"]')).toBeChecked();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Tab");
  await page.keyboard.type("qzcelltwo ");
  await expect.poll(() => mirror(page)).toContain("qzcelltwo");
  expect(consoleErrors).toEqual([]);
});
