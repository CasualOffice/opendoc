import { readFile } from "node:fs/promises";
import { inflateRawSync } from "node:zlib";

import {
  test,
  expect,
  gotoEditor,
  expectSaveEnabled,
  menuCommandRow,
  saveDocument,
  runAppMenuCommand,
  useCompactChrome,
} from "./fixtures.mjs";

const ODT = "org.oasis.opendocument.text";
const TEXT = "text.plain";

/** One named part out of an OPC package, as text.
 *
 *  Twenty lines of local-file-header walk rather than a dependency, and rather
 *  than a raw byte search for the string: an OPC writer may store a part
 *  deflated, in which case `bytes.includes("…template…")` silently fails to find
 *  a content type that IS there — a guard that passes or fails on the
 *  compression setting rather than on what the file says. Handles both stored
 *  (method 0) and deflated (method 8) entries, which is every entry these
 *  writers produce.
 *
 *  Complexity: O(bytes) once, over a document the test itself just exported. */
function opcPart(zip, name) {
  let at = 0;
  while (at + 30 <= zip.length && zip.readUInt32LE(at) === 0x0403_4b50) {
    const method = zip.readUInt16LE(at + 8);
    const compressed = zip.readUInt32LE(at + 18);
    const nameLength = zip.readUInt16LE(at + 26);
    const extraLength = zip.readUInt16LE(at + 28);
    const entry = zip.subarray(at + 30, at + 30 + nameLength).toString("utf8");
    const dataAt = at + 30 + nameLength + extraLength;
    const data = zip.subarray(dataAt, dataAt + compressed);
    if (entry === name) {
      return (method === 8 ? inflateRawSync(data) : data).toString("utf8");
    }
    at = dataAt + compressed;
  }
  return null;
}

async function waitForOpenedDocument(page, name) {
  await expect(page.locator("#docTitle")).toHaveValue(name);
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, {
    timeout: 45_000,
  });
  await expectSaveEnabled(page);
}

test("browser Open and Save dispatch text through the generic ODT exporter", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles({
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("Alpha\nBeta\n", "utf8"),
  });
  await waitForOpenedDocument(page, "notes.txt");

  // `#saveFormat` is no longer a control the user operates — Save keeps the
  // source format, the way Word's Save does, and CHANGING format is File ▸
  // Export as <format>, which is Word's Save As. The select survives as the
  // state holder Save reads, so it is still the honest place to assert which
  // format a round trip landed on, and which exporters are registered.
  const format = page.locator("#saveFormat");
  await expect(format).toHaveValue(TEXT);
  // PDF leads the list: the builtin registry now registers the export-only
  // real-text PDF adapter (`docs/98`, HF-030), and `application.pdf` sorts
  // first on id. It has no importer, so it never appears as something the
  // picker offers to open — which the Rust side asserts separately.
  // EVERY option is a NAME. This line used to read `text.markdown`,
  // `text.html` and `org.openxmlformats.wordprocessingml.template`, because
  // three export-only writers shipped in the engine and nothing named them —
  // the owner's report was "i dont see export of other formats", and they were
  // right: a capability that ships with no name is a capability that did not
  // arrive. The names now come out of the string table
  // (`en_strings.mjs` `format.*`) through the format catalogue, so this list is
  // also the guard that a format cannot lose its name again: a missing key
  // would render as the dotted key and a missing catalogue entry as the raw id,
  // and both fail here.
  //
  // Order is the ENGINE's — `availableExportFormats()` sorts on format id — so
  // it is deliberately not the File menu's order. RTF is absent because RTF has
  // no writer at all (`can_export: false`), which is what the disabled File row
  // below says out loud.
  await expect(format.locator("option")).toHaveText([
    "PDF",
    "Normalized JSON",
    "ODT",
    "DOCX",
    "Word Template",
    "Web Page",
    "Markdown",
    "Plain text",
  ]);

  const downloadPromise = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.odt");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("notes.odt");
  const path = await download.path();
  const bytes = await readFile(path);
  expect(bytes.subarray(0, 2).toString()).toBe("PK");

  // Drive the product command, not WasmDocument directly: this is the guard
  // that the registered real-text exporter is actually reachable from File.
  const pdfDownloadPromise = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.pdf");
  const pdfDownload = await pdfDownloadPromise;
  expect(pdfDownload.suggestedFilename()).toBe("notes.pdf");
  const pdfPath = await pdfDownload.path();
  const pdfBytes = await readFile(pdfPath);
  expect(pdfBytes.subarray(0, 8).toString()).toBe("%PDF-1.7");
  const pdfText = pdfBytes.toString("latin1");
  expect(pdfText).toContain("/Subtype/Type0");
  expect(pdfText).toContain("/ToUnicode");

  await page.locator("#file").setInputFiles({
    name: "roundtrip.odt",
    mimeType: "application/vnd.oasis.opendocument.text",
    buffer: bytes,
  });
  await waitForOpenedDocument(page, "roundtrip.odt");
  await expect(format).toHaveValue(ODT);
  expect(consoleErrors).toEqual([]);
});

test("cross-format browser Save visibly reports compatibility findings", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const downloadPromise = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.odt");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("opendoc-demo.odt");
  await expect(page.locator("#compatibilityStatus")).toBeVisible();
  await expect(page.locator("#compatibilityStatus")).toContainText(
    "export finding",
  );
  await expect(page.locator("#status")).toContainText("compatibility finding");
  expect(consoleErrors).toEqual([]);
});

// ── The three writers that shipped with no way to reach them ────────────────
//
// Markdown, single-file HTML and the Word template all landed in the engine,
// were registered, worked — and had NO command, so `File ▸ Export` listed six
// formats and the Save-as picker listed these three by their raw ids. `SKILL` §9
// rule 4: "built" is not "reachable", and this is the guard that says which one
// this is.
//
// It asserts the EFFECT, not the row. A test that found `file.export.markdown`
// in a menu would pass over an exporter that wrote an empty file, so each of the
// three is driven and its bytes are read: a Markdown file that carries the
// document's own text, an HTML file that is really self-contained, and a template
// package whose content type is really the template one and not the document one.
test("the three unnamed export-only writers are reachable from File and write what they claim", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?blank=1");
  await expect(page.locator("#file")).toBeEnabled();
  await page.locator("#file").setInputFiles({
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("Alpha\nBeta\n", "utf8"),
  });
  await waitForOpenedDocument(page, "notes.txt");

  const markdown = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.markdown");
  const md = await markdown;
  expect(md.suggestedFilename()).toBe("notes.md");
  const mdText = (await readFile(await md.path())).toString("utf8");
  expect(mdText).toContain("Alpha");
  expect(mdText).toContain("Beta");

  const html = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.html");
  const web = await html;
  expect(web.suggestedFilename()).toBe("notes.html");
  const htmlText = (await readFile(await web.path())).toString("utf8");
  expect(htmlText).toMatch(/<html[\s>]/i);
  expect(htmlText).toContain("Alpha");
  // SELF-CONTAINED is the promise the format makes ("single-file HTML5"), and it
  // is the one a local-first product cannot break: a page that fetched a
  // stylesheet or an image from the network would render differently, or not at
  // all, for the reader who opened it offline. Asserted as the absence of any
  // absolute URL rather than of a `<link>`, because a remote `@import` or a
  // `background: url(https://…)` would be the same defect by another route.
  expect(htmlText).not.toMatch(/https?:\/\//);

  const template = page.waitForEvent("download");
  await runAppMenuCommand(page, "file", "file.export.dotx");
  const dotx = await template;
  expect(dotx.suggestedFilename()).toBe("notes.dotx");
  const dotxBytes = await readFile(await dotx.path());
  expect(dotxBytes.subarray(0, 2).toString()).toBe("PK");
  // A `.dotx` is a `.docx` with ONE content type changed, so the whole of what
  // makes it a template is this string — and an exporter that shipped the
  // document content type under a `.dotx` name would produce a file Word opens
  // as an ordinary document. That is the defect this line exists for, and it is
  // why the part is inflated rather than searched for in the raw bytes.
  const contentTypes = opcPart(dotxBytes, "[Content_Types].xml");
  expect(contentTypes, "the package has no [Content_Types].xml").not.toBeNull();
  expect(contentTypes).toContain("wordprocessingml.template.main+xml");
  expect(contentTypes).not.toContain("wordprocessingml.document.main+xml");

  expect(consoleErrors).toEqual([]);
});

// Never a dead control, and never a control that throws instead (`SKILL` §10).
//
// RTF is the live case of a format the chrome can NAME and the engine cannot
// WRITE: `crates/casual-doc-io/src/rtf.rs` declares `can_export: false` and
// registers no writer, so `file.export.rtf` used to run an export that threw. It
// is now disabled carrying a reason that names the format, and the reason is
// deliberately NOT the host-capability one — "this cannot be done" and "not for
// you" are different answers and a reader deserves the right one.
test("an export whose format has no writer is disabled carrying the reason", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await useCompactChrome(page);
  const rtf = await menuCommandRow(page, "file", "file.export.rtf");
  await expect(rtf, "file.export.rtf must not offer an export the engine cannot perform").toBeDisabled();
  await expect(rtf).toHaveAttribute("title", /cannot write/i);
  await expect(rtf).toHaveAttribute("title", /Rich Text Format/);
  // The formats that CAN be written are not collateral damage: this guard has to
  // be able to tell "this one format has no writer" from "every export is off",
  // and without this line it could not.
  const odt = await menuCommandRow(page, "file", "file.export.odt");
  await expect(odt, "ODT has a writer and must stay enabled").toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

// The asymmetry, held from the webapp side.
//
// All three new formats are EXPORT ONLY, and the Open picker's `accept` list is a
// separate hand-written string from the engine's importer registry — so the two
// can drift, and the drift that matters is offering to open a file this build
// cannot read. A `.md` in the Open dialog is a file chooser that accepts a
// document and then refuses it, which is worse than not offering it.
test("the Open picker never offers a format the engine cannot import", async ({ page }) => {
  await gotoEditor(page);
  const accept = await page.locator("#file").getAttribute("accept");
  for (const extension of [".md", ".markdown", ".html", ".htm", ".dotx"]) {
    expect(accept, `${extension} has no importer and must not be offered to open`).not.toContain(
      extension,
    );
  }
  expect(accept, "the formats that DO import must still be offered").toContain(".docx");
  expect(accept).toContain(".odt");
  expect(accept).toContain(".rtf");
});
