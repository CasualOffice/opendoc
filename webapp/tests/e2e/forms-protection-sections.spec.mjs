// Forms protection is per SECTION (`docs/165` M6, `docs/109` FID-AT-06).
// ECMA-376 §17.18.29: `w:edit="forms"` protects "form fields in sections where
// the `formProt` element has a value of true [and puts] no restrictions in
// sections where `formProt` is false". LibreOffice writes `w:formProt
// w:val="false"` on every section it saves, so a protected form with an
// unprotected cover note is an ordinary file. Asserted from the reader's side:
// typing lands in the open section and is refused, with a reason, in the other.
import { MOD, expect, gotoEditor, stableBox, test } from "./fixtures.mjs";
import { zip } from "./toc-docx.mjs";

const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';
const PAGE =
  '<w:pgSz w:w="12240" w:h="15840"/>' +
  '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>';

/** Two sections under forms protection: the first states no `w:formProt`
 *  (protected), the second states `false` (open). */
function mixedFormsDocx() {
  const body =
    '<w:p><w:r><w:t xml:space="preserve">Locked terms paragraph.</w:t></w:r></w:p>' +
    `<w:p><w:pPr><w:sectPr>${PAGE}<w:type w:val="continuous"/></w:sectPr></w:pPr>` +
    '<w:r><w:t xml:space="preserve">End of the first section.</w:t></w:r></w:p>' +
    '<w:p><w:r><w:t xml:space="preserve">Notes paragraph.</w:t></w:r></w:p>' +
    `<w:sectPr><w:type w:val="continuous"/>${PAGE}<w:formProt w:val="false"/></w:sectPr>`;
  return {
    name: "mixed-forms.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
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
      "word/document.xml": `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${W}><w:body>${body}</w:body></w:document>`,
      "word/_rels/document.xml.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rIdSettings" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" ' +
        'Target="settings.xml"/></Relationships>',
      "word/settings.xml":
        `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings ${W}>` +
        '<w:documentProtection w:edit="forms" w:enforcement="1"/><w:defaultTabStop w:val="720"/></w:settings>',
    }),
  };
}

/** Puts the caret at the start of `word`, with a click where Find draws it. */
async function caretAtWord(page, word) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(word);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  const box = await stableBox(page.locator(".overlay .highlight").first());
  await page.mouse.click(box.x + 1, box.y + box.height / 2);
}

test("under forms protection, a section whose file says formProt false is editable and the other is not", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(mixedFormsDocx());
  await expect(page.locator("#a11yDocument")).toContainText("Notes paragraph.");

  // The open section: typing lands.
  await caretAtWord(page, "Notes");
  await page.keyboard.type("Q");
  await expect(page.locator("#a11yDocument"), "typing landed in the open section").toContainText("QNotes paragraph.");

  // The protected section: typing is refused, and the reader is told why.
  await caretAtWord(page, "Locked");
  await page.keyboard.type("Z");
  await expect(page.locator("#status")).toContainText(/form fields/i);
  await expect(page.locator("#a11yDocument")).not.toContainText("ZLocked");
  expect(consoleErrors).toEqual([]);
});
