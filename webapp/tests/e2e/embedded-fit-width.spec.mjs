// An embedded editor fits the page to its container instead of clipping it.
//
// The embedding playground mounts `editor.html` in a ~750px frame. At the
// default 100% zoom a Letter page plus its margins is wider than that, so the
// frame scrolled sideways and the first thing a visitor saw on the page that
// exists to sell embeddability was a clipped document — the title cut to
// "OpenDoc Feature Test Docur…" and the status bar to "11,516 character".
//
// Word for the web fits the page to its container rather than shipping a
// horizontal scrollbar, and a host that configures nothing has to land there
// too — so the choice is made by the editor on open (`openingZoomMode` in
// `view_zoom.mjs`), not by a parameter the host has to know about.
//
// The assertion is the GUARANTEE — "the document is not clipped" — rather than
// "zoomMode === 'fit-width'", so a different mechanism that also fits would keep
// this green and a fit mode that somehow still overflowed would not.
import { test, expect, gotoEditor } from "./fixtures.mjs";

/** How far the frame's document overflows its own viewport, and what the zoom
 *  control reads — both from inside the frame the host mounted. */
async function overflow(frame) {
  return frame.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
    pageWidth: document.querySelector(".page-wrap")?.getBoundingClientRect().width ?? 0,
    viewportWidth: document.getElementById("pages").clientWidth,
    zoom: document.getElementById("zoom").value,
  }));
}

/** Mounts `editor.html` in a host-sized frame, exactly as `embed.html` and the
 *  playground do: a plain iframe in a narrow column. */
async function mountNarrow(page, width) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/embedding.html");
  const handle = await page.evaluateHandle(async (w) => {
    const frame = document.createElement("iframe");
    frame.style.cssText = `display:block;width:${w}px;height:640px;border:0;position:fixed;left:0;top:0;z-index:99999;background:#fff`;
    frame.src = "./editor.html?fixture=rich";
    document.body.append(frame);
    await new Promise((resolve) => frame.addEventListener("load", resolve, { once: true }));
    return frame;
  }, width);
  const frame = await handle.contentFrame();
  await frame.waitForFunction(
    () =>
      document.querySelectorAll(".page-wrap").length > 0 &&
      document.body.dataset.fontsReady === "true",
    null,
    { timeout: 45_000 },
  );
  return frame;
}

test("an editor embedded in a narrow column fits the page instead of clipping it", async ({
  page,
}) => {
  const frame = await mountNarrow(page, 750);
  const seen = await overflow(frame);

  expect(
    seen.scrollWidth,
    `the embedded editor scrolls sideways (${seen.scrollWidth} > ${seen.clientWidth}): the ` +
      "page is clipped at the frame's edge, which is what a host's visitor sees first",
  ).toBeLessThanOrEqual(seen.clientWidth + 1);
  expect(
    seen.pageWidth,
    `the page is ${Math.round(seen.pageWidth)}px inside a ${Math.round(seen.viewportWidth)}px ` +
      "viewport, so part of every line is off-screen",
  ).toBeLessThanOrEqual(seen.viewportWidth);
  expect(seen.zoom, "the zoom control does not say the document was fitted").toMatch(/fit width/i);
});

test("a full-size window is left at 100%, and a chosen zoom is never overridden", async ({
  page,
}) => {
  // The other half of the rule: the automatic fit must not touch a window that
  // has room, and must not fight a deliberate zoom afterwards.
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await expect(page.locator("#zoom")).toHaveValue("100%");

  // A deliberate zoom, then a relayout: the choice has to survive.
  await page.locator("#zoom").fill("150%");
  await page.locator("#zoom").press("Enter");
  await expect(page.locator("#zoom")).toHaveValue("150%");
  await page.setViewportSize({ width: 900, height: 760 });
  await expect(page.locator("#zoom")).toHaveValue("150%");
});

test("a phone is left readable rather than fitted to a postage stamp", async ({ page }) => {
  // The floor on the rule. A 390px viewport fits a Letter page at about 31%,
  // which is not a document anybody can read — worse than the clipping it would
  // be curing. Below `FIT_ON_OPEN_FLOOR` the page stays at full size and the
  // reader pans, which is also where Word for the web's own zoom stops.
  const frame = await mountNarrow(page, 390);
  const seen = await overflow(frame);
  expect(
    seen.zoom,
    `a ${Math.round(seen.viewportWidth)}px viewport was fitted to ${seen.zoom}, which is ` +
      "smaller than anything can be read at",
  ).toBe("100%");
});
