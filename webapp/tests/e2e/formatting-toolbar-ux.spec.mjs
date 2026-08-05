// Formatting toolbar UX v2 (Q1–Q5): real dropdown swatch pickers for text
// color + highlight, a searchable font menu, paste-as-plain-text, an editable
// zoom control with Fit modes, and grow/shrink + change-case. These extend the
// existing toolbar coverage (they do not replace the reflection/tri-state suites
// in ribbon-home / history-formatting / paragraph-format-reflection).
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

// Selects `count` characters forward from the current caret.
async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

test("Q1: text-color picker opens, applies a standard swatch, and records a recent", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);

  // The raw OS color input is gone — the caret opens a real swatch menu.
  await page.locator("#textColor").click();
  const menu = page.locator("#textColorMenu");
  await expect(menu).toBeVisible();
  await expect(menu).toContainText("Standard colors");
  await expect(menu.locator(".color-row-action[data-auto]")).toContainText("Automatic");
  await page.screenshot({ path: "test-results/q1-text-color-grid.png" });

  await menu.locator('[data-color="#ff0000"]').click();
  await expect(menu).toBeHidden();
  // The "A" underline bar reflects the applied color.
  await expect
    .poll(() => page.locator("#textColorBar").evaluate((el) => getComputedStyle(el).backgroundColor))
    .toBe("rgb(255, 0, 0)");

  // Reopen: the color is now remembered under a "Recent" group.
  await page.locator("#textColor").click();
  await expect(menu).toContainText("Recent");
  await expect(menu.locator('.swatch-grid [data-color="#ff0000"]').last()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("Q1: highlight picker applies a named color and offers No color", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);

  await page.locator("#highlight").click();
  const menu = page.locator("#highlightMenu");
  await expect(menu).toBeVisible();
  await expect(menu.locator('[data-highlight="none"]')).toContainText("No color");
  await menu.locator('[data-highlight="green"]').click();
  await expect(menu).toBeHidden();
  await expect
    .poll(() => page.locator("#highlightBar").evaluate((el) => getComputedStyle(el).backgroundColor))
    .toBe("rgb(0, 255, 0)");
  expect(consoleErrors).toEqual([]);
});

test("Q2: font menu filters by search and applies a font, reflecting the choice", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 6);

  await page.locator("#fontFamily").click();
  const menu = page.locator("#fontMenu");
  await expect(menu).toBeVisible();
  await expect(page.locator("#fontMenuInput")).toBeFocused();
  await page.screenshot({ path: "test-results/q2-font-menu.png" });

  // Type-to-filter narrows the list to matching faces.
  await page.locator("#fontMenuInput").fill("georg");
  const rows = menu.locator(".font-menu-item");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveText(/Georgia/);
  // Each name renders in its own typeface.
  await expect(rows.first()).toHaveCSS("font-family", /Georgia/);

  // Keyboard: Enter applies the active row.
  await page.locator("#fontMenuInput").press("Enter");
  await expect(menu).toBeHidden();
  await expect(page.locator("#fontFamilyLabel")).toHaveText("Georgia");

  // Reopen: the chosen font is now under "Recently used".
  await page.locator("#fontFamily").click();
  await expect(menu).toContainText("Recently used");
  expect(consoleErrors).toEqual([]);
});

test("Q3: Ctrl/Cmd+Shift+V pastes clipboard text as plain text", async ({
  page,
  context,
  consoleErrors,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const marker = "PLAINPASTEXYZ";
  await page.evaluate((text) => navigator.clipboard.writeText(text), marker);
  await page.keyboard.press(`${MOD}+Shift+V`);

  // The model-derived accessibility tree mirrors inserted content.
  await expect(page.locator("#a11yDocument")).toContainText(marker);
  expect(consoleErrors).toEqual([]);
});

test("Q4: zoom accepts a typed percentage, a Fit mode, and rejects garbage", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const zoom = page.locator("#zoom");
  const pageCanvas = page.locator(".page-wrap .page").first();
  const baseWidth = (await pageCanvas.boundingBox()).width;

  // Typed percentage applies and re-renders larger.
  await zoom.fill("150%");
  await zoom.press("Enter");
  await expect(zoom).toHaveValue("150%");
  await expect.poll(async () => (await pageCanvas.boundingBox()).width).toBeGreaterThan(baseWidth * 1.3);

  // Fit width via the presets menu.
  await page.locator("#zoomMenuBtn").click();
  const menu = page.locator("#zoomMenu");
  await expect(menu).toBeVisible();
  await menu.locator('[data-zoom-mode="fit-width"]').click();
  await expect(menu).toBeHidden();
  await expect(zoom).toHaveValue("Fit width");

  // Garbage input is rejected and the last valid display is restored.
  await zoom.fill("banana");
  await zoom.press("Enter");
  await expect(zoom).not.toHaveValue("banana");
  expect(consoleErrors).toEqual([]);
});

test("Q5: grow/shrink step the font size and change-case transforms the selection", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Type a known word, select it, and confirm grow/shrink move the size.
  await page.keyboard.type("hello world");
  await selectBackward(page, "hello world".length);

  const size = page.locator("#fontSize");
  const before = Number(await size.inputValue());
  await page.locator("#growFont").click();
  await expect.poll(async () => Number(await size.inputValue())).toBeGreaterThan(before);
  const grown = Number(await size.inputValue());
  await page.locator("#shrinkFont").click();
  await expect.poll(async () => Number(await size.inputValue())).toBeLessThan(grown);

  // Change case: UPPERCASE the selected word.
  await page.locator("#changeCaseBtn").click();
  const caseMenu = page.locator("#changeCaseMenu");
  await expect(caseMenu).toBeVisible();
  await caseMenu.locator('[data-case="upper"]').click();
  await expect(caseMenu).toBeHidden();
  await expect(page.locator("#a11yDocument")).toContainText("HELLO WORLD");
  expect(consoleErrors).toEqual([]);
});

// Selects `count` characters backward from the caret (used after typing).
async function selectBackward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowLeft");
}
