import {
  test,
  expect,
  definedParagraphStyles,
  gotoEditor,
  clickIntoFirstPage,
  reflectedParagraphStyle,
  MOD,
} from "./fixtures.mjs";

// docs/64 — the Home ribbon mirrors template.png: a single no-wrap band of
// labeled groups. Two hard rules this suite guards:
//   1. the ribbon NEVER shows a horizontal scrollbar — groups that don't fit
//      collapse into the "⋯" overflow menu instead;
//   2. every icon-only control has a delayed name+shortcut tooltip (docs/64 §3).
// It also checks that the rebuilt controls stay functional (the live Styles
// gallery applies a real style).

async function ribbonHasNoHScroll(page) {
  return page
    .locator('.ribbon-panel[data-panel="home"]')
    .evaluate((el) => el.scrollWidth <= el.clientWidth + 1);
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
  await expect(
    page.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toBeVisible();
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
      const label = document
        .querySelector(`#${id} .fmt-big-label`)
        .getBoundingClientRect();
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
    const highlight = document
      .querySelector(".color-control-highlight")
      .getBoundingClientRect();
    const fontGroup = document
      .querySelector('[data-group="font"]')
      .getBoundingClientRect();
    return fontGroup.right - highlight.right;
  });
  expect(highlightDividerClearance).toBeGreaterThanOrEqual(12);

  await page
    .locator('#ribbonReviewModeControl [data-review-mode="suggesting"]')
    .click();
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
  await expect(
    menu.locator(".rgroup-label", { hasText: "Paragraph" }),
  ).toBeVisible();
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

test("ribbon tabs use roving focus and arrow-key activation", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabHome").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#tabInsert")).toBeFocused();
  await expect(page.locator("#tabInsert")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#tabHome")).toHaveAttribute("tabindex", "-1");
  await expect(page.locator("#panelInsert")).toBeVisible();

  // End goes to the last ENABLED tab. That is View: the strip ends File · … ·
  // Review · View · Table, with Table contextual and disabled until the caret is
  // in one (ONLYOFFICE's and Word's order — docs/122). This asserts the
  // roving-focus contract, not a fixed roster, so it moves with the tab strip
  // rather than pinning it.
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

test("the Styles gallery applies a real style and stays inside the band's width budget", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const trigger = page.locator("#stylesTrigger");
  await expect(trigger).toBeVisible();
  await trigger.click();
  const menu = page.locator("#stylesMenu");
  // A SHORT list, capped at six — what Docs offers and the bottom of Word's visible
  // gallery range (docs/115). `styles-control.spec.mjs` owns the list's composition;
  // here the concern is only that this band still holds it.
  // The menu lists every paragraph style the document defines; the SHORT part
  // is the suggested group promoted above them (`docs/115`, revised). Which
  // styles land in that group is `styles-control.spec.mjs`'s subject — here
  // the concern is only that this band still holds the control.
  const rows = await menu.locator(".style-option").count();
  const defined = (await definedParagraphStyles(page)).length;
  expect(defined).toBeGreaterThan(4);
  expect(rows).toBe(defined);
  await page.keyboard.press("Escape");

  // The width budget, stated as a number rather than as "the same width as the
  // control above it" — there is no control above it any more. 234px is what the
  // group measured when it still carried the select, and the Home band had 10px of
  // slack at 1280px, so the group must not grow past that or a whole group is exiled
  // into the "⋯" overflow. The no-horizontal-scrollbar rule at the top of this file
  // is the other half of the same guarantee.
  const groupWidth = await page.evaluate(
    () =>
      document.querySelector('[data-group="styles"]').getBoundingClientRect()
        .width,
  );
  expect(groupWidth).toBeLessThanOrEqual(234);

  // Apply the first offered style; the trigger reflects it back by NAME, proving
  // the click ran a real edit that the reflection path picked up.
  await trigger.click();
  const firstRow = menu.locator(".style-option").first();
  const styleName = await firstRow.getAttribute("data-style");
  await firstRow.click();
  await expect(page.locator("#stylesTriggerLabel")).toHaveText(styleName);
  // And the control publishes the same answer for the rest of the chrome.
  await expect.poll(() => reflectedParagraphStyle(page)).toBe(styleName);

  expect(consoleErrors).toEqual([]);
});

// Reads the computed (weight, px size, style) of every menu row's label — what the
// user actually sees rendered in each row. Requires the menu to be open.
async function galleryCardLooks(page) {
  return page.$$eval("#stylesMenu .style-option .style-option-name", (labels) =>
    labels.map((el) => {
      const cs = getComputedStyle(el);
      return {
        style: el.closest(".style-option").dataset.style,
        weight: cs.fontWeight,
        size: cs.fontSize,
        italic: cs.fontStyle,
      };
    }),
  );
}

test("each Styles menu row is drawn IN its own style (model-driven preview)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#stylesTrigger").click();
  const looks = await galleryCardLooks(page);
  // Every row in the menu, not just the promoted six: each one has to be drawn
  // in its own style, because that is the reason the list is worth opening.
  expect(looks.length).toBeGreaterThan(1);
  // Every row's label carries an inline preview weight (the engine-resolved
  // style drove it), never the bare default only.
  for (const look of looks) {
    expect(["400", "450", "500", "600", "650", "700"]).toContain(look.weight);
  }
  // The rows genuinely differ — a real visual hierarchy, not a list of identical
  // labels: at least two distinct (weight, size) pairs across the menu.
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

  const before = (await definedParagraphStyles(page)).length;

  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Create style from selection");
  await page
    .locator(".cmd-item", { hasText: "Create style from selection" })
    .first()
    .click();

  const dialog = page.locator("#styleNameDialog");
  await expect(dialog).toBeVisible();
  await page.locator("#styleNameInput").fill("E2E Callout");
  await page.locator("#styleNameConfirm").click();
  await expect(dialog).toBeHidden();

  // The style registry gained the new style and the caret's paragraph now uses it.
  await expect
    .poll(async () => (await definedParagraphStyles(page)).length)
    .toBe(before + 1);
  await expect.poll(() => reflectedParagraphStyle(page)).toBe("E2E Callout");
  expect(
    (await definedParagraphStyles(page)).filter((s) => s === "E2E Callout"),
  ).toHaveLength(1);
  // A brand-new style is a style the document is now USING, so the band offers it —
  // otherwise creating a style would leave it unreachable from the control that
  // created it (docs/115 §5.3).
  await expect(page.locator("#stylesTriggerLabel")).toHaveText("E2E Callout");
  await page.locator("#stylesTrigger").click();
  await expect(
    page.locator('#stylesMenu .style-option[data-style="E2E Callout"]'),
  ).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("Update <style> to match selection reflows the style and its menu preview", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Home`);
  await page.keyboard.press("Shift+End");

  const styleName = await reflectedParagraphStyle(page);
  expect(styleName).not.toBe("");

  // The caret's style ALWAYS has a row now (docs/115 §5.3 pins it into the offered
  // set), so this is asserted rather than guarded by an `if` — a conditional here would
  // have quietly skipped the only assertion that proves the reflow.
  const rowName = () =>
    page.locator(
      `#stylesMenu .style-option[data-style="${styleName}"] .style-option-name`,
    );
  const weightOfRow = async () => {
    await page.locator("#stylesTrigger").click();
    const weight = await rowName().evaluate(
      (el) => getComputedStyle(el).fontWeight,
    );
    await page.keyboard.press("Escape");
    return weight;
  };
  await page.locator("#stylesTrigger").click();
  await expect(rowName()).toHaveCount(1);
  await page.keyboard.press("Escape");
  const weightBefore = await weightOfRow();

  // Toggle bold on the selection, then redefine the style to match it.
  await page.keyboard.press(`${MOD}+b`);
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("match selection");
  await page
    .locator(".cmd-item", { hasText: "match selection" })
    .first()
    .click();

  // The redefined style is still applied and its menu row's rendered weight changed
  // to match the new definition (proving every paragraph using the style now reflows
  // through the new run props, and that the row's preview is rebuilt on a DEFINITION
  // change and not only when the offered NAMES change).
  await expect.poll(() => reflectedParagraphStyle(page)).toBe(styleName);
  await expect.poll(weightOfRow).not.toBe(weightBefore);

  expect(consoleErrors).toEqual([]);
});
