const VIEWPORT_INSET = 8;

export function clampContextMenuPosition(
  x,
  y,
  width,
  height,
  viewportWidth,
  viewportHeight,
  inset = VIEWPORT_INSET,
) {
  return {
    left: Math.max(inset, Math.min(x, viewportWidth - width - inset)),
    top: Math.max(inset, Math.min(y, viewportHeight - height - inset)),
  };
}

export function normalizeMenuEntries(commands) {
  const entries = [];
  let previousGroup = null;
  for (const command of commands.filter((candidate) => candidate.visible !== false)) {
    if (previousGroup !== null && command.group !== previousGroup) {
      entries.push({ separator: true });
    }
    entries.push(command);
    previousGroup = command.group;
  }
  return entries;
}

/** The rows the keyboard can land on: not separators, not notes (a note is
 *  text the menu says, not a command), not disabled rows. */
export function enabledMenuIndexes(entries) {
  return entries.flatMap((entry, index) =>
    !entry.separator && !entry.note && entry.enabled !== false ? [index] : []);
}

export function moveMenuIndex(entries, current, direction) {
  const enabled = enabledMenuIndexes(entries);
  if (!enabled.length) return -1;
  const position = enabled.indexOf(current);
  if (direction === "first") return enabled[0];
  if (direction === "last") return enabled.at(-1);
  if (position < 0) return direction === -1 ? enabled.at(-1) : enabled[0];
  return enabled[(position + direction + enabled.length) % enabled.length];
}

// ---- The short-menu policy (docs/184 D-12, audit CANVAS-07, CRITIC-09) --------
//
// A right-click menu is a SHORT list of what applies to the thing under the
// pointer. Measured before this policy: 36 rows and 784px of an 800px window
// on a table cell, with Undo/Redo greyed at the top and nine clipboard rows
// before the first table command. Word's table menu is about 14 entries and
// Docs' about 15; neither shows Undo/Redo, and neither lists a command it
// cannot run there.
//
// The policy is three rules applied to every context menu at ONE choke point
// (`editor_menu_render.mjs` `showContextMenu`), so no builder can opt out:
//
//   1. History is not a context command. Undo/Redo stay on the toolbar, the
//      Edit menu, the palette and their chords.
//   2. A top-level row that cannot run here is HIDDEN, not greyed. Inside a
//      submenu a greyed row with its reason stays, because there it is one of a
//      family the reader opened on purpose (Word greys there too). When several
//      hidden rows share ONE reason (the review mode, a read-only container) the
//      menu leads with that reason as a note, so a refusal still says something
//      (SKILL §10).
//   3. A budget: at most CONTEXT_MENU_BUDGET.items rows and .heightPx pixels.
//      Over it, the trailing groups fold into one "More" submenu, so the
//      guarantee holds for any builder, a plug-in's contributions included.

/** The ceiling every context menu is held to (`context-menu.spec.mjs` asserts
 *  it against the rendered menu). */
export const CONTEXT_MENU_BUDGET = Object.freeze({ items: 16, heightPx: 480 });

/** Command ids that never appear on a context menu (rule 1). */
export const CONTEXT_MENU_EXCLUDED = Object.freeze(new Set(["edit.undo", "edit.redo"]));

/** Rows a reader can act on or read: separators excluded. */
export function countMenuRows(entries) {
  return entries.filter((entry) => !entry.separator).length;
}

/**
 * Rules 1 and 2 over a command list (before `normalizeMenuEntries` adds the
 * separators). Returns a new list; the input is not mutated. A submenu whose
 * rows are ALL refused is hidden like a refused row, with their first reason.
 *
 * Complexity: O(rows), one submenu level deep.
 */
export function pruneContextMenu(commands, { noteId = "context.refusedNote" } = {}) {
  const hiddenReasons = new Map();
  const hide = (reason) => {
    if (reason) hiddenReasons.set(reason, (hiddenReasons.get(reason) ?? 0) + 1);
  };
  const kept = [];
  for (const command of commands) {
    if (!command || command.visible === false) continue;
    if (CONTEXT_MENU_EXCLUDED.has(command.id)) continue;
    if (command.note) {
      kept.push(command);
      continue;
    }
    if (Array.isArray(command.submenu)) {
      const live = command.submenu.filter((row) => row && row.visible !== false && !row.separator);
      if (live.length === 0) continue;
      if (live.every((row) => row.enabled === false && !Array.isArray(row.submenu))) {
        hide(live.find((row) => row.disabledReason)?.disabledReason ?? "");
        continue;
      }
      kept.push(command);
      continue;
    }
    if (command.enabled === false) {
      hide(command.disabledReason ?? "");
      continue;
    }
    kept.push(command);
  }
  // One reason shared by two or more hidden rows is a MODE talking (Viewing,
  // Suggesting, a read-only container), not a row's own precondition: say it.
  let shared = "";
  let most = 1;
  for (const [reason, count] of hiddenReasons) {
    if (count > most) {
      shared = reason;
      most = count;
    }
  }
  if (shared && !kept.some((command) => command.note)) {
    kept.unshift({ id: noteId, note: true, label: shared, group: "note" });
  }
  return kept;
}

/** The groups of `commands` in order of first appearance. */
function groupOrder(commands) {
  const order = [];
  for (const command of commands) {
    const group = command.group ?? "";
    if (!order.includes(group)) order.push(group);
  }
  return order;
}

/**
 * Rule 3's fold: the last `groups` groups move into one "More" submenu at the
 * end. `groups` <= 0 returns the list unchanged; the first group never folds,
 * because it is the reason for the menu (the suggestions, the target's own
 * commands), and a note never folds.
 *
 * Complexity: O(rows).
 */
export function foldContextMenu(commands, groups, label = "") {
  const order = groupOrder(commands.filter((command) => !command.note));
  const folding = Math.min(Math.max(0, groups), Math.max(0, order.length - 1));
  if (folding === 0) return commands;
  const folded = new Set(order.slice(order.length - folding));
  const isFolded = (command) => !command.note && folded.has(command.group ?? "");
  return [
    ...commands.filter((command) => !isFolded(command)),
    {
      id: "context.more",
      label,
      group: "context-more",
      icon: "more",
      submenu: commands.filter(isFolded),
    },
  ];
}

/**
 * How many trailing groups must fold for `commands` to fit the row budget, by
 * counting alone. The pixel half needs the rendered menu: `showContextMenu`
 * measures, and folds one more group while it is still too tall.
 *
 * Complexity: O(groups × rows), a few dozen operations.
 */
export function foldsForRowBudget(commands, maxItems = CONTEXT_MENU_BUDGET.items) {
  const groups = groupOrder(commands.filter((command) => !command.note)).length;
  for (let folding = 0; folding < groups; folding += 1) {
    if (countMenuRows(foldContextMenu(commands, folding)) <= maxItems) return folding;
  }
  return Math.max(0, groups - 1);
}
