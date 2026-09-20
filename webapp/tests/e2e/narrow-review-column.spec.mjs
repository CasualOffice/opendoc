// HF-088 — the comments column must not swallow the page at narrow widths.
//
// The failure this guards was never "the element is missing"; the column was
// present, visible and 320px wide ON TOP of a 390px document. So nothing here
// asserts that an element exists. It hit-tests: a grid of points over the page
// sheet, asking the browser what is actually on top at each one. If the column
// covers the document, those points come back as review chrome and the test
// fails — which is the user's complaint, stated in the only terms that cannot
// be satisfied by an off-screen or zero-width element.
//
// `105` UX-019 (the whole editor at 390px: the Home tab collapsing to a single
// `⋯` menu, no way to type on a touch device) is a separate, larger row and is
// NOT addressed here. This spec deliberately says nothing about the ribbon.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

const WIDE = { width: 1440, height: 900 };
const TABLET = { width: 860, height: 900 };
const NARROW = { width: 620, height: 800 };
const PHONE = { width: 390, height: 844 };

const card = (page) =>
  page.locator("#reviewSidebar .review-margin-card.review-margin-comment").first();

/** Adds one real comment, so the column has something to render. */
async function addComment(page, body) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("NARROWANCHOR");
  await page.keyboard.press("Shift+Home");
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  await sidebar.locator('[data-testid="review-comment-composer"]').fill(body);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
  await expect(card(page)).toBeVisible();
}

/**
 * What the browser reports on top of the document sheet.
 *
 * Samples a grid over the visible part of the first page and classifies each
 * point: `document` when the topmost element is the page/canvas, `review` when
 * it is any part of the comments column, `other` for anything else.
 */
function hitTestPage(page, limitY = null) {
  return page.evaluate((limit) => {
    const sheet = document.querySelector(".page-wrap .page");
    const rect = sheet.getBoundingClientRect();
    const left = Math.max(rect.left, 0);
    const right = Math.min(rect.right, window.innerWidth);
    const top = Math.max(rect.top, 0);
    const bottom = Math.min(rect.bottom, limit ?? window.innerHeight);
    const result = {
      document: 0,
      review: 0,
      other: 0,
      samples: 0,
      reviewPoints: [],
      band: Math.round(bottom - top),
      windowHeight: window.innerHeight,
      windowWidth: window.innerWidth,
      leftmostReviewX: Infinity,
    };
    if (right <= left || bottom <= top) return result;
    const COLS = 8;
    const ROWS = 8;
    for (let c = 0; c < COLS; c++) {
      for (let r = 0; r < ROWS; r++) {
        const x = left + ((c + 0.5) * (right - left)) / COLS;
        const y = top + ((r + 0.5) * (bottom - top)) / ROWS;
        const hit = document.elementFromPoint(Math.round(x), Math.round(y));
        result.samples += 1;
        if (!hit) {
          result.other += 1;
        } else if (hit.closest("#reviewSidebar")) {
          result.review += 1;
          result.leftmostReviewX = Math.min(result.leftmostReviewX, Math.round(x));
          result.reviewPoints.push(`${Math.round(x)},${Math.round(y)}`);
        } else if (hit.closest(".page-wrap")) {
          result.document += 1;
        } else {
          result.other += 1;
        }
      }
    }
    return result;
  }, limitY);
}

/** Geometry of the comments column relative to the window. */
function columnBox(page) {
  return page.evaluate(() => {
    const el = document.getElementById("reviewSidebar");
    const rect = el.getBoundingClientRect();
    const viewport = document.getElementById("viewport");
    return {
      position: getComputedStyle(el).position,
      top: Math.round(rect.top),
      left: Math.round(rect.left),
      width: Math.round(rect.width),
      height: Math.round(rect.height),
      windowWidth: window.innerWidth,
      windowHeight: window.innerHeight,
      sheetMode: viewport.classList.contains("review-sheet"),
      scrollable: el.scrollHeight > el.clientHeight + 1,
      // Does the column push the page stack sideways?
      viewportScrollWidth: viewport.scrollWidth,
      viewportClientWidth: viewport.clientWidth,
    };
  });
}

test("at tablet width the column stays a margin beside the page", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await addComment(page, "Tablet-width comment");

  await page.setViewportSize(TABLET);
  await expect.poll(async () => (await columnBox(page)).sheetMode).toBe(false);

  const box = await columnBox(page);
  expect(box.position, "860px is still the margin shape").toBe("absolute");
  expect(box.top).toBeLessThan(box.windowHeight / 2);

  // At 860px the 816px sheet is wider than the room the 316px gutter leaves, so
  // the narrowed column overlaps the sheet's right edge — deliberately, and the
  // way Docs does it. What must remain true is that the document is still the
  // thing on screen: the text column is untouched and the overlap is confined
  // to the outer right margin.
  const hits = await hitTestPage(page);
  expect(hits.samples).toBeGreaterThan(0);
  expect(hits.document / hits.samples).toBeGreaterThan(0.7);
  expect(
    hits.leftmostReviewX,
    `the column reached into the page at ${hits.reviewPoints.join(" ")}`,
  ).toBeGreaterThan(hits.windowWidth * 0.6);

  expect(consoleErrors).toEqual([]);
});

for (const [name, size] of [
  ["620px", NARROW],
  ["390px", PHONE],
]) {
  test(`at ${name} the column becomes a bottom sheet and leaves the page alone`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(WIDE);
    await gotoEditor(page);
    await addComment(page, `Comment at ${name}`);
    const wide = await columnBox(page);

    await page.setViewportSize(size);

    // Wait on the GUARANTEE, not on a class name: no part of the comments
    // column may be on top of the page in the upper half of the screen. This
    // both settles the re-layout and states the defect — a column that floats
    // over the text fails here with the coordinates it covered.
    await expect
      .poll(async () => (await hitTestPage(page, Math.round(size.height / 2))).reviewPoints)
      .toEqual([]);
    const box = await columnBox(page);

    // A sheet: pinned to the bottom edge, full width, at most half the screen.
    expect(box.position).toBe("fixed");
    expect(box.left).toBe(0);
    expect(box.width).toBe(box.windowWidth);
    expect(box.height).toBeLessThanOrEqual(Math.round(box.windowHeight / 2) + 1);
    expect(box.top).toBeGreaterThanOrEqual(Math.floor(box.windowHeight / 2) - 1);

    // Above the sheet's top edge every point over the page is document. That
    // is the row's actual complaint, stated as a hit test: the old column sat
    // ON the text, so points in the reading area came back as review chrome.
    const hits = await hitTestPage(page, box.top);
    expect(hits.samples).toBeGreaterThan(0);
    expect(
      hits.review,
      `the comments covered the document at ${hits.reviewPoints.join(" ")}`,
    ).toBe(0);
    expect(hits.document).toBe(hits.samples);
    // …and that unoccluded band is a usable share of the screen, not a sliver.
    expect(hits.band).toBeGreaterThan(hits.windowHeight * 0.35);

    // The comment is still readable, in the sheet, on screen.
    const cardBox = await card(page).boundingBox();
    expect(cardBox, "the comment card must be laid out").not.toBeNull();
    expect(cardBox.y).toBeGreaterThanOrEqual(box.top - 1);
    expect(cardBox.y + cardBox.height).toBeLessThanOrEqual(box.windowHeight + 1);
    expect(cardBox.width).toBeGreaterThan(200);

    // And the document is still editable underneath it — "usable", not "present".
    await clickIntoFirstPage(page);
    await page.keyboard.type("STILLTYPING");
    await expect(page.locator("#a11yDocument")).toContainText("STILLTYPING");

    // The column costs the page stack no horizontal room: the reserved gutter
    // is gone, so the sideways scroll with comments open is the sideways scroll
    // without them. (That remaining scroll is the 816px sheet itself at phone
    // width — `105` UX-019 / HF-083, not this row.)
    const openOverflow = box.viewportScrollWidth - box.viewportClientWidth;
    await page.locator("#reviewClose").click();
    await expect(page.locator("#reviewSidebar")).toBeHidden();
    const closed = await columnBox(page);
    expect(openOverflow).toBe(closed.viewportScrollWidth - closed.viewportClientWidth);
    expect(wide.sheetMode).toBe(false);

    expect(consoleErrors).toEqual([]);
  });
}

test("the sheet is dismissable and gives the screen back", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  await addComment(page, "Dismiss me");

  await page.setViewportSize(PHONE);
  await expect
    .poll(async () => (await hitTestPage(page, Math.round(PHONE.height / 2))).reviewPoints)
    .toEqual([]);

  await page.locator("#reviewClose").click();
  await expect(page.locator("#reviewSidebar")).toBeHidden();

  // Every sampled point over the page is document again, including the bottom
  // half the sheet had.
  const hits = await hitTestPage(page);
  expect(hits.review).toBe(0);
  expect(hits.document).toBe(hits.samples);

  expect(consoleErrors).toEqual([]);
});
