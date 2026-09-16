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
// This parses `APP_MENU_SECTIONS` out of main.js as text. That is deliberate:
// main.js has zero exports (docs/105 UX-003/CQ-001) and cannot be imported, and
// a browser-driven check would make a structural rule cost a Playwright run.
// The e2e side of this contract — that each row actually renders and runs — is
// `webapp/tests/e2e/menu-taxonomy.spec.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");

/** Extracts a balanced `{...}` or `[...]` literal that starts at `open`. */
function balanced(text, open, pair) {
  const [l, r] = pair;
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === l) depth += 1;
    else if (text[i] === r) {
      depth -= 1;
      if (depth === 0) return text.slice(open + 1, i);
    }
  }
  throw new Error("unbalanced literal");
}

/** `APP_MENU_SECTIONS` as {menuName: [commandId, ...]}. */
function menuSections() {
  const at = source.indexOf("const APP_MENU_SECTIONS = {");
  assert.ok(at > 0, "APP_MENU_SECTIONS must exist");
  const body = balanced(source, source.indexOf("{", at), "{}");
  const out = {};
  const key = /(\w+):\s*\[/g;
  let m;
  while ((m = key.exec(body))) {
    const list = balanced(body, m.index + m[0].length - 1, "[]");
    out[m[1]] = [...list.matchAll(/"([\w.]+)"/g)].map((x) => x[1]);
    key.lastIndex = m.index + m[0].length + list.length;
  }
  return out;
}

test("no command has two menu homes", () => {
  const sections = menuSections();
  const homes = new Map();
  for (const [menu, ids] of Object.entries(sections)) {
    for (const id of ids) {
      if (!homes.has(id)) homes.set(id, []);
      // A menu may legitimately list an id once; twice in the SAME menu is also
      // a bug, so push per occurrence rather than per menu.
      homes.get(id).push(menu);
    }
  }
  const duplicated = [...homes.entries()].filter(([, menus]) => menus.length > 1);
  assert.deepEqual(
    duplicated.map(([id, menus]) => `${id} → ${menus.join(", ")}`),
    [],
    "each command belongs in exactly ONE menu; reach it from other SURFACES " +
      "(palette, ribbon, context menu) instead of repeating it in the bar",
  );
});

test("every menu in the taxonomy has a button, and every button has a menu", () => {
  const sections = menuSections();
  const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const buttons = [...html.matchAll(/class="app-menu-button"[^>]*data-menu="(\w+)"/g)].map(
    (m) => m[1],
  );
  assert.deepEqual(
    buttons.filter((b) => !sections[b]),
    [],
    "a menu button with no section renders an empty popover",
  );
  assert.deepEqual(
    Object.keys(sections).filter((s) => !buttons.includes(s)),
    [],
    "a section with no button is unreachable — the exact UX-012 shape, where " +
      "every table command existed and no menu offered them",
  );
});

test("the Table menu covers every structural table command", () => {
  const sections = menuSections();
  assert.ok(sections.table, "there must be a Table menu (docs/105 UX-012)");

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

  const inMenu = new Set(sections.table);
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
  const at = source.indexOf("const TABLE_MENU_LABELS = new Map([");
  assert.ok(at > 0, "TABLE_MENU_LABELS must exist");
  const body = balanced(source, source.indexOf("[", at), "[]");
  const labelled = new Set([...body.matchAll(/\["(table\.[\w.]+)",/g)].map((m) => m[1]));
  const inMenu = new Set(menuSections().table);

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
  const sections = menuSections();
  assert.ok(
    sections.help?.includes("help.about"),
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
