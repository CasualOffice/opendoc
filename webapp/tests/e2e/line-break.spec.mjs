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
import { readFile } from "node:fs/promises";
import {
  test,
  expect,
  applyParagraphStyle,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  expectEditorFocused,
  runPaletteCommand,
  stableBox,
  MOD,
} from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";

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

/** The text of every run and break in the saved body, in document order:
 *  `"one"`, `"<br>"`, `"two"`. Read from the SAVED file, because the defect this
 *  guards was invisible on screen-reader text: "one", Shift+Enter, "two" saved
 *  `<w:t>onetwo</w:t><w:br/>` (`docs/109` TA-01). */
async function savedInlineOrder(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  const xml = opcPart(await readFile(await (await download).path()), "word/document.xml");
  return [...xml.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>|<w:br\b[^>]*\/>/g)].map((m) =>
    m[0].startsWith("<w:br") ? "<br>" : m[1],
  );
}

test("Shift+Enter keeps the text in one paragraph, and the caret after the break", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await freshParagraph(page);

  await page.keyboard.type("one");
  const caret = page.locator(".overlay .caret").first();
  const before = await stableBox(caret);
  await page.keyboard.press("Shift+Enter");
  // Its own undo label, so a line break is never mistaken for a paragraph break
  // in the history — they are different edits and undo has to say which.
  await expect(page.locator("#undoBtn")).toHaveAttribute("title", /Undo Line break/);
  // The caret is on the NEW line: below where it was, back at the left edge.
  await expect
    .poll(async () => (await stableBox(caret)).y)
    .toBeGreaterThan(before.y + before.height / 2);
  expect((await stableBox(caret)).x).toBeLessThan(before.x);
  await expectEditorFocused(page);
  await page.keyboard.type("two");

  // One block containing both halves. A paragraph break would give two.
  await expect(mirror(page).locator("h1").first()).toHaveText(/^one\s*two$/);
  // And the SAVED order is text, break, text — the guarantee itself. Before the
  // fix this read ["onetwo", "<br>"]: the break was zero positions wide, so the
  // caret after it was the same offset as the caret before it.
  const order = await savedInlineOrder(page);
  const at = order.indexOf("one");
  expect(order.slice(at, at + 3)).toEqual(["one", "<br>", "two"]);

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
  await expect(headings.first()).toHaveText(/^title\s*subtitle$/);

  expect(consoleErrors).toEqual([]);
});

test("Shift+Enter in a list item does not start a second item", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await freshParagraph(page);

  await applyParagraphStyle(page, "Normal");
  await page.keyboard.type("one");
  await page.locator("#bulletList").click();
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("two");

  // One bullet, both lines. This is the case users hit most: Enter is how you
  // make the next bullet, so Shift+Enter has to be how you DON'T.
  const items = mirror(page).locator("li");
  await expect(items).toHaveCount(1);
  await expect(items.first()).toHaveText(/^one\s*two$/);
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
  await expect(mirror(page).locator("h1").first()).toHaveText(/^keep\s*tail$/);

  expect(consoleErrors).toEqual([]);
});

test("the Line break command lands the next keystroke after the break too", async ({
  page,
  consoleErrors,
}) => {
  // The second surface (SKILL §10): the palette's `insert.lineBreak` runs the
  // same `insertLineBreakAtSelection`, so it must leave the caret in the same
  // place. Checked on the SAVED order for the same reason as above.
  await gotoEditor(page);
  await freshParagraph(page);
  await page.keyboard.type("left");
  await runPaletteCommand(page, "insert.lineBreak", "line break");
  await expectEditorFocused(page);
  await page.keyboard.type("right");
  const order = await savedInlineOrder(page);
  const at = order.indexOf("left");
  expect(order.slice(at, at + 3)).toEqual(["left", "<br>", "right"]);
  expect(consoleErrors).toEqual([]);
});
