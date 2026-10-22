// A tab is ONE position in every space the editor measures a paragraph in.
//
// Typing and the caret always counted a `w:tab` as one position, but the
// length rule the Suggesting-mode splices and comment markers use charged it
// zero. So in any paragraph with a tab — a form label, a tabbed list, a table
// of contents row — a suggestion or a comment after the tab landed one position
// early per tab: suggesting "X" between the tab and "b" of `a<TAB>b` saved
// `a<TAB>bX` (`docs/109` HF-297, the same family as the line break, HF-296).
//
// Read from the SAVED file, because the screen-reader text of `a<TAB>Xb` and
// `a<TAB>bX` differ only by where an inserted run sits, and the saved order is
// the guarantee a reader keeps.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, expectEditorFocused, setReviewMode, MOD } from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";
import { zip } from "./toc-docx.mjs";

const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';

/** A one-paragraph DOCX holding `a<TAB>b`. */
function tabbedDocx() {
  const body =
    '<w:p><w:r><w:t>a</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>b</w:t></w:r></w:p>' +
    '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>';
  return {
    name: "tabbed.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: zip({
      "[Content_Types].xml":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/word/document.xml" ' +
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
        "</Types>",
      "_rels/.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rId1" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" ' +
        'Target="word/document.xml"/></Relationships>',
      "word/document.xml": `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${W}><w:body>${body}</w:body></w:document>`,
    }),
  };
}

/** The runs, tabs and insertion wrappers of the saved body, in order. */
async function savedOrder(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  const xml = opcPart(await readFile(await (await download).path()), "word/document.xml");
  return [...xml.matchAll(/<w:t(?:\s[^>]*)?>([^<]*)<\/w:t>|<w:tab\/>|<w:ins\b|<\/w:ins>/g)].map((m) =>
    m[0] === "<w:tab/>" ? "TAB" : m[0].startsWith("<w:ins") ? "INS[" : m[0] === "</w:ins>" ? "]" : m[1],
  );
}

test("a suggestion typed after a tab lands where the caret is", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(tabbedDocx());
  await expect(page.locator("#a11yDocument p").first()).toHaveText(/^a\s+b$/);
  await page.locator(".page-wrap .page").first().click({ position: { x: 60, y: 60 } });
  await page.keyboard.press(`${MOD}+Home`);
  // After "a", then after the tab: two positions, as Word counts them.
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await setReviewMode(page, "suggesting");
  await page.locator("#pages").focus();
  await expectEditorFocused(page);
  await page.keyboard.type("X");
  const order = await savedOrder(page);
  const at = order.indexOf("a");
  expect(order.slice(at, at + 6)).toEqual(["a", "TAB", "INS[", "X", "]", "b"]);
  expect(consoleErrors).toEqual([]);
});
