// Insert ▸ Drop Cap. The engine owns the OOXML representation and layout; this
// module owns the small modal that reads and writes that public host API.
import { t } from "./i18n.mjs";

const MODES = new Set(["none", "drop", "margin"]);

/** Normalizes the JSON returned by `WasmDocument.dropCap`. Exported so malformed
 * host data and the 1..=10 UI bound are testable without a browser. */
export function normalizeDropCap(value) {
  let parsed = value;
  if (typeof value === "string") {
    try {
      parsed = JSON.parse(value);
    } catch {
      parsed = null;
    }
  }
  const mode = MODES.has(parsed?.mode) ? parsed.mode : "none";
  const numeric = Number(parsed?.lines);
  const lines = Number.isFinite(numeric)
    ? Math.min(10, Math.max(1, Math.round(numeric)))
    : 3;
  return { mode, lines };
}

/** Mounts the dialog over markup already present in `editor.html`.
 *
 * `io` supplies the application state this isolated surface needs:
 *   getDoc()              current WasmDocument
 *   selectionNode()       paragraph at the caret
 *   mutationBlocked()     review-mode policy, with its user-facing reason
 *   apply(mode, lines)    one gated, undoable edit through applyEditResult
 *   registerModal(...)    shared focus/dismissal contract
 *   status(message)       status-bar announcement
 */
export function createDropCapDialog(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("dropCapDialog");
  const form = el("dropCapForm");
  const linesInput = el("dropCapLines");
  const closeBtn = el("dropCapClose");
  const cancelBtn = el("dropCapCancel");
  if (!dialog) return { open() {}, close() {} };

  const choices = [...dialog.querySelectorAll('input[name="dropCapMode"]')];
  let reflected = { mode: "none", lines: 3 };

  const selectedMode = () =>
    choices.find((choice) => choice.checked)?.value ?? "none";

  function reflectEnabledState() {
    linesInput.disabled = selectedMode() === "none";
  }

  const modal = io.registerModal(dialog, {
    initialFocus: () => choices.find((choice) => choice.checked),
    fallbackFocus: io.fallbackFocus,
  });

  function close() {
    modal.close();
  }

  function open() {
    const doc = io.getDoc();
    const node = io.selectionNode();
    if (!doc || !node || io.mutationBlocked()) return;
    try {
      reflected = normalizeDropCap(doc.dropCap(node));
    } catch (error) {
      console.warn("dropCap read ignored:", error?.message ?? error);
      io.status(t("dropCap.readError"), "error");
      return;
    }
    const selected =
      choices.find((choice) => choice.value === reflected.mode) ?? choices[0];
    selected.checked = true;
    linesInput.value = String(reflected.lines);
    reflectEnabledState();
    modal.open();
  }

  for (const choice of choices)
    choice.addEventListener("change", reflectEnabledState);
  closeBtn.addEventListener("click", close);
  cancelBtn.addEventListener("click", close);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const next = normalizeDropCap({
      mode: selectedMode(),
      lines: linesInput.value,
    });
    if (
      next.mode === reflected.mode &&
      (next.mode === "none" || next.lines === reflected.lines)
    ) {
      close();
      return;
    }
    const applied = await io.apply(next.mode, next.lines);
    if (!applied) return;
    close();
    io.status(t(next.mode === "none" ? "dropCap.removed" : "dropCap.applied"));
  });

  return { open, close };
}
