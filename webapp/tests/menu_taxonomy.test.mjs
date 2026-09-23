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
  FILE_SURFACE,
  RIBBON_TABS,
  TABLE_MENU_LABELS,
  fileMenuSections,
  fileSurfaceCommandIds,
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

// ---- One navigation axis (`109` UX-014, docs/122) --------------------------
// The editor showed a menu bar AND a ribbon tab strip at once, so a command's
// home was a guess between two systems. Each chrome now has exactly one axis,
// and the File roster is ONE declaration with two renderings. These are the
// rules that keep it one.

test("the File page and the File menu offer the same rows, in the same order", () => {
  assert.deepEqual(
    APP_MENU_SECTIONS.file,
    fileMenuSections(),
    "the compact chrome's File dropdown and the ribbon chrome's File page must " +
      "read the SAME declaration. Two rosters for one surface is how the ribbon " +
      "drifted out of sync with the Insert menu the first time (UX-010)",
  );
  assert.deepEqual(
    fileSurfaceCommandIds(),
    FILE_SURFACE.flatMap((section) => section.ids),
    "the flattened roster must be the declared roster",
  );
});

test("every File section is headed and non-empty", () => {
  for (const section of FILE_SURFACE) {
    assert.ok(section.heading, `a File section with no heading: ${section.ids?.join(", ")}`);
    assert.ok(
      Array.isArray(section.ids) && section.ids.length > 0,
      `File section "${section.heading}" offers nothing, so the page prints a ` +
        "heading over blank space",
    );
  }
  const ids = fileSurfaceCommandIds();
  assert.equal(new Set(ids).size, ids.length, "a File row listed twice answers twice");
});

// The Tools and Help menus are gone; their rows are not. Removing a top-level
// name is only legitimate if every row it held landed somewhere durable, and
// "somewhere durable" for these five is the File surface (Settings and Help are
// File-page items in ONLYOFFICE) plus the Review band (the two proofing
// switches, where Word keeps them).
test("the rows the Tools and Help menus held all have a new home", () => {
  assert.equal(APP_MENU_SECTIONS.tools, undefined, "there is no Tools menu any more");
  assert.equal(APP_MENU_SECTIONS.help, undefined, "there is no Help menu any more");

  const onFile = new Set(fileSurfaceCommandIds());
  for (const id of ["view.settings", "help.commands", "help.shortcuts", "help.about"]) {
    assert.ok(onFile.has(id), `${id} lost its only durable home when Tools/Help went`);
  }
  const inReview = new Set(menuCommandIds("review"));
  for (const id of ["tools.spellCheck", "tools.grammarCheck", "tools.smartQuotes"]) {
    assert.ok(inReview.has(id), `${id} lost its only durable home when Tools went`);
  }
});

test("the tab strip renders exactly the declared order, with File first", () => {
  const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const strip = html.slice(html.indexOf('class="ribbon-tabs"'), html.indexOf('id="ribbonViewToggle"'));
  const rendered = [...strip.matchAll(/class="ribbon-tab[^"]*"[^>]*data-tab="(\w+)"/g)].map((m) => m[1]);
  assert.deepEqual(
    rendered,
    RIBBON_TABS.map((t) => t.tab),
    "the strip's order is declared in RIBBON_TABS so it is reviewable in a " +
      "diff. ONLYOFFICE's own order is an emergent property of seven addTab " +
      "calls with hard-coded indices and appears in no single file; copying the " +
      "order is worth it, copying that is not",
  );
  assert.equal(rendered[0], "file", "File is the first tab (ONLYOFFICE Toolbar.js:182)");
  assert.equal(
    RIBBON_TABS.at(-1).contextual,
    true,
    "contextual tabs go last, as they do in both Word and ONLYOFFICE",
  );

  // The File tab is the one that shows a PAGE, and it must have one to show.
  assert.match(strip, /id="tabFile"[^>]*aria-controls="panelFile"/);
  assert.match(html, /id="panelFile"[^>]*data-panel="file"/);
  assert.match(html, /id="filePageBody"/, "the File page needs a body to render rows into");
});

test("Help offers About, and About is the only place claiming a version", () => {
  assert.ok(
    fileSurfaceCommandIds().includes("help.about"),
    "About must be reachable from the File surface — the product shipped with " +
      "no About anywhere, so a bug report could not name the build it came " +
      "from. It used to live in a Help menu; ONLYOFFICE's Help is a File-page " +
      "panel, and that is where it is now",
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
