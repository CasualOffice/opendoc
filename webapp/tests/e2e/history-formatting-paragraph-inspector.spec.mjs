import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  runAppMenuCommand,
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
  // Through the Format menu. This used to reach ¶ through the ribbon's "⋯"
  // overflow, which at 390px no longer exists: the phone tier (docs/148) runs
  // the compact chrome and there is no ribbon to overflow. `layout.paragraph`
  // has a Format menu row in either chrome, so the route is one the width
  // cannot take away — and the subject here is the PANEL, not the button.
  await runAppMenuCommand(page, "format", "layout.paragraph");

  const panel = page.locator("#paragraphPropertiesPanel");
  await expect(panel).toBeVisible();

  // THE GUARANTEE: the inspector is entirely on screen. It was written as four
  // pinned numbers — x >= 34 (a rail column that is now a strip above the
  // document), right <= 382, bottom <= 692 — which described a right-hand
  // drawer beside a 34px rail. At the phone rung the same panel is a bottom
  // sheet spanning the window, which satisfies "viewport-bounded" better than
  // the numbers did and fails every one of them. The defect those numbers were
  // written for is a panel hanging off an edge, so that is what is asserted.
  const bounds = await panel.boundingBox();
  const window = await page.evaluate(() => ({ w: innerWidth, h: innerHeight }));
  expect(bounds.x, "the panel starts inside the window").toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width, "the panel ends inside the window").toBeLessThanOrEqual(window.w + 1);
  expect(bounds.y, "the panel starts below the top edge").toBeGreaterThanOrEqual(0);
  expect(bounds.y + bounds.height, "the panel ends above the bottom edge").toBeLessThanOrEqual(window.h + 1);
  // And it is a panel over the document, not a takeover: the document is still
  // there to inspect. That is what the old `x >= 34` was really protecting.
  expect(bounds.height, "the panel leaves the document some screen").toBeLessThan(window.h * 0.8);
  await expect(page.locator("#viewport")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
