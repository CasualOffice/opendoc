// Folding, wired: the DOM/engine half of ADR-049's chrome.
//
// The split is `reflow_view.mjs` / `reflow_chrome.mjs`: the decisions in
// `fold_view.mjs`, which `node` runs with no browser, and the wiring here.
// Nothing in this file decides anything — it reads the engine's fold state,
// drives the four engine calls, paints the in-body chevron, and hands the
// outline panel its callbacks.
//
// WHAT THIS OWNS, and why each piece is here rather than in `main.js`:
//
//   * the **one** `FoldSet`, which is the engine's (`foldState()`), not a shadow
//     copy. ADR-049 said the live fold state "lives beside `docReflow` in
//     `prefs.mjs`" and that sentence is WRONG: `NodeId`s are minted at import,
//     so a persisted fold set would restore one document's folds onto whatever
//     ids a later open happened to mint. The ADR is corrected rather than
//     followed. The cross-session tier is `w15:collapsed`, which the engine
//     reads at open — a real document property with real identity.
//   * the four command rows, so the labels, the state and the refusal live with
//     the control and `main.js` spreads the result into its registry;
//   * the **in-body chevron** at the caret's heading — Word's affordance, which
//     ONLYOFFICE lacks entirely. Only the CARET's heading, deliberately: a
//     chevron at every heading costs a `caretRect`-shaped query per heading,
//     which is O(document) per render and breaches `docs/107` §4. The caret's
//     heading is the sound first increment and the one Word's own hover
//     affordance is anchored to.
//   * the page-count sentence, because Word says it and a page indicator that
//     silently means something else while folded is a lie.
//
// WHAT IT DOES NOT OWN: print. `print.mjs` forces `Paged` itself and the same
// wrapper is where "fully expanded" belongs — a rule enforced next to the thing
// it is a rule about cannot be forgotten by a second caller. **Built there, not
// here, as of `109` FOLD-001:** `expandFolds` inside `withPagedLayout`, restoring
// through `setFoldSet` in one re-layout, with the rule guarded over the wrapper
// rather than over either printer.

import { t } from "./i18n.mjs";
import { chevronPlacement, foldCommands, pageCountCaveat, parseOutlineRows } from "./fold_view.mjs";

/**
 * Wires folding.
 *
 * `onChanged` must REPAINT AND REFRESH THE CHROME, not merely repaint. A fold
 * changes the layout and the projection of the document together, so a host that
 * only re-rasters the pages leaves the outline tree and the accessibility mirror
 * describing the old fold state while the canvas shows the new one — the panel
 * that drives the fold disagreeing with the fold. In this shell that is
 * `renderAll()` followed by `scheduleChromeRefresh({ stats: true, outline: true })`;
 * the outline flag carries the mirror with it. Stated here because an e2e run
 * caught it and nothing at the model tier could have.
 *
 * `getPages` and `scaleOf` are the pair `ruler.mjs` and `touch_selection.mjs`
 * already take, and they are here for the same reason: an overlay element has to
 * land on the page record the engine named, at that page's own scale. They are
 * optional — omit them and the margin chevron simply does not paint, which is
 * what a non-DOM host wants.
 *
 * @param {{
 *   getDoc: () => any,
 *   caretNode: () => string,
 *   onChanged: () => void,
 *   setStatus: (message: string, kind?: string) => void,
 *   getPages?: () => Array<{overlay?: HTMLElement}>,
 *   scaleOf?: (page: any) => ({sx: number, sy: number}),
 * }} deps
 */
/** How far into the page margin the in-body chevron sits, in CSS pixels.
 *  Matches `.fold-body-chevron`'s 18px width plus a 2px gap. */
const CHEVRON_GUTTER = 20;

export function createFoldChrome({
  getDoc,
  caretNode,
  onChanged,
  setStatus,
  getPages = () => [],
  scaleOf = () => ({ sx: 1, sy: 1 }),
}) {
  /** The last state the ENGINE reported, re-read after every call rather than
   *  mutated here. One state, and it is not this module's. */
  let state = { folded: [], available: false, withheldReason: "" };
  /** The outline rows the last `sync` saw, so the caret's heading and its fold
   *  state are answered without a second engine call. Replaced wholesale; never
   *  patched, because a patched copy is the shadow state this design refuses. */
  let rows = [];

  const read = () => {
    const doc = getDoc();
    if (!doc) {
      state = { folded: [], available: false, withheldReason: "" };
      rows = [];
      return state;
    }
    try {
      state = JSON.parse(doc.foldState());
    } catch {
      state = { folded: [], available: false, withheldReason: "" };
    }
    try {
      rows = parseOutlineRows(doc.documentOutline());
    } catch {
      rows = [];
    }
    return state;
  };

  /** Why folding is withheld, as a sentence, or `""`. The ENGINE's reason — it
   *  is the one that knows a windowed body cannot be folded — with a local
   *  fallback for "no document open at all". */
  const reason = () => {
    if (!getDoc()) return t("fold.noDocument");
    return state.available ? "" : state.withheldReason || t("fold.unavailable");
  };

  /** The heading the caret is in, with its live fold state, or `null`.
   *
   *  O(headings) over the rows `sync` already read, and the caret's heading is
   *  the row whose node IS the caret's node: a heading is a paragraph, so this
   *  is an identity test rather than a containment search. A caret inside a body
   *  paragraph under a heading is deliberately NOT treated as being "in" that
   *  heading — Word's chevron is on the heading itself, and guessing an ancestor
   *  would make the toggle act on something the reader did not point at. */
  const caretHeading = () => {
    const node = caretNode();
    if (!node) return null;
    const row = rows.find((candidate) => candidate.node === node);
    return row ? { node: row.node, collapsed: row.collapsed } : null;
  };

  /**
   * Where the margin chevron goes: the overlay of the page the caret's heading
   * STARTS on, and that heading's start rectangle in the overlay's own
   * coordinates. `null` when there is nothing to anchor to.
   *
   * Host and rectangle come back together, from ONE engine call, deliberately:
   * two independent getters can answer about different pages between them — a
   * repaint, a scroll out of the window, a page record replaced wholesale — and
   * a chevron placed on page 3's overlay at page 7's y lands off the sheet.
   *
   * Offset `0`, not the caret's offset, so the chevron sits beside the start of
   * the heading instead of sliding along the line as the caret moves. The
   * -20px margin that puts it OUTSIDE the text column is in `style.css`, with
   * the rest of the control's look.
   *
   * Complexity: one `caretRect`, which is answered from the laid-out page the
   * caret is on — O(1) in document size, per `docs/107` §4.
   */
  const chevronAnchor = () => {
    const doc = getDoc();
    const node = caretNode();
    if (!doc || !node) return null;
    let flat;
    try {
      flat = doc.caretRect(node, 0);
    } catch {
      return null;
    }
    // `[page, x, y, w, h]` in twips, or empty when the node is not placed —
    // which is exactly what a heading inside ANOTHER fold looks like.
    if (!flat || flat.length < 5) return null;
    const page = getPages()[flat[0] - 1];
    if (!page?.overlay) return null;
    const { sx, sy } = scaleOf(page);
    return {
      host: page.overlay,
      rect: { x: flat[1] * sx, y: flat[2] * sy, height: flat[4] * sy },
    };
  };

  /** Runs one engine fold call, reports a refusal rather than swallowing it,
   *  and lets the host repaint. */
  const run = (call) => {
    const doc = getDoc();
    if (!doc) {
      setStatus(t("fold.noDocument"), "warn");
      return;
    }
    try {
      call(doc);
    } catch (error) {
      // The engine's refusal, said out loud. A fold that silently does nothing
      // is the failure mode this whole lane exists to end.
      setStatus(String(error?.message ?? error), "warn");
      return;
    }
    read();
    // AFTER the repaint, not before. `renderAll` publishes its own
    // "Rendering N pages…" line and then clears it, so a caveat announced first
    // was overwritten and then wiped — set, and never seen. `onChanged` is
    // allowed to be synchronous, so the result is normalised rather than
    // assumed to be a promise, and the announcement happens either way round.
    Promise.resolve(onChanged()).then(announce, announce);
  };

  /** Says that the on-screen page count is not the printed one, once per change
   *  rather than on every render, because Word says it and because collapsed
   *  content occupies no pages. Silent when nothing is folded. */
  const announce = () => {
    const sentence = pageCountCaveat(state.folded?.length ?? 0, t);
    if (sentence) setStatus(sentence, "info");
  };

  const toggle = (node, collapsed) => run((doc) => doc.setFold(node, collapsed));

  return {
    /** Re-reads the engine's fold state and outline. Call it on every content
     *  or view change, exactly where `buildOutline` is already called. */
    sync: read,
    state: () => state,
    reason,
    caretHeading,
    /** Fold or unfold one heading by node id — the panel's disclosure, the body
     *  chevron and `view.fold.toggle` all come through here, which is what makes
     *  them one state rather than three. */
    toggle,
    foldAll: () => run((doc) => doc.foldAll()),
    unfoldAll: () => run((doc) => doc.unfoldAll()),
    foldToLevel: (level) => run((doc) => doc.foldToLevel(level)),
    /** The four command rows (twelve, with the level picker's rungs), for
     *  `main.js` to spread into its registry.
     *
     *  Re-reads the engine FIRST, rather than trusting whatever the last `sync`
     *  left behind. Without that, `view.fold.toggle` would refuse with "put the
     *  caret in a heading" for any reader who had never opened the outline
     *  panel — the rows would be generated from an empty outline — which is the
     *  shape of a control that is present, enabled-looking and wrong.
     *
     *  Affordable because `editorCommands()` is rebuilt per GESTURE: opening the
     *  palette, opening a menu, a chord press, a context menu. `docs/107` §4
     *  budgets an interaction at `O(1)` in document size; a gesture that is
     *  already building the whole registry is not that interaction, and one
     *  `documentOutline()` is in proportion to it. */
    commands: () => {
      read();
      return foldCommands({
        t,
        reason,
        caretHeading,
        toggle: () => {
          const heading = caretHeading();
          if (!heading) {
            setStatus(t("fold.noHeadingAtCaret"), "warn");
            return;
          }
          toggle(heading.node, !heading.collapsed);
        },
        foldAll: () => run((doc) => doc.foldAll()),
        unfoldAll: () => run((doc) => doc.unfoldAll()),
        pickLevel: (level) => run((doc) => doc.foldToLevel(level)),
      });
    },
    /** The options `renderOutline` needs to make its disclosures live. */
    outlineOptions: () => ({
      onToggle: state.available ? toggle : null,
      toggleWithheldReason: reason(),
      labels: {
        tree: t("fold.treeLabel"),
        collapse: t("fold.collapseHeading"),
        expand: t("fold.expandHeading"),
      },
    }),
    /**
     * Paints (or removes) the in-body chevron at the caret's heading.
     *
     * Word's affordance, and ONLYOFFICE has nothing like it. ONE chevron, at the
     * caret's heading only: a chevron at every heading needs that heading's
     * on-page rectangle, which is a per-heading query and therefore O(document)
     * per render — the budget `docs/107` §4 sets for an interaction is O(1) in
     * document size. Extending it to every heading in the viewport is a real
     * increment and wants the viewport's own heading list, which the engine does
     * not expose yet; the caret's heading is the sound first one.
     *
     * Idempotent: it reuses the one element it owns, so calling it on every
     * render costs a class toggle and two style writes.
     *
     * **WHAT IT DOES NOT DO, stated because the bound is real and would
     * otherwise read as a bug:** it does not re-read the outline. It paints from
     * whatever the last `sync` left, so the chevron is live once the outline
     * state has been synced — which is on every content change while the outline
     * panel is open, after any fold command, and on open — and absent before
     * that. It is NOT synced per repaint on purpose: `documentOutline()` is
     * `O(document)`, `drawSelection` runs on every caret move, and paying a
     * document walk per arrow key is precisely the per-interaction budget
     * `docs/107` §4 forbids. Closing it properly needs an `O(1)` "is this node a
     * heading" from the engine — `is_heading` walks every surface — which is
     * `109` FOLD-006 and not worked around here. Folding itself is reachable
     * without the chevron from the outline tree, the View menu and the palette,
     * so no capability depends on this.
     */
    syncBodyChevron: () => {
      const heading = caretHeading();
      const anchor = heading && state.available ? chevronAnchor() : null;
      // A chevron on any OTHER overlay is stale — the caret has moved to a
      // different sheet, or there is nothing to anchor to at all. Sweeping
      // rather than remembering the last host is what keeps a chevron from
      // being orphaned by a repaint that replaced the page records. O(pages
      // in the DOM), which is the viewport's window and not the document.
      for (const page of getPages()) {
        if (page.overlay && page.overlay !== anchor?.host) {
          page.overlay.querySelector(".fold-body-chevron")?.remove();
        }
      }
      if (!anchor) return;
      const { host, rect } = anchor;
      // The chevron lives in the page MARGIN beside the heading, which is where
      // Word puts it. When there is no margin to live in, it is not painted.
      //
      // This replaces a clamp, and the clamp was a defect dressed as a fix. At
      // the phone rung reflow pulls the text to a 16px inset, so an 18px chevron
      // offset by 20px painted at -4px — outside the window. Clamping it to 0
      // stopped the overflow and put a slice of an absolutely-positioned BUTTON
      // on top of the heading's first glyph, where it captures the tap that
      // should place the caret. A control that steals a click from the text is
      // worse than no control.
      //
      // Withholding it costs nothing a reader can reach for: folding is still on
      // the outline tree, on View ▸ Show and in the palette, so this is a
      // missing ornament and not a missing capability.
      const place = chevronPlacement(rect, CHEVRON_GUTTER);
      if (!place.show) {
        host.querySelector(".fold-body-chevron")?.remove();
        return;
      }
      let chevron = host.querySelector(".fold-body-chevron");
      if (!chevron) {
        chevron = document.createElement("button");
        chevron.type = "button";
        chevron.className = "fold-body-chevron";
        chevron.tabIndex = -1;
        const glyph = document.createElement("span");
        glyph.className = "ms";
        glyph.setAttribute("aria-hidden", "true");
        chevron.append(glyph);
        chevron.addEventListener("mousedown", (event) => {
          // The chevron must not take the caret: the reader is pointing at a
          // disclosure, not clicking into the heading.
          event.preventDefault();
        });
        chevron.addEventListener("click", (event) => {
          event.stopPropagation();
          const current = caretHeading();
          if (current) toggle(current.node, !current.collapsed);
        });
        host.append(chevron);
      }
      const label = t(heading.collapsed ? "fold.expandHeading" : "fold.collapseHeading");
      chevron.title = label;
      chevron.setAttribute("aria-label", label);
      chevron.dataset.node = heading.node;
      chevron.querySelector(".ms").textContent = heading.collapsed ? "chevron_right" : "expand_more";
      chevron.style.left = `${place.left}px`;
      chevron.style.top = `${rect.y}px`;
      chevron.style.height = `${rect.height}px`;
    },
  };
}
