// A soft-wrap boundary offset both ENDS one line and STARTS the next, and
// `caret_start_line` resolved that tie to the next line. So every gesture that
// asks for "the end of this line" answered with the start of the following one:
//
//   * End moved the caret a full line DOWN (measured y 279.3 -> 316.1);
//   * clicking past the last word of a wrapped line teleported the caret 660px
//     left and one line down;
//   * and because Home then operated from the wrong line, it appeared broken too.
//
// This is NOT the painted-vs-editing layout defect that #525 fixed — it
// reproduces with no tracked change anywhere in the document, which is what
// separated the two causes.
//
// The fix needs no affinity threaded through the API: Word and Docs answer these
// gestures with the position after the last non-space character, which is
// strictly inside the line and so has no tie to resolve.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, stableBox } from "./fixtures.mjs";

const caretBox = (page) =>
  page.locator(".overlay .caret").evaluate((el) => {
    const rect = el.getBoundingClientRect();
    return { x: rect.x, y: rect.y, height: rect.height };
  });

/** A paragraph long enough to wrap several times, with the caret mid-line. */
async function wrappedParagraph(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  // Real WORDS, and long enough that the caret below lands on a line with MORE
  // LINES AFTER IT. Two separate ways this fixture can silently stop testing
  // anything, both of which an earlier version did:
  //   * a run of one repeated character breaks mid-run with no space at the wrap
  //     point, so the boundary this file is about never arises;
  //   * a paragraph short enough that the caret's line is the LAST line has no
  //     wrap boundary to get wrong either.
  await page.keyboard.type(
    "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima " +
      "mike november oscar papa quebec ".repeat(2),
  );
  await moveCaretToDocStart(page);
  for (let i = 0; i < 50; i++) await page.keyboard.press("ArrowRight");
  await expect(page.locator(".overlay .caret")).toBeVisible();
}

test("End keeps the caret on the line it was on", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await wrappedParagraph(page);

  const before = await caretBox(page);
  await page.keyboard.press("End");
  const after = await caretBox(page);

  expect(
    Math.abs(after.y - before.y),
    `End moved the caret ${Math.round(after.y - before.y)}px vertically — onto another line`,
  ).toBeLessThan(before.height / 2);
  // And it genuinely went to the end: the caret must have moved right.
  expect(after.x).toBeGreaterThan(before.x);

  expect(consoleErrors).toEqual([]);
});

test("Home keeps the caret on the line it was on", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await wrappedParagraph(page);

  const before = await caretBox(page);
  await page.keyboard.press("Home");
  const after = await caretBox(page);

  expect(
    Math.abs(after.y - before.y),
    `Home moved the caret ${Math.round(after.y - before.y)}px vertically`,
  ).toBeLessThan(before.height / 2);
  expect(after.x).toBeLessThan(before.x);

  expect(consoleErrors).toEqual([]);
});

test("typing after End continues the line you were on", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await wrappedParagraph(page);

  const before = await caretBox(page);
  await page.keyboard.press("End");
  await page.keyboard.type("Z");
  const after = await caretBox(page);

  // The character has to land on the SAME visual line. Landing at the start of
  // the next line is the defect, and it is silent — the text is still in the
  // right place in the model, so only the geometry reveals it.
  expect(
    Math.abs(after.y - before.y),
    "the typed character landed on a different line from the one End was pressed on",
  ).toBeLessThan(before.height / 2);

  expect(consoleErrors).toEqual([]);
});

test("clicking past the last word of a wrapped line stays on that line", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await wrappedParagraph(page);

  const before = await caretBox(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  // Past the last word but short of the sheet edge. Measured against the unfixed
  // code: from 76px inside the sheet's right edge inwards the caret jumped to the
  // next line, so 40px is comfortably inside the band that reproduces it. At 90px
  // in it does not reproduce, which is why the number is not arbitrary.
  await page.mouse.click(box.x + box.width - 40, before.y + before.height / 2);
  const after = await caretBox(page);

  expect(
    Math.abs(after.y - before.y),
    `the click landed ${Math.round(after.y - before.y)}px away vertically — on the next line`,
  ).toBeLessThan(before.height / 2);

  expect(consoleErrors).toEqual([]);
});
