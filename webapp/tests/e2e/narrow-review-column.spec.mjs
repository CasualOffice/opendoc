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
import { clickIntoFirstPage, expect, gotoEditor, moveCaretToDocStart, stableBox, test } from "./fixtures.mjs";

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
    // Where the document AREA starts: the top of `#viewport`'s content box.
    // Everything above it is chrome, and chrome is the only thing that can push
    // the reader's band down, because the sheet is pinned to the bottom edge.
    // Read off the scroll container and not off the page: the page's own top
    // moves with the scroll offset, and the band the reader gets does not.
    const viewport = document.getElementById("viewport");
    const vpStyle = getComputedStyle(viewport);
    const readingTop =
      viewport.getBoundingClientRect().top +
      parseFloat(vpStyle.borderTopWidth) +
      parseFloat(vpStyle.paddingTop);
    const result = {
      document: 0,
      review: 0,
      other: 0,
      samples: 0,
      reviewPoints: [],
      band: Math.round(bottom - top),
      pageHeight: Math.round(rect.height),
      readingTop: Math.round(readingTop),
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

    // Wait for the layout to SETTLE before measuring it. Resizing into this rung
    // turns reflow on, and reflow's width feed is debounced — deliberately, so a
    // drag does not relayout per frame. Before reflow existed the page kept paper
    // width whatever the window did, so there was no settling to wait for and no
    // race to see; now there is, and reading `scrollWidth` mid-flight measures
    // the paper layout that is about to be replaced (794 into 390).
    //
    // Polled on the guarantee rather than slept on: the page stack fits the
    // window once reflow has been applied.
    await expect
      .poll(async () => (await columnBox(page)).viewportScrollWidth, {
        message: "the page stack must settle to the window's width once reflow applies",
      })
      .toBeLessThanOrEqual(size.width);
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
    //
    // Two claims, because "a usable share of the SCREEN" is not a claim about
    // how much the document happens to say. This was one line —
    // `hits.band > windowHeight * 0.35` — and `hits.band` is the visible extent
    // of the PAGE ELEMENT, clamped at both ends. In paper mode that clamp never
    // bit: a page is 11in of sheet whatever is written on it, so the element was
    // always taller than the window and `band` could only ever mean "the screen
    // the sheet left over". Reflow (ADR-046, `docs/151` §6) stops padding a short
    // document out to a sheet — a tile is trimmed to its content — so at 390x844
    // the whole `rich` fixture is 269px of text with 271px of empty desk between
    // its foot and the sheet's head, and the old line read that 269 as a sliver.
    // It was measuring the fixture, not the reader's room. Nothing in the product
    // could have satisfied it either: the only way to make a 269px document 296px
    // tall is to print blank paper under it, which is the thing reflow exists to
    // stop doing.
    //
    // So: the ROOM is chrome-and-sheet geometry, and the document must occupy all
    // of that room it is long enough to reach.
    const readingRoom = box.top - hits.readingTop;
    expect(
      readingRoom,
      "the chrome above the document and the sheet below it must leave a readable band",
    ).toBeGreaterThan(hits.windowHeight * 0.35);
    // Nothing may sit in that room but the document: either the page fills it, or
    // the page is shorter and every line of it is on screen. The -1 absorbs the
    // subpixel rounding `band` and `pageHeight` each carry.
    expect(
      hits.band,
      "the document must occupy the band the sheet leaves it, not be pushed out of it",
    ).toBeGreaterThanOrEqual(Math.min(hits.pageHeight, readingRoom) - 1);

    // The comment is still readable, in the sheet, on screen.
    const cardBox = await stableBox(card(page));
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
  //
  // Sampled down to the top of the phone tier's PERMANENT bottom chrome, not to
  // the foot of the window (docs/148). The command bar is docked there at this
  // width and was never the sheet's to give back — before it existed this read
  // `hitTestPage(page)` and measured the same thing, because nothing was in the
  // way. Asserting to the foot of the window would charge the sheet for chrome
  // it does not own, which is the shape of guard that reddens `main` for a
  // change that removed nothing.
  const bottomChrome = await page.evaluate(() => {
    const bar = document.getElementById("compactToolbar");
    if (!bar || bar.hidden || getComputedStyle(bar).position !== "fixed") return window.innerHeight;
    return Math.round(bar.getBoundingClientRect().top);
  });
  const hits = await hitTestPage(page, bottomChrome);
  expect(hits.review).toBe(0);
  expect(hits.document).toBe(hits.samples);

  expect(consoleErrors).toEqual([]);
});

/**
 * Every control on the bottom chrome, hit-tested at the point a finger aims at.
 *
 * The CONTROLS and not the bar's bounding box, deliberately. `--h-compact-bar` is
 * nominally `clamp(38px, 3.4vh, 46px)` — 38px at this height — while the painted
 * bar is 41px (`.compact-toolbar`'s own `min-height: 40px` plus its 1px top
 * border), so every bottom reserve derived from that token is 3px short and the
 * sheet's bottom edge lands on the bar's border rather than above it. That 3px is
 * a token defect to fix where the token lives; it is not a command a finger
 * cannot reach, and a guard written against the bounding box would be asserting
 * the token's arithmetic instead of the thing the user lost.
 *
 * `self` counts a hit that landed inside the control, `review` one that landed in
 * the comments sheet — which is the whole complaint — and `other` anything else.
 */
function bottomChromeReach(page) {
  return page.evaluate(() => {
    const out = { controls: 0, self: 0, review: 0, other: 0, covered: [] };
    const surfaces = [document.getElementById("compactToolbar"), document.querySelector(".footer")];
    for (const surface of surfaces) {
      if (!surface || surface.hidden || !surface.getClientRects().length) continue;
      for (const control of surface.querySelectorAll("button, [role='button'], a[href], select")) {
        if (control.hidden || control.disabled || !control.getClientRects().length) continue;
        const box = control.getBoundingClientRect();
        if (box.width < 1 || box.height < 1) continue;
        const x = Math.round(box.left + box.width / 2);
        const y = Math.round(box.top + box.height / 2);
        out.controls += 1;
        const hit = document.elementFromPoint(x, y);
        if (hit && (control.contains(hit) || hit.contains(control))) out.self += 1;
        else if (hit?.closest("#reviewSidebar")) {
          out.review += 1;
          out.covered.push(`${control.id || control.className}@${x},${y}`);
        } else out.other += 1;
      }
    }
    return out;
  });
}

// The comments sheet must not swallow the bottom command surface.
//
// `inset: auto 0 0 0` is the right shape for a shell whose commands are at the
// top, and the wrong one for this shell. Measured at 390x844 before the fix: the
// sheet occupied 645-844 while `#compactToolbar` is fixed at 773-814 and the
// status bar at 814-844, so opening one comment hid the phone's entire command
// surface. Word for mobile stacks its bottom ribbon sheet above the command row
// and Google Docs' sheets sit above the keyboard-attached row; ONLYOFFICE is not
// the authority here, because their phone chrome is a top Navbar plus toolbars
// INSIDE the sheet (`apps/documenteditor/mobile/src/view/edit/Edit.jsx`) and they
// have no global bottom row to swallow.
//
// This is also the precondition of the test above: it treats the command bar as
// PERMANENT chrome that was "never the sheet's to give back", which is only true
// if the sheet never had it.
//
// `docs/148` §5's region table records the oversight in two adjacent rows — the
// compact toolbar "docked to the bottom, above the status bar", and four rows
// later the comment column "bottom sheet (already, at 700px) | HF-088,
// UNCHANGED". The rung that put a command surface at the bottom edge never
// re-asked what was already parked there.
//
// Three rungs, because the standoff is not one number. Below 620px
// (`PHONE_MAX_WIDTH`) the chrome is the status bar plus the docked command bar;
// between 621 and 700px (`REVIEW_SHEET_MAX_WIDTH`) the sheet still applies but
// `phone-mode` does not, so the status bar is the only bottom chrome and it is in
// the normal flow — a rung the fix reaches through `--h-footer` as the fallback
// and which nothing else in this file visits.
for (const [name, size] of [
  ["390px", PHONE],
  ["620px", NARROW],
  ["660px", { width: 660, height: 800 }],
]) {
  test(`at ${name} the sheet stands off the bottom chrome instead of covering it`, async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize(WIDE);
    await gotoEditor(page);
    await addComment(page, `Above the bar at ${name}`);

    await page.setViewportSize(size);
    await expect.poll(async () => (await columnBox(page)).sheetMode).toBe(true);
    await expect(page.locator("#reviewSidebar")).toBeVisible();
    // No wait on reflow here, unlike the rung tests above: this guard reads the
    // sheet's and the bar's own boxes and neither comes from the debounced width
    // feed. Reflow is not even on at 660px — it arms below the phone rung — so
    // waiting for the page stack to fit the window would hang on the paper
    // layout that is correct at this width.
    await expect
      .poll(async () => (await columnBox(page)).height)
      .toBeGreaterThan(0);

    // The observable, proved live: there ARE controls down there to cover.
    const reach = await bottomChromeReach(page);
    expect(reach.controls, "the bottom chrome must carry controls at this rung").toBeGreaterThan(0);

    // THE GUARANTEE: not one of them is behind the sheet.
    expect(
      reach.review,
      `the comments sheet covered ${reach.covered.join(" ")}`,
    ).toBe(0);
    expect(reach.self, "every bottom-chrome control answers its own hit test").toBe(reach.controls);

    expect(consoleErrors).toEqual([]);
  });
}
