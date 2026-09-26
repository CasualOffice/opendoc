// One implementation of the WAI-ARIA radio-group pattern, for every segmented
// "pick exactly one" control in the editor (`109` UX-021).
//
// The defect this replaces: five containers declared `role="radiogroup"` while
// their children were `aria-pressed` toggle buttons — or, before the group had
// ever been reflected, buttons carrying no role and no state at all. A screen
// reader therefore announced "radio group, three items" and then "toggle
// button, not pressed", which is a direct contradiction, and the arrow keys did
// nothing because a radio group's keyboard contract was never implemented.
// Every group also hand-rolled its own "set aria-pressed on the clicked one,
// clear the others" loop, which is the same six lines written six times.
//
// The competitive standard, checked rather than assumed:
//
//   * ONLYOFFICE never mixes the two shapes. Where it declares a radio group it
//     owns real radios — `role="radio"` wrappers around
//     `<input type="radio" name=...>` (`common/main/lib/component/RadioBox.js:75-76`),
//     referenced by `aria-owns`
//     (`documenteditor/main/app/template/MailMerge.template:64`,
//     `documenteditor/main/app/view/FileMenuPanels.js:329`). Where a choice is a
//     mutually exclusive *formatting* state it uses plain `aria-pressed` toggle
//     buttons in a toggle group with NO radiogroup wrapper — that is what the
//     four alignment buttons are (`documenteditor/main/app/view/Toolbar.js:559-607`
//     `toggleGroup: 'alignGroup'`, pressed state written at
//     `common/main/lib/component/Button.js:637`).
//   * Word and Google Docs split the same way: a dialog choice (Page Setup ▸
//     Orientation) is a radio pair where Tab enters the group once and the
//     arrows move AND change the selection, while the ribbon's alignment
//     buttons are toggle buttons in a toolbar where the arrows move focus only
//     and nothing is highlighted when the selection is mixed.
//
// So the rule this module encodes, and the reason `#paraPanelAlign` is NOT
// bound here: a container is a radio group only when exactly one of its options
// is always true. Paragraph alignment over a multi-paragraph selection is
// tri-state (`aria-pressed="mixed"`), and `aria-checked="mixed"` is not legal on
// a radio, so that control stays a `role="group"` of toggle buttons — the
// Word/Docs/ONLYOFFICE shape for the same control.

import { rovingIndex } from "./ribbon_nav.mjs";

/** Where an arrow key lands inside a radio group, on either axis.
 *
 * A radio group is navigable with both pairs of arrows regardless of how it is
 * laid out (WAI-ARIA APG), unlike a toolbar, which owns one axis only. So the
 * vertical pair is folded onto the horizontal one and the band's own arithmetic
 * — including its wrapping and its `Home`/`End` — is reused rather than
 * reimplemented. Returns `null` for every key a radio group does not own, so
 * the caller leaves the event alone.
 */
export function radioIndex(key, index, count) {
  const folded = { ArrowUp: "ArrowLeft", ArrowDown: "ArrowRight" }[key] ?? key;
  return rovingIndex(folded, index, count);
}

/** Binds a container declared `role="radiogroup"` so that it is actually one.
 *
 * `attr` is the attribute carrying each option's value (`data-valign`,
 * `data-orientation`, …); its presence is also what marks an element as an
 * option, so decorative children inside the group are ignored. `onSelect` is
 * called with the chosen value when the *user* picks one — never from
 * `reflect`, so pushing document state into the control cannot loop back into
 * an edit.
 *
 * All O(options), which is two to four everywhere it is used.
 *
 * The returned handle is the only way callers should read or write the control:
 * the old call sites each queried `[aria-pressed="true"]` for the current
 * value, which is exactly the coupling that made the contradiction spread.
 */
export function bindRadioGroup(container, { attr, onSelect } = {}) {
  if (!container || !attr) return null;
  const optionSelector = `[${attr}]`;
  const options = () => [...container.querySelectorAll(optionSelector)];
  const optionAt = (event) => {
    const option = event.target?.closest?.(optionSelector);
    return option && container.contains(option) && !option.disabled ? option : null;
  };

  /** Publishes `value` as the checked option. ARIA only — no `onSelect`. */
  function reflect(value) {
    const items = options();
    let checked = null;
    for (const item of items) {
      const on = item.getAttribute(attr) === value;
      item.setAttribute("role", "radio");
      item.setAttribute("aria-checked", String(on));
      // `aria-pressed` means "toggle button". A radio that also claims to be a
      // toggle is the contradiction this module exists to remove, and it is
      // not an attribute `role="radio"` allows (axe `aria-allowed-attr`).
      item.removeAttribute("aria-pressed");
      item.tabIndex = on ? 0 : -1;
      if (on) checked = item;
    }
    // A group with nothing checked still needs exactly one Tab stop, or the
    // keyboard cannot reach it at all — which is the state every one of these
    // controls was in before its surface had been opened once.
    if (!checked && items.length) (items.find((i) => !i.disabled) ?? items[0]).tabIndex = 0;
  }

  /** The checked option, read from the DOM so the control has one source of
   *  truth, or `null` when it is showing no selection. */
  function selected() {
    return options().find((i) => i.getAttribute("aria-checked") === "true") ?? null;
  }

  /** The checked option's value, or `null` when nothing is selected. */
  function value() {
    return selected()?.getAttribute(attr) ?? null;
  }

  function select(value, { focus = false } = {}) {
    const item = options().find((i) => i.getAttribute(attr) === value);
    if (!item || item.disabled) return;
    reflect(value);
    if (focus) item.focus();
    onSelect?.(value, item);
  }

  container.addEventListener("click", (event) => {
    const option = optionAt(event);
    if (!option) return;
    // Matching `onButton`: a segmented control must not take the caret's
    // selection away from the document the way a plain button press would.
    event.preventDefault();
    select(option.getAttribute(attr));
  });
  container.addEventListener("mousedown", (event) => {
    if (optionAt(event)) event.preventDefault();
  });

  container.addEventListener("keydown", (event) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const items = options().filter((i) => !i.disabled);
    const next = radioIndex(event.key, items.indexOf(optionAt(event)), items.length);
    if (next === null) return;
    // In a radio group the arrows both move focus and change the selection, so
    // arrowing to an option chooses it — no second keypress, which is what
    // makes the group operable at all.
    event.preventDefault();
    select(items[next].getAttribute(attr), { focus: true });
  });

  // Adopt whatever the markup declared, at bind time. Without this the control
  // is only a correct radio group from the first time its surface reflects
  // document state into it — and five of these groups sat in the page with no
  // role, no checked state and no Tab stop at all until then, which is worse
  // than the contradiction the row names.
  reflect(
    options()
      .find(
        (i) =>
          i.getAttribute("aria-checked") === "true" || i.getAttribute("aria-pressed") === "true",
      )
      ?.getAttribute(attr) ?? null,
  );

  return {
    reflect,
    select,
    value,
    selected,
    options,
    /** The control's own Tab stop — what a surface should focus on open. */
    focusSelected() {
      const items = options();
      (items.find((i) => i.tabIndex === 0) ?? items[0])?.focus({ preventScroll: true });
    },
  };
}
