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
  const { itemClass, separatorClass, headingFor, formatShortcut, onRun } = options;
  host.replaceChildren();
  let rendered = 0;
  let groups = 0;
  sections.forEach((ids, index) => {
    const commands = ids.map((id) => byId.get(id)).filter(Boolean);
    if (!commands.length) return;
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
    for (const command of commands) {
      host.appendChild(commandRow(command, { itemClass, formatShortcut, onRun }));
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
