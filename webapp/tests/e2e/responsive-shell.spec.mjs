// Narrow-window and floating-chrome geometry (HF-083, HF-088, HF-084, HF-097,
// HF-095, HF-096).
//
// Mobile and tablet browsers are a supported target (docs/18-SUPPORT-MATRIX.md),
// but every other e2e spec in this directory runs at >=1280px, so nothing in CI
// ever looked at the shell below a desktop width — which is how a page could
// render at the wrong aspect ratio and a comment column could eat the document
// without a single test noticing. These assertions are deliberately about
// measured geometry at a viewport size, not about which rules the stylesheet
// contains.
import { test, expect, stableBox, gotoEditor } from "./fixtures.mjs";

const viewportMetrics = (page) =>
  page.evaluate(() => {
    const v = document.getElementById("viewport");
    return { scrollWidth: v.scrollWidth, clientWidth: v.clientWidth, left: v.getBoundingClientRect().left };
  });

// ---- HF-083 -----------------------------------------------------------------
test("the sheet keeps its true size and aspect ratio in a window narrower than the page", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);

  const wrap = page.locator(".page-wrap").first();
  const wide = await stableBox(wrap);
  const wideRatio = wide.width / wide.height;

  // A window narrower than a Letter sheet at 100% zoom — split screen, tablet
  // portrait, the home-page embed.
  await page.setViewportSize({ width: 700, height: 900 });
  await expect.poll(async () => (await viewportMetrics(page)).clientWidth).toBeLessThan(720);

  const narrow = await stableBox(wrap);

  // The page renders at its real size: no axis was clamped without the other
  // following, so nothing is horizontally compressed.
  expect(narrow.width / narrow.height).toBeCloseTo(wideRatio, 3);
  expect(Math.abs(narrow.width - wide.width)).toBeLessThan(1);

  // The room it now needs is genuinely reachable: the viewport scrolls
  // horizontally, and — this is the part centering breaks — nothing has spilled
  // off the LEFT of the scroll origin, where no scrollbar can ever reach it.
  const metrics = await viewportMetrics(page);
  expect(metrics.scrollWidth).toBeGreaterThan(metrics.clientWidth);
  expect(narrow.x).toBeGreaterThanOrEqual(metrics.left - 1);

  // Scrolled fully right, the page's right edge comes into view.
  await page.evaluate(() => {
    const v = document.getElementById("viewport");
    v.scrollLeft = v.scrollWidth;
  });
  const scrolled = await stableBox(wrap);
  expect(scrolled.x + scrolled.width).toBeLessThanOrEqual(metrics.left + metrics.clientWidth + 1);

  expect(consoleErrors).toEqual([]);
});

// ---- HF-088 -----------------------------------------------------------------
test("opening comments below tablet width costs the document no width", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 700, height: 900 });
  await gotoEditor(page);

  const wrap = page.locator(".page-wrap").first();
  const closed = await stableBox(wrap);
  const before = await viewportMetrics(page);

  await page.locator("#railReview").click();
  await expect(page.locator("#reviewSidebar")).toBeVisible();

  // The reserved right gutter is gone at this width: the column no longer
  // charges the page 316px it does not have.
  const padding = await page.evaluate(
    () => getComputedStyle(document.getElementById("pages")).paddingRight,
  );
  expect(padding).toBe("0px");

  // So the document's own footprint is exactly what it was with comments shut —
  // same sheet width, same horizontal scroll extent, no dead scroll region.
  const open = await stableBox(wrap);
  expect(Math.abs(open.width - closed.width)).toBeLessThan(1);
  const after = await viewportMetrics(page);
  expect(Math.abs(after.scrollWidth - before.scrollWidth)).toBeLessThan(2);

  // And the column itself can no longer be the thing that forces the overflow.
  const sidebar = await stableBox(page.locator("#reviewSidebar"));
  expect(sidebar.width).toBeLessThanOrEqual(320);

  expect(consoleErrors).toEqual([]);
});

// ---- HF-084 -----------------------------------------------------------------
test("the object properties panel starts below the ribbon, expanded and collapsed", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => {
      const s = document.getElementById("status");
      return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );

  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();

  const panel = page.locator(".object-inspector");
  await expect(panel).toBeVisible();

  // The panel outranks the ribbon on z-index, so any overlap at all hides ribbon
  // commands — including the "⋯" overflow button at that same right edge.
  const ribbon = await stableBox(page.locator(".ribbon"));
  const overflow = page.locator(".ribbon-overflow-btn");
  const panelBox = await stableBox(panel);
  expect(panelBox.y).toBeGreaterThanOrEqual(ribbon.y + ribbon.height - 1);

  // If the ribbon is currently overflowing, the "⋯" button must be hit-testable
  // rather than painted over.
  if (await overflow.isVisible()) {
    const o = await stableBox(overflow);
    const top = await page.evaluate(
      ([x, y]) => document.elementFromPoint(x, y)?.closest(".ribbon-overflow-btn") !== null,
      [o.x + o.width / 2, o.y + o.height / 2],
    );
    expect(top).toBe(true);
  }

  // Collapsing the ribbon moves the chrome's bottom edge up, and the panel has
  // to follow it or it leaves a dead band under the tab strip.
  await page.locator("#ribbonViewToggle").click();
  await expect(page.locator(".ribbon")).toHaveClass(/is-collapsed/);
  const collapsedRibbon = await stableBox(page.locator(".ribbon"));
  const collapsedPanel = await stableBox(panel);
  expect(collapsedPanel.y).toBeGreaterThanOrEqual(collapsedRibbon.y + collapsedRibbon.height - 1);
  expect(collapsedPanel.y).toBeLessThan(panelBox.y);

  expect(consoleErrors).toEqual([]);
});

// ---- HF-097 -----------------------------------------------------------------
test("the menu bar shows that it has clipped Tools and Help rather than cutting them dead", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 460, height: 900 });
  await gotoEditor(page);

  const bar = page.locator("#appMenuBar");
  await expect(bar).toBeVisible();

  // Precondition: at this width the bar really is clipping menus.
  const overflowing = await bar.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
  expect(overflowing).toBe(true);

  // The clip is signalled instead of silent: the trailing edge fades out.
  const mask = await bar.evaluate((el) => {
    const s = getComputedStyle(el);
    return s.maskImage && s.maskImage !== "none" ? s.maskImage : s.webkitMaskImage;
  });
  expect(mask).toMatch(/gradient/);
  expect(mask).toMatch(/transparent|rgba\(0, 0, 0, 0\)/);

  // Help is still reachable by scrolling the bar — it was never removed.
  await bar.evaluate((el) => {
    el.scrollLeft = el.scrollWidth;
  });
  await expect(page.locator('.app-menu-button[data-menu="help"]')).toBeInViewport();

  expect(consoleErrors).toEqual([]);
});

// ---- HF-095 -----------------------------------------------------------------
test("the outline's current-location colour survives to Heading 3 and deeper", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#railOutline").click();
  await expect(page.locator("#outlinePanel")).toBeVisible();

  // The demo document may not carry an H3, and the defect is a cascade-order
  // one, so build the exact rows main.js emits and read what the browser
  // computes for them.
  await page.evaluate(() => {
    const body = document.getElementById("outlineBody");
    body.replaceChildren();
    for (const level of [1, 3, 6]) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `outline-item lvl-${level}`;
      b.dataset.probe = String(level);
      b.textContent = `Heading ${level}`;
      body.appendChild(b);
    }
  });

  const colorOf = (level, extra) =>
    page.evaluate(
      ([lvl, cls]) => {
        const el = document.querySelector(`[data-probe="${lvl}"]`);
        el.className = `outline-item lvl-${lvl}${cls}`;
        return getComputedStyle(el).color;
      },
      [level, extra],
    );

  // Resting: depth still reads quieter than the top level.
  const restingShallow = await colorOf(1, "");
  const restingDeep = await colorOf(3, "");
  expect(restingDeep).not.toBe(restingShallow);

  // Current location: an H3 and an H6 get the SAME accent an H1 gets. Before the
  // fix the depth rule won on source order and they stayed grey.
  const activeShallow = await colorOf(1, " is-active");
  expect(await colorOf(3, " is-active")).toBe(activeShallow);
  expect(await colorOf(6, " is-active")).toBe(activeShallow);

  expect(consoleErrors).toEqual([]);
});

// ---- HF-096 -----------------------------------------------------------------
test("the left rail's hover fill is a colour you can actually see", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const railBg = await page.evaluate(
    () => getComputedStyle(document.querySelector(".rail")).backgroundColor,
  );

  const button = page.locator("#railOutline");
  await button.hover();
  const hoverBg = await button.evaluate((el) => getComputedStyle(el).backgroundColor);

  // The hover fill must differ from the surface it is painted on, or the
  // declaration does nothing at all.
  expect(hoverBg).not.toBe(railBg);
  expect(hoverBg).not.toBe("rgba(0, 0, 0, 0)");

  expect(consoleErrors).toEqual([]);
});
