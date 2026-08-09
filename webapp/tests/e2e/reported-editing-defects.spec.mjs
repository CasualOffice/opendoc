// Reproductions for defects reported against the merged editor, before any fix:
//
//   1. "things are editing on page one, not on others"
//   2. "when i do bold, cant remove that property — same with italic, underline"
//   3. "while editing header, suddenly redirect to a random page"
//
// Each drives the editor the way the report describes and asserts what the user
// expected to happen, so a red here IS the reported bug.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

/** Opens the editor on a genuinely MULTI-PAGE document.
 *
 *  The bundled `?fixture=rich` sample is a single page, and the 8-page corpus
 *  document is local-only (not redistributable), so the pages are made here: a
 *  long paste through the editor's own paste path. A defect about "later pages"
 *  cannot be reproduced on a document that has none. */
async function openMultiPage(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  const onePage = await page.locator("#viewport").evaluate((v) => v.scrollHeight);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.evaluate(() => {
    const line = "The quick brown fox jumps over the lazy dog and keeps on running. ";
    const data = new DataTransfer();
    data.setData("text/plain", `${line.repeat(6)}\n`.repeat(60));
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  });
  await expect
    .poll(async () => page.locator("#viewport").evaluate((v) => v.scrollHeight))
    .toBeGreaterThan(onePage * 2);
  // Wait for pagination to SETTLE, not merely to have started: page wraps are
  // added as the layout converges, and clicking while the count is still moving
  // aims at a page that is about to be somewhere else.
  let last = -1;
  await expect
    .poll(async () => {
      const now = await page.locator(".page-wrap").count();
      const stable = now === last;
      last = now;
      return stable && now > 1;
    }, { intervals: [250, 250, 250, 250, 250, 250, 250, 250] })
    .toBe(true);
  return last;
}

/** Scrolls the viewport to a later page and returns that page's box. */
async function scrollToPage(page, index) {
  // Every page keeps a lightweight `.page-wrap`; only nearby ones mount a canvas
  // (virtualization), so scrolling one into view is what makes it paintable.
  const target = page.locator(".page-wrap").nth(index);
  // `scrollIntoViewIfNeeded` can align the page's BOTTOM, leaving its top above
  // the viewport — a click at "the top of the page" then lands off-screen and
  // reports a product failure that is really a harness one.
  await target.evaluate((el) => el.scrollIntoView({ block: "start" }));
  await page.waitForTimeout(400);
  const canvas = target.locator(".page").first();
  let box = null;
  await expect
    .poll(async () => {
      box = await canvas.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  return box;
}

/** Clicks into a page's text column and reports whether a caret landed. */
async function clickIntoPage(page, box) {
  for (let fy = 0.18; fy < 0.8; fy += 0.06) {
    await page.mouse.click(box.x + box.width * 0.45, box.y + box.height * fy);
    if ((await page.locator(".overlay .caret").count()) === 1) return true;
  }
  return false;
}

test("[1] typing works on a later page, not only page one", async ({ page, consoleErrors }) => {
  const pageCount = await openMultiPage(page);
  expect(pageCount, "the corpus document must be multi-page").toBeGreaterThan(1);

  const box = await scrollToPage(page, Math.min(2, pageCount - 1));
  expect(await clickIntoPage(page, box), "a click on a later page must place a caret").toBe(true);

  await page.keyboard.type("LATERPAGE");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  await expect(page.locator("#a11yDocument")).toContainText("LATERPAGE");

  expect(consoleErrors).toEqual([]);
});

test("[1b] the caret stays on the page that was clicked", async ({ page, consoleErrors }) => {
  // A click that silently retargets page one would still "work" above, so pin
  // the scroll position: typing must not yank the view back to the top.
  const pageCount = await openMultiPage(page);
  expect(pageCount).toBeGreaterThan(1);

  const box = await scrollToPage(page, Math.min(2, pageCount - 1));
  expect(await clickIntoPage(page, box)).toBe(true);
  const before = await page.locator("#viewport").evaluate((v) => v.scrollTop);
  expect(before, "the viewport is scrolled away from page one").toBeGreaterThan(100);

  await page.keyboard.type("X");
  await page.waitForTimeout(300);
  const after = await page.locator("#viewport").evaluate((v) => v.scrollTop);
  expect(Math.abs(after - before), "typing must not jump the view").toBeLessThan(200);

  expect(consoleErrors).toEqual([]);
});

for (const [name, key, label] of [
  ["bold", "b", "Bold"],
  ["italic", "i", "Italic"],
  ["underline", "u", "Underline"],
]) {
  test(`[2] ${name} toggles OFF again on the same selection`, async ({ page, consoleErrors }) => {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
    await moveCaretToDocStart(page);
    // Down into ordinary body prose: the first paragraph is a heading, which is
    // already bold, so asserting an absolute state there tests the fixture, not
    // the toggle.
    for (let i = 0; i < 4; i += 1) await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    await page.keyboard.type("TOGGLE");
    for (let i = 0; i < 6; i += 1) await page.keyboard.press("Shift+ArrowLeft");

    const button = page.locator(`#${name}`);
    const initial = await button.getAttribute("aria-pressed");
    const flipped = initial === "true" ? "false" : "true";

    await page.keyboard.press(`${MOD}+${key}`);
    await expect(button, `${label} must flip on the first press`).toHaveAttribute(
      "aria-pressed",
      flipped,
    );

    // The report: the property cannot be taken off again.
    await page.keyboard.press(`${MOD}+${key}`);
    await expect(button, `${label} must flip BACK on the second press`).toHaveAttribute(
      "aria-pressed",
      initial,
    );

    expect(consoleErrors).toEqual([]);
  });
}

test("[2b] bold applied to already-bold imported text can be removed", async ({
  page,
  consoleErrors,
}) => {
  // Turning a toggle off on text that arrived bold from the FILE is the case a
  // caret-only "armed format" path gets wrong.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // The demo's first paragraph is a heading, which is bold in the document.
  await page.keyboard.press("Home");
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("Shift+ArrowRight");
  const bold = page.locator("#bold");
  const wasOn = (await bold.getAttribute("aria-pressed")) === "true";
  test.skip(!wasOn, "the first run is not bold in this document");

  await page.keyboard.press(`${MOD}+b`);
  await expect(bold, "bold must clear on text that arrived bold").toHaveAttribute(
    "aria-pressed",
    "false",
  );

  expect(consoleErrors).toEqual([]);
});

test("[3] entering the header does not jump the view to another page", async ({
  page,
  consoleErrors,
}) => {
  const pageCount = await openMultiPage(page);
  expect(pageCount).toBeGreaterThan(1);

  // Work on a later page, as a user editing a long document would be.
  const box = await scrollToPage(page, Math.min(2, pageCount - 1));
  const before = await page.locator("#viewport").evaluate((v) => v.scrollTop);

  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");
  await page.waitForTimeout(400);

  const after = await page.locator("#viewport").evaluate((v) => v.scrollTop);
  expect(
    Math.abs(after - before),
    "entering this page's header must not scroll to a different page",
  ).toBeLessThan(300);

  expect(consoleErrors).toEqual([]);
});

test("[3b] typing in a header keeps the view on that header", async ({ page, consoleErrors }) => {
  const pageCount = await openMultiPage(page);
  expect(pageCount).toBeGreaterThan(1);

  const box = await scrollToPage(page, Math.min(2, pageCount - 1));
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  const before = await page.locator("#viewport").evaluate((v) => v.scrollTop);

  await page.keyboard.type("HDR");
  await page.waitForTimeout(400);
  const after = await page.locator("#viewport").evaluate((v) => v.scrollTop);
  expect(Math.abs(after - before), "typing in a header must not scroll away").toBeLessThan(300);

  expect(consoleErrors).toEqual([]);
});

// "cant out cursor anywhere to right.. if clicked it will thorugh me out of
// editing" — the band is the header's territory, all of it. Only the body is
// the way out.

/** Enters the header of the first page and returns that page's box. */
async function enterHeader(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  let box = null;
  await expect
    .poll(async () => {
      box = await canvas.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);
  await page.mouse.dblclick(box.x + box.width * 0.5, box.y + 12);
  await expect.poll(() => page.locator("#pages").getAttribute("data-running-edit")).toBe("header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  return box;
}

test("[4] clicking the empty right-hand end of the header keeps the context", async ({
  page,
  consoleErrors,
}) => {
  const box = await enterHeader(page);
  await page.mouse.click(box.x + box.width * 0.85, box.y + 14);
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "header");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  // And it is a working caret, not a leftover: typing goes into the header.
  await page.keyboard.type("RIGHT");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  expect(consoleErrors).toEqual([]);
});

test("[5] clicking the empty space below the header line keeps the context", async ({
  page,
  consoleErrors,
}) => {
  const box = await enterHeader(page);
  // Inside the band the editor itself draws, near its lower edge — derived from
  // the product's own geometry rather than a guessed pixel offset.
  const band = await page.locator(".running-band").first().boundingBox();
  await page.mouse.click(box.x + box.width * 0.3, band.y + band.height - 4);
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "header");
  await page.keyboard.type("BELOW");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  expect(consoleErrors).toEqual([]);
});

test("[6] clicking in the body is the way OUT of the header", async ({ page, consoleErrors }) => {
  const box = await enterHeader(page);
  await page.mouse.click(box.x + box.width * 0.45, box.y + box.height * 0.4);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-running-edit", "header");
  expect(consoleErrors).toEqual([]);
});

test("[7] clicking the footer band while in the header moves to the footer", async ({
  page,
  consoleErrors,
}) => {
  await enterHeader(page);
  // The page is taller than the viewport, so its FOOTER is off-screen until the
  // page bottom is scrolled into view — clicking at the page's bottom edge
  // without this lands outside the window entirely.
  await page.locator(".page-wrap").first().evaluate((el) => el.scrollIntoView({ block: "end" }));
  await page.waitForTimeout(300);
  const box = await page.locator(".page-wrap .page").first().boundingBox();
  await page.mouse.click(box.x + box.width * 0.5, box.y + box.height - 14);
  // Word switches stories rather than dropping you into the body.
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", "footer");
  expect(consoleErrors).toEqual([]);
});
