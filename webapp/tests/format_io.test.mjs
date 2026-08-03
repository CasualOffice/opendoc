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
