// Review ▸ Display for review (`docs/153` review.display-mode-simple-markup,
// review.display-mode-original, review.display-mode-final; `docs/109` RM-31).
//
// Word's four views of a reviewed document, driven through the real picker on
// the Review band, and each asserted by what the canvas then SHOWS rather than
// by the picker's own state: the inline markers, the change bars, the markup
// layout's body class, and — for Original — the document text itself, which must
// read as if the tracked insertion had been rejected while the live document
// still holds it.
import { test, expect, gotoEditor, clickIntoFirstPage, menuCommandRow, setReviewMode } from "./fixtures.mjs";

const INSERTED = "ZQXW_INSERTED ";

async function pick(page, mode) {
  await page.locator("#tabReview").click();
  await expect(page.locator("#panelReview")).toBeVisible();
  await page.locator("#reviewDisplayBtn").click();
  const menu = page.locator("#reviewDisplayMenu");
  await expect(menu).toBeVisible();
  await menu.locator(`[data-display-mode="${mode}"]`).click();
  await expect(menu).toBeHidden();
}

test("the four review displays show what Word's do, and Original is the rejected document, read-only", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");
  await page.keyboard.type(INSERTED);
  await expect(page.locator("#a11yDocument")).toContainText(INSERTED.trim());
  await setReviewMode(page, "editing");
  // Suggesting turned the markup on (review UX Q1), so the picker says All markup.
  await expect(page.locator("body")).toHaveClass(/showing-changes/);
  await expect(page.locator(".review-insertion-marker").first()).toBeVisible();

  // Simple markup: the text as accepted, a bar in the margin, no inline markers.
  // Mutation: paint the revision markers in every mode (`overlayFor`) -> the
  // insertion-marker count is not 0.
  await pick(page, "simple");
  await expect(page.locator("body")).not.toHaveClass(/showing-changes/);
  await expect(page.locator(".review-change-bar").first()).toBeVisible();
  await expect(page.locator(".review-insertion-marker")).toHaveCount(0);
  await page.locator("#reviewDisplayBtn").click();
  await expect(page.locator('#reviewDisplayMenu [data-display-mode="simple"]')).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");

  // Clicking a bar shows all markup — Word's gesture.
  await page.locator(".review-change-bar").first().click();
  await expect(page.locator("body")).toHaveClass(/showing-changes/);

  // No markup: neither markers nor bars.
  await pick(page, "none");
  await expect(page.locator(".review-change-bar")).toHaveCount(0);
  await expect(page.locator(".review-insertion-marker")).toHaveCount(0);

  // Original: every change rejected, so the insertion is gone from what is
  // shown, and the copy refuses an edit by saying it is the original.
  // Mutation: skip `showAsOriginal()` on the copy -> the inserted text is still
  // in the accessible mirror. So does dropping the chrome refresh from
  // `showVersionPreview`: the canvas swaps but the mirror reads the live copy.
  await pick(page, "original");
  await expect(page.locator("#a11yDocument")).not.toContainText(INSERTED.trim());
  await clickIntoFirstPage(page);
  await page.keyboard.press("x");
  await expect(page.locator("#a11yDocument")).not.toContainText(INSERTED.trim());

  // Original is read-only, so the editor composes the READING chrome
  // (`capabilities.mjs` READING_REGIONS, `docs/126`): the menu bar instead of the
  // ribbon, one navigation axis. The ribbon picker is gone by design, and the way
  // back must be the menu bar's Display for review flyout, on screen and enabled.
  await expect(page.locator("#reviewDisplayBtn")).toBeHidden();
  const back = await menuCommandRow(page, "review", "review.displayMode.allMarkup");
  await expect(back).toBeEnabled();
  // Leaving Original puts the live document back, the suggestion intact.
  await back.click();
  await expect(page.locator("#reviewDisplayBtn")).toBeVisible();
  await expect(page.locator("#a11yDocument")).toContainText(INSERTED.trim());
  await expect(page.locator("body")).toHaveClass(/showing-changes/);

  expect(consoleErrors).toEqual([]);
});

test("the four displays are palette commands too", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const ids = await page.evaluate(() =>
    window.opendoc.describe().commands.map((command) => command.id).filter((id) => id.startsWith("review.displayMode.")),
  );
  expect(ids.sort()).toEqual([
    "review.displayMode.allMarkup",
    "review.displayMode.noMarkup",
    "review.displayMode.original",
    "review.displayMode.simpleMarkup",
  ]);
  expect(consoleErrors).toEqual([]);
});
