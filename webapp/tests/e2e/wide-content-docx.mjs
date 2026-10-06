// Builds, in memory, the documents the pageless guards need — and reads back the
// screenshots they take.
//
// No shipped sample carries content WIDER than a reading column, and the one
// engine fixture that once claimed to (`docs/166` §5) fitted anyway, which is
// how a dropped table survived two design documents. So the inputs are built
// here, with the widths in the XML where a reader of the test can check them:
//
//   * `wideContentDocx()` — a 12-column table declared 12in wide in `dxa` with a
//     FIXED layout (the two over-wide arms of the column solver), a picture
//     declared 9in wide, and enough prose to cut the column into several tiles;
//   * `longTableDocx()` — a bordered three-column table long enough to cross
//     several tile cuts, because a seam shows on a vertical line that runs
//     THROUGH a cut and nowhere else.
//
// It also holds `openInReflow` and the PNG reader those guards share.
//
// Returned as bytes rather than committed, like `toc-docx.mjs` — whose package
// writer this shares rather than copies — so nothing has to be regenerated and
// no `--check` can stale.
import { deflateSync, inflateSync } from "node:zlib";

import { expect, gotoEditor } from "./fixtures.mjs";
import { crc32, zip } from "./toc-docx.mjs";

/** Opens one of these in-memory documents and turns reflow on (a desktop window
 *  opens on paper), then waits for the shell's own report that the render is
 *  done. Shared by the pageless specs, which need different launch flags and so
 *  live in different files. */
export async function openInReflow(page, file) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  await page.waitForFunction(
    (name) =>
      document.title.includes(name.replace(/\.docx$/, "")) ||
      document.querySelector(".page-wrap") !== null,
    file.name,
    { timeout: 45_000 },
  );
  await expect
    .poll(() => page.evaluate(() => document.body.dataset.fontsReady), { timeout: 45_000 })
    .toBe("true");
  const reflowing = () =>
    page.evaluate(() => document.getElementById("viewport").classList.contains("is-reflow"));
  if (!(await reflowing())) {
    await page.locator('.ribbon-tab[data-tab="view"]').click();
    await page.locator("#viewReflowBtn").click();
  }
  await expect.poll(reflowing, { timeout: 30_000 }).toBe(true);
}

/** The picture's colours, so a guard can find each edge in a raster. */
export const IMAGE_LEFT_RGB = [30, 160, 60];
export const IMAGE_RIGHT_RGB = [220, 30, 30];

/** An RGB PNG: green on its left 6%, red on its right 6%, blue between — so a
 *  crop is visible as a missing colour, not as a plausible-looking picture. */
function stripedPng(width, height) {
  const rows = [];
  for (let y = 0; y < height; y++) {
    const row = Buffer.alloc(1 + width * 3);
    for (let x = 0; x < width; x++) {
      const rgb =
        x >= width * 0.94 ? IMAGE_RIGHT_RGB : x < width * 0.06 ? IMAGE_LEFT_RGB : [40, 90, 200];
      row[1 + x * 3] = rgb[0];
      row[2 + x * 3] = rgb[1];
      row[3 + x * 3] = rgb[2];
    }
    rows.push(row);
  }
  const chunk = (tag, data) => {
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length, 0);
    const body = Buffer.concat([Buffer.from(tag, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body), 0);
    return Buffer.concat([head, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(Buffer.concat(rows))),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const NS =
  'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" ' +
  'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" ' +
  'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" ' +
  'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" ' +
  'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"';

const PROSE =
  "Pageless reading is one continuous column of text. This paragraph is here so " +
  "the column is cut into several tiles, and so a seam between two tiles, if there " +
  "is one, falls across ordinary prose where a reader would see it.";

const para = (text) => `<w:p><w:r><w:t xml:space="preserve">${text}</w:t></w:r></w:p>`;

/** The last column's text in a row of `table(…)`, for a guard that types there. */
export const lastColumnText = (label, row) => `LASTCOL ${label} r${row}`;

function table(cols, colWidth, rows, label) {
  const grid = Array.from({ length: cols }, () => `<w:gridCol w:w="${colWidth}"/>`).join("");
  const borders = ["top", "left", "bottom", "right", "insideH", "insideV"]
    .map((side) => `<w:${side} w:val="single" w:sz="8" w:space="0" w:color="333333"/>`)
    .join("");
  let out =
    `<w:tbl><w:tblPr><w:tblW w:w="${cols * colWidth}" w:type="dxa"/>` +
    `<w:tblBorders>${borders}</w:tblBorders><w:tblLayout w:type="fixed"/></w:tblPr>` +
    `<w:tblGrid>${grid}</w:tblGrid>`;
  for (let r = 0; r < rows; r++) {
    out += "<w:tr>";
    for (let c = 0; c < cols; c++) {
      const fill = r === 0 ? '<w:shd w:val="clear" w:color="auto" w:fill="DCE6F2"/>' : "";
      const text = c === cols - 1 ? lastColumnText(label, r) : `${label} r${r}c${c + 1}`;
      out +=
        `<w:tc><w:tcPr><w:tcW w:w="${colWidth}" w:type="dxa"/>${fill}</w:tcPr>` +
        `<w:p><w:r><w:t xml:space="preserve">${text}</w:t></w:r></w:p></w:tc>`;
    }
    out += "</w:tr>";
  }
  return `${out}</w:tbl>`;
}

const EMU_PER_INCH = 914_400;

function picture(rid, widthIn, heightIn) {
  const cx = widthIn * EMU_PER_INCH;
  const cy = heightIn * EMU_PER_INCH;
  return (
    "<w:p><w:r><w:drawing>" +
    `<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="${cx}" cy="${cy}"/>` +
    '<wp:docPr id="1" name="Wide picture" descr="A picture wider than any reading column"/>' +
    '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">' +
    '<pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="wide.png"/><pic:cNvPicPr/></pic:nvPicPr>' +
    `<pic:blipFill><a:blip r:embed="${rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>` +
    `<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="${cx}" cy="${cy}"/></a:xfrm>` +
    '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>' +
    "</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
  );
}

function docx(body, { image = false } = {}) {
  const document =
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${NS}><w:body>` +
    body +
    '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>' +
    '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" ' +
    'w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>';
  const entries = {
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
      (image
        ? '<Relationship Id="rIdImg1" ' +
          'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" ' +
          'Target="media/wide1.png"/>'
        : "") +
      "</Relationships>",
  };
  if (image) entries["word/media/wide1.png"] = stripedPng(900, 200);
  return zip(entries);
}

const DOCX_MIME = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/** The wide table's geometry, so a guard states its precondition in numbers. */
export const WIDE_TABLE = { cols: 12, colTwip: 1440, label: "W" };

/** A 12in-wide fixed table, a 9in picture, and prose to cut several tiles. */
export function wideContentDocx() {
  let body = para("Wide content in a pageless column.");
  body += table(WIDE_TABLE.cols, WIDE_TABLE.colTwip, 6, WIDE_TABLE.label);
  body += para("After the wide table.");
  body += picture("rIdImg1", 9, 2);
  body += para("After the wide picture.");
  for (let i = 0; i < 30; i++) body += para(`${i + 1}. ${PROSE}`);
  return { name: "wide.docx", mimeType: DOCX_MIME, buffer: docx(body, { image: true }) };
}

/** A bordered 3-column table long enough to run through several tile cuts —
 *  `colTwip` narrow enough, on a phone, that the TILES carry it rather than a
 *  wide-table scroller. */
export function longTableDocx(rows = 160, colTwip = 2600) {
  const body = para("A long table.") + table(3, colTwip, rows, "L") + para("The end.");
  return { name: "long-table.docx", mimeType: DOCX_MIME, buffer: docx(body) };
}

/**
 * Decodes an 8-bit RGB or RGBA, non-interlaced PNG — what Playwright's
 * `screenshot()` writes — into `{ width, height, rgba }`. Enough of the format
 * for a seam check, and nothing more: a screenshot that is not that shape
 * throws rather than being misread.
 */
export function decodePng(buffer) {
  let at = 8;
  let width = 0;
  let height = 0;
  let channels = 0;
  const idat = [];
  while (at < buffer.length) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.toString("ascii", at + 4, at + 8);
    const data = buffer.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      if (data[8] !== 8 || data[12] !== 0) throw new Error("only 8-bit non-interlaced PNG");
      channels = { 2: 3, 6: 4 }[data[9]];
      if (!channels) throw new Error(`unsupported PNG colour type ${data[9]}`);
    } else if (type === "IDAT") {
      idat.push(data);
    }
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const rgba = new Uint8Array(width * height * 4);
  let prev = new Uint8Array(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const out = new Uint8Array(stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? out[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      let v = line[i];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[i] = v & 0xff;
    }
    for (let x = 0; x < width; x++) {
      rgba[(y * width + x) * 4] = out[x * channels];
      rgba[(y * width + x) * 4 + 1] = out[x * channels + 1];
      rgba[(y * width + x) * 4 + 2] = out[x * channels + 2];
      rgba[(y * width + x) * 4 + 3] = channels === 4 ? out[x * channels + 3] : 255;
    }
    prev = out;
  }
  return { width, height, rgba };
}
