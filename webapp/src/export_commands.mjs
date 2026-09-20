// The File ▸ Export commands, as data.
//
// One row per format the engine can write. They were five near-identical
// literals in `main.js`, which meant adding a format touched the 18k-line file
// and the only thing that varied between the lines was a string — exactly the
// shape that belongs in a table. Extracted when PDF became the fifth
// (`docs/109` HF-030), so the file did not grow to gain a format.
//
// Order is the order they appear in the File menu, and it is deliberate:
// PDF and DOCX are what people actually ask for, so they come first;
// normalized JSON is a debugging artifact and comes last.
//
// The format ids are the engine's own, from `casual_doc_io::formats`. The
// menu taxonomy in `command_taxonomy.mjs` lists the same ids, and
// `menu_taxonomy.test.mjs` asserts the two agree — so a row added here
// without a home in the File menu fails the build rather than becoming a
// command reachable only from the palette.
export const EXPORT_COMMANDS = Object.freeze([
  Object.freeze({
    id: "file.export.pdf",
    label: "Export as PDF…",
    kw: "export save as pdf acrobat",
    // Real-text PDF, not the 150-DPI raster `file.print` builds for physical
    // printing (HF-030). Deliberately separate commands: a printer wants a
    // rendered page, a saved PDF wants selectable, searchable text.
    format: "application.pdf",
  }),
  Object.freeze({
    id: "file.export.docx",
    label: "Export as DOCX…",
    kw: "export save as word",
    format: "org.openxmlformats.wordprocessingml.document",
  }),
  Object.freeze({
    id: "file.export.odt",
    label: "Export as ODT…",
    kw: "export save as opendocument",
    format: "org.oasis.opendocument.text",
  }),
  Object.freeze({
    id: "file.export.text",
    label: "Export as Plain text…",
    kw: "export save as txt",
    format: "text.plain",
  }),
  Object.freeze({
    id: "file.export.json",
    label: "Export as Normalized JSON…",
    kw: "export save as json",
    format: "org.casualoffice.normalized-json",
  }),
]);

/**
 * The export rows as command descriptors.
 *
 * @param {(format: string) => unknown} exportAs how to run one export.
 * @returns {Array<object>} descriptors in File-menu order.
 */
export function exportCommands(exportAs) {
  return EXPORT_COMMANDS.map(({ id, label, kw, format }) => ({
    id,
    label,
    group: "File",
    kw,
    run: () => exportAs(format),
  }));
}
