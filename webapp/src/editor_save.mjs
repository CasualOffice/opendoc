// Save and export: exporting the document in a chosen format, and Save itself.
// Moved out of `main.js` unchanged (docs/177); it reaches the rest of the
// editor only through the context `ed`.
import { downloadBytes } from "./save_formats.mjs";
import { findingTotals } from "./compat_findings.mjs";
import { downloadNameForFormat } from "./format_io.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSave(ed) {
  function exportDocumentAs(targetFormat, intent = "export") {
    if (!ed.doc || !targetFormat) return;
    try {
      let artifact;
      if (targetFormat === ed.currentSourceFormat) {
        try {
          artifact = ed.doc.exportAs(targetFormat, "exact_if_unchanged");
        } catch {
          artifact = ed.doc.exportAs(targetFormat, "preserve_when_safe");
        }
      } else {
        artifact = ed.doc.exportAs(targetFormat, "semantic");
      }
      const bytes = artifact.bytes;
      const mimeType = artifact.mimeType;
      const extension = artifact.suggestedExtension;
      const report = artifact.reportJson;
      const findings = findingTotals(report).headline; // the chip's count: Word's bookkeeping excluded (FID-FW-01)
      artifact.free();
      const saved = downloadBytes(bytes, mimeType, downloadNameForFormat(ed.currentName, extension), document);
      ed.hostSession?.noteWrite(intent, { format: targetFormat, name: saved, bytes: bytes.length });
      ed.markDocumentSaved();
      ed.compatFindings.show(report, "export");
      ed.setStatus(
        findings === 0
          ? `Saved ${saved}`
          : `Saved ${saved} with ${findings.toLocaleString()} compatibility finding${findings === 1 ? "" : "s"}`,
      );
    } catch (err) {
      console.error(err);
      ed.setStatus(`Save failed: ${err?.message ?? err}`, "error");
    }
  }

  /** Saves using the format chosen in the Save-format selector (default: source). */
  function saveDocument() {
    exportDocumentAs(ed.saveFormatEl && ed.saveFormatEl.value ? ed.saveFormatEl.value : ed.currentSourceFormat, "save");
  }
  ed.saveBtn?.addEventListener("click", saveDocument);

  /** Moves the caret to an engine Caret result (document bounds). Shift extends. */
  return { exportDocumentAs, saveDocument };
}
