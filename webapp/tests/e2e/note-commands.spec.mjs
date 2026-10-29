// Footnotes and endnotes: go to the next or previous note, convert one kind to
// the other, and the Footnote and Endnote options dialog (`docs/153`
// page.note.goto, page.note.convert, page.note.settings; `docs/109` RM-24).
//
// Driven through the References band's Notes dropdown, and asserted by what the
// engine then answers — where the caret went, which kind a "next" can still
// find, and what the dialog reads back on reopening — not by the dropdown's own
// state.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

async function notesMenu(page, command) {
  await page.locator("#tabReferences").click();
  await expect(page.locator("#panelReferences")).toBeVisible();
  await page.locator("#refNotesMenuBtn").click();
  const menu = page.locator("#refNotesMenu");
  await expect(menu).toBeVisible();
  await menu.locator(`[data-command="${command}"]`).click();
}

test("go to a footnote, convert it to an endnote, and set the endnote options", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // A footnote at the end of the first line, so the document start is before it.
  await page.keyboard.press("End");
  await page.locator("#tabReferences").click();
  await page.locator("#refFootnoteBtn").click();
  await expect(page.locator("#status")).toContainText("Footnote added");

  // Next footnote from the document start lands on it; a second Next says there
  // is none. Mutation: `>=` for `>` in `adjacent_note_inner` -> the second Next
  // finds the same footnote again and the status never says so.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await notesMenu(page, "reference.nextFootnote");
  await expect(page.locator("#status")).not.toContainText("no more footnotes");
  await notesMenu(page, "reference.nextFootnote");
  await expect(page.locator("#status")).toContainText("There are no more footnotes that way");

  // Convert all footnotes to endnotes: afterwards there is no footnote to go to,
  // and there is an endnote. Mutation: skip the RemoveNote half of the lowering
  // -> the convert is refused and the status never says "Notes converted".
  await notesMenu(page, "reference.convertNotes");
  const convert = page.locator("#convertNotesDialog");
  await expect(convert).toBeVisible();
  await convert.locator('input[value="footnotesToEndnotes"]').check();
  await page.locator("#convertNotesApply").click();
  await expect(convert).toBeHidden();
  await expect(page.locator("#status")).toContainText("Notes converted");
  await moveCaretToDocStart(page);
  await notesMenu(page, "reference.nextFootnote");
  await expect(page.locator("#status")).toContainText("There are no more footnotes that way");
  await moveCaretToDocStart(page);
  await notesMenu(page, "reference.nextEndnote");
  await expect(page.locator("#status")).not.toContainText("no more endnotes");

  // The options dialog writes the section's endnote format and reads it back.
  await notesMenu(page, "reference.noteSettings");
  const settings = page.locator("#noteSettingsDialog");
  await expect(settings).toBeVisible();
  await page.locator("#noteSettingsKind").selectOption("endnote");
  await expect(page.locator('#noteSettingsPosition option[value="pageBottom"]')).toBeHidden();
  await page.locator("#noteSettingsFormat").selectOption("lowerRoman");
  await page.locator("#noteSettingsApply").click();
  await expect(settings).toBeHidden();
  await expect(page.locator("#status")).toContainText("Endnote options applied");
  await notesMenu(page, "reference.noteSettings");
  await page.locator("#noteSettingsKind").selectOption("endnote");
  await expect(page.locator("#noteSettingsFormat")).toHaveValue("lowerRoman");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the six note commands are palette commands and References-menu rows", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const ids = await page.evaluate(() =>
    window.opendoc
      .describe()
      .commands.map((command) => command.id)
      .filter((id) => /^reference\.(next|previous)(Foot|End)note$|^reference\.(noteSettings|convertNotes)$/.test(id)),
  );
  expect(ids.sort()).toEqual([
    "reference.convertNotes",
    "reference.nextEndnote",
    "reference.nextFootnote",
    "reference.noteSettings",
    "reference.previousEndnote",
    "reference.previousFootnote",
  ]);
  expect(consoleErrors).toEqual([]);
});
