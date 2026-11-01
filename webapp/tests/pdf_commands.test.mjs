// When the two PDF commands may run (`pdf_commands.mjs`, `docs/182` §17a):
// enabled only where they can do something, and otherwise disabled with the
// reason that says what is missing (SKILL §10), never absent.
import assert from "node:assert/strict";
import test from "node:test";

import { readFileSync } from "node:fs";
import {
  convertRefusal,
  createPdfCommands,
  hostPdfLayout,
  keepLayoutRefusal,
  ocrSummaryOf,
  pdfLayoutOf,
  recogniseRefusal,
} from "../src/pdf_commands.mjs";
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

test("Convert to flowing document runs only on a PDF opened with its exact layout", () => {
  assert.equal(convertRefusal({ pdfOrigin: true, pdfLayout: "fixed" }), "");
  assert.equal(convertRefusal({ pdfOrigin: true, pdfLayout: "flowing" }), "pdfReflow.alreadyFlowing");
  assert.equal(convertRefusal({ pdfOrigin: false, pdfLayout: "" }), "pdfReflow.notPdf");
});

// `docs/182` §4: a PDF opens flowing, so "Keep exact page layout" is offered
// on a flowing PDF and refused, with the reason, on anything else. Mutation:
// return "" for `pdfLayout: "fixed"` and the second assertion fails.
test("Keep exact page layout runs only on a flowing PDF session", () => {
  assert.equal(keepLayoutRefusal({ pdfOrigin: true, pdfLayout: "flowing" }), "");
  assert.equal(keepLayoutRefusal({ pdfOrigin: true, pdfLayout: "fixed" }), "pdfFixed.already");
  assert.equal(keepLayoutRefusal({ pdfOrigin: false, pdfLayout: "" }), "pdfFixed.notPdf");
});

test("the host's layout option keeps the exact layout only when it says so, in any case", () => {
  assert.equal(hostPdfLayout(""), "flowing");
  assert.equal(hostPdfLayout("?pdfLayout=fixed"), "fixed");
  assert.equal(hostPdfLayout("?PDFLAYOUT=Fixed"), "fixed");
  assert.equal(hostPdfLayout("?pdfLayout=flowing"), "flowing");
  assert.equal(hostPdfLayout("?pdfLayout=sideways"), "flowing");
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

test("the deployment's own models are the default; the host's option overrides them, and none turns them off", () => {
  const withTag = { querySelector: (sel) => (sel.includes("opendoc-ocr-manifest") ? { getAttribute: () => "./ocr/manifest.json" } : null) };
  const noTag = { querySelector: () => null };
  assert.match(ocrManifestUrl("", withTag), /\/ocr\/manifest\.json$/);
  assert.equal(ocrManifestUrl("", noTag), null, "a build that staged no models offers none");
  assert.match(ocrManifestUrl("?pdfOcrModels=https://host.example/m.json", withTag), /^https:\/\/host\.example\/m\.json$/);
  assert.equal(ocrManifestUrl("?pdfocrmodels=none", withTag), null);
});

test("a scanned PDF that recognition will reopen holds its first summary back, so it shows once", () => {
  const url = "https://host.example/models.json";
  const commands = (summary, search = `?pdfOcrModels=${encodeURIComponent(url)}`) =>
    createPdfCommands(
      { doc: { pdfOrigin: true, pdfLayout: "flowing", pdfOcrSummaryJson: JSON.stringify(summary) } },
      { search: () => search, ocrService: null, view: {} },
    );
  assert.equal(commands(scan).willAutoRecognise(), true, "unread scans with models: recognition reopens it");
  assert.equal(commands({ ...scan, provider: "pp-ocrv5" }).willAutoRecognise(), false, "the recognised open is final");
  assert.equal(commands({ ...scan, rasterPages: 0 }).willAutoRecognise(), false, "no scans, nothing to wait for");
  assert.equal(commands(scan, "?pdfOcrModels=none").willAutoRecognise(), false, "no models, no reopen");
  // The open's summary is gated on it (the open that recognition makes
  // reports the final state).
  const open = readFileSync(new URL("../src/editor_file_open.mjs", import.meta.url), "utf8");
  assert.match(open, /const recognising = origin === "pdf" && pdfCommands\.willAutoRecognise\(\);/);
  assert.match(open, /if \(origin && !ed\.activatingRestore && !recognising\) ed\.compatFindings\.open\(\);/);
});

// The PDF layout switch (UX-051, owner 2026-11-01): View ▸ Page display ▸ PDF
// layout, the palette and the status-bar chip all run these two rows. It is
// SYMMETRIC: from the exact layout, Flowing REOPENS the PDF flowing (keeping
// the original, so the reader can switch back), not "Convert to flowing
// document", which builds a document that keeps no PDF.
test("the PDF layout switch: two radio rows, the current layout checked, symmetric", async () => {
  const opened = [];
  const sessionIn = (pdfLayout) => ({
    doc: { pdfOrigin: true, pdfLayout, pdfOriginal: () => new Uint8Array([37, 80]) },
    currentName: "report.pdf",
    confirmDiscardIfEdited: async () => true,
    setStatus: (text) => opened.push({ status: text }),
  });
  const io = {
    search: () => "",
    ocrService: null,
    view: {},
    pdfImport: { run: (bytes, name, options) => ({ name, options }) },
    openBytes: async (bytes, name, _a, _b, options) =>
      opened.push({ name, origin: options.origin, prepared: options.prepare() }),
  };
  const rows = (ed) => Object.fromEntries(createPdfCommands(ed, io).layoutCommands().map((row) => [row.id, row]));
  const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

  const fromExact = rows(sessionIn("fixed"));
  assert.deepEqual(
    Object.values(fromExact).map((row) => [row.id, row.checked, row.radio, row.enabled]),
    [
      ["view.pdfLayout.flowing", false, true, true],
      ["view.pdfLayout.exact", true, true, true],
    ],
  );
  fromExact["view.pdfLayout.flowing"].run();
  await tick();
  const reopened = opened.pop();
  assert.equal(reopened.origin, "pdf", "Flowing reopens the PDF as a PDF session, so the switch stays");
  assert.equal(reopened.name, "report.pdf", "under its own name, not as a converted copy");
  assert.equal(reopened.prepared.options.layout, "flowing");
  assert.equal(reopened.prepared.options.mode, undefined, "an open, not a reflow conversion that keeps no PDF");

  const fromFlowing = rows(sessionIn("flowing"));
  assert.equal(fromFlowing["view.pdfLayout.flowing"].checked, true);
  fromFlowing["view.pdfLayout.exact"].run();
  await tick();
  assert.equal(opened.pop().prepared.options.mode, "fixed");

  // The checked row refuses with its reason; a document that is not a PDF
  // gets both rows disabled with the reason, never absent.
  fromFlowing["view.pdfLayout.flowing"].run();
  await tick();
  assert.ok(opened.pop().status, "the checked row says it is already in that layout");
  for (const row of Object.values(rows({ doc: { pdfOrigin: false, pdfLayout: "" } }))) {
    assert.equal(row.enabled, false);
    assert.equal(row.checked, false);
    assert.ok(row.disabledReason);
  }
  assert.equal(pdfLayoutOf({ pdfOrigin: true, pdfLayout: "fixed" }), "exact");
  assert.equal(pdfLayoutOf({ pdfOrigin: true, pdfLayout: "flowing" }), "flowing");
  assert.equal(pdfLayoutOf({ pdfOrigin: false }), "");
});
