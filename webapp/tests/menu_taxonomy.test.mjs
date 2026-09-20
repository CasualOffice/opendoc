// The menu bar's taxonomy, enforced as a contract instead of a tidy-up.
//
// docs/105 UX-014 is a list of taxonomy faults that all have the same shape:
// somebody added a command to a second menu because it felt reachable there,
// and nothing objected. Eight ids ended up in two menus each, which is what
// made browsing the bar feel repetitive — the same row answered twice, and the
// menus stopped being a map of where things live. Meanwhile the entire
// `table.*` family sat in NO menu (UX-012), so browsing the bar said the
// editor could not edit tables at all.
//
// This used to parse `APP_MENU_SECTIONS` out of main.js as TEXT, with a brace
// matcher, because main.js had zero exports (`109` HF-085) and could not be
// imported. The taxonomy now lives in `src/command_taxonomy.mjs`, so the rules
// below read the real data structure the editor renders from. The e2e side of
// this contract — that each row actually renders and runs — is
// `webapp/tests/e2e/menu-taxonomy.spec.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  APP_MENU_SECTIONS,
  TABLE_MENU_LABELS,
  menuCommandIds,
  menuHomes,
} from "../src/command_taxonomy.mjs";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");

test("no command has two menu homes", () => {
  const duplicated = [...menuHomes().entries()].filter(([, menus]) => menus.length > 1);
  assert.deepEqual(
    duplicated.map(([id, menus]) => `${id} → ${menus.join(", ")}`),
    [],
    "each command belongs in exactly ONE menu; reach it from other SURFACES " +
      "(palette, ribbon, context menu) instead of repeating it in the bar",
  );
});

test("every menu in the taxonomy has a button, and every button has a menu", () => {
  const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const buttons = [...html.matchAll(/class="app-menu-button"[^>]*data-menu="(\w+)"/g)].map(
    (m) => m[1],
  );
  assert.deepEqual(
    buttons.filter((b) => !APP_MENU_SECTIONS[b]),
    [],
    "a menu button with no section renders an empty popover",
  );
  assert.deepEqual(
    Object.keys(APP_MENU_SECTIONS).filter((s) => !buttons.includes(s)),
    [],
    "a section with no button is unreachable — the exact UX-012 shape, where " +
      "every table command existed and no menu offered them",
  );
});

test("the Table menu covers every structural table command", () => {
  assert.ok(APP_MENU_SECTIONS.table, "there must be a Table menu (docs/105 UX-012)");

  // The ids `tableToolCommands` actually builds. Submenu PARENTS (table.insert,
  // table.delete, table.select, table.layout) are containers, not commands, and
  // are correctly absent from a flat menu.
  const parents = new Set(["table.insert", "table.delete", "table.select", "table.layout"]);
  const built = new Set(
    [
      ...source.matchAll(/tableMutation\("(table\.[\w.]+)"/g),
      ...source.matchAll(/id: "(table\.[\w.]+)"/g),
    ]
      .map((m) => m[1])
      .filter((id) => !parents.has(id)),
  );

  const inMenu = new Set(menuCommandIds("table"));
  assert.deepEqual(
    [...built].filter((id) => !inMenu.has(id)).sort(),
    [],
    "a table command exists but no Table menu row offers it",
  );
  assert.deepEqual(
    [...inMenu].filter((id) => !built.has(id)).sort(),
    [],
    "the Table menu lists an id no command builds — it would render nothing",
  );
});

test("every Table menu row has a label, and no label is orphaned", () => {
  const labelled = new Set(TABLE_MENU_LABELS.keys());
  const inMenu = new Set(menuCommandIds("table"));

  assert.deepEqual(
    [...inMenu].filter((id) => !labelled.has(id)).sort(),
    [],
    "a Table menu row with no menu label falls back to the palette's " +
      '"Table: …" text, which stutters inside a menu already called Table',
  );
  assert.deepEqual(
    [...labelled].filter((id) => !inMenu.has(id)).sort(),
    [],
    "a label for a row that is not in the menu — dead weight that will rot",
  );
});

test("Help offers About, and About is the only place claiming a version", () => {
  assert.ok(
    menuCommandIds("help").includes("help.about"),
    "About must be reachable from Help — the product shipped with no About " +
      "anywhere, so a bug report could not name the build it came from",
  );

  // The version must come from the engine, not from a string in the markup.
  // A number on a user-facing surface that no committed artifact backs is the
  // EV-002 failure (a page claiming 19 families against 26 in the data).
  const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const slot = html.match(/<dd id="aboutVersion">([^<]*)<\/dd>/);
  assert.ok(slot, "About must carry a version slot");
  assert.doesNotMatch(
    slot[1],
    /\d/,
    "the version slot must not ship a hardcoded number; it is filled from " +
      "engineVersion(), which the engine compiles from its own manifest",
  );
  assert.match(
    source,
    /engineVersion\b/,
    "main.js must read the version from the engine export",
  );
});
