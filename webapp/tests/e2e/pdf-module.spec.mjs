// The lazily loaded PDF module in the editor (`docs/182` §17a): when it is
// fetched and when it is not, Cancel during the fetch, the saves that go
// through it (the exact bytes, and the byte-preserving update the PDF module
// plans), text recognition's consent with a host's models and without any,
// and Convert to flowing document.
//
// The OCR specs here never use a real model: the host's manifest is served by
// the spec with files of the right size and SHA-256 that are not models, so the
// consent, the download, the verification, the offline cache and the preload
// are what is exercised, and the engine's refusal to load them is the expected
// ending. A real model, and the main thread while it runs, is
// `ocr-main-thread.spec.mjs`.
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import {
  closeAppMenu,
  expect,
  gotoEditor,
  menuCommandRow,
  runAppMenuCommand,
  stableBox,
  test,
  useCompactChrome,
} from "./fixtures.mjs";
import { PDF_FIXTURE, scanPdf } from "./pdf-import-helpers.mjs";

const PDF_WASM = /casual_doc_wasm_pdf_bg\.wasm/;
const findings = (page) => page.locator("#compatibilityFindingsDialog");
const dialog = (page) => page.locator("#pdfImportDialog");

/** Counts the requests for the PDF module's wasm. */
function countPdfModuleFetches(page) {
  const seen = { count: 0 };
  page.on("request", (request) => {
    if (PDF_WASM.test(request.url())) seen.count += 1;
  });
  return seen;
}

async function openBuffer(page, name, buffer) {
  await page.locator("#file").setInputFiles({ name, mimeType: "application/octet-stream", buffer });
  await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe(name);
}

async function closeFindings(page) {
  await expect(findings(page)).toBeVisible();
  await page.locator("#compatibilityFindingsDialogDone").click();
  await expect(findings(page)).toBeHidden();
}

async function download(page, commandId) {
  const pending = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", commandId);
  const saved = await pending;
  return { name: saved.suggestedFilename(), bytes: await readFile(await saved.path()) };
}

/** Serves a host's OCR manifest whose three files are the right size and hash
 *  but are not models. Returns what was asked for: `manifest` and `files`
 *  count requests, `hold()` keeps the next model-file response back until
 *  `release()`. Routed on the CONTEXT, not the page: the OCR worker fetches
 *  the manifest and the models, and a worker's requests are the context's. */
async function serveFakeModels(page) {
  const files = {
    "det.onnx": Buffer.alloc(150_000, 1),
    "rec.onnx": Buffer.alloc(250_000, 2),
    "dict.txt": Buffer.from("a\nb\nc\n"),
  };
  const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
  const entry = (name) => ({ url: name, sha256: sha(files[name]), bytes: files[name].length });
  const manifest = { name: "fake", detection: entry("det.onnx"), packs: [{ name: "latin", model: entry("rec.onnx"), dictionary: entry("dict.txt") }] };
  const seen = { manifest: 0, files: 0, gate: null };
  seen.hold = () => {
    let release;
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    seen.gate = gate;
    seen.release = () => {
      seen.gate = null;
      release();
    };
  };
  // Matched on the PATH, never by a glob over the whole URL: the editor's own
  // address carries `?pdfOcrModels=/ocr-test/manifest.json`, and a `**/...`
  // glob matched the page navigation itself and answered it with the
  // manifest, so the editor never booted (the quarantine of c418f74c).
  await page.context().route(
    (url) => url.pathname === "/ocr-test/manifest.json",
    (route) => {
      seen.manifest += 1;
      seen.manifestAt ??= Date.now();
      return route.fulfill({ contentType: "application/json", body: JSON.stringify(manifest) });
    },
  );
  await page.context().route((url) => /^\/ocr-test\/[^/]+\.(onnx|txt)$/.test(url.pathname), async (route) => {
    seen.files += 1;
    if (seen.gate) await seen.gate;
    const name = new URL(route.request().url()).pathname.split("/").pop();
    // A cancelled run terminated the worker that asked: nothing to answer.
    await route.fulfill({ contentType: "application/octet-stream", body: files[name] }).catch(() => {});
  });
  return seen;
}

test.describe("the lazily loaded PDF module", () => {
  test.setTimeout(150_000);

  test("a DOCX never fetches the PDF module; the first PDF fetches it once and a second PDF reuses it", async ({
    page,
    consoleErrors,
  }) => {
    const fetches = countPdfModuleFetches(page);
    await gotoEditor(page);
    expect(fetches.count, "opening the startup DOCX fetched the PDF module").toBe(0);

    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-prose.pdf");
    await closeFindings(page);
    expect(fetches.count).toBe(1);

    await openBuffer(page, "again.pdf", await readFile(PDF_FIXTURE));
    await closeFindings(page);
    expect(fetches.count, "a second PDF open fetched the module again").toBe(1);
    expect(consoleErrors).toEqual([]);
  });

  test("Cancel while the PDF module downloads leaves the open document, and the next open still works", async ({
    page,
  }) => {
    let release;
    const held = new Promise((resolve) => {
      release = resolve;
    });
    await page.route(PDF_WASM, async (route) => {
      await held;
      // Cancel aborts the editor's fetch, which settles the route first; the
      // abort is only for a fetch still open when the test releases it.
      await route.abort().catch(() => {});
    });
    await gotoEditor(page);
    const before = await page.locator("#a11yDocument").textContent();

    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect(dialog(page)).toBeVisible();
    await expect(page.locator("#pdfImportDialogPhase")).toHaveText("Loading the PDF reader…");
    await page.locator("#pdfImportDialogCancel").click();
    await expect(dialog(page)).toBeHidden();
    await expect(page.locator("#status")).toHaveText(
      "Opening synthetic-prose.pdf was cancelled — the document you had open is still here",
    );
    await expect(page.locator("#docTitle")).toHaveValue("opendoc-demo.docx");
    expect(await page.locator("#a11yDocument").textContent()).toBe(before);

    // The aborted load kept nothing: the next open fetches and opens.
    release();
    await page.unroute(PDF_WASM);
    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-prose.pdf");
    await closeFindings(page);
  });

  test("an unchanged PDF saves as its own bytes and an edited one as an update appended to them", async ({
    page,
    consoleErrors,
  }) => {
    // Only the exact page layout saves back into the original (`docs/182` §4).
    await gotoEditor(page, "&pdfLayout=fixed");
    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-prose.pdf");
    await closeFindings(page);
    const original = await readFile(PDF_FIXTURE);

    const unchanged = await download(page, "file.export.pdf");
    expect(unchanged.bytes.equals(original), "an unchanged PDF is the original file").toBe(true);

    await expect(page.locator("body")).toHaveAttribute("data-fonts-ready", "true");
    const sheet = await stableBox(page.locator(".page-wrap .page").first());
    await page.mouse.dblclick(sheet.x + sheet.width * 0.2, sheet.y + sheet.height * 0.125);
    await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
    await page.keyboard.type("EDITED ");
    await expect(page.locator("#a11yDocument")).toContainText("EDITED");

    // The PDF module planned it (`planSave`), the main module wrote it: the
    // original is the prefix, byte for byte, and the update follows.
    const edited = await download(page, "file.export.pdf");
    expect(edited.bytes.length).toBeGreaterThan(original.length);
    expect(edited.bytes.subarray(0, original.length).equals(original), "the original is kept as the prefix").toBe(true);
    expect(edited.bytes.subarray(original.length).toString("latin1")).toContain("OpenDocEdit");
    expect(consoleErrors).toEqual([]);
  });

  test("with no recognition models, a scan's summary says its text is missing and why", async ({ page }) => {
    await gotoEditor(page);
    await openBuffer(page, "scan.pdf", scanPdf());
    await expect(findings(page)).toBeVisible();
    const ocr = findings(page).locator('[data-kind="pdfOcr"]');
    await expect(ocr).toContainText("Scanned pages");
    await expect(ocr.locator('[data-state="pending"]')).toContainText("only a picture: 1");
    await expect(ocr.locator('[data-state="unavailable"]')).toHaveText(/Text recognition is unavailable/);
    const action = findings(page).locator('[data-action="file.pdf.recogniseText"]');
    await expect(action).toBeDisabled();
    await expect(action).toHaveAttribute("title", /no recognition models/);
    // A PDF opens flowing: keeping its exact layout is live, converting is
    // not (it already is a flowing document), said with the reason.
    await expect(findings(page).locator('[data-action="file.pdf.keepExactLayout"]')).toBeEnabled();
    const convert = findings(page).locator('[data-action="file.pdf.convertToFlowing"]');
    await expect(convert).toBeDisabled();
    await expect(convert).toHaveAttribute("title", "This PDF is already open as a flowing document.");
  });

  // The contract since auto-recognition (owner, 2026-10-30): a scanned PDF is
  // recognised on its own, downloading the models WITHOUT a prompt, in the
  // BACKGROUND (owner report 2026-11-02): no dialog, its progress in the status
  // bar, and the same File-menu command, now "Stop recognising text", cancels
  // it. "Recognise text" asked for by hand still confirms first, naming the
  // size and the origin, until the models are kept in this browser.
  test("with a host's models, a scan is recognised on its own without asking; by hand, recognition asks until the models are kept", async ({
    page,
  }) => {
    const seen = await serveFakeModels(page);
    await gotoEditor(page, "&pdfOcrModels=/ocr-test/manifest.json");
    // Held, so the automatic run can be seen downloading and cancelled there.
    seen.hold();
    await openBuffer(page, "scan.pdf", scanPdf());

    // Automatic: no consent prompt and no dialog; the download shows in the
    // status bar while the scanned pages stay on screen.
    await expect(page.locator("body[data-pdf-recognition=running]")).toHaveCount(1);
    await expect(page.locator("#status")).toHaveText("Downloading recognition models…");
    await expect(dialog(page)).toBeHidden();
    await expect(page.locator("#confirmDialog")).toBeHidden();
    await expect.poll(() => seen.files, { message: "the automatic run downloads without asking" }).toBeGreaterThan(0);

    // Stop terminates the worker: nothing is kept, the document stays.
    if (await findings(page).isVisible()) await closeFindings(page);
    await runAppMenuCommand(page, "file", "file.pdf.recogniseText");
    await expect(page.locator("body[data-pdf-recognition=running]")).toHaveCount(0);
    await expect(page.locator("#status")).toHaveText("Text recognition was cancelled — the document is unchanged");
    await expect(page.locator("#docTitle")).toHaveValue("scan.pdf");
    seen.release();
    if (await findings(page).isVisible()) await closeFindings(page);
    const before = seen.files;

    // By hand, with nothing kept: it asks first. Declined: nothing is downloaded.
    await runAppMenuCommand(page, "file", "file.pdf.recogniseText");
    await expect(page.locator("#confirmDialog")).toBeVisible();
    await expect(page.locator("#confirmTitle")).toHaveText("Download text recognition?");
    await expect(page.locator("#confirmDescription")).toContainText("0.4 MB");
    await expect(page.locator("#confirmDescription")).toContainText(new URL(page.url()).host);
    await page.locator("#confirmCancel").click();
    await expect(page.locator("#confirmDialog")).toBeHidden();
    expect(seen.files, "a model was fetched before the reader agreed").toBe(before);

    // Agreed: the three files are fetched and verified; they are not models,
    // so the engine refuses them, said in words, and the document stays.
    await runAppMenuCommand(page, "file", "file.pdf.recogniseText");
    await page.locator("#confirmAccept").click();
    await expect(page.locator("#status")).toContainText("Text recognition failed", { timeout: 60_000 });
    expect(seen.files).toBe(before + 3);
    await expect(page.locator("#docTitle")).toHaveValue("scan.pdf");

    // Kept: by hand again, it asks nothing and downloads nothing.
    await page.evaluate(() => {
      document.getElementById("status").textContent = "";
    });
    await runAppMenuCommand(page, "file", "file.pdf.recogniseText");
    await expect(page.locator("#status")).toContainText("Text recognition failed", { timeout: 60_000 });
    await expect(page.locator("#confirmDialog")).toBeHidden();
    expect(seen.files, "the verified models were downloaded again").toBe(before + 3);
  });

  // The preload (`docs/182` §17a.5): the first scanned page the open reads
  // starts the models loading in the OCR worker, while the rest of the PDF is
  // still being read, so the automatic run starts warm. A DOCX and a PDF with
  // no scans never ask for them. Mutation: drop the `onScans` call in
  // `pdf_import.mjs` and the first poll times out with the manifest unasked.
  test("a scanned PDF starts loading the models while it is still being read; a DOCX and a text PDF never ask", async ({
    page,
  }) => {
    const seen = await serveFakeModels(page);
    await gotoEditor(page, "&pdfOcrModels=/ocr-test/manifest.json");
    expect(seen.manifest + seen.files, "a DOCX asked for the OCR models").toBe(0);

    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-prose.pdf");
    await closeFindings(page);
    expect(seen.manifest + seen.files, "a PDF with no scans asked for the OCR models").toBe(0);

    // 400 scanned pages, read in the worker (HF-423). The guarantee: the
    // models are asked for while the pages are still being read, so the
    // download overlaps the read, not after it. Stated as an order of events:
    // the manifest request starts before the import's finish mark (set when
    // the worker's package arrives). Seen red with the worker's pause between
    // pages removed: the preload message waited behind the whole read.
    await page.evaluate(() => performance.clearMarks("opendoc:pdf-import:finish"));
    await page.locator("#file").setInputFiles({ name: "long-scan.pdf", mimeType: "application/pdf", buffer: scanPdf({ pages: 400 }) });
    await expect.poll(() => seen.files, { message: "the preload never fetched the models" }).toBe(3);
    const finishedAt = () =>
      page.evaluate(() => {
        const mark = performance.getEntriesByName("opendoc:pdf-import:finish")[0];
        return mark ? performance.timeOrigin + mark.startTime : null;
      });
    await expect.poll(finishedAt, { timeout: 60_000 }).not.toBeNull();
    expect(seen.manifestAt, "the models were asked for only after every page was read").toBeLessThan(await finishedAt());

    // The automatic run that follows uses what the preload kept: no second
    // download (these files are not models, so it ends refused, in words).
    await expect(page.locator("#status")).toContainText("Text recognition failed", { timeout: 60_000 });
    expect(seen.files, "the automatic run downloaded the models again").toBe(3);
  });

  test("Convert to flowing document opens the PDF rebuilt as a new document and says what it inferred", async ({
    page,
    consoleErrors,
  }) => {
    const NOT_PDF = "Only a document opened from a PDF can be converted to a flowing document.";
    const convertRow = async () => {
      await useCompactChrome(page);
      const row = await menuCommandRow(page, "file", "file.pdf.convertToFlowing");
      return row;
    };
    // Conversion starts from a PDF opened with its exact page layout.
    await gotoEditor(page, "&pdfLayout=fixed");
    // A DOCX: the row is there, disabled with the reason (SKILL §10).
    let row = await convertRow();
    await expect(row).toBeDisabled();
    await expect(row).toHaveAttribute("title", NOT_PDF);
    await closeAppMenu(page);

    await page.locator("#file").setInputFiles(PDF_FIXTURE);
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe("synthetic-prose.pdf");
    // From the summary (the first surface).
    await findings(page).locator('[data-action="file.pdf.convertToFlowing"]').click();
    await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe(
      "synthetic-prose (flowing).docx",
    );
    await expect(findings(page)).toBeVisible();
    const disclosure = findings(page).locator('[data-kind="pdfReflowDisclosure"]');
    await expect(disclosure).toContainText("A flowing document rebuilt from a PDF");
    await expect(disclosure.locator('[data-fact="new"]')).toContainText("This is a new document");
    await expect(findings(page).locator('[data-feature="pdf.import.paragraph_boundary_inferred"]')).toBeVisible();
    await closeFindings(page);
    // Flowing: the body is paragraphs, not frames on page anchors.
    await expect(page.locator("#a11yDocument")).toContainText("Synthetic prose");

    // A converted document keeps no PDF: the File menu row (the second
    // surface) is disabled again, with the same reason.
    row = await convertRow();
    await expect(row).toBeDisabled();
    await expect(row).toHaveAttribute("title", NOT_PDF);
    await closeAppMenu(page);
    expect(consoleErrors).toEqual([]);
  });
});
