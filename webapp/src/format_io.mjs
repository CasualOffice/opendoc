// The `mime` beside each label is the MEDIA TYPE THE ENGINE ITSELF WRITES on the
// artifact it hands back (`ExportArtifact::mime_type`), copied here for the one
// caller that has bytes but no artifact: **a version downloaded out of the
// checkpoint store** (`docs/139` VH-007). Every other download comes straight
// from an export and uses `artifact.mimeType`, which is why this column did not
// exist before — a stored checkpoint is bytes plus a format id, and the format →
// media type answer had nowhere to live. `docs/139` named exactly that as what
// VH-007 was waiting on.
//
// Copied, therefore GUARDED: `format_io.test.mjs` reads the six `const *_MIME`
// declarations out of the Rust sources and fails if this table and the engine
// ever disagree, in either direction. A media type typed into JavaScript and left
// to drift is how a saved DOCX comes back as `application/octet-stream`.
export const FORMAT_CATALOG = Object.freeze({
  // Export only — the engine registers no PDF importer, so this never appears
  // as something the picker offers to open.
  "application.pdf": Object.freeze({
    label: "PDF",
    extension: "pdf",
    mime: "application/pdf",
  }),
  "org.openxmlformats.wordprocessingml.document": Object.freeze({
    label: "DOCX",
    extension: "docx",
    mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  }),
  "org.oasis.opendocument.text": Object.freeze({
    label: "ODT",
    extension: "odt",
    mime: "application/vnd.oasis.opendocument.text",
  }),
  "application.rtf": Object.freeze({
    label: "Rich Text Format",
    extension: "rtf",
    mime: "application/rtf",
  }),
  "org.casualoffice.normalized-json": Object.freeze({
    label: "Normalized JSON",
    extension: "json",
    mime: "application/vnd.casualoffice.document+json",
  }),
  "text.plain": Object.freeze({
    label: "Plain text",
    extension: "txt",
    mime: "text/plain",
  }),
});

const DOCUMENT_EXTENSION = /\.(docx|odt|rtf|pdf|json|txt)$/i;

export function formatInfo(formatId) {
  return (
    FORMAT_CATALOG[formatId] ?? {
      label: formatId,
      extension: "document",
      // `application/octet-stream` is what a browser is told when nothing here
      // knows better, and it is deliberately NOT silent: `isKnownFormat` below is how
      // a caller asks whether the answer is real, so a download of an
      // unrecognised format can SAY that rather than quietly offering
      // `report.document` and letting the reader find out.
      mime: "application/octet-stream",
    }
  );
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
