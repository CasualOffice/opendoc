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
test("every Review ribbon command is reachable from the menu bar", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await gotoEditor(page);

  // This used to require the Review MENU specifically. That was an
  // implementation detail standing in for the real rule, which is docs/105's
  // command-surface parity: no capability may be reachable from only one
  // surface. Pinning it to one menu also forced duplication — editing mode had
  // to sit in both View and Review to satisfy this test and Docs' taxonomy at
  // once — so the guard was arguing for the repetition it was not written to
  // cause. The requirement is that a ribbon command has a menu home, not which
  // menu that is; the home is reported below so a failure names it.
  const MENUS = ["file", "edit", "view", "insert", "format", "table", "review", "tools", "help"];
  const home = new Map();
  for (const menu of MENUS) {
    await page.locator(`.app-menu-button[data-menu="${menu}"]`).click();
    const ids = await page
      .locator("#appMenuPopover .app-menu-item[data-command]")
      .evaluateAll((items) => items.map((item) => item.dataset.command));
    for (const id of ids) if (!home.has(id)) home.set(id, menu);
    await page.keyboard.press("Escape");
  }

  await openReviewTab(page);
  const ribbonCommands = await page
    .locator("#panelReview .rgroup button[data-command]")
    .evaluateAll((buttons) => buttons.map((button) => button.dataset.command));

  expect(ribbonCommands.length).toBeGreaterThan(0);
  const homeless = ribbonCommands.filter((id) => !home.has(id));
  expect(
    homeless,
    "a Review ribbon command with no menu home is reachable from the ribbon " +
      "and the palette only — the single-surface defect this guard exists for",
  ).toEqual([]);

  // Tracked-change OPERATIONS must still live in Review; only mode selection
  // and Add comment were moved (to View and Insert, following Docs), so this
  // pins the part of the taxonomy that should not drift again.
  for (const id of ["review.acceptAll", "review.rejectAll", "review.next", "review.previous"]) {
    if (ribbonCommands.includes(id)) {
      expect(home.get(id), `${id} belongs in the Review menu`).toBe("review");
    }
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
