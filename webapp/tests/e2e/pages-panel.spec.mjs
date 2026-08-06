import { test, expect } from "./fixtures.mjs";

// The rail's Pages panel is a page navigator: one column of REAL rendered page
// thumbnails (not blank boxes), with the scrollbar gutter reserved so it never
// overlaps a card. Clicking a thumbnail navigates and marks it active.
test("the Pages panel shows one column of real page thumbnails and navigates", async ({
  page,
  consoleErrors,
}) => {
  // The default editor loads the multi-page sample.docx (unlike gotoEditor's
  // single-page rich fixture) — a navigator needs several pages.
  await page.goto("/editor.html");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 1
      );
    },
    null,
    { timeout: 45_000 },
  );

  await page.locator("#railPages").click();
  const panel = page.locator("#pagesPanel");
  await expect(panel).toBeVisible();

  const thumbs = panel.locator(".page-thumb");
  await expect(thumbs.first()).toBeVisible();
  expect(await thumbs.count()).toBeGreaterThan(1);

  // Each card renders a real page bitmap into a canvas with non-zero pixels —
  // not the old empty numbered box.
  const firstCanvas = panel.locator(".page-thumb-canvas").first();
  await expect(firstCanvas).toBeVisible();
  const dims = await firstCanvas.evaluate((c) => ({ w: c.width, h: c.height }));
  expect(dims.w).toBeGreaterThan(0);
  expect(dims.h).toBeGreaterThan(0);

  // Single column, with a stable scrollbar gutter.
  const grid = await panel.locator("#pagesBody").evaluate((el) => {
    const s = getComputedStyle(el);
    return { cols: s.gridTemplateColumns, gutter: s.scrollbarGutter };
  });
  expect(grid.cols.trim().split(/\s+/).length).toBe(1); // one column track
  expect(grid.gutter).toContain("stable");

  // Clicking a thumbnail keeps exactly one active card.
  await thumbs.nth(1).click();
  await expect(panel.locator(".page-thumb.is-active")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});
