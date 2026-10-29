// The chrome scales from one number and the document does not (docs/109
// HF-413).
//
// `tests/chrome_units.test.mjs` holds the SOURCE half: no px in a chrome rule,
// and the root curve evaluated from the stylesheet text. This spec holds what
// only a browser can answer — that the rem actually resolves against that root,
// that the document region really stays in CSS px while the chrome moves, and
// that the scripts which lay the chrome out (the ribbon's overflow pass) hear
// about a root change that does not resize the window.
import { expect, gotoEditor, test } from "./fixtures.mjs";

const rootPx = (page) =>
  page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));

/** The chrome and document lengths this spec compares, in CSS px. */
const measure = (page) =>
  page.evaluate(() => {
    const height = (selector) => document.querySelector(selector)?.getBoundingClientRect().height ?? null;
    const pages = document.querySelector("#pages");
    return {
      root: parseFloat(getComputedStyle(document.documentElement).fontSize),
      // Chrome: the header bar is `--h-header`, a token in rem.
      header: height(".bar"),
      // Document: the gap between sheets is the layout's `PAGE_GAP_PX`.
      pageGap: pages ? getComputedStyle(pages).rowGap : null,
    };
  });

/** Which ribbon groups the overflow pass has exiled, and whether ⋯ shows. */
const overflowState = (page) =>
  page.evaluate(() => {
    const btn = document.getElementById("ribbonOverflowBtn");
    const menu = document.getElementById("ribbonOverflowMenu");
    return {
      shown: !!btn && !btn.hidden,
      exiled: [...(menu?.children ?? [])].map((el) => el.dataset.group || el.id || el.className),
    };
  });

test("the root follows the window: 14px at 1024, 16px across 1280-1440, 20px at 2560", async ({
  page,
  consoleErrors,
}) => {
  const expected = [
    [1024, 14],
    [1152, 15],
    [1280, 16],
    [1440, 16],
    // 16 + (1920 - 1440) / 280
    [1920, 16 + 480 / 280],
    [2560, 20],
  ];
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  for (const [width, px] of expected) {
    await page.setViewportSize({ width, height: 800 });
    await expect.poll(() => rootPx(page), `root at ${width}px`).toBeCloseTo(px, 2);
  }
  expect(consoleErrors).toEqual([]);
});

test("--chrome-scale grows the chrome and leaves the document alone", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  const before = await measure(page);
  expect(before.root, "precondition: the default root at the budget width").toBe(16);
  expect(before.header).toBeGreaterThan(0);

  await page.evaluate(() => document.documentElement.style.setProperty("--chrome-scale", "1.25"));
  await expect.poll(() => rootPx(page)).toBe(20);
  const after = await measure(page);

  // The header's rows are rem tokens and rem type, so it scales with the root
  // (within layout rounding, which is per row).
  expect(Math.abs(after.header - before.header * 1.25)).toBeLessThanOrEqual(2);
  // The page gap is the document's: it must not move with the chrome.
  expect(after.pageGap).toBe(before.pageGap);
  expect(after.pageGap).toBe("22px");

  expect(consoleErrors).toEqual([]);
});

test("the ribbon re-decides its overflow when the root changes without a resize", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await page.locator('[data-tab="home"]').click();
  const base = await overflowState(page);

  // A root change alone: the window and the band keep their width, so a
  // ResizeObserver on the band never fires. 1.75 makes the Home band far wider
  // than 1280px, so the right answer is certainly different from `base`.
  await page.evaluate(() => document.documentElement.style.setProperty("--chrome-scale", "1.75"));
  await expect.poll(() => rootPx(page)).toBe(28);

  // The reference answer: force a re-decision the old way, by resizing.
  const live = await expect
    .poll(() => overflowState(page), { message: "the live re-decision" })
    .not.toEqual(base)
    .then(() => overflowState(page));
  await page.setViewportSize({ width: 1281, height: 800 });
  await page.setViewportSize({ width: 1280, height: 800 });
  await expect.poll(() => overflowState(page)).toEqual(live);

  // And back: the exiled groups come home on the same signal.
  await page.evaluate(() => document.documentElement.style.setProperty("--chrome-scale", "1"));
  await expect.poll(() => overflowState(page)).toEqual(base);

  expect(consoleErrors).toEqual([]);
});
