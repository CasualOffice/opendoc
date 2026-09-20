// The keyboard-shortcut reference (HF-151), as data and as DOM.
//
// Extracted from `main.js` (`109` HF-085). The grouping half is a pure
// function of the command registry, which is the half worth being able to test
// without a browser: the reference's whole claim is that it can neither list a
// chord that does not exist nor miss one that does, and that claim is about
// this transformation, not about the markup it ends up in.

/**
 * The reference's sections, in display order.
 *
 * @param {Array<{shortcut?:string, group:string, label:string}>} commands the
 *        command registry's palette-visible entries.
 * @param {Array<{keys:string,label:string}>} navigation the caret-movement
 *        table, which has no commands behind it.
 * @param {(chord:string) => string} format renders a chord for the keyboard in
 *        front of the user (⌘P on a Mac, Ctrl+P elsewhere).
 * @returns {Array<[string, Array<{keys:string,label:string}>]>}
 */
export function shortcutGroups(commands, navigation, format) {
  const groups = new Map([["Moving around", navigation]]);
  for (const command of commands) {
    if (!command.shortcut) continue;
    const rows = groups.get(command.group) ?? [];
    // A command can be listed twice by the registry (the same chord reached
    // from two contexts); the reference shows each chord once.
    if (rows.some((row) => row.keys === format(command.shortcut))) continue;
    rows.push({ keys: format(command.shortcut), label: command.label });
    groups.set(command.group, rows);
  }
  return [...groups].filter(([, rows]) => rows.length);
}

/** Renders `groups` into the dialog body, replacing whatever was there. */
export function renderShortcutsReference(body, groups) {
  body.replaceChildren();
  for (const [group, rows] of groups) {
    const section = document.createElement("section");
    section.className = "shortcuts-group";
    const heading = document.createElement("h3");
    heading.textContent = group;
    section.appendChild(heading);
    for (const row of rows) {
      const line = document.createElement("div");
      line.className = "shortcuts-row";
      const label = document.createElement("span");
      label.textContent = row.label;
      const keys = document.createElement("span");
      keys.className = "shortcuts-keys";
      keys.textContent = row.keys;
      line.append(label, keys);
      section.appendChild(line);
    }
    body.appendChild(section);
  }
}
