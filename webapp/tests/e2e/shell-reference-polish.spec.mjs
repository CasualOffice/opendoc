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

test("the outline inspector uses the shared rounded panel surface", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const trigger = page.locator("#railOutline");
  await expect(trigger).toBeEnabled();
  await trigger.click();

  const panel = page.locator("#outlinePanel");
  await expect(panel).toBeVisible();
  const style = await panel.evaluate((element) => {
    const computed = getComputedStyle(element);
    const bounds = element.getBoundingClientRect();
    return {
      radius: computed.borderRadius,
      borderTop: computed.borderTopWidth,
      borderRight: computed.borderRightWidth,
      borderBottom: computed.borderBottomWidth,
      borderLeft: computed.borderLeftWidth,
      shadow: computed.boxShadow,
      bottomGap: window.innerHeight - bounds.bottom,
    };
  });
  expect(style.radius).toBe("10px");
  expect([
    style.borderTop,
    style.borderRight,
    style.borderBottom,
    style.borderLeft,
  ]).toEqual(["1px", "1px", "1px", "1px"]);
  expect(style.shadow).not.toBe("none");
  expect(style.bottomGap).toBeGreaterThanOrEqual(8);
  expect(consoleErrors).toEqual([]);
});

test("outline navigation centers the heading target", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.locator("#railOutline").click();
  const item = page.locator("#outlineBody .outline-item").first();
  await expect(item).toBeVisible();

  await page.evaluate(() => {
    window.__outlineScrollCalls = [];
    const viewport = document.getElementById("viewport");
    const original = viewport.scrollTo.bind(viewport);
    viewport.scrollTo = function (options) {
      window.__outlineScrollCalls.push(options);
      return original(options);
    };
  });
  await item.click();
  await expect
    .poll(() => page.evaluate(() => window.__outlineScrollCalls.at(-1)?.behavior))
    .toBe("auto");
  await expect(item).toHaveClass(/is-active/);
  await expect(item).toHaveAttribute("aria-current", "location");
  expect(consoleErrors).toEqual([]);
});

// The shell must never give the document a horizontal scrollbar. `documentElement`
// alone is NOT a sufficient guard: any `overflow` value on an ancestor stops the
// spill from ever reaching it, so a footer that is 100px too wide reads as clean.
// A revision of this bar shipped exactly that — `overflow-x: clip` on `.footer`
// left `documentElement.scrollWidth` at the viewport while `.footer.scrollWidth`
// reported 861 — which is why the footer's live-control half is measured against
// its own box here too. `.foot-left` is deliberately NOT measured that way: it is
// a truncation region (the transient engine message ellipsizes inside it), so its
// `scrollWidth` legitimately exceeds its box whenever a long message is showing.
for (const width of [720, 390]) {
  test(`the editor shell does not create page overflow at ${width}px`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 800 });
    await gotoEditor(page);

    const metrics = await page.evaluate(() => {
      const box = (selector) => {
        const el = document.querySelector(selector);
        return { scroll: el.scrollWidth, client: el.clientWidth, right: el.getBoundingClientRect().right };
      };
      return {
        viewport: window.innerWidth,
        document: document.documentElement.scrollWidth,
        header: document.querySelector(".bar").scrollWidth,
        footer: box(".footer"),
        footRight: box(".foot-right"),
      };
    });
    expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
    expect(metrics.header).toBeLessThanOrEqual(metrics.viewport);
    // The strip and its live controls fit inside their own boxes, so no ancestor
    // `overflow` can launder a layout mistake into a passing assertion.
    expect(metrics.footer.scroll).toBeLessThanOrEqual(metrics.footer.client);
    expect(metrics.footRight.scroll).toBeLessThanOrEqual(metrics.footRight.client);
    expect(metrics.footRight.right).toBeLessThanOrEqual(metrics.viewport);
    await expect(page.locator("#searchTrigger")).toBeVisible();
    await expect(page.locator("#railOutline")).toContainText("Outline");
    expect(consoleErrors).toEqual([]);
  });
}

// The counts are the point of the status bar, so the ladder must not trade them
// away while the strip still has room. Word keeps its word count and page
// position at tablet widths; the character count shipped for that same parity.
test("the status bar keeps its counts and live controls at 720px", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 720, height: 800 });
  await gotoEditor(page);

  await expect(page.locator("#statWords")).toBeVisible();
  await expect(page.locator("#statChars")).toBeVisible();
  await expect(page.locator("#statPages")).toBeVisible();
  await expect(page.locator("#reviewModeControl")).toBeVisible();
  await expect(page.locator(".zoom")).toBeVisible();

  // Every mode segment is fully readable — not squeezed to an ellipsis — and the
  // zoom control is clickable rather than merely on-screen.
  const clipped = await page.evaluate(() =>
    [...document.querySelectorAll(".footer .review-mode-seg")].filter((el) => el.scrollWidth > el.clientWidth).length,
  );
  expect(clipped).toBe(0);
  await page.locator("#zoomIn").click();

  expect(consoleErrors).toEqual([]);
});
