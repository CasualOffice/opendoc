import assert from "node:assert/strict";
import test from "node:test";

import {
  compatibilityOccurrenceCount,
  downloadNameForFormat,
  ensureDocumentExtension,
  formatInfo,
} from "../src/format_io.mjs";

test("format catalog exposes stable labels and extensions", () => {
  assert.deepEqual(formatInfo("org.oasis.opendocument.text"), {
    label: "ODT",
    extension: "odt",
  });
  assert.deepEqual(formatInfo("text.plain"), {
    label: "Plain text",
    extension: "txt",
  });
});

test("document names keep recognized extensions and switch export suffixes", () => {
  assert.equal(ensureDocumentExtension("Notes", "odt"), "Notes.odt");
  assert.equal(ensureDocumentExtension("Notes.TXT", "odt"), "Notes.TXT");
  assert.equal(downloadNameForFormat("Notes.docx", "odt"), "Notes.odt");
  assert.equal(downloadNameForFormat("Notes", "json"), "Notes.json");
});

test("compatibility report counts occurrences rather than only buckets", () => {
  assert.equal(
    compatibilityOccurrenceCount(
      JSON.stringify({
        entries: [
          { feature: "one", occurrences: 2 },
          { feature: "two", occurrences: 3 },
        ],
      }),
    ),
    5,
  );
  assert.throws(() => compatibilityOccurrenceCount("{}"), /entries array/);
});

// RTF is import-only today, so it never appears in the Save-format picker and
// `formatInfo`'s label is easy to leave out without anything looking broken.
// Both halves still matter, and each fails differently:
//
//  - without the FORMAT_CATALOG entry the chrome falls back to `label: formatId`
//    and shows the user the raw string `application.rtf`;
//  - without `rtf` in DOCUMENT_EXTENSION, saving a document opened from
//    `report.rtf` produces `report.rtf.docx` rather than `report.docx`, because
//    the name is only rewritten when it already ends in a known extension.
//
// The second is the one an end-to-end test could not see: it is a filename, not
// a rendered surface, which is why it is asserted here rather than in the
// browser spec that covers reaching the importer at all.
test("RTF is a named format, and a .rtf name is rewritten rather than appended to", () => {
  assert.deepEqual(formatInfo("application.rtf"), {
    label: "Rich Text Format",
    extension: "rtf",
  });
  assert.equal(downloadNameForFormat("report.rtf", "docx"), "report.docx");
  assert.equal(downloadNameForFormat("report.rtf", "odt"), "report.odt");
  assert.equal(ensureDocumentExtension("report.rtf", "docx"), "report.rtf");
});
