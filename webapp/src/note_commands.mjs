// Footnotes and endnotes: the options dialog, Next / Previous note, and Convert
// (`docs/153` `page.note.settings`, `page.note.goto`, `page.note.convert`;
// `docs/109` RM-24). The engine half is `casual-doc-wasm` `notes_settings.rs`.
//
// ---- THE SHAPE, FROM THE REFERENCES ------------------------------------------
//
//   * Word 365: References ▸ Footnotes holds Insert Footnote, Insert Endnote and
//     a Next Footnote split button (Next/Previous Footnote, Next/Previous
//     Endnote); the group's launcher opens the Footnote and Endnote dialog —
//     Location (footnotes: Bottom of page / Below text; endnotes: End of section
//     / End of document), Number format, Start at, Numbering (Continuous /
//     Restart each section / Restart each page), Apply changes to (This section /
//     Whole document) — whose Convert… button offers Convert all footnotes to
//     endnotes, Convert all endnotes to footnotes, and Swap.
//   * ONLYOFFICE: the References tab's Footnote dropdown carries the same set —
//     `DE.Views.Links` `textGotoFootnote` (with previous/next arrows),
//     `mniNoteSettings` and `mniConvertNote` with the same three conversions.
//
// So: ONE dropdown on the References band's Notes group holding the four
// navigation rows, Note settings… and Convert notes… — ONLYOFFICE's grouping,
// which costs the band one button rather than Word's two. The same six rows
// are References-menu rows and palette rows, declared once in
// `NOTE_COMMANDS` and handed to the References surface table, so the three
// faces cannot disagree about what is available or why not.
//
// Everything that changes the document goes through `runEdit` with the gate, as
// one undoable step; navigation changes nothing and needs no grant.
import { t } from "./i18n.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";
import { registerModal } from "./modal.mjs";

/** The six commands. `kind`/`forward` for the four navigation rows. */
export const NOTE_COMMANDS = Object.freeze([
  Object.freeze({ command: "reference.nextFootnote", labelKey: "notes.nextFootnote", kind: "footnote", forward: true, kw: "next footnote go to note navigate" }),
  Object.freeze({ command: "reference.previousFootnote", labelKey: "notes.previousFootnote", kind: "footnote", forward: false, kw: "previous footnote go to note navigate back" }),
  Object.freeze({ command: "reference.nextEndnote", labelKey: "notes.nextEndnote", kind: "endnote", forward: true, kw: "next endnote go to note navigate" }),
  Object.freeze({ command: "reference.previousEndnote", labelKey: "notes.previousEndnote", kind: "endnote", forward: false, kw: "previous endnote go to note navigate back" }),
  Object.freeze({ command: "reference.noteSettings", labelKey: "notes.settingsCommand", kw: "footnote endnote settings options number format start at restart location numbering" }),
  Object.freeze({ command: "reference.convertNotes", labelKey: "notes.convertCommand", kw: "convert footnotes endnotes swap change note kind" }),
]);

/** The note positions each kind can take (`ST_FtnPos` / `ST_EdnPos`), in Word's order. */
export const NOTE_POSITIONS = Object.freeze({
  footnote: Object.freeze(["pageBottom", "beneathText"]),
  endnote: Object.freeze(["sectionEnd", "documentEnd"]),
});

/** The restart choices each kind offers. Word does not offer "each page" for endnotes. */
export const NOTE_RESTARTS = Object.freeze({
  footnote: Object.freeze(["continuous", "eachSection", "eachPage"]),
  endnote: Object.freeze(["continuous", "eachSection"]),
});

/**
 * The dialog's choice as the engine's `NoteProperties` JSON. Pure, so the shape
 * the engine is handed is checked in node. A number format the dialog does not
 * offer (an imported `chicago`, say) is carried through as the value it read.
 *
 * @param {{position: string, numberFormat: unknown, start: number|string, restart: string}} choice
 */
export function notePropertiesJson(choice) {
  const start = Math.trunc(Number(choice.start));
  return JSON.stringify({
    position: choice.position,
    numberFormat: choice.numberFormat,
    numberStart: Number.isFinite(start) ? Math.min(Math.max(start, 0), 32767) : 1,
    numberRestart: choice.restart,
  });
}

/**
 * The References-surface rows for the six commands. `label` is a getter, read
 * when a surface is built rather than at import, so it is localised.
 *
 * @param {object} ed the editor context; `ed.noteCommands` is read lazily.
 */
export function noteSurfaceRows(ed) {
  return NOTE_COMMANDS.map((row) => ({
    command: row.command,
    get label() {
      return t(row.labelKey);
    },
    kw: row.kw,
    buttons: () => [document.getElementById("refNotesMenuBtn")].filter(Boolean),
    requires: "caret",
    // The button opens the popover, which `registerPopover` owns.
    ownsClick: true,
    run: () => ed.noteCommands.run(row.command),
  }));
}

/** Installs the menu and the two dialogs on the editor context `ed`. */
export function createNoteCommands(ed) {
  const el = (id) => document.getElementById(id);

  /** Next or previous note of `kind` from the caret; says so at either end. */
  function go(kind, forward) {
    const caret = ed.selection?.focus;
    if (!ed.doc || !caret) return;
    let hit = null;
    try {
      hit = JSON.parse(ed.doc.adjacentNote(kind, caret.node, caret.offset, forward));
    } catch {
      hit = null;
    }
    if (!hit) {
      ed.setStatus(kind === "footnote" ? t("notes.noMoreFootnotes") : t("notes.noMoreEndnotes"));
      return;
    }
    ed.navigateToAnchor(hit.node, hit.offset);
  }

  // ---- Note settings ---------------------------------------------------------
  const settingsDialog = el("noteSettingsDialog");
  const kindSelect = el("noteSettingsKind");
  const positionSelect = el("noteSettingsPosition");
  const formatSelect = el("noteSettingsFormat");
  const startInput = el("noteSettingsStart");
  const restartSelect = el("noteSettingsRestart");
  const scopeSelect = el("noteSettingsScope");
  /** The number format the dialog opened on, for a format it has no row for. */
  let openedFormat = "decimal";
  const settingsModal = settingsDialog
    ? registerModal(settingsDialog, {
        initialFocus: () => kindSelect,
        fallbackFocus: () => ed.pagesEl,
        defaultAction: () => void applySettings(),
      })
    : null;

  /** Shows the options a kind can take and nothing else. O(options). */
  function reflectKindChoices(kind) {
    for (const option of positionSelect?.options ?? []) {
      option.hidden = !NOTE_POSITIONS[kind].includes(option.value);
    }
    for (const option of restartSelect?.options ?? []) {
      option.hidden = !NOTE_RESTARTS[kind].includes(option.value);
    }
  }

  /** Loads what the caret's section resolves to for `kind`. */
  function loadSettings(kind) {
    const caret = ed.selection?.focus;
    let current = null;
    try {
      current = JSON.parse(ed.doc.noteSettings(kind, caret?.node ?? ""));
    } catch {
      current = null;
    }
    reflectKindChoices(kind);
    if (!current) return;
    if (positionSelect) positionSelect.value = current.position;
    openedFormat = current.numberFormat;
    const known = typeof openedFormat === "string" && [...(formatSelect?.options ?? [])].some((option) => option.value === openedFormat);
    const keep = formatSelect?.querySelector('option[value="keep"]');
    if (keep) keep.hidden = known;
    if (formatSelect) formatSelect.value = known ? openedFormat : "keep";
    if (startInput) startInput.value = String(current.start);
    if (restartSelect) restartSelect.value = NOTE_RESTARTS[kind].includes(current.restart) ? current.restart : "continuous";
  }

  function openSettings() {
    if (!ed.doc || !settingsModal) return;
    const kind = kindSelect?.value === "endnote" ? "endnote" : "footnote";
    loadSettings(kind);
    if (scopeSelect) scopeSelect.value = "section";
    settingsModal.open();
  }

  async function applySettings() {
    if (!ed.doc) return;
    const kind = kindSelect?.value === "endnote" ? "endnote" : "footnote";
    const caret = ed.selection?.focus;
    const json = notePropertiesJson({
      position: positionSelect?.value,
      numberFormat: formatSelect?.value === "keep" || !formatSelect ? openedFormat : formatSelect.value,
      start: startInput?.value ?? 1,
      restart: restartSelect?.value ?? "continuous",
    });
    const scope = scopeSelect?.value === "document" ? "document" : "section";
    const applied = await ed.runEdit(() => ed.doc.setNoteSettings(kind, caret?.node ?? "", scope, json), { gate: true });
    if (!applied) return;
    settingsModal.close();
    ed.setStatus(kind === "footnote" ? t("notes.footnoteSettingsApplied") : t("notes.endnoteSettingsApplied"));
  }

  kindSelect?.addEventListener("change", () => loadSettings(kindSelect.value === "endnote" ? "endnote" : "footnote"));
  el("noteSettingsApply")?.addEventListener("click", () => void applySettings());
  for (const id of ["noteSettingsCancel", "noteSettingsClose"]) el(id)?.addEventListener("click", () => settingsModal?.close());
  // Word's Convert… button inside the dialog: the same Convert dialog the menu opens.
  el("noteSettingsConvert")?.addEventListener("click", () => {
    settingsModal?.close();
    openConvert();
  });

  // ---- Convert notes -----------------------------------------------------------
  const convertDialog = el("convertNotesDialog");
  const selectionOnly = el("convertNotesSelection");
  const convertModal = convertDialog
    ? registerModal(convertDialog, {
        initialFocus: () => convertDialog.querySelector("input[name=convertNotes]:checked"),
        fallbackFocus: () => ed.pagesEl,
        defaultAction: () => void applyConvert(),
      })
    : null;

  function openConvert() {
    if (!ed.doc || !convertModal) return;
    // A selection narrows the conversion; without one there is nothing to narrow to,
    // so the box is off and disabled WITH the reason rather than greyed in silence.
    const range = ed.hasRange();
    if (selectionOnly) {
      selectionOnly.checked = false;
      selectionOnly.disabled = !range;
      selectionOnly.closest("label")?.setAttribute("title", range ? "" : t("notes.convertNeedsSelection"));
    }
    convertModal.open();
  }

  async function applyConvert() {
    if (!ed.doc) return;
    const conversion = convertDialog?.querySelector("input[name=convertNotes]:checked")?.value ?? "footnotesToEndnotes";
    const narrow = selectionOnly?.checked === true && ed.hasRange();
    const { anchor, focus } = ed.selection ?? {};
    const applied = await ed.runEdit(
      () =>
        narrow
          ? ed.doc.convertNotes(conversion, anchor.node, anchor.offset, focus.node, focus.offset)
          : ed.doc.convertNotes(conversion, null, 0, null, 0),
      { gate: true },
    );
    if (!applied) return;
    convertModal.close();
    ed.setStatus(t("notes.converted"));
  }

  el("convertNotesApply")?.addEventListener("click", () => void applyConvert());
  for (const id of ["convertNotesCancel", "convertNotesClose"]) el(id)?.addEventListener("click", () => convertModal?.close());

  // ---- The menu ------------------------------------------------------------------
  const button = el("refNotesMenuBtn");
  const menu = el("refNotesMenu");
  if (button && menu) {
    const popover = registerPopover(button, menu, () => {});
    for (const row of menu.querySelectorAll("[data-command]")) {
      onButton(row, () => {
        popover.close();
        run(row.dataset.command);
      });
    }
  }

  /** Runs one of the six by id — the menu rows, the References menu and the palette. */
  function run(command) {
    const row = NOTE_COMMANDS.find((candidate) => candidate.command === command);
    if (!row) return;
    if (row.kind) go(row.kind, row.forward);
    else if (command === "reference.noteSettings") openSettings();
    else if (command === "reference.convertNotes") openConvert();
  }

  return { run, openSettings, openConvert };
}
