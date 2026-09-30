import { readFile } from "node:fs/promises";

import {
  test,
  expect,
  gotoEditor,
  expectSaveEnabled,
  saveDocument,
  runAppMenuCommand,
} from "./fixtures.mjs";

const ODT = "org.oasis.opendocument.text";
const TEXT = "text.plain";

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
  // Three export-only formats joined this list. They are NOT yet named: with no
  // `FORMAT_CATALOG` entry the picker falls back to the raw format id, and this
  // asserts that unflattering truth rather than hiding it, because the day they
  // are named this line has to be the thing that changes.
  //
  // Naming them is not a one-line edit, which is why it is a follow-up and not
  // smuggled in here: `format_io.mjs` carries an unrouted-strings CEILING of 6,
  // every label in it is an untranslated literal, and a ceiling is paid down, not
  // raised. Doing it properly means three keys in the string table and a
  // translation in each of the eighteen locales, with the picker looking the
  // label up instead of reading it out of a frozen data table.
  await expect(format.locator("option")).toHaveText([
    "PDF",
    "Normalized JSON",
    "ODT",
    "DOCX",
    "org.openxmlformats.wordprocessingml.template",
    "text.html",
    "text.markdown",
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
