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
// it is a rule about cannot be forgotten by a second caller. That is this lane's
// one unfinished thread and it is named in the report rather than implied.

import { t } from "./i18n.mjs";
import { foldCommands, pageCountCaveat, parseOutlineRows } from "./fold_view.mjs";

/**
 * Wires folding.
 *
 * @param {{
 *   getDoc: () => any,
 *   caretNode: () => string,
 *   onChanged: () => void,
 *   setStatus: (message: string, kind?: string) => void,
 *   chevronHost?: () => HTMLElement|null,
 *   caretRect?: () => ({x: number, y: number, height: number}|null),
 * }} deps
 */
export function createFoldChrome({
  getDoc,
  caretNode,
  onChanged,
  setStatus,
  chevronHost = () => null,
  caretRect = () => null,
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
    announce();
    onChanged();
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
     *  `main.js` to spread into its registry. */
    commands: () =>
      foldCommands({
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
      }),
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
     */
    syncBodyChevron: () => {
      const host = chevronHost();
      if (!host) return;
      let chevron = host.querySelector(".fold-body-chevron");
      const heading = caretHeading();
      const rect = caretRect();
      if (!heading || !rect || !state.available) {
        chevron?.remove();
        return;
      }
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
      chevron.style.left = `${rect.x}px`;
      chevron.style.top = `${rect.y}px`;
      chevron.style.height = `${rect.height}px`;
    },
  };
}
