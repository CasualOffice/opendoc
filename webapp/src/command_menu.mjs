// Rendering and driving a list of commands as rows: the compact chrome's menu
// bar, and the ribbon chrome's File page.
//
// These were one hand-written block inside `main.js` that only the menu bar
// used. Collapsing the editor's two competing navigation systems into one
// (`109` UX-014, docs/122) gave the same roster a SECOND rendering — the File
// tab's page — and two renderers of one command list is how the two surfaces
// would drift into offering different things. So the row builder lives here
// once and both callers use it, which is the same argument `INSERT_SURFACE`
// already makes for enablement: one declaration, one implementation, several
// faces.
//
// It is also the HF-085 direction: ~150 lines of behaviour that a test could
// previously reach only by loading the whole 17.8k-line module.
//
// Nothing here knows what a command DOES. The caller supplies the live command
// descriptors — `{id, label, shortcut, enabled, disabledReason, run}` — and the
// shortcut formatter, so gating and activation stay single-sourced in `main.js`
// with the state they read.

/**
 * Renders `sections` (arrays of command ids, one array per separator group) into
 * `host` as activatable rows, skipping any id the registry does not offer.
 *
 * A row carries its command id in `data-command` so a surface's membership is
 * readable from the DOM — which is what lets the reachability guard compare what
 * a surface OFFERS against the whole registry, rather than sampling labels.
 *
 * O(rows). `byId` is a Map so this never rescans the registry per id: the
 * registry is rebuilt per open and a linear `find` inside this loop would make
 * opening a menu O(commands²).
 *
 * @param {HTMLElement} host cleared and refilled.
 * @param {string[][]} sections id groups, in render order.
 * @param {Map<string, object>} byId the live registry, id → descriptor.
 * @param {object} options
 * @param {string} options.itemClass class for each row.
 * @param {string} [options.separatorClass] class for the rule between groups;
 *   omitted means no separators (the File page uses headings instead).
 * @param {(section: string[], index: number) => string|undefined} [options.headingFor]
 *   a heading to print above a group.
 * @param {(shortcut: string|undefined) => string} options.formatShortcut
 * @param {(command: object) => void} options.onRun invoked for an enabled row.
 * @returns {number} how many rows were rendered, so a caller can tell an empty
 *   surface from a full one without re-reading the DOM.
 */
export function renderCommandRows(host, sections, byId, options) {
  const { itemClass, separatorClass, headingFor, formatShortcut, onRun, rowFor } = options;
  host.replaceChildren();
  let rendered = 0;
  let groups = 0;
  sections.forEach((ids, index) => {
    // A section can carry rows that are not commands — the File page's category
    // rows, which select a pane instead of running one. A category row takes
    // the PLACE of the command it stands in for rather than being appended
    // after the section, so the two File surfaces list the same things in the
    // same order; `SKIP_ROW` is how one category row speaks for several
    // commands (Export, for the six formats behind it).
    const rows = [];
    for (const id of ids) {
      const replacement = rowFor?.(id, ids, index);
      if (replacement === SKIP_ROW) continue;
      if (replacement) {
        rows.push(replacement);
        continue;
      }
      const command = byId.get(id);
      if (command) rows.push(commandRow(command, { itemClass, formatShortcut, onRun }));
    }
    if (!rows.length) return;
    if (separatorClass && groups > 0) {
      const separator = document.createElement("div");
      separator.className = separatorClass;
      separator.setAttribute("role", "separator");
      host.appendChild(separator);
    }
    const heading = headingFor?.(ids, index);
    if (heading) {
      const h = document.createElement("h3");
      h.className = `${itemClass}-heading`;
      h.textContent = heading;
      host.appendChild(h);
    }
    groups += 1;
    for (const row of rows) {
      host.appendChild(row);
      rendered += 1;
    }
  });
  return rendered;
}

/** One row. Disabled rows keep their reason in `title`, which is the only
 *  channel a disabled button has — it takes no focus and fires no events. */
function commandRow(command, { itemClass, formatShortcut, onRun }) {
  const item = document.createElement("button");
  item.type = "button";
  item.className = itemClass;
  item.setAttribute("role", "menuitem");
  item.dataset.command = command.id;
  item.disabled = command.enabled === false;
  if (command.disabledReason) item.title = command.disabledReason;

  const label = document.createElement("span");
  label.className = `${itemClass}-label`;
  label.textContent = command.label;
  const hint = document.createElement("span");
  hint.className = `${itemClass}-hint`;
  hint.textContent = formatShortcut(command.shortcut);
  item.append(label, hint);
  item.addEventListener("click", () => {
    if (command.enabled === false) return;
    onRun(command);
  });
  return item;
}

/**
 * The compact chrome's menu bar: a row of buttons, one popover, and the
 * keyboard contract a menu bar owes (arrows within and between menus, Home/End,
 * Escape, hover-to-switch once open, light dismiss).
 *
 * Returned rather than wired as a side effect so `main.js` can hand over the
 * three things only it knows — the live registry, the taxonomy and the shortcut
 * formatter — and keep the state those read.
 *
 * @param {object} deps
 * @param {HTMLElement} deps.bar the `nav` holding `.app-menu-button`s.
 * @param {HTMLElement} deps.popover the shared popover element.
 * @param {(name: string) => string[][]} deps.sectionsFor the menu's taxonomy.
 * @param {() => Map<string, object>} deps.registry live descriptors by id.
 * @param {(shortcut: string|undefined) => string} deps.formatShortcut
 * @returns {{open: Function, close: Function, isOpen: () => boolean, activeMenu: () => string|null}}
 */
export function createMenuBar({ bar, popover, sectionsFor, registry, formatShortcut }) {
  const buttons = [...bar.querySelectorAll(".app-menu-button")];
  let activeMenu = null;
  let activeTrigger = null;

  const focusable = () => [...popover.querySelectorAll(".app-menu-item:not(:disabled)")];

  function position(trigger) {
    const rect = trigger.getBoundingClientRect();
    const viewportWidth = document.documentElement.clientWidth;
    const width = popover.offsetWidth;
    popover.style.left = `${Math.max(8, Math.min(rect.left, viewportWidth - width - 8))}px`;
    popover.style.top = `${rect.bottom + 4}px`;
  }

  function close({ restoreFocus = false } = {}) {
    if (popover.hidden) return;
    popover.hidden = true;
    for (const button of buttons) button.setAttribute("aria-expanded", "false");
    const trigger = activeTrigger;
    activeMenu = null;
    activeTrigger = null;
    if (restoreFocus) trigger?.focus({ preventScroll: true });
  }

  function open(name, { focusFirst = true } = {}) {
    const trigger = buttons.find((button) => button.dataset.menu === name);
    if (!trigger) return;
    for (const button of buttons) {
      button.setAttribute("aria-expanded", String(button === trigger));
    }
    activeMenu = name;
    activeTrigger = trigger;
    renderCommandRows(popover, sectionsFor(name), registry(), {
      itemClass: "app-menu-item",
      separatorClass: "app-menu-separator",
      formatShortcut,
      onRun: (command) => {
        const from = activeTrigger;
        close();
        from?.focus({ preventScroll: true });
        command.run();
      },
    });
    popover.setAttribute("aria-label", `${trigger.textContent.trim()} menu`);
    popover.hidden = false;
    position(trigger);
    if (focusFirst) focusable()[0]?.focus({ preventScroll: true });
  }

  const adjacent = (trigger, direction) => {
    const index = buttons.indexOf(trigger);
    return buttons[(index + direction + buttons.length) % buttons.length];
  };

  for (const button of buttons) {
    button.addEventListener("click", () => {
      if (!popover.hidden && activeMenu === button.dataset.menu) close({ restoreFocus: true });
      else open(button.dataset.menu);
    });
    button.addEventListener("pointerenter", () => {
      if (!popover.hidden && activeMenu !== button.dataset.menu) {
        open(button.dataset.menu, { focusFirst: false });
      }
    });
    button.addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        open(button.dataset.menu);
      } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault();
        const next = adjacent(button, event.key === "ArrowRight" ? 1 : -1);
        next.focus({ preventScroll: true });
        next.scrollIntoView({ inline: "nearest", block: "nearest" });
      } else if (event.key === "Escape") {
        close({ restoreFocus: true });
      }
    });
  }

  popover.addEventListener("keydown", (event) => {
    const items = focusable();
    const index = items.indexOf(document.activeElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      items[(index + direction + items.length) % items.length]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      items[event.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      const next = adjacent(activeTrigger, event.key === "ArrowRight" ? 1 : -1);
      next.scrollIntoView({ inline: "nearest", block: "nearest" });
      open(next.dataset.menu);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close({ restoreFocus: true });
    } else if (event.key === "Tab") {
      close();
    }
  });

  document.addEventListener("pointerdown", (event) => {
    if (!popover.hidden && !popover.contains(event.target) && !bar.contains(event.target)) close();
  });
  window.addEventListener("resize", () => close());

  return { open, close, isOpen: () => !popover.hidden, activeMenu: () => activeMenu };
}

/**
 * Fills the File page's content pane with the document's own information.
 *
 * ONLYOFFICE's File page is never empty: `FileMenu.js:390` opens it on the
 * Save-As pane when the document can be downloaded and on the Info pane
 * otherwise, and `FileMenu.js:414` builds that Info pane from the document.
 * We have no Save-As pane yet, so this is their fallback — and it is here
 * rather than left blank because three quarters of an empty window reads as a
 * broken page however good the reason for it.
 *
 * It takes the figures rather than computing them: the status bar already owns
 * that arithmetic, and a second count is a second answer to "how long is this
 * document". `info.format` is likewise the format CATALOGUE's label ("DOCX"),
 * not the internal uniform type identifier the document carries — the caller
 * resolves it through the same `formatInfo()` the save-format picker reads, so
 * the page and the picker cannot name one format two different ways.
 */
export function renderFilePageInfo(host, info) {
  if (!host) return;
  host.replaceChildren();
  if (!info) return;
  const title = document.createElement("h2");
  title.textContent = info.name || "Untitled document";
  const sub = document.createElement("p");
  sub.className = "file-detail-sub";
  sub.textContent = info.format || "";
  const list = document.createElement("dl");
  for (const [label, value] of info.rows ?? []) {
    if (value === undefined || value === null || value === "") continue;
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = String(value);
    list.append(dt, dd);
  }
  host.append(title, sub, list);
}

/**
 * A category row on the File page's rail — one that SELECTS a pane instead of
 * running a command.
 *
 * ONLYOFFICE's File page is a rail plus a pane, and the rail carries both
 * kinds: `Save`/`Print` act, while `Save as`, `Info` and `Advanced Settings`
 * select what the pane shows (`FileMenu.js:390` even picks which one it opens
 * on). Ours had no pane, so six export formats sat in the rail as six rows
 * where theirs has one.
 */
/** Returned by `rowFor` for a command another row already speaks for: the
 *  export formats behind the one `Export` row. It is a sentinel rather than a
 *  boolean so "no replacement" and "already covered" cannot be confused. */
export const SKIP_ROW = Symbol("skip row");

export function categoryRow({ id, label, itemClass, selected, onSelect, covers }) {
  const item = document.createElement("button");
  item.type = "button";
  item.className = `${itemClass} ${itemClass}-category`;
  item.dataset.filePane = id;
  // The commands this row stands in for. The two File surfaces render the same
  // roster differently — a dropdown runs each command, the page opens a pane —
  // and this is what lets a parity test prove they still offer the same things.
  if (covers?.length) item.dataset.covers = covers.join(" ");
  item.setAttribute("aria-pressed", String(!!selected));
  const text = document.createElement("span");
  text.className = `${itemClass}-label`;
  text.textContent = label;
  const chevron = document.createElement("span");
  chevron.className = "ms file-page-item-chevron";
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = "chevron_right";
  item.append(text, chevron);
  item.addEventListener("click", () => onSelect(id));
  return item;
}

/**
 * The Export pane: one tile per format the engine can write.
 *
 * ONLYOFFICE's Save As pane is a grid of format tiles (`filemenu.less`
 * `.format-items`), and it is the pane their File page opens on whenever the
 * document can be downloaded. This is that, with our formats.
 */
export function renderExportPane(host, formats, onExport) {
  host.replaceChildren();
  const title = document.createElement("h2");
  title.textContent = "Export";
  const sub = document.createElement("p");
  sub.className = "file-detail-sub";
  sub.textContent = "Choose a format to save a copy as.";
  const grid = document.createElement("div");
  grid.className = "file-format-grid";
  for (const format of formats) {
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "file-format-tile";
    tile.disabled = format.enabled === false;
    if (format.reason) tile.title = format.reason;
    // Icon plus name, which is what ONLYOFFICE's Save As tiles are. The
    // extension used to be a second text chip beside the name, so a PDF tile
    // read "PDF PDF" and every tile carried two labels for one format.
    const glyph = document.createElement("span");
    glyph.className = "ms file-format-icon";
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = format.icon || "draft";
    const name = document.createElement("span");
    name.className = "file-format-name";
    name.textContent = format.label;
    tile.append(glyph, name);
    tile.addEventListener("click", () => onExport(format));
    grid.appendChild(tile);
  }
  host.append(title, sub, grid);
}

/**
 * The New pane: one tile per starter document.
 *
 * ONLYOFFICE's Create New pane is a grid of document tiles with `Blank` first
 * (`FileMenuPanels.js:1318-1350`), so this is that grid — the same tiles the
 * Export pane uses, because they are the same thing: pick one, something
 * happens.
 */
/** Tile thumbnail width in CSS pixels. 132px against a 612pt page is a 0.216
 *  scale — close to the miniature Google Docs shows in its template gallery,
 *  and wide enough that a title reads as a title next to body text. */
const TEMPLATE_PREVIEW_WIDTH = 132;

/**
 * A miniature of the template's own first page: its real paragraphs at their
 * real point sizes on a real Letter sheet, scaled down as one block. Nothing
 * here is drawn by hand, so a template whose body changes previews differently
 * without anybody redrawing an icon.
 */
export function templatePreview(thumbnail, width = TEMPLATE_PREVIEW_WIDTH) {
  const scale = width / thumbnail.widthPt;
  const frame = document.createElement("span");
  frame.className = "file-template-page";
  frame.setAttribute("aria-hidden", "true");
  frame.style.width = `${width}px`;
  frame.style.height = `${Math.round(thumbnail.heightPt * scale)}px`;
  const sheet = document.createElement("span");
  sheet.className = "file-template-sheet";
  sheet.style.width = `${thumbnail.widthPt}px`;
  sheet.style.height = `${thumbnail.heightPt}px`;
  sheet.style.padding = `${thumbnail.marginPt}px`;
  sheet.style.transform = `scale(${scale})`;
  for (const block of thumbnail.blocks) {
    const line = document.createElement("span");
    line.className = "file-template-line";
    line.style.fontSize = `${block.sizePt}px`;
    line.style.marginTop = `${block.beforePt}px`;
    line.style.marginBottom = `${block.afterPt}px`;
    if (block.bold) line.style.fontWeight = "700";
    if (block.italic) line.style.fontStyle = "italic";
    // An empty paragraph still occupies a line on the page, and the preview is
    // wrong if the blank lines of a letter collapse away.
    line.textContent = block.text || "\u00a0";
    sheet.appendChild(line);
  }
  frame.appendChild(sheet);
  return frame;
}

export function renderTemplatePane(host, templates, onPick, thumbnailFor) {
  host.replaceChildren();
  const title = document.createElement("h2");
  title.textContent = "New document";
  const sub = document.createElement("p");
  sub.className = "file-detail-sub";
  sub.textContent = "Start from a blank page or a shape that is already laid out.";
  const grid = document.createElement("div");
  grid.className = "file-template-grid";
  for (const template of templates ?? []) {
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "file-format-tile file-template-tile";
    tile.dataset.templateId = template.id;
    const thumbnail = thumbnailFor?.(template.id);
    if (thumbnail) tile.appendChild(templatePreview(thumbnail));
    const text = document.createElement("span");
    text.className = "file-template-text";
    const name = document.createElement("span");
    name.className = "file-format-name";
    name.textContent = template.label;
    const description = document.createElement("span");
    description.className = "file-template-description";
    description.textContent = template.description ?? "";
    text.append(name, description);
    tile.appendChild(text);
    tile.addEventListener("click", () => onPick(template.id));
    grid.appendChild(tile);
  }
  host.append(title, sub, grid);
}
