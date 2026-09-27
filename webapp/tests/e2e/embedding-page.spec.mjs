// The embedding guide, as a reader gets it.
//
// `embedding_page.test.mjs` proves the page says true things. This proves it is
// a usable page: it renders with the site chrome, every route on it resolves, it
// does not overflow a phone, and its text is legible.
//
// The legibility part is no longer this page's to carry. It used to be: this
// spec held an INHERITED_FAINT allow-list of shared docs-shell components —
// `.doc-rail-title`, `.doc-toc-list`, `.code-panel-head`, `.disp-row.head`,
// `.site-footer-copy`, `.brand-badge` and the rest — all painted in `--faint` at
// about 3.2:1, and asserted that this page added no NEW failure while those
// stayed broken. It even asserted the list was non-empty, so it would fail the
// day somebody fixed them: the right property for a list that must not outlive
// the defect it describes.
//
// HF-189 fixed them (`--faint-on-paper` in `src/marketing.css`), so the list is
// gone. What replaces it is not weaker: `site-contrast.spec.mjs` sweeps EVERY
// site page at two widths with no allow-list at all, sharing the same measurement
// through `contrast-audit.mjs`, and this page's own legibility test below now
// demands zero failures rather than "no new ones". The one thing the old test
// could do that a site-wide sweep cannot is prove the sweep ran on THIS page —
// that is what is kept.
import { test, expect } from "./fixtures.mjs";
import { auditRegion } from "./contrast-audit.mjs";

test("the embedding guide renders with the site chrome and marks itself in the nav", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/embedding.html");

  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Embed the editor");
  const nav = page.locator('header.site-header nav[aria-label="Primary navigation"]');
  // A Docs page, not a fifth primary link: the header stays the four links
  // `site-nav-consistency` asserts, and the guide is reached from the docs rail.
  await expect(nav.getByRole("link")).toHaveCount(4);
  await expect(page.locator('header nav a[data-nav="docs"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
  // Reachable from the neighbouring page's rail, which is what makes it findable
  // rather than merely deployed.
  await page.goto("/docs.html");
  await page.locator('.doc-rail-list a[href="./embedding.html"]').click();
  await expect(page).toHaveURL(/embedding\.html$/);

  expect(consoleErrors).toEqual([]);
});

test("every route on the guide resolves", async ({ page }) => {
  // A documentation page whose links 404 is worse than no page. Same-origin
  // links are fetched; the in-page anchors must have targets. External links are
  // not fetched — a test that reaches github.com fails for reasons that have
  // nothing to do with this repository.
  await page.goto("/embedding.html");
  const hrefs = await page.locator("a[href]").evaluateAll((links) =>
    links.map((a) => a.getAttribute("href")),
  );
  expect(hrefs.length).toBeGreaterThan(10);

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

test("the guide does not overflow a phone", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/embedding.html");
  const metrics = await page.evaluate(() => ({
    viewport: window.innerWidth,
    document: document.documentElement.scrollWidth,
  }));
  expect(metrics.document).toBeLessThanOrEqual(metrics.viewport);
  // The code panels are the risk: extracted source is as wide as it is, so each
  // one scrolls itself rather than the page.
  const overflowing = await page.locator(".code-panel pre").evaluateAll((nodes) =>
    nodes.filter((n) => getComputedStyle(n).overflowX !== "auto").length,
  );
  expect(overflowing, "a code panel must scroll itself, not the page").toBe(0);
  expect(consoleErrors).toEqual([]);
});

test("the guide's own text meets WCAG AA", async ({ page }) => {
  await page.goto("/embedding.html");
  await page.addStyleTag({
    content: "*, *::before, *::after { transition: none !important; animation: none !important; }",
  });
  const swept = await page.evaluate(auditRegion, { selector: "body" });
  // No allow-list, and not "no NEW failures" either: zero. Every component this
  // page inherits from the docs shell was measured and fixed under HF-189.
  expect(
    swept.failures.map((failure) => failure.describe),
    "text on this page is below the WCAG AA floor",
  ).toEqual([]);
  expect(
    swept.unresolved,
    "a background behind text here cannot be read out of CSS, so the sweep could " +
      "not measure it — and it will not guess",
  ).toEqual([]);
  // The replacement for the old non-empty assertion. That one could not pass while
  // the page was clean; this one cannot pass while the page is EMPTY, which is the
  // failure mode a zero-failure assertion has instead. Measured at 380 elements
  // desktop and 350 phone, so 40 is a floor and not a count.
  expect(
    swept.examined,
    `only ${swept.examined} text elements were measured — a sweep over a page that ` +
      "did not render finds no failures and passes",
  ).toBeGreaterThan(40);
});
