// The site's motion is an enhancement, never a precondition for reading it
// (`109` HF-293).
//
// `src/site_motion.mjs` fades sections in as they scroll into view and runs the
// illustrations' loops while they are on screen. Both are built so that the
// FINISHED page is the default: every hide-before-reveal rule and every loop is
// scoped under `.js-motion`, which only that module sets, and only when the
// reader has not asked for reduced motion. This spec holds the guarantee on the
// rendered pages rather than on the stylesheet, in all three conditions:
//
//   1. scripts OFF   — nothing is hidden, every illustration rests on its last frame;
//   2. reduced motion — the same, with scripts on;
//   3. motion allowed — the class goes on, what is already on screen is NOT hidden
//                       (no flash), what is below the fold is, and scrolling to it
//                       reveals it.
//
// Pages are DISCOVERED from the `*.page.html` templates, so a new site page is
// covered on arrival, the way `site-contrast.spec.mjs` discovers its own.
import { readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test, expect } from "./fixtures.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const PAGES = readdirSync(WEBAPP)
  .filter((name) => name.endsWith(".page.html"))
  .map((name) => "/" + (name === "index.page.html" ? "" : name.replace(/\.page\.html$/, ".html")))
  .sort();

/** Everything the motion layer could have hidden, read back from the page:
 *  reveal targets that are not fully opaque or are displaced, and the home
 *  hero's illustrated parts that are not resting on their finished frame. */
const hiddenContent = () => {
  const hidden = [];
  for (const el of document.querySelectorAll("[data-reveal]")) {
    const cs = getComputedStyle(el);
    if (Number(cs.opacity) < 1 || (cs.transform !== "none" && !/^matrix\(1, 0, 0, 1, 0, 0\)$/.test(cs.transform))) {
      hidden.push(`[data-reveal] <${el.tagName.toLowerCase()}.${el.className}> opacity=${cs.opacity} transform=${cs.transform}`);
    }
  }
  // The hero's illustration states its story in its final frame: the typed line,
  // the second participant and the comment. None may be mid-loop or hidden.
  for (const selector of [".od-type", ".od-peer-caret", ".od-comment", ".cb-arrive", ".cb-check"]) {
    for (const el of document.querySelectorAll(selector)) {
      const cs = getComputedStyle(el);
      const box = el.getBoundingClientRect();
      if (Number(cs.opacity) < 1 || box.width < 1 || cs.animationName !== "none") {
        hidden.push(`${selector} opacity=${cs.opacity} width=${box.width} animation=${cs.animationName}`);
      }
    }
  }
  return {
    motionClass: document.documentElement.classList.contains("js-motion"),
    reveals: document.querySelectorAll("[data-reveal]").length,
    hidden,
  };
};

test.describe("with scripts off", () => {
  test.use({ javaScriptEnabled: false });

  for (const path of PAGES) {
    test(`${path} shows every section and finished illustrations`, async ({ page }) => {
      await page.goto(path);
      const state = await page.evaluate(hiddenContent);
      expect(state.motionClass, "nothing may set the motion class without scripts").toBe(false);
      expect(state.hidden).toEqual([]);
    });
  }
});

test.describe("with reduced motion", () => {
  for (const path of PAGES) {
    test(`${path} holds still and shows everything`, async ({ page, consoleErrors }) => {
      // Emulated on the page itself, before navigation, so the module reads the
      // preference at start exactly as a reader's browser would report it.
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.goto(path);
      // The module has run (the language picker it loads beside is the witness
      // that scripts executed) and still declined to animate.
      await expect(page.locator(".site-lang")).toBeVisible();
      const state = await page.evaluate(hiddenContent);
      expect(state.motionClass, "reduced motion must never get the motion class").toBe(false);
      expect(state.hidden).toEqual([]);
      const looping = await page.evaluate(
        () =>
          document
            .getAnimations()
            .filter((a) => a.playState === "running" && a.effect?.getTiming().iterations === Infinity)
            .map((a) => a.animationName ?? String(a.id)),
      );
      expect(looping, "an animation loops forever under reduced motion").toEqual([]);
      expect(consoleErrors).toEqual([]);
    });
  }
});

test("with motion allowed, the first screen never blinks out and the rest reveals on scroll", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await expect(page.locator("html")).toHaveClass(/\bjs-motion\b/);

  // Above the fold: revealed before the class went on, so never hidden.
  const firstScreen = await page.evaluate(() =>
    [...document.querySelectorAll("[data-reveal]")]
      .filter((el) => el.getBoundingClientRect().top < innerHeight)
      .map((el) => el.classList.contains("is-revealed")),
  );
  expect(firstScreen.length).toBeGreaterThan(0);
  expect(firstScreen.every(Boolean), "content in the first viewport was hidden for a reveal").toBe(true);

  // Below the fold: hidden until reached, then fully shown.
  const target = page.locator("#roadmap .home-head");
  await expect(target).not.toHaveClass(/\bis-revealed\b/);
  await target.scrollIntoViewIfNeeded();
  await expect(target).toHaveClass(/\bis-revealed\b/);
  await expect.poll(() => target.evaluate((el) => Number(getComputedStyle(el).opacity))).toBe(1);

  // And the hero's loop only runs while the hero is on screen.
  await expect(page.locator("#homeEmbed")).not.toHaveClass(/\bis-in\b/);
  await page.evaluate(() => window.scrollTo({ top: 0, behavior: "instant" }));
  await expect(page.locator("#homeEmbed")).toHaveClass(/\bis-in\b/);
  expect(consoleErrors).toEqual([]);
});
