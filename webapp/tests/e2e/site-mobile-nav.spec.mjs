// The marketing site's navigation on a phone (HF-087).
//
// site-nav-consistency.spec.mjs proves the four primary links are in the DOM on
// every page; it says nothing about whether they are on SCREEN. They were not:
// `.site-nav` was `display: none` under 820px with no toggle and no replacement,
// so Docs and Fidelity were reachable only by URL or Back. This spec measures
// what a phone actually renders.
import { test, expect } from "./fixtures.mjs";

const NAV_LINKS = ["Overview", "Editor", "Docs", "Fidelity"];
const PHONES = [
  { name: "iPhone-class", width: 390, height: 844 },
  { name: "small Android", width: 360, height: 780 },
];

for (const phone of PHONES) {
  test(`every primary destination is reachable on a ${phone.name} viewport`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: phone.width, height: phone.height });
    await page.goto("/docs.html");

    const nav = page.locator('header nav[aria-label="Primary navigation"]');
    await expect(nav).toBeVisible();

    for (const label of NAV_LINKS) {
      const link = nav.getByRole("link", { name: label, exact: true });
      await expect(link).toBeVisible();
      // Visible is not enough — it has to be inside the window, tappable.
      await expect(link).toBeInViewport();
    }

    // GitHub and the CTA survive too: the header keeps every route out of the
    // page, not just the one that sells the editor.
    const header = page.locator("header.site-header");
    await expect(header.getByRole("link", { name: "GitHub" })).toBeInViewport();
    await expect(header.getByRole("link", { name: /Open the editor/ })).toBeInViewport();

    // Wrapping the header must not have introduced a sideways scroll.
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);

    expect(consoleErrors).toEqual([]);
  });
}

test("the wide-window header is untouched: one row, nav beside the brand", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/");

  const header = page.locator(".site-header-inner");
  const brand = await header.locator(".brand").boundingBox();
  const nav = await header.locator(".site-nav").boundingBox();

  // Same row as the brand — the nav has not been pushed onto a line of its own
  // at desktop width.
  expect(Math.abs(nav.y - brand.y)).toBeLessThan(brand.height);
  expect(nav.x).toBeGreaterThan(brand.x);
  expect((await header.boundingBox()).height).toBeLessThanOrEqual(60);
});
