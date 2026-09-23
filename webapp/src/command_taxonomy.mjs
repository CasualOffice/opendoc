// Where a command lives, and how its label reads on the surface it is shown on.
//
// Extracted from `main.js` for the same reason `edit_errors.mjs` was (`109`
// HF-085): the module is ~90% of the webapp with zero exports, so every rule
// about the command surfaces could only be checked by reading main.js as TEXT.
// `menu_taxonomy.test.mjs` really did parse `APP_MENU_SECTIONS` out of the file
// with a brace matcher, which is why the parity guards `109` UX-004 asks for —
// "the ids this surface offers are EXACTLY this set" — could not be written:
// a regex over an 18k-line file can prove what is there, never what is missing.
//
// Nothing here touches the DOM or the engine. The behaviour (what a command
// DOES, whether it is enabled, what it refuses with) stays in `main.js` with
// the state it reads; only the taxonomy and the pure label/shape rules move.

// ---- The File surface: ONE declaration, two renderings ---------------------
//
// The editor used to carry TWO navigation systems at once: an application menu
// bar (File / Edit / View / …) in the header and a ribbon tab strip (Home /
// Insert / Layout / …) under it. A person had to decide WHICH OF TWO PLACES a
// command lived before they could look for it, which is the cognitive burden
// `109` UX-014 records and the owner asked to remove. The fix is one axis per
// chrome — never two on screen together (docs/122):
//
//   ribbon mode   the tab strip IS the axis, and `File` is its first tab,
//                 opening a full-window page. Source-verified from ONLYOFFICE:
//                 `apps/documenteditor/main/app/view/Toolbar.js:182-186`
//                 declares the tabs as File, Home, Insert, Layout, References,
//                 and File alone carries `haspanel: false` — it is a page, not
//                 a band. The menu bar is hidden here.
//   compact mode  there is no band to put a File page under, so the axis is the
//                 menu bar and `File` is a dropdown, the shape Google Docs and
//                 Google Drive use. The ribbon is hidden here.
//
// Both renderings read THIS list, so the two cannot drift into different File
// rosters — the "prefer one mechanism over two" rule. The headings are what the
// page groups by; the dropdown renders the same groups as separator runs.
//
// The roster follows ONLYOFFICE's File page, whose items are
// `DE.Views.FileMenu.btn*` in their `locale/en.json`: Create New, Open, Open
// Recent, Save, Save As, Save Copy As, Download As, Print, Info, Advanced
// Settings, Help. The ORDER is Google Docs' File menu (New, Open … Page setup,
// Print …), because that is the order this editor already shipped and the owner
// named Drive as the compact-mode reference. Their Protect, Rename, Version
// History, Access Rights, Suggest a Feature and Switch to Mobile are NOT here:
// see docs/122 §6 for why each one is absent rather than forgotten.
export const FILE_SURFACE = [
  { heading: "New and open", ids: ["file.new", "file.open", "file.recoverDrafts"] },
  {
    heading: "Save",
    ids: [
      "file.save",
      "file.export.pdf",
      "file.export.docx",
      "file.export.odt",
      "file.export.text",
      "file.export.json",
    ],
  },
  // Page setup was under Tools, which is where nobody looks for paper size —
  // Docs files it under File and Word under Layout. It is on the Layout ribbon
  // too; this gives it a File home that matches the competition.
  { heading: "Print", ids: ["layout.pageSetup", "file.print"] },
  { heading: "Document", ids: ["file.properties"] },
  // ONLYOFFICE's "Advanced Settings" and "Help" are both File-page items. They
  // were the whole content of a `Tools` menu and a `Help` menu, which is two
  // more top-level names for five rows — and the two that fell off the end of
  // the menu bar behind a hidden scrollbar (`109` HF-097).
  { heading: "Settings", ids: ["view.settings"] },
  { heading: "Help", ids: ["help.commands", "help.shortcuts", "help.about"] },
];

/**
 * The ribbon's tab strip, as ONE ordered list.
 *
 * Declared rather than left to emerge from the markup because ONLYOFFICE's own
 * order is an emergent property of seven `Mixtbar.addTab(tab, panel, after)`
 * calls with hard-coded indices into a sparse array — their order appears in no
 * single file and cannot be read off one. That is a trap, not a pattern to copy.
 * `menu_taxonomy.test.mjs` asserts `editor.html` renders exactly this sequence,
 * so the strip's order is reviewable in a diff.
 *
 * The order itself IS theirs, minus the tabs this editor has nothing to put on:
 * File, Home, Insert, Layout, References, Review, View, then contextual tabs at
 * the right-hand end. `page: true` marks the one tab that shows a page instead
 * of a band; `contextual: true` marks a tab that is disabled until its object is
 * selected.
 */
export const RIBBON_TABS = [
  { tab: "file", label: "File", page: true },
  { tab: "home", label: "Home" },
  { tab: "insert", label: "Insert" },
  { tab: "layout", label: "Layout" },
  { tab: "references", label: "References" },
  { tab: "review", label: "Review" },
  { tab: "view", label: "View" },
  { tab: "table", label: "Table", contextual: true },
];

/** The File roster as separator groups, the shape the menu-bar renderer takes. */
export function fileMenuSections() {
  return FILE_SURFACE.map((section) => section.ids);
}

/** Every command id the File surface offers, in page order. */
export function fileSurfaceCommandIds() {
  return FILE_SURFACE.flatMap((section) => section.ids);
}

// The COMPACT chrome's menu bar. One command has ONE menu home: eight ids used
// to sit in two menus each (the three review modes, `review.toggle`,
// `view.showChanges`, `review.comment`, `layout.paragraph`, `file.properties`),
// which is what made browsing the bar feel repetitive — the same row answered
// twice and the menus stopped telling you where a thing lives. Reachability
// from more than one SURFACE is a requirement here (docs/105 command-surface
// parity) and is unaffected: every command below is still in the palette, and
// most are on the ribbon. What is fixed is duplication WITHIN the bar.
//
// Where the split was a judgement call it follows Google Docs, which is the
// stated bar: editing mode is View ▸ Mode, a comment is Insert ▸ Comment, page
// setup is File ▸ Page setup. Tracked-change OPERATIONS stay in Review.
//
// There is no `tools` menu. Its three rows went where the competition puts
// them: Settings is ONLYOFFICE's File ▸ Advanced Settings, and spell check and
// smart quotes are proofing — Word's Review ▸ Proofing group. That is two fewer
// top-level names to scan and it is what stops the bar overflowing (HF-097).
export const APP_MENU_SECTIONS = {
  file: fileMenuSections(),
  edit: [
    ["edit.undo", "edit.redo"],
    ["edit.cut", "edit.copy", "edit.paste", "edit.pasteText"],
    ["edit.selectAll", "edit.find"],
  ],
  view: [
    ["view.outline", "view.showChanges"],
    ["view.zoomIn", "view.zoomOut"],
    // The ribbon-density switch belongs in View, next to the other things that
    // change what the window shows rather than what the document says.
    ["view.compactRibbon"],
    // Editing mode is View ▸ Mode in Docs. It was in both View and Review.
    ["review.mode.editing", "review.mode.suggesting", "review.mode.viewing"],
  ],
  insert: [["insert.table", "insert.image", "insert.shape", "insert.textbox", "insert.link", "insert.bookmark", "insert.field"], ["insert.header", "insert.footer"], ["insert.footnote", "insert.endnote"], ["layout.firstPageVariant", "layout.evenOddVariant"], ["insert.symbol", "insert.emoji"], ["review.comment"]],
  format: [
    ["format.bold", "format.italic", "format.underline", "format.strike"],
    ["format.grow", "format.shrink", "format.color", "format.highlight"],
    ["format.case.upper", "format.case.lower", "format.case.title", "format.case.sentence", "format.case.toggle"],
    ["format.superscript", "format.subscript", "format.clear"],
    ["paragraph.align.start", "paragraph.align.center", "paragraph.align.end", "paragraph.align.justify"],
    // Checklist, restart and continue existed on the ribbon and in the palette
    // but in no menu, so browsing Format said the editor had no checklists at
    // all (docs/104 HF-076).
    ["paragraph.list.bullet", "paragraph.list.numbered", "paragraph.list.checklist"],
    ["paragraph.list.restart", "paragraph.list.continue"],
    ["paragraph.indent.decrease", "paragraph.indent.increase", "layout.paragraph"],
    // Copying formatting is a FORMAT action. Filing it under Edit put it next to
    // cut/paste, where it reads as clipboard behaviour.
    ["format.painter"],
    ["style.updateFromSelection", "style.createFromSelection"],
  ],
  // Every structural table command already ran through `tableToolCommands` and
  // was reachable from the right-click menu and the palette — and from no menu
  // at all (docs/105 UX-012), so a user browsing the bar was told the editor
  // could not edit tables. The rows below are the SAME command objects, so
  // gating, disabled reasons and the transactions they run cannot drift.
  table: [
    ["table.insert.rowAbove", "table.insert.rowBelow", "table.insert.columnLeft", "table.insert.columnRight"],
    ["table.delete.row", "table.delete.column", "table.delete.table"],
    ["table.select.row", "table.select.column", "table.select.table"],
    ["table.merge", "table.split"],
    ["table.distribute.rows", "table.distribute.columns"],
    ["table.sort.ascending", "table.sort.descending"],
    ["table.cellFormat", "table.properties"],
  ],
  review: [
    ["review.toggle"],
    ["review.previous", "review.next"],
    ["review.acceptNext", "review.rejectNext"],
    ["review.acceptAll", "review.rejectAll"],
    // Proofing. Word's Review tab opens with a Proofing group, and these three
    // are the only proofing switches this editor has. They were the content of
    // a `Tools` menu that existed for them alone.
    ["tools.spellCheck", "tools.grammarCheck", "tools.smartQuotes"],
  ],
};

// Menu labels for the table rows. `tableToolCommands` supplies behaviour; only
// the LABEL differs by surface. The palette needs its "Table:" prefix because it
// is one flat global list, while a row inside the Table menu already has that
// noun from the menu it sits in. This map is explicit rather than derived from
// the submenu trail because the trails do not compose into readable text
// ("Delete Delete row", "Autofit & sort Distribute rows"). `menu-taxonomy`
// fails if this map and the command set disagree in either direction.
export const TABLE_MENU_LABELS = new Map([
  ["table.insert.rowAbove", "Insert row above"],
  ["table.insert.rowBelow", "Insert row below"],
  ["table.insert.columnLeft", "Insert column left"],
  ["table.insert.columnRight", "Insert column right"],
  ["table.delete.row", "Delete row"],
  ["table.delete.column", "Delete column"],
  ["table.delete.table", "Delete table"],
  ["table.select.row", "Select row"],
  ["table.select.column", "Select column"],
  ["table.select.table", "Select table"],
  ["table.merge", "Merge cells"],
  ["table.split", "Split cell…"],
  ["table.distribute.rows", "Distribute rows"],
  ["table.distribute.columns", "Distribute columns"],
  ["table.sort.ascending", "Sort ascending"],
  ["table.sort.descending", "Sort descending"],
  ["table.cellFormat", "Cell formatting…"],
  ["table.properties", "Table properties…"],
]);

/** The menu names the bar offers, in bar order. */
export function appMenuNames() {
  return Object.keys(APP_MENU_SECTIONS);
}

/** Every command id the named menu offers, flattened out of its separator
 *  groups and in the order the popover renders them. An unknown menu name gives
 *  `[]`, which is what `renderAppMenu` already does with one. */
export function menuCommandIds(name) {
  return (APP_MENU_SECTIONS[name] ?? []).flat();
}

/** Every command id the bar offers, across all menus, in bar order. */
export function allMenuCommandIds() {
  return appMenuNames().flatMap((name) => menuCommandIds(name));
}

/**
 * `id → [menu, ...]`, one entry per OCCURRENCE.
 *
 * Per occurrence rather than per menu on purpose: one command has one menu
 * home, and listing an id twice inside the SAME menu is the same defect as
 * listing it in two — the bar answers the same question twice and stops being
 * a map of where things live.
 */
export function menuHomes() {
  const homes = new Map();
  for (const name of appMenuNames()) {
    for (const id of menuCommandIds(name)) {
      if (!homes.has(id)) homes.set(id, []);
      homes.get(id).push(name);
    }
  }
  return homes;
}

/**
 * Flattens a nested command tree (submenus and all) into flat surface rows.
 *
 * Both the table rows and the object rows needed this, and both had grown
 * their own copy of it inside `editorCommands`. They had to: a palette row
 * whose `run` is a submenu is a dead row, so a flat surface has to walk the
 * tree and keep the parent's name in the label. One copy, so the next
 * contextual family cannot reintroduce the dead-parent row a third time.
 *
 * @param {Array} entries the tree, as the context-menu builders produce it.
 * @param {(entry: object, trail: string) => object} describe supplies the
 *   surface-specific `{label, group, kw}`; `trail` is the parent names joined
 *   with spaces, `""` at the top level.
 * @param {string} [trail] internal — the accumulated parent trail.
 * @returns {Array} leaf rows carrying the ORIGINAL `enabled`, `disabledReason`
 *   and `run`, so gating and the transactions they run cannot drift by surface.
 */
export function flattenCommandTree(entries, describe, trail = "") {
  return entries.flatMap((entry) =>
    entry.submenu
      ? flattenCommandTree(entry.submenu, describe, trail ? `${trail} ${entry.label}` : entry.label)
      : [{
        id: entry.id,
        ...describe(entry, trail),
        enabled: entry.enabled,
        disabledReason: entry.disabledReason,
        run: entry.run,
      }],
  );
}

/**
 * A table command's label on `surface`.
 *
 * In the Table MENU the noun is already supplied by the menu the row sits in,
 * so the palette's "Table:" prefix would read as a stutter. Everywhere else the
 * palette is one flat global list and the row has to name its own subject.
 */
export function tableCommandLabel(id, label, trail, surface) {
  if (surface === "menu" && TABLE_MENU_LABELS.has(id)) return TABLE_MENU_LABELS.get(id);
  return trail ? `Table: ${trail} ${label}` : `Table: ${label}`;
}

/**
 * The Table menu's rows when the caret is not in a table: every row present and
 * disabled, with the reason.
 *
 * Building the menu only from a live table context meant browsing to Table with
 * the caret in a paragraph opened an EMPTY popover — which says the editor
 * cannot edit tables, the same thing having no Table menu at all said (UX-012).
 * Word and Docs both show the rows greyed with the reason. The labels come from
 * the same map the live rows use, so the placeholders cannot drift away from
 * the commands they stand in for.
 */
export function tableMenuPlaceholders(disabledReason) {
  return [...TABLE_MENU_LABELS].map(([id, label]) => ({
    id,
    label,
    group: "Table",
    kw: `table ${label}`.toLowerCase(),
    enabled: false,
    disabledReason,
    run: () => {},
  }));
}
