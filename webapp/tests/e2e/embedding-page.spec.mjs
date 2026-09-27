// The embedding guide, as a reader gets it.
//
// `embedding_page.test.mjs` proves the page says true things. This proves it is
// a usable page: it renders with the site chrome, every route on it resolves, it
// does not overflow a phone, and its text is legible.
//
// The legibility part needs saying plainly, because it is a gap this spec
// documents rather than closes. `theme-contrast.spec.mjs` sweeps the EDITOR in
// both themes; the marketing and docs pages have no such sweep, and
// `src/marketing.css` is light-only (`color-scheme: light`, no
// `prefers-color-scheme` block), so "both themes" does not apply here. Sweeping
// this page turns up components of the shared docs shell — the section rail's
// captions, the on-this-page list, a code panel's caption, the pager's direction
// labels — that are painted in `--faint` and measure about 3.2:1, below the AA
// floor of 4.5:1 for their size. Those are `docs.html`'s and `fidelity.html`'s
// too, they are not this page's to restyle (`docs/63`: the tokens are
// deliberate, propose rather than change), and the fix is a token pairing
// decision for the owner. So the sweep below asserts the exact thing this page
// is responsible for: the failures are only the inherited components named here,
// and nothing new. Add a paragraph in an unreadable colour and it fails.
import { test, expect } from "./fixtures.mjs";

/** Shared-shell components that already fail AA on every docs page.
 *
 *  An allow-list, not an exemption: each entry is a class from
 *  `src/marketing.css`, and any OTHER failing element fails the test. Closing
 *  these is a `--faint` pairing change across the whole site. */
const INHERITED_FAINT = [
  ".doc-rail-title",
  ".doc-rail-list",
  ".doc-toc-title",
  ".doc-toc-list",
  ".doc-eyebrow",
  ".doc-pager-dir",
  ".code-panel-head",
  ".disp-row.head", // the shared table component's own column captions
  ".site-footer-copy",
  ".brand-badge",
].join(", ");

/** Contrast ratio of every text-bearing element in a region against the
 *  background composited behind it, as a list of failures.
 *
 *  The same technique as `theme-contrast.spec.mjs` — colours resolved by the
 *  canvas parser rather than by a regex, because Chrome answers in whatever form
 *  the cascade produced (`rgb()`, `color(srgb …)`, `color-mix`, `oklab()`) and a
 *  regex that assumes one reads another's components as RGB. It is a local copy
 *  because that one lives inside a spec file, and importing a spec would register
 *  its tests here; if a third caller ever needs it, it should move into
 *  `fixtures.mjs` for all three rather than be copied again. */
const auditRegion = ({ selector, allowed }) => {
  const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  const parse = (value) => {
    ctx.fillStyle = "#ff00ff";
    ctx.fillStyle = value;
    if (ctx.fillStyle === "#ff00ff" && !/f0f|ff00ff|magenta/i.test(value)) {
      throw new Error(`unparseable colour: ${value}`);
    }
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
    return { r, g, b, a: a / 255 };
  };
  const luminance = (c) => {
    const channel = (v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
  };
  const composite = (fg, bg) => ({
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: 1,
  });
  const backdrop = (el) => {
    const layers = [];
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      const c = parse(getComputedStyle(n).backgroundColor);
      if (c.a > 0) layers.push(c);
      if (c.a === 1) break;
    }
    let acc = { r: 255, g: 255, b: 255, a: 1 };
    for (let i = layers.length - 1; i >= 0; i--) acc = composite(layers[i], acc);
    return acc;
  };
  const contrast = (a, b) => {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  };

  const failures = [];
  for (const el of document.querySelector(selector).querySelectorAll("*")) {
    const own = [...el.childNodes]
      .filter((n) => n.nodeType === 3)
      .map((n) => n.textContent.trim())
      .join("");
    if (!own) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || cs.display === "none") continue;
    const box = el.getBoundingClientRect();
    if (box.width < 1 || box.height < 1) continue;

    const bg = backdrop(el);
    const ratio = contrast(composite(parse(cs.color), bg), bg);
    const size = parseFloat(cs.fontSize);
    const isLarge = size >= 24 || (size >= 18.66 && parseInt(cs.fontWeight, 10) >= 700);
    const required = isLarge ? 3 : 4.5;
    if (ratio < required) {
      // The nearest class in the shared stylesheet, so a failure can be charged
      // to a component rather than to a paragraph.
      const owner = [el, el.parentElement, el.parentElement?.parentElement]
        .filter(Boolean)
        .flatMap((n) => [...n.classList])
        .join(" ");
      failures.push({
        text: own.slice(0, 40),
        ratio: Number(ratio.toFixed(2)),
        required,
        size,
        classes: owner,
        // Charged to a shared component, or to this page. `closest` is asked of
        // the FAILING element, so a paragraph that merely sits inside an allowed
        // component is not excused — only that component's own text is.
        inherited: el.closest(allowed) !== null,
      });
    }
  }
  return failures;
};

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
  const failures = await page.evaluate(auditRegion, { selector: "body", allowed: INHERITED_FAINT });
  const unexpected = failures.filter((failure) => !failure.inherited);
  expect(
    unexpected,
    "text on this page is below the AA floor, and it is not one of the shared " +
      "components already known to fail",
  ).toEqual([]);
  // And the allow-list must not outlive the defect: if the shared components are
  // fixed, this fails and the list comes out.
  expect(
    failures.length,
    "the inherited --faint components now pass AA — remove INHERITED_FAINT from this spec",
  ).toBeGreaterThan(0);
});
