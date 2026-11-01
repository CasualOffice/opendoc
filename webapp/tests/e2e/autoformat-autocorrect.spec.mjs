// docs/174 TA-05 and TA-06, with real keystrokes: typing "1. " or "- " at the
// start of a paragraph makes a real list, and AutoCorrect fixes "teh", turns
// "--" into a dash and "(c)" into ©, and capitalises a sentence. Each conversion
// is its OWN undo step, so the very next Cmd+Z gives back exactly what was
// typed (Word, Docs and ONLYOFFICE all do this). Both are switches in Settings.
import { MOD, clickIntoFirstPage, expect, gotoEditor, test } from "./fixtures.mjs";

/** A fresh empty paragraph at the end of the document, in Editing mode. */
async function freshParagraph(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.press("Enter");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "false");
}

const mirror = (page) => page.locator("#a11yDocument").innerText();

test("1. and a space start a numbered list; the next Undo gives back the literal 1.", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("1. ");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.type("QZfirst");
  await expect.poll(() => mirror(page)).toContain("QZfirst");
  await expect.poll(() => mirror(page)).not.toContain("1. QZfirst");

  // Undo the typing after the conversion, then the conversion itself.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => mirror(page)).not.toContain("QZfirst");
  await page.keyboard.press(`${MOD}+z`);
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "false");
  // The literal marker is back, caret after it: typing continues the text.
  await page.keyboard.type("QZafter");
  await expect.poll(() => mirror(page)).toContain("1. QZafter");
  expect(consoleErrors).toEqual([]);
});

test("- and a space start a bulleted list", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("- QZitem");
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => mirror(page)).not.toContain("- QZitem");
  expect(consoleErrors).toEqual([]);
});

test("AutoCorrect fixes typos, dashes, symbols and sentence case, each undone by one Cmd+Z", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.keyboard.type("qzone teh well--maybe (c) ok. qztwo ");
  await expect.poll(() => mirror(page)).toContain("Qzone the well—maybe © ok. Qztwo");

  // The last correction (capitalising "qztwo") is the newest undo step.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => mirror(page)).toContain("ok. qztwo");
  // The caret is still after the space, so typing simply continues.
  await page.keyboard.type("QZend");
  await expect.poll(() => mirror(page)).toContain("ok. qztwo QZend");
  expect(consoleErrors).toEqual([]);
});

test("switched off in Settings, nothing is converted", async ({ page, consoleErrors }) => {
  await freshParagraph(page);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  // Settings shows one topic per page (approved design D3, docs/184).
  await page.locator('#settingsPanel [data-settings-tab="proofing"]').click();
  await page.locator("#autocorrectToggle").uncheck();
  await page.locator("#autoFormatListsToggle").uncheck();
  await page.keyboard.press("Escape");
  await expect(page.locator("#settingsPanel")).toBeHidden();
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await page.keyboard.type("1. qzone teh ");
  await expect.poll(() => mirror(page)).toContain("1. qzone teh");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "false");
  expect(consoleErrors).toEqual([]);
});
