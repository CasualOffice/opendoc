import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

// Waits two animation frames so a render scheduled via requestAnimationFrame
// (the review margin renderer) has committed before we measure geometry.
async function settle(page) {
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
}

async function addCommentAtDocStart(page, text) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("ANCHORED_COMMENT");
  for (let i = 0; i < "ANCHORED_COMMENT".length; i++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await page.locator("#selComment").click();
  const composer = page
    .locator("#reviewSidebar")
    .locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill(text);
  await page
    .locator("#reviewSidebar")
    .locator('[data-testid="review-comment-submit"]')
    .click();
}

test("the comment column shares the canvas scroll context (one scroll owner)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await addCommentAtDocStart(page, "Stays pinned to its text");

  const card = page.locator(".review-margin-card.review-margin-comment");
  const marker = page.locator(".review-comment-marker").first();
  await expect(card).toBeVisible();
  await expect(marker).toHaveCount(1);

  // Structural invariant: the comment column is a descendant of the single
  // `.viewport` scroll container and is NOT an independent scroll context of its
  // own — the old sibling `overflow:auto` sidebar (and its JS scroll-sync) is
  // gone, so there is exactly one scrollbar, at the viewport's far-right edge.
  const structure = await page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    const sidebar = document.getElementById("reviewSidebar");
    const insideViewport = viewport.contains(sidebar);
    // A real scroll container would keep a non-zero scrollTop; this layer cannot
    // scroll on its own, so the write is a no-op.
    sidebar.scrollTop = 9999;
    const sidebarScrollTop = sidebar.scrollTop;
    const viewportCanScroll =
      viewport.scrollHeight - viewport.clientHeight > 1;
    return { insideViewport, sidebarScrollTop, viewportCanScroll };
  });
  expect(structure.insideViewport).toBe(true);
  expect(structure.sidebarScrollTop).toBe(0);
  expect(structure.viewportCanScroll).toBe(true);

  // Tracking invariant: the on-screen gap between a comment card and its
  // anchored text is fixed. Because both now live in the same scroll context,
  // scrolling the canvas a large amount must move the card and its anchor
  // together — the difference of their client-rect tops stays constant (no
  // separate sidebar scroll position to desync, no momentum drift).
  const before = await page.evaluate(() => {
    const cardEl = document.querySelector(
      ".review-margin-card.review-margin-comment",
    );
    const markerEl = document.querySelector(".review-comment-marker");
    return {
      card: cardEl.getBoundingClientRect().top,
      marker: markerEl.getBoundingClientRect().top,
    };
  });

  const scrolledBy = await page.locator("#viewport").evaluate((v) => {
    const target = Math.min(1200, v.scrollHeight - v.clientHeight);
    v.scrollTop = target;
    return v.scrollTop;
  });
  expect(scrolledBy).toBeGreaterThan(200);
  await settle(page);

  const after = await page.evaluate(() => {
    const cardEl = document.querySelector(
      ".review-margin-card.review-margin-comment",
    );
    const markerEl = document.querySelector(".review-comment-marker");
    return {
      card: cardEl.getBoundingClientRect().top,
      marker: markerEl.getBoundingClientRect().top,
    };
  });

  // Both actually moved with the scroll (anchor rose by roughly the scroll
  // amount) — proving we exercised real scrolling, not a no-op.
  expect(before.marker - after.marker).toBeGreaterThan(200);
  expect(before.card - after.card).toBeGreaterThan(200);

  // The card-to-anchor gap is preserved: they stayed aligned within a tight
  // tolerance across the large scroll.
  const gapBefore = before.card - before.marker;
  const gapAfter = after.card - after.marker;
  expect(Math.abs(gapAfter - gapBefore)).toBeLessThanOrEqual(2);

  expect(consoleErrors).toEqual([]);
});

test("the single viewport scrollbar sits past the comment column, not between it and the canvas", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await addCommentAtDocStart(page, "Right-margin comment");

  const geometry = await page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    const sidebar = document.getElementById("reviewSidebar");
    const page0 = document.querySelector(".page");
    const vr = viewport.getBoundingClientRect();
    const sr = sidebar.getBoundingClientRect();
    const pr = page0.getBoundingClientRect();
    return {
      viewportRight: vr.right,
      sidebarLeft: sr.left,
      sidebarRight: sr.right,
      pageRight: pr.right,
    };
  });

  // The comment column sits to the right of the page (unchanged spatial look)…
  expect(geometry.sidebarLeft).toBeGreaterThanOrEqual(geometry.pageRight);
  // …and inside the viewport's right edge, so the viewport's own scrollbar (at
  // its far-right edge) is past the comments, never between page and comments.
  expect(geometry.sidebarRight).toBeLessThanOrEqual(geometry.viewportRight + 1);

  expect(consoleErrors).toEqual([]);
});
