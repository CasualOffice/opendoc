// Menu DOM: the icon table, the level helpers, and the row renderer for the
// editor context menu and its submenu flyouts. Everything that needs the open
// level stack — hover-to-open, activation, level teardown — arrives through the
// `hooks` argument, so this module renders rows and owns no menu state.

// Compact, currentColor stroke icons for primary rows and submenu parents. The
// icon gutter is always reserved so labels align whether or not a row has one —
// the same alignment Word and Google Docs use.
const menuIcon = (inner) =>
  `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${inner}</svg>`;
export const MENU_ICONS = {
  cut: menuIcon('<circle cx="4" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><path d="M5.3 10.7 13 3M10.7 10.7 3 3"/>'),
  copy: menuIcon('<rect x="5.5" y="5.5" width="8" height="8" rx="1.4"/><path d="M10.5 5.5V3.4A1.4 1.4 0 0 0 9.1 2H3.4A1.4 1.4 0 0 0 2 3.4v5.7A1.4 1.4 0 0 0 3.4 10.5h2.1"/>'),
  paste: menuIcon('<rect x="3.5" y="3" width="9" height="11" rx="1.4"/><path d="M6 3.2V2.4A1.1 1.1 0 0 1 7.1 1.3h1.8A1.1 1.1 0 0 1 10 2.4v.8z"/>'),
  link: menuIcon('<path d="M6.7 9.3 9.3 6.7"/><path d="M7.2 4.6 8.4 3.4a2.4 2.4 0 0 1 3.4 3.4L10.6 8"/><path d="M8.8 11.4 7.6 12.6a2.4 2.4 0 0 1-3.4-3.4L5.4 8"/>'),
  linkOpen: menuIcon('<path d="M9 3h4v4"/><path d="M13 3 7.5 8.5"/><path d="M11 9.5V12a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 12V6.5A1.5 1.5 0 0 1 4 5h2.5"/>'),
  comment: menuIcon('<path d="M2.5 3.5h11a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H7l-3 2.3V10.5H2.5a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z"/>'),
  accept: menuIcon('<path d="M3 8.5 6.3 12 13 4"/>'),
  reject: menuIcon('<path d="M4 4 12 12M12 4 4 12"/>'),
  format: menuIcon('<path d="M4 12.5 7.5 3.5h1L12 12.5M5.4 9.5h5.2"/>'),
  list: menuIcon('<path d="M6 4h8M6 8h8M6 12h8"/><path d="M2.7 4h.01M2.7 8h.01M2.7 12h.01"/>'),
  paragraph: menuIcon('<path d="M8.5 2.5H12M8.5 6H12M4 9.5H12M4 13H12M5 6.5A2.2 2.2 0 0 1 5 2.5h1.5v4"/>'),
  settings: menuIcon('<circle cx="8" cy="8" r="1.9"/><path d="M8 1.7v1.8M8 12.5v1.8M2.4 5.5l1.6.9M12 9.6l1.6.9M2.4 10.5l1.6-.9M12 6.4l1.6-.9"/>'),
  tableInsert: menuIcon('<path d="M2.5 6h7M2.5 10h4M6 2.5v8"/><path d="M11.5 8.5v5M9 11h5"/>'),
  tableDelete: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M6.2 6.2 9.8 9.8M9.8 6.2 6.2 9.8"/>'),
  tableSelect: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M2.5 6.5h11M6.5 2.5v11"/>'),
  tableLayout: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M2.5 8h11M8 2.5v11"/>'),
  wrap: menuIcon('<rect x="2.5" y="3" width="6" height="6" rx="1"/><path d="M10.5 4h3M10.5 7h3M2.5 11.5h11M2.5 13.5h11"/>'),
  altText: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1.4"/><path d="M5 10.5 7 5l2 5.5M5.6 9h2.8"/><path d="M10.5 5v5.5"/>'),
  crop: menuIcon('<path d="M4.5 1.5v10a1 1 0 0 0 1 1h9M1.5 4.5h10a1 1 0 0 1 1 1v9"/>'),
  delete: menuIcon('<path d="M3 4.5h10M6.5 4.5V3a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v1.5M5 4.5l.6 8a1 1 0 0 0 1 .95h2.8a1 1 0 0 0 1-.95l.6-8"/>'),
};

export function menuLevelItems(level) {
  return [...level.el.querySelectorAll(":scope > .menu-item")];
}

export function menuItemAt(level, index) {
  return menuLevelItems(level).find(
    (item) => Number(item.dataset.menuIndex) === index,
  ) ?? null;
}

export function focusMenuIndex(level, index, focus = true) {
  level.index = index;
  for (const item of menuLevelItems(level)) {
    const active = Number(item.dataset.menuIndex) === index;
    item.tabIndex = active ? 0 : -1;
    item.classList.toggle("active", active);
    if (active && focus) {
      item.focus({ preventScroll: true });
      item.scrollIntoView({ block: "nearest" });
    }
  }
}

export function renderMenuLevel(el, entries, depth, hooks) {
  el.replaceChildren();
  entries.forEach((entry, index) => {
    if (entry.separator) {
      const sep = document.createElement("div");
      sep.className = "menu-divider";
      sep.setAttribute("role", "separator");
      el.appendChild(sep);
      return;
    }
    const hasSub = Array.isArray(entry.submenu);
    const button = document.createElement("button");
    button.type = "button";
    button.className =
      `menu-item${entry.danger ? " danger" : ""}${hasSub ? " has-submenu" : ""}`;
    button.dataset.menuIndex = String(index);
    button.dataset.commandId = entry.id;
    button.setAttribute("role", "menuitem");
    button.disabled = entry.enabled === false;
    button.tabIndex = -1;
    if (hasSub) {
      button.setAttribute("aria-haspopup", "menu");
      button.setAttribute("aria-expanded", "false");
    }
    if (entry.disabledReason) button.title = entry.disabledReason;
    const icon = document.createElement("span");
    icon.className = "menu-item-icon";
    if (entry.icon && MENU_ICONS[entry.icon]) icon.innerHTML = MENU_ICONS[entry.icon];
    button.appendChild(icon);
    const label = document.createElement("span");
    label.className = "menu-item-label";
    label.textContent = entry.label;
    button.appendChild(label);
    if (hasSub) {
      const caret = document.createElement("span");
      caret.className = "menu-item-caret";
      caret.textContent = "›";
      button.appendChild(caret);
    } else if (entry.shortcut) {
      // Only the keyboard shortcut is ever shown on the right. Disabled rows are
      // greyed in place with no reason text (Google Docs convention) so the menu
      // never widens to fit an explanation; the reason stays as a hover title.
      const hint = document.createElement("span");
      hint.className = "menu-item-hint";
      hint.textContent = hooks.shortcutText(entry.shortcut);
      button.appendChild(hint);
    }
    button.addEventListener("mousemove", () => {
      if (button.disabled) return;
      hooks.onHover(depth, index, button, entry, hasSub);
    });
    button.addEventListener("click", (event) => {
      if (hasSub) event.stopPropagation();
      hooks.onActivate(depth, button, entry, hasSub);
    });
    el.appendChild(button);
  });
}
