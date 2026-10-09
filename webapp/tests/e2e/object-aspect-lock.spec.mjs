// Word's "Lock aspect ratio" (`docs/109` FID-AT-09): a corner drag keeps an
// object's proportions exactly when its FILE locks them (DrawingML's
// `noChangeAspect`), not because of what kind of object it is. Word honours the
// flag both ways, so both directions are here, driven by a real pointer on the
// real grips:
//
//   * a picture whose file states no lock stretches (an absent flag is unlocked,
//     which is how Word reads the same file);
//   * a shape whose file locks its ratio keeps it;
//   * a picture inserted HERE keeps its proportions, because Insert ▸ Picture
//     writes the lock Word writes on every picture it inserts.
//
// The document is built in memory, with the locks in the XML where a reader of
// the test can check them.
import { MOD, expect, gotoEditor, moveCaretToDocStart, stableBox, test } from "./fixtures.mjs";
import { zip } from "./toc-docx.mjs";
import { stripedPng } from "./wide-content-docx.mjs";

const EMU_PER_INCH = 914_400;
const NS =
  'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" ' +
  'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" ' +
  'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" ' +
  'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" ' +
  'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" ' +
  'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"';
const para = (text) => `<w:p><w:r><w:t xml:space="preserve">${text}</w:t></w:r></w:p>`;
const CX = 2 * EMU_PER_INCH;
const CY = 1 * EMU_PER_INCH;

/** An in-line 2in × 1in picture whose file states NO lock: no
 *  `wp:cNvGraphicFramePr`, and an empty `pic:cNvPicPr`. */
const unlockedPicture =
  "<w:p><w:r><w:drawing>" +
  `<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="${CX}" cy="${CY}"/>` +
  '<wp:docPr id="1" name="Stretchy"/>' +
  '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">' +
  '<pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="stretchy.png"/><pic:cNvPicPr/></pic:nvPicPr>' +
  '<pic:blipFill><a:blip r:embed="rIdImg1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>' +
  `<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="${CX}" cy="${CY}"/></a:xfrm>` +
  '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>' +
  "</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>";

/** A floating 2in × 1in rectangle whose file locks its ratio where Word writes
 *  a shape's lock: `<a:spLocks noChangeAspect="1"/>` in `wps:cNvSpPr`. */
const lockedShape =
  "<w:p><w:r><w:drawing>" +
  '<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" ' +
  'behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>' +
  '<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>' +
  '<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>' +
  `<wp:extent cx="${CX}" cy="${CY}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>` +
  '<wp:wrapTopAndBottom/><wp:docPr id="2" name="Steadfast"/>' +
  '<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">' +
  '<wps:wsp><wps:cNvSpPr><a:spLocks noChangeAspect="1"/></wps:cNvSpPr>' +
  `<wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="${CX}" cy="${CY}"/></a:xfrm>` +
  '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>' +
  '<a:solidFill><a:srgbClr val="C9D7F0"/></a:solidFill>' +
  '<a:ln w="12700"><a:solidFill><a:srgbClr val="2A4B8D"/></a:solidFill></a:ln></wps:spPr>' +
  "<wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>";

function lockDocx() {
  const body =
    para("Stretchy picture below.") +
    unlockedPicture +
    para("Steadfast shape below.") +
    lockedShape +
    para("After the shape.");
  const document =
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${NS}><w:body>${body}` +
    '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>' +
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" ' +
    'w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>';
  return {
    name: "aspect-locks.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: zip({
      "[Content_Types].xml":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Default Extension="png" ContentType="image/png"/>' +
        '<Override PartName="/word/document.xml" ' +
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
        "</Types>",
      "_rels/.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rId1" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" ' +
        'Target="word/document.xml"/></Relationships>',
      "word/document.xml": document,
      "word/_rels/document.xml.rels":
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
        '<Relationship Id="rIdImg1" ' +
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" ' +
        'Target="media/stretchy.png"/></Relationships>',
      "word/media/stretchy.png": stripedPng(200, 100),
    }),
  };
}

/** Where `word` is: Find selects it, and the highlight is the engine's own
 *  geometry for those characters. */
async function wordBox(page, word) {
  if (await page.locator("#pages").getAttribute("data-object-mode")) await page.keyboard.press("Escape");
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(word);
  await expect(page.locator("#findStatus")).toHaveText("1 match");
  await page.keyboard.press("Escape");
  return stableBox(page.locator(".overlay .highlight").first());
}

const outline = (page) => stableBox(page.locator(".overlay .object-outline").first());

/** Selects the object drawn at `at` with a click, as a reader would. */
async function selectAt(page, at, kind) {
  await page.mouse.click(at.x, at.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", kind);
}

/** A real corner drag on the south-east grip, mostly sideways, without Shift. */
async function dragCorner(page, dx, dy) {
  const grip = await stableBox(page.locator('.overlay .object-handle[data-handle="4"]').first());
  const from = { x: grip.x + grip.width / 2, y: grip.y + grip.height / 2 };
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx, from.y + dy, { steps: 8 });
  await page.mouse.up();
}

/** Corner-drags the selected object and returns its width ÷ height before and
 *  after, once the resize has committed (the outline grew). */
async function ratioAcrossCornerDrag(page) {
  const before = await outline(page);
  await dragCorner(page, 90, 6);
  await expect
    .poll(async () => Math.round((await outline(page)).width), { message: "the corner drag resized it" })
    .toBeGreaterThan(Math.round(before.width) + 30);
  const after = await outline(page);
  return { before: before.width / before.height, after: after.width / after.height };
}

async function openLockDocx(page) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(lockDocx());
  await expect(page.locator("#a11yDocument")).toContainText("After the shape.");
}

test("a picture whose file locks no aspect ratio stretches on a corner drag, as in Word", async ({
  page,
  consoleErrors,
}) => {
  await openLockDocx(page);
  const label = await wordBox(page, "Stretchy");
  await selectAt(page, { x: label.x + 24, y: label.y + label.height + 24 }, "image");

  const { before, after } = await ratioAcrossCornerDrag(page);
  expect(before, "it opened 2:1").toBeCloseTo(2, 1);
  expect(Math.abs(after - before), `the ratio followed the pointer: ${before} → ${after}`).toBeGreaterThan(0.25);
  expect(consoleErrors).toEqual([]);
});

test("a shape whose file locks its aspect ratio keeps it on a corner drag, as in Word", async ({
  page,
  consoleErrors,
}) => {
  await openLockDocx(page);
  const label = await wordBox(page, "Steadfast");
  await selectAt(page, { x: label.x + 24, y: label.y + label.height + 24 }, "shape");

  const { before, after } = await ratioAcrossCornerDrag(page);
  expect(before, "it opened 2:1").toBeCloseTo(2, 1);
  expect(Math.abs(after - before), `the ratio held without Shift: ${before} → ${after}`).toBeLessThan(0.05);
  expect(consoleErrors).toEqual([]);
});

test("a picture inserted here keeps its proportions on a corner drag, because it carries Word's lock", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  const heading = await wordBox(page, "Rich");
  await page.mouse.click(heading.x + heading.width / 2, heading.y + heading.height / 2);
  await moveCaretToDocStart(page);
  await page.getByRole("tab", { name: "Insert" }).click();
  const [chooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.locator("#insertPictureBtn").click(),
  ]);
  await chooser.setFiles({ name: "inserted.png", mimeType: "image/png", buffer: stripedPng(120, 60) });
  await expect(page.locator("#status")).toContainText("Picture inserted");

  // It went in at the start of the heading's line, ahead of "Rich".
  await selectAt(page, { x: heading.x + 12, y: heading.y + 12 }, "image");
  const { before, after } = await ratioAcrossCornerDrag(page);
  expect(before, "it went in 2:1").toBeCloseTo(2, 1);
  expect(Math.abs(after - before), `the ratio held without Shift: ${before} → ${after}`).toBeLessThan(0.05);
  expect(consoleErrors).toEqual([]);
});
