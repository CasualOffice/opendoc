export const FORMAT_CATALOG = Object.freeze({
  // Export only — the engine registers no PDF importer, so this never appears
  // as something the picker offers to open.
  "application.pdf": Object.freeze({
    label: "PDF",
    extension: "pdf",
  }),
  "org.openxmlformats.wordprocessingml.document": Object.freeze({
    label: "DOCX",
    extension: "docx",
  }),
  "org.oasis.opendocument.text": Object.freeze({
    label: "ODT",
    extension: "odt",
  }),
  "application.rtf": Object.freeze({
    label: "Rich Text Format",
    extension: "rtf",
  }),
  "org.casualoffice.normalized-json": Object.freeze({
    label: "Normalized JSON",
    extension: "json",
  }),
  "text.plain": Object.freeze({
    label: "Plain text",
    extension: "txt",
  }),
});

const DOCUMENT_EXTENSION = /\.(docx|odt|rtf|pdf|json|txt)$/i;

export function formatInfo(formatId) {
  return (
    FORMAT_CATALOG[formatId] ?? {
      label: formatId,
      extension: "document",
    }
  );
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
