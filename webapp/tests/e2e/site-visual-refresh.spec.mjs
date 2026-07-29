import { test, expect } from "./fixtures.mjs";

test("the developer landing page exposes the real editor routes and current preview", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Own the document engine, not the dependency.",
  );
  await expect(page.getByRole("link", { name: "Try the live demo" })).toHaveAttribute(
    "href",
    "./editor.html?demo=1",
  );
  await expect(page.getByRole("link", { name: "Open your DOCX" })).toHaveAttribute(
    "href",
    "./editor.html",
  );
  await expect(page.locator(".product-stage img")).toHaveJSProperty("complete", true);
  expect(
    await page.locator(".product-stage img").evaluate((image) => image.naturalWidth),
  ).toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
});

test("the refreshed landing page has no narrow-viewport page overflow", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");

  const metrics = await page.evaluate(() => ({
    viewport: window.innerWidth,
    document: document.documentElement.scrollWidth,
  }));
  expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
  await expect(page.getByRole("link", { name: "Open editor" })).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
