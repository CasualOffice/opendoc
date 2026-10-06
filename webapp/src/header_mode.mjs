// SPDX-License-Identifier: Apache-2.0
// The editing-mode selector at the top right of the header (`109` UX-025).
//
// ---- WHY IT EXISTS ---------------------------------------------------------
//
// All three references put Editing / Suggesting / Viewing in the TOP chrome and
// none puts it in a status bar: ONLYOFFICE at the right of the tab row, Google
// Docs at the right of the toolbar ("Editing ▾"), Word at the right of the
// ribbon. Ours lived only in the status bar (docs/123 §4.5 recorded the gap and
// left it, because the move touched the compact chrome). The status bar keeps
// its segmented control as the second surface — Word does put a view switcher
// there — and this adds the one a person coming from any of the three looks
// for first.
//
// ---- ONE STATE, TWO FACES -------------------------------------------------
//
// The established answer to "two controls for one piece of state" is a single
// source of truth with views over it, and the cheapest correct version of that
// here needs no new state at all: the status bar's segments ARE the published
// state. `setReviewMode` in `main.js` writes `aria-pressed` on every
// `[data-review-mode]` button and `reflectReviewModeAccess` writes `disabled`
// and the reason on the same buttons. So this module
//
//   * READS the mode, the availability and the reason off those attributes
//     (a `MutationObserver`, not a poll and not a second copy of the rules), and
//   * WRITES by pressing the segment — `button.click()` — so the change runs
//     through `setReviewMode` exactly as a click on the status bar does: the
//     same read-only refusal, the same announcement, the same focus return.
//
// Nothing here decides whether a mode is allowed. A disabled segment is a
// disabled row with the segment's own reason, and a click on it does nothing,
// because the segment would do nothing. That is what keeps UX-016's defect —
// two controls that can disagree — unrepresentable rather than merely avoided.
//
// It needs no edit to `main.js` (lane boundary, and `main.js` sits on a line
// ratchet): it binds to markup and to attributes `main.js` already maintains.
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
// listeners when it loads, and this module's rules (`readSegments`,
// `nextRowIndex`) are unit-tested in Node, where there is no document.

/** The glyph each mode is drawn with — Docs' pencil / suggestion / eye. */
export const MODE_ICONS = Object.freeze({
  editing: "edit",
  suggesting: "rate_review",
  viewing: "visibility",
});

/** What the status bar's segmented control currently says, as plain data.
 *
 *  `buttons` are the `[data-review-mode]` segments. Pure apart from reading
 *  attributes, so the rule — "the pressed segment is the mode; with none
 *  pressed it is Editing, which is `setReviewMode`'s own fallback" — is
 *  testable without a page. */
export function readSegments(buttons) {
  const rows = [...buttons].map((button) => ({
    mode: button.dataset?.reviewMode ?? button.getAttribute?.("data-review-mode"),
    pressed: button.getAttribute("aria-pressed") === "true",
    disabled: !!button.disabled,
    reason: button.disabled ? button.getAttribute("title") || "" : "",
  }));
  const current = rows.find((row) => row.pressed)?.mode ?? "editing";
  return { current, rows };
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
 * Binds the header control to the status bar's segments.
 *
 * Inert, returning `null`, when any of the markup is missing — a host shell
 * that composes the header or the status bar away gets nothing rather than a
 * throw (`document_protection.mjs`'s pattern).
 *
 * @param {{button: HTMLElement|null, text: HTMLElement|null, icon: HTMLElement|null,
 *          menu: HTMLElement|null, segments: HTMLElement|null,
 *          registerPopover: Function}} parts — `registerPopover` is
 *          `popover_manager.mjs`'s, so this menu shares the one manager.
 * @returns {{sync: () => void, popover: object}|null}
 */
export function installHeaderModeMenu({ button, text, icon, menu, segments, registerPopover }) {
  if (!button || !text || !menu || !segments || !registerPopover) return null;
  const items = [...menu.querySelectorAll("[data-header-mode]")];
  const segmentFor = (mode) => segments.querySelector(`[data-review-mode="${mode}"]`);

  function sync() {
    const { current, rows } = readSegments(segments.querySelectorAll("[data-review-mode]"));
    for (const item of items) {
      const row = rows.find((r) => r.mode === item.dataset.headerMode);
      item.setAttribute("aria-checked", String(item.dataset.headerMode === current));
      // `aria-disabled`, not `disabled`: a disabled button cannot take focus, and
      // the REASON is on this row's title — a keyboard user has to be able to
      // land on the row to be told why it is unavailable (SKILL §10).
      if (row?.disabled) {
        item.setAttribute("aria-disabled", "true");
        if (row.reason) item.setAttribute("title", row.reason);
        else item.removeAttribute("title");
      } else {
        item.removeAttribute("aria-disabled");
        item.removeAttribute("title");
      }
    }
    const checked = items.find((item) => item.dataset.headerMode === current);
    const label = checked?.querySelector(".header-mode-item-label")?.textContent?.trim() ?? "";
    if (text.textContent !== label) text.textContent = label;
    if (icon) icon.textContent = MODE_ICONS[current] ?? MODE_ICONS.editing;
    button.dataset.mode = current;
  }

  const popover = registerPopover(button, menu, sync, { needsDocument: false });

  function choose(item) {
    if (item.getAttribute("aria-disabled") === "true") return;
    const segment = segmentFor(item.dataset.headerMode);
    // Close first, through the manager, so `aria-expanded` and the keyboard's
    // place are restored before `setReviewMode` moves focus to the document.
    popover.close();
    // The status bar's own button, pressed. `setReviewMode` is the only writer of
    // the mode, and this is how it is reached without a second path into it.
    if (segment && !segment.disabled) segment.click();
  }

  for (const item of items) {
    item.addEventListener("click", (event) => {
      event.preventDefault();
      choose(item);
    });
  }

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

  // The segments are the state; observe exactly the three attributes that carry
  // it. The menu's own text is observed too, because a locale change rewrites
  // the row labels in place (`localizeTree`) and the button repeats one of them.
  const observer = new MutationObserver(sync);
  observer.observe(segments, {
    subtree: true,
    attributes: true,
    attributeFilter: ["aria-pressed", "disabled", "title"],
  });
  observer.observe(menu, { subtree: true, characterData: true, childList: true });
  sync();
  return { sync, popover };
}
