// The document must paint before the named web fonts arrive.
//
// `provisionFonts` fetches six variable faces — ~9.3 MB — and the open path used
// to `await` it before the first `renderAll()`. So nothing was on screen until
// every byte of that had landed, on top of the engine's own ~9 MB download:
// seconds of blank editor before a single glyph appeared, and a total stall if
// the fonts were slow or blocked. The bundled faces are metric-compatible
// substitutes, so painting from them first is a correct layout, not a throwaway
// approximation, and the upgrade re-renders when the real faces register.
//
// These tests hold the font requests open (never resolving, then failing) so
// "the fonts have not arrived" is a controlled state rather than a race.
import { test, expect } from "./fixtures.mjs";

// The named faces are served from OUR OWN ORIGIN now, not a CDN (`109` HF-176),
// so this intercepts the origin's own font directory. Routing
// `cdn.jsdelivr.net` here stopped intercepting anything the moment those faces
// were committed: every request would have succeeded, `fontsReady` would have
// been `"true"` immediately, and the assertion below that the upgrade has NOT
// happened yet would have failed — loudly, which is the only reason this did not
// become a spec that silently tested nothing.
const FONT_CDN = "**/assets/fonts/script/*";

/** What a reader would notice moving under them when the upgrade lands.
 *
 *  `caret` is the local signal: where the engine draws the caret for the model
 *  position the reader themselves chose.
 *
 *  `band` and `scrollHeight` are the GLOBAL one, and they are why this is not
 *  just a caret assertion. `.page-band`'s height is the whole document's scroll
 *  space — every page's box plus its gap, for all 14 pages of the fixture, not
 *  just the two sheets that are materialized near the viewport. So a line that
 *  re-wrapped anywhere in the document, on any page, moves this number. A guard
 *  that watched only the caret would be a guard about the first screen. */
async function documentShape(page) {
  return page.evaluate(() => {
    const rect = document.querySelector("#pages .overlay .caret")?.getBoundingClientRect();
    return {
      caret: rect ? [Math.round(rect.x), Math.round(rect.y), Math.round(rect.height)] : null,
      sheets: document.querySelectorAll(".page-wrap").length,
      scrollHeight: document.querySelector("#pages")?.scrollHeight ?? 0,
      band: Math.round(document.querySelector(".page-band")?.getBoundingClientRect().height ?? 0),
      text: (document.getElementById("a11yDocument")?.textContent ?? "").slice(0, 600),
    };
  });
}

test("the document renders while the named web fonts are still in flight", async ({ page }) => {
  // Hold every font request open for the life of the test: nothing resolves, so
  // any code path that awaits provisioning can never complete.
  let released;
  const held = new Promise((resolve) => {
    released = resolve;
  });
  await page.route(FONT_CDN, async (route) => {
    await held;
    await route.abort();
  });

  await page.goto("/editor.html?fixture=rich");

  // Pages are composed and the document's own text is readable through the
  // model-derived accessibility tree, with the fonts still outstanding.
  await expect(page.locator(".page-wrap").first()).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#a11yDocument")).toContainText("Rich Document");

  // The upgrade has NOT happened yet — proving the render above was the
  // pre-font paint and not a late assertion that quietly waited for it.
  expect(await page.evaluate(() => document.body.dataset.fontsReady)).toBeUndefined();

  released();
});

test("the editor stays usable when the font CDN is unreachable", async ({ page }) => {
  await page.route(FONT_CDN, (route) => route.abort());

  await page.goto("/editor.html?fixture=rich");
  await expect(page.locator(".page-wrap").first()).toBeVisible({ timeout: 30_000 });

  // Editing works on the bundled faces — a font CDN outage must not cost the
  // user their editor.
  await page.locator("#pages").focus();
  await page.keyboard.type("Z");
  await expect(page.locator("#a11yDocument")).toContainText("Z");
});

// The whole justification for fetching 9.28 MB of named faces before anyone
// asked is that the upgrade is INVISIBLE — that the first paint is already laid
// out on the right advance widths, so when the real faces register nothing
// moves. That claim had never been measured, and the cost of it being wrong is
// specific: a reader who clicked during the window would find their caret
// somewhere else once the fonts landed, which reads as the editor losing their
// cursor.
//
// So it is measured here rather than asserted in a comment,
// and it holds for the whole of our own origin's font directory — all 22 faces
// an editor load asks for, 11.09 MB, the six named families plus every
// coverage-driven script face the fixture's own scalars need. Both mechanisms
// are in the window at once, which is the honest version of the reader's
// experience; nothing is excluded to make the assertion easier.
//
// What makes the answer trustworthy is `font_substitution.rs`: a run naming a
// font we do not have is laid out on its metric partner's advances — Arial on
// Liberation Sans, Calibri on Carlito — so the first paint is the final layout
// for every family that has a partner, and `sample.docx`'s families all do.
//
// WHICH IS ALSO THE LIMIT OF WHAT THIS PROVES, and it is written down because
// the measurement is easy to over-read. The invisibility is CONDITIONAL on the
// document's families having metric partners. A document that names `Noto Sans`
// or `Noto Serif` outright gets Liberation Sans/Serif on the first paint —
// `GenericFamily`'s sans/serif default, not a metric partner of either — so when
// the real face registers that document necessarily re-wraps, and a caret
// placed in the window moves with the text it is in. No fixture in the tree
// does that, which is why it is not asserted here rather than why it is
// untrue.
test("the font upgrade does not move the document under the reader", async ({ page }) => {
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  await page.route(FONT_CDN, async (route) => {
    await held;
    await route.fallback();
  });

  await page.goto("/editor.html");
  await expect(page.locator(".page-wrap").first()).toBeVisible({ timeout: 45_000 });
  // The upgrade has NOT happened, so what follows is the pre-font paint. This is
  // the assertion that stops the rest being a comparison of one moment with
  // itself, and the reason the route holds rather than aborts.
  expect(await page.evaluate(() => document.body.dataset.fontsReady)).toBeUndefined();

  // A reader puts their caret in the middle of a paragraph, exactly as they
  // would while the editor is still settling.
  await page.locator("#pages").click({ position: { x: 220, y: 180 } });
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  const before = await documentShape(page);
  expect(before.caret).not.toBeNull();
  expect(before.band).toBeGreaterThan(0);

  release();
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true", null, {
    timeout: 45_000,
  });
  // `fontsReady` is set after the upgrade's own `renderAll` + `drawSelection`,
  // so there is nothing left to wait for; a `waitForTimeout` here would only
  // hide a repaint that arrives later than it should.
  expect(await documentShape(page)).toEqual(before);
});

test("the editor preloads the engine at parse time", async ({ page }) => {
  await page.goto("/editor.html?fixture=rich");

  // `init()` sits at the bottom of a ~500 KB module graph, so without an
  // explicit preload the wasm transfer cannot begin until that graph resolves.
  const preload = page.locator('head link[rel="preload"][href*="casual_doc_wasm_bg.wasm"]');
  await expect(preload).toHaveAttribute("as", "fetch");
  // crossorigin must match how wasm-bindgen's glue requests it, or the browser
  // treats the preload as a different resource and downloads ~9 MB twice.
  await expect(preload).toHaveAttribute("crossorigin", /.*/);
});
