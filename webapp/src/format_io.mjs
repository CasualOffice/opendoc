export const FORMAT_CATALOG = Object.freeze({
  "org.openxmlformats.wordprocessingml.document": Object.freeze({
    label: "DOCX",
    extension: "docx",
  }),
  "org.oasis.opendocument.text": Object.freeze({
    label: "ODT",
    extension: "odt",
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

const DOCUMENT_EXTENSION = /\.(docx|odt|json|txt)$/i;

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
