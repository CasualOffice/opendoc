// The Arrange controls a selected object carries: Position, Bring forward /
// Send backward / Group / Ungroup, Rotate and flip.
//
// The RULES are `object_arrange.mjs`; this is the DOM. Separated because the
// rules are worth testing in node and the DOM is not, and kept out of `main.js`
// because the object bar is already the densest piece of chrome in the product.
//
// COMPETITIVE STANDARD. Word puts these on the Shape Format ▸ Arrange group and
// repeats Position and Wrap on the object's own Layout Options flyout; Google
// Docs puts wrap and position on a chip directly under the selected image. This
// host has the chip already, so the chip is where these go — with the same
// commands also on the object right-click menu, the Layout ribbon band and the
// command palette, because a capability on one surface is the defect this
// repository keeps re-finding (SKILL §10).
//
// Nothing here reads the document. Every menu is rendered from state the caller
// has already gathered for the selection it is describing, which is what keeps
// opening a menu O(1) rather than a walk per row.

/** Builds one menu row: an icon, a label, and an optional trailing check. */
function menuRow(icon, label, { checked = false, enabled = true, reason = "" } = {}) {
  const row = document.createElement("button");
  row.type = "button";
  row.className = "menu-item";
  row.setAttribute("role", "menuitem");
  if (!enabled) {
    row.disabled = true;
    // The reason rides on `title` AND on the accessible name, because a
    // disabled control that cannot say why is the dead control rule with extra
    // steps — and `title` alone is invisible to a screen reader and to touch.
    if (reason) {
      row.title = reason;
      row.setAttribute("aria-describedby", "");
      row.setAttribute("aria-label", `${label} — ${reason}`);
    }
  }
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  row.appendChild(glyph);
  const text = document.createElement("span");
  text.textContent = label;
  row.appendChild(text);
  if (checked) {
    const mark = document.createElement("span");
    mark.className = "menu-item-check";
    mark.setAttribute("aria-hidden", "true");
    mark.textContent = "✓";
    row.appendChild(mark);
    row.setAttribute("aria-checked", "true");
    row.setAttribute("role", "menuitemradio");
  }
  return row;
}

/**
 * Word's Position gallery: nine cells, each an alignment pair against the
 * margin, drawn as a miniature page with the object's place marked.
 *
 * A gallery rather than a pair of number fields, deliberately: the numeric route
 * already exists in the object properties panel, and "put it top-right" is a
 * pointing gesture in both competitors. The crop dialog is this repository's
 * standing example of getting that backwards.
 *
 * O(9).
 *
 * @param {HTMLElement} menu the popover element to fill
 * @param {{presets: object[], t: (key: string) => string, activeId: string|null,
 *          reason: string|null, onPick: (preset: object) => void}} io
 */
export function renderPositionGallery(menu, io) {
  menu.replaceChildren();
  const grid = document.createElement("div");
  grid.className = "object-position-grid";
  grid.setAttribute("role", "group");
  for (const preset of io.presets) {
    const label = io.t(preset.key);
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "object-position-cell";
    cell.setAttribute("role", "menuitemradio");
    cell.dataset.position = preset.id;
    cell.title = label;
    cell.setAttribute("aria-label", label);
    cell.setAttribute("aria-checked", String(preset.id === io.activeId));
    if (io.reason) {
      cell.disabled = true;
      cell.title = io.reason;
      cell.setAttribute("aria-label", `${label} — ${io.reason}`);
    }
    // The miniature: a page with three text rules and a filled block where the
    // object would land. Built as elements, never as markup with a token in it.
    const sheet = document.createElement("span");
    sheet.className = "object-position-sheet";
    sheet.setAttribute("aria-hidden", "true");
    const mark = document.createElement("span");
    mark.className = `object-position-mark pos-${preset.v}-${preset.h}`;
    sheet.appendChild(mark);
    cell.appendChild(sheet);
    grid.appendChild(cell);
  }
  menu.appendChild(grid);
  return grid;
}

/**
 * Word's Arrange menu: the four stacking commands, then Group and Ungroup.
 *
 * Group's enablement is the ENGINE's `canGroupObjects` verdict, and its reason
 * is the engine's own sentence — this host does not re-derive "they must be on
 * the same page", because it would drift the day that rule changed.
 *
 * O(6).
 *
 * @param {HTMLElement} menu
 * @param {object} io
 */
export function renderArrangeMenu(menu, io) {
  menu.replaceChildren();
  const icons = { front: "flip_to_front", forward: "arrow_upward", backward: "arrow_downward", back: "flip_to_back" };
  for (const choice of io.zChoices) {
    const row = menuRow(icons[choice.value] ?? "layers", io.t(choice.key), {
      enabled: io.zOrder.enabled,
      reason: io.zOrder.reason,
    });
    row.dataset.zOrder = choice.value;
    if (io.zOrder.enabled) row.addEventListener("click", () => io.onZOrder(choice.value));
    menu.appendChild(row);
  }
  const rule = document.createElement("div");
  rule.className = "menu-separator";
  rule.setAttribute("role", "separator");
  menu.appendChild(rule);

  const group = menuRow("workspaces", io.t("object.group"), {
    enabled: io.group.enabled,
    reason: io.group.reason,
  });
  group.dataset.arrange = "group";
  if (io.group.enabled) group.addEventListener("click", () => io.onGroup());
  menu.appendChild(group);

  const ungroup = menuRow("workspaces_outline", io.t("object.ungroup"), {
    enabled: io.ungroup.enabled,
    reason: io.ungroup.reason,
  });
  ungroup.dataset.arrange = "ungroup";
  if (io.ungroup.enabled) ungroup.addEventListener("click", () => io.onUngroup());
  menu.appendChild(ungroup);
  return menu;
}

/**
 * Word's Rotate menu: right 90, left 90, flip vertical, flip horizontal — the
 * four both competitors ship, and the same four in the same order.
 *
 * O(4).
 */
export function renderRotateMenu(menu, io) {
  menu.replaceChildren();
  const icons = {
    right90: "rotate_right",
    left90: "rotate_left",
    flipV: "flip",
    flipH: "flip",
  };
  for (const choice of io.choices) {
    const row = menuRow(icons[choice.value] ?? "rotate_right", io.t(choice.key), {
      enabled: io.enabled,
      reason: io.reason,
    });
    row.dataset.rotate = choice.value;
    if (io.enabled) row.addEventListener("click", () => io.onPick(choice.value));
    menu.appendChild(row);
  }
  return menu;
}
