// The SDK page, as a reader gets it. `sdk_page.test.mjs` proves it says true
// things; this proves it is a usable page: the site chrome, its own nav entry,
// every route resolving, no horizontal scroll on a phone, and legible text.
import { test, expect } from "./fixtures.mjs";
import { auditRegion } from "./contrast-audit.mjs";

test("the SDK page renders with the site chrome and is reachable from three places", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/sdk.html");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("The OpenDoc SDK");
  await expect(page.locator('header nav a[data-nav="sdk"]')).toHaveAttribute("aria-current", "page");

  await page.goto("/docs.html");
  await page.locator('.doc-rail-list a[href="./sdk.html"]').click();
  await expect(page).toHaveURL(/sdk\.html$/);

  await page.goto("/");
  await page.locator('#sdk a[href="./sdk.html"]').click();
  await expect(page).toHaveURL(/sdk\.html$/);
  expect(consoleErrors).toEqual([]);
});

test("every route on the SDK page resolves", async ({ page }) => {
  await page.goto("/sdk.html");
  const hrefs = await page.locator("a[href]").evaluateAll((links) => links.map((a) => a.getAttribute("href")));
  const anchors = await page.locator("[id]").evaluateAll((els) => els.map((el) => el.id));
  for (const href of new Set(hrefs)) {
    if (href.startsWith("#")) {
      expect(anchors, `${href} has no target on the page`).toContain(href.slice(1));
      continue;
    }
    if (href.startsWith("http")) continue;
    const response = await page.request.get(new URL(href, page.url()).href);
    expect(response.status(), `${href} does not resolve`).toBeLessThan(400);
  }
});

test("the SDK page does not overflow a phone", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/sdk.html");
  const metrics = await page.evaluate(() => ({
    viewport: window.innerWidth,
    document: document.documentElement.scrollWidth,
  }));
  expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
  // Code panels and the wide benchmark table each scroll themselves.
  const panels = await page.locator(".code-panel pre").evaluateAll((nodes) =>
    nodes.filter((n) => getComputedStyle(n).overflowX !== "auto").length,
  );
  expect(panels, "a code panel must scroll itself, not the page").toBe(0);
  const tables = await page.locator(".doc-table-scroll").evaluateAll((nodes) =>
    nodes.filter((n) => getComputedStyle(n).overflowX !== "auto").length,
  );
  expect(tables, "a table must scroll itself, not the page").toBe(0);
  expect(consoleErrors).toEqual([]);
});

test("the SDK page's text meets WCAG AA", async ({ page }) => {
  await page.goto("/sdk.html");
  await page.addStyleTag({
    content: "*, *::before, *::after { transition: none !important; animation: none !important; }",
  });
  const swept = await page.evaluate(auditRegion, { selector: "body" });
  expect(swept.failures.map((failure) => failure.describe)).toEqual([]);
  expect(swept.unresolved).toEqual([]);
});
