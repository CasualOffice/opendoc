// Which formats this chrome can NAME, what a file of each is called on disk,
// and what media type the engine writes on its bytes.
//
// The name is a `labelKey`, not a literal. It used to be English sitting in this
// frozen table, which made every format name untranslatable in eighteen
// languages and made ADDING a format cost an unrouted-strings ceiling — which is
// why Markdown, HTML and the Word template shipped in the engine and reached the
// Save picker as the raw strings `text.markdown` and
// `org.openxmlformats.wordprocessingml.template`. A capability that ships with no
// name is a capability that did not arrive, so the name moved to where every
// other user-facing string in the editor lives (`en_strings.mjs`, `docs/124`) and
// is resolved through `t()` at RENDER time. Reading it at render also means a
// locale change repaints the picker into the new language without this table
// being rebuilt.
//
// ONE name per format, read by every surface: the Save-as picker
// (`save_formats.mjs`), the File page's Export tiles (`file_pane.mjs`), the File
// menu's `Export as …` rows (`export_commands.mjs`) and the version-history
// download. Word and Google Docs both name a format once and reuse it across
// Save As, Download and the file-type chip; two spellings of one format is how a
// menu starts reading as a different product from the picker beside it.
//
// The `mime` beside each name is the MEDIA TYPE THE ENGINE ITSELF WRITES on the
// artifact it hands back (`ExportArtifact::mime_type`), copied here for the one
// caller that has bytes but no artifact: **a version downloaded out of the
// checkpoint store** (`docs/139` VH-007). Every other download comes straight
// from an export and uses `artifact.mimeType`, which is why this column did not
// exist before — a stored checkpoint is bytes plus a format id, and the format →
// media type answer had nowhere to live. `docs/139` named exactly that as what
// VH-007 was waiting on.
//
// Copied, therefore GUARDED: `format_io.test.mjs` reads the `const *_MIME`
// declarations out of the Rust sources and fails if this table and the engine
// ever disagree, in either direction. A media type typed into JavaScript and left
// to drift is how a saved DOCX comes back as `application/octet-stream`.
import { t } from "./i18n.mjs";

export const FORMAT_CATALOG = Object.freeze({
  // Export only — the engine registers no PDF importer, so this never appears
  // as something the picker offers to open.
  "application.pdf": Object.freeze({
    labelKey: "format.pdf",
    extension: "pdf",
    mime: "application/pdf",
  }),
  "org.openxmlformats.wordprocessingml.document": Object.freeze({
    labelKey: "format.docx",
    extension: "docx",
    mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  }),
  "org.oasis.opendocument.text": Object.freeze({
    labelKey: "format.odt",
    extension: "odt",
    mime: "application/vnd.oasis.opendocument.text",
  }),
  // IMPORT only: the engine's descriptor says `can_export: false` and there is
  // no RTF writer, so this id is absent from `availableExportFormats()` and the
  // File menu's row for it is disabled carrying that as its reason. It is named
  // here all the same, because a document opened FROM a `.rtf` is described by
  // its format on the File page and in version history whether or not it can be
  // written back.
  "application.rtf": Object.freeze({
    labelKey: "format.rtf",
    extension: "rtf",
    mime: "application/rtf",
  }),
  // The three that shipped in the engine with no name. All three are EXPORT
  // ONLY and the engine's descriptors say so (`can_import: false`): writing
  // Markdown or HTML is a serializer and reading either needs a parser, and a
  // `.dotx` would import perfectly well as a document but deliberately does
  // not. So none of them may ever appear in the Open picker, which is what
  // `multi-format-io.spec.mjs` holds.
  "text.markdown": Object.freeze({
    labelKey: "format.markdown",
    extension: "md",
    mime: "text/markdown",
  }),
  // "Web Page", which is the engine descriptor's own display name and the word
  // Word, LibreOffice and Google Docs all use in a Save-as list. Deliberately
  // not "HTML": the name a person picks there is what they want to end up with,
  // not the markup language it happens to be written in.
  "text.html": Object.freeze({
    labelKey: "format.html",
    extension: "html",
    mime: "text/html",
  }),
  "org.openxmlformats.wordprocessingml.template": Object.freeze({
    labelKey: "format.dotx",
    extension: "dotx",
    mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
  }),
  "org.casualoffice.normalized-json": Object.freeze({
    labelKey: "format.json",
    extension: "json",
    mime: "application/vnd.casualoffice.document+json",
  }),
  "text.plain": Object.freeze({
    labelKey: "format.text",
    extension: "txt",
    mime: "text/plain",
  }),
});

// Every extension the table above can produce, plus the import-only spellings a
// file can arrive under. It is what tells `downloadNameForFormat` that
// `report.md` becomes `report.docx` rather than `report.md.docx`.
const DOCUMENT_EXTENSION = /\.(docx|dotx|odt|rtf|pdf|json|txt|md|markdown|html|htm)$/i;

/** A format's NAME in the active locale, or the raw id when this build cannot
 *  name it.
 *
 *  The raw id is deliberately ugly and deliberately not routed: it is the one
 *  honest answer for a checkpoint written by a newer build, and it is visible in
 *  a screenshot. `isKnownFormat` is how a caller tells the two apart and says so.
 *
 *  THREE ways a name can go missing, and all three end at the id rather than at
 *  something worse. No catalogue entry is the original case. A catalogue entry
 *  with no `labelKey` would otherwise hand `t()` `undefined` and put an EMPTY
 *  option in the Save picker — a row with no name at all, which is less
 *  recoverable than a row named after its format, and is what the mutation proof
 *  for this change actually produced before this branch existed. A `labelKey`
 *  pointing at a key no catalogue answers makes `t()` return the key, so without
 *  the last comparison the picker would read `format.markdown`; a dotted key on
 *  screen tells a reader nothing and tells a bug report nothing either.
 *
 *  This is a DEGRADATION path, not a substitute for the guard:
 *  `format_io.test.mjs` fails the build for all three, because falling back
 *  gracefully to an id is still shipping a format with no name.
 *
 *  Complexity: O(1), one catalogue lookup. Called per render, never per
 *  keystroke. */
export function formatLabel(formatId) {
  const key = FORMAT_CATALOG[formatId]?.labelKey;
  if (!key) return formatId;
  const named = t(key);
  return named === key ? formatId : named;
}

/** `{ label, extension, mime }` for a format id.
 *
 *  `label` is resolved through `t()` on every call rather than stored, so the
 *  picker, the Export tiles and the File menu all read one name and a locale
 *  change is a repaint rather than a rebuild. */
export function formatInfo(formatId) {
  const entry = FORMAT_CATALOG[formatId];
  if (entry) {
    return { label: formatLabel(formatId), extension: entry.extension, mime: entry.mime };
  }
  return {
    label: formatId,
    extension: "document",
    // `application/octet-stream` is what a browser is told when nothing here
    // knows better, and it is deliberately NOT silent: `isKnownFormat` below is how
    // a caller asks whether the answer is real, so a download of an
    // unrecognised format can SAY that rather than quietly offering
    // `report.document` and letting the reader find out.
    mime: "application/octet-stream",
  };
}

/** Whether this build recognises a format id at all.
 *
 *  A checkpoint written by an older build can name a format this one has never
 *  heard of. `formatInfo` still answers — it has to, or the file could not be
 *  handed over at all — and this is how the caller knows the answer is a
 *  fallback and reports it (`docs/139` VH-007: a download must not silently
 *  differ from what the preview showed). O(1). */
export function isKnownFormat(formatId) {
  return Object.hasOwn(FORMAT_CATALOG, formatId);
}

export function ensureDocumentExtension(name, fallbackExtension) {
  return DOCUMENT_EXTENSION.test(name) ? name : `${name}.${fallbackExtension}`;
}

export function downloadNameForFormat(name, extension) {
  return DOCUMENT_EXTENSION.test(name)
    ? name.replace(DOCUMENT_EXTENSION, `.${extension}`)
    : `${name}.${extension}`;
}

export function compatibilityOccurrenceCount(reportJson) {
  const report = JSON.parse(reportJson);
  if (!report || !Array.isArray(report.entries)) {
    throw new Error("compatibility report has no entries array");
  }
  return report.entries.reduce((total, entry) => {
    const occurrences = Number(entry?.occurrences);
    return (
      total +
      (Number.isSafeInteger(occurrences) && occurrences > 0 ? occurrences : 0)
    );
  }, 0);
}

/** Occurrences in an IMPORT report, which may legitimately not exist.
 *
 *  A blank document was never imported, and some formats report nothing, so the
 *  engine hands back an empty string rather than an empty report. That is not an
 *  error and must not stop a document opening. A report that exists but is
 *  malformed still throws, because that is a real defect and silently reporting
 *  "0 findings" would be the worst possible answer — it claims a clean import.
 */
export function importFindingCount(reportJson) {
  if (!reportJson) return 0;
  return compatibilityOccurrenceCount(reportJson);
}
