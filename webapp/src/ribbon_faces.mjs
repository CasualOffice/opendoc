// Which command each ribbon control stands for — the Home, View and Table bands
// (`109` UX-005 / `105` CQ-004).
//
// THE DEFECT AS IT STOOD WHEN THIS FILE WAS WRITTEN — a historical measurement,
// not the current state, and dated because it kept being read as current. On
// 2026-09-26, re-measured in the browser before anything was changed (the row's
// own "~23 of ~90" and its 2026-09-20 recount of "39 of 109" were both stale
// already): 52 of 120 visible ribbon controls carried a command id. The ribbon has
// grown since — 123 controls, 114 with `data-command`, 9 declaring a chooser
// family, 0 unclassified as of 2026-09-27 — so do not quote 120 as a control
// count. `ribbon-command-faces.spec.mjs` measures the live figure; that is the
// one to trust, and the numbers here only explain why the work happened. Insert (17/17), Layout
// (13/13), References (7/7) and Review (15/15) are declarative — `INSERT_SURFACE`,
// `LAYOUT_SURFACE`, `REFERENCE_SURFACE` and `REVIEW_SURFACE` each stamp
// `dataset.command` onto their buttons — and Home (0/41), View (0/8) and Table
// (0/19) were bound by hand, one `addEventListener("click")` at a time, scattered
// over nine regions of `main.js`. The tabs that are declarative are exactly the
// tabs with set-equality parity tests, which is the argument for extending the
// declaration rather than a coincidence (`105` CQ-004 says so in as many words).
//
// WHAT THIS TABLE IS, AND IS NOT. It is a DECLARATION OF IDENTITY: "this control
// is a face of that command". It is not a second wiring. The existing handlers
// stay, because several registry commands are *implemented by* these very
// controls — `format.clear` runs `clearFormattingBtn.click()`,
// `format.superscript` runs `superBtn.click()`, `format.family` runs
// `fontFamilyBtn.click()`, `table.cellFormat` runs `tableBtn.click()` — so
// routing the button's own click back through the registry would be a loop, not
// a unification. What makes the declaration load-bearing rather than decorative
// is the guard: `ribbon-command-faces.spec.mjs` drives each control in a browser
// and asserts it produces the same observable effect as running the id it names.
// #623 is the reason that guard exists — `⌘⌥M` was bound to `comment.add`, an id
// the registry does not return, and a table-only check passed it.
//
// TWO KINDS OF FACE, because a ribbon holds two kinds of control:
//
//   `command`  the control runs one command. It carries `data-command`, the same
//              attribute the four declarative tabs already stamp, so every
//              existing parity guard sees it without being taught anything.
//
//   `family`   the control opens a CHOOSER over a family of commands — the
//              change-case menu, the underline-style menu, the two list
//              galleries, the spacing menu, the styles list, the two colour
//              pickers' apply halves. There is no single command to name: every
//              member of the family is its own registry command (`format.case.*`,
//              `paragraph.listFormat.*`, `style.*`, …) and already reachable from
//              the palette, so the button is a doorway, not a capability. It
//              carries `data-command-family` with the id PREFIX it discloses, and
//              the guard asserts at least one live registry command sits under
//              that prefix. Inventing a `format.case` "command" whose only
//              behaviour is to open a menu would add a row to the palette that
//              does nothing a user asked for, and a string to translate in
//              nineteen languages, to satisfy a guard.
//
// Nothing may be unclassified. A visible ribbon control with neither attribute
// fails the guard, which is how the next hand-bound button gets caught at the
// moment it is added rather than at the next audit.
//
// RELATED, AND NOT YET ONE MECHANISM: `one-axis-navigation.spec.mjs` carries a
// hand-written `VALUE_FAMILIES` list mapping the same prefixes to the same
// controls, so that a `format.size.14` row is not reported as palette-only. Six of
// its eight entries are now declared here, in the DOM, where the app itself says
// it. The remaining two are not ribbon controls (`view.zoom.` is owned by the
// footer's zoom field, `insert.field.` by the field dialog), so that list cannot
// simply be derived from this one yet — but it should be, and the way there is to
// let those two surfaces declare their family too rather than to grow a second
// copy of this table.
//
// Selectors, not element references, so this file is DATA: it can be read and
// checked in node with no DOM (the caller hands in the root, so nothing here
// reaches for a global `document`), and `main.js` keeps its own element bindings.
// Labels are never mentioned here, so the module adds no translatable string
// (the `ruler.mjs` shape).

/** A face declaration: one of `command` (runs it) or `family` (discloses it). */
const face = (select, command) => ({ select, command });
const chooser = (select, family) => ({ select, family });

/** The Home band. Word's group order — Undo, Clipboard, Font, Paragraph, Styles,
 *  Editing — and the ids are the ones the Edit and Format menus already use, so
 *  Home stops being the one tab whose membership only a human could read.
 *
 *  `#fontSize` is an `<input>`, not a button: `format.size` focuses and selects
 *  it, which is what "Font size…" means on a ribbon that has the field right
 *  there. It is a face of that command as much as a button would be. */
export const HOME_FACES = Object.freeze([
  face("#undoBtn", "edit.undo"),
  face("#redoBtn", "edit.redo"),
  face("#clearFormatting", "format.clear"),
  face("#formatPainter", "format.painter"),
  face("#pasteBtn", "edit.paste"),
  face("#cutBtn", "edit.cut"),
  face("#copyBtn", "edit.copy"),
  face("#fontFamily", "format.family"),
  face("#fontSize", "format.size"),
  face("#shrinkFont", "format.shrink"),
  face("#growFont", "format.grow"),
  chooser("#changeCaseBtn", "format.case."),
  face("#bold", "format.bold"),
  face("#italic", "format.italic"),
  face("#underline", "format.underline"),
  chooser("#underlineMenuBtn", "format.underline."),
  face("#strike", "format.strike"),
  face("#subscript", "format.subscript"),
  face("#superscript", "format.superscript"),
  // The split colour controls. The caret half IS `format.color` — that command's
  // run opens this very popover — while the apply half repeats the swatch the
  // control already shows, which is an accelerator for a choice the family
  // offers, not a capability of its own. Word's split button, exactly.
  chooser("#textColorApply", "format.color"),
  face("#textColor", "format.color"),
  chooser("#highlightApply", "format.highlight"),
  face("#highlight", "format.highlight"),
  face("#bulletList", "paragraph.list.bullet"),
  chooser("#bulletListMenuBtn", "paragraph.listFormat.bullet"),
  face("#numberedList", "paragraph.list.numbered"),
  chooser("#numberedListMenuBtn", "paragraph.listFormat."),
  face("#checkList", "paragraph.list.checklist"),
  face("#indentDec", "paragraph.indent.decrease"),
  face("#indentInc", "paragraph.indent.increase"),
  face("#paraOptsBtn", "layout.paragraph"),
  face("#alignStart", "paragraph.align.start"),
  face("#alignCenter", "paragraph.align.center"),
  face("#alignEnd", "paragraph.align.end"),
  face("#alignJustify", "paragraph.align.justify"),
  chooser("#spacingBtn", "paragraph.spacing."),
  face("#restartList", "paragraph.list.restart"),
  face("#continueList", "paragraph.list.continue"),
  chooser("#stylesTrigger", "style."),
  // Find and Replace are two faces of ONE command: `#replaceBtn`'s handler is
  // `findBtn.click()`, and the panel it opens is the same panel with the replace
  // row focused. Declaring both against `edit.find` says that out loud; declaring
  // a separate `edit.replace` would be a second name for one dialog.
  face("#findBtn", "edit.find"),
  face("#replaceBtn", "edit.find"),
]);

/** The View band. `#reviewBtn` is `review.toggle`'s second ribbon face — the
 *  Review band has the other one — which is legitimate and is why the declarative
 *  tables all take `buttons`, plural. */
export const VIEW_FACES = Object.freeze([
  face("#viewOutlineBtn", "view.outline"),
  face("#reviewBtn", "review.toggle"),
  // Version history's second ribbon face; the File page's row is the other
  // surface. Declared here for the same reason every other View control is: a
  // visible ribbon control with no command id fails `ribbon-command-faces`, and
  // that is how a hand-bound button gets caught the moment it is added.
  face("#viewVersionsBtn", "file.versionHistory"),
  face("#viewZoomOut", "view.zoomOut"),
  face("#viewZoomIn", "view.zoomIn"),
  face("#viewFitWidth", "view.zoom.fitWidth"),
  face("#viewFitPage", "view.zoom.fitPage"),
  // `view.zoom.100` is generated from the status-bar zoom menu's own preset list,
  // and its run is `setZoom(1)` — which is exactly what `data-zoom-action="actual"`
  // does (`view_zoom.mjs`).
  face("#viewZoomActual", "view.zoom.100"),
  face("#pageSetupBtn", "layout.pageSetup"),
]);

/** The contextual Table band. Its buttons are keyed by `data-table-*` in the
 *  markup and bound by four delegated loops, so unlike Home they were already
 *  data-driven — they simply never named the command they run, which is the half
 *  a parity test needs.
 *
 *  `#tableStyleBtn` is a chooser over `table.style.*`. */
export const TABLE_FACES = Object.freeze([
  face('[data-table-select="row"]', "table.select.row"),
  face('[data-table-select="column"]', "table.select.column"),
  face('[data-table-select="table"]', "table.select.table"),
  face('[data-table-action="insert-row-above"]', "table.insert.rowAbove"),
  face('[data-table-action="insert-row-below"]', "table.insert.rowBelow"),
  face('[data-table-action="insert-column-left"]', "table.insert.columnLeft"),
  face('[data-table-action="insert-column-right"]', "table.insert.columnRight"),
  face('[data-table-action="delete-row"]', "table.delete.row"),
  face('[data-table-action="delete-column"]', "table.delete.column"),
  face('[data-table-action="delete-table"]', "table.delete.table"),
  face('[data-table-distribute="rows"]', "table.distribute.rows"),
  face('[data-table-distribute="columns"]', "table.distribute.columns"),
  face('[data-table-sort="ascending"]', "table.sort.ascending"),
  face('[data-table-sort="descending"]', "table.sort.descending"),
  face("#mergeCellsBtn", "table.merge"),
  face("#splitCellBtn", "table.split"),
  face("#tableBtn", "table.cellFormat"),
  face("#tablePropertiesBtn", "table.properties"),
  chooser("#tableStyleBtn", "table.style."),
]);

/** Every band this module speaks for, by ribbon panel id. */
export const RIBBON_FACES = Object.freeze({
  panelHome: HOME_FACES,
  panelView: VIEW_FACES,
  panelTable: TABLE_FACES,
});

/** Stamps the declarations onto the live controls.
 *
 *  Returns the selectors that matched nothing. A caller that ignores that is
 *  choosing to let a renamed control silently lose its command id, so `main.js`
 *  does not ignore it — a face with no control is a markup/JS disagreement and
 *  the guard reports it by name.
 *
 *  Complexity: O(faces) once at boot. Touches no document state. */
export function stampRibbonFaces(root, faces = RIBBON_FACES) {
  const unresolved = [];
  for (const [panelId, entries] of Object.entries(faces)) {
    const panel = root.getElementById?.(panelId) ?? root.querySelector(`#${panelId}`);
    for (const entry of entries) {
      const control = panel?.querySelector(entry.select);
      if (!control) {
        unresolved.push(`${panelId} ${entry.select}`);
        continue;
      }
      if (entry.command) control.dataset.command = entry.command;
      else control.dataset.commandFamily = entry.family;
    }
  }
  return unresolved;
}

/** Every command id the tables name, for a guard that wants to check them
 *  against the live registry without re-walking the DOM. */
export function declaredCommandIds(faces = RIBBON_FACES) {
  return Object.values(faces)
    .flat()
    .filter((entry) => entry.command)
    .map((entry) => entry.command);
}

/** Every family prefix the tables name. */
export function declaredFamilies(faces = RIBBON_FACES) {
  return Object.values(faces)
    .flat()
    .filter((entry) => entry.family)
    .map((entry) => entry.family);
}
