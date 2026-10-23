// docs/174 TA-10: zoomed in past the window's width, moving the caret to the end
// of a line left it off screen to the right (x=2288 in a 1440-wide window at
// 310%), because the scroll-into-view rule only moved `scrollTop`. Word and Docs
// both follow the caret horizontally. This drives the real keys at a real zoom
// and asserts the guarantee: the caret is inside the viewport, both ways.
import { clickIntoFirstPage, expect, gotoEditor, moveCaretToDocStart, test } from "./fixtures.mjs";

async function setZoom(page, value) {
  const zoom = page.locator("#zoom");
  await zoom.fill(value);
  await zoom.press("Enter");
  await expect(zoom).toHaveValue(value);
}

/** The caret's screen rect against the viewport's visible box (scrollbars excluded). */
async function caretInView(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    const viewport = document.querySelector("#viewport");
    if (!caret) return { found: false };
    const c = caret.getBoundingClientRect();
    const v = viewport.getBoundingClientRect();
    const right = v.left + viewport.clientWidth;
    const bottom = v.top + viewport.clientHeight;
    return {
      found: true,
      x: Math.round(c.left),
      inside: c.left >= v.left && c.right <= right && c.top >= v.top && c.bottom <= bottom,
      scrollLeft: Math.round(viewport.scrollLeft),
      scrollable: viewport.scrollWidth > viewport.clientWidth,
    };
  });
}

test("the caret stays in view horizontally when zoomed, at both ends of a line", async ({ page, consoleErrors }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // A paragraph long enough that its first line runs the full measure, so the
  // line's end sits at the right margin of a page three windows wide.
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData("text/plain", `${"Wide line that runs to the right margin of the page. ".repeat(6)}\n`);
    document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await setZoom(page, "300%");
  await moveCaretToDocStart(page);

  const start = await caretInView(page);
  expect(start.scrollable, "at 300% the page must be wider than the window, or this proves nothing").toBe(true);
  expect(start.inside).toBe(true);

  await page.keyboard.press("End");
  await expect.poll(async () => (await caretInView(page)).inside, { message: "caret after End" }).toBe(true);
  const end = await caretInView(page);
  expect(end.scrollLeft, "End must have scrolled right to reveal the caret").toBeGreaterThan(start.scrollLeft);

  await page.keyboard.press("Home");
  await expect.poll(async () => (await caretInView(page)).inside, { message: "caret after Home" }).toBe(true);
  expect((await caretInView(page)).scrollLeft).toBeLessThan(end.scrollLeft);
  expect(consoleErrors).toEqual([]);
});
