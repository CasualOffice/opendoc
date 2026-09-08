// In Suggesting mode the editor PAINTED one layout and INTERACTED with another.
//
// The markup view shows struck deletions; the editing layout gives them zero
// width. Caret rects, selection rects and hit-testing all came from the editing
// layout and were drawn over — and read from — markup pixels. Once anything
// upstream had been struck the two disagreed by exactly the width of the struck
// text, and the consequence was not cosmetic:
//
//   strike "Rich " from "Rich Document", then select seven characters. The
//   highlight covered what read as "Rich Do". Backspace struck "Documen" —
//   FIVE characters the user never touched — leaving "t".
//
// The two spaces are relatable: markup space is editing space plus the deleted
// spans, so `view_pos`/`edit_pos` map between them and every screen-to-model
// boundary now uses the layout that produced the pixels.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, stableBox } from "./fixtures.mjs";

const mirrorText = (page) =>
  page.locator("#a11yDocument").evaluate((el) => el.textContent.trim());

/** Enters Suggesting and strikes the leading "Rich " from the demo's heading. */
async function strikeLeadingWord(page) {
  await clickIntoFirstPage(page);
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await moveCaretToDocStart(page);
  for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");
  // The markup view is what makes the two spaces diverge; if it is not on, the
  // rest of this file proves nothing.
  await expect(page.locator("body")).toHaveClass(/showing-changes/);
  await expect.poll(() => mirrorText(page)).toContain("Document");
}

/** Page-local x of the selection highlight's left edge. */
async function highlightLeft(page) {
  return page.evaluate(() => {
    const rect = document.querySelector(".overlay .highlight").getBoundingClientRect();
    const page1 = document.querySelector("canvas.page").getBoundingClientRect();
    return rect.x - page1.x;
  });
}

test("the selection highlight covers the text that will actually be deleted", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Baseline: where the heading's first character sits with nothing struck. That
  // is also where the struck text will later begin, so it is the x the highlight
  // must NOT start at.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowRight");
  const lineStart = await highlightLeft(page);
  await page.keyboard.press("ArrowLeft");

  await strikeLeadingWord(page);

  // Select seven characters of the SURVIVING text.
  await moveCaretToDocStart(page);
  for (let i = 0; i < 7; i++) await page.keyboard.press("Shift+ArrowRight");
  const selectionStart = await highlightLeft(page);

  // The highlight must begin past the struck word, not at the line start. Before
  // the fix it began exactly at `lineStart` and covered the struck text.
  expect(
    selectionStart - lineStart,
    `the highlight starts ${Math.round(selectionStart - lineStart)}px into the line; ` +
      "at 0 it is covering the struck text and deleting something else",
  ).toBeGreaterThan(30);

  // And the edit matches what was shown: seven characters of "Document" go.
  await page.keyboard.press("Backspace");
  await expect.poll(() => mirrorText(page)).toMatch(/^t\b|^tParagraph/);

  expect(consoleErrors).toEqual([]);
});

test("clicking a word puts the caret in that word", async ({ page, consoleErrors }) => {
  await gotoEditor(page);

  // An oracle measured BEFORE any suggestion exists: the painted width of "Rich "
  // in the untouched heading. In the markup view the struck word occupies exactly
  // that width, so `lineStart + richWidth` is where the surviving "Document"
  // begins ON SCREEN — a position derived from pixels rather than from the caret
  // machinery this test is checking.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");
  const painted = await page.evaluate(() => {
    const rect = document.querySelector(".overlay .highlight").getBoundingClientRect();
    const page1 = document.querySelector("canvas.page").getBoundingClientRect();
    return { left: rect.x - page1.x, width: rect.width };
  });
  await page.keyboard.press("ArrowLeft");

  await strikeLeadingWord(page);

  // The y has to be taken AFTER entering Suggesting: the mode banner pushes the
  // page down, so a y measured beforehand points at the wrong line and the click
  // silently leaves the caret where it already was — which passes for the wrong
  // reason. An earlier draft of this test did exactly that.
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const midY = await page.locator(".overlay .caret").evaluate((el) => {
    const rect = el.getBoundingClientRect();
    return rect.y + rect.height / 2;
  });
  await page.mouse.click(box.x + painted.left + painted.width + 2, midY);
  await page.keyboard.type("#");

  const text = await mirrorText(page);
  // Correct: the click lands at the start of "Document" -> "#Document".
  // Hit-testing the editing layout resolves that same x about five characters in,
  // because "Document" is painted from the line start there -> "Docu#ment".
  expect(
    text.slice(0, 12),
    `the click resolved into the middle of the word: ${text.slice(0, 20)}`,
  ).toMatch(/^#Document/);

  expect(consoleErrors).toEqual([]);
});
