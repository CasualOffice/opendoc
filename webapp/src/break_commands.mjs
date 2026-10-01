// Breaks: the page break, the column break and the four section breaks.
//
// `SKILL` §9 rule 4 — "built" is not "reachable" — in its most expensive form.
// `insertBreak("page")` and `insertSectionBreak(start)` have been in
// `casual-doc-wasm` for some time, with their own refusals, their own undo
// labels and their own engine tests, and the string `insertBreak` did not appear
// anywhere in `webapp/` at all. So the most-used break in a word processor could
// not be inserted, and a document could only ever have the sections it was
// imported with. `docs/153` ranks the page break 1 and the section break 2.
//
// THE SHAPE, FROM THE THREE REFERENCES RATHER THAN FROM THE BINDING.
//
//   * Word 365: Layout ▸ Page Setup ▸ Breaks is ONE dropdown holding Page,
//     Column and Text Wrapping under a "Page Breaks" heading and Next Page,
//     Continuous, Even Page and Odd Page under "Section Breaks". Insert ▸ Pages
//     carries Page Break again, on its own, because it is the one people reach
//     for constantly. Ctrl+Enter is the page break.
//   * ONLYOFFICE, read from their source rather than their documentation
//     (`apps/documenteditor/main/app/view/Toolbar.js:2382-2390`): the Insert tab
//     has one Breaks button whose menu is Page Break / Column Break / Insert
//     Section Break, the last opening a submenu of exactly the four start types
//     (`:2372-2380`, `Asc.c_oAscSectionBreakType`).
//   * Google Docs: Insert ▸ Break, flat, with the full name on each row —
//     "Page break", "Section break (next page)", "Section break (continuous)",
//     "Column break".
//
// So: ONE dropdown on the Insert band, holding all six, plus Ctrl/Cmd+Enter for
// the page break. The rows are FLAT with full names, which is Google Docs'
// choice rather than Word's heading-plus-short-name, for one reason — the same
// label has to serve the command palette, where "Next Page" alone says nothing.
// One label per command is this repository's rule and it decides the tie.
//
// WHERE WE DELIBERATELY DIFFER. Word's third page-break kind, Text Wrapping
// (`w:br w:type="textWrapping"`), is not here: that is a line break, the editor
// already has it on Shift+Enter as `insert.lineBreak`, and offering it a second
// time under a different name in a Breaks menu would be two names for one
// gesture. ONLYOFFICE does not offer it in their Breaks menu either.
//
// WHY THE IDS ARE `layout.break.*` AND NOT `insert.*`. Breaks are page setup —
// Word files them under Layout ▸ Page Setup, and a section break IS a page-setup
// boundary. The `layout.*` namespace with a face on the Insert band is a shape
// this chrome already uses three times over — `layout.firstPageVariant`,
// `layout.evenOddVariant` and `layout.headerFooterSettings` are all `layout.*`
// commands with Insert-band buttons and Insert-menu rows, because what they
// configure is the page while where people look for them is Insert.
//
// NO ENGINE CHANGE. Every binding this needs already existed; what was missing
// was a control, a menu row, a contract row and a chord.
import { t } from "./i18n.mjs";
// The popover registry, imported rather than injected. ESM gives one instance of
// it, so a popover registered here joins the same list `main.js`'s do and gets
// the same outside-click dismissal, Escape handling, `aria-expanded` bookkeeping
// and focus restoration. Injecting these two would have bought nothing but two
// more lines in the file the ratchet is protecting.
import { onButton, registerPopover } from "./popover_manager.mjs";

/** The four section start types, in the order all three references list them.
 *
 *  The strings are the engine's own `insertSectionBreak` vocabulary
 *  (`"nextPage"`, `"continuous"`, `"evenPage"`, `"oddPage"`), so the control
 *  passes what the engine named rather than mapping one spelling to another. */
export const SECTION_STARTS = Object.freeze(["nextPage", "continuous", "evenPage", "oddPage"]);

/** Command id -> what to ask the engine for.
 *
 *  `kind` is a forced break (`insertBreak`); `start` is a section boundary
 *  (`insertSectionBreak`). Exactly one is present on each row, so a row cannot
 *  mean both and `run` needs no third branch.
 *
 *  The order is the order the dropdown and the Insert menu show, and the order
 *  is the references': the two forced breaks, then the four section starts. */
export const BREAK_COMMANDS = Object.freeze([
  Object.freeze({ command: "layout.break.page", kind: "page", labelKey: "break.page", kw: "page break new page ctrl enter force next page pagination" }),
  Object.freeze({ command: "layout.break.column", kind: "column", labelKey: "break.column", kw: "column break newspaper columns next column force" }),
  ...SECTION_STARTS.map((start) =>
    Object.freeze({
      command: `layout.break.section.${start}`,
      start,
      labelKey: `break.section.${start}`,
      kw: `section break ${start.toLowerCase()} page setup divide margins orientation columns header footer`,
    }),
  ),
]);

/** Every break command id, for the guards and the host contract. */
export const BREAK_COMMAND_IDS = Object.freeze(BREAK_COMMANDS.map((row) => row.command));

/** The dropdown's ribbon face and the popover it opens, in `editor.html`.
 *
 *  Named here rather than passed in because this module is the only thing that
 *  reads either one, and two ids travelling through `main.js` as host members
 *  would be two more lines in the file the ratchet protects for no reader. */
const BREAKS_BUTTON_ID = "insertBreaksBtn";
const BREAKS_MENU_ID = "insertBreaksMenu";

/**
 * The break rows, shaped for `LAYOUT_SURFACE`.
 *
 * `label` is a GETTER, not a value, and that is load-bearing. `LAYOUT_SURFACE`
 * is a module-level array evaluated when `main.js` is imported — before any
 * locale catalogue has been fetched — so a `label: t(key)` here would freeze the
 * KEY into the palette in every language, which is why every sibling row in that
 * table carries an English literal and a line of unrouted-string debt instead.
 * A getter is read when the palette builds its rows, which is after the
 * catalogue is installed and again after a locale change. So these six commands
 * are localised in all eighteen languages and this module carries no English at
 * all — a ceiling of zero rather than one of six.
 *
 * `requires: "caret"` and never `"missing"`: the engine honours a forced break
 * only in the body's own block list and refuses anywhere else — a table cell, a
 * text box, a content control, a header or footer, a note, a comment — by NAME,
 * with a `refused:`-coded sentence that `editRefusalMessage` passes through
 * verbatim. So the control stays live wherever there is a paragraph and the
 * engine does the explaining, which is better than a greyed button: "A break
 * cannot be inserted inside a table" tells a reader what to do, and a dark
 * button tells them nothing.
 *
 * Complexity: O(1) per row. Nothing here reads the document.
 *
 * @param {object} io
 * @param {() => object|null} io.doc the live document, or null.
 * @param {() => {node: string, offset: number}|null} io.caret the caret position.
 * @param {(thunk: () => unknown, options?: object) => Promise<boolean>} io.runEdit
 * @param {(text: string) => void} io.setStatus
 */
export function breakSurfaceRows(io) {
  return BREAK_COMMANDS.map((row) => ({
    command: row.command,
    get label() {
      return t(row.labelKey);
    },
    kw: row.kw,
    buttons: () => [document.getElementById(BREAKS_BUTTON_ID)].filter(Boolean),
    requires: "caret",
    // The button's click opens the popover, which `registerPopover` owns, so the
    // surface table must not attach a second handler to it — the same split
    // `layout.lineNumbers` spells out. Every ROW in the popover runs its own
    // command, and so does the chord and the menu entry.
    ownsClick: true,
    run: () => void insertBreak(io, row),
  }));
}

/**
 * Asks the engine for one break and says what happened.
 *
 * Through `io.runEdit` with `gate: true`, so it is refused in Viewing mode with
 * the mode's own sentence and gated in Suggesting mode exactly as every other
 * mutation is — a break is not a special case and must not grow a second policy.
 *
 * Says something on SUCCESS as well as on refusal. A break is invisible at the
 * caret until the page reflows below it, and on a short document it may move
 * nothing on screen at all, so silence reads as "nothing happened" — which is
 * the one outcome `docs/67` forbids.
 */
async function insertBreak(io, row) {
  const doc = io.doc();
  const caret = io.caret();
  if (!doc || !caret) return;
  const applied = await io.runEdit(
    () =>
      row.kind === undefined
        ? doc.insertSectionBreak(caret.node, caret.offset, row.start)
        : doc.insertBreak(caret.node, caret.offset, row.kind),
    { gate: true },
  );
  // `runEdit` has already said WHY when it refused; a second sentence here would
  // overwrite the engine's own reason with a vaguer one.
  //
  // Three literal `t()` calls rather than one composed key. `build-locale.mjs`
  // extracts the catalogue by SCANNING for `t("literal")`, so a template literal
  // here is collected as the key `break.inserted.${row.kind ?? ` and reported as
  // called-but-undeclared — the extractor is right to say so, because a key it
  // cannot read is a key no translator is ever shown.
  if (!applied) return;
  if (row.kind === "page") io.setStatus(t("break.inserted.page"));
  else if (row.kind === "column") io.setStatus(t("break.inserted.column"));
  else io.setStatus(t("break.inserted.section"));
}

/**
 * Wires the Breaks dropdown: the popover itself and the command each row runs.
 *
 * Each row in the popover carries the command id it runs, so the markup and this
 * module cannot drift into disagreeing about which row means which break, and a
 * guard can read the popover's roster without reading this file.
 *
 * It CLOSES before it acts. A menu that stays open over the place the break just
 * landed hides the one thing the reader wants to look at, and closing through the
 * manager rather than hiding the element keeps `aria-expanded` honest and sends
 * the keyboard back to the trigger.
 *
 * Returns nothing on purpose: the rows a surface needs come from
 * `breakSurfaceRows`, and a second object holding the same commands would be a
 * second place to look for them.
 *
 * @param {object} io the same host `breakSurfaceRows` takes.
 */
export function bindBreaksMenu(io) {
  const button = document.getElementById(BREAKS_BUTTON_ID);
  const menu = document.getElementById(BREAKS_MENU_ID);
  if (!button || !menu) return;
  const popover = registerPopover(button, menu, () => {});
  const byId = new Map(BREAK_COMMANDS.map((row) => [row.command, row]));
  for (const row of menu.querySelectorAll("[data-command]")) {
    const spec = byId.get(row.dataset.command);
    if (!spec) continue;
    onButton(row, () => {
      popover.close();
      void insertBreak(io, spec);
    });
  }
}
