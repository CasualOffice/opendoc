// About (HF: "which build am I on?").
//
// Extracted from `main.js` (`109` HF-085). There was no About anywhere in the
// product: no version, no licence, no way for someone reporting a bug to say
// which build they were on. The version comes from the engine's own
// `engineVersion()`, compiled from its crate manifest, so it cannot drift from
// what actually shipped.

import { registerModal } from "./modal.mjs";

/**
 * Wires the About dialog and returns its toggle.
 *
 * @param {() => string} engineVersion the engine's own version getter.
 * @param {() => Element} fallbackFocus where focus goes when the dialog closes
 *        and the thing that opened it is gone.
 * @returns {(open: boolean) => void}
 */
export function createAboutDialog(engineVersion, fallbackFocus) {
  const dialog = document.getElementById("aboutDialog");
  const close = document.getElementById("aboutClose");
  const modal = dialog
    ? registerModal(dialog, { initialFocus: () => close, fallbackFocus })
    : null;

  /** Stamps the engine version into the panel. Shared with the File page's
   *  About PANE, which shows the same element without opening the dialog. */
  const stampVersion = () => {
    const slot = document.getElementById("aboutVersion");
    if (slot) {
      // The engine may not have booted yet — About is a `noDoc` command, so it
      // is reachable from the very first frame. Say so rather than printing a
      // placeholder that reads like a version.
      let version = "";
      try {
        version = engineVersion();
      } catch {
        version = "";
      }
      slot.textContent = version || "not loaded yet";
    }
  };

  const toggle = (open) => {
    if (!modal) return;
    if (!open) {
      modal.close();
      return;
    }
    stampVersion();
    modal.open();
  };

  close?.addEventListener("click", () => toggle(false));
  // The stamp comes back too: the File page's About PANE shows this same
  // element without opening the dialog, and an About that says "not loaded
  // yet" forever would be worse than no pane.
  toggle.stampVersion = stampVersion;
  return toggle;
}
