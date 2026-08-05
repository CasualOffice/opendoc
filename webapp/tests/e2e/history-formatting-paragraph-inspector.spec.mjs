import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

test("history labels and mixed run formatting reflect engine state", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.keyboard.type("AB");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Typing",
  );
  await page.locator("#undoBtn").click();
  await expect(page.locator("#redoBtn")).toHaveAttribute(
    "aria-label",
    "Redo Typing",
  );
  await page.locator("#redoBtn").click();

  // Format only B, then extend over AB. Whether the heading's inherited bold is
  // initially on or off, the two characters now disagree and must read Mixed.
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#bold").click();
  await page.locator("#pages").focus();
  await page.keyboard.press("Shift+ArrowLeft");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "mixed");

  // Activating a mixed toggle applies it to the entire selection.
  await page.locator("#bold").click();
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Formatting",
  );

  const fontSize = page.locator("#fontSize");
  await fontSize.fill("13.5");
  await fontSize.press("Tab");
  await expect(fontSize).toHaveValue("13.5");

  // Highlight is now a swatch-picker dropdown (Q1): open it and pick Yellow.
  await page.locator("#highlight").click();
  await expect(page.locator("#highlightMenu")).toBeVisible();
  await page.locator('#highlightMenu [data-highlight="yellow"]').click();
  await expect(page.locator("#highlightMenu")).toBeHidden();
  await expect
    .poll(() =>
      page.locator("#highlightBar").evaluate((el) => getComputedStyle(el).backgroundColor),
    )
    .toBe("rgb(255, 255, 0)");

  await page.locator("#superscript").click();
  await expect(page.locator("#superscript")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.locator("#superscript").click();
  await expect(page.locator("#superscript")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  await expect(page.locator("#subscript")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
  expect(consoleErrors).toEqual([]);
});

test("paragraph properties use a rounded live inspector with per-action undo", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const trigger = page.locator("#paraOptsBtn");
  const panel = page.locator("#paragraphPropertiesPanel");
  await trigger.click();
  await expect(panel).toBeVisible();
  await expect(page.locator("#paragraphPropertiesContext")).toContainText(
    "paragraph",
  );
  await expect(page.locator("#paraOptsMenu")).toHaveCount(0);
  await expect(panel.getByText("Changes apply automatically")).toBeVisible();
  await expect(panel.locator('button:has-text("Apply")')).toHaveCount(0);
  await expect(panel.locator('button:has-text("Reset")')).toHaveCount(0);
  await expect(panel).toHaveCSS("border-radius", "10px");

  const left = page.locator("#indentLeft");
  const original = await left.inputValue();
  await left.fill("0.25");
  await left.press("Tab");
  await expect(left).toHaveValue("0.25");
  await expect(page.locator("#undoBtn")).toHaveAttribute(
    "aria-label",
    "Undo Paragraph formatting",
  );
  await page.locator("#undoBtn").click();
  await expect(left).toHaveValue(original);

  await left.focus();
  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(trigger).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("paragraph inspector stays viewport-bounded on a narrow editor", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // At this narrow width the Paragraph group collapses into the ribbon's "⋯"
  // overflow menu (docs/64 — no horizontal scrollbar); open it to reach ¶.
  const paraOpts = page.locator("#paraOptsBtn");
  if (!(await paraOpts.isVisible())) await page.locator("#ribbonOverflowBtn").click();
  await paraOpts.click();

  const panel = page.locator("#paragraphPropertiesPanel");
  await expect(panel).toBeVisible();
  const bounds = await panel.boundingBox();
  expect(bounds.x).toBeGreaterThanOrEqual(34);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(382);
  expect(bounds.y).toBeGreaterThanOrEqual(0);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(692);
  await expect(panel).toHaveCSS("border-radius", "10px");
  await expect(page.locator("#viewport")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
