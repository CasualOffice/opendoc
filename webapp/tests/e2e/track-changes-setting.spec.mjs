// The document's Track Changes setting (`w:trackRevisions`, `docs/165` M7,
// `docs/109` HF-282). Word saves a document with tracking on and opens it that
// way; here that is Suggesting mode. Guarded from the reader's side, with real
// clicks and real keys:
//
//   * a file saved with tracking on opens in Suggesting, and typing is tracked;
//   * switching to Suggesting and saving writes the setting, and the saved file
//     opens in Suggesting again;
//   * the switch is one undoable edit, and its Undo puts the mode back.
import { readFile } from "node:fs/promises";

import { MOD, clickIntoFirstPage, expect, gotoEditor, setReviewMode, test } from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";
import { zip } from "./toc-docx.mjs";

const DOCX_MIME = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';

/** A one-paragraph document whose settings say Track Changes is on — the
 *  element Word writes, in the part Word writes it to. */
function trackedDocx() {
  return {
    name: "tracked.docx",
    mimeType: DOCX_MIME,
    buffer: zip({
      "[Content_Types].xml":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/word/document.xml" ' +
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
        '<Override PartName="/word/settings.xml" ' +
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>' +
        "</Types>",
      "_rels/.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rId1" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" ' +
        'Target="word/document.xml"/></Relationships>',
      "word/document.xml":
        `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${W}><w:body>` +
        '<w:p><w:r><w:t xml:space="preserve">A contract under review.</w:t></w:r></w:p>' +
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>' +
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" ' +
        'w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>',
      "word/_rels/document.xml.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rIdSettings" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" ' +
        'Target="settings.xml"/></Relationships>',
      "word/settings.xml":
        `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings ${W}>` +
        '<w:trackRevisions/><w:defaultTabStop w:val="720"/></w:settings>',
    }),
  };
}

const modeButton = (page, mode) => page.locator(`#reviewModeControl [data-review-mode="${mode}"]`);

/** Saves with the keyboard and returns the bytes the browser downloaded. */
async function saveBytes(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  return readFile(await (await download).path());
}

test("a document saved with Track Changes on opens in Suggesting, and what is typed is tracked", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(modeButton(page, "editing")).toHaveAttribute("aria-pressed", "true");
  await page.locator("#file").setInputFiles(trackedDocx());
  await expect(page.locator("#a11yDocument")).toContainText("A contract under review.");

  await expect(modeButton(page, "suggesting"), "the file's setting is honoured").toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.type(" Signed.");
  await expect(page.locator(".overlay .review-insertion-marker"), "the typing is a tracked insertion").not.toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("switching to Suggesting is saved with the document, and the saved file opens in Suggesting", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  const untracked = await saveBytes(page);
  expect(opcPart(untracked, "word/settings.xml"), "the fixture does not track").not.toContain("<w:trackRevisions");

  await setReviewMode(page, "suggesting");
  await expect(page.locator("#undoBtn"), "the switch is an edit with a name").toHaveAttribute(
    "aria-label",
    /Track changes/,
  );
  const tracked = await saveBytes(page);
  expect(opcPart(tracked, "word/settings.xml"), "the save writes the setting").toContain("<w:trackRevisions/>");

  // Reopen what was saved in a fresh editor, which starts in Editing: it opens
  // in Suggesting. (Not by switching back to Editing first — that switch is an
  // edit of its own, and the unsaved-work guard would rightly stop the open.)
  await gotoEditor(page);
  await expect(modeButton(page, "editing")).toHaveAttribute("aria-pressed", "true");
  await page.locator("#file").setInputFiles({ name: "saved.docx", mimeType: DOCX_MIME, buffer: tracked });
  await expect(page.locator("#docTitle")).toHaveValue("saved.docx");
  await expect(modeButton(page, "suggesting")).toHaveAttribute("aria-pressed", "true");
  expect(consoleErrors).toEqual([]);
});

test("the switch is one Undo, and undoing it puts the mode back", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Track changes/);

  await page.keyboard.press(`${MOD}+z`);
  await expect(modeButton(page, "editing"), "the mode follows the setting back").toHaveAttribute(
    "aria-pressed",
    "true",
  );
  const saved = await saveBytes(page);
  expect(opcPart(saved, "word/settings.xml")).not.toContain("<w:trackRevisions");
  expect(consoleErrors).toEqual([]);
});
