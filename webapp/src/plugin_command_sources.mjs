// SPDX-License-Identifier: Apache-2.0
// Commands contributed to the one registry from outside `editor_palette.mjs`
// (`docs/183` §8): the AI provider settings row, Manage add-ins and every
// plugin's `plugin.<id>.<command>` rows.
//
// A module, not a property on the editor context, so the registry reads its
// contributions through an import like every other source it has, and the
// context keeps the names `module_seams.test.mjs` holds it to.
//
// Complexity: O(sources + rows) per registry build.

/** `(context) => command[]` functions, in registration order. */
const sources = [];

/** Adds a source of command rows. Returns a function that removes it. */
export function addCommandSource(source) {
  if (typeof source !== "function") return () => {};
  sources.push(source);
  return () => {
    const at = sources.indexOf(source);
    if (at >= 0) sources.splice(at, 1);
  };
}

/** `element` when it is on screen, else null: a dialog's focus fallback must not
 *  name a control inside a hidden ribbon band or a closed Settings dialog, or the
 *  keyboard lands on `<body>` (`modal.mjs`). O(1). */
export function shown(element) {
  return element && element.isConnected && element.getClientRects().length > 0 ? element : null;
}

/** Every contributed row for this registry build. A source that throws is
 *  skipped and reported, so one plugin's bad row cannot empty the palette. */
export function contributedCommands(context) {
  const rows = [];
  for (const source of sources) {
    try {
      rows.push(...(source(context) ?? []));
    } catch (err) {
      console.warn("opendoc: a command source threw", err);
    }
  }
  return rows;
}
