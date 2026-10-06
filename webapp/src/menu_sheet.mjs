// The phone's menu bar, behind one button (docs/148 §5.3b; `109` HF-276).
//
// At the phone rung the header spent ~92px of an 844px window on a desktop menu
// bar wrapped onto two rows (File Edit View Insert Format Table / References
// Review), and with the docked command bar and the status bar under the page the
// chrome took ~40% of the screen. Google Docs' phone header is ONE row — the
// document and a handful of icons, with everything else behind an overflow
// control — and Word mobile and ONLYOFFICE mobile do the same. `docs/148` §5.3
// had argued the bar must stay on screen because "an axis may not be
// abbreviated"; this keeps the axis WHOLE — all eight names, each opening its
// own menu, nothing renamed or dropped — and moves where it is drawn, which is
// the part §5.3 conflated with abbreviating it.
//
// ---- The pattern, named first (SKILL.md §8) --------------------------------
//
// A DISCLOSURE (WAI-ARIA APG "Disclosure (Show/Hide)"): one button with
// `aria-expanded` and `aria-controls` revealing a region it does not replace.
// Not a second menu: the region is the existing `<nav id="appMenuBar">` and its
// eight existing `.app-menu-button`s, so `command_menu.mjs`'s `createMenuBar`
// keeps every behaviour it already had — which menu each opens, the rows, the
// submenus, the gating, the keyboard model — and this module only decides
// whether the nav is SHOWN. That is ADR-044's "one shell" rule: a phone paints
// the same surfaces arranged differently, and a second menu renderer would be a
// second answer to "what is in the Format menu".
//
// The sheet sits at the bottom, where every other phone sheet sits (`style.css`,
// "Panels, dialogs and menus are bottom sheets"), so tapping a name opens that
// menu as a sheet in the same place — the menus REPLACE the list rather than
// stacking a second card beside it. The list is hidden while a menu is open and
// comes back when that menu is dismissed with Escape, which returns focus to the
// name that opened it: a drill, the shape `docs/148` already gives a submenu.
//
// Dismissal follows the rules every popover here follows (`popover_manager.mjs`,
// `light-dismiss-contract.spec.mjs`): an outside press, Escape, focus leaving,
// and — because a menu closes before its command runs — the command itself.
//
// Wired from `compact_toolbar.mjs`, the compact chrome's module, rather than from
// `main.js`, which is under a line ratchet and a single owner; the calls it needs
// are four `getElementById`s, so the seam costs nothing to move.

/**
 * Makes `bar` a sheet that `trigger` shows and hides at the phone rung.
 *
 * Every listener is O(1): nothing here reads the document, the registry or the
 * menu rows, so it is off every per-keystroke path (`docs/107` §4).
 *
 * @param {object} deps
 * @param {HTMLElement|null} deps.trigger   the header's menus button
 * @param {HTMLElement|null} deps.bar       `#appMenuBar`, the eight menu names
 * @param {HTMLElement|null} deps.popover   `#appMenuPopover`, where a menu opens
 * @param {() => boolean} [deps.isPhone]    whether the rung is in force now
 * @param {Document} [deps.root]
 * @param {Window} [deps.view]
 * @returns {{open: Function, close: Function, isOpen: () => boolean} | null}
 */
export function createMenuSheet({
  trigger,
  bar,
  popover = null,
  isPhone = () => true,
  root = globalThis.document,
  view = globalThis,
}) {
  if (!trigger || !bar || !root) return null;
  const names = () => [...bar.querySelectorAll(".app-menu-button")];
  const isOpen = () => bar.classList.contains("is-open");
  const inside = (node) =>
    !!node && (bar.contains(node) || trigger.contains(node) || !!popover?.contains(node));

  function open({ focus = false } = {}) {
    bar.classList.add("is-open");
    trigger.setAttribute("aria-expanded", "true");
    // From the keyboard the list takes focus, as `openPopover` does for every
    // popover opened by keyboard (`docs/104` HF-070); by pointer it does not,
    // so a tap leaves the document's selection where it was.
    if (focus) names()[0]?.focus({ preventScroll: true });
  }

  /** Closing never strands the keyboard: focus inside the list goes back to the
   *  button that opened it, which is the reader's place. */
  function close({ restoreFocus = false } = {}) {
    if (!isOpen()) return;
    const held = bar.contains(root.activeElement);
    bar.classList.remove("is-open");
    trigger.setAttribute("aria-expanded", "false");
    if (restoreFocus || held) trigger.focus({ preventScroll: true });
  }

  trigger.addEventListener("click", (event) => {
    if (isOpen()) close({ restoreFocus: true });
    // `detail === 0` is a click with no pointer behind it — Enter or Space.
    else open({ focus: event.detail === 0 });
  });

  // Capture, so the list's own keys are decided before `createMenuBar`'s:
  // laid out as a column, Up and Down walk it (that bar treats Down as "open
  // this menu", which is right for a horizontal menubar and wrong for a list).
  // Enter, Space and Right still reach it, so opening a menu is unchanged.
  bar.addEventListener(
    "keydown",
    (event) => {
      if (!isOpen() || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
      const list = names();
      const at = list.indexOf(root.activeElement);
      const step = event.key === "ArrowDown" ? 1 : -1;
      list[(at + step + list.length) % list.length]?.focus({ preventScroll: true });
      event.preventDefault();
      event.stopPropagation();
    },
    true,
  );

  // Escape dismisses the list from wherever focus is — a tap leaves it on the
  // menus button, outside the list. CAPTURE on the document, so the menu's
  // state is read BEFORE `createMenuBar` acts on the same key: Escape inside an
  // open menu closes that menu and lands on its name in the list (one level
  // out, as a submenu's Escape does), and only an Escape with no menu open
  // closes the list itself.
  root.addEventListener(
    "keydown",
    (event) => {
      if (event.key !== "Escape" || !isOpen()) return;
      if (popover && !popover.hidden) return;
      close();
    },
    true,
  );

  // An outside press dismisses, on `pointerdown` like every other surface here
  // (#556). The open menu counts as inside: it was opened FROM the list.
  root.addEventListener("pointerdown", (event) => {
    if (isOpen() && !inside(event.target)) close();
  });

  // Focus leaving — Tab past the last name, or a dialog taking the screen.
  root.addEventListener("focusin", (event) => {
    if (isOpen() && !inside(event.target)) close();
  });

  // A command ran. `createMenuBar` closes its menu BEFORE running the command
  // (its `onRun`), so a click inside the menu that leaves it hidden is a command
  // that has run — and the list it was chosen from has done its job too. A
  // submenu parent keeps the menu open and is ignored here for that reason.
  // Bubble phase, so this hears the click after the row has acted on it.
  popover?.addEventListener("click", () => {
    if (isOpen() && popover.hidden) close();
  });

  // Growing past the rung paints the bar as a row again; a sheet left "open"
  // there would be a state nobody can see or leave.
  view.addEventListener?.("resize", () => {
    if (isOpen() && !isPhone()) close();
  });

  return { open, close, isOpen };
}
