// Whether a Layout / References ribbon row can run right now, and why not.
//
// Extracted from `main.js` rather than grown inside it (`109` HF-085): the two
// functions are a pure decision over a state bag, and they are read by four
// callers — the ribbon's enablement sweep, the palette's rows, the object
// right-click menu and the References tab's own buttons — so they are exactly
// the kind of thing that should be testable without a browser.
//
// ## Everything here is O(1), and that is the load-bearing property
//
// `ribbonSurfaceEnabled` runs on **every keystroke and every caret move**. The
// two counts it reads (stale captions, contents fields) are CACHES refreshed on
// the deliberate interactions that change them — opening a document, inserting
// or updating, reaching the References tab — precisely so that nothing here
// walks the document. `documentOutline` shipped the other way round once and
// made a panel take minutes on a real file (SKILL §8).
import { t } from "./i18n.mjs";

/**
 * Whether a row's precondition is met.
 *
 * @param {object} entry a `LAYOUT_SURFACE` / `REFERENCE_SURFACE` row
 * @param {object} state the editor's own answers, all O(1):
 *        `hasDoc`, `objectSelected`, `hasCaret`, `inRunningStory`,
 *        `staleCaptions` (count), `tocFields` (count), `onTocEntry` (boolean)
 * @returns {boolean}
 */
export function ribbonSurfaceEnabled(entry, state) {
  // "missing" is a command that is REAL as a user intention and has no engine
  // operation behind it. It ships permanently disabled carrying its reason.
  if (entry.requires === "missing") return false;
  if (!state.hasDoc) return false;
  if (entry.requires === "object") return state.objectSelected;
  // A caption is body-only: the engine refuses one in a header, footer or note,
  // as Word and ONLYOFFICE do, so the control says so rather than throwing.
  if (entry.requires === "bodyCaret") return state.hasCaret && !state.inRunningStory;
  if (entry.requires === "staleCaptions") return state.staleCaptions > 0;
  if (entry.requires === "tocField") return state.tocFields > 0;
  if (entry.requires === "caret") return state.hasCaret;
  if (entry.requires === "tocEntry") return state.hasCaret && state.onTocEntry;
  return true;
}

/**
 * Why a row is unavailable, for the button title and the palette's
 * `disabledReason`. Never an empty string where {@link ribbonSurfaceEnabled}
 * said no — a disabled control with no reason is the defect `docs/63` forbids.
 */
export function ribbonSurfaceReason(entry, state) {
  if (entry.requires === "missing") return entry.reason;
  if (!state.hasDoc) return "Open a document first";
  if (entry.requires === "object") return "Select an image, shape or text box first";
  if (entry.requires === "staleCaptions") return t("caption.numbersAlreadyRight");
  if (entry.requires === "tocField") return t("toc.noneToUpdate");
  if (entry.requires === "bodyCaret" && state.hasCaret) return t("caption.bodyOnly");
  if (entry.requires === "tocEntry") return t("toc.notAnEntry");
  if (entry.requires === "caret" || entry.requires === "bodyCaret") {
    return "Place the caret in a paragraph";
  }
  return "";
}
