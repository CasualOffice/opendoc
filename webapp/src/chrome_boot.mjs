// Which chrome the editor paints, decided once and in one place (docs/109 UX-064,
// PROPOSAL D-16).
//
// The decision is taken TWICE, and that is the point of this file: once by the
// inline script in `editor.html`'s <head>, before the first frame, and once by
// `setChromeMode` when the modules have loaded. They used to disagree for 2-6 s
// of every cold load — the head knew nothing, so both navigation axes painted —
// and the only way two copies of a rule stay one rule is a test that reads both.
// `chrome_boot.test.mjs` reads the inline script and fails when its preference
// key, its compact value or its breakpoint parts company with the constants here.
//
// Pure: no DOM, no storage. O(1).

import { PHONE_MAX_WIDTH, mediaWidth } from "./phone_chrome.mjs";

/** The persisted chrome choice. The inline head script spells it literally. */
export const CHROME_MODE_PREF = "opendoc.chromeMode";

/** The media query at or below which the shell is a phone, as the head script
 *  must spell it (em, HF-413). */
export const PHONE_QUERY = mediaWidth(PHONE_MAX_WIDTH);

/** `ribbon` | `compact` | `phone` — the value of `html[data-chrome]`. A phone is
 *  compact chrome on the phone rung: it has one navigation axis and it is the
 *  menu, whatever the preference says (docs/148 §5); the preference is kept, so
 *  growing back past the rung restores it. */
export function resolveChrome({ pref, phone }) {
  if (phone) return "phone";
  return pref === "compact" ? "compact" : "ribbon";
}
