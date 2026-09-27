// The save surface: which formats the Save selector offers, what the
// compatibility chip says, and the one step that puts bytes on the visitor's
// disk.
//
// Extracted from `main.js` to pay for the host contract's wiring (`docs/126`
// phase 2) — the file was AT its ratchet with zero slack, and the honest ways to
// go green are to put the new code in a module or take something else out
// (`module_seams.test.mjs`). This is the something else, and it is worth more
// than the lines: three questions that could only be asked by loading an 16k-line
// module into a browser are now answerable in node.
//
//   * does the Save selector offer every exporter the ENGINE registered, and does
//     it default to the format the document was opened as, so a round-trip save
//     keeps its format?
//   * what does the compatibility chip read when an import or an export reported
//     findings, and is it really hidden when there are none?
//   * does the download step name the file the caller asked for?
//
// Every DOM node and every engine call arrives as an argument. Nothing here
// reaches for a global, which is what makes the above true; the module is not in
// `PURE_MODULES` because the download step must build an anchor, and an anchor
// needs a document.
import { formatInfo } from "./format_io.mjs";

/**
 * Surfaces the compatibility-finding count from an import or export in the
 * status chip; hidden when there is nothing to report.
 *
 * `phase` is a word the caller supplies ("import" / "export") because it is part
 * of the sentence, and the sentence is the caller's to localise.
 *
 * Complexity: O(1).
 */
export function showCompatibilityFindings(chip, count, phase) {
  if (!chip) return;
  chip.hidden = count === 0;
  chip.textContent =
    count === 0 ? "" : `${count.toLocaleString()} ${phase} finding${count === 1 ? "" : "s"}`;
  chip.title =
    count === 0
      ? ""
      : `${count.toLocaleString()} compatibility finding${count === 1 ? "" : "s"} reported during ${phase}`;
}

/**
 * Fills the Save-format selector with every registered exporter, defaulting to
 * the format the document was opened as.
 *
 * The default is the point: saving a DOCX back as DOCX is what preserves
 * unchanged bytes, and a selector that defaulted to the first option in the
 * engine's list would quietly turn every round trip into a format conversion.
 *
 * Complexity: O(formats) — six of them.
 *
 * @param {HTMLSelectElement|null} select
 * @param {readonly string[]} formats the engine's `availableExportFormats()`.
 * @param {string} sourceFormat the format the document was opened as.
 * @param {Document} view the document the `<option>` elements are created in.
 */
export function populateSaveFormats(select, formats, sourceFormat, view) {
  if (!select) return;
  select.replaceChildren();
  for (const formatId of formats ?? []) {
    const option = view.createElement("option");
    option.value = formatId;
    option.textContent = formatInfo(formatId).label;
    select.append(option);
  }
  select.value = sourceFormat;
  if (!select.value && select.options.length > 0) select.selectedIndex = 0;
  select.disabled = select.options.length === 0;
}

/**
 * Hands `bytes` to the visitor as a file named `name`.
 *
 * The object URL is revoked immediately after the click, which is what stops a
 * session of saves from pinning every exported document in memory — a document
 * is megabytes, and the blob is not collected while a URL still refers to it.
 *
 * Complexity: O(bytes), once, on an explicit save.
 *
 * @returns {string} the name the file was offered under.
 */
export function downloadBytes(bytes, mimeType, name, view) {
  const url = URL.createObjectURL(new Blob([bytes], { type: mimeType }));
  const anchor = view.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
  return anchor.download;
}
