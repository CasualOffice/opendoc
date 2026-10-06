// The editing-mode selector at the top right (`109` UX-025).
//
// All three references put Editing / Suggesting / Viewing in the TOP chrome —
// ONLYOFFICE at the right of the tab row, Google Docs at the right of the toolbar,
// Word at the right of the ribbon — and none puts it only in a status bar, which
// is where ours was. The header now carries Docs' "Editing ▾", and the status
// bar's segments stay as the second face.
//
// The defect this must not reintroduce is UX-016's: two controls for one state
// that can disagree. So every test here crosses the two faces — a change made on
// one is read off the other — and the header is asserted to hold no state of its
// own: choosing a row PRESSES the status bar's segment, and a segment that
// cannot be pressed is a row that cannot be chosen, with the segment's reason.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  expectEditorFocused,
  stableBox,
  MOD,
} from "./fixtures.mjs";

const segment = (page, mode) => page.locator(`#reviewModeControl [data-review-mode="${mode}"]`);
const row = (page, mode) => page.locator(`#headerModeMenu [data-header-mode="${mode}"]`);

test("the header names the mode in force, and choosing a row changes it on both faces", async ({
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
  await expect(segment(page, "suggesting")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#headerModeText")).toHaveText("Suggesting");
  await expect(button).toHaveAttribute("aria-expanded", "false");
  // The mode change goes back to the document, as the status bar's does.
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("a change made on any other surface is what the header shows", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await segment(page, "viewing").click();
  await expect(page.locator("#headerModeText")).toHaveText("Read only");
  await expect(page.locator("#headerModeBtn")).toHaveAttribute("data-mode", "viewing");
  await segment(page, "editing").click();
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
  await expect(segment(page, "viewing")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#headerModeMenu")).toBeHidden();

  await button.focus();
  await page.keyboard.press("ArrowDown");
  await expect(row(page, "viewing")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("#headerModeMenu")).toBeHidden();
  await expect(button).toBeFocused();
  await expect(segment(page, "viewing")).toHaveAttribute("aria-pressed", "true");
});

test("a mode the reader may not enter is a disabled row with its reason, and changes nothing", async ({
  page,
}) => {
  // A `readonly` container: `reflectReviewModeAccess` disables Editing and
  // Suggesting on the status bar WITH a reason. The header must say the same and
  // do the same, because it reads them from there.
  await gotoEditor(page, "&mode=readonly");
  await expect(segment(page, "editing")).toBeDisabled();
  const reason = await segment(page, "editing").getAttribute("title");
  expect(reason).toBeTruthy();
  await page.locator("#headerModeBtn").click();
  await expect(row(page, "editing")).toHaveAttribute("aria-disabled", "true");
  await expect(row(page, "editing")).toBeDisabled();
  await expect(row(page, "editing")).toHaveAttribute("title", reason);
  // Still FOCUSABLE — the reason is on the row, and a keyboard user has to be
  // able to land on it to hear it.
  await row(page, "editing").focus();
  await expect(row(page, "editing")).toBeFocused();
  // `force`, because the actionability check refuses an `aria-disabled` target —
  // a person's click is not refused, and it is the click that must do nothing.
  await row(page, "editing").click({ force: true });
  await expect(segment(page, "viewing")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#headerModeText")).toHaveText("Read only");
});

test("withholding the review region takes the header face as well as the status bar's", async ({
  page,
}) => {
  await page.goto("/editor.html?fixture=rich&mode=owner&chrome=-review");
  await page.waitForFunction(() => document.body.dataset.chromeWithheld !== undefined);
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"));
  await expect(page.locator("#reviewModeControl")).toBeHidden();
  await expect(page.locator("#headerModeControl")).toBeHidden();
});

test("it speaks the interface's language", async ({ page }) => {
  await page.goto("/editor.html?fixture=rich&lang=de");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await page.waitForFunction(() => document.body.classList.contains("doc-loaded"));
  await expect(page.locator("#headerModeText")).toHaveText("Bearbeiten");
  await page.locator("#headerModeBtn").click();
  await expect(row(page, "suggesting")).toContainText("Vorschlagen");
});
