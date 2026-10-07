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
 * @param {Array<{shortcut?:string, group:string, label:string,
 *        referenceRow?:{id:string,label:string}}>} commands the command
 *        registry's palette-visible entries. Commands sharing a `referenceRow`
 *        id are listed as ONE row under that row's label.
 * @param {Array<{keys:string,label:string}>} navigation the caret-movement
 *        table, which has no commands behind it.
 * @param {(chord:string) => string} format renders a chord for the keyboard in
 *        front of the user (⌘P on a Mac, Ctrl+P elsewhere).
 * @returns {Array<[string, Array<{keys:string,label:string}>]>}
 */
export function shortcutGroups(commands, navigation, format) {
  const groups = new Map([["Moving around", navigation]]);
  /** Rows standing for a FAMILY of commands, by family id, with their chords. */
  const families = new Map();
  for (const command of commands) {
    if (!command.shortcut) continue;
    const rows = groups.get(command.group) ?? [];
    const keys = format(command.shortcut);
    // A command can be listed twice by the registry (the same chord reached
    // from two contexts); the reference shows each chord once.
    if (rows.some((row) => row.keys === keys)) continue;
    // A family is ONE row, the way Google Docs lists "Apply heading style
    // [1-6] · Ctrl+Alt+[1-6]" and Word lists F6 / Shift+F6 as one "pane" row: a
    // reference is read by scanning, and four rows that differ by one digit cost
    // a 1280x720 laptop the height `dialog-fit` holds this card to.
    const family = command.referenceRow;
    if (family) {
      const row = families.get(family.id);
      if (row) {
        row.chords.push(keys);
        row.keys = joinChords(row.chords);
        continue;
      }
      const fresh = { keys, label: family.label, chords: [keys] };
      families.set(family.id, fresh);
      rows.push(fresh);
      groups.set(command.group, rows);
      continue;
    }
    rows.push({ keys, label: command.label });
    groups.set(command.group, rows);
  }
  return [...groups]
    .filter(([, rows]) => rows.length)
    .map(([group, rows]) => [group, rows.map(({ keys, label }) => ({ keys, label }))]);
}

/** Several chords as one key cell: `Ctrl+Alt+1–3` when they differ only by a
 *  final digit that counts up, `F6 / Shift+F6` otherwise. */
export function joinChords(chords) {
  const prefix = chords[0].slice(0, -1);
  const digits = chords.map((chord) => chord.slice(-1));
  const consecutive =
    chords.length > 1 &&
    chords.every((chord) => chord.length === chords[0].length && chord.startsWith(prefix)) &&
    digits.every((digit, index) => /\d/.test(digit) && Number(digit) === Number(digits[0]) + index);
  return consecutive ? `${prefix}${digits[0]}–${digits.at(-1)}` : chords.join(" / ");
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
