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
// rosters — the "prefer one mechanism over two" rule. The band NAMES are what
// the page prints as headings and what the dropdown announces as group names.
//
// The roster follows ONLYOFFICE's File page, whose items are
// `DE.Views.FileMenu.btn*` in their `locale/en.json`: Create New, Open, Open
// Recent, Save, Save As, Save Copy As, Download As, Print, Info, Advanced
// Settings, Help. The ORDER is Google Docs' File menu (New, Open … Page setup,
// Print …), because that is the order this editor already shipped and the owner
// named Drive as the compact-mode reference. Their Protect, Rename, Version
// History, Access Rights, Suggest a Feature and Switch to Mobile are NOT here:
// see docs/122 §6 for why each one is absent rather than forgotten.
//
// ---- Bands are NAMED, and the name is a catalogue key ----------------------
//
// Every band in every menu carries a `nameKey`, never an English string. Three
// things follow from that, and all three are requirements rather than taste:
//
//   * a screen reader gets a `role="group"` with a NAME ("Clipboard, group")
//     instead of an anonymous rule it can only announce as "separator", so the
//     band means the same thing to a reader as the hairline does to a viewer;
//   * the name is translated, in every locale, like every other user-facing
//     string in the editor (docs/124). An English literal here would also raise
//     this file's unrouted-string ceiling, which is refused;
//   * the File PAGE prints it as a heading and the File DROPDOWN announces it
//     as a group name, from the one declaration — the two File surfaces cannot
//     name a band two different things.
//
// The keys live under `menuGroup.*` in `en_strings.mjs`. Where the ribbon
// already had a group of the same name, the English is the SAME sentence and
// every locale's value was copied from the catalogue entry the ribbon uses, not
// translated a second time: a band called "Illustrations" in the menu and a
// ribbon group called something else would read as two products.
export const FILE_SURFACE = [
  { nameKey: "menuGroup.newAndOpen", ids: ["file.new", "file.open", "file.recoverDrafts"] },
  { nameKey: "menuGroup.save", ids: ["file.save"] },
  // The nine export formats behind one row. Google Docs files exactly these
  // under Download; Word's backstage gives Export its own page. Ten rows of
  // which nine say "export as" is the shape that made this menu long, and Save
  // is not one of them — it belongs beside New and Open, not behind a flyout.
  //
  // The File PAGE renders a submenu flat, so its Export category keeps the
  // heading-over-rows treatment it already had; only the dropdown folds.
  //
  // `dotx`, `html` and `markdown` joined the six: all three writers shipped in
  // the engine and reached NO menu at all, so the only place they surfaced was
  // the Save-as picker, under their raw format ids. Order follows Word's
  // backstage — the document formats, then the interchange ones, with the
  // debugging artifact last.
  {
    nameKey: "filePane.export.label",
    submenu: true,
    ids: [
      "file.export.pdf",
      "file.export.docx",
      "file.export.dotx",
      "file.export.odt",
      "file.export.rtf",
      "file.export.html",
      "file.export.markdown",
      "file.export.text",
      "file.export.json",
    ],
  },
  // Page setup was under Tools, which is where nobody looks for paper size —
  // Docs files it under File and Word under Layout. It is on the Layout ribbon
  // too; this gives it a File home that matches the competition.
  { nameKey: "menuGroup.print", ids: ["layout.pageSetup", "file.print"] },
  { nameKey: "menuGroup.document", ids: ["file.properties"] },
  // Version history's PRIMARY home, and the one both references agree on: Google
  // Docs is File ▸ Version history ▸ See version history, ONLYOFFICE is a File
  // page item (`DE.Views.FileMenu.btnHistory`), Word puts it under File ▸ Info.
  // `docs/139` §8.1 names this entry point first. It is its own heading rather
  // than a row under Document because a document's metadata and its past are
  // different questions, and because §6 of `docs/122` listed Version History as
  // "not adopted … `docs/107` designs it; not built" — that sentence was true
  // when it was written and is corrected there in this change.
  { nameKey: "menuGroup.history", ids: ["file.versionHistory"] },
  // ONLYOFFICE's "Advanced Settings" and "Help" are both File-page items. They
  // were the whole content of a `Tools` menu and a `Help` menu, which is two
  // more top-level names for five rows — and the two that fell off the end of
  // the menu bar behind a hidden scrollbar (`109` HF-097).
  // The measurement-unit preference sits in the SAME band as Settings, and in
  // ONLYOFFICE it is literally the same pane: `cmbUnit` is a row of their File ▸
  // Advanced Settings (`FileMenuPanels.js:747`), and Word's home for it is File ▸
  // Options ▸ Advanced. This row lands on the chooser inside Settings rather than
  // being a second copy of it, so the preference has one control and two ways in.
  { nameKey: "menuGroup.settings", ids: ["view.settings", "view.measurementUnits"] },
  { nameKey: "menuGroup.help", ids: ["help.commands", "help.shortcuts", "help.about"] },
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

/** The File roster as named bands, the shape both menu renderers take. */
export function fileMenuSections() {
  return FILE_SURFACE;
}

/** Every command id the File surface offers, in page order. */
export function fileSurfaceCommandIds() {
  return FILE_SURFACE.flatMap((section) => section.ids);
}

/** One band: a catalogue key for its name, and the commands it holds.
 *
 *  A function rather than an object literal per band because the shape is
 *  declared forty-odd times below and the noise was the reason the old
 *  `string[][]` shape survived so long — the bands existed, they just could not
 *  say what they were. */
const band = (nameKey, ...ids) => ({ nameKey, ids });

/** One band rendered as a SUBMENU: the band's name becomes a row, and its
 *  commands live in a flyout behind it.
 *
 *  A named band with a hairline over it still leaves every row on screen, and
 *  the owner's report was about length, not about grouping: Format listed 31
 *  rows, Table 28. Google Docs' Format menu is about ten rows of which most
 *  open a submenu (Text, Align & indent, Line & paragraph spacing, Bullets &
 *  numbering); Word and ONLYOFFICE decompose Table the same way (Insert,
 *  Delete, Select, Merge, Cell size). So the long bands become submenus and the
 *  short ones stay inline under their heading — a one- or two-row band behind a
 *  flyout is a click spent to save nothing.
 *
 *  Same declaration, one more field: every surface that reads `APP_MENU_SECTIONS`
 *  — the menu bar, the compact toolbar's Table dropdown, the File page — reads
 *  this too, so a submenu cannot exist in one and not the other. The File page
 *  deliberately renders them FLAT: a full window has the room to show a group,
 *  which is the same reason it uses headings where the dropdown uses names. */
const sub = (nameKey, ...ids) => ({ nameKey, ids, submenu: true });

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
//
// ---- The bands, and whose convention each one follows ----------------------
//
// The owner's report was that the menus are flat lists: "grouping of things is
// necessary in file menus in compact mode … basically menus like File, Edit,
// View". Compact mode is where it bites hardest, because the menu bar is that
// chrome's ONLY navigation axis — there is no ribbon behind it to fall back on.
//
// Nothing below is invented. Where Word, Google Docs and ONLYOFFICE agree on a
// band and its place, that is what this is; where they differ the choice is
// named in the comment beside it. The rule the four references share and that
// this follows everywhere: **bulk and destructive rows get their own band**, so
// "Accept all changes" is never one keystroke past "Accept change", and
// "Delete table" is never one past "Insert row above". A separator is a safety
// affordance before it is a scanning one.
export const APP_MENU_SECTIONS = {
  file: fileMenuSections(),
  // Word's Edit menu, Docs' Edit menu and LibreOffice's all read the same:
  // undo/redo | cut/copy/paste | select all | find and replace. Select all and
  // Find each get their own band in all three — they are one row each here, and
  // a one-row band is still the right answer when the three references agree
  // that this row is not a member of the band above it.
  edit: [
    band("menuGroup.undo", "edit.undo", "edit.redo"),
    band("menuGroup.clipboard", "edit.cut", "edit.copy", "edit.paste", "edit.pasteText"),
    band("menuGroup.selection", "edit.selectAll"),
    band("menuGroup.find", "edit.find"),
  ],
  // Docs opens View with Mode; Word's View tab reads Views | Show | Zoom. So:
  // mode, then what the window shows, then how big. The ribbon-density switch
  // joins Show rather than standing alone — it changes what the window shows
  // and not what the document says, which is the test every row in that band
  // passes.
  view: [
    band("menuGroup.mode", "review.mode.editing", "review.mode.suggesting", "review.mode.viewing"),
    // `view.pages` sits beside `view.outline` because the two are the same kind
    // of thing — a navigation panel the rail opens — and they were not the same
    // kind of REACHABLE thing: the outline had a command id and Pages did not.
    // Its only surface in the whole product was the rail's `#railPages` tile,
    // so a chrome that withheld the rail (a host withholding the region, or a
    // phone reclaiming 44px of height) took `#pagesPanel` off that device class
    // entirely while it stayed reachable at 1280px. That is "never a dead
    // control" seen from the other side, and it is why `docs/148` §5.3 kept the
    // rail on a phone against all three references. With an id here the rail is
    // a convenience rather than a life-support system. `docs/148` §9 item 3.
    // `view.reflow` sits in Show with the other view STATES rather than in Zoom:
    // it changes what is laid out, not how large it is drawn. All three
    // references file their equivalent under View for the same reason
    // (`docs/151` §6.1).
    // Folding joins Show, and the three rows are the three Word offers on a
    // heading's context menu: Expand/Collapse, Collapse All Headings, Expand All
    // Headings. They are view STATES of what the window shows — which is the
    // test every row in this band passes — and they sit beside `view.outline`
    // because the outline panel is the other surface that drives the same
    // `FoldSet`.
    //
    // The NINE level rungs (`view.fold.level.*`) stay palette-only, deliberately
    // and not by omission: nine inline rows for one choice would be three times
    // the longest band in this menu, a flyout would need a band name and
    // therefore a new key in nineteen catalogues, and Word itself puts level
    // selection in the Table-of-Contents dialog rather than on a menu. The
    // palette is a real surface and `view.fold.all` / `view.fold.none` are the
    // two rungs anybody reaches for.
    band(
      "menuGroup.show",
      "view.outline",
      "view.pages",
      "view.showChanges",
      "view.reflow",
      // Beside `view.reflow` because it is the question that view raises:
      // what is this layout not showing me the way the page does? The engine
      // has answered it since reflow shipped and nothing asked (`docs/166`
      // R-7), and the answer is document-derived, so the row is honest in both
      // views rather than reciting a fixed list.
      "view.reflowApproximations",
      "view.compactRibbon",
    ),
    // A SUBMENU, for the reason this file already applies to
    // `menuGroup.formattingMarks`: three inline rows for one gesture family that
    // most people reach through the heading chevron or the outline tree, not
    // through View. It is also the row that had to give. Adding
    // `view.reflowApproximations` took this menu to FOURTEEN top-level rows
    // against `menu-submenus.spec.mjs`'s cap of 13 — "no menu makes a reader scan
    // more than a screenful", which is the owner's own ask ("my ask was to group
    // them and create sub menus .. so it's readable"). CI said `view lists 14
    // rows at the top level`, and that cap is a UX guarantee, not a ratchet to
    // raise.
    //
    // Folding and not something else, by this file's own test: Show holds STATES
    // of the window, and `view.outline`, `view.pages`, `view.showChanges`,
    // `view.reflow` and `view.compactRibbon` each answer "what is the window
    // showing". The three fold rows are one FAMILY acting on headings, which is
    // the `menuGroup.formattingMarks` shape exactly — and the nine level rungs
    // below are already palette-only on the same argument one step further down.
    //
    // The band name is each catalogue's own `fold.treeLabel` — the outline
    // tree's label, the one string in all nineteen that already names these
    // things in that language. No translation here is one I invented.
    sub("menuGroup.headings", "view.fold.toggle", "view.fold.all", "view.fold.none"),
    // Formatting marks. Google Docs' only surface for this is View ▸ Show
    // non-printing characters, so the View menu is where a reader trained on Docs
    // looks; the ribbon's ¶ button is where a reader trained on Word looks, and
    // both exist.
    //
    // A SUBMENU, with the ¶ toggle as its first row and the five individual
    // switches under it. Six inline rows would make this the longest band in the
    // menu for one gesture most people only ever use whole — the same length
    // argument `menuGroup.textWidth` and `menuGroup.breaks` are flyouts for. It is
    // NOT in Show beside the other view states, because Show holds STATES of the
    // window and this is a set of five independent ones.
    sub(
      "menuGroup.formattingMarks",
      "view.formattingMarks",
      "view.formattingMarks.tab",
      "view.formattingMarks.space",
      "view.formattingMarks.paragraph",
      "view.formattingMarks.lineBreak",
      "view.formattingMarks.pageBreak",
    ),
    // Text width, a SUBMENU of four mutually exclusive steps, between Show and
    // Zoom (`docs/154` §5.1, ADR-048). It is where Google Docs puts View ▸ Text
    // width and where Word's Immersive Reader puts Column Width, and it is a
    // flyout rather than four inline rows because four rows of one radio group
    // would be the longest band in this menu for a single choice — the length
    // complaint `sub` exists to answer. It is NOT in Show: Show holds view
    // STATES, and this is a measure, which is the same distinction that keeps
    // `view.reflow` out of Zoom.
    sub(
      "menuGroup.textWidth",
      "view.textWidth.narrow",
      "view.textWidth.reading",
      "view.textWidth.fit",
      "view.textWidth.full",
    ),
    band("menuGroup.zoom", "view.zoomIn", "view.zoomOut"),
  ],
  // Word's Insert TAB group order, which ONLYOFFICE's Insert tab also follows:
  // Tables | Illustrations | Links | Comments | Header & footer | Text |
  // Symbols. The ten-row opening run this menu used to have — table, image,
  // shape, text box, link, bookmark, three field rows and drop cap in one
  // undifferentiated band — was the single worst instance of the owner's
  // report.
  //
  // `insert.field.page` and `insert.field.date` sit beside `insert.field`, not
  // inside the picker only: the Insert BAND gives those two kinds their own
  // buttons (Word's Insert tab has Page Number and Date & Time; ONLYOFFICE's
  // has both), and `insert-surface.spec.mjs` holds the ribbon and the MENU BAR
  // at exact parity on the `insert.` namespace — a ribbon face with no menu row
  // is the drift that guard exists to catch. The remaining four kinds stay
  // picker-only and are reachable from the palette by name.
  //
  // Footnote and endnote are NOT here any more; they are References ▸ Notes,
  // where Word, ONLYOFFICE and this editor's own References BAND already keep
  // them. Docs files a footnote under Insert, and it is outvoted two to one —
  // and outvoted a third time by our ribbon, which had the notes on References
  // while the menu had them on Insert.
  insert: [
    band("menuGroup.table", "insert.table"),
    // Breaks. A SUBMENU, because six rows of "… break" would make this menu long
    // for a group nobody opens twice in a paragraph — the same argument the
    // Export band makes on the File surface, and the shape ONLYOFFICE uses for
    // the same six (`Toolbar.js:2382-2390`).
    //
    // The ids are `layout.*` because a break IS page setup — Word files them
    // under Layout ▸ Page Setup — while Insert is where people look for them,
    // which is exactly why the three running-content rows below are `layout.*`
    // on this menu too. The menu home MATTERS and is not decoration: in compact
    // chrome the ribbon is hidden, so a command with only a band face is
    // palette-only there, which is the hole the References menu was added to
    // close.
    sub(
      "menuGroup.breaks",
      "layout.break.page",
      "layout.break.column",
      "layout.break.section.nextPage",
      "layout.break.section.continuous",
      "layout.break.section.evenPage",
      "layout.break.section.oddPage",
    ),
    band("menuGroup.illustrations", "insert.image", "insert.shape", "insert.chart"),
    band("menuGroup.links", "insert.link", "insert.bookmark"),
    band("menuGroup.comments", "review.comment"),
    sub(
      "menuGroup.headerFooter",
      "insert.header",
      "insert.footer",
      "layout.firstPageVariant",
      "layout.evenOddVariant",
      "layout.headerFooterSettings",
    ),
    band("menuGroup.text", "insert.textbox", "insert.dropCap"),
    sub("menuGroup.fields", "insert.field", "insert.field.page", "insert.field.date"),
    sub("menuGroup.symbols", "insert.symbol", "insert.emoji"),
  ],
  // The References menu, which the compact chrome did not have at all.
  //
  // This is the hole compact mode had that no guard could see: the table of
  // contents, captions and cross-references live on the References BAND, the
  // ribbon is hidden in compact mode, and the menu bar offered none of them —
  // so in the chrome the owner was looking at, a whole tab's worth of
  // capability was reachable only by typing its name into the palette. The
  // one-axis reachability guard counts the ribbon whichever chrome is showing,
  // which is why it stayed green over a real gap.
  //
  // Its bands are the References BAND's own groups, by the same names:
  // Navigation | Notes | Captions. Word's References tab reads Table of
  // Contents | Footnotes | Captions, and ONLYOFFICE's reads Table of Contents |
  // Footnotes | Caption — the same three, in the same order.
  references: [
    band(
      "menuGroup.navigation",
      "reference.tableOfContents",
      "reference.updateFields",
      "reference.goToHeading",
    ),
    band("menuGroup.notes", "insert.footnote", "insert.endnote"),
    band(
      "menuGroup.captions",
      "reference.caption",
      "reference.crossReference",
      "reference.updateCaptionNumbers",
    ),
  ],
  // Word's Home tab reads Font | Paragraph | Styles, and Docs' Format menu reads
  // Text | Align & indent | Line & paragraph spacing | Bullets & numbering |
  // Clear formatting. Both put CLEAR FORMATTING in a band of its own at the end;
  // it used to sit here next to superscript and subscript, where it reads as one
  // more character effect rather than as the row that throws the others away.
  //
  // Colour is split out of Word's single Font group deliberately. Word can hold
  // ten controls in one group because they are a grid of icons; a menu renders
  // them as ten stacked rows, which is the shape this change exists to stop.
  format: [
    sub(
      "menuGroup.font",
      "format.bold",
      "format.italic",
      "format.underline",
      "format.strike",
      "format.superscript",
      "format.subscript",
      "format.grow",
      "format.shrink",
    ),
    sub("menuGroup.textColor", "format.color", "format.highlight"),
    sub(
      "menuGroup.changeCase",
      "format.case.upper",
      "format.case.lower",
      "format.case.title",
      "format.case.sentence",
      "format.case.toggle",
    ),
    sub(
      "menuGroup.alignment",
      "paragraph.align.start",
      "paragraph.align.center",
      "paragraph.align.end",
      "paragraph.align.justify",
    ),
    // Checklist, restart and continue existed on the ribbon and in the palette
    // but in no menu, so browsing Format said the editor had no checklists at
    // all (docs/104 HF-076).
    sub(
      "menuGroup.lists",
      "paragraph.list.bullet",
      "paragraph.list.numbered",
      "paragraph.list.checklist",
      "paragraph.list.restart",
      "paragraph.list.continue",
    ),
    // Tab stops join the paragraph band because that is where Word keeps them —
    // the Tabs… dialog opens off the Paragraph launcher on both Home and Layout
    // — and because they are a paragraph property, which is what the band means.
    //
    // It is filed here for a reason that is not the phone's. `setTabStop` had
    // exactly two call sites, both inside `ruler.mjs`, and no command id: the
    // ruler was the ONLY surface in the product from which a tab stop could be
    // set, moved, retyped or removed, at every width, for every user. That is a
    // one-surface capability (`docs/105` UX-004) and it has been one all along;
    // the phone rung is only where it became visible, because a ruler showing
    // 0-3in of an 8.5in page is 24px of a 844px screen spent on a control
    // nobody drags with a finger. `docs/148` §9 item 7.
    sub(
      "menuGroup.paragraph",
      "paragraph.indent.decrease",
      "paragraph.indent.increase",
      "layout.tabStops",
      "layout.paragraph",
    ),
    // Copying formatting is a FORMAT action — filing it under Edit put it next
    // to cut/paste, where it reads as clipboard behaviour — and it belongs with
    // the two style-from-selection rows, because all three are "take the
    // formatting this selection already has and reuse it".
    sub(
      "menuGroup.styles",
      "format.painter",
      "style.updateFromSelection",
      "style.createFromSelection",
    ),
    band("menuGroup.clearFormatting", "format.clear"),
  ],
  // Every structural table command already ran through `tableToolCommands` and
  // was reachable from the right-click menu and the palette — and from no menu
  // at all (docs/105 UX-012), so a user browsing the bar was told the editor
  // could not edit tables. The rows below are the SAME command objects, so
  // gating, disabled reasons and the transactions they run cannot drift.
  //
  // The band names are Word's own Table Layout tab: Rows & Columns | Merge |
  // Cell Size | Data (sort). Deleting keeps the band it already had, which is
  // the safety rule above: "Delete table" must not sit one row under "Insert
  // column right".
  table: [
    sub(
      "menuGroup.rowsAndColumns",
      "table.insert.rowAbove",
      "table.insert.rowBelow",
      "table.insert.columnLeft",
      "table.insert.columnRight",
    ),
    sub("menuGroup.delete", "table.delete.row", "table.delete.column", "table.delete.table"),
    // Reordering. The gutter's drag is the pointer half (`docs/141` §4.2.3) and
    // these are the half a keyboard, a menu and the palette can reach — the rule
    // every other gesture in that layer already follows.
    sub(
      "menuGroup.move",
      "table.move.rowUp",
      "table.move.rowDown",
      "table.move.columnLeft",
      "table.move.columnRight",
    ),
    sub("menuGroup.select", "table.select.row", "table.select.column", "table.select.table"),
    // Merge, then the two ways out of one. `table.unmerge` is the gesture Word
    // and Google Docs both put on the right-click menu of a merged cell, and it
    // had no reachable path at all: the split dialog's smallest legal value is
    // 1x2, so no number a person could type unmerged a cell (`docs/141` TBL-02).
    sub("menuGroup.merge", "table.merge", "table.unmerge", "table.split"),
    // Sizing the caret's band from the keyboard. These are the ONLY table
    // commands with a chord (Alt+Shift+Arrow), and they are on the menu as well
    // because a chord is not a surface a user browses. Distributing rows and
    // columns is the same question — how big is this band — so Word's Cell Size
    // group holds both, and so does this one.
    sub(
      "menuGroup.cellSize",
      "table.column.grow",
      "table.column.shrink",
      "table.row.grow",
      "table.row.shrink",
      "table.distribute.rows",
      "table.distribute.columns",
    ),
    band("menuGroup.sort", "table.sort.ascending", "table.sort.descending"),
    // `table.borderStyle` sits with the other two format rows, which is where
    // Word keeps the pen: Table Design holds Line Style beside the border
    // controls rather than in a submenu of its own. It had three surfaces --
    // the cell-format popover, the in-table context menu and the palette --
    // and not this one, which is the single-surface shape `105` UX-012 is
    // about: the menu is the surface a reader browses when they do not already
    // know the capability exists.
    band(
      "menuGroup.properties",
      "table.cellFormat",
      "table.borderStyle",
      "table.properties",
    ),
    // Clearing the table style. The named styles are generated per document, so
    // they cannot be listed here — the ribbon's chooser and the palette's
    // `table.style.<name>` rows are their two surfaces — but "back to no style"
    // is a fixed command and the Table menu is where Word keeps it.
    band("menuGroup.style", "table.style.none"),
  ],
  // Word's Review tab order, which ONLYOFFICE's also follows: Proofing first,
  // then Comments, then Tracking, then Changes. Proofing led this menu in
  // neither reference before — it was last, under three bands of change
  // navigation — and it is the band a person opens Review for most often.
  //
  // Accept-all and reject-all are their own band. They were one row below
  // "Accept change and move to next", which is an irreversible whole-document
  // action one keystroke from a single-change one.
  review: [
    // Proofing. Word's Review tab opens with a Proofing group, and these three
    // are the only proofing switches this editor has. They were the content of
    // a `Tools` menu that existed for them alone.
    // Increment B adds a fourth row: the language-pack install surface
    // (`docs/146` §3/§10, ADR-042). Word keeps its install table in File ▸ Options
    // ▸ Language and this editor has no Options backstage, so the Proofing band —
    // where the switches already are — is the menu home, and the Review band and
    // the palette are its other two surfaces (`105` UX-004 forbids one).
    sub(
      "menuGroup.proofing",
      "tools.spellCheck",
      "tools.grammarCheck",
      "tools.smartQuotes",
      "tools.languages",
    ),
    band("menuGroup.comments", "review.comment.resolve", "review.comment.delete"),
    band("menuGroup.tracking", "review.toggle"),
    sub(
      "menuGroup.changes",
      "review.previous",
      "review.next",
      "review.acceptNext",
      "review.rejectNext",
    ),
    band("menuGroup.allChanges", "review.acceptAll", "review.rejectAll"),
    // Compare, last on the Review menu, which is where Word keeps it: their
    // Review tab reads Proofing | Comments | Tracking | Changes | Compare, and
    // Compare is the rightmost group. ONLYOFFICE files theirs under
    // Collaboration ▸ Compare behind `canReview`.
    band("menuGroup.compare", "review.compare"),
    // Protect, its own band and last, which is Word 365's own Review-tab order
    // (… Changes | Compare | Protect | Ink). Its own band rather than a row under
    // Compare because the rule every band in these menus follows is that a
    // document-wide, hard-to-notice action does not sit one keystroke from a
    // reversible navigation one.
    // Protect holds BOTH authority questions, which is the grouping Word's own
    // Review tab uses: what this document asks of everyone (`restrictEditing`,
    // `w:documentProtection`) and what the people in a shared session may do
    // (`manageAccess`). They are genuinely different authorities — `access.rs`
    // is built on keeping them apart — and they are the same QUESTION to a
    // reader looking for "who can change this", which is what a menu band is
    // for. A band of its own would have cost a new `menuGroup.*` key in
    // nineteen catalogues to separate two rows nobody is looking for
    // separately.
    band("menuGroup.protect", "review.restrictEditing", "review.manageAccess"),
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
  ["table.move.rowUp", "Move row up"],
  ["table.move.rowDown", "Move row down"],
  ["table.move.columnLeft", "Move column left"],
  ["table.move.columnRight", "Move column right"],
  ["table.select.row", "Select row"],
  ["table.select.column", "Select column"],
  ["table.select.table", "Select table"],
  ["table.merge", "Merge cells"],
  ["table.unmerge", "Unmerge cells"],
  ["table.split", "Split cell…"],
  ["table.distribute.rows", "Distribute rows"],
  ["table.distribute.columns", "Distribute columns"],
  ["table.sort.ascending", "Sort ascending"],
  ["table.sort.descending", "Sort descending"],
  ["table.column.grow", "Widen column"],
  ["table.column.shrink", "Narrow column"],
  ["table.row.grow", "Taller row"],
  ["table.row.shrink", "Shorter row"],
  ["table.cellFormat", "Cell formatting…"],
  // Word's own wording in Table Design, and an ellipsis because the row opens
  // the cell-format popover with the pen focused rather than applying a style.
  ["table.borderStyle", "Border line style…"],
  ["table.properties", "Table properties…"],
  ["table.style.none", "No table style"],
]);

/** The menu names the bar offers, in bar order. */
export function appMenuNames() {
  return Object.keys(APP_MENU_SECTIONS);
}

/** Every command id the named menu offers, flattened out of its named bands
 *  and in the order the popover renders them. An unknown menu name gives `[]`,
 *  which is what `renderAppMenu` already does with one. */
export function menuCommandIds(name) {
  return sectionCommandIds(APP_MENU_SECTIONS[name] ?? []);
}

/** Every command id a list of bands holds, in render order. The one place that
 *  knows a band is `{nameKey, ids}` and not a bare array, so a surface that
 *  needs the flat roster — the compact toolbar's Table dropdown, the parity
 *  guards — never has to know either. */
export function sectionCommandIds(sections) {
  return sections.flatMap((section) => section.ids);
}

/** Every band in every menu, tagged with the menu it belongs to. What the
 *  grouping guards walk: they ask about bands across the whole bar, and doing
 *  that by hand needed a nested loop in each of them. */
export function allMenuSections() {
  return appMenuNames().flatMap((menu) =>
    (APP_MENU_SECTIONS[menu] ?? []).map((section) => ({ menu, ...section })),
  );
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
