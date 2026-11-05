// Zoom keeps the reader's place (audit CANVAS-08, CANVAS-V-02; `zoom_anchor.mjs`),
// and so do the other moves of the view this lane changed: PageDown, the jumps
// to the story's edges, and Page X of Y.
//
// Measured before: with the caret on line 2 of page 1 at y=456, 300% put it at
// y=969, below the 770px viewport, scrollTop and scrollLeft still 0; and
// Ctrl+wheel at (800, 600) zoomed around the top centre, not the pointer.
// Word and ONLYOFFICE keep the caret (buttons, keys) or the pointer (wheel).
import { MOD, clickIntoFirstPage, documentPageCount, expect, gotoEditor, moveCaretToDocStart, pageSheet, stableBox, test } from "./fixtures.mjs";

/** Where the caret is on screen, and the viewport it must stay inside. */
async function caretInView(page) {
  return page.evaluate(() => {
    const caret = document.querySelector("#pages .overlay .caret");
    const view = document.getElementById("viewport").getBoundingClientRect();
    if (!caret) return null;
    const r = caret.getBoundingClientRect();
    return {
      caret: { left: r.left, top: r.top, bottom: r.bottom },
      view: { left: view.left, top: view.top, right: view.right, bottom: view.bottom },
    };
  });
}

/** The fraction of its page a client point sits at, and which page. */
async function pagePoint(page, x, y) {
  return page.evaluate(([px, py]) => {
    const wraps = [...document.querySelectorAll(".page-wrap")];
    const wrap = wraps.find((el) => {
      const r = el.getBoundingClientRect();
      return px >= r.left && px <= r.right && py >= r.top && py <= r.bottom;
    });
    if (!wrap) return null;
    const r = wrap.getBoundingClientRect();
    return { number: wrap.dataset.pageNumber, fx: (px - r.left) / r.width, fy: (py - r.top) / r.height };
  }, [x, y]);
}

/** Where that fraction of that page is on screen now. */
async function clientOf(page, point) {
  return page.evaluate((p) => {
    const wrap = document.querySelector(`.page-wrap[data-page-number="${p.number}"]`);
    if (!wrap) return null;
    const r = wrap.getBoundingClientRect();
    return { x: r.left + p.fx * r.width, y: r.top + p.fy * r.height };
  }, point);
}

/** Makes the document several pages long (the fixture is one page, which a
 *  PageDown or a "Page 2 of" cannot be tested on), caret back at the start. */
async function longDocument(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  const lines = Array.from({ length: 150 }, (_, i) => `PLACE_KEEPING_LINE_${String(i).padStart(3, "0")}`).join("\n");
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  }, lines);
  await expect.poll(() => documentPageCount(page)).toBeGreaterThan(2);
  await moveCaretToDocStart(page);
}

test("zooming to 300% keeps the caret in view", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowDown");
  const before = await caretInView(page);
  expect(before).not.toBeNull();

  // The status bar's +, up the ladder to 300%.
  for (let i = 0; i < 8; i += 1) {
    if ((await page.locator("#zoom").inputValue()) === "300%") break;
    await page.locator("#zoomIn").click();
  }
  await expect(page.locator("#zoom")).toHaveValue("300%");
  await expect
    .poll(async () => {
      const now = await caretInView(page);
      if (!now) return "no caret";
      const inside =
        now.caret.left >= now.view.left &&
        now.caret.left <= now.view.right &&
        now.caret.top >= now.view.top &&
        now.caret.bottom <= now.view.bottom;
      return inside ? "in view" : JSON.stringify(now);
    })
    .toBe("in view");
  expect(consoleErrors).toEqual([]);
});

test("Ctrl+wheel keeps the point under the pointer within 4px", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  const sheet = await stableBox(page.locator(".page-wrap").first());
  // A point on page 1's centre line, well down the sheet. While the page is
  // narrower than the window it stays centred, so no scroll can hold a point
  // off the centre line still; once it is wider, the horizontal scroll does.
  const x = Math.round(sheet.x + sheet.width * 0.5);
  const y = Math.round(Math.min(sheet.y + sheet.height * 0.6, 640));
  await page.mouse.move(x, y);

  for (const steps of [3, 2]) {
    const point = await pagePoint(page, x, y);
    expect(point, "the pointer is over a page").not.toBeNull();
    const zoomBefore = await page.locator("#zoom").inputValue();
    await page.keyboard.down("Control");
    for (let i = 0; i < steps; i += 1) await page.mouse.wheel(0, -100);
    await page.keyboard.up("Control");
    await expect(page.locator("#zoom")).not.toHaveValue(zoomBefore);
    await expect
      .poll(async () => {
        const now = await clientOf(page, point);
        return now ? Math.round(Math.hypot(now.x - x, now.y - y)) : 999;
      }, { message: "the document point under the pointer drifted" })
      .toBeLessThanOrEqual(4);
  }

  // And back out: zooming out keeps the same point too.
  const point = await pagePoint(page, x, y);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, 100);
  await page.keyboard.up("Control");
  await expect
    .poll(async () => {
      const now = await clientOf(page, point);
      return now ? Math.round(Math.hypot(now.x - x, now.y - y)) : 999;
    })
    .toBeLessThanOrEqual(4);
  expect(consoleErrors).toEqual([]);
});

// Audit CANVAS-10: in a 612px viewport the first PageDown scrolled 146px. Word
// and Docs step one screen less a line and keep the caret's place on screen.
test("PageDown steps one screen less a line, and the caret keeps its place", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await longDocument(page);
  await page.keyboard.press("ArrowDown");
  const before = await page.evaluate(() => {
    const view = document.getElementById("viewport");
    const caret = document.querySelector("#pages .overlay .caret").getBoundingClientRect();
    return { scroll: view.scrollTop, client: view.clientHeight, caretY: caret.top, line: caret.height };
  });
  await page.keyboard.press("PageDown");
  await expect
    .poll(() => page.evaluate(() => document.getElementById("viewport").scrollTop))
    .toBeGreaterThan(before.scroll + before.client - 3 * before.line);
  const after = await page.evaluate(() => {
    const view = document.getElementById("viewport");
    const caret = document.querySelector("#pages .overlay .caret").getBoundingClientRect();
    return { scroll: view.scrollTop, caretY: caret.top };
  });
  expect(after.scroll - before.scroll, "one screen less a line").toBeLessThanOrEqual(before.client);
  expect(Math.abs(after.caretY - before.caretY), "the caret keeps its place on screen").toBeLessThanOrEqual(before.line);
  expect(consoleErrors).toEqual([]);
});

// Audit CANVAS-09: scrolled to pages 2 and 6, the status bar still said
// "Page 1 of 14"; it followed only the caret. Word and ONLYOFFICE name the
// page most in view while scrolling, and the caret's page once it moves.
test("Page X of Y follows the page in view while scrolling, and the caret after", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await longDocument(page);
  await expect(page.locator("#statPages")).toContainText(/^Page 1 of/);
  await pageSheet(page, 2);
  await page.evaluate(() => document.querySelector('.page-wrap[data-page-number="2"]').scrollIntoView({ block: "start" }));
  await expect(page.locator("#statPages")).toContainText(/^Page 2 of/);
  // The caret moves (typing would too): its page is the answer again.
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("#statPages")).toContainText(/^Page 1 of/);
  expect(consoleErrors).toEqual([]);
});

// CANVAS-V-02: Ctrl+wheel to 161% left the portrait page's left edge 233px into
// the window while its right edge was off screen. The band is as wide as the
// WIDEST page, so in a document with a landscape section every portrait page is
// centred in a band wider than itself, and an anchor at the left margin holds
// that offset still. Once the page is wider than the window, Word and ONLYOFFICE
// keep about 24px beside it. `?fixture=sections` is four portrait pages and two
// landscape ones (`ruler-landscape.spec.mjs` pins its geometry); the condition
// is created, not found: on a one-orientation document the scroll clamp alone
// hides the gutter.
test("a page zoomed wider than the window keeps at most a 24px side gutter", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/editor.html?fixture=sections");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return status !== null && status.textContent === "" && document.querySelectorAll(".page-wrap").length > 0 && document.body.dataset.fontsReady === "true";
    },
    null,
    { timeout: 45_000 },
  );
  const sheet = await stableBox(page.locator('.page-wrap[data-page-number="1"]'));
  // The left margin of page 1: the point a pure anchor holds still, offset and all.
  await page.mouse.move(Math.round(sheet.x + 30), Math.round(Math.min(sheet.y + 300, 600)));
  await page.keyboard.down("Control");
  for (let i = 0; i < 5; i += 1) {
    const before = await page.locator("#zoom").inputValue();
    await page.mouse.wheel(0, -100);
    await expect(page.locator("#zoom")).not.toHaveValue(before);
  }
  await page.keyboard.up("Control");
  await expect(page.locator("#zoom")).toHaveValue("161%");
  const gutter = () =>
    page.evaluate(() => {
      const view = document.getElementById("viewport");
      const v = view.getBoundingClientRect();
      const left = v.left + view.clientLeft;
      const right = left + view.clientWidth;
      const r = document.querySelector('.page-wrap[data-page-number="1"]').getBoundingClientRect();
      const band = document.querySelector(".page-band").getBoundingClientRect();
      return { wider: r.width > right - left, bandWider: band.width > r.width + 100, left: Math.round(r.left - left), right: Math.round(right - r.right) };
    });
  // The preconditions, explicit: the page no longer fits the window, and the
  // band is wider than the page (the landscape section).
  expect(await gutter()).toMatchObject({ wider: true, bandWider: true });
  await expect.poll(async () => { const g = await gutter(); return Math.max(g.left, g.right); }).toBeLessThanOrEqual(25);
  expect(consoleErrors).toEqual([]);
});

// Ctrl+Home and Ctrl+End are jumps: they put the caret at the story's edge and
// bring it into view. (A copy of PageDown's keep-the-screen-height step once
// sat in this path and threw on every jump, before the caret was drawn.)
test("Ctrl+End and Ctrl+Home jump to the document's edges and show the caret", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await longDocument(page);
  const total = await documentPageCount(page);
  await page.keyboard.press(`${MOD}+End`);
  await expect(page.locator("#statPages")).toContainText(new RegExp(`^Page ${total} of`));
  await expect.poll(async () => {
    const now = await caretInView(page);
    return now && now.caret.top >= now.view.top && now.caret.bottom <= now.view.bottom ? "in view" : JSON.stringify(now);
  }).toBe("in view");
  await page.keyboard.press(`${MOD}+Home`);
  await expect(page.locator("#statPages")).toContainText(/^Page 1 of/);
  await expect.poll(async () => {
    const now = await caretInView(page);
    return now && now.caret.top >= now.view.top && now.caret.bottom <= now.view.bottom ? "in view" : JSON.stringify(now);
  }).toBe("in view");
  expect(consoleErrors).toEqual([]);
});
