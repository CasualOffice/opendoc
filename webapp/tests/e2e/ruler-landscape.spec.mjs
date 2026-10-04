// The ruler describes the page the caret is on, including when that page is a
// different shape from page 1.
//
// The owner's report: "rules doesnt work for landscape pages". They did not.
// `buildRuler()` asked the engine for `pageGeometry()`, which answered with the
// document's OPENING section for every page in the document, and took its pixel
// width from `pageBandModel.widths[0]` — page 1's. So on `sections.docx`, whose
// second section turns landscape, pages 5 and 6 were drawn with page 1's portrait
// width and page 1's margins. The scale is `pageWidthPx / widthTwip`, so a wrong
// width moves every inch tick and every number on the strip, and the shaded
// margin zones stop lining up with the paper underneath them.
//
// HOW THIS IS MEASURED, and why there is no test hook in the product: everything
// below comes from the DOM and from the fixture's own committed geometry. The
// discriminator is a FRACTION, not a pixel count, which is what makes it work:
//
//   section 1   12,240 x 15,840 twips, side margins 1,440  ->  1440/12240 = 0.1176
//   section 2   15,840 x 12,240 twips, side margins 1,440  ->  1440/15840 = 0.0909
//
// The two sections have the SAME side margins, so a ruler that keeps page 1's
// geometry gets page 1's margin fraction (0.1176) on a landscape page where the
// right answer is 0.0909. Both halves of the defect — the width and the margins —
// are therefore visible without asserting a single hard-coded pixel, and without
// the product exposing anything for the test's benefit.
//
// These assert the GUARANTEE rather than the mechanism: the ruler is as wide as
// the paper it sits above, and its unshaded span covers exactly that page's text
// column. A later change may compute either from somewhere else and should still
// be held to both.
import { documentPageCount, expect, pageSheet, stableBox, test } from "./fixtures.mjs";

/** The fixture's committed geometry, in twips. Asserted against the engine by
 *  `generate_sections_fixture_docx`, so these are not a second source of truth —
 *  if the fixture is regenerated to a different shape, that test fails first. */
const SECTION_OF_PAGE = {
  0: { width: 12_240, sideMargin: 1_440, label: "portrait" },
  4: { width: 15_840, sideMargin: 1_440, label: "landscape" },
};
const TWIPS_PER_INCH = 1_440;

async function openSections(page) {
  await page.goto("/editor.html?fixture=sections");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        document.body.dataset.fontsReady === "true"
      );
    },
    null,
    { timeout: 45_000 },
  );
  // The DOCUMENT's page count, not the number of sheets on screen: only the pages
  // near the viewport are materialized (`docs/113` §8.6), so waiting for six
  // `.page-wrap` elements waits for something that never happens.
  expect(await documentPageCount(page)).toBe(6);
  await expect(page.locator(".ruler")).toBeVisible();
}

/** Scrolls page `index` into existence and puts the caret in it. A page outside
 *  the viewport has no sheet at all, so it has to be scrolled to first — and it is
 *  addressed by the page it IS, never by its position among the sheets that
 *  happen to exist. */
async function caretOnPage(page, index) {
  await pageSheet(page, index + 1);
  const wrap = page.locator(`#pages .page-wrap[data-page-number="${index + 1}"]`);
  await wrap.evaluate((el) => el.scrollIntoView({ block: "center" }));
  await expect.poll(() => wrap.locator("canvas.page").count()).toBe(1);
  const box = await stableBox(wrap);
  await page.mouse.click(box.x + box.width / 2, box.y + Math.min(140, box.height / 2));
}

/** The ruler's width, and where its unshaded content span sits inside it, as
 *  fractions of the ruler — the form the fixture's geometry predicts. */
async function rulerShape(page) {
  return await page.evaluate(() => {
    const ruler = document.querySelector(".ruler");
    const content = ruler.querySelector(".ruler-content");
    const r = ruler.getBoundingClientRect();
    const c = content.getBoundingClientRect();
    return {
      width: r.width,
      startPx: c.left - r.left,
      endPx: r.right - c.right,
    };
  });
}

/** The rendered width of one page's paper, scrolling it into existence first. */
async function paperWidth(page, index) {
  await pageSheet(page, index + 1);
  const box = await stableBox(page.locator(`#pages .page-wrap[data-page-number="${index + 1}"]`));
  return box.width;
}

test.describe("the ruler follows the caret's page geometry", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("the fixture really does change shape", async ({ page }) => {
    // A precondition, stated as its own test: if the landscape section ever stops
    // being wider than the portrait one, every assertion below would pass on a
    // document that never exercises the defect, and this is the test that should
    // say so rather than them silently going quiet.
    await openSections(page);
    const portrait = await paperWidth(page, 0);
    const landscape = await paperWidth(page, 4);
    expect(
      landscape,
      "pages 5-6 of sections.docx must render wider than page 1",
    ).toBeGreaterThan(portrait + 8);
  });

  test("the ruler is as wide as the paper under it, on both shapes", async ({
    page,
    consoleErrors,
  }) => {
    await openSections(page);
    for (const index of [0, 4]) {
      const { label } = SECTION_OF_PAGE[index];
      await caretOnPage(page, index);
      const paper = await paperWidth(page, index);
      await expect
        .poll(async () => Math.abs((await rulerShape(page)).width - paper), {
          message:
            `on the ${label} page ${index + 1} the ruler is not the width of the paper ` +
            `(${paper}px). Before the fix it kept page 1's width, so every tick was wrong.`,
        })
        .toBeLessThanOrEqual(1.5);
    }
    expect(consoleErrors).toEqual([]);
  });

  test("the shaded margins cover that page's own margins, not page 1's", async ({ page }) => {
    // The width alone is not the whole defect: the margins came from the opening
    // section too, so a change that resized the strip and kept the old margins
    // would still shade the wrong part of it. Because both sections carry 1,440
    // twip side margins, the geometry is the only thing that can tell them apart —
    // the same margin is a DIFFERENT fraction of a different page.
    //
    // Asserted in pixels with a 2px tolerance rather than as a fraction to three
    // places: `.ruler-content` carries a 1px edge, which is 0.0012 of the strip and
    // was enough to fail a fraction comparison while being visually exact. The
    // tolerance is nowhere near loose enough to hide the defect — on the landscape
    // page the wrong answer is ~28px away, an order of magnitude outside it.
    await openSections(page);
    for (const index of [0, 4]) {
      const { width, sideMargin, label } = SECTION_OF_PAGE[index];
      await caretOnPage(page, index);
      const shape = await rulerShape(page);
      const predicted = (sideMargin / width) * shape.width;
      const wrongAnswer =
        (SECTION_OF_PAGE[0].sideMargin / SECTION_OF_PAGE[0].width) * shape.width;
      expect(
        Math.abs(shape.startPx - predicted),
        `on the ${label} page the shaded left margin should be ${predicted.toFixed(1)}px of a ` +
          `${shape.width.toFixed(1)}px strip (${sideMargin}/${width}); page 1's geometry would ` +
          `put it at ${wrongAnswer.toFixed(1)}px, and reading that here is the defect`,
      ).toBeLessThanOrEqual(2);
      expect(
        Math.abs(shape.endPx - predicted),
        `on the ${label} page the shaded right margin should also be ${predicted.toFixed(1)}px`,
      ).toBeLessThanOrEqual(2);
    }
  });

  test("an inch on the ruler is an inch on that page", async ({ page }) => {
    // The numbered ticks are measured from the content edge, so "1" sits one inch
    // right of where the text starts. One inch is `1440/width` of the paper, which
    // differs between the two sections — on the landscape page this was one inch
    // at the portrait scale, against the portrait content edge.
    await openSections(page);
    for (const index of [0, 4]) {
      const { width, sideMargin, label } = SECTION_OF_PAGE[index];
      await caretOnPage(page, index);
      const one = page.locator(".ruler .ruler-num").filter({ hasText: /^1$/ }).first();
      await expect(one).toBeVisible();
      const [shape, tick] = await Promise.all([rulerShape(page), stableBox(one)]);
      const rulerLeft = await page.evaluate(
        () => document.querySelector(".ruler").getBoundingClientRect().left,
      );
      const predicted = ((sideMargin + TWIPS_PER_INCH) / width) * shape.width;
      expect(
        Math.abs(tick.x + tick.width / 2 - (rulerLeft + predicted)),
        `on the ${label} page the "1" tick should sit ${predicted.toFixed(1)}px into a ` +
          `${shape.width.toFixed(1)}px ruler — one inch past a ${sideMargin}-twip margin on ` +
          `${width}-twip paper`,
      ).toBeLessThanOrEqual(3);
    }
  });
});
