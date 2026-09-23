import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  MOD,
  openCommandPalette,
  openAppMenu,
  openFilePage,
  expectEditorFocused,
} from "./fixtures.mjs";

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

test("the command palette opens from the Help menu and restores focus on close", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // There is no longer a Search box in the top bar — it duplicated the palette,
  // which is the surface Word and Docs both use. What must survive is the
  // contract the box was guarding: a POINTER user can open the palette, and
  // closing it puts focus back where it came from rather than on <body>.
  // The palette's clickable route is the File surface now: "Find a command" was
  // a Help-menu row, and Help is a File group, as it is in ONLYOFFICE.
  await expect(page.locator("#tabFile")).toBeVisible();
  await expect(page.locator("#tabFile")).toHaveAttribute("aria-selected", "false");

  await openCommandPalette(page);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await expect(page.locator("#cmdInput")).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
  // Focus returns somewhere real, not to the document body — the regression this
  // test has always existed to catch. Running a File-page row closes the page,
  // so "somewhere real" is the editing surface rather than the row, which no
  // longer exists to receive it.
  await expectEditorFocused(page);
  await expect(page.locator("#tabFile")).toHaveAttribute("aria-selected", "false");
  expect(consoleErrors).toEqual([]);
});

test("the no-document state keeps only useful top-bar actions", async ({ page, consoleErrors }) => {
  await page.goto("/editor.html?blank=1");

  await expect(page.locator("body")).not.toHaveClass(/doc-loaded/);
  // `.file` was the label wrapping the hidden file input — removed with the Open
  // button it belonged to. A navigation axis is what must be present with no
  // document loaded, because it is the only route to Open — and EXACTLY one, or
  // the doubled-navigation defect is back (docs/122).
  //
  // In the empty state that axis is the menu bar in both chromes: the band is
  // hidden here, so the tab strip is not on screen to be the axis. ONLYOFFICE
  // arrives at the same shape from the other side — their read-only viewport
  // declares the File tab and nothing else.
  await expect(page.locator(".app-menu-bar")).toBeVisible();
  await expect(page.locator(".ribbon-tabs")).toBeHidden();
  await expect(page.locator("#settingsBtn")).toBeVisible();
  // Open, Save and Search are no longer top-bar controls at all: they duplicated
  // the File menu and the palette. Assert they are ABSENT rather than hidden —
  // `toBeHidden()` also passes for a node that does not exist, so it would have
  // gone green either way and proved nothing.
  await expect(page.locator("#searchTrigger")).toHaveCount(0);
  await expect(page.locator("#openBtn")).toHaveCount(0);
  await expect(page.locator("#save")).toHaveCount(0);
  await expect(page.locator("#propertiesBtn")).toBeHidden();
  // The capabilities are still reachable, which is the half that matters: the
  // File menu is present and offers Open even with no document loaded.
  await openAppMenu(page, "file");
  await expect(
    page.locator('#appMenuPopover .app-menu-item[data-command="file.open"]'),
  ).toBeEnabled();
  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("the selection formatting toolbar follows light and dark panel surfaces", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+a`);

  const toolbar = page.locator("#selToolbar");
  await expect(toolbar).toBeVisible();

  // This test used to compare the floating toolbar against the HEADER. That
  // stopped being the right comparison when the chrome adopted one grey field:
  // header, ribbon, status bar and desk share `--bg`, and `--surface` is
  // reserved for the page and for things that FLOAT over it. The toolbar is one
  // of those, so the rule to guard is that it uses the floating-panel surface —
  // comparing it to the header would now assert the opposite of the design.
  const colors = async () =>
    page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const bar = getComputedStyle(document.getElementById("selToolbar"));
      return {
        toolbarBackground: bar.backgroundColor,
        toolbarColor: bar.color,
        surface: root.getPropertyValue("--surface").trim(),
        chromeField: getComputedStyle(document.querySelector(".bar")).backgroundColor,
        documentColor: getComputedStyle(document.body).color,
      };
    });

  // A token name in CSS and a resolved rgb() string are not comparable, so
  // resolve the token through a throwaway element painted with it.
  const resolve = (token) =>
    page.evaluate((t) => {
      const probe = document.createElement("div");
      probe.style.backgroundColor = `var(${t})`;
      document.body.appendChild(probe);
      const value = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return value;
    }, token);

  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
  const light = await colors();
  expect(light.toolbarBackground).toBe(await resolve("--surface"));
  expect(light.toolbarColor).toBe(light.documentColor);
  // And it is NOT the chrome field — that is the whole point of a floating panel.
  expect(light.toolbarBackground).not.toBe(light.chromeField);

  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  const dark = await colors();
  expect(dark.toolbarBackground).toBe(await resolve("--surface"));
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
    // The header must still offer its essential affordance at this width. That
    // used to be the Search box; it is now the menu bar, which is the only route
    // to Open, Save and the palette, so dropping it at a narrow width would
    // strand the user completely.
    await expect(page.locator("#tabFile")).toBeVisible();
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
