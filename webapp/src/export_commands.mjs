// The File ▸ Export commands, as data.
//
// One row per format the engine can write. They were five near-identical
// literals in `main.js`, which meant adding a format touched the 18k-line file
// and the only thing that varied between the lines was a string — exactly the
// shape that belongs in a table. Extracted when PDF became the fifth
// (`docs/109` HF-030), so the file did not grow to gain a format.
//
// Order is the order they appear in the File menu, and it is deliberate:
// PDF and DOCX are what people actually ask for, so they come first; the
// interchange formats follow; normalized JSON is a debugging artifact and comes
// last.
//
// The format ids are the engine's own, from `casual_doc_io::formats`. The
// menu taxonomy in `command_taxonomy.mjs` lists the same ids, and
// `menu_taxonomy.test.mjs` asserts the two agree — so a row added here
// without a home in the File menu fails the build rather than becoming a
// command reachable only from the palette.
//
// A ROW CARRIES NO ENGLISH. Each one used to name its format in English on the
// row itself, which put nine untranslated sentences in a frozen table and made
// every new format cost an unrouted-strings ceiling. The name is now
// composed at registry-build time from ONE pattern (`filePane.export.as`) and
// the ONE name the format catalogue holds (`format_io.mjs`), so the File menu
// row, the File page's Export tile and the Save-as picker cannot name a format
// three different things. This is Word's and Google Docs' shape too: the
// format is named once and the surface supplies the verb.
import { formatLabel } from "./format_io.mjs";
import { t } from "./i18n.mjs";

export const EXPORT_COMMANDS = Object.freeze([
  Object.freeze({
    id: "file.export.pdf",
    icon: "picture_as_pdf",
    kw: "export save as pdf acrobat",
    // Real-text PDF, not the 150-DPI raster `file.print` builds for physical
    // printing (HF-030). Deliberately separate commands: a printer wants a
    // rendered page, a saved PDF wants selectable, searchable text.
    format: "application.pdf",
  }),
  Object.freeze({
    id: "file.export.docx",
    icon: "description",
    kw: "export save as word",
    format: "org.openxmlformats.wordprocessingml.document",
  }),
  Object.freeze({
    id: "file.export.dotx",
    icon: "description",
    kw: "export save as word template dotx starter",
    // The same OPC package as a DOCX with one content type changed, which is
    // why Word files it beside the document format rather than under a
    // separate heading. It shipped in the engine (`casual-doc-io/src/dotx.rs`)
    // and had no command at all, so the only place it surfaced was the Save-as
    // picker, as the raw string `org.openxmlformats.wordprocessingml.template`.
    format: "org.openxmlformats.wordprocessingml.template",
  }),
  Object.freeze({
    id: "file.export.odt",
    icon: "description",
    kw: "export save as opendocument",
    format: "org.oasis.opendocument.text",
  }),
  Object.freeze({
    id: "file.export.rtf",
    icon: "article",
    kw: "export save as rich text format rtf",
    // RTF is IMPORT ONLY: `crates/casual-doc-io/src/rtf.rs` declares
    // `can_export: false` and registers no writer, so this id is absent from
    // `availableExportFormats()` and this row is DISABLED WITH THE REASON below
    // rather than offering a save that throws. It stays in the menu because a
    // reader who finds no RTF row at all cannot tell a missing writer from a
    // missing feature — `SKILL` §10, never a dead control.
    format: "application.rtf",
  }),
  Object.freeze({
    id: "file.export.html",
    icon: "language",
    kw: "export save as html web page single file",
    // Single-file HTML5, the engine's `text.html` writer. Word's Save As calls
    // this "Web Page"; the name comes from the format catalogue, which is where
    // that decision is recorded.
    format: "text.html",
  }),
  Object.freeze({
    id: "file.export.markdown",
    icon: "code",
    kw: "export save as markdown md commonmark",
    format: "text.markdown",
  }),
  Object.freeze({
    id: "file.export.text",
    icon: "subject",
    kw: "export save as txt",
    format: "text.plain",
  }),
  Object.freeze({
    id: "file.export.json",
    icon: "data_object",
    kw: "export save as json",
    format: "org.casualoffice.normalized-json",
  }),
]);

/**
 * `Export as <format>…` in the active locale.
 *
 * One pattern, nine rows. Exported because the File page's Export tiles and the
 * taxonomy guard both need the same sentence, and a second composition of it is
 * how two surfaces start disagreeing about a format's name.
 *
 * Complexity: O(1).
 */
export function exportCommandLabel(formatId) {
  return t("filePane.export.as", { format: formatLabel(formatId) });
}

/**
 * The export rows as command descriptors.
 *
 * Two independent reasons a row can be unavailable, and they are NOT the same
 * refusal:
 *
 *   * the host withheld `download`, which is "never, for you" — every row goes
 *     dark together and the reason is the capability's;
 *   * this engine build registers no writer for that format, which is "this
 *     cannot be done at all" — one row goes dark and the reason names the
 *     format. RTF is the live case: it imports and cannot be written.
 *
 * Either way the row is DISABLED CARRYING ITS REASON and never removed, and
 * never left enabled to throw. `writableFormats` is the engine's own
 * `availableExportFormats()`, so a format that gains or loses a writer changes
 * this surface with no edit here — one mechanism rather than a hand-kept second
 * list of which formats are real.
 *
 * @param {(format: string) => unknown} exportAs how to run one export.
 * @param {boolean} allowed whether the host granted `download`.
 * @param {string} refusedReason what to say when it did not.
 * @param {readonly string[]} writableFormats `doc.availableExportFormats()`, or
 *   `null` when no document is open yet and the question cannot be asked — in
 *   which case the format half is not claimed either way. The caller asks the
 *   engine per registry build rather than caching the answer: a build happens
 *   once per menu or palette open, never per keystroke, and
 *   `availableExportFormats` reads the FORMAT REGISTRY, not the document, so it
 *   is O(formats) and O(1) in document size.
 * @returns {Array<object>} descriptors in File-menu order.
 */
export function exportCommands(exportAs, allowed = true, refusedReason = "", writableFormats = null) {
  const writable = writableFormats === null ? null : new Set(writableFormats);
  return EXPORT_COMMANDS.map(({ id, kw, format }) => {
    const hasWriter = writable === null || writable.has(format);
    return {
      id,
      label: exportCommandLabel(format),
      group: "File",
      kw,
      // Every export writes a FILE the visitor keeps, so all of them are the one
      // `download` capability. Disabled WITH the reason rather than removed: a host
      // that withheld downloads still wants the row to say so, and a reader who
      // finds no Export at all cannot tell a permission from a missing feature.
      enabled: allowed && hasWriter,
      disabledReason: allowed
        ? t("filePane.export.noWriter", { format: formatLabel(format) })
        : refusedReason,
      run: () => exportAs(format),
    };
  });
}
