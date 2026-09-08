// Shift+Enter was a paragraph break everywhere, because the engine had no
// line-break operation at all — the handler fell straight through to the same
// split Enter uses. In a bulleted list that produced a second bullet; at the end
// of a heading it dropped the next line into body text; anywhere else it silently
// ended the paragraph. All three are the opposite of what the gesture means in
// Word and Docs, where the text stays in ONE paragraph and therefore keeps its
// style, list membership, numbering and spacing.
//
// The fix needed no new operation: `w:br` is an inline node, so it is the
// existing `InsertInlineObject` carrying `InlineNode::Break`, and its inverse is
// the existing `RemoveInlineObject`. The closed op set (ADR-030, invariant I2)
// stays closed, and undo/redo and the transaction log work without knowing the
// gesture exists.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

const mirror = (page) => page.locator("#a11yDocument");

/** A fresh body paragraph at the top of the document, so the assertions are not
 *  at the mercy of where arrow keys land — three ArrowDowns from the start of the
 *  demo put the caret inside a nested TABLE CELL, which quietly measured the
 *  wrong container while an earlier draft of this test looked like it passed. */
async function freshParagraph(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Enter");
  await page.keyboard.press("ArrowUp");
}

test("Shift+Enter keeps the text in one paragraph", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await freshParagraph(page);

  await page.keyboard.type("one");
  await page.keyboard.press("Shift+Enter");
  // Its own undo label, so a line break is never mistaken for a paragraph break
  // in the history — they are different edits and undo has to say which.
  await expect(page.locator("#undoBtn")).toHaveAttribute("title", /Undo Line break/);
  await page.keyboard.type("two");

  // One block containing both halves. A paragraph break would give two.
  await expect(mirror(page).locator("h1").first()).toHaveText("onetwo");

  // And it undoes as one action, restoring exactly the text before the break.
  await page.locator("#undoBtn").click();
  await page.locator("#undoBtn").click();
  await expect(mirror(page).locator("h1").first()).toHaveText("one");

  expect(consoleErrors).toEqual([]);
});

test("Shift+Enter at the end of a heading keeps the next line in the heading", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await freshParagraph(page);

  // The fresh paragraph inherits Heading 1 from the title it was split from.
  await page.keyboard.type("title");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("subtitle");

  // Both halves in the SAME heading. Enter would apply the style's `w:next` and
  // drop "subtitle" into a body paragraph.
  const headings = mirror(page).locator("h1");
  await expect(headings.first()).toHaveText("titlesubtitle");

  expect(consoleErrors).toEqual([]);
});

test("Shift+Enter in a list item does not start a second item", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await freshParagraph(page);

  await page.locator("#paragraphStyle").selectOption("Normal");
  await page.keyboard.type("one");
  await page.locator("#bulletList").click();
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("two");

  // One bullet, both lines. This is the case users hit most: Enter is how you
  // make the next bullet, so Shift+Enter has to be how you DON'T.
  const items = mirror(page).locator("li");
  await expect(items).toHaveCount(1);
  await expect(items.first()).toHaveText("onetwo");
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  expect(consoleErrors).toEqual([]);
});

test("Shift+Enter replaces a selection first, like typing does", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await freshParagraph(page);

  await page.keyboard.type("keepDROP");
  for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("tail");

  // "DROP" is gone — a break landing beside text the user meant to overwrite is
  // the same defect as a typed character doing so.
  await expect(mirror(page).locator("h1").first()).toHaveText("keeptail");

  expect(consoleErrors).toEqual([]);
});
