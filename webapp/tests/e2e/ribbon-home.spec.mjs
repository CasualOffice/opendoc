import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// docs/64 — the Home ribbon mirrors template.png: a single no-wrap band of
// labeled groups. Two hard rules this suite guards:
//   1. the ribbon NEVER shows a horizontal scrollbar — groups that don't fit
//      collapse into the "⋯" overflow menu instead;
//   2. every icon-only control has a delayed name+shortcut tooltip (docs/64 §3).
// It also checks that the rebuilt controls stay functional (the live Styles
// gallery applies a real style).

async function ribbonHasNoHScroll(page) {
  return page.locator('.ribbon-panel[data-panel="home"]').evaluate(
    (el) => el.scrollWidth <= el.clientWidth + 1,
  );
}

test("the Home ribbon never horizontally scrolls; narrow widths collapse groups into the overflow menu", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Wide (the default 1280 test viewport): the whole band fits, no overflow
  // control, and — crucially — no horizontal scrollbar.
  await expect(page.locator("#ribbonOverflowBtn")).toBeHidden();
  expect(await ribbonHasNoHScroll(page)).toBe(true);
  // Spec-critical groups are inline at this width.
  await expect(page.locator('#reviewModeControl [data-review-mode="suggesting"]')).toBeVisible();

  // Narrow: groups that don't fit move into the "⋯" menu — still no scrollbar.
  await page.setViewportSize({ width: 760, height: 720 });
  await expect(page.locator("#ribbonOverflowBtn")).toBeVisible();
  expect(await ribbonHasNoHScroll(page)).toBe(true);

  // The overflowed controls remain reachable through the menu.
  await page.locator("#ribbonOverflowBtn").click();
  const menu = page.locator("#ribbonOverflowMenu");
  await expect(menu).toBeVisible();
  await expect(menu.locator(".rgroup-label", { hasText: "Review" })).toBeVisible();
  await expect(
    menu.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toBeVisible();

  await page.setViewportSize({ width: 1280, height: 720 });
  expect(consoleErrors).toEqual([]);
});

test("icon-only ribbon controls show a delayed name + shortcut tooltip", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const tooltip = page.locator(".ribbon-tooltip");
  await expect(tooltip).toBeHidden();

  // Hovering Bold reveals the custom tooltip (after its ~350ms delay) with the
  // control name and its keyboard shortcut chip.
  await page.locator("#bold").hover();
  await expect(tooltip).toBeVisible({ timeout: 2000 });
  await expect(tooltip).toContainText("Bold");
  await expect(tooltip.locator("kbd")).toContainText("B");

  // Moving away hides it and restores the native title for accessibility.
  await page.mouse.move(0, 0);
  await expect(tooltip).toBeHidden();
  await expect(page.locator("#bold")).toHaveAttribute("title", /Bold/);

  expect(consoleErrors).toEqual([]);
});

test("the ribbon collapses to a compact tab strip and expands again", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const ribbon = page.locator(".ribbon");
  const body = page.locator(".ribbon-body");
  const toggle = page.locator("#ribbonViewToggle");
  await expect(body).toBeVisible();

  // Collapse to compact view: the group band hides, only the tab strip remains.
  await toggle.click();
  await expect(ribbon).toHaveClass(/is-collapsed/);
  await expect(body).toBeHidden();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");

  // Clicking a tab brings the full ribbon back (Word behavior).
  await page.locator("#tabInsert").click();
  await expect(ribbon).not.toHaveClass(/is-collapsed/);
  await expect(body).toBeVisible();

  // The explicit toggle also expands/collapses directly.
  await toggle.click();
  await expect(body).toBeHidden();
  await toggle.click();
  await expect(body).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("the live Styles gallery applies a real document style", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const gallery = page.locator("#stylesGallery");
  await expect(gallery.locator(".style-card").first()).toBeVisible();

  // Apply the first offered style; the gallery reflects the active style back
  // (its card becomes aria-selected), proving the click ran a real edit that
  // the reflection path picked up.
  const firstCard = gallery.locator(".style-card").first();
  const styleName = await firstCard.getAttribute("data-style");
  await firstCard.click();
  await expect(
    gallery.locator(`.style-card[data-style="${styleName}"]`),
  ).toHaveAttribute("aria-selected", "true");
  // The hidden reflection select mirrors the same value.
  await expect(page.locator("#paragraphStyle")).toHaveValue(styleName);

  expect(consoleErrors).toEqual([]);
});
