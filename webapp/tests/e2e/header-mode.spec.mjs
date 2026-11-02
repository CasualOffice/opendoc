// The editing-mode selector at the top right (`109` UX-025).
//
// All three references put Editing / Suggesting / Viewing in the TOP chrome —
// ONLYOFFICE at the right of the tab row, Google Docs at the right of the toolbar,
// Word at the right of the ribbon — and none puts it in a status bar. The header
// carries Docs' "Editing ▾", and since `docs/184` D6 it is the ONLY mode switch on
// screen: the status bar's control is gone (`shell-sparse.spec.mjs` holds that).
//
// The defect this must not reintroduce is UX-016's: two controls for one state
// that can disagree. So the rows ARE the published state — `setReviewMode` checks
// them, `reflectReviewModeAccess` refuses them with a reason — and every change
// made elsewhere (the palette, Ctrl+Shift+E, Review ▸ Track changes) is read off
// the header.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  stableBox,
  runPaletteCommand,
  MOD,
} from "./fixtures.mjs";

const row = (page, mode) => page.locator(`#headerModeMenu [data-review-mode="${mode}"]`);

test("the header names the mode in force, and choosing a row changes it", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const button = page.locator("#headerModeBtn");
  await expect(button).toBeVisible();
  await expect(button).toHaveAccessibleName("Editing mode Editing");
  // Top right: in the header's second row, right of the tab strip.
  const btn = await stableBox(button);
  const strip = await stableBox(page.locator(".ribbon-nav"));
  expect(btn.x).toBeGreaterThan(strip.x + strip.width - 1);
  expect(btn.y + btn.height).toBeLessThanOrEqual(strip.y + strip.height + 2);

  await button.click();
  const menu = page.locator("#headerModeMenu");
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitemradio")).toHaveCount(3);
  await expect(row(page, "editing")).toHaveAttribute("aria-checked", "true");

  await row(page, "suggesting").click();
  await expect(menu).toBeHidden();
  await expect(row(page, "suggesting")).toHaveAttribute("aria-checked", "true");
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#headerModeText")).toHaveText("Suggesting");
  await expect(button).toHaveAttribute("aria-expanded", "false");
  // The mode change goes back to the document, as the palette's does.
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("a change made on any other surface is what the header shows", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await runPaletteCommand(page, "review.mode.viewing", "viewing mode");
  await expect(page.locator("#headerModeText")).toHaveText("Read only");
  await expect(page.locator("#headerModeBtn")).toHaveAttribute("data-mode", "viewing");
  await runPaletteCommand(page, "review.mode.editing", "editing mode");
  await expect(page.locator("#headerModeText")).toHaveText("Editing");
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Shift+E`);
  await expect(page.locator("#headerModeText")).toHaveText("Suggesting");
  await page.locator("#headerModeBtn").click();
  await expect(row(page, "suggesting")).toHaveAttribute("aria-checked", "true");
  await expect(row(page, "editing")).toHaveAttribute("aria-checked", "false");
});

test("it is keyboard-operable: opens on the checked row, arrows move, Enter chooses, Escape returns", async ({
  page,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const button = page.locator("#headerModeBtn");
  await button.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#headerModeMenu")).toBeVisible();
  await expect(row(page, "editing")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "suggesting")).toBeFocused();
  await page.keyboard.press("End");
  await expect(row(page, "viewing")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "editing")).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(row(page, "viewing")).toHaveAttribute("aria-checked", "true");
  await expect(page.locator("#headerModeMenu")).toBeHidden();

  await button.focus();
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "viewing")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("#headerModeMenu")).toBeHidden();
  await expect(button).toBeFocused();
  await expect(row(page, "viewing")).toHaveAttribute("aria-checked", "true");
});

test("a mode the reader may not enter is a disabled row with its reason, and changes nothing", async ({
  page,
}) => {
  // A `readonly` container: `reflectReviewModeAccess` refuses Editing and
  // Suggesting on the header's rows WITH a reason.
  await gotoEditor(page, "&mode=readonly");
  await page.locator("#headerModeBtn").click();
  await expect(row(page, "editing")).toHaveAttribute("aria-disabled", "true");
  await expect(row(page, "editing")).toBeDisabled();
  const reason = await row(page, "editing").getAttribute("title");
  expect(reason).toBeTruthy();
  // Still FOCUSABLE — the reason is on the row, and a keyboard user has to be
  // able to land on it to hear it.
  await row(page, "editing").focus();
  await expect(row(page, "editing")).toBeFocused();
  // `force`, because the actionability check refuses an `aria-disabled` target —
  // a person's click is not refused, and it is the click that must do nothing.
  await row(page, "editing").click({ force: true });
  await expect(row(page, "viewing")).toHaveAttribute("aria-checked", "true");
  await expect(page.locator("#headerModeText")).toHaveText("Read only");
});

test("withholding the review region takes the mode selector and its menu", async ({
  page,
}) => {
  await page.goto("/editor.html?fixture=rich&mode=owner&chrome=-review");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"));
  await expect(page.locator("#headerModeControl")).toBeHidden();
  await expect(page.locator("#headerModeMenu")).toBeHidden();
});

test("it speaks the interface's language", async ({ page }) => {
  await page.goto("/editor.html?fixture=rich&lang=de");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"));
  await expect(page.locator("#headerModeText")).toHaveText("Bearbeiten");
  await page.locator("#headerModeBtn").click();
  await expect(row(page, "suggesting")).toContainText("Vorschlagen");
});
