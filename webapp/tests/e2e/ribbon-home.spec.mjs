import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

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

  // Wide (the default 1280 test viewport): the whole corrected band fits, no
  // overflow control, and — crucially — no horizontal scrollbar.
  await expect(page.locator("#ribbonOverflowBtn")).toBeHidden();
  expect(await ribbonHasNoHScroll(page)).toBe(true);
  // Editing mode follows the Vellum reference into the footer and remains
  // visible independently of Home-band overflow; the requested Home control is
  // mirrored from the same mode state.
  await expect(page.locator('#reviewModeControl [data-review-mode="suggesting"]')).toBeVisible();
  await expect(page.locator("#ribbonReviewModeControl")).toBeVisible();

  // Undo/Redo occupy distinct rows; Clipboard and Editing expose their authored
  // icons rather than appearing as text-only/empty commands.
  const undoBox = await page.locator("#undoBtn").boundingBox();
  const redoBox = await page.locator("#redoBtn").boundingBox();
  expect(redoBox.y).toBeGreaterThan(undoBox.y);
  await expect(page.locator("#pasteBtn .ms")).toHaveText("content_paste");
  await expect(page.locator("#copyBtn .ms")).toHaveText("content_copy");
  await expect(page.locator("#findBtn .ms")).toHaveText("search");
  await expect(page.locator("#replaceBtn .ms")).toHaveText("find_replace");
  const tileContentFits = await page.evaluate(() =>
    ["cutBtn", "copyBtn", "findBtn", "replaceBtn"].every((id) => {
      const button = document.getElementById(id).getBoundingClientRect();
      const icon = document.querySelector(`#${id} .ms`).getBoundingClientRect();
      const label = document.querySelector(`#${id} .fmt-big-label`).getBoundingClientRect();
      return (
        icon.left >= button.left &&
        icon.right <= button.right &&
        label.left >= button.left &&
        label.right <= button.right &&
        label.top > icon.top
      );
    }),
  );
  expect(tileContentFits).toBe(true);

  const highlightDividerClearance = await page.evaluate(() => {
    const highlight = document.querySelector(".color-control-highlight").getBoundingClientRect();
    const fontGroup = document.querySelector('[data-group="font"]').getBoundingClientRect();
    return fontGroup.right - highlight.right;
  });
  expect(highlightDividerClearance).toBeGreaterThanOrEqual(12);

  await page.locator('#ribbonReviewModeControl [data-review-mode="suggesting"]').click();
  await expect(
    page.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toHaveAttribute("aria-pressed", "true");
  await page.locator('#reviewModeControl [data-review-mode="editing"]').click();
  await expect(
    page.locator('#ribbonReviewModeControl [data-review-mode="editing"]'),
  ).toHaveAttribute("aria-pressed", "true");

  // Narrow: groups that don't fit move into the "⋯" menu — still no scrollbar.
  await page.setViewportSize({ width: 760, height: 720 });
  await expect(page.locator("#ribbonOverflowBtn")).toBeVisible();
  expect(await ribbonHasNoHScroll(page)).toBe(true);

  // The overflowed controls remain reachable through the menu.
  await page.locator("#ribbonOverflowBtn").click();
  const menu = page.locator("#ribbonOverflowMenu");
  await expect(menu).toBeVisible();
  await expect(menu.locator(".rgroup-label", { hasText: "Paragraph" })).toBeVisible();
  await expect(menu.locator("#alignCenter")).toBeVisible();

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

test("overflowed icon controls keep tooltips and the command surface restores focus on Escape", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.setViewportSize({ width: 760, height: 720 });

  const trigger = page.locator("#ribbonOverflowBtn");
  const menu = page.locator("#ribbonOverflowMenu");
  await trigger.click();
  await expect(menu).toBeVisible();
  await expect(page.locator("#fontFamily")).toBeFocused();

  await page.locator("#alignCenter").hover();
  await expect(page.locator(".ribbon-tooltip")).toContainText("Center");

  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(trigger).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("ribbon tabs use roving focus and arrow-key activation", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabHome").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#tabInsert")).toBeFocused();
  await expect(page.locator("#tabInsert")).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#tabHome")).toHaveAttribute("tabindex", "-1");
  await expect(page.locator("#panelInsert")).toBeVisible();

  await page.keyboard.press("End");
  await expect(page.locator("#tabView")).toBeFocused();
  await expect(page.locator("#panelView")).toBeVisible();
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

test("the Styles selector exposes every style and the quick gallery applies a real style", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const gallery = page.locator("#stylesGallery");
  await expect(gallery.locator(".style-card").first()).toBeVisible();
  await expect(gallery.locator(".style-card")).toHaveCount(4);
  expect(await page.locator("#paragraphStyle option").count()).toBeGreaterThan(4);
  const styleWidths = await page.evaluate(() => [
    document.querySelector("#paragraphStyle").getBoundingClientRect().width,
    document.querySelector("#stylesGallery").getBoundingClientRect().width,
  ]);
  expect(Math.abs(styleWidths[0] - styleWidths[1])).toBeLessThanOrEqual(1);

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

// Reads the computed (weight, px size, style) of every gallery card's label —
// what the user actually sees rendered in each card.
async function galleryCardLooks(page) {
  return page.$$eval("#stylesGallery .style-card .style-card-name", (labels) =>
    labels.map((el) => {
      const cs = getComputedStyle(el);
      return {
        style: el.closest(".style-card").dataset.style,
        weight: cs.fontWeight,
        size: cs.fontSize,
        italic: cs.fontStyle,
      };
    }),
  );
}

test("each Styles gallery card is drawn IN its own style (model-driven preview)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const looks = await galleryCardLooks(page);
  expect(looks.length).toBe(4);
  // Every card's label carries an inline preview weight (the engine-resolved
  // style drove it), never the bare default only.
  for (const look of looks) {
    expect(["400", "450", "500", "600", "650", "700"]).toContain(look.weight);
  }
  // The cards genuinely differ — a real visual hierarchy, not four identical
  // labels: at least two distinct (weight, size) pairs across the gallery.
  const distinct = new Set(looks.map((l) => `${l.weight}/${l.size}`));
  expect(distinct.size).toBeGreaterThanOrEqual(2);

  expect(consoleErrors).toEqual([]);
});

test("Create style from selection adds a new paragraph style and applies it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // Select the first line so the new style captures real run formatting.
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("Shift+End");

  const before = await page.locator("#paragraphStyle option").count();

  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Create style from selection");
  await page.locator(".cmd-item", { hasText: "Create style from selection" }).first().click();

  const dialog = page.locator("#styleNameDialog");
  await expect(dialog).toBeVisible();
  await page.locator("#styleNameInput").fill("E2E Callout");
  await page.locator("#styleNameConfirm").click();
  await expect(dialog).toBeHidden();

  // The style registry gained the new style and the caret's paragraph now uses it.
  await expect(page.locator("#paragraphStyle option")).toHaveCount(before + 1);
  await expect(page.locator("#paragraphStyle")).toHaveValue("E2E Callout");
  await expect(page.locator('#paragraphStyle option[value="E2E Callout"]')).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

test("Update <style> to match selection reflows the style and its gallery preview", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("Shift+End");

  const styleName = await page.locator("#paragraphStyle").inputValue();
  expect(styleName).not.toBe("");

  const cardName = () =>
    page.locator(`#stylesGallery .style-card[data-style="${styleName}"] .style-card-name`);
  const looksInGallery = (await cardName().count()) > 0;

  const weightBefore = looksInGallery
    ? await cardName().evaluate((el) => getComputedStyle(el).fontWeight)
    : null;

  // Toggle bold on the selection, then redefine the style to match it.
  await page.keyboard.press(`${MOD}+b`);
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("match selection");
  await page.locator(".cmd-item", { hasText: "match selection" }).first().click();

  // The redefined style is still applied and, when previewed in the gallery, the
  // card's rendered weight changed to match the new definition (proving every
  // paragraph using the style now reflows through the new run props).
  await expect(page.locator("#paragraphStyle")).toHaveValue(styleName);
  if (looksInGallery) {
    await expect
      .poll(() => cardName().evaluate((el) => getComputedStyle(el).fontWeight))
      .not.toBe(weightBefore);
  }

  expect(consoleErrors).toEqual([]);
});
