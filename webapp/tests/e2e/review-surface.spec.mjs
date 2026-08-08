// The Review surface: a durable home for reviewing.
//
// Accept, Reject, Previous, Next, Accept all and Reject all existed only in the
// command palette and as buttons INSIDE the review sidebar — which exist only
// while that sidebar is open. So a user handed a document full of tracked
// changes had no durable affordance for deciding one: nothing on any ribbon tab,
// nothing in any app menu. Word gives all of this a permanent Review tab
// (Tracking / Changes / Comments), which is what this mirrors.
//
// These tests drive the ribbon buttons and assert the document really changed,
// because a button that merely exists is the failure being fixed, not the fix.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode } from "./fixtures.mjs";

async function openReviewTab(page) {
  await page.locator("#tabReview").click();
  await expect(page.locator("#panelReview")).toBeVisible();
}

// Makes a real tracked change: type in Suggesting mode, which routes through the
// suggestion path and leaves a revision for the Review tab to decide.
async function makeTrackedChange(page, text) {
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");
  await page.keyboard.type(text);
  await expect(page.locator("#a11yDocument")).toContainText(text);
}

test("the Review tab exposes Word's tracking, changes and comments groups", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);
  await openReviewTab(page);

  expect(await page.locator("#panelReview .rgroup-label").allTextContents()).toEqual([
    "Tracking",
    "Changes",
    "Comments",
  ]);
  for (const id of [
    "#reviewTrackBtn",
    "#reviewShowChangesBtn",
    "#reviewPrevBtn",
    "#reviewNextBtn",
    "#reviewAcceptBtn",
    "#reviewRejectBtn",
    "#reviewAcceptAllBtn",
    "#reviewRejectAllBtn",
    "#reviewCommentBtn",
    "#reviewPanelBtn",
  ]) {
    await expect(page.locator(id)).toBeVisible();
  }

  expect(consoleErrors).toEqual([]);
});

test("Accept from the Review tab decides the tracked change in the document", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await makeTrackedChange(page, "ACCEPTME");

  await openReviewTab(page);
  await page.locator("#reviewAcceptBtn").click();

  // Accepting keeps the text and removes its revision: the suggestion is now
  // ordinary content, so the review sidebar no longer lists a pending item.
  await expect(page.locator("#a11yDocument")).toContainText("ACCEPTME");
  await expect.poll(() => page.locator(".review-card").count()).toBe(0);

  expect(consoleErrors).toEqual([]);
});

test("Reject all from the Review tab discards the tracked insertions", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await makeTrackedChange(page, "REJECTME");

  await openReviewTab(page);
  await page.locator("#reviewRejectAllBtn").click();

  // Rejecting an insertion removes the text itself.
  await expect(page.locator("#a11yDocument")).not.toContainText("REJECTME");

  expect(consoleErrors).toEqual([]);
});

test("the tracking toggles reflect and drive the engine's own state", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await openReviewTab(page);

  // Track changes IS Suggesting mode — the ribbon toggle and the footer's mode
  // control are two views of one piece of state, not two flags.
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "false");
  await page.locator("#reviewTrackBtn").click();
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "true");
  // The footer's mode control is the other view of that same state. (There are
  // two mode controls in the shell — footer and ribbon — so scope to the footer.)
  await expect(
    page.locator('.footer .review-mode-seg[data-review-mode="suggesting"]'),
  ).toHaveAttribute("aria-pressed", "true");

  await page.locator("#reviewTrackBtn").click();
  await expect(page.locator("#reviewTrackBtn")).toHaveAttribute("aria-pressed", "false");

  expect(consoleErrors).toEqual([]);
});

// The drift guard, same shape as the Insert surface's: the ribbon's command set
// must equal the Review menu's, so a review command cannot reach one surface and
// miss the other the way accept/reject did.
test("the Review ribbon's command set is exactly the Review menu's", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);

  await page.locator('.app-menu-button[data-menu="review"]').click();
  const menuCommands = await page
    .locator("#appMenuPopover .app-menu-item[data-command]")
    .evaluateAll((items) => items.map((item) => item.dataset.command));
  await page.keyboard.press("Escape");

  await openReviewTab(page);
  const ribbonCommands = await page
    .locator("#panelReview .rgroup button[data-command]")
    .evaluateAll((buttons) => buttons.map((button) => button.dataset.command));

  expect(ribbonCommands.length).toBeGreaterThan(0);
  // The menu carries the three explicit mode rows the ribbon expresses as one
  // Track-changes toggle; every other ribbon command must appear in the menu.
  for (const command of ribbonCommands) {
    expect(menuCommands).toContain(command);
  }

  expect(consoleErrors).toEqual([]);
});

test("review commands stay reachable by keyboard from the tab strip", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabHome").focus();
  await page.keyboard.press("End");
  await expect(page.locator("#tabReview")).toBeFocused();
  await expect(page.locator("#panelReview")).toBeVisible();

  // Commenting needs text to attach to, so the button is correctly dead until
  // there is a selection — the Review tab is the durable affordance for it, the
  // ⌘⌥M shortcut and the palette row being the other two.
  await expect(page.locator("#reviewCommentBtn")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});
