// A page index the view computed for one layout must never be asked of another
// (`docs/109` HF-400).
//
// The owner's report: open a long document, then a short one, and the open fails
// with "page index 13 out of range (0..4)". The engine's own count was stable
// (4 pages, every pass); what asked for page 13 was the HOST. `computeFitZoom`
// picked its page from the ruler's last page index, clamped to the view's
// `pages` array, and both still described the PREVIOUS document: the open path
// fits the zoom before the first render publishes the new page set. The same
// stale pair is read on every fit-mode render, so turning Show changes off
// while the caret sat on a page only the markup layout has failed the same way.
//
// Both cases are created here rather than hoped for: the long document puts
// the caret, and so the ruler, on page 16, and the precondition is asserted, so
// the guard cannot go green by never reaching the state it is about.
import {
  expect,
  gotoEditor,
  openAppMenu,
  pageSheet,
  test,
} from "./fixtures.mjs";
import { zip } from "./toc-docx.mjs";

const DOCX_MIME = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';

/** A package with only the main part: what the importer needs and nothing else. */
function docx(name, body) {
  return {
    name,
    mimeType: DOCX_MIME,
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
      "word/document.xml":
        `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${W}><w:body>${body}` +
        // The reported file's page: Letter with 0.65" top/bottom, 0.75" sides.
        '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>' +
        '<w:pgMar w:top="936" w:right="1080" w:bottom="936" w:left="1080" w:header="720" ' +
        'w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>',
    }),
  };
}

const para = (text) => `<w:p><w:r><w:t xml:space="preserve">${text}</w:t></w:r></w:p>`;
const pageBreak = '<w:p><w:r><w:br w:type="page"/></w:r></w:p>';

const LONG_PAGES = 16;

/** Sixteen pages, one forced break each, so the last page is page 16. */
function longDocx() {
  const pages = [];
  for (let n = 1; n <= LONG_PAGES; n++) pages.push(para(`Long document, page ${n}.`));
  return docx("long.docx", pages.join(pageBreak));
}

function table(rows, cols, label) {
  const cell = (r, c) =>
    `<w:tc><w:tcPr><w:tcW w:w="${Math.floor(10080 / cols)}" w:type="dxa"/></w:tcPr>${para(`${label} ${r}.${c}`)}</w:tc>`;
  let xml = '<w:tbl><w:tblPr><w:tblW w:w="10080" w:type="dxa"/></w:tblPr><w:tblGrid>';
  for (let c = 0; c < cols; c++) xml += `<w:gridCol w:w="${Math.floor(10080 / cols)}"/>`;
  xml += "</w:tblGrid>";
  for (let r = 0; r < rows; r++) {
    xml += "<w:tr>";
    for (let c = 0; c < cols; c++) xml += cell(r + 1, c + 1);
    xml += "</w:tr>";
  }
  return `${xml}</w:tbl>`;
}

/** The reported file's shape: a design-system spec of headings, prose and eight
 *  token tables, a few pages long — far fewer pages than the document before it. */
function designSystemDocx() {
  const parts = [para("Design system: final.")];
  for (let t = 1; t <= 8; t++) {
    parts.push(para(`Section ${t}: tokens and usage.`));
    parts.push(table(6, 3, `T${t}`));
  }
  return docx("design-system.docx", parts.join(""));
}

/** The 1-based total the page indicator reports. */
async function pageTotal(page) {
  const text = (await page.locator("#statPages").textContent()) ?? "";
  return Number(text.match(/of\s+~?([\d,]+)/)?.[1]?.replace(/,/g, "") ?? "0");
}

/** Waits for an open to finish, either way, and returns the status line. */
async function settledStatus(page, name) {
  await expect(page.locator("#status")).not.toHaveText(`Opening ${name}…`);
  return (await page.locator("#status").textContent()) ?? "";
}

/** Puts the caret on page `n` with a real click, and asserts it is there. */
async function caretOnPage(page, n) {
  const sheet = await pageSheet(page, n);
  await sheet.locator(".page").click({ position: { x: 120, y: 120 } });
  await expect(page.locator("#statPages"), "precondition: the caret is on the last page").toContainText(
    new RegExp(`\\b${n}\\b.*\\bof\\s+${n}\\b`),
  );
}

test("opening a short document after the caret sat on page 16 of a long one opens it (HF-400)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(longDocx());
  await expect(page.locator("#a11yDocument")).toContainText(`Long document, page ${LONG_PAGES}.`);
  await expect.poll(() => pageTotal(page)).toBe(LONG_PAGES);
  await caretOnPage(page, LONG_PAGES);

  await page.locator("#file").setInputFiles(designSystemDocx());
  const status = await settledStatus(page, "design-system.docx");
  expect(status, "the open must not fail on a page index from the previous document").not.toMatch(
    /Could not open|out of range/,
  );
  await expect(page.locator("#a11yDocument")).toContainText("T8 6.3");
  const total = await pageTotal(page);
  expect(total, "precondition: the new document is shorter than the caret's old page").toBeLessThan(LONG_PAGES);
  // The new document is the one on screen, and every one of its pages has a sheet.
  await expect(page.locator(`.page-wrap[data-page-number="${LONG_PAGES}"]`)).toHaveCount(0);
  await expect(page.locator(".page-wrap .page").first()).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

/** One paragraph kept, then a tracked deletion several pages long, then the
 *  tail: the markup layout lays the struck text out, the editing layout gives it
 *  no width, so markup has pages the editing layout does not. */
function trackedDeletionDocx() {
  const struck = "Struck words that only the markup view lays out. ".repeat(500);
  return docx(
    "tracked.docx",
    para("Kept opening paragraph.") +
      '<w:p><w:del w:id="1" w:author="Reviewer" w:date="2026-10-01T00:00:00Z">' +
      `<w:r><w:delText xml:space="preserve">${struck}</w:delText></w:r></w:del>` +
      '<w:r><w:t xml:space="preserve">Tail that survives.</w:t></w:r></w:p>',
  );
}

test("turning Show changes off with the caret on a markup-only page re-renders (HF-400)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(trackedDeletionDocx());
  await expect(page.locator("#a11yDocument")).toContainText("Tail that survives.");
  // A document carrying tracked changes opens with markup shown, so the deleted
  // pages are laid out and the tail is on the last of them.
  const markupPages = await pageTotal(page);
  expect(markupPages, "precondition: markup lays out the deleted pages").toBeGreaterThan(2);
  await caretOnPage(page, markupPages);

  // Fit width is the mode whose render re-derives the zoom from the caret's page.
  await page.locator("#zoomMenuBtn").click();
  await page.locator('#zoomMenu [data-zoom-mode="fit-width"]').click();
  await expect(page.locator("#zoom")).toHaveValue("Fit width");

  await openAppMenu(page, "view");
  await page.locator('#appMenuPopover [data-command="view.showChanges"]').click();
  // The render that follows the toggle is the one that used to throw, before it
  // published its page set — so the indicator kept the markup layout's total.
  await expect
    .poll(() => pageTotal(page), "the editing layout's pages replace the markup layout's")
    .toBeLessThan(markupPages);
  await expect(page.locator('.page-wrap[data-page-number="1"]')).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});
