// The caret was drawn from the wrong measurements in two ways, both reported as
// "cursor length is wrong" and "the cursor is in the wrong place after Enter".
//
//   * HEIGHT came from `Line::height`, which is `ascent + descent + leading`.
//     Leading is the gap BETWEEN lines, so the caret grew with the paragraph's
//     line spacing while the text did not: at double spacing it measured twice
//     the height of the glyphs it sat in. Word and Docs draw the caret over the
//     text's own vertical extent — it is telling you where the glyphs go, not how
//     far apart the lines are.
//
// The sibling defect — an empty CENTERED paragraph parks the caret at the left
// margin while the text lands in the middle of the page — is real and is NOT
// fixed. It cannot be repaired in the hit-test: an empty line carries no runs, so
// the aligned origin never reaches the galley. See the note in
// `stops_for` (crates/casual-doc-layout/src/hittest.rs); the fix belongs in the
// shaper. No test is written for it here, because a green one would be a lie.
//
// What is asserted below is measured against what is actually painted rather than
// against constants: a caret is only right relative to the glyphs it sits among.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

const caretBox = (page) =>
  page.locator(".overlay .caret").evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, height: r.height };
  });

async function caretInBody(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Down out of the 24pt heading into ordinary body text.
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowDown");
  await expect(page.locator(".overlay .caret")).toBeVisible();
}

test("the caret height does not change with line spacing", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await caretInBody(page);
  const single = await caretBox(page);

  await page.locator("#spacingBtn").click();
  await page.locator('#spacingMenu .spacing-line[data-percent="200"]').click();
  // Wait for the edit to land rather than for the caret to move: the first body
  // line's top does not necessarily shift when the spacing below it grows.
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");

  const double = await caretBox(page);
  // Same text, same font, twice the leading. The caret measured 18.39px then
  // 36.80px before the fix.
  expect(
    double.height,
    `caret grew from ${single.height.toFixed(2)}px to ${double.height.toFixed(2)}px on the same text`,
  ).toBeCloseTo(single.height, 1);

  expect(consoleErrors).toEqual([]);
});
