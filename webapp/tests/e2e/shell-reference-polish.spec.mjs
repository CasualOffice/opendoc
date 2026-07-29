import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

test("the reference typography and icons load locally", async ({ page, consoleErrors }) => {
  const chromeFontRequests = [];
  const remoteGoogleFontRequests = [];
  page.on("request", (request) => {
    const url = request.url();
    if (url.includes("/assets/fonts/")) chromeFontRequests.push(url);
    if (/fonts\.(?:googleapis|gstatic)\.com/.test(url)) remoteGoogleFontRequests.push(url);
  });

  await gotoEditor(page);
  await page.evaluate(() => document.fonts.ready);

  const typography = await page.evaluate(() => ({
    body: getComputedStyle(document.body).fontFamily,
    symbol: getComputedStyle(document.querySelector(".ms")).fontFamily,
    interReady: document.fonts.check("13px Inter"),
    symbolsReady: document.fonts.check('18px "Material Symbols Outlined"'),
  }));

  expect(typography.body).toMatch(/^Inter\b/);
  expect(typography.symbol).toMatch(/^"Material Symbols Outlined"|^Material Symbols Outlined/);
  expect(typography.interReady).toBe(true);
  expect(typography.symbolsReady).toBe(true);
  expect(chromeFontRequests.some((url) => url.endsWith("inter-latin-400-700.woff2"))).toBe(true);
  expect(chromeFontRequests.some((url) => url.endsWith("material-symbols-outlined.woff2"))).toBe(
    true,
  );
  expect(remoteGoogleFontRequests).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("the visible Search control opens the real command palette and restores focus", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const trigger = page.locator("#searchTrigger");
  await expect(trigger).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");

  await trigger.click();
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await expect(page.locator("#cmdInput")).toBeFocused();
  await expect(trigger).toHaveAttribute("aria-expanded", "true");

  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
  await expect(trigger).toBeFocused();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");
  expect(consoleErrors).toEqual([]);
});

test("the no-document state keeps only useful top-bar actions", async ({ page, consoleErrors }) => {
  await page.goto("/editor.html?blank=1");

  await expect(page.locator("body")).not.toHaveClass(/doc-loaded/);
  await expect(page.locator(".file")).toBeVisible();
  await expect(page.locator("#settingsBtn")).toBeVisible();
  await expect(page.locator("#searchTrigger")).toBeHidden();
  await expect(page.locator("#save")).toBeHidden();
  await expect(page.locator("#propertiesBtn")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("the selection formatting toolbar follows light and dark editor surfaces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);

  const toolbar = page.locator("#selToolbar");
  await expect(toolbar).toBeVisible();

  const colors = async () =>
    page.evaluate(() => ({
      toolbarBackground: getComputedStyle(document.getElementById("selToolbar")).backgroundColor,
      toolbarColor: getComputedStyle(document.getElementById("selToolbar")).color,
      surfaceBackground: getComputedStyle(document.querySelector(".bar")).backgroundColor,
      documentColor: getComputedStyle(document.body).color,
    }));

  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
  const light = await colors();
  expect(light.toolbarBackground).toBe(light.surfaceBackground);
  expect(light.toolbarColor).toBe(light.documentColor);

  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  const dark = await colors();
  expect(dark.toolbarBackground).toBe(dark.surfaceBackground);
  expect(dark.toolbarColor).toBe(dark.documentColor);
  expect(dark.toolbarBackground).not.toBe(light.toolbarBackground);
  expect(consoleErrors).toEqual([]);
});

for (const width of [720, 390]) {
  test(`the editor header does not create page overflow at ${width}px`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 800 });
    await gotoEditor(page);

    const metrics = await page.evaluate(() => ({
      viewport: window.innerWidth,
      document: document.documentElement.scrollWidth,
      header: document.querySelector(".bar").scrollWidth,
    }));
    expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
    expect(metrics.header).toBeLessThanOrEqual(metrics.viewport);
    await expect(page.locator("#searchTrigger")).toBeVisible();
    await expect(page.locator("#railOutline")).toContainText("Outline");
    expect(consoleErrors).toEqual([]);
  });
}
