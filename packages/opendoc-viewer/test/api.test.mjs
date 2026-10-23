// The Viewer SDK package, driven the way a host drives it: the built WebAssembly
// module through the JS API, in Node. Needs `./build.sh` first (CI's
// `viewer-package` job builds, then runs this). A missing build fails loudly:
// "nothing to test" must never read as a pass.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { CORE_FAMILIES, PACKAGE_FONTS, nodeFontProvider, openDocument } from "../src/node.js";

const SAMPLE = new URL("../../../sample.docx", import.meta.url);
const sample = await readFile(SAMPLE);

const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

test("a DOCX opens, every page renders to RGBA of its size, and a page is not blank", async () => {
  const doc = await openDocument(sample);
  assert.ok(doc.pageCount >= 1);
  const sizes = doc.pageSizes();
  assert.equal(sizes.length, doc.pageCount);
  for (let i = 0; i < doc.pageCount; i++) {
    assert.deepEqual(doc.pageSize(i), sizes[i]);
    const { width, height, rgba } = doc.renderPage(i, { dpi: 24 });
    assert.equal(width, Math.ceil((sizes[i].widthTwips * 24) / 1440));
    assert.equal(height, Math.ceil((sizes[i].heightTwips * 24) / 1440));
    assert.equal(rgba.length, width * height * 4);
    assert.ok(
      rgba.some((value, index) => index % 4 !== 3 && value !== 255),
      `page ${i} rendered blank`,
    );
  }
  assert.throws(() => doc.pageSize(doc.pageCount), /does not exist/);
  assert.throws(() => doc.renderPage(doc.pageCount), /does not exist/);
  doc.free();
  assert.throws(() => doc.pageCount, /freed/);
});

test("render is deterministic per page across two opens", async () => {
  const a = await openDocument(sample);
  const b = await openDocument(sample);
  assert.equal(a.pageCount, b.pageCount);
  for (let i = 0; i < a.pageCount; i++) {
    const first = digest(a.renderPage(i, { dpi: 24 }).rgba);
    assert.equal(digest(a.renderPage(i, { dpi: 24 }).rgba), first, `page ${i} changed between renders`);
    assert.equal(digest(b.renderPage(i, { dpi: 24 }).rgba), first, `page ${i} differs between opens`);
  }
});

test("a thumbnail is a PNG, and the PDF is a PDF with one page per laid-out page", async () => {
  const doc = await openDocument(sample);
  const png = doc.renderPagePng(0, { dpi: 24 });
  assert.deepEqual([...png.slice(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  assert.equal(doc.hasPdf, true);
  const pdf = Buffer.from(doc.toPdf());
  assert.equal(pdf.subarray(0, 5).toString("latin1"), "%PDF-");
  const pages = pdf.toString("latin1").split("/Type/Page/Parent").length - 1;
  assert.equal(pages, doc.pageCount);
});

test("text per page concatenates to the whole text; the outline has levels and pages", async () => {
  const doc = await openDocument(sample);
  const pages = Array.from({ length: doc.pageCount }, (_, i) => doc.pageText(i));
  assert.equal(pages.join(""), doc.text());
  assert.match(doc.text(), /OpenDoc Feature Test Document/);
  assert.notEqual(pages[0], pages[1]);
  const outline = doc.outline();
  assert.ok(outline.length >= 3, JSON.stringify(outline));
  for (const entry of outline) {
    assert.ok(entry.level >= 1 && entry.level <= 9);
    assert.ok(entry.page === null || (entry.page >= 0 && entry.page < doc.pageCount));
    if (entry.page !== null) {
      assert.ok(pages[entry.page].includes(entry.text), `"${entry.text}" is not on page ${entry.page}`);
    }
  }
  assert.throws(() => doc.pageText(doc.pageCount), /does not exist/);
});

test("bytes that are not a DOCX are refused with a sentence", async () => {
  await assert.rejects(openDocument(new TextEncoder().encode("not a document")), /open document/);
});

// The font policy (docs/176 §10, Q2), on the artefact that ships it.
test("the module embeds only the core family; the host's faces fill every need", async () => {
  assert.deepEqual([...CORE_FAMILIES], ["Liberation Sans"]);

  // Core set only: Calibri's substitute is NOT in the module, and the viewer says so.
  const bare = await openDocument(sample, { fontProvider: null });
  const calibri = bare.fontNeeds().find((need) => need.requested === "Calibri");
  assert.deepEqual(calibri, { requested: "Calibri", substitute: "Carlito", substituteAvailable: false });
  for (const need of bare.fontNeeds()) {
    if (need.substitute && need.substitute !== "Liberation Sans") {
      assert.equal(need.substituteAvailable, false, `${need.substitute} is embedded in the module`);
      assert.ok(need.substitute in PACKAGE_FONTS, `the package does not ship ${need.substitute}`);
    }
  }

  // With the package's faces served by the host, every need is met.
  const asked = [];
  const provider = nodeFontProvider();
  const served = await openDocument(sample, {
    fontProvider: (need) => {
      asked.push(need.requested);
      return provider(need);
    },
  });
  assert.ok(asked.includes("Calibri"), `the hook was not asked for Calibri: ${asked}`);
  assert.deepEqual(
    served.fontNeeds().filter((need) => !need.substituteAvailable),
    [],
    "a need is still unmet with every package face served",
  );
  assert.notEqual(
    digest(served.renderPage(0, { dpi: 24 }).rgba),
    digest(bare.renderPage(0, { dpi: 24 }).rgba),
    "serving Carlito did not change the page",
  );

  // Preloading the same faces gives the same pixels in one layout.
  const carlito = await provider({ requested: "Calibri", substitute: "Carlito", substituteAvailable: false });
  const others = await Promise.all(
    served
      .fontNeeds()
      .filter((need) => need.substitute in PACKAGE_FONTS && need.substitute !== "Carlito")
      .map((need) => provider({ ...need, substituteAvailable: false })),
  );
  const preloaded = await openDocument(sample, { fonts: [...carlito, ...others.flat()], fontProvider: null });
  assert.equal(preloaded.pageCount, served.pageCount);
  assert.equal(
    digest(preloaded.renderPage(0, { dpi: 24 }).rgba),
    digest(served.renderPage(0, { dpi: 24 }).rgba),
  );
});

/** Horizontal bands of ink on a rendered page: `[top, bottom)` row ranges. */
function inkBands({ width, height, rgba }) {
  const bands = [];
  let top = -1;
  for (let y = 0; y <= height; y++) {
    let ink = false;
    if (y < height) {
      for (let x = 0; x < width && !ink; x++) {
        const at = (y * width + x) * 4;
        ink = rgba[at] < 250 || rgba[at + 1] < 250 || rgba[at + 2] < 250;
      }
    }
    if (ink && top === -1) top = y;
    if (!ink && top !== -1) {
      bands.push([top, y]);
      top = -1;
    }
  }
  return bands;
}

// A field value (PAGE) is re-shaped at pagination. With Calibri served by the
// host, the value is shaped in the host's Carlito; the run used to keep the
// resolver's bundled Carlito id, whose bytes the core-font module does not
// have, so the renderer outlined Carlito glyph ids from Liberation Sans:
// sample.docx's footer "2 of 14" painted as "Ï of ãÕ". The fixture paints
// "Page 1" twice, once as text and once with the 1 from a PAGE field; the two
// lines must be the same pixels.
test("a field value is drawn from the face that shaped it", async () => {
  const fixture = await readFile(new URL("./fixtures/field-face.docx", import.meta.url));
  const doc = await openDocument(fixture);
  assert.deepEqual(
    doc.fontNeeds().filter((need) => !need.substituteAvailable),
    [],
    "Carlito was not served",
  );
  const page = doc.renderPage(0, { dpi: 96 });
  const bands = inkBands(page);
  assert.equal(bands.length, 2, `expected two lines of ink, found ${JSON.stringify(bands)}`);
  // The lines sit at different fractional baselines, so their anti-aliased
  // rows differ by a pixel; their glyphs' horizontal extents do not. A glyph
  // drawn from the wrong face has a different width and position.
  const [literal, field] = bands.map((band) => inkColumns(page, band));
  assert.equal(field.length, literal.length, `glyph columns ${JSON.stringify({ literal, field })}`);
  literal.forEach(([start, end], index) => {
    assert.ok(
      Math.abs(field[index][0] - start) <= 1 && Math.abs(field[index][1] - end) <= 1,
      `glyph ${index} of the field line is not the literal line's: ${JSON.stringify({ literal, field })}`,
    );
  });
});

/** Runs of columns with ink inside a band: one per glyph (or touching pair). */
function inkColumns({ width, rgba }, [top, bottom]) {
  const spans = [];
  let start = -1;
  for (let x = 0; x <= width; x++) {
    let ink = false;
    for (let y = top; x < width && y < bottom && !ink; y++) {
      const at = (y * width + x) * 4;
      ink = rgba[at] < 160 && rgba[at + 1] < 160 && rgba[at + 2] < 160;
    }
    if (ink && start === -1) start = x;
    if (!ink && start !== -1) {
      spans.push([start, x]);
      start = -1;
    }
  }
  return spans;
}
