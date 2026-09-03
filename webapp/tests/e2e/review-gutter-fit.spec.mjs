// Turning on Suggesting pushed the page off the right of the window, and took
// the comment column with it.
//
// `.pages` is `width: max-content`, so the gutter reserved for the comment
// column was ADDED to the sheet's width rather than taken out of the window:
// 364px of gutter plus an 816px Letter sheet plus the 55px rail needs a ~1235px
// window, and the first step-down was at 860px. Every window between those —
// ordinary laptop territory — clipped the sheet, and since the column is
// absolutely positioned inside that same overflowing box, it was carried
// off-screen too. Worse, the width never recovered: resizing the window smaller
// left `.pages` at its original size.
//
// The widths below bracket that band deliberately. 1280 was fine before the fix
// and must stay fine; 1100 and 980 are inside it.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

async function enterSuggesting(page) {
  await clickIntoFirstPage(page);
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await page.keyboard.type("Inserted suggestion for review. ");
  await expect(page.locator(".review-sidebar")).toBeVisible();
}

/** Anything that has escaped the window horizontally. */
async function overflow(page) {
  return page.evaluate(() => {
    const report = [];
    const pages = document.getElementById("pages");
    const sidebar = document.querySelector(".review-sidebar");
    for (const [name, el] of [["#pages", pages], [".review-sidebar", sidebar]]) {
      if (!el) continue;
      const box = el.getBoundingClientRect();
      if (box.right > window.innerWidth + 1) {
        report.push(`${name} runs to ${Math.round(box.right)} in a ${window.innerWidth}px window`);
      }
      if (box.left < -1) report.push(`${name} starts at ${Math.round(box.left)}`);
    }
    return report;
  });
}

for (const width of [1280, 1100, 980]) {
  test(`the page and the comment column both fit a ${width}px window in Suggesting`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width, height: 800 });
    await gotoEditor(page);
    await enterSuggesting(page);

    expect(await overflow(page)).toEqual([]);
    // The column is useless if it is technically on-screen but too narrow to
    // read, so the floor is asserted rather than left to the clamp.
    const sidebar = await page.locator(".review-sidebar").boundingBox();
    expect(sidebar.width).toBeGreaterThanOrEqual(240);

    expect(consoleErrors).toEqual([]);
  });
}

test("the layout recovers when the window is made smaller", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await enterSuggesting(page);

  // The original defect was not only that the reservation was too big, but that
  // it was baked in: `.pages` kept its width through a resize, so a window that
  // started wide stayed broken after being narrowed.
  for (const width of [1100, 980, 900]) {
    await page.setViewportSize({ width, height: 800 });
    // Let the resize settle before measuring: boundingBox() is null while one is
    // mid-flight, and the layout under test is the settled one.
    await page.waitForFunction((w) => window.innerWidth === w, width);
    await expect(page.locator("#pages")).toBeVisible();
    // Playwright's boundingBox is {x, y, width, height} — there is no `right`.
    const box = await page.locator("#pages").boundingBox();
    expect(box.x + box.width, `#pages after resizing to ${width}px`).toBeLessThanOrEqual(width + 1);
    expect(await overflow(page), `after resizing to ${width}px`).toEqual([]);
  }

  expect(consoleErrors).toEqual([]);
});

// The reservation has to be computed from the sheet's REAL width, which depends
// on zoom and paper size — neither of which CSS can see. The stylesheet carries
// a Letter-sized fallback so it degrades gracefully, and at 100% zoom that
// fallback is within a couple of dozen pixels of the truth, which is close
// enough to hide a missing `--page-width` entirely. Zooming in separates them:
// once the sheet is wider than the window there is no spare space at all, and
// anything still reserved is dead margin that the comment column floats out into.
test("no gutter is reserved once the sheet is wider than the window", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1100, height: 800 });
  await gotoEditor(page);
  await enterSuggesting(page);

  for (let step = 0; step < 4; step++) await page.locator("#zoomIn").click();

  const state = await page.evaluate(() => {
    const pages = document.getElementById("pages");
    const sheet = document.querySelector(".page-wrap");
    return {
      reserved: parseFloat(getComputedStyle(pages).paddingRight),
      sheetWidth: sheet.getBoundingClientRect().width,
      available: document.getElementById("viewport").clientWidth,
    };
  });

  // Precondition: the zoom actually outgrew the window, or this proves nothing.
  expect(state.sheetWidth).toBeGreaterThan(state.available);
  expect(
    state.reserved,
    `${Math.round(state.reserved)}px reserved beside a ${Math.round(state.sheetWidth)}px sheet in a ${state.available}px space`,
  ).toBe(0);

  expect(consoleErrors).toEqual([]);
});
