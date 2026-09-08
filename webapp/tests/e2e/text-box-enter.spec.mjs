// A text box could hold text but could never gain a second paragraph.
//
// `find_paragraph_mut` descends into a paragraph's INLINES — which is where a
// text box hangs — but `split_paragraph` and `join_paragraphs` walked only into
// tables and content controls. So the editor could RESOLVE a caret inside a box
// (typing worked) and then could not MUTATE its structure: Enter returned
// NodeNotFound and surfaced as the generic "That edit isn't supported for this
// selection yet". The two walks have to agree about where a paragraph can live,
// or an edit resolves a position it cannot act on.
//
// Both directions are asserted, because they close together: a box that can gain
// a paragraph must also be able to lose one, or Enter works inside a box and the
// Backspace that undoes it by hand does not.
import { test, expect, gotoEditor, stableBox } from "./fixtures.mjs";

const FIXTURE = "../fixtures/generated/inline-text-box.docx";
// Where the fixture's box sits, as a fraction of the page (see text-box-editing).
const BOX = { fx: 0.18, fy: 0.11 };

async function caretInsideBox(page) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect
    .poll(() => page.locator("#docTitle").inputValue(), { timeout: 30_000 })
    .toContain("inline-text-box");
  const box = await stableBox(page.locator(".page-wrap .page").first());
  await page.mouse.dblclick(box.x + box.width * BOX.fx, box.y + box.height * BOX.fy);
  // Edit mode is the state where keystrokes belong to the box rather than the body.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
}

test("Enter inside a text box splits the paragraph in the box", async ({
  page,
  consoleErrors,
}) => {
  await caretInsideBox(page);

  await page.keyboard.type("AA");
  await page.keyboard.press("Enter");

  // The undo label is the assertion, because it names the operation the engine
  // actually applied. Before the fix nothing was applied at all and the label
  // stayed on the preceding "Typing".
  await expect(page.locator("#undoBtn")).toHaveAttribute("title", /Undo Paragraph break/);
  // And it must not have been refused: the generic rejection is exactly what a
  // user saw before, so its absence is part of the contract.
  await expect(page.locator("#status")).not.toContainText(/isn't supported/i);

  await page.keyboard.type("BB");

  // Backspace at the start of the second paragraph joins it back — the inverse
  // walk had the identical gap and would otherwise still be broken here.
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await expect(page.locator("#status")).not.toContainText(/isn't supported/i);

  expect(consoleErrors).toEqual([]);
});
