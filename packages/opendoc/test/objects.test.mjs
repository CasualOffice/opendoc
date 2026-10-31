// Pictures, headers and footers, notes and sections by edit (docs/176 §12.12),
// through the JS API a host calls. Needs `./build.sh` first.
import assert from "node:assert/strict";
import test from "node:test";

import { Document } from "../src/node.js";

/** A real PNG: a blank page drawn by the engine at 4 dpi. */
function png(dpi = 4) {
  const blank = Document.create();
  try {
    return blank.renderPagePng(0, { dpi });
  } finally {
    blank.free();
  }
}

const images = (doc) => doc.paragraphs.flatMap((p) => p.runs).filter((r) => r.kind === "image");

function reopen(doc) {
  const { bytes, report } = doc.save("docx");
  assert.deepEqual(report.losses, [], "nothing lost on save");
  return Document.open(bytes);
}

test("a picture is added, resized, described, replaced and deleted", () => {
  const doc = Document.create();
  const picture = doc.addPicture(png(), { widthPt: 72 });
  let [image] = images(doc);
  assert.equal(image.id, picture);
  assert.equal(image.image.widthPt, 72);
  assert.deepEqual(doc.mediaBytes(image.image.part), png());
  doc.resizePicture(picture, 144, 36);
  doc.setAltText(picture, "Logo");
  doc.replacePicture(picture, png(6));
  [image] = images(doc);
  assert.equal(image.id, picture);
  assert.equal(image.image.description, "Logo");
  assert.deepEqual(doc.mediaBytes(image.image.part), png(6));
  const back = reopen(doc);
  assert.deepEqual(back.mediaBytes(images(back)[0].image.part), png(6));
  const { undo } = doc.edit([{ op: "deletePicture", picture }]);
  assert.equal(images(doc).length, 0);
  doc.undo(undo);
  assert.equal(images(doc).length, 1);
});

test("headers and footers: default, first and even, with page fields", () => {
  const doc = Document.create();
  doc.addParagraph("Body");
  doc.insertText(doc.header(0), 0, "Report");
  const footer = doc.footer(0);
  doc.insertText(footer, 0, "Page ");
  doc.insertField(footer, 5, "page");
  doc.setDifferentFirstPage(0);
  doc.insertText(doc.header(0, { kind: "first" }), 0, "Cover");
  doc.setDifferentOddEven();
  doc.insertText(doc.header(0, { kind: "even" }), 0, "Even");
  for (const d of [doc, reopen(doc)]) {
    const texts = d.headersFooters.map((h) => h.text);
    for (const want of ["Report", "Cover", "Even"]) assert.ok(texts.includes(want), `${want} in ${texts}`);
    assert.ok(texts.some((t) => t.startsWith("Page ")));
    assert.equal(d.sections[0].titlePage, true);
    const fields = d.headersFooters.flatMap((h) => h.blocks).flatMap((b) => b.runs ?? []).map((r) => r.field).filter(Boolean);
    assert.ok(fields.some((f) => f.startsWith("PAGE")));
  }
  doc.removeHeader(0);
  assert.ok(!doc.sections[0].headers.some((h) => h.kind === "default"));
});

test("notes are added, edited and deleted", () => {
  const doc = Document.create();
  const p = doc.addParagraph("Claim.");
  const foot = doc.addFootnote(p, 6, "Source");
  const end = doc.addEndnote(p, 6, "Later");
  assert.deepEqual(doc.notes.map((n) => [n.kind, n.text]).sort(), [["endnote", "Later"], ["footnote", "Source"]]);
  doc.setNoteText(foot, "Source, p. 4");
  assert.ok(doc.notes.some((n) => n.id === foot && n.text === "Source, p. 4"));
  doc.deleteNote(end);
  assert.ok(!doc.notes.some((n) => n.id === end));
  assert.ok(reopen(doc).notes.some((n) => n.text === "Source, p. 4"));
});

test("sections are added, typed and set up, atomically", () => {
  const doc = Document.create();
  doc.addParagraph("Portrait");
  assert.equal(doc.addSection("nextPage"), 1);
  doc.addParagraph("Landscape");
  doc.setPageSetup(1, { orientation: "landscape", topPt: 36, columns: 2 });
  doc.setSectionStart(1, "continuous");
  for (const d of [doc, reopen(doc)]) {
    const [a, b] = d.sections;
    assert.equal(b.orientation, "landscape");
    assert.equal(b.pageWidthPt, a.pageHeightPt);
    assert.equal(b.marginsPt.top, 36);
    assert.equal(b.columns, 2);
    assert.equal(b.start, "continuous");
  }
  const before = doc.save("json").bytes;
  assert.throws(() => doc.edit([{ op: "addSection" }, { op: "setPageSetup", section: 9, setup: {} }]), /section 9/);
  assert.deepEqual(doc.save("json").bytes, before, "a refused batch changed nothing");
});

test("a page renders as SVG, from the same layout, with its text", () => {
  const doc = Document.create();
  doc.addParagraph("Vector & <text>");
  const svg = doc.renderPageSvg(0);
  assert.ok(svg.startsWith("<svg "), svg.slice(0, 40));
  assert.match(svg, /<text [^>]*>Vector &amp; &lt;text&gt;<\/text>/);
  assert.throws(() => doc.renderPageSvg(5), /page/);
});
