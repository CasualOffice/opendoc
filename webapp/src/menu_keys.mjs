// Arrow keys walk a menu; that is what `role="menu"` promises. One
// implementation for the small footer menus (the interface language, the PDF
// layout), so they cannot drift apart. Free of DOM globals at import, so a
// module that is unit-tested in Node can import it.

/**
 * Wires ArrowDown / ArrowUp (wrapping), Home and End on `list`, moving focus
 * among the rows matching `selector`. A no-op for a missing list.
 *
 * @param {HTMLElement|null|undefined} list the `role="menu"` element
 * @param {string} [selector] the rows, in order
 */
export function menuArrowKeys(list, selector = ".menu-item") {
  list?.addEventListener("keydown", (event) => {
    const step = { ArrowDown: 1, ArrowUp: -1, Home: 0, End: 0 }[event.key];
    if (step === undefined) return;
    const rows = [...list.querySelectorAll(selector)].filter((row) => !row.disabled);
    if (rows.length === 0) return;
    const at = rows.indexOf(document.activeElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : (at + step + rows.length) % rows.length;
    event.preventDefault();
    rows[next]?.focus();
  });
}
