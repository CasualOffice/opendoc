// Typing goes into the box you clicked — on a page the first viewport never
// shows.
//
// The owner reported, on a real loan agreement: "where it's typing is wrong …
// at last page of list in boxes". Clicking a form's empty value box and typing
// put the characters into the LABEL cell beside it. A cell's border box is far
// larger than its text — the row-height and vertical-alignment slack, the blank
// area around a picture, the whole of an empty cell — and hit-testing reasoned
// only about lines, so a click in any of those text-free regions fell through
// to whichever line painted something nearest. Nothing was refused and nothing
// was reported: the text simply appeared somewhere else.
//
// Two reasons this survived every existing spec:
//
//   * every browser spec here operates inside the FIRST viewport and near the
//     document start, and this needs a later page; and
//   * the canvas holds no text, so the check has to read the `#a11yDocument`
//     mirror, where each cell is its own `<td>` — which is exactly what makes
//     "it went into the neighbouring box" observable.
//
// `fixtures/generated/cell-hit-routing.docx` is built for this: page 2 carries
// a `LOGO | TEXT` row and a `LABEL | (empty)` row, both taller than their
// content. The engine-level guards are
// `crates/casual-doc-render/tests/table_cell_hit_routing.rs`.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const FIXTURE = "../fixtures/generated/cell-hit-routing.docx";

/** The fixture's page is a 7200-twip square, so a page-local twip coordinate is
 *  this fraction of the rendered page box however the editor is zoomed. */
const PAGE_TWIPS = 7200;

/** Page-2 geometry, as `paginate_document` resolves it (see the engine test).
 *  Row 1 `LOGO | TEXT` spans y [840, 3240); row 2 `LABEL | (empty)` spans
 *  y [3240, 4440); the columns resolve to x [600, 1980) and [1980, 6600). */
const LEFT_COLUMN = 1_290;
const RIGHT_COLUMN = 4_290;
/** Below the logo line (which ends at 1080) and well inside row 1. */
const LOGO_ROW_BLANK = 2_340;
/** In the blank top of row 2 — its content is pinned to the bottom of the row,
 *  so no line covers this y at all. */
const FORM_ROW_BLANK = 3_440;

async function openFixture(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect(page.locator("#a11yDocument")).toContainText("PAGE TWO TABLE", {
    timeout: 60_000,
  });
  // The table is on page 2 by construction; if it ever reaches page 1 the
  // guard has stopped testing what it says it tests.
  await expect(page.locator(".page-wrap")).toHaveCount(2);
}

/** Scrolls page 2 into view and clicks a page-local twip coordinate on it.
 *
 *  The editor scrolls inside `#viewport`, not the window, and `.page-wrap`
 *  elements are recycled as pages come in and out of view — so the box is read
 *  AFTER the scroll settles, and a coordinate outside the viewport would be a
 *  click on nothing at all. */
async function clickPageTwo(page, xTwips, yTwips) {
  await page.evaluate(() => {
    document.querySelectorAll(".page-wrap")[1]?.scrollIntoView({ block: "start" });
  });
  const box = await stableBox(page.locator(".page-wrap").nth(1));
  const x = box.x + (box.width * xTwips) / PAGE_TWIPS;
  const y = box.y + (box.height * yTwips) / PAGE_TWIPS;
  const viewport = page.viewportSize();
  expect(
    y >= 0 && y <= viewport.height,
    `the click must be inside the viewport (y=${Math.round(y)})`,
  ).toBe(true);
  await page.mouse.click(x, y);
}

/** The accessibility mirror's table cells, in order. */
function cells(page) {
  return page.evaluate(() =>
    [...document.querySelectorAll("#a11yDocument table td")].map((td) => td.textContent),
  );
}

test("typing after clicking an empty value cell lands in that cell", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await clickPageTwo(page, RIGHT_COLUMN, FORM_ROW_BLANK);
  await page.keyboard.insertText("QZX");

  await expect
    .poll(async () => (await cells(page))[3])
    .toBe("QZX");
  const after = await cells(page);
  expect(after[2], "the label cell is untouched").toBe("Interest rate");
  expect(consoleErrors).toEqual([]);
});

test("typing after clicking a label cell lands in the label", async ({ page }) => {
  await openFixture(page);
  await clickPageTwo(page, LEFT_COLUMN, FORM_ROW_BLANK);
  await page.keyboard.insertText("QZX");

  await expect
    .poll(async () => (await cells(page))[2])
    .toContain("QZX");
  expect((await cells(page))[3], "the value cell stays empty").toBe("");
});

test("typing after clicking a picture-only cell lands in that cell", async ({ page }) => {
  await openFixture(page);
  const before = await cells(page);
  await clickPageTwo(page, LEFT_COLUMN, LOGO_ROW_BLANK);
  await page.keyboard.insertText("QZX");

  await expect
    .poll(async () => (await cells(page))[0])
    .toBe("QZX");
  const after = await cells(page);
  expect(after[1], "the wordy cell beside it is untouched").toBe(before[1]);
});
