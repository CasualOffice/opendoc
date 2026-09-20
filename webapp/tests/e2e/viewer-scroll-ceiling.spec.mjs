// docs/113 §8.6 — the browser's own scroll ceiling, and the pages behind it.
//
// The viewer built one sheet per page in normal flow, so the scroll container
// was as tall as the document. A browser stops scrolling at 2^24 = 16,777,216
// CSS px: past that, the end of the document is simply unreachable, silently.
// Measured on the owner's file at 27,549,376 px, and at 16,910,594 px for a
// document only 15,687 pages long — which is why `MAX_VIEWER_BLOCKS` was
// pinned below what the engine can hold.
//
// Everything here is about the case every other browser spec in this repo
// misses: the LAST page of a document far too tall to lay out flat. The unit
// half of the guard (the arithmetic, at 250,000 pages) is
// `tests/page_scroll.test.mjs`; this is the half only a browser can answer —
// a real scroller, a real sheet at the end of it, and real ink on that sheet.
//
// The ceiling is reached here by ZOOM rather than by length, deliberately: a
// 3,300-page document at 500% is 17.5 million px of document, past the wall,
// and it opens in seconds instead of the two minutes 775,000 paragraphs would
// cost. It is the same ceiling — the viewer caps the CSS height of the band,
// not a page count — and it is a real thing a reader can do to a long report.
import { test, expect, documentPageCount, gotoEditor, pageSheet } from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

/** The wall, as the usual figure has it: 2^24. This Chromium's own is 2^25 —
 *  probed with the cap removed, a 34,993,644 px band was clamped to
 *  33,554,432 px and the last 271 pages became unreachable. The viewer is
 *  bounded by the smaller figure, so the smaller one is what is asserted. */
const BROWSER_SCROLL_WALL = 16_777_216;

/** 6,600 Letter pages at 500% zoom is 6,600 × (1056 × 5 + 22) = 35.0M px of
 *  document: past BOTH walls, so removing the cap does not merely build an
 *  over-tall container, it makes the end of this document genuinely
 *  unreachable — which is what the guard below has to be able to catch. The
 *  fixture is explicit page breaks, so it opens in about a second. */
const PAGES = 6_600;
const ZOOM = "500%";

/** Ink: any pixel that is not paper-white. */
const INK = (canvas) => {
  const context = canvas.getContext("2d", { willReadFrequently: true });
  const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
  for (let i = 0; i < data.length; i += 4) {
    if (data[i] !== 255 || data[i + 1] !== 255 || data[i + 2] !== 255) return true;
  }
  return false;
};

test.describe.configure({ mode: "serial" });

test.describe("a document taller than the browser can scroll", () => {
  /** @type {import("@playwright/test").Page} */
  let page;

  test.beforeAll(async ({ browser }) => {
    page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    await gotoEditor(page);
    await page.setInputFiles("#file", {
      name: "tall.docx",
      mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      buffer: Buffer.from(makeLargeDocx(PAGES)),
    });
    await expect.poll(() => documentPageCount(page), { timeout: 180_000 }).toBe(PAGES);
    // Zoom through the control a reader uses, not an internal.
    await page.locator("#zoom").fill(ZOOM);
    await page.locator("#zoom").press("Enter");
    await expect.poll(() => page.locator("#zoom").inputValue()).toBe(ZOOM);
    await expect.poll(() => page.locator(".page-wrap").count()).toBeGreaterThan(0);
  });

  test.afterAll(async () => {
    await page?.close();
  });

  test("the scroll container stays inside the browser's limit", async () => {
    const measured = await page.evaluate(() => {
      const viewport = document.getElementById("viewport");
      const band = document.querySelector(".page-band");
      return {
        scrollHeight: viewport.scrollHeight,
        bandHeight: band ? band.getBoundingClientRect().height : -1,
        sheets: document.querySelectorAll(".page-wrap").length,
      };
    });
    // The document is genuinely past the wall: 6,600 pages at 500% is 35.0M px
    // of paper. If this stops being true the guard has stopped guarding.
    expect(PAGES * (1056 * 5 + 22)).toBeGreaterThan(BROWSER_SCROLL_WALL);
    expect(
      measured.scrollHeight,
      `a ${measured.scrollHeight} px scroll container has pages no scroll can reach`,
    ).toBeLessThan(BROWSER_SCROLL_WALL);
    // ...and it is the band that is bounded, not the pages: only a window of
    // sheets exists at all.
    expect(measured.bandHeight).toBeLessThan(BROWSER_SCROLL_WALL);
    expect(measured.sheets).toBeLessThan(20);
  });

  test("the last page can be scrolled to, and it has ink on it", async () => {
    const last = await pageSheet(page, PAGES);
    await last.locator("canvas").waitFor({ state: "attached", timeout: 60_000 });
    expect(await last.locator("canvas").evaluate(INK), `page ${PAGES} painted nothing`).toBe(true);
    // The scrollbar is at the end of the document when the last page is: the
    // position it reports is the position in the WHOLE document, not in the
    // window of sheets that happens to exist.
    const atEnd = await page.evaluate(() => {
      const viewport = document.getElementById("viewport");
      return viewport.scrollTop / (viewport.scrollHeight - viewport.clientHeight);
    });
    expect(atEnd).toBeGreaterThan(0.99);
  });

  test("find scrolls to a match on a page nowhere near the viewport", async () => {
    // Back to the top, so the match is thousands of pages away.
    await page.evaluate(() => {
      document.getElementById("viewport").scrollTop = 0;
    });
    await pageSheet(page, 1);
    const needle = `Page ${PAGES} of ${PAGES}`;
    await page.locator("#findBtn").click();
    await page.locator("#findInput").fill(needle);
    await page.locator("#findInput").press("Enter");
    await expect(page.locator("#findStatus")).toContainText("1 match", { timeout: 60_000 });
    // The highlight must exist AND be on screen: a selection updated on a page
    // the viewer never scrolled to is the silent failure this is written for.
    const highlight = page.locator(".overlay .highlight").first();
    await expect(highlight).toBeVisible({ timeout: 60_000 });
    const onScreen = await page.evaluate(() => {
      const mark = document.querySelector(".overlay .highlight");
      const viewport = document.getElementById("viewport").getBoundingClientRect();
      const rect = mark.getBoundingClientRect();
      return rect.top >= viewport.top - 1 && rect.bottom <= viewport.bottom + 1;
    });
    expect(onScreen, "the match was selected off screen").toBe(true);
    // And it is the last page's own sheet the highlight sits on.
    const onPage = await highlight.evaluate(
      (element) => element.closest(".page-wrap")?.dataset.pageNumber,
    );
    expect(Number(onPage)).toBe(PAGES);
  });

  test("arrow navigation keeps the caret inside the viewport at the far end", async () => {
    await pageSheet(page, PAGES);
    // Click inside the part of the sheet that is ON SCREEN: at 500% a page is
    // 5,280 px tall, so its own centre is usually somewhere outside the window
    // and `page.mouse.click` does not scroll to reach it.
    const point = await page.evaluate((n) => {
      const box = document
        .querySelector(`.page-wrap[data-page-number="${n}"]`)
        .getBoundingClientRect();
      const view = document.getElementById("viewport").getBoundingClientRect();
      const top = Math.max(box.top, view.top);
      const bottom = Math.min(box.bottom, view.bottom);
      const left = Math.max(box.left, view.left);
      const right = Math.min(box.right, view.right);
      return { x: (left + right) / 2, y: (top + bottom) / 2 };
    }, PAGES);
    await page.mouse.click(point.x, point.y);
    // The click lands on the page's only line, which is near its top and so
    // usually above the viewport — clicking does not scroll, in this editor or
    // in Word. What must not happen is arrowing leaving it there.
    await expect(page.locator(".overlay .caret")).toHaveCount(1);

    const caretPosition = () =>
      page.evaluate(() => {
        const caret = document.querySelector(".overlay .caret");
        if (!caret) return "no caret";
        const viewport = document.getElementById("viewport").getBoundingClientRect();
        const rect = caret.getBoundingClientRect();
        if (rect.top < viewport.top - 2) return `${Math.round(viewport.top - rect.top)} px above`;
        if (rect.top > viewport.bottom) return `${Math.round(rect.top - viewport.bottom)} px below`;
        return "inside";
      });
    for (let i = 0; i < 6; i++) {
      await page.keyboard.press("ArrowUp");
      // Polled, because the answer must SETTLE inside the viewport — but with
      // a short budget, because "it came back a second later" is not the same
      // product as "it never left".
      await expect
        .poll(caretPosition, { timeout: 2_000, message: `after ${i + 1} ArrowUp presses` })
        .toBe("inside");
    }
    // Position, not painted height, and deliberately so: on this branch the
    // engine reports a ZERO-HEIGHT caret rect once ArrowUp reaches the
    // paragraph carrying a page break, so the caret is placed correctly and
    // paints nothing. Reproduced at 100% zoom on a 6-page document, where no
    // part of this work is engaged (`scale` is exactly 1 and the sheets sit at
    // their true positions), so it is not the page band's — it is the same
    // family as the ArrowUp fixes that landed on `main` after this branch was
    // cut. Recorded in `docs/113` §8.6 rather than silently asserted around.
  });

  test("a comment card stays pinned to its anchor while scrolling", async () => {
    // The review column positions cards against their markers. Under
    // compression a pixel of scroll is `scale` pixels of document, so a card
    // whose position was stored in scroll coordinates slides away from the
    // marker it points at as soon as the reader scrolls — by (scale - 1) × the
    // distance, which here is more than a card's height within one page.
    await pageSheet(page, 1);
    const point = await page.evaluate(() => {
      const box = document.querySelector('.page-wrap[data-page-number="1"]').getBoundingClientRect();
      const view = document.getElementById("viewport").getBoundingClientRect();
      return { x: Math.max(box.left, view.left) + 200, y: Math.max(box.top, view.top) + 500 };
    });
    await page.mouse.click(point.x, point.y);
    await page.keyboard.press("Home");
    for (let i = 0; i < 6; i++) await page.keyboard.press("Shift+ArrowRight");
    await page.locator("#selComment").click();
    const composer = page.locator('[data-testid="review-comment-composer"]');
    await expect(composer).toBeVisible();
    // QZX, not a marker containing "@": the corpus is full of e-mail addresses
    // and every search matches those.
    await composer.fill("QZXNOTE");
    await page.locator('[data-testid="review-comment-submit"]').click();

    // The offset between a card and its own marker, or a reason there is none.
    const drift = () =>
      page.evaluate(() => {
        const card = [...document.querySelectorAll(".review-margin-card")].find((element) =>
          element.textContent.includes("QZXNOTE"),
        );
        if (!card) return "no card";
        const marker = document.querySelector(
          ".overlay .review-comment-marker, .overlay .highlight",
        );
        if (!marker) return "no marker";
        return Math.round(card.getBoundingClientRect().top - marker.getBoundingClientRect().top);
      });
    await expect.poll(drift).not.toBe("no card");
    const settled = await drift();
    expect(typeof settled, `card was not positioned: ${settled}`).toBe("number");

    // Scroll a fraction of a page — far too little to unmount the card, and
    // under compression far more than enough to move the band under it.
    await page.evaluate(() => {
      document.getElementById("viewport").scrollTop += 200;
    });
    await page.waitForTimeout(200);
    const after = await drift();
    expect(typeof after, `card vanished after scrolling: ${after}`).toBe("number");
    expect(
      Math.abs(after - settled),
      `the card slid ${after - settled} px away from its marker`,
    ).toBeLessThanOrEqual(2);
  });

  test("the Pages navigator windows its thumbnails, and still jumps", async () => {
    await pageSheet(page, PAGES);
    await page.locator("#railPages").click();
    const cards = page.locator("#pagesBody .page-thumb");
    // Bounded: a card per page would be 3,300 renderPage calls on the main
    // thread, which is not a navigator, it is a hang.
    await expect.poll(() => cards.count(), { timeout: 60_000 }).toBeGreaterThan(0);
    expect(await cards.count()).toBeLessThanOrEqual(40);
    // It says it is a window rather than letting 40 cards read as a document.
    await expect(page.locator("#pagesBody .panel-window-note")).toContainText(
      `of ${PAGES.toLocaleString()}`,
    );
    // The cards follow the reader: scrolled to the end, they are the end.
    const numbers = await cards.evaluateAll((els) => els.map((el) => Number(el.dataset.page)));
    expect(Math.max(...numbers)).toBe(PAGES);
    // And a card still jumps. Pick one a screen or two back, so the jump is a
    // real move rather than a no-op.
    const target = PAGES - 20;
    await page.locator(`#pagesBody .page-thumb[data-page="${target}"]`).click();
    await expect(page.locator(`.page-wrap[data-page-number="${target}"]`)).toBeVisible();
    await page.locator("#railPages").click();
  });
});
