// The document must paint before the named web fonts arrive.
//
// `provisionFonts` fetches six variable faces — ~9.5 MB from a CDN — and the
// open path used to `await` it before the first `renderAll()`. So nothing was on
// screen until every byte of that had landed, on top of the engine's own ~9 MB
// download: seconds of blank editor before a single glyph appeared, and a total
// stall if the CDN was slow or blocked. The bundled faces are metric-compatible
// substitutes, so painting from them first is a correct layout, not a throwaway
// approximation, and the upgrade re-renders when the real faces register.
//
// These tests hold the font CDN open (never resolving, then failing) so "the
// fonts have not arrived" is a controlled state rather than a race.
import { test, expect } from "./fixtures.mjs";

const FONT_CDN = "**/cdn.jsdelivr.net/**";

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
