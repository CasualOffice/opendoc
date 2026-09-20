// Typing lands where the caret is drawn, on a tab-indented line.
//
// The owner reported, on the last page of a real loan agreement: clicking
// between the `h` and the `e` of `the` in
// `\t\t\tThe Voice of the Tax Agent community. Since 1992` drew the caret
// exactly there and inserted the character three bytes earlier, just after the
// `f` of `of`. Reproduced in this browser against that document before the fix
// (`The Voice ofQZX the Tax Agent`) and after it (`of thQZXe Tax Agent`); the
// owner's file stays on the owner's disk, so the committed guard uses
// `fixtures/generated/tab-hit-offsets.docx`, which carries the same shape.
//
// A `w:tab` is one byte (`\t`) of a paragraph's model text, and the layout's
// tab layer threaded its caret byte cursor across tabs as if they were
// zero-width. Because the caret is painted at the very stop the hit resolved
// to, both agreed on the wrong answer — so the caret looked right and only the
// insertion was wrong. That is why this needs a spec that TYPES and reads the
// text back: nothing about the caret's own position can detect it.
//
// The engine-level guards are `crates/casual-doc-render/tests/tab_hit_offsets.rs`.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const FIXTURE = "../fixtures/generated/tab-hit-offsets.docx";

/** The fixture's page is a 7200-twip square, so a page-local twip coordinate is
 *  this fraction of the rendered page box however the editor is zoomed. */
const PAGE_TWIPS = 7200;

/** Where `caret_rect` paints the caret for the `e` of `the` on the fixture's
 *  first, three-tab-indented paragraph: x=3965, y=600, height 238. The click
 *  goes 20 twips into that glyph and 60 twips down the line, so it is
 *  unambiguously ON the `e` and not on a neighbour. */
const CLICK_X = 3_985;
const CLICK_Y = 660;

/** Appears nowhere in the fixture. A marker that also occurs in the document
 *  makes a "did it land?" search succeed no matter what happened — an earlier
 *  probe of this defect used `@` against a document containing an email
 *  address and reported success on every run. */
const MARKER = "QZX";

test("typing after clicking a glyph on a tab-indented line inserts at that glyph", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect(page.locator("#a11yDocument")).toContainText(
    "The Voice of the Tax Agent community. Since 1992",
    { timeout: 60_000 },
  );

  const box = await stableBox(page.locator(".page-wrap").first());
  const x = box.x + (box.width * CLICK_X) / PAGE_TWIPS;
  const y = box.y + (box.height * CLICK_Y) / PAGE_TWIPS;
  const viewport = page.viewportSize();
  expect(
    y >= 0 && y <= viewport.height && x >= 0 && x <= viewport.width,
    `the click must be inside the viewport (x=${Math.round(x)}, y=${Math.round(y)})`,
  ).toBe(true);

  await page.mouse.click(x, y);
  await page.keyboard.insertText(MARKER);

  // Between the `h` and the `e` — not `of|QZX the`, which is where the three
  // uncounted tab bytes used to put it.
  await expect
    .poll(() => page.evaluate(() => document.querySelector("#a11yDocument").textContent))
    .toContain(`The Voice of th${MARKER}e Tax Agent community. Since 1992`);
  expect(consoleErrors).toEqual([]);
});
