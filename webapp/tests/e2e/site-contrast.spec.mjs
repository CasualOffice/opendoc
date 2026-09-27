// The site's text, measured. HF-189.
//
// `theme-contrast.spec.mjs` sweeps the EDITOR and says "both themes". Neither
// half of that applied to the public pages: nothing swept them at all, and
// `src/marketing.css` is light-only (`color-scheme: light`, no
// `prefers-color-scheme` block and no `[data-theme]` palette), so there is no
// second theme here to sweep. The result was that `--faint` (#8a8e95) sat at
// about 3.2:1 on the page grounds — under the 4.5:1 AA floor at the 10–13px the
// docs shell uses it at — on `docs.html` and `fidelity.html` as much as on the
// newer `embedding.html`, and no gate in the repository would ever have said so.
// `embedding-page.spec.mjs` had an allow-list naming those components, which
// documented the defect without being able to close it.
//
// Two rules this spec is built to obey:
//
//   1. It WALKS the pages. A sweep that visits a list of selectors somebody
//      already wrote down cannot find the next one, and the list becomes the
//      guarantee instead of the floor. Every text-bearing element is measured.
//   2. It cannot pass on nothing. A 404, a stylesheet that did not load, a page
//      that never rendered — each produces a document with no measurable text,
//      and a sweep over no text reports no failures. So every page asserts a
//      floor on how much text was measured, and asserts that the site palette
//      actually reached the document before believing the colours it read.
//
// ONE theme, deliberately, and asserted rather than assumed: the last test here
// fails if a dark site palette is ever introduced, because at that point this
// sweep would be covering half the surface while still reading as complete.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test, expect } from "./fixtures.mjs";
import { auditRegion } from "./contrast-audit.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const MARKETING_CSS = readFileSync(join(WEBAPP, "src", "marketing.css"), "utf8");

/** Every site page, DISCOVERED rather than listed.
 *
 *  A hardcoded list is the failure mode this whole spec is about: a new page gets
 *  added, nobody remembers the sweep, and the sweep stays green.
 *
 *  Primarily the pages `build-site.py` GENERATES — one per `*.page.html`
 *  template. That is the definition of the site, and it is deliberately not "any
 *  page that links `src/marketing.css`": the first draft used the link, and
 *  deleting the link from `docs.page.html` (the mutation written to prove the
 *  unpainted-page guard) quietly dropped `docs.html` out of the sweep instead of
 *  failing it. A page cannot leave this gate by removing its own stylesheet.
 *
 *  Plus any OTHER top-level page that does link the stylesheet, so a hand-written
 *  one (`embed.html`) is covered too. `editor.html` is out: it is the
 *  application, painted by `src/style.css`, swept by `theme-contrast.spec.mjs`.
 *  The `*.page.html` templates themselves are out: they are the generator's
 *  inputs, the same bodies without the inlined chrome, so sweeping them would
 *  audit every shared component twice and neither the header nor the footer once. */
const GENERATED = readdirSync(WEBAPP)
  .filter((name) => name.endsWith(".page.html"))
  .map((name) => name.replace(/\.page\.html$/, ".html"));
const PAGES = [
  ...GENERATED,
  ...readdirSync(WEBAPP).filter(
    (name) =>
      name.endsWith(".html") &&
      !name.endsWith(".page.html") &&
      name !== "editor.html" &&
      !GENERATED.includes(name) &&
      readFileSync(join(WEBAPP, name), "utf8").includes("src/marketing.css"),
  ),
].sort();

/** Text-bearing elements a real site page must present before a sweep over it
 *  means anything.
 *
 *  MEASURED: 51 on the smallest page (`embed.html`), 65 on `docs.html` at phone
 *  width, 380 on `embedding.html`. 40 sits under all of them and far above what a
 *  404 page, an empty shell or an unstyled document can produce — which is the
 *  only thing this number has to do. It is a floor, not a count, so it does not
 *  need maintaining every time a page gains a paragraph. */
const MIN_EXAMINED = 40;

/** Desktop and phone, because the docs shell restyles itself below 900px —
 *  `.brand-badge`, `.site-footer-copy` and `.disp-row.head` all change size
 *  there, and the AA floor is size-dependent. Sweeping one width audits one
 *  half of the stylesheet. */
const VIEWPORTS = [
  { name: "desktop", width: 1280, height: 900 },
  { name: "phone", width: 390, height: 844 },
];

/** Reads back what the document actually resolved, so a sweep never trusts
 *  colours from a stylesheet that failed to load. Runs in the page. */
const paletteProbe = () => {
  const root = getComputedStyle(document.documentElement);
  return {
    bg: root.getPropertyValue("--bg").trim(),
    faintOnPaper: root.getPropertyValue("--faint-on-paper").trim(),
    bodyBackground: getComputedStyle(document.body).backgroundColor,
    text: (document.body.textContent ?? "").trim().length,
  };
};

for (const viewport of VIEWPORTS) {
  for (const page_ of PAGES) {
    test(`${page_} text meets WCAG AA (${viewport.name})`, async ({ page, consoleErrors }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto(`/${page_}`);
      await page.addStyleTag({
        content:
          "*, *::before, *::after { transition: none !important; animation: none !important; }",
      });

      // The palette reached the document. Without this, a 404 on the stylesheet
      // leaves black-on-white everywhere, every ratio passes, and the sweep
      // reports a clean page it never actually measured.
      const palette = await page.evaluate(paletteProbe);
      expect(palette.bg, `${page_} did not load src/marketing.css`).toBe("#fbfaf8");
      expect(
        palette.faintOnPaper,
        "--faint-on-paper is not defined — the AA-safe faint tone is the fix HF-189 landed",
      ).not.toBe("");
      expect(palette.bodyBackground, `${page_} is not painted on the site ground`).toBe(
        "rgb(251, 250, 248)",
      );

      const swept = await page.evaluate(auditRegion, { selector: "body" });
      expect(
        swept.unresolved,
        `a background behind text on ${page_} cannot be read out of CSS — the sweep ` +
          "will not guess at it, so give it a resolvable colour or measure it another way",
      ).toEqual([]);
      expect(
        swept.examined,
        `only ${swept.examined} text elements measured on ${page_} (${viewport.name}) — ` +
          "a sweep over an empty or unrendered page finds no failures and passes",
      ).toBeGreaterThan(MIN_EXAMINED);
      expect(
        swept.failures.map((failure) => failure.describe),
        `text below the WCAG AA floor on ${page_} at ${viewport.name} width`,
      ).toEqual([]);

      expect(consoleErrors).toEqual([]);
    });
  }
}

test("--faint-on-paper clears AA on every ground the site paints it on", async ({ page }) => {
  // The token's ratio is MEASURED here, not asserted from a hex code in a
  // comment, and it is measured by the same sweep that measures the pages — so
  // there is one implementation of the WCAG formula in this repository and no
  // chance of a token that satisfies a second, kinder one.
  //
  // Probes, not a hand-picked page: the token has to clear the floor against
  // every surface token the site paints a panel or a band in, not only the one
  // it happens to be used on today. `--faint` failed precisely because it was
  // chosen against one ground and used on a quieter one.
  await page.goto(`/${PAGES[0]}`);
  // Every opaque background this stylesheet paints behind text: the surface
  // tokens, plus the two literals it uses directly (the hero's url pill and the
  // fidelity figure's hatch). Derived from the stylesheet, not typed out, so a
  // new surface token joins the probe set on its own.
  const grounds = [
    ...new Set(
      [...MARKETING_CSS.matchAll(/(--(?:bg|surface|paper)[\w-]*)\s*:/g)].map((match) => match[1]),
    ),
  ].sort();
  const literals = ["#eceff5", "#efeee9"];
  expect(grounds.length, "no surface tokens were found to probe").toBeGreaterThan(4);

  await page.evaluate(
    ({ tokens, hexes }) => {
      const host = document.createElement("div");
      host.id = "contrastProbes";
      for (const ground of [...tokens.map((t) => `var(${t})`), ...hexes]) {
        const probe = document.createElement("p");
        // 10px is the smallest size the token is used at (`.brand-badge`), so the
        // probe is measured against the strict 4.5:1 floor, never large text's 3:1.
        probe.style.cssText =
          `margin:0;padding:8px;font-size:10px;background:${ground};color:var(--faint-on-paper)`;
        probe.textContent = `faint on ${ground}`;
        host.appendChild(probe);
      }
      document.body.appendChild(host);
    },
    { tokens: grounds, hexes: literals },
  );

  // Scoped to the probes: `body` would fold in whatever the page itself does, and
  // then this test would be asserting the page rather than the token.
  const swept = await page.evaluate(auditRegion, { selector: "#contrastProbes" });
  expect(
    swept.examined,
    "the probes were not measured, so this test proved nothing",
  ).toBe(grounds.length + literals.length);
  expect(swept.unresolved).toEqual([]);
  expect(
    swept.failures.map((failure) => failure.describe),
    "--faint-on-paper is below 4.5:1 on a ground this stylesheet paints",
  ).toEqual([]);
});

test("the sweep covers every page the site generator builds", async () => {
  // The floor that stops the page list emptying out unnoticed. MEASURED: five
  // pages today — four generated (index, docs, fidelity, embedding) plus the
  // hand-written embed host. If a template is deleted the count drops and this
  // fails, which is the difference between a discovered list and an absent one.
  expect(GENERATED.length, `generated pages: ${GENERATED.join(", ")}`).toBeGreaterThan(3);
  expect(PAGES, `site pages swept: ${PAGES.join(", ")}`).toContain("embed.html");
  for (const generated of GENERATED) expect(PAGES).toContain(generated);
});

test("the site is one theme, and stays one theme while this sweep is one theme", async () => {
  // Pinned to the guarantee, not to the circumstance. This sweep runs each page
  // once because `marketing.css` has exactly one palette; the moment a second
  // one exists, running it once covers half the surface and still reports a
  // pass. So the single-palette fact is asserted, and adding a dark site theme
  // fails here until this spec grows a second pass — the same contract
  // `theme-contrast.spec.mjs` has for the editor.
  expect(MARKETING_CSS).toMatch(/color-scheme:\s*light\s*;/);
  const darkEntryPoints = [
    ...MARKETING_CSS.matchAll(/@media[^{]*prefers-color-scheme[^{]*\{/g),
    ...MARKETING_CSS.matchAll(/:root\s*\[\s*data-theme\s*=\s*["']?dark/g),
  ].map((match) => match[0]);
  expect(
    darkEntryPoints,
    "the site stylesheet grew a dark palette — sweep both themes here, as the " +
      "editor's contrast spec does, before this passes again",
  ).toEqual([]);
});
