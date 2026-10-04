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
//   * `#viewport`'s `is-reflow` class, which is what makes the surface ONE
//     colour edge to edge. A tile is not a sheet: it is a rasterisation unit, it
//     is cut mid-paragraph at a line boundary, and both a paper edge across the
//     middle of a sentence and a differently-coloured field around it are lies
//     about the document. The `gap: 0` half of the same claim is `main.js`'s
//     `buildPageBand` call; the rest is `style.css`'s `#viewport.is-reflow`,
//     which is where the whole argument is written down, correcting `docs/151`
//     §6.2's claim that removing the shadow and the radius is enough;
//   * WHICH PANELS THE VIEW WANTS. Reflow has no page numbers, so the outline
//     is the navigation that replaces them, and it opens when reflow turns on
//     (the owner's description of the view: outline left, document scroll
//     right). The DECISION is here; the mechanics are the host's
//     `openOutline`, because panel exclusivity is the shell's rule and not this
//     module's — see the dep.
//
// WHAT IT DOES NOT OWN: print. `print.mjs` forces `Paged` itself, because the
// requirement is about the PDF and the raster the printer gets, not about the
// chrome, and a rule enforced next to the thing it is a rule about cannot be
// forgotten by a second caller (`file.export.pdf` is that second caller).

// WHAT THE WIDTH CONTROL OWNS (`docs/154` §5.1, ADR-048). The measure is a
// per-viewer choice with four steps and an explicit Full, because WCAG 2.1 SC
// 1.4.8 asks for "a mechanism" and a capped default with no control satisfies
// the criterion in exactly one configuration. The steps and their targets are
// `reflow_view.mjs`'s, where they can be answered in `node`; what is here is the
// popover, the ribbon face, the three command rows and the preference.

import { t } from "./i18n.mjs";
import { PHONE_MAX_WIDTH } from "./phone_chrome.mjs";
import { registerPopover } from "./popover_manager.mjs";
import { readPref, writePref } from "./prefs.mjs";
import {
  REFLOW_PREF_KEY,
  REFLOW_WIDTH_DEFAULT,
  REFLOW_WIDTH_PREF_KEY,
  REFLOW_WIDTH_STEPS,
  createWidthFeed,
  reflowAvailability,
  reflowCapTwip,
  reflowMeasure,
  reflowWidthStep,
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
 *   openOutline?: () => void,
 *   view?: Window,
 *   widthButton?: HTMLButtonElement|null,
 *   widthMenu?: HTMLElement|null,
 * }} deps
 *
 * `openOutline` is called once each time reflow TURNS ON, above the phone rung.
 * It must be a request, not a command: the host decides whether a panel slot is
 * free, because the outline, the review sidebar and the Pages navigator already
 * arbitrate between themselves so the canvas is never squeezed from both sides,
 * and a view asking for a panel must not win that argument. Omit it and reflow
 * changes no panel at all, which is what a non-DOM host wants.
 */
export function createReflowChrome({
  button,
  viewport,
  getDoc,
  unavailableReason,
  onChanged,
  setStatus,
  openOutline = () => {},
  view = window,
  widthButton = view.document?.getElementById("viewTextWidthBtn") ?? null,
  widthMenu = view.document?.getElementById("textWidthMenu") ?? null,
}) {
  /** `null` until the reader chooses, which is what lets the phone rung supply
   *  a default without overriding anybody. Re-read from the rung on every
   *  question rather than resolved once at boot, so a rotation across the rung
   *  answers correctly and there is no second copy of the state to go stale. */
  let chosen = readPref(REFLOW_PREF_KEY, null, view);
  /** The width step the reader has chosen, or `null` for "never chosen" — which
   *  resolves to `REFLOW_WIDTH_DEFAULT`, the one line that changes the desktop
   *  default (`reflow_view.mjs`). */
  let widthChoice = readPref(REFLOW_WIDTH_PREF_KEY, null, view);
  /** The last thing the ENGINE was told, so `sync` is a no-op when nothing moved
   *  and the O(document) re-shape happens once per real change. */
  let applied = null;
  /** The zoom the last `sync` ran at, so the `resize` listener can measure the
   *  same column the render pass would. Without it the feed would compare a
   *  CAPPED painted width against a RAW window width and schedule a pass on
   *  every desktop resize — the O(1) guarantee is about comparing like with
   *  like. `null` until the first render, where observing the raw width is both
   *  the old behaviour and the only thing available. */
  let lastCssPerTwip = null;
  /** The engine's own list of what reflow approximates, reported not hidden. */
  let approximations = [];
  /** The last measure handed to the engine, kept for `surfaceSlackPx` — the one
   *  question about the view that something OUTSIDE the column has to ask.
   *  `null` before the first sync, which reads as "no surface", not as a guess. */
  let lastMeasure = null;
  /** The view state the panels were last adopted for. `null` rather than `false`
   *  so the FIRST resolution counts as a transition: below the phone rung reflow
   *  is on from boot with no reader involved, and a transition detector seeded
   *  `false` would call that "already on" and never ask for the panel. */
  let adopted = null;

  /** The phone rung, asked once so the default and the panel rule cannot
   *  disagree about where it is. */
  const onPhoneRung = () => view.matchMedia(`(max-width: ${PHONE_MAX_WIDTH}px)`).matches;

  const wanted = () => (chosen === null ? onPhoneRung() : chosen === "1");

  const availability = () => reflowAvailability(unavailableReason(), t("reflow.unavailable"));

  /** On when the reader wants it AND the engine can give it. */
  const isOn = () => wanted() && availability().available;

  /** The width step in effect. `null` is "never chosen", which is the default. */
  const activeWidth = () => reflowWidthStep(widthChoice ?? REFLOW_WIDTH_DEFAULT).id;

  /**
   * The cap for the active step, in twips, read from the document.
   *
   * Two engine calls, both cheap and neither O(document):
   * `stylePreview("Normal")` resolves the document's default face and size
   * through the style cascade (O(styles), reading `definitions()` only), and
   * `pageSetup()` returns the FIRST section's geometry straight off
   * `sections.first()`. The caret-aware `pageSetupSections`/`sectionLayout` are
   * deliberately NOT used: both resolve a node by walking every paragraph
   * (`section_of`), and a cap is not worth a document walk per render.
   *
   * A failure returns `Infinity` — i.e. `available`, which is where this feature
   * started. A reading comfort must never be the reason a document fails to lay
   * out.
   *
   * Complexity: O(styles) + O(1). Called once per render pass, only when reflow
   * is on.
   */
  function capTwip(doc) {
    const step = activeWidth();
    let preview = null;
    try {
      if (step === "fit") {
        const setup = JSON.parse(doc.pageSetup() ?? "null");
        const size = setup?.pageSize;
        const margins = setup?.pageMargins;
        if (!size || !margins) return Infinity;
        return reflowCapTwip(step, {
          docMeasureTwip: size.widthTwips - margins.startTwips - margins.endTwips,
        });
      }
      preview = doc.stylePreview?.("Normal");
      return reflowCapTwip(step, {
        face: preview?.fontFamily,
        fontSizePt: preview?.sizePoints,
      });
    } catch {
      return Infinity;
    } finally {
      // The preview is a wasm-owned struct; the style gallery leaves it to the
      // collector because it builds one per OPENING, and this builds one per
      // RENDER. Freed explicitly for that difference alone.
      preview?.free?.();
    }
  }

  /** The measure for a given window width at the last known zoom, or `null`.
   *  One helper so the render pass and the `resize` listener cannot ask the
   *  question two different ways — which is exactly how a capped width would
   *  come to be compared against a raw one. */
  function measureAt(cssPerTwip, doc) {
    if (!(cssPerTwip > 0) || !doc) return null;
    return reflowMeasure(viewport.clientWidth, cssPerTwip, { capTwip: capTwip(doc) });
  }

  /**
   * The panels this view wants, asked for once per transition INTO reflow.
   *
   * Reflow has no page numbers, so the outline is what replaces them, and the
   * owner's description of the view is "an outline on the left and the whole
   * scroll on the right". It is a REQUEST through `openOutline`: the host owns
   * panel exclusivity and may refuse.
   *
   * Called BEFORE the measure is taken, and that ordering is the reason this is
   * not folded into `reflect()`. Opening a 252px panel narrows the scroller by
   * that much; a column measured first and a panel opened after would hand the
   * engine a width that had already stopped being true, and the reader would see
   * one render at the wrong measure. `reflect()` runs at the END of `sync`.
   *
   * NOT at the phone rung. There, `.side-panel` is a bottom sheet
   * (`style.css`, `body.phone-mode .side-panel`) that covers 55vh of the
   * document it exists to navigate, and "outline left" has no left in a 390px
   * window — which is also the rung where reflow is on by DEFAULT, so this is
   * the common case rather than an edge. Same `matchMedia` as the default, so
   * there is one statement of where the phone tier starts.
   *
   * Complexity: O(1). The host's `openOutline` is not — building the outline is
   * a document walk — but it runs once per mode change, which is already the one
   * interaction in this module that is allowed to cost O(document).
   */
  function adoptPanels() {
    const on = isOn();
    if (on === adopted) return;
    adopted = on;
    if (on && !onPhoneRung()) openOutline();
  }

  /**
   * How much uniform surface there is beside the measure, per side, in CSS px —
   * `0` whenever there is no surface to speak of, which includes every paged
   * view.
   *
   * This exists because the uniform surface changed what the space beside the
   * column IS. On paper that space is the desk and nothing may be painted on it;
   * in reflow it is the same surface the text sits on, so it is a margin, and an
   * affordance that belongs beside a line of text can live there. Its one caller
   * is the fold disclosure, which was unreachable in reflow for want of exactly
   * this — see `fold_chrome.mjs`'s `syncBodyChevron`.
   *
   * Derived from the last measure rather than from the DOM, so it is the same
   * number the engine was given and cannot drift from it. Deliberately
   * SYMMETRIC, and therefore conservative: with the review sidebar open the
   * stack reserves a right-hand gutter and the column shifts left, so the true
   * left slack is larger than this and the answer errs towards withholding.
   *
   * Complexity: O(1), one `clientWidth` read.
   */
  function surfaceSlackPx() {
    if (!isOn() || !lastMeasure) return 0;
    return Math.max(0, Math.floor((viewport.clientWidth - lastMeasure.totalPx) / 2));
  }

  const feed = createWidthFeed({
    onSettled: () => {
      // The width bucket moved and the gesture has settled: this is the only
      // place a resize is allowed to cost anything, and `onChanged` routes it
      // through the same full render an open goes through.
      if (isOn()) onChanged();
    },
  });

  /** The width control's face: the step's short label on the button, a radio
   *  check on the row that is in effect, and the whole control disabled with a
   *  reason when there are no pages to reflow or no reflow to measure.
   *
   *  Disabled rather than hidden when reflow is OFF, and carrying why: a text
   *  width is meaningless on paper, where the measure is the document's, and a
   *  control that silently did nothing there is the "never a dead control" rule
   *  broken the quiet way. */
  function reflectWidth() {
    if (!widthButton) return;
    const step = reflowWidthStep(activeWidth());
    const on = isOn();
    widthButton.disabled = !getDoc() || !on;
    const label = widthButton.querySelector("[data-width-label]") ?? widthButton;
    label.textContent = t(step.shortKey);
    widthButton.title = on ? t(step.titleKey) : t("textWidth.pagedWithheld");
    widthButton.setAttribute("aria-label", t("textWidth.command"));
    if (!widthMenu) return;
    for (const row of widthMenu.querySelectorAll("[data-text-width]")) {
      row.setAttribute("aria-checked", String(row.dataset.textWidth === step.id));
    }
  }

  function reflect() {
    viewport.classList.toggle("is-reflow", isOn());
    reflectWidth();
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
    // The panels first: they change `clientWidth`, which the measure reads.
    adoptPanels();
    const on = isOn();
    lastCssPerTwip = cssPerTwip > 0 ? cssPerTwip : lastCssPerTwip;
    const measure = on ? measureAt(cssPerTwip, doc) : null;
    lastMeasure = measure;
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

  /** Choose a width step. The same function behind every surface, so the ribbon
   *  popover, the View menu and the palette cannot disagree about what a step
   *  means — `SKILL.md` §10's ≥2-surfaces floor is only worth anything if the
   *  surfaces are faces of ONE command.
   *
   *  A step change is O(document): the galley cache is width-scoped, so a new
   *  width keeps nothing. It is a deliberate choice rather than a gesture, which
   *  is why it goes straight through `onChanged` with no debounce — unlike a
   *  resize, which `createWidthFeed` coalesces. */
  function setWidth(stepId) {
    const step = reflowWidthStep(stepId);
    widthChoice = step.id;
    writePref(REFLOW_WIDTH_PREF_KEY, step.id, view);
    feed.cancel();
    if (isOn()) onChanged();
    reflect();
    setStatus(isOn() ? t(step.titleKey) : t("textWidth.pagedWithheld"));
  }

  button?.addEventListener("click", () => set(!isOn()));
  if (widthButton && widthMenu) {
    registerPopover(widthButton, widthMenu, reflectWidth);
    widthMenu.addEventListener("click", (event) => {
      const row = event.target.closest("[data-text-width]");
      if (!row) return;
      setWidth(row.dataset.textWidth);
    });
  }
  view.addEventListener("resize", () => {
    // Free unless the bucket moved. This is the O(1)-per-interaction guarantee
    // and it is asserted in `reflow_view.test.mjs` against a fake clock.
    //
    // The PAINTED width, not the window's: above the cap the column does not
    // move with the window at all, so a desktop resize produces the same bucket
    // and costs one division. Feeding the raw `clientWidth` here would compare a
    // capped width against an uncapped one and schedule a pass for a column that
    // is not going to change (`reflow_view.mjs`'s `reflowMeasure`).
    if (!isOn()) return;
    const measure = measureAt(lastCssPerTwip, getDoc());
    feed.observe(measure ? measure.totalPx : viewport.clientWidth);
  });

  return {
    isOn,
    sync,
    toggle: () => set(!isOn()),
    /** The width step in effect, for a surface that has to say which it is. */
    widthStep: activeWidth,
    setWidth,
    /** The width control's command rows — one per step, generated from the one
     *  table, so the palette and the View menu cannot offer a step the popover
     *  does not have. Declared here rather than in `main.js` because the labels,
     *  the state and the refusal all live with the control; `main.js` spreads the
     *  result into its registry. */
    commands: () =>
      REFLOW_WIDTH_STEPS.map((step) => ({
        id: `view.textWidth.${step.id}`,
        label: t(step.commandKey),
        group: "View",
        kw: `text width measure column reading line length characters wcag ${step.id}`,
        enabled: isOn(),
        disabledReason: t("textWidth.pagedWithheld"),
        run: () => setWidth(step.id),
      })),
    // Two withholdings, two sentences. One shared "reflow withholds things"
    // string would be cheaper and would tell a reader who reached for the ruler
    // about a panel they were not reaching for; each control says what IT is and
    // why reflow has nothing for it.
    /** "" when the Pages navigator may be shown, the reason when it may not. */
    withheldReason: () => (isOn() ? t("reflow.pagesWithheld") : ""),
    /** "" when the ruler may be drawn, the reason when it may not. */
    rulerWithheldReason: () => (isOn() ? t("reflow.rulerWithheld") : ""),
    /** Paintable surface beside the measure, per side, in CSS px. See above. */
    surfaceSlackPx,
    /** What the engine says reflow approximates — surfaced, not hidden. */
    approximations: () => approximations,
    setEnabled: reflect,
  };
}
