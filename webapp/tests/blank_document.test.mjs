// The starter templates and their previews, tested against the package they
// actually write.
//
// The File page's New pane shows a miniature of each template's first page. The
// miniature is not a drawing: it is built from the same `[styleId, text]` body
// `templateBytes` turns into `word/document.xml`, resolved against the same
// style table `word/styles.xml` is generated from. That is the whole point of
// `BLANK_STYLE_METRICS` — a preview cannot claim a 28pt title while the
// document opens with a 16pt one. These tests hold that single source down.
import assert from "node:assert/strict";
import test from "node:test";

const {
  BLANK_DOCX_PARTS,
  BLANK_PAGE,
  BLANK_STYLE_METRICS,
  DOCUMENT_TEMPLATES,
  templateBytes,
  templateThumbnail,
} = await import("../src/blank_document.mjs");

const stylesXml = BLANK_DOCX_PARTS.find(([name]) => name === "word/styles.xml")[1];
const documentXml = BLANK_DOCX_PARTS.find(([name]) => name === "word/document.xml")[1];

test("styles.xml carries the size every metric in the table names", () => {
  for (const [id, metrics] of Object.entries(BLANK_STYLE_METRICS)) {
    const halfPoints = metrics.sizePt * 2;
    assert.ok(
      stylesXml.includes(`<w:sz w:val="${halfPoints}"/><w:szCs w:val="${halfPoints}"/>`),
      `${id} is ${metrics.sizePt}pt in the table but styles.xml has no ${halfPoints} half-point run size`,
    );
  }
});

test("styles.xml carries the spacing every metric in the table names", () => {
  for (const [id, metrics] of Object.entries(BLANK_STYLE_METRICS)) {
    if (metrics.beforePt === 0) continue;
    assert.ok(
      stylesXml.includes(
        `<w:spacing w:before="${metrics.beforePt * 20}" w:after="${metrics.afterPt * 20}"/>`,
      ),
      `${id} spacing ${metrics.beforePt}/${metrics.afterPt}pt is missing from styles.xml`,
    );
  }
});

test("the page geometry in the table is the page the section writes", () => {
  assert.ok(
    documentXml.includes(
      `<w:pgSz w:w="${BLANK_PAGE.widthPt * 20}" w:h="${BLANK_PAGE.heightPt * 20}"/>`,
    ),
    "pgSz does not match BLANK_PAGE",
  );
  const margin = BLANK_PAGE.marginPt * 20;
  assert.ok(
    documentXml.includes(
      `<w:pgMar w:top="${margin}" w:right="${margin}" w:bottom="${margin}" w:left="${margin}"`,
    ),
    "pgMar does not match BLANK_PAGE",
  );
});

test("every template previews on the same page its document uses", () => {
  for (const template of DOCUMENT_TEMPLATES) {
    const thumbnail = templateThumbnail(template.id);
    assert.equal(thumbnail.widthPt, BLANK_PAGE.widthPt, template.id);
    assert.equal(thumbnail.heightPt, BLANK_PAGE.heightPt, template.id);
    assert.equal(thumbnail.marginPt, BLANK_PAGE.marginPt, template.id);
  }
});

test("a preview block exists for every paragraph the template writes", () => {
  for (const template of DOCUMENT_TEMPLATES) {
    const thumbnail = templateThumbnail(template.id);
    assert.equal(
      thumbnail.blocks.length,
      template.body.length,
      `${template.id} previews ${thumbnail.blocks.length} of ${template.body.length} paragraphs`,
    );
    thumbnail.blocks.forEach((block, index) => {
      const [style, text] = template.body[index];
      assert.equal(block.text, text ?? "", `${template.id} paragraph ${index} text`);
      assert.equal(block.style, style ?? "Normal", `${template.id} paragraph ${index} style`);
    });
  }
});

test("a preview block carries the style's real metrics, not a default", () => {
  const report = templateThumbnail("report");
  const title = report.blocks[0];
  assert.equal(title.style, "Title");
  assert.equal(title.sizePt, BLANK_STYLE_METRICS.Title.sizePt);
  assert.equal(title.bold, BLANK_STYLE_METRICS.Title.bold);
  const subtitle = report.blocks[1];
  assert.equal(subtitle.italic, true, "Subtitle is italic in the styles, so it must preview italic");
  const heading = report.blocks.find((block) => block.style === "Heading1");
  assert.equal(heading.bold, true, "headings are bold in the styles");
  assert.ok(
    heading.sizePt < title.sizePt,
    "a heading previewing at or above the title size would misrepresent the page",
  );
  assert.ok(
    heading.beforePt > 0,
    "Heading1 has space before it in the styles; a preview without it packs the page wrongly",
  );
});

test("an unstyled paragraph previews as body text", () => {
  const letter = templateThumbnail("letter");
  assert.ok(letter.blocks.every((block) => block.style === "Normal"));
  assert.equal(letter.blocks[0].sizePt, BLANK_STYLE_METRICS.Normal.sizePt);
});

test("the blank template previews an empty page, and an unknown id falls back to it", () => {
  const blank = templateThumbnail("blank");
  assert.deepEqual(
    blank.blocks.map((block) => block.text),
    [""],
  );
  assert.deepEqual(templateThumbnail("no-such-template"), blank);
});

test("each template still writes a package, and no two are the same bytes", () => {
  const sizes = new Map();
  for (const template of DOCUMENT_TEMPLATES) {
    const bytes = templateBytes(template.id);
    assert.ok(bytes.length > 0, template.id);
    assert.equal(bytes[0], 0x50, `${template.id} is not a ZIP`);
    sizes.set(template.id, bytes.length);
  }
  assert.equal(new Set(sizes.values()).size, sizes.size, "two templates wrote identical packages");
});
