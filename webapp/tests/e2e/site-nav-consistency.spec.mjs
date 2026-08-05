import { test, expect } from "./fixtures.mjs";

// Every marketing/docs page must render the SAME shared header partial with the
// four primary-nav links (Overview · Editor · Docs · Fidelity) plus a GitHub
// link and the "Open the editor" CTA. This proves the build-time partial
// inlining kept the nav consistent site-wide — the whole point of authoring
// pages from _partials/.
const PAGES = ["/", "/docs.html", "/fidelity.html"];
const NAV_LINKS = ["Overview", "Editor", "Docs", "Fidelity"];

for (const path of PAGES) {
  test(`shared header + primary nav render on ${path}`, async ({ page, consoleErrors }) => {
    await page.goto(path);

    const header = page.locator("header.site-header");
    await expect(header).toBeVisible();

    // Brand lockup is present and links home.
    await expect(header.getByRole("link", { name: "OpenDoc home" })).toHaveAttribute("href", "./");

    const nav = header.locator('nav[aria-label="Primary navigation"]');
    for (const label of NAV_LINKS) {
      await expect(nav.getByRole("link", { name: label, exact: true })).toHaveCount(1);
    }
    // Exactly the four primary links — no Pricing/Editions (OSS-only positioning).
    await expect(nav.getByRole("link")).toHaveCount(NAV_LINKS.length);

    // GitHub is a header link that sits outside the primary nav.
    await expect(header.getByRole("link", { name: "GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/CasualOffice/opendoc",
    );

    // The "Open the editor" CTA points at the real editor on every page.
    await expect(header.getByRole("link", { name: /Open the editor/ })).toHaveAttribute(
      "href",
      "./editor.html",
    );

    expect(consoleErrors).toEqual([]);
  });
}

test("the active page is marked in the nav", async ({ page }) => {
  await page.goto("/docs.html");
  await expect(page.locator('header nav a[data-nav="docs"]')).toHaveAttribute("aria-current", "page");
  // A non-active link on the same page carries no active marker.
  await expect(page.locator('header nav a[data-nav="overview"]')).not.toHaveAttribute("aria-current", "page");

  await page.goto("/");
  await expect(page.locator('header nav a[data-nav="overview"]')).toHaveAttribute("aria-current", "page");

  await page.goto("/fidelity.html");
  await expect(page.locator('header nav a[data-nav="fidelity"]')).toHaveAttribute("aria-current", "page");
});
