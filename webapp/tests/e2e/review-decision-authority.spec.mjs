// HF-199: a commentor may suggest, and may not decide.
//
// The host contract has required `edit` for every accept/reject command since SDK
// phase 2, as Word's reviewer and Google Docs' Commenter are both refused; the
// chrome let a `commentor` container decide anyway. These read the EFFECT: a
// suggestion the commentor made is still there after every route that used to
// reject it, and the controls say why rather than doing nothing.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

const REFUSAL =
  "Accepting or rejecting a change edits the document, and this editor may only comment here.";
const MARK = "Zqxv";

const mirrorHas = (page, text) =>
  page.evaluate((needle) => (document.getElementById("a11yDocument")?.textContent || "").includes(needle), text);

test("a commentor's suggestion survives every route to Reject, and each says why", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page, "&mode=commentor");
  // A commentor container opens in Suggesting, so this typing is a suggestion.
  await expect(page.locator('#reviewModeControl [data-review-mode="suggesting"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.type(MARK);
  await expect.poll(() => mirrorHas(page, MARK)).toBe(true);

  // The Review band: disabled, with the reason on the control.
  await page.locator('[data-tab="review"]').click();
  for (const id of ["#reviewRejectAllBtn", "#reviewAcceptAllBtn", "#reviewRejectBtn", "#reviewAcceptBtn"]) {
    await expect(page.locator(id)).toBeDisabled();
    await expect(page.locator(id)).toHaveAttribute("title", REFUSAL);
  }

  // The palette: the row is there, disabled, and carries the reason as its hint.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Reject all changes");
  const row = page.locator('#cmdList [data-command-id="review.rejectAll"]');
  await expect(row).toHaveJSProperty("disabled", true);
  await expect(row).toHaveAttribute("title", REFUSAL);
  await page.keyboard.press("Escape");

  // The chord for Reject ▸ Next, with the caret back in the suggestion.
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press(`${MOD}+Alt+Backspace`);
  await expect(page.locator("#status")).toContainText(REFUSAL);

  // THE EFFECT: after all of that, the suggestion is still in the document.
  expect(await mirrorHas(page, MARK)).toBe(true);
  expect(consoleErrors).toEqual([]);
});

test("an edit container still decides, so the gate is the grant and not the mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page, "&mode=edit");
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.type(MARK);
  await expect.poll(() => mirrorHas(page, MARK)).toBe(true);
  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewRejectAllBtn")).toBeEnabled();
  await page.locator("#reviewRejectAllBtn").click();
  await expect.poll(() => mirrorHas(page, MARK)).toBe(false);
  expect(consoleErrors).toEqual([]);
});
