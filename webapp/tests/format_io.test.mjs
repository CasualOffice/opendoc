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
  formatLabel,
  isKnownFormat,
} from "../src/format_io.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

// A format's name comes out of the catalogue now, so these tests need one
// installed — the same `EN_STRINGS` the extractor builds `locales/en.json` from.
setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

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

// THE DEFECT THIS FILE EXISTS TO PREVENT, now that the name is a key.
//
// Markdown, HTML and the Word template shipped in the engine, were registered,
// worked, appeared in the Save-as picker — and were labelled `text.markdown` and
// `org.openxmlformats.wordprocessingml.template`, because nothing named them.
// The owner's report was "i dont see export of other formats", and that is
// exactly right: a capability that ships with no name is a capability that did
// not arrive (`SKILL` §9 rule 4 — "built" is not "reachable").
//
// So the assertion is BOTH directions. Every format the chrome can name really
// resolves to a name — a `labelKey` pointing at a key no catalogue answers would
// put the dotted key itself on screen, which is worse than the raw id — and no
// format is left without one.
test("every format in the catalogue resolves to a real name, not a key or an id", () => {
  const unnamed = [];
  for (const [formatId, entry] of Object.entries(FORMAT_CATALOG)) {
    if (!entry.labelKey) unnamed.push(`${formatId} declares no labelKey`);
    else if (!Object.hasOwn(EN_STRINGS, entry.labelKey)) {
      unnamed.push(`${formatId}: ${entry.labelKey} is in no string table`);
    } else if (formatLabel(formatId) === entry.labelKey || formatLabel(formatId) === formatId) {
      unnamed.push(`${formatId} renders as its ${entry.labelKey === formatLabel(formatId) ? "key" : "id"}`);
    }
  }
  assert.deepEqual(unnamed, []);
  // The three that had no name at all, asserted by value: this is the line that
  // changes the day one of them is renamed, and the line that fails if a name is
  // dropped again.
  assert.equal(formatLabel("text.markdown"), "Markdown");
  assert.equal(formatLabel("text.html"), "Web Page");
  assert.equal(formatLabel("org.openxmlformats.wordprocessingml.template"), "Word Template");
});

// How a MISSING name degrades, which is a different question from whether one is
// missing (the guard above) and has to be asked separately.
//
// The mutation proof for this change dropped `text.markdown`'s `labelKey` and the
// Save picker rendered an EMPTY option — `t(undefined)` has no answer, so the
// dropdown offered a blank row. That is strictly worse than the raw id it
// replaced: a reader can report "it says text.markdown" and cannot report a row
// that is not there. Both failure shapes now end at the id.
test("a format whose name cannot be resolved falls back to its id, never to a blank", () => {
  const catalogue = { ...EN_STRINGS };
  delete catalogue["format.markdown"];
  setCatalogue("en", catalogue);
  try {
    // The key is declared on the entry and answered by nothing: `t()` returns the
    // key, and a dotted key on screen helps nobody.
    assert.equal(formatLabel("text.markdown"), "text.markdown");
    assert.equal(formatInfo("text.markdown").label, "text.markdown");
    assert.notEqual(formatLabel("text.markdown"), "");
    // A format this build has never heard of, which is the original case and must
    // keep working: a checkpoint written by a newer build is still downloadable.
    assert.equal(formatLabel("org.example.formatFromTheFuture"), "org.example.formatFromTheFuture");
  } finally {
    setCatalogue("en", { ...EN_STRINGS });
  }
});

// An export-only format still needs its extension to behave like every other
// one, and that is a filename rather than a rendered surface — which is why it
// is asserted here and not in the browser spec.
test("the three export-only formats name their files properly", () => {
  assert.equal(downloadNameForFormat("notes.txt", "md"), "notes.md");
  assert.equal(downloadNameForFormat("notes.md", "docx"), "notes.docx");
  assert.equal(downloadNameForFormat("notes.txt", "html"), "notes.html");
  assert.equal(downloadNameForFormat("notes.html", "docx"), "notes.docx");
  assert.equal(downloadNameForFormat("notes.docx", "dotx"), "notes.dotx");
  assert.equal(downloadNameForFormat("notes.dotx", "docx"), "notes.docx");
  assert.equal(ensureDocumentExtension("notes.md", "docx"), "notes.md");
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
  "org.openxmlformats.wordprocessingml.template": ["crates/casual-doc-io/src/dotx.rs", "DOTX_MIME"],
  "org.casualoffice.normalized-json": ["crates/casual-doc-io/src/normalized_json.rs", "JSON_MIME"],
  "text.markdown": ["crates/casual-doc-io/src/markdown.rs", "MARKDOWN_MIME"],
  "text.html": ["crates/casual-doc-io/src/html/mod.rs", "HTML_MIME"],
  "text.plain": ["crates/casual-doc-io/src/text.rs", "TEXT_MIME"],
  "org.oasis.opendocument.text": ["crates/casual-doc-odf/src/package.rs", "ODT_MIME"],
  "application.rtf": ["crates/casual-doc-rtf/src/lib.rs", "RTF_MIME"],
});

test("every format's media type is the one the engine writes on its artifact", () => {
  const drift = [];
  for (const [formatId, [path, constant]] of Object.entries(RUST_MIME)) {
    const source = readFileSync(join(REPO, path), "utf8");
    // `\s*` after the `=`, because a long media type is wrapped onto the next
    // line by rustfmt — `DOTX_MIME` is, and a regex that required the value on
    // the same line would have reported the engine as no longer declaring the
    // constant at all. A guard that cannot read half its own inputs is a guard
    // that passes for the wrong reason.
    const declared = new RegExp(
      `(?:pub )?const ${constant}: &str =\\s*"([^"]+)"`,
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
  assert.equal(formatLabel("application.rtf"), "Rich Text Format");
  assert.equal(downloadNameForFormat("report.rtf", "docx"), "report.docx");
  assert.equal(downloadNameForFormat("report.rtf", "odt"), "report.odt");
  assert.equal(ensureDocumentExtension("report.rtf", "docx"), "report.rtf");
});
