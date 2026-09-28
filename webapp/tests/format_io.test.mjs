import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  FORMAT_CATALOG,
  compatibilityOccurrenceCount,
  downloadNameForFormat,
  ensureDocumentExtension,
  formatInfo,
  isKnownFormat,
} from "../src/format_io.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

test("format catalog exposes stable labels and extensions", () => {
  assert.deepEqual(formatInfo("org.oasis.opendocument.text"), {
    label: "ODT",
    extension: "odt",
    mime: "application/vnd.oasis.opendocument.text",
  });
  assert.deepEqual(formatInfo("text.plain"), {
    label: "Plain text",
    extension: "txt",
    mime: "text/plain",
  });
});

// The media types in `FORMAT_CATALOG` are a copy of the engine's, kept for the
// one caller that has bytes and no artifact: a version downloaded out of the
// checkpoint store (`docs/139` VH-007). A copy drifts, so this reads the
// originals out of the Rust and fails in BOTH directions — a value changed here
// and a value changed there.
//
// The Rust is the authority and this is the copy, which is why the assertion is
// written as "every media type the engine declares appears here against the
// right format id" rather than as a literal table repeated a third time.
const RUST_MIME = Object.freeze({
  "application.pdf": ["crates/casual-doc-io/src/pdf.rs", "PDF_MIME"],
  "org.openxmlformats.wordprocessingml.document": ["crates/casual-doc-io/src/docx.rs", "DOCX_MIME"],
  "org.casualoffice.normalized-json": ["crates/casual-doc-io/src/normalized_json.rs", "JSON_MIME"],
  "text.plain": ["crates/casual-doc-io/src/text.rs", "TEXT_MIME"],
  "org.oasis.opendocument.text": ["crates/casual-doc-odf/src/package.rs", "ODT_MIME"],
  "application.rtf": ["crates/casual-doc-rtf/src/lib.rs", "RTF_MIME"],
});

test("every format's media type is the one the engine writes on its artifact", () => {
  const drift = [];
  for (const [formatId, [path, constant]] of Object.entries(RUST_MIME)) {
    const source = readFileSync(join(REPO, path), "utf8");
    const declared = new RegExp(
      `(?:pub )?const ${constant}: &str = "([^"]+)"`,
    ).exec(source);
    assert.ok(declared, `${path} no longer declares ${constant}`);
    const ours = FORMAT_CATALOG[formatId]?.mime;
    if (ours !== declared[1]) drift.push(`${formatId}: webapp ${ours} vs ${path} ${declared[1]}`);
  }
  assert.deepEqual(
    drift,
    [],
    "a media type typed into JavaScript and left to drift is how a saved DOCX " +
      "comes back as application/octet-stream",
  );
  // Both directions: a format added to the catalogue without a media type, or
  // without a Rust constant to check it against, is caught here rather than by a
  // reader whose download opened in the wrong application.
  assert.deepEqual(
    Object.keys(FORMAT_CATALOG).filter((id) => !RUST_MIME[id]),
    [],
    "a format in FORMAT_CATALOG with no engine constant to check its media type against",
  );
  for (const [id, entry] of Object.entries(FORMAT_CATALOG)) {
    assert.ok(entry.mime, `${id} has no media type`);
  }
});

test("an unrecognised format still answers, and says that the answer is a fallback", () => {
  // A checkpoint written by an older build can name a format this one has never
  // heard of. It must still be downloadable — the bytes are the reader's — and
  // the caller must be able to SAY that the name and the media type are guesses
  // rather than offering `report.document` silently.
  assert.equal(isKnownFormat("text.plain"), true);
  assert.equal(isKnownFormat("org.example.formatFromTheFuture"), false);
  assert.deepEqual(formatInfo("org.example.formatFromTheFuture"), {
    label: "org.example.formatFromTheFuture",
    extension: "document",
    mime: "application/octet-stream",
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
    mime: "application/rtf",
  });
  assert.equal(downloadNameForFormat("report.rtf", "docx"), "report.docx");
  assert.equal(downloadNameForFormat("report.rtf", "odt"), "report.odt");
  assert.equal(ensureDocumentExtension("report.rtf", "docx"), "report.rtf");
});
