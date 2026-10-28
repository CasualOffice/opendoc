// Opening a file the user picked or dropped: the size ceiling and handleFile.
// Moved out of `main.js` unchanged (docs/177); it reaches the rest of the
// editor only through the context `ed`.
import { t } from "./i18n.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFilePick(ed) {
  const MAX_OPEN_BYTES = 64 * 1024 * 1024;

  /** The one entry point for "open these bytes as the document" — the picker and
   *  the viewport drop both land here, which is why the unsaved-work question is
   *  asked here and nowhere else (docs/104 HF-002). Two paths asking the same
   *  question two ways is how one of them ends up not asking it. */
  async function handleFile(file) {
    if (!file) return;
    // The WASM `open` auto-detects any registered format from the bytes, so accept
    // every format the picker offers (DOCX, ODT, RTF, PDF, normalized JSON, plain text) —
    // the extension is only a friendly pre-filter; detection is authoritative.
    if (!/\.(docx|odt|rtf|pdf|json|txt)$/.test(file.name.toLowerCase())) {
      ed.setStatus(t("filePick.unsupported"), "error");
      return;
    }
    // The open document lives in the wasm heap and nowhere else, so replacing it
    // is unrecoverable. Ask before, not after.
    // The engine refuses anything over its package-admission limit, but it only
    // gets to refuse AFTER the whole file has been read into memory. Checking the
    // size the picker already told us is both faster and a message the user can
    // act on ("this file is too big"), instead of a decode failure deep in open().
    if (file.size > MAX_OPEN_BYTES) {
      ed.setStatus(
        `${file.name} is ${Math.round(file.size / 1024 / 1024)} MB — the editor opens files up to 64 MB`,
        "error",
      );
      return;
    }
    if (!(await ed.confirmDiscardIfEdited())) {
      ed.setStatus("Open cancelled — your changes are still here");
      return;
    }
    // Reading a file can fail for reasons that have nothing to do with its
    // contents: it moved or was renamed since it was picked, the volume went away,
    // permission was withdrawn. That rejection used to land on a discarded promise
    // at both call sites, so a dropped file that could not be read did nothing at
    // all — no status, no error, no clue (docs/104 HF-065). Reported here, at the
    // one entry point both the picker and the drop go through, so neither caller
    // can be the one that forgets.
    let buf;
    try {
      buf = await file.arrayBuffer();
    } catch (err) {
      console.warn("file read failed:", err);
      ed.setStatus(`${file.name} could not be read — it may have moved or been renamed`, "error");
      return;
    }
    await ed.openBytes(new Uint8Array(buf), file.name);
  }

  ed.fileEl.addEventListener("change", async (e) => {
    const file = e.target.files[0];
    // Clear the picker's value either way. A file input fires `change` only when
    // the selection differs from what it already holds, so keeping the value
    // makes "cancel the discard prompt, then pick the same file again" a dead
    // control — and re-opening the same file impossible generally (HF-065).
    e.target.value = "";
    await handleFile(file);
  });
  return { handleFile };
}
