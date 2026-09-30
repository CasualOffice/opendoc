// Reflow, wired: the DOM half of `docs/151` §6.
//
// The split is the one `spelling.mjs` / `spell_check.mjs` already established
// here — the decisions in a module `node` can run (`reflow_view.mjs`: which
// widths count as the same width, how long to wait, when the engine will
// refuse), the wiring in this one. Nothing in here decides anything; it reads
// the preference, feeds the width, drives the one engine setter, and reflects
// the state onto the controls that have to say what is going on.
//
// WHAT THIS OWNS, and why each piece is here rather than in `main.js`:
//
//   * the per-viewer preference (ADR-046 §3.4 — a viewer's choice, never a
//     document property), including the phone-rung DEFAULT for a reader who has
//     never chosen;
//   * the `resize` listener, because a resize in reflow is the one event in the
//     product that can cost O(document) and it must not (`docs/107` §4). The
//     width feed in `reflow_view.mjs` is what makes it not;
//   * the View band's face, so the toggle is never a control that looks live and
//     then does nothing;
//   * `#viewport`'s `is-reflow` class, which is what takes the sheet shadow and
//     the corner radius off a TILE. A tile is not a sheet: it is a rasterisation
//     unit, it is cut mid-paragraph at a line boundary, and drawing a paper edge
//     across the middle of a sentence would be a lie about the document. The
//     `gap: 0` half of the same claim is `main.js`'s `buildPageBand` call.
//
// WHAT IT DOES NOT OWN: print. `print.mjs` forces `Paged` itself, because the
// requirement is about the PDF and the raster the printer gets, not about the
// chrome, and a rule enforced next to the thing it is a rule about cannot be
// forgotten by a second caller (`file.export.pdf` is that second caller).

import { t } from "./i18n.mjs";
import { PHONE_MAX_WIDTH } from "./phone_chrome.mjs";
import { readPref, writePref } from "./prefs.mjs";
import {
  REFLOW_PREF_KEY,
  createWidthFeed,
  reflowAvailability,
  reflowMeasure,
} from "./reflow_view.mjs";

/**
 * Wires the reflow view.
 *
 * @param {{
 *   button: HTMLButtonElement|null,
 *   viewport: HTMLElement,
 *   getDoc: () => any,
 *   unavailableReason: () => string,
 *   onChanged: () => void,
 *   setStatus: (message: string, kind?: string) => void,
 *   view?: Window,
 * }} deps
 */
export function createReflowChrome({
  button,
  viewport,
  getDoc,
  unavailableReason,
  onChanged,
  setStatus,
  view = window,
}) {
  /** `null` until the reader chooses, which is what lets the phone rung supply
   *  a default without overriding anybody. Re-read from the rung on every
   *  question rather than resolved once at boot, so a rotation across the rung
   *  answers correctly and there is no second copy of the state to go stale. */
  let chosen = readPref(REFLOW_PREF_KEY, null, view);
  /** The last thing the ENGINE was told, so `sync` is a no-op when nothing moved
   *  and the O(document) re-shape happens once per real change. */
  let applied = null;
  /** The engine's own list of what reflow approximates, reported not hidden. */
  let approximations = [];

  const wanted = () =>
    chosen === null ? view.matchMedia(`(max-width: ${PHONE_MAX_WIDTH}px)`).matches : chosen === "1";

  const availability = () => reflowAvailability(unavailableReason(), t("reflow.unavailable"));

  /** On when the reader wants it AND the engine can give it. */
  const isOn = () => wanted() && availability().available;

  const feed = createWidthFeed({
    onSettled: () => {
      // The width bucket moved and the gesture has settled: this is the only
      // place a resize is allowed to cost anything, and `onChanged` routes it
      // through the same full render an open goes through.
      if (isOn()) onChanged();
    },
  });

  function reflect() {
    viewport.classList.toggle("is-reflow", isOn());
    if (!button) return;
    const { available, reason } = availability();
    button.disabled = !getDoc() || !available;
    button.setAttribute("aria-pressed", String(isOn()));
    // Never a dead control: when the engine refuses, the control carries the
    // refusal rather than sitting there greyed and silent. When it does not, the
    // tooltip carries the state AND what reflow withholds, because the ruler and
    // the Pages panel disappearing is the reader's most visible consequence.
    button.title = reason || `${t(isOn() ? "reflow.on" : "reflow.off")} ${t("reflow.withholds")}`;
    button.setAttribute("aria-label", t(isOn() ? "reflow.commandOn" : "reflow.commandOff"));
  }

  /**
   * Bring the engine's layout view into line with the shell's, and say what
   * width the tiles are. Called by the render pass, before it reads `pageCount`.
   *
   * Complexity: O(1) when nothing moved — the common case, including every
   * render that is not a width change. O(document) exactly when the view or the
   * width bucket actually changes, which is what the quantisation in
   * `reflow_view.mjs` makes rare.
   *
   * @param {number} cssPerTwip CSS px per twip at the current zoom.
   * @returns {boolean} whether the engine is now laying out reflowed.
   */
  function sync(cssPerTwip) {
    const doc = getDoc();
    if (!doc) return false;
    const on = isOn();
    const measure = on ? reflowMeasure(viewport.clientWidth, cssPerTwip, {}) : null;
    const next = measure
      ? [measure.contentWidthTwip, 0, measure.gutterTwip]
      : [0, 0, 0];
    if (applied && next.every((value, i) => value === applied[i])) return on && applied[0] > 0;
    try {
      approximations = JSON.parse(doc.setLayoutView(...next))?.approximations ?? [];
      applied = next;
      feed.adopt(measure ? measure.totalPx : 0);
    } catch (error) {
      // The engine refused. Fall back to paper and say why in its own words —
      // a toggle reading "on" over an unchanged 794px page would be lying about
      // the one thing the reader can see.
      chosen = "0";
      applied = null;
      setStatus(String(error?.message ?? error), "error");
    }
    reflect();
    return !!applied && applied[0] > 0;
  }

  function set(on) {
    chosen = on ? "1" : "0";
    writePref(REFLOW_PREF_KEY, chosen, view);
    feed.cancel();
    onChanged();
    // After the render, not before: `sync` is what learns whether the engine
    // accepted, and the button must not claim a state the engine refused.
    reflect();
    const { available, reason } = availability();
    setStatus(available ? t(isOn() ? "reflow.on" : "reflow.off") : reason);
  }

  button?.addEventListener("click", () => set(!isOn()));
  view.addEventListener("resize", () => {
    // Free unless the bucket moved. This is the O(1)-per-interaction guarantee
    // and it is asserted in `reflow_view.test.mjs` against a fake clock.
    if (isOn()) feed.observe(viewport.clientWidth);
  });

  return {
    isOn,
    sync,
    toggle: () => set(!isOn()),
    // Two withholdings, two sentences. One shared "reflow withholds things"
    // string would be cheaper and would tell a reader who reached for the ruler
    // about a panel they were not reaching for; each control says what IT is and
    // why reflow has nothing for it.
    /** "" when the Pages navigator may be shown, the reason when it may not. */
    withheldReason: () => (isOn() ? t("reflow.pagesWithheld") : ""),
    /** "" when the ruler may be drawn, the reason when it may not. */
    rulerWithheldReason: () => (isOn() ? t("reflow.rulerWithheld") : ""),
    /** What the engine says reflow approximates — surfaced, not hidden. */
    approximations: () => approximations,
    setEnabled: reflect,
  };
}
