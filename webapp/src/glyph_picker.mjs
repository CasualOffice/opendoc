// The Symbol and Emoji pickers.
//
// One builder for both: they are the same dialog — a tab strip, a searchable
// grid of glyphs, and an insert — differing only in which set they show and
// whether the tabs read as glyphs or as names. Two copies of this drifted once
// already (`docs/104` HF-045), which is why it is one function.
//
// It takes its three collaborators as arguments rather than reaching for them:
// what to do with a chosen glyph, whether editing is currently refused, and
// where focus goes afterwards. That is what lets the grid's filtering and
// keyboard model be tested without an engine behind it.

export function createGlyphPicker({
  dialogId,
  gridId,
  tabsId,
  searchId,
  emptyId,
  closeId,
  doneId,
  groups,
  tabsAreEmoji,
  insertGlyph,
  blockedInViewing,
  returnFocusToEditor,
  isDocumentOpen,
  triggerId,
}) {
  const dialog = document.getElementById(dialogId);
  const grid = document.getElementById(gridId);
  const tabs = document.getElementById(tabsId);
  const search = document.getElementById(searchId);
  const empty = document.getElementById(emptyId);
  const closeBtn = document.getElementById(closeId);
  const doneBtn = document.getElementById(doneId);
  if (!dialog || !grid || !tabs || !search) return { open: () => {} };

  let returnFocus = null;
  let activeGroup = 0;

  // One tab button per category. Emoji tabs show the category's glyph; symbol
  // tabs show the category name (which fits the narrower label).
  groups.forEach((group, index) => {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "glyph-tab";
    tab.dataset.groupIndex = String(index);
    tab.setAttribute("role", "tab");
    tab.title = group.name;
    tab.setAttribute("aria-label", group.name);
    tab.textContent = tabsAreEmoji ? group.icon : group.name;
    tab.addEventListener("click", () => {
      search.value = "";
      selectGroup(index);
    });
    tabs.append(tab);
  });

  function currentItems() {
    const query = search.value.trim().toLowerCase();
    if (query) {
      return groups
        .flatMap((group) => group.items)
        .filter(
          (item) => item.n.toLowerCase().includes(query) || item.c === query,
        );
    }
    return groups[activeGroup].items;
  }

  function renderGrid() {
    const items = currentItems();
    grid.replaceChildren();
    for (const item of items) {
      const cell = document.createElement("button");
      cell.type = "button";
      cell.className = "glyph-cell";
      cell.tabIndex = -1;
      cell.textContent = item.c;
      cell.title = tabsAreEmoji
        ? item.n
        : `${item.n} (U+${item.c.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")})`;
      cell.setAttribute("aria-label", item.n);
      cell.dataset.glyph = item.c;
      cell.addEventListener("click", () => insertGlyph(item.c));
      grid.append(cell);
    }
    const first = grid.querySelector(".glyph-cell");
    if (first) first.tabIndex = 0;
    grid.hidden = items.length === 0;
    if (empty) empty.hidden = items.length !== 0;
  }

  function selectGroup(index) {
    activeGroup = index;
    for (const tab of tabs.querySelectorAll(".glyph-tab")) {
      const on =
        Number(tab.dataset.groupIndex) === index && !search.value.trim();
      tab.classList.toggle("is-active", on);
      tab.setAttribute("aria-selected", String(on));
    }
    renderGrid();
  }

  // Roving-tabindex arrow navigation across the grid; Enter/Space fire the
  // button's own click, so insertion (keep-open) is shared with pointer use.
  grid.addEventListener("keydown", (event) => {
    const cells = [...grid.querySelectorAll(".glyph-cell")];
    if (!cells.length) return;
    const current = cells.indexOf(document.activeElement);
    if (current < 0) return;
    let next = -1;
    if (event.key === "ArrowRight") next = current + 1;
    else if (event.key === "ArrowLeft") next = current - 1;
    else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      // Columns = how many cells share the first cell's top edge.
      const top0 = cells[0].offsetTop;
      let cols = cells.findIndex((cell) => cell.offsetTop > top0);
      if (cols < 0) cols = cells.length;
      next = current + (event.key === "ArrowDown" ? cols : -cols);
    } else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = cells.length - 1;
    else return;
    if (next < 0 || next >= cells.length) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    cells[current].tabIndex = -1;
    cells[next].tabIndex = 0;
    cells[next].focus();
  });

  // A query flattens all categories into a filtered result set and clears the
  // active tab highlight; clearing it restores the current category.
  search.addEventListener("input", () => selectGroup(activeGroup));

  function close() {
    if (dialog.hidden) return;
    dialog.hidden = true;
    document.getElementById(triggerId)?.setAttribute("aria-expanded", "false");
    const to = returnFocus;
    returnFocus = null;
    if (to && typeof to.focus === "function" && document.contains(to))
      to.focus({ preventScroll: true });
    else returnFocusToEditor();
  }

  dialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  });
  closeBtn?.addEventListener("click", () => close());
  doneBtn?.addEventListener("click", () => close());

  function open() {
    // An open document is the only precondition — Word's Insert ▸ Symbol and
    // Docs' Insert ▸ Special characters are never gated on a prior click.
    if (!isDocumentOpen()) return;
    // Viewing is read-only; fail closed before opening (mirrors the link dialog)
    // so the picker never opens onto a dead insert. Suggesting is allowed — the
    // insert routes through the tracked suggestion path.
    if (blockedInViewing()) return;
    // Keep insert tools mutually exclusive in the document-side column. A
    // second picker must replace the first, never stack over the canvas or
    // recreate the old modal-overlay experience.
    const siblingId = tabsAreEmoji ? "symbolDialog" : "emojiDialog";
    const sibling = document.getElementById(siblingId);
    if (sibling && !sibling.hidden) sibling.hidden = true;
    returnFocus = document.activeElement;
    search.value = "";
    selectGroup(0);
    dialog.hidden = false;
    document.getElementById(triggerId)?.setAttribute("aria-expanded", "true");
    queueMicrotask(() => search.focus());
  }

  return { open };
}
