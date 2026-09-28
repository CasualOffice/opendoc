// The DOM half of the ribbon's colour and underline pickers: the four element
// factories the text-colour menu, the highlight menu and the underline menu are
// built out of.
//
// These came out of `main.js` under the line ratchet (`module_seams`), and they
// belong out here on their own account, the same way `review_chrome.mjs`'s card
// factories do: not one of them reads a piece of application state. Each takes
// values and returns a node, so what a swatch is NAMED — the thing a screen
// reader reads, and the thing a spec has to trust — is answerable without a
// browser. The menus that decide WHICH swatches to build, and what is active,
// stay in `main.js` with the state they depend on.

/** One swatch cell button. `value` is what gets applied (a hex for text, a named
 *  colour for highlight); `color` is the display hex; `active` lights it.
 *
 *  The label is both the tooltip and the accessible name, because a swatch has
 *  no text of its own and an unnamed colour button is a colour nobody can choose
 *  without a mouse and working eyes. */
export function makeSwatchCell(kind, value, color, label, active) {
  const cell = document.createElement("button");
  cell.type = "button";
  cell.className = "swatch-cell";
  cell.style.setProperty("--sw", color);
  cell.title = label;
  cell.setAttribute("aria-label", label);
  cell.dataset[kind === "text" ? "color" : "highlight"] = value;
  if (active) cell.classList.add("is-active");
  // White on white is invisible, so the cell carries its own border instead.
  if (color.toLowerCase() === "#ffffff") cell.classList.add("is-light");
  return cell;
}

/** A grid of swatch cells, grouped so a screen reader announces them as one set. */
export function makeSwatchGrid(cells) {
  const grid = document.createElement("div");
  grid.className = "swatch-grid";
  grid.setAttribute("role", "group");
  for (const cell of cells) grid.appendChild(cell);
  return grid;
}

/** A section heading inside a picker menu ("Standard colors", "Recent"). */
export function makeMenuHeading(text) {
  const h = document.createElement("div");
  h.className = "menu-heading";
  h.textContent = text;
  return h;
}

/**
 * (Re)builds a colour picker menu: the reset row, the standard swatches, the
 * recently-used row, and (for text) "More colours…".
 *
 * Everything that varies is an argument — which swatches, which one is active,
 * what the recents are, and how to turn a stored value into a colour and a label
 * — so the only thing left here is the SHAPE of the menu. That is what makes it
 * the same shape for text and for highlight, which is the whole reason the two
 * pickers read alike, and it is checkable without a browser.
 *
 * Complexity: O(swatches + recents).
 *
 * @param {Element} menu the popover to fill
 * @param {{kind: "text"|"highlight", active: string, swatches: Array, recents: string[],
 *   colorOf: (value: string) => string, labelOf: (value: string) => string,
 *   matches: (value: string) => boolean}} spec
 */
export function renderColorMenu(menu, { kind, swatches, recents, colorOf, labelOf, matches }) {
  menu.replaceChildren();
  const isText = kind === "text";
  // Automatic (text) / No colour (highlight) — the reset entry.
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "color-row-action";
  if (isText) {
    reset.dataset.auto = "1";
    reset.innerHTML = '<span class="color-chip" style="--sw:#000000"></span><span>Automatic</span>';
  } else {
    reset.dataset.highlight = "none";
    reset.innerHTML = '<span class="color-chip color-chip-none"></span><span>No color</span>';
  }
  menu.appendChild(reset);
  menu.appendChild(makeMenuHeading(isText ? "Standard colors" : "Highlight colors"));
  menu.appendChild(
    makeSwatchGrid(swatches.map((v) => makeSwatchCell(kind, v, colorOf(v), labelOf(v), matches(v)))),
  );
  if (recents.length) {
    menu.appendChild(makeMenuHeading("Recent"));
    menu.appendChild(
      makeSwatchGrid(recents.map((v) => makeSwatchCell(kind, v, colorOf(v), labelOf(v), matches(v)))),
    );
  }
  if (!isText) return;
  const more = document.createElement("button");
  more.type = "button";
  more.className = "color-row-action color-more";
  more.dataset.more = "1";
  more.innerHTML = '<span class="ms" aria-hidden="true">colorize</span><span>More colors…</span>';
  menu.appendChild(more);
}

/** One row of the underline-style radio group, drawn in the style it applies —
 *  the same reason the Styles list draws each row in its own style: the point of
 *  opening the menu is to SEE the difference between a dotted and a dashed rule. */
export function makeUnderlineStyleOption(style, label) {
  const option = document.createElement("button");
  option.type = "button";
  option.className = "color-row-action underline-style-option";
  option.dataset.underlineStyle = style;
  const preview = document.createElement("span");
  preview.className = `underline-style-preview underline-style-${style}`;
  preview.textContent = style === "none" ? "ab" : "Sample";
  option.append(preview, document.createTextNode(label));
  return option;
}
