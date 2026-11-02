// SPDX-License-Identifier: Apache-2.0
// The editing-mode selector at the top right of the header (`109` UX-025).
//
// ---- WHY IT EXISTS ---------------------------------------------------------
//
// All three references put Editing / Suggesting / Viewing in the TOP chrome and
// none puts it in a status bar: ONLYOFFICE at the right of the tab row, Google
// Docs at the right of the toolbar ("Editing ▾"), Word at the right of the
// ribbon. It is the ONLY mode switch on screen (`docs/184` D6, `109` UX-054):
// the status bar's segmented control that used to be a second face of the same
// state was removed, because a mode shown twice reads as two settings. The
// other routes to the mode are not faces on screen: the palette's three
// `review.mode.*` rows, the menu bar's Mode group, Review ▸ Track changes and
// Ctrl+Shift+E (SKILL §10: every capability on at least two surfaces).
//
// ---- ONE STATE, ONE WRITER -------------------------------------------------
//
// The menu's rows carry `data-review-mode`, so `main.js` collects them as its
// `reviewModeButtons` and they ARE the published state: `setReviewMode` (the
// only writer of the mode) stamps `aria-checked` on them, and
// `reflectReviewModeAccess` stamps `aria-disabled` with the reason on `title`.
// A row's click is bound by `editor_review_decisions.mjs` to `setReviewMode`, so
// choosing a row runs the same read-only refusal, announcement and focus
// return as the palette. This module only
//
//   * READS the checked row to name the mode on the button (a
//     `MutationObserver`, not a poll and not a second copy of the rules), and
//   * closes the menu, in the CAPTURE phase, before the row's own click moves
//     focus to the document — and swallows a click on a refused row, so a
//     refused mode is never asked for.
//
// Nothing here decides whether a mode is allowed.
//
// ---- SHAPE ------------------------------------------------------------------
//
// Docs' shape: one button naming the mode in force, opening a three-row menu
// of `menuitemradio`s, each with a one-line description. The menu runs on the
// shared anchored-popover manager, so outside-click and Escape dismissal, focus
// return and "one popover at a time" are the same code every other menu uses.
// Keyboard: Enter/Space/ArrowDown open it on the checked row; ArrowUp/ArrowDown/
// Home/End move; Enter/Space choose; Escape closes; Tab closes and moves on.
//
// Complexity: O(1) per mutation and per key — three rows.
//
// The popover manager is PASSED IN rather than imported: it installs document
// listeners when it loads, and this module's rules (`readModeRows`,
// `nextRowIndex`) are unit-tested in Node, where there is no document.

/** The glyph each mode is drawn with — Docs' pencil / suggestion / eye. */
export const MODE_ICONS = Object.freeze({
  editing: "edit",
  suggesting: "rate_review",
  viewing: "visibility",
});

/** What the mode rows currently say, as plain data.
 *
 *  `rows` are the `[data-review-mode]` menu items. Pure apart from reading
 *  attributes, so the rule — "the checked row is the mode; with none checked it
 *  is Editing, which is `setReviewMode`'s own fallback" — is testable without a
 *  page. */
export function readModeRows(rows) {
  const read = [...rows].map((row) => ({
    mode: row.dataset?.reviewMode ?? row.getAttribute?.("data-review-mode"),
    checked: row.getAttribute("aria-checked") === "true",
    disabled: row.getAttribute("aria-disabled") === "true",
    reason: row.getAttribute("aria-disabled") === "true" ? row.getAttribute("title") || "" : "",
  }));
  const current = read.find((row) => row.checked)?.mode ?? "editing";
  return { current, rows: read };
}

/** Moves focus within the menu's rows: ArrowDown/ArrowUp wrap, Home/End jump.
 *  Returns the index to focus, or -1 when the key is not a menu key. */
export function nextRowIndex(key, at, count) {
  if (count === 0) return -1;
  switch (key) {
    case "ArrowDown":
      return at < 0 ? 0 : (at + 1) % count;
    case "ArrowUp":
      return at < 0 ? count - 1 : (at - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return -1;
  }
}

/**
 * Binds the header control to its menu's rows.
 *
 * Inert, returning `null`, when any of the markup is missing — a host shell
 * that composes the header away gets nothing rather than a throw
 * (`document_protection.mjs`'s pattern).
 *
 * @param {{button: HTMLElement|null, text: HTMLElement|null, icon: HTMLElement|null,
 *          menu: HTMLElement|null, registerPopover: Function}} parts — `registerPopover`
 *          is `popover_manager.mjs`'s, so this menu shares the one manager.
 * @returns {{sync: () => void, popover: object}|null}
 */
export function installHeaderModeMenu({ button, text, icon, menu, registerPopover }) {
  if (!button || !text || !menu || !registerPopover) return null;
  const items = [...menu.querySelectorAll("[data-review-mode]")];

  function sync() {
    const { current } = readModeRows(items);
    const checked = items.find((item) => item.dataset.reviewMode === current);
    const label = checked?.querySelector(".header-mode-item-label")?.textContent?.trim() ?? "";
    if (text.textContent !== label) text.textContent = label;
    if (icon) icon.textContent = MODE_ICONS[current] ?? MODE_ICONS.editing;
    button.dataset.mode = current;
  }

  const popover = registerPopover(button, menu, sync, { needsDocument: false });

  // Capture: this runs before the row's own click (`setReviewMode`), so the menu
  // is closed — `aria-expanded` and the keyboard's place restored — before the
  // mode change moves focus to the document. A refused row is still FOCUSABLE
  // (`aria-disabled`, so a keyboard user can land on it and hear the reason), and
  // its click stops here: nothing asks for a mode that was refused.
  menu.addEventListener(
    "click",
    (event) => {
      const item = event.target.closest?.("[data-review-mode]");
      if (!item || !items.includes(item)) return;
      event.preventDefault();
      if (item.getAttribute("aria-disabled") === "true") {
        event.stopPropagation();
        return;
      }
      popover.close();
    },
    true,
  );
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Tab") {
      // A menu is not a Tab stop of its own: Tab leaves it, the way it leaves a
      // native one. Closing returns focus to the trigger, and the browser's own
      // Tab then moves on from there.
      popover.close();
      return;
    }
    const at = items.indexOf(document.activeElement);
    const next = nextRowIndex(event.key, at, items.length);
    if (next < 0) return;
    event.preventDefault();
    items[next].focus();
  });

  button.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    // `click()` reports `detail === 0`, which is the popover manager's "opened
    // from the keyboard" signal: it focuses the checked row.
    if (menu.hidden) button.click();
  });

  // The rows are the state; observe the attribute that carries the mode. The
  // menu's own text is observed too, because a locale change rewrites the row
  // labels in place (`localizeTree`) and the button repeats one of them.
  const observer = new MutationObserver(sync);
  observer.observe(menu, { subtree: true, attributes: true, attributeFilter: ["aria-checked"], characterData: true, childList: true });
  sync();
  return { sync, popover };
}
