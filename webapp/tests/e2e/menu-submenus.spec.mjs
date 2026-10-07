import { test, expect } from "./fixtures.mjs";
import { appMenuButton, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// The owner's report was about LENGTH, not about grouping: "my ask was to group
// them and create sub menus .. so it's readable". Named bands with separators
// still leave every row on screen — Format listed 31 rows and Table 28.
//
// So what these guard is the shape a reader meets: a top-level menu short
// enough to scan, with the rest one level in, reachable by pointer, keyboard
// and touch alike.
const LONGEST_TOP_LEVEL = 13;

async function openCompactMenu(page, menu) {
  if (await page.locator("#modeCompact").isVisible()) await page.locator("#modeCompact").click();
  await page.locator(`.app-menu-button[data-menu="${menu}"]`).click();
  await expect(page.locator("#appMenuPopover")).toBeVisible();
}

/** Rows at the top level — a flyout's rows are not among them. */
const topLevelRows = (page) =>
  page
    .locator("#appMenuPopover .app-menu-item")
    .evaluateAll((els) => els.filter((el) => !el.closest(".app-menu-item-flyout")).length);

test("no menu makes a reader scan more than a screenful", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  const seen = {};
  for (const menu of ["file", "edit", "view", "insert", "references", "format", "table", "review"]) {
    const trigger = page.locator(`.app-menu-button[data-menu="${menu}"]`);
    if (!(await trigger.count())) continue;
    await openCompactMenu(page, menu);
    seen[menu] = await topLevelRows(page);
    await page.keyboard.press("Escape");
  }
  // Format and Table are the two the owner named; they are the proof the rest
  // of the rule is worth having.
  expect(seen.format, "Format was 31 rows").toBeLessThanOrEqual(10);
  expect(seen.table, "Table was 28 rows").toBeLessThanOrEqual(12);
  for (const [menu, rows] of Object.entries(seen)) {
    expect(rows, `${menu} lists ${rows} rows at the top level`).toBeLessThanOrEqual(LONGEST_TOP_LEVEL);
  }
  expect(consoleErrors).toEqual([]);
});

test("a submenu opens by pointer and by keyboard, and closing returns the focus", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await openCompactMenu(page, "format");
  const parent = page.locator("#appMenuPopover .app-menu-item-parent").first();
  await expect(parent).toHaveAttribute("aria-haspopup", "menu");
  await expect(parent).toHaveAttribute("aria-expanded", "false");

  await parent.hover();
  await expect(page.locator("#appMenuPopover .app-menu-item-flyout:not([hidden])")).toHaveCount(1);
  await expect(parent).toHaveAttribute("aria-expanded", "true");

  // AND IT IS ON SCREEN. `hidden` being false is not the same thing: the
  // popover scrolls, an overflow container clips on both axes, and the first
  // version of this shipped a flyout 173px of which was never painted while
  // every state assertion passed. So ask the document what is at the flyout's
  // own top-left corner — if something else answers, it is behind a clip.
  const painted = await page.evaluate(() => {
    const flyout = document.querySelector("#appMenuPopover .app-menu-item-flyout:not([hidden])");
    const box = flyout.getBoundingClientRect();
    const at = document.elementFromPoint(Math.round(box.left + 8), Math.round(box.top + 8));
    return {
      insideViewport: box.left >= 0 && box.right <= window.innerWidth && box.width > 0,
      hitsItself: !!at && flyout.contains(at),
    };
  });
  expect(painted.insideViewport, "the flyout must be inside the window").toBe(true);
  expect(painted.hitsItself, "the flyout must be the thing at its own corner, not clipped").toBe(true);

  // Keyboard: Right opens and lands INSIDE, Left closes and comes back OUT. The
  // pointer is moved away first because hovering the parent legitimately
  // reopens it, and a guard that cannot tell the two apart proves nothing.
  await page.mouse.move(4, 4);
  await parent.focus();
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(() => page.evaluate(() => !!document.activeElement?.closest(".app-menu-item-flyout")))
    .toBe(true);
  await page.keyboard.press("ArrowLeft");
  await expect
    .poll(() =>
      page.evaluate(() => document.activeElement?.classList.contains("app-menu-item-parent") ?? false),
    )
    .toBe(true);
  await expect(page.locator("#appMenuPopover .app-menu-item-flyout:not([hidden])")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("a command inside a submenu runs the command", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  if (await page.locator("#modeCompact").isVisible()) await page.locator("#modeCompact").click();
  await clickIntoFirstPage(page);
  await page.keyboard.type("SUBMENUROW");
  await page.keyboard.press("Shift+Home");
  const before = await page.locator("#bold").getAttribute("aria-pressed");

  await page.locator('.app-menu-button[data-menu="format"]').click();
  await page.locator("#appMenuPopover .app-menu-item-parent").first().hover();
  const row = page.locator("#appMenuPopover .app-menu-item-flyout:not([hidden]) .app-menu-item").first();
  await row.click();

  // The document changed, and the menu got out of the way — the two halves of
  // "a row ran". Asserting the popover closed alone would pass on a dead row.
  await expect(page.locator("#appMenuPopover")).toBeHidden();
  await expect.poll(() => page.locator("#bold").getAttribute("aria-pressed")).not.toBe(before);
  expect(consoleErrors).toEqual([]);
});

test("on a phone a submenu drills inline and nothing scrolls sideways", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);
  // Through the header's menus sheet, the phone's route to the bar (docs/148 §5.3b).
  await (await appMenuButton(page, "format")).click();
  await expect(page.locator("#appMenuPopover")).toBeVisible();
  // Touch has no hover, so the click path is the only one — and it must open.
  await page.locator("#appMenuPopover .app-menu-item-parent").first().click();

  const geometry = await page.evaluate(() => {
    const flyout = document.querySelector("#appMenuPopover .app-menu-item-flyout:not([hidden])");
    const popover = document.querySelector("#appMenuPopover");
    return {
      open: !!flyout,
      position: flyout ? getComputedStyle(flyout).position : null,
      right: flyout ? flyout.getBoundingClientRect().right : null,
      windowWidth: window.innerWidth,
      popoverScrollsX: popover.scrollWidth > popover.clientWidth,
      pageScrollsX: document.documentElement.scrollWidth > document.documentElement.clientWidth,
    };
  });

  expect(geometry.open, "the click path must open a submenu where there is no hover").toBe(true);
  // A sideways flyout in a bottom sheet is either off screen or a horizontal
  // scroller, and no horizontal scrolling is the property the phone layout is
  // guarded on (`docs/148`). So it drills: static, in flow, under its parent.
  expect(geometry.position).toBe("static");
  expect(geometry.right).toBeLessThanOrEqual(geometry.windowWidth);
  expect(geometry.popoverScrollsX, "the menu must not scroll sideways").toBe(false);
  expect(geometry.pageScrollsX, "the page must not scroll sideways").toBe(false);
  expect(consoleErrors).toEqual([]);
});
