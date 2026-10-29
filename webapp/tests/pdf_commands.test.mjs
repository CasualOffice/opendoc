// When the two PDF commands may run (`pdf_commands.mjs`, `docs/182` §17a):
// enabled only where they can do something, and otherwise disabled with the
// reason that says what is missing (SKILL §10), never absent.
import assert from "node:assert/strict";
import test from "node:test";

import { convertRefusal, ocrSummaryOf, recogniseRefusal } from "../src/pdf_commands.mjs";
import { ocrManifestUrl } from "../src/ocr_models.mjs";

const scan = { rasterPages: 2, hybridPages: 0, regionsRecognized: 0, regionsSkipped: 2, wordsKept: 0, provider: null };

test("Recognise text runs only on a PDF's unread scans with a host's models", () => {
  const url = "https://host.example/models.json";
  assert.equal(recogniseRefusal({ pdfOrigin: true, ocr: scan, manifestUrl: url }), "");
  assert.equal(recogniseRefusal({ pdfOrigin: false, ocr: scan, manifestUrl: url }), "pdfOcr.notPdf");
  assert.equal(
    recogniseRefusal({ pdfOrigin: true, ocr: { ...scan, rasterPages: 0 }, manifestUrl: url }),
    "pdfOcr.noScans",
  );
  assert.equal(
    recogniseRefusal({ pdfOrigin: true, ocr: { ...scan, rasterPages: 0, hybridPages: 1 }, manifestUrl: url }),
    "",
    "a page with native text and a picture to read is a scan too",
  );
  assert.equal(
    recogniseRefusal({ pdfOrigin: true, ocr: { ...scan, provider: "pp-ocrv5" }, manifestUrl: url }),
    "pdfOcr.done",
  );
  assert.equal(recogniseRefusal({ pdfOrigin: true, ocr: scan, manifestUrl: null }), "pdfOcr.noModels");
  assert.equal(recogniseRefusal({ pdfOrigin: true, ocr: null, manifestUrl: url }), "pdfOcr.noScans");
});

test("Convert to flowing document runs only on a PDF-origin session", () => {
  assert.equal(convertRefusal({ pdfOrigin: true }), "");
  assert.equal(convertRefusal({ pdfOrigin: false }), "pdfReflow.notPdf");
});

test("the OCR summary is read from the session, and a session without one has none", () => {
  assert.deepEqual(ocrSummaryOf({ pdfOcrSummaryJson: JSON.stringify(scan) }), scan);
  assert.equal(ocrSummaryOf({ pdfOcrSummaryJson: "" }), null);
  assert.equal(ocrSummaryOf(null), null);
});

test("the host's manifest option is read in any case, like every host option", () => {
  assert.equal(ocrManifestUrl("?pdfOcrModels=https://a.example/m.json"), "https://a.example/m.json");
  assert.equal(ocrManifestUrl("?PDFOCRMODELS=https://a.example/m.json"), "https://a.example/m.json");
  assert.equal(ocrManifestUrl("?blank=1"), null);
});
