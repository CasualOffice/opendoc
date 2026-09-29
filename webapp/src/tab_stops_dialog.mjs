// Format ▸ Paragraph ▸ Tab stops — the second surface for a capability that had
// exactly one (`docs/148` §5.3 and §9 item 7; `docs/105` UX-004).
//
// THE DEFECT. `ruler.mjs` held both call sites of `setTabStop` and the only call
// sites of `moveTabStop` and `removeTabStop`. Tab stops therefore had no command
// id, no menu row, no palette row and no keyboard route: dragging a 24px strip
// with a pointer was the whole of the product's answer, at 1280px as much as at
// 390px. SKILL §10's "every capability reachable from ≥2 surfaces" is the rule it
// broke, and `docs/148` §5.3 records the consequence — the ruler could not be
// withheld on a phone without taking tab stops off the device class.
//
// THE STANDARD IS WORD'S, and is quoted before any code (SKILL §8): Home ▸
// Paragraph launcher ▸ Tabs…, a list of the paragraph's stops, a position field,
// an alignment choice and Set / Clear / Clear all. LibreOffice ships the same
// dialog. Google Docs has no dialog at all — its only surface is the ruler too —
// so Docs does not outvote Word on whether one should exist.
//
// WHAT WORD OFFERS AND THIS DELIBERATELY DOES NOT. Each is absent, never present
// and inert (`docs/63`; SKILL §10 "never a dead control"):
//
//   * DEFAULT TAB STOPS. `w:defaultTabStop` is modelled and honoured by
//     `casual-doc-layout/src/tabs.rs`, and `casual-doc-wasm` exposes no reader and
//     no writer. A stepper over it would display an invented number and write
//     nowhere.
//   * LEADER (none / dots / dashes / underline). `TabLeader` is modelled and the
//     layout engine draws it, but `setTabStop` takes no leader argument — it only
//     PRESERVES a leader already on the stop — and `paragraphTabs` answers
//     `[position, alignment]` pairs with no leader in them. Nothing to show, no
//     way to set.
//   * "TAB STOPS TO BE CLEARED". Word's read-out of pending clears exists because
//     Word defers everything to OK. Every action here applies immediately as its
//     own undoable step, so nothing is ever pending — which is also the same
//     shape the ruler already has, and one mechanism beats two (SKILL §8).
//
// BAR IS OFFERED, because the engine has it: alignment code 4 reaches
// `TabAlignment::Bar` and the layout engine draws the rule. It is one of the gaps
// SKILL §1 records against ONLYOFFICE, and until this dialog existed it was a
// capability with no way in at all.
//
// APPLY-NOW, NOT APPLY-ON-OK. Word batches the session and applies it when OK is
// pressed. The engine has no batch tab-stop operation, so deferring would turn
// one OK into N `runToolbarEdit` calls and N undo steps — strictly worse than
// Word for the one thing undo is for. Each button here is one edit over the
// selected paragraphs, and Clear all is `clearTabStops`, which empties the list
// in a single `apply_paragraph_props` (see `clearAllCalls` below).
//
// COMPLEXITY. `paragraphTabs` resolves a node id through `paragraph_properties`,
// which is a linear scan of the document (SKILL §8). It is called ONCE per open
// and once per applied action — never inside a loop over stops, which is the
// shape that rule exists to refuse. Everything else here is O(stops), and a
// paragraph's tab stops number in the single digits.

import { t } from "./i18n.mjs";
import { bindRadioGroup } from "./radio_group.mjs";

/** The alignments this dialog offers, in Word's order, with the codes
 *  `setTabStop`/`paragraphTabs` speak (`tab_alignment_from_code` in
 *  `casual-doc-wasm/src/lib.rs`).
 *
 *  `clear` (code 5) is not here: it suppresses an inherited stop rather than
 *  placing one, it has no position of its own to type, and Word does not offer
 *  it either. */
export const ALIGNMENTS = Object.freeze([
  Object.freeze({ value: "start", code: 0 }),
  Object.freeze({ value: "center", code: 1 }),
  Object.freeze({ value: "end", code: 2 }),
  Object.freeze({ value: "decimal", code: 3 }),
  Object.freeze({ value: "bar", code: 4 }),
]);

/** Word's own ceiling for a tab stop, in inches, and the same one `editor.html`
 *  gives the page-size fields. A position past the paper is not a position. */
export const MAX_POSITION_INCHES = 22;

const CODE_TO_VALUE = new Map(ALIGNMENTS.map(({ value, code }) => [code, value]));
const VALUE_TO_CODE = new Map(ALIGNMENTS.map(({ value, code }) => [value, code]));

/**
 * A typed position → twips, or a reason it is not one.
 *
 * DELIBERATELY NOT `inchesToTwips`. That helper answers 0 for blank AND for
 * "abc" — correct for a margin box, where an empty field genuinely means no
 * margin, and wrong here: silently turning a typo into a stop at the left margin
 * is the "clamped to something the user did not ask for" failure. Three
 * outcomes, three different sentences.
 *
 * `twipsPerInch` is injected rather than imported so the arithmetic is testable
 * against a scale and the module keeps one source for the factor.
 *
 * @returns {{twips: number}|{error: "needPosition"|"badPosition"|"outOfRange"}}
 */
export function parsePosition(text, { twipsPerInch, maxInches = MAX_POSITION_INCHES } = {}) {
  const raw = String(text ?? "").trim();
  if (raw === "") return { error: "needPosition" };
  const inches = Number(raw);
  if (!Number.isFinite(inches)) return { error: "badPosition" };
  if (inches < 0 || inches > maxInches) return { error: "outOfRange" };
  return { twips: Math.round(inches * twipsPerInch) };
}

/**
 * `paragraphTabs`'s flat `[pos, code, pos, code, …]` → `{position, align}` rows,
 * ascending, one row per position.
 *
 * The engine sorts and de-duplicates on write, so on a well-formed document this
 * is a re-shape. It sorts and de-duplicates anyway because an IMPORTED document
 * is not written by us: `w:tabs` in a DOCX can carry two `w:tab` elements at one
 * position, and a list that shows the same inch twice is a list a user cannot
 * act on. The LAST entry at a position wins, matching `setTabStop`'s own
 * retain-then-push.
 */
export function stopsFromFlat(flat) {
  const byPosition = new Map();
  for (let i = 0; i + 1 < (flat?.length ?? 0); i += 2) {
    const position = flat[i];
    const code = flat[i + 1];
    byPosition.set(position, { position, align: CODE_TO_VALUE.get(code) ?? "start" });
  }
  return [...byPosition.values()].sort((a, b) => a.position - b.position);
}

/**
 * The list AFTER a Set, without asking the engine: the stop is added, or the one
 * already at that exact position is retyped, and the order is kept.
 *
 * This is what makes the dialog show the truth between the click and the
 * repaint, and it is the same rule `setTabStop` applies on the engine side —
 * written here so a node test can hold the two to the same behaviour.
 */
export function withStop(stops, position, align) {
  const next = stops.filter((stop) => stop.position !== position);
  next.push({ position, align });
  return next.sort((a, b) => a.position - b.position);
}

/**
 * The engine calls Clear all becomes — ONE, whatever the paragraph holds.
 *
 * `clearTabStops` empties `p.tabs` over the whole selection inside a single
 * `apply_paragraph_props`, so one press of undo puts every stop back. Removing
 * them one at a time would be N edits and N undo steps for one button, which is
 * the defect this returns a plan for rather than looping in place.
 */
export function clearAllCalls(stops) {
  return stops.length === 0 ? [] : [{ op: "clearTabStops" }];
}

/** The alignment code the engine wants for one of `ALIGNMENTS`' values. */
export function codeFor(align) {
  return VALUE_TO_CODE.get(align) ?? 0;
}

/**
 * Mounts the Tab stops dialog over the markup already in `editor.html`.
 *
 * Every dependency is injected; nothing here reaches for the application, and no
 * DOM lookup happens outside this factory, so a second editor on a page gets its
 * own dialog rather than sharing one.
 *
 *   `getDoc()`             the open document, or null
 *   `getSelection()`       the live selection, or null
 *   `runToolbarEdit(cb, o)` the gated, repainting, undoable edit path — the same
 *                          one `ruler.mjs` uses, so a dialog change and a ruler
 *                          drag are the same kind of edit
 *   `registerModal(el, o)` the host's modal registry (focus, Escape, backdrop)
 *   `setStatus(text, kind)` the status line, for an engine refusal
 *   `fallbackFocus()`      where focus goes when the opener has gone away
 *   `twipsPerInch`         the scale the position field is typed in
 *   `formatInches(twips)`  twips → the text a field shows (`units.mjs`)
 */
export function createTabStopsDialog({
  getDoc,
  getSelection,
  runToolbarEdit,
  registerModal,
  setStatus,
  fallbackFocus,
  twipsPerInch,
  formatInches,
}) {
  const el = (id) => document.getElementById(id);
  const dialog = el("tabStopsDialog");
  if (!dialog) return { open() {}, close() {}, isOpen: () => false };

  const form = el("tabStopsForm");
  const list = el("tabStopsList");
  const empty = el("tabStopsEmpty");
  const positionInput = el("tabStopsPosition");
  const alignSeg = el("tabStopsAlign");
  const note = el("tabStopsNote");
  const clearBtn = el("tabStopsClear");
  const clearAllBtn = el("tabStopsClearAll");
  const closeBtn = el("tabStopsClose");
  const doneBtn = el("tabStopsDone");

  /** The caret paragraph's stops, read once per open and once per edit. */
  let stops = [];
  /** The position of the row the list is on, or null. Positions are unique per
   *  `stopsFromFlat`, so a position IS the row's identity — no index that a
   *  repaint could invalidate. */
  let selectedPosition = null;

  const alignGroup = bindRadioGroup(alignSeg, { attr: "data-tabalign" });

  /** The dispatch a plan entry names (see `clearAllCalls`).
   *
   *  Both operations are here although Clear all uses only the first, so that a
   *  plan naming the other would RUN — and be caught by the undo-step guard,
   *  which is the property that matters — rather than throwing somewhere the
   *  guard cannot read. */
  const PLAN_OPS = {
    clearTabStops: () => (a, b, c, d) => getDoc().clearTabStops(a, b, c, d),
    removeTabStop: (call) => (a, b, c, d) => getDoc().removeTabStop(a, b, c, d, call.position),
  };

  /** An alignment's name, read from the control that already shows it.
   *
   *  NOT a second set of catalogue keys. The five words are declared once, in
   *  the markup, where their English sits beside the key and `localize.mjs` has
   *  already replaced them by the time anything is painted — so the list and the
   *  buttons cannot drift into two vocabularies for one choice, and a row can
   *  never paint a raw key the way `captionDialog.headingLevel` did. */
  function alignLabel(align) {
    return alignSeg.querySelector(`[data-tabalign="${align}"]`)?.textContent?.trim() ?? align;
  }

  const modal = registerModal(dialog, {
    initialFocus: () => positionInput,
    fallbackFocus,
    onClose: () => {
      note.textContent = "";
    },
  });

  /** The caret's paragraph node, or "" when there is no caret to read. */
  function caretNode() {
    return getSelection()?.focus.node ?? "";
  }

  function say(key) {
    note.textContent = key ? t(key) : "";
  }

  /** One engine read. O(document) through `paragraph_properties`, once. */
  function readStops() {
    const doc = getDoc();
    const node = caretNode();
    if (!doc || !node) return [];
    try {
      return stopsFromFlat(doc.paragraphTabs(node));
    } catch (error) {
      console.warn("tab stops read ignored:", error?.message ?? error);
      setStatus(error?.message ?? String(error), "error");
      return [];
    }
  }

  /** Publishes which row is chosen, WITHOUT rebuilding the list.
   *
   *  Separate from `reflect` because the first version of this rebuilt on every
   *  selection change, which destroyed the button the keyboard was standing on:
   *  ArrowDown chose the next row and then dropped focus onto `<body>`, so the
   *  list was operable with a pointer and not without one. Found by the spec,
   *  not by reading. */
  function markSelection() {
    for (const button of list.querySelectorAll("[data-position]")) {
      const chosen = Number(button.dataset.position) === selectedPosition;
      button.setAttribute("aria-selected", String(chosen));
      button.classList.toggle("is-active", chosen);
    }
    // Never a dead control: each refusal carries its reason as a tooltip, in the
    // reader's language, and the two reasons are different because the ways out
    // of them are different — choose a row, versus there are no rows.
    const selected = stops.some((stop) => stop.position === selectedPosition);
    clearBtn.disabled = !selected;
    clearBtn.title = selected ? "" : t(stops.length ? "tabStops.selectStop" : "tabStops.noneToClear");
    clearAllBtn.disabled = stops.length === 0;
    clearAllBtn.title = stops.length === 0 ? t("tabStops.noneToClear") : "";
  }

  /** Repaints the list from `stops`. O(stops). */
  function reflect() {
    list.replaceChildren();
    for (const stop of stops) {
      const row = document.createElement("li");
      row.className = "bookmark-row";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "bookmark-goto";
      button.dataset.position = String(stop.position);
      button.setAttribute("role", "option");
      // ONE string, not a position concatenated with a word: the two swap order
      // in several languages and the unit belongs inside the sentence a
      // translator sees. RTL falls out of that rather than being arranged here.
      button.textContent = t("tabStops.rowLabel", {
        position: formatInches(stop.position),
        align: alignLabel(stop.align),
      });
      row.append(button);
      list.append(row);
    }
    empty.hidden = stops.length > 0;
    list.hidden = stops.length === 0;
    markSelection();
  }

  /** Puts a row's values into the fields, the way Word's list does. */
  function selectRow(position) {
    selectedPosition = position;
    const stop = stops.find((candidate) => candidate.position === position);
    if (stop) {
      positionInput.value = formatInches(stop.position);
      alignGroup?.reflect(stop.align);
    }
    say("");
    markSelection();
  }

  /** Re-reads the paragraph after an edit and keeps the row the user was on if
   *  it is still there. */
  function refresh(keepPosition = selectedPosition) {
    stops = readStops();
    selectedPosition = stops.some((stop) => stop.position === keepPosition) ? keepPosition : null;
    reflect();
  }

  /** One gated, undoable edit over the selected paragraphs, then a re-read.
   *  `paragraphLevel: true` is what makes a multi-paragraph selection one step
   *  and what routes the change through the suggestion gate. */
  async function apply(call, keepPosition) {
    if (!getDoc() || !getSelection()) return;
    await runToolbarEdit(call, { paragraphLevel: true });
    refresh(keepPosition);
  }

  async function set() {
    const parsed = parsePosition(positionInput.value, { twipsPerInch });
    if (parsed.error) {
      say(`tabStops.${parsed.error}`);
      positionInput.focus();
      return;
    }
    const align = alignGroup?.value() ?? "start";
    say("");
    // Shown before the engine answers, and from the same rule the engine
    // applies, so the list never lags a click behind the ruler.
    stops = withStop(stops, parsed.twips, align);
    selectedPosition = parsed.twips;
    reflect();
    await apply(
      (a, b, c, d) => getDoc().setTabStop(a, b, c, d, parsed.twips, codeFor(align)),
      parsed.twips,
    );
  }

  async function clear() {
    if (selectedPosition === null) return;
    const position = selectedPosition;
    await apply((a, b, c, d) => getDoc().removeTabStop(a, b, c, d, position), null);
  }

  async function clearAll() {
    // The PLAN drives the dispatch, so "one undo step" is a property of
    // `clearAllCalls` that a node test can hold and a mutation can break.
    for (const call of clearAllCalls(stops)) {
      await apply(PLAN_OPS[call.op](call), null);
    }
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void set();
  });
  clearBtn.addEventListener("click", () => void clear());
  clearAllBtn.addEventListener("click", () => void clearAll());
  closeBtn.addEventListener("click", () => modal.close());
  doneBtn.addEventListener("click", () => modal.close());

  list.addEventListener("click", (event) => {
    const button = event.target?.closest?.("[data-position]");
    if (!button || !list.contains(button)) return;
    event.preventDefault();
    selectRow(Number(button.dataset.position));
    button.focus();
  });

  // The listbox's own keyboard contract: the arrows move between options and
  // choose as they go, which is what makes the list operable without a pointer.
  list.addEventListener("keydown", (event) => {
    const buttons = [...list.querySelectorAll("[data-position]")];
    const at = buttons.indexOf(document.activeElement);
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step || at === -1) return;
    event.preventDefault();
    const next = buttons[Math.min(buttons.length - 1, Math.max(0, at + step))];
    if (!next) return;
    next.focus();
    selectRow(Number(next.dataset.position));
  });

  /** Opens on the caret's paragraph. Refuses silently only when the caller had
   *  no business calling — the COMMAND carries the reason and is disabled
   *  without a caret, which is where a user reads it. */
  function open() {
    if (!getDoc() || !caretNode()) return;
    stops = readStops();
    selectedPosition = null;
    positionInput.value = "";
    alignGroup?.reflect(stops[0]?.align ?? "start");
    say("");
    reflect();
    modal.open();
  }

  return {
    open,
    close: () => modal.close(),
    isOpen: () => modal.isOpen,
  };
}
