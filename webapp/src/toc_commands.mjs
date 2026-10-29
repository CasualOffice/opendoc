// References ▸ Table of contents: generating one, and updating one.
//
// Both controls shipped DISABLED with a reason for as long as there was no
// engine operation behind them — the rule this repository keeps
// (`docs/63`: never a control that silently does nothing). The operations exist
// now (`insertTableOfContents`, `updateTableOfContents`), so the reasons come
// off and this module is what the buttons run.
//
// ## The competitive standard, and which one this follows
//
// * **Word** — References ▸ Table of Contents inserts one, and **Update Table**
//   opens a two-radio dialog: *Update page numbers only* / *Update entire
//   table*. The insert side offers **Show levels**, 1..9, defaulting to 3.
// * **ONLYOFFICE** — the same split, as two rows of one dropdown rather than a
//   modal, and the same 1..9 level choice in its settings dialog.
// * **Google Docs** — inserts with no options at all and updates with a refresh
//   button on the table itself; no level choice anywhere.
//
// **Followed: Word.** The two products that offer a level choice agree on the
// range and the default, and the update split is the first thing a user reaches
// for after editing a document — which is exactly why "update" cannot be one
// button that guesses. Word's radio pair is kept as a radio pair, and each mode
// ALSO gets its own command so the choice is reachable without the modal.
//
// ## Which table of contents is being updated
//
// `updateTableOfContents("")` means "the document's only contents field", and
// the engine refuses rather than guessing when there are none or several. So the
// empty id covers the ordinary document, and only a document with more than one
// pays for `fieldRangeSpans()` — which is **O(document)** and documented as not
// a keystroke accessor. It is called here on the update gesture itself, never
// from enablement: the ribbon's enablement reads `fieldRangeEntries()`, which is
// O(field ranges) and walks nothing, cached the way the stale-caption count is.
//
// ## Settings, removal, Add text and the table of figures (`docs/153` RM-25)
//
// The insert dialog is the settings dialog both rivals ship — page numbers,
// right alignment, the tab leader, links, outline levels or chosen styles, and a
// format gallery — and in its figures mode, Word's Insert Table of Figures. The
// engine stores every choice where Word does (the instruction's switches and the
// entries' tab stop), so an update keeps them and the dialog re-opens on them.
// Remove takes the field and its entries as one undo step. Add text lists a
// paragraph at a level WITHOUT restyling it, which is Word's outline-level
// behaviour and deliberately not ONLYOFFICE's (theirs applies `Heading n`,
// `sdkjs/word/api.js:12183-12187`; the engine's doc comment says why).
//
// The style-based path in `toc_navigation.mjs` stays exactly as it is and stays
// necessary: a hand-built table of contents has no field at all, so nothing here
// reports it, and following one of its entries is not a field question.
import { t } from "./i18n.mjs";
// The popover registry, imported rather than injected, as `break_commands.mjs`
// does: one ESM instance, so Add text's menu gets the shared dismissal,
// Escape, `aria-expanded` and focus-return handling.
import { onButton, registerPopover } from "./popover_manager.mjs";

/** Word's default for "Show levels", and the engine's own default for `0`. */
export const DEFAULT_TOC_LEVELS = 3;

/** The deepest heading level a table of contents can list (`w:outlineLvl` is
 *  0..8, so nine levels). */
export const MAX_TOC_LEVELS = 9;

/** The two update modes the facade takes, in the order Word's dialog lists them. */
export const UPDATE_MODES = Object.freeze(["pageNumbers", "entire"]);

/** The tab leaders the settings dialog offers — ONLYOFFICE's four
 *  (`TableOfContentsSettings.js:197-216`: none, dots, dashes, underline) — as
 *  the engine's `TocSettings.leader` tokens. */
export const TOC_LEADERS = Object.freeze(["none", "dot", "hyphen", "underscore"]);

/** The format gallery, in ONLYOFFICE's order (`TableOfContentsSettings.js:936-943`):
 *  `current` keeps the document's own `TOC n` styles; the rest rewrite them. */
export const TOC_FORMATS = Object.freeze(["current", "simple", "standard", "modern", "classic", "online"]);

/** Add text's rows: "Do not show" (`-1`), then levels 1..9. */
export const ADD_TEXT_LEVELS = Object.freeze([-1, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

/** The command id of one Add text row: `reference.addText.none` or
 *  `reference.addText.<level>`. O(1). */
export function addTextCommand(level) {
  return `reference.addText.${level < 0 ? "none" : level}`;
}

/** The menu label of one Add text row, from the catalogue. O(1). */
export function addTextLabel(level) {
  return level < 0 ? t("toc.addText.none") : t("toc.addText.level", { level });
}

/**
 * The engine's `insertTableOfContentsWith` settings for what the dialog's
 * controls say, or `null` when building from styles and no style was given a
 * level (an empty list is a table with nothing to list, which the dialog
 * refuses before the engine has to).
 *
 * Pure, so `toc_commands.test.mjs` can hold every branch without a browser.
 * O(styles listed).
 *
 * @param {object} form
 * @param {"contents"|"figures"} form.mode
 * @param {boolean} form.pageNumbers
 * @param {boolean} form.rightAlign
 * @param {string} form.leader one of `TOC_LEADERS`
 * @param {boolean} form.hyperlinks
 * @param {"outline"|"styles"} form.source
 * @param {number} form.levels
 * @param {Array<[string, string]>} form.styles `[name, level box value]`
 * @param {string} form.format one of `TOC_FORMATS`
 * @param {string} form.caption the caption label (figures)
 * @param {boolean} form.includeLabel
 */
export function tocSettingsFromForm(form) {
  const settings = {
    pageNumbers: !!form.pageNumbers,
    rightAlign: !!form.rightAlign,
    leader: TOC_LEADERS.includes(form.leader) ? form.leader : "dot",
    hyperlinks: !!form.hyperlinks,
  };
  if (form.mode === "figures") {
    settings.caption = String(form.caption ?? "").trim() || "Figure";
    settings.includeLabel = !!form.includeLabel;
    return settings;
  }
  settings.format = TOC_FORMATS.includes(form.format) ? form.format : "current";
  if (form.source === "styles") {
    const styles = [];
    for (const [name, raw] of form.styles ?? []) {
      if (String(raw ?? "").trim() === "") continue;
      const level = Number(raw);
      if (!Number.isInteger(level) || level < 1 || level > MAX_TOC_LEVELS) continue;
      styles.push([name, level]);
    }
    if (styles.length === 0) return null;
    settings.styles = styles;
    return settings;
  }
  const raw = Number(form.levels);
  settings.levels = Number.isFinite(raw)
    ? Math.min(MAX_TOC_LEVELS, Math.max(1, Math.round(raw)))
    : DEFAULT_TOC_LEVELS;
  return settings;
}

/**
 * The References rows this module owns, shaped for `REFERENCE_SURFACE`: Remove
 * table of contents, Table of figures, and the ten Add text rows.
 *
 * `label` is a GETTER, for the reason `break_commands.mjs` spells out: the
 * surface table is built before a catalogue is installed, so a value would
 * freeze the KEY into the palette. `commands` is a thunk for the same kind of
 * reason — the surfaces are declared before the dialogs are built.
 *
 * The ten Add text rows share one ribbon face, whose click opens the popover
 * (`ownsClick`); each row is its own palette entry, so a level is reachable
 * without the pointer.
 *
 * Complexity: O(1) per row; nothing here reads the document.
 *
 * @param {() => object} commands the `createTocCommands` result, read lazily
 */
export function tocSurfaceRows(commands) {
  const button = (id) => () => [document.getElementById(id)];
  return [
    {
      command: "reference.removeToc",
      get label() {
        return t("toc.remove.command");
      },
      kw: "remove delete table of contents toc field entries",
      buttons: button("refTocRemoveBtn"),
      requires: "tocField",
      run: () => void commands().remove.run(),
    },
    {
      command: "reference.tableOfFigures",
      get label() {
        return t("toc.figures.command");
      },
      kw: "table of figures captions list figure table equation index",
      buttons: button("refTableOfFiguresBtn"),
      requires: "bodyCaret",
      run: () => commands().insert.open("figures"),
    },
    ...ADD_TEXT_LEVELS.map((level) => ({
      command: addTextCommand(level),
      get label() {
        return level < 0 ? t("toc.addText.paletteNone") : t("toc.addText.paletteLevel", { level });
      },
      kw: `add text table of contents toc level outline list paragraph ${level < 0 ? "do not show hide" : level}`,
      buttons: button("refAddTextBtn"),
      requires: "bodyCaret",
      ownsClick: true,
      run: () => void commands().addText.run(level),
    })),
  ];
}

/**
 * Whether a field instruction is a table of contents.
 *
 * `TOC` is the field kind; `TOC \o "1-3" \z \u` is what we generate and
 * ` TOC \h ` is what Word writes. Matched on the first word so a `TOA` (table of
 * authorities) or a `RD` in the same document is not mistaken for one.
 *
 * O(length of the instruction).
 */
export function isTocInstruction(instruction) {
  return /^\s*TOC\b/i.test(String(instruction ?? ""));
}

/**
 * `fieldRangeSpans()`'s lines as `{ id, instruction, nodes }`.
 *
 * The wire shape is `id\tinstruction\tnode node …`; a range whose markers are
 * missing reports an empty node list rather than being dropped, so an entry with
 * no third column is kept with no nodes rather than skipped.
 *
 * O(total line length).
 */
export function parseFieldRangeSpans(lines) {
  return [...(lines ?? [])].map((line) => {
    const [id = "", instruction = "", nodes = ""] = String(line).split("\t");
    return { id, instruction, nodes: nodes ? nodes.split(" ").filter(Boolean) : [] };
  });
}

/**
 * The id of the contents field covering `node`, or `""`.
 *
 * This is the question `toc_navigation.mjs` could not ask before the spans
 * accessor existed — "is the caret inside the contents field" — and it is what
 * decides which table an update is aimed at when a document holds several.
 *
 * O(spans × nodes per span).
 */
export function tocFieldAt(spans, node) {
  if (!node) return "";
  for (const span of spans) {
    if (!isTocInstruction(span.instruction)) continue;
    if (span.nodes.includes(node)) return span.id;
  }
  return "";
}

/**
 * Builds the two dialogs and the contents-field cache.
 *
 * `io` is the whole contact with the application:
 *
 *   `getDoc()`           the open document, or null
 *   `caretNode()`        the caret's paragraph id, or null
 *   `selection()`        `{anchor, focus}` positions, for Add text
 *   `applyEdit(fn)`      Promise<boolean>; one gated, undoable engine call
 *   `mutationBlocked()`  true (having said why) when the review mode refuses
 *   `status(text, kind)` the status line
 *   `registerModal(dialog, options)`
 *   `fallbackFocus()`
 */
export function createTocCommands(io) {
  const el = (id) => document.getElementById(id);
  /** How many contents FIELDS the document holds. Read by the ribbon on every
   *  keystroke, so it is a cached number and never a walk. */
  let fieldCount = 0;

  const fields = {
    get count() {
      return fieldCount;
    },

    /** One `fieldRangeEntries()` read — O(field ranges), no document walk. Called
     *  from the same deliberate interactions the stale-caption count is: opening
     *  a document, inserting or updating a table, and reaching the References
     *  tab.
     *
     *  `io.changed()` is called after, because the count is what the ribbon's
     *  enablement reads and the ribbon syncs INSIDE the edit that changed it —
     *  so without this the Update button stays disabled until something else
     *  happens to sync it, which is exactly the shape of a control that does
     *  nothing for no visible reason. */
    refresh() {
      const doc = io.getDoc();
      const previous = fieldCount;
      fieldCount = 0;
      try {
        fieldCount = (doc?.fieldRangeEntries?.() ?? []).filter((line) =>
          isTocInstruction(String(line).split("\t")[1] ?? ""),
        ).length;
      } catch {
        // A build whose engine predates the binding: "no contents field" is the
        // honest state, and the button then says so rather than throwing.
        fieldCount = 0;
      }
      if (fieldCount !== previous) io.changed?.();
    },
  };

  // ---- Insert: the settings dialog ----------------------------------------
  //
  // ONE dialog in two modes, as ONLYOFFICE's is (`TableOfContentsSettings.js`,
  // `type == 1` for figures): Table of contents builds from outline levels or
  // from chosen styles, Table of figures from the captions of one label; the
  // page-number, leader and link controls are shared. Opened with the caret in
  // an existing table of the same kind, it opens ON that table's settings and
  // its Insert replaces it — Word's "Replace this table of contents?" without
  // the extra question, because the caret placement already asked it.
  const insertDialog = el("tocDialog");
  const levelsInput = el("tocLevels");
  const captionSelect = el("tocCaptionLabel");
  const stylesList = el("tocStylesList");
  /** "contents" or "figures": which mode the dialog is in. */
  let mode = "contents";
  const insertModal = insertDialog
    ? io.registerModal(insertDialog, {
        initialFocus: () => (mode === "figures" ? captionSelect : levelsInput),
        fallbackFocus: io.fallbackFocus,
      })
    : null;

  /** The level the dialog is asking for, clamped to what the engine accepts.
   *  A blank or unreadable box is Word's default rather than a refusal: the
   *  question is "how deep", and 3 is the answer every product ships with. */
  function levels() {
    const raw = Number(levelsInput?.value);
    if (!Number.isFinite(raw)) return DEFAULT_TOC_LEVELS;
    return Math.min(MAX_TOC_LEVELS, Math.max(1, Math.round(raw)));
  }

  const checked = (id) => !!el(id)?.checked;
  const setChecked = (id, value) => {
    const box = el(id);
    if (box) box.checked = !!value;
  };
  const sourceValue = () =>
    insertDialog?.querySelector('input[name="tocSource"]:checked')?.value === "styles" ? "styles" : "outline";

  /** Shows the controls of `next` and hides the other mode's. */
  function showMode(next) {
    mode = next === "figures" ? "figures" : "contents";
    for (const part of insertDialog?.querySelectorAll("[data-toc-mode]") ?? []) {
      part.hidden = part.dataset.tocMode !== mode;
    }
  }

  /** Shows the style list only when building from styles, and the levels box
   *  only when building from outline levels — the two are alternatives. */
  function reflectSource() {
    const styles = sourceValue() === "styles";
    if (stylesList) stylesList.hidden = !styles;
    const levelsField = levelsInput?.closest("label");
    if (levelsField) levelsField.hidden = styles;
  }

  /** Page numbers off disables right alignment, and right alignment off
   *  disables the leader — ONLYOFFICE's own dependency (`:163-166`, `:186`). */
  function reflectNumbers() {
    const numbers = checked("tocPageNumbers");
    const right = el("tocRightAlign");
    if (right) right.disabled = !numbers;
    const leader = el("tocLeader");
    if (leader) leader.disabled = !numbers || !checked("tocRightAlign");
  }

  /** Rebuilds the style list: one row per paragraph style, its level box
   *  prefilled from `chosen` (`[[name, level], …]`), and Heading 1..3 at their
   *  own levels when nothing was chosen yet. O(styles). */
  function renderStyles(doc, chosen) {
    if (!stylesList) return;
    const levelsByName = new Map(chosen ?? []);
    if (!chosen) {
      for (let level = 1; level <= DEFAULT_TOC_LEVELS; level += 1) levelsByName.set(`Heading ${level}`, level);
    }
    stylesList.replaceChildren();
    let names = [];
    try {
      names = [...(doc.listStyles?.() ?? [])];
    } catch {
      names = [];
    }
    for (const name of [...new Set([...names, ...levelsByName.keys()])]) {
      const row = document.createElement("label");
      row.className = "dialog-field";
      const text = document.createElement("span");
      text.textContent = name;
      const input = document.createElement("input");
      input.type = "number";
      input.min = "1";
      input.max = String(MAX_TOC_LEVELS);
      input.step = "1";
      input.inputMode = "numeric";
      input.dataset.tocStyle = name;
      input.setAttribute("aria-label", t("toc.styleLevel", { style: name }));
      input.value = levelsByName.has(name) ? String(levelsByName.get(name)) : "";
      row.append(text, input);
      stylesList.append(row);
    }
  }

  /** The caption labels the document uses plus Word's built-ins. O(document),
   *  on the opening gesture only. */
  function renderCaptionLabels(doc, chosen) {
    if (!captionSelect) return;
    let labels = [];
    try {
      labels = [...(doc.captionLabels?.() ?? [])];
    } catch {
      labels = [];
    }
    if (chosen && !labels.includes(chosen)) labels.push(chosen);
    if (labels.length === 0) labels = ["Figure"];
    captionSelect.replaceChildren(
      ...labels.map((label) => {
        const option = document.createElement("option");
        option.value = label;
        option.textContent = label;
        return option;
      }),
    );
    captionSelect.value = chosen && labels.includes(chosen) ? chosen : labels.includes("Figure") ? "Figure" : labels[0];
  }

  /** Puts `settings` (the engine's `tableOfContentsSettings` shape) into the
   *  form. Absent fields take the defaults the engine itself uses. */
  function fill(doc, settings) {
    const values = settings ?? {};
    setChecked("tocPageNumbers", values.pageNumbers ?? true);
    setChecked("tocRightAlign", values.rightAlign ?? true);
    setChecked("tocHyperlinks", values.hyperlinks ?? false);
    const leader = el("tocLeader");
    if (leader) leader.value = TOC_LEADERS.includes(values.leader) ? values.leader : "dot";
    const format = el("tocFormat");
    // The format is not stored on a table (it lives in the TOC n styles), so a
    // table re-opened here is "from document" — keep what it looks like.
    if (format) format.value = "current";
    if (levelsInput) levelsInput.value = String(values.levels ?? DEFAULT_TOC_LEVELS);
    const source = values.styles ? "styles" : "outline";
    for (const radio of insertDialog?.querySelectorAll('input[name="tocSource"]') ?? []) {
      radio.checked = radio.value === source;
    }
    renderStyles(doc, values.styles ?? null);
    renderCaptionLabels(doc, values.caption ?? "");
    setChecked("tocIncludeLabel", values.includeLabel ?? true);
    reflectSource();
    reflectNumbers();
  }

  /** The id of the table the caret is in, and its settings, or `null`. The span
   *  read is O(document) and runs on the opening gesture only. */
  function tableAtCaret(doc) {
    if (fields.count === 0) return null;
    let id = "";
    try {
      id = tocFieldAt(parseFieldRangeSpans(doc.fieldRangeSpans()), io.caretNode());
    } catch {
      return null;
    }
    if (!id) return null;
    try {
      return { id, settings: JSON.parse(doc.tableOfContentsSettings(id)) };
    } catch {
      return null;
    }
  }

  const insert = {
    /** Opens the dialog in `which` mode ("contents" or "figures"). */
    open(which = "contents") {
      const doc = io.getDoc();
      if (!insertModal || !doc || io.mutationBlocked()) return;
      const existing = tableAtCaret(doc);
      const existingMode = existing?.settings?.caption ? "figures" : "contents";
      if (existing && existingMode !== which) {
        // The engine would REPLACE the table the caret is in; replacing a table of
        // contents with a table of figures (or back) is never what was meant.
        io.status(t("toc.caretInOtherTable"), "warn");
        return;
      }
      showMode(which);
      fill(doc, existing?.settings ?? null);
      insertModal.open();
    },

    close() {
      insertModal?.close();
    },

    /** The settings the form describes, in the engine's shape. */
    settings() {
      return tocSettingsFromForm({
        mode,
        pageNumbers: checked("tocPageNumbers"),
        rightAlign: checked("tocRightAlign"),
        leader: el("tocLeader")?.value,
        hyperlinks: checked("tocHyperlinks"),
        source: sourceValue(),
        levels: levels(),
        styles: [...(stylesList?.querySelectorAll("input[data-toc-style]") ?? [])].map((input) => [
          input.dataset.tocStyle,
          input.value,
        ]),
        format: el("tocFormat")?.value,
        caption: captionSelect?.value,
        includeLabel: checked("tocIncludeLabel"),
      });
    },

    /** Inserts (or, with the caret in a table, replaces) at the caret, then
     *  re-reads the cache the insert invalidates.
     *
     *  The engine's refusals here are sentences a reader can act on — "the
     *  document has no headings at level 3 or above to list" is the whole
     *  explanation — so they are shown verbatim instead of being flattened into
     *  the generic "that edit isn't supported" `runEdit` writes for internal
     *  vocabulary (`edit_errors.mjs`). */
    async run() {
      const doc = io.getDoc();
      const node = io.caretNode();
      if (!doc || !node || io.mutationBlocked()) return;
      const settings = insert.settings();
      if (!settings) {
        io.status(t("toc.noStylesChosen"), "warn");
        return;
      }
      const figures = mode === "figures";
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.insertTableOfContentsWith(node, JSON.stringify(settings));
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      fields.refresh();
      if (refusal) io.status(refusal, "warn");
      else if (applied) io.status(t(figures ? "toc.figures.inserted" : "toc.inserted"));
      if (applied) insert.close();
    },
  };

  for (const radio of insertDialog?.querySelectorAll('input[name="tocSource"]') ?? []) {
    radio.addEventListener("change", reflectSource);
  }
  el("tocPageNumbers")?.addEventListener("change", reflectNumbers);
  el("tocRightAlign")?.addEventListener("change", reflectNumbers);

  /** The id of the table an update or a removal is aimed at: `""` for the only
   *  one, the caret's when there are several, `null` (having said why) when the
   *  caret is in none of them. O(document) only when there are several. */
  function targetTable(doc) {
    if (fields.count <= 1) return "";
    const id = tocFieldAt(parseFieldRangeSpans(doc.fieldRangeSpans()), io.caretNode());
    if (!id) {
      io.status(t("toc.whichTable"), "warn");
      return null;
    }
    return id;
  }

  // ---- Remove -------------------------------------------------------------
  const remove = {
    /** Removes the table as a unit — its field and every entry, one undo step
     *  (ONLYOFFICE's `textContentsRemove`, Word's Remove Table of Contents). */
    async run() {
      const doc = io.getDoc();
      if (!doc || io.mutationBlocked()) return;
      const id = targetTable(doc);
      if (id === null) return;
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.removeTableOfContents(id);
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      fields.refresh();
      if (refusal) io.status(refusal, "warn");
      else if (applied) io.status(t("toc.removed"));
    },
  };

  // ---- Add text -----------------------------------------------------------
  const addTextButton = el("refAddTextBtn");
  const addTextMenu = el("refAddTextMenu");

  const addText = {
    /** Lists the selected paragraphs at `level` (1..9), or keeps them out of the
     *  table (`-1`), without restyling them. */
    async run(level) {
      const doc = io.getDoc();
      const selection = io.selection?.();
      if (!doc || !selection || io.mutationBlocked()) return;
      const { anchor, focus } = selection;
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.addParagraphsToContents(anchor.node, anchor.offset, focus.node, focus.offset, level);
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      if (refusal) io.status(refusal, "warn");
      else if (applied) {
        io.status(level < 0 ? t("toc.addText.removed") : t("toc.addText.done", { level }));
      }
    },

    /** The contents level the caret's paragraph is listed at, or -1. */
    current() {
      const doc = io.getDoc();
      const node = io.caretNode();
      if (!doc || !node) return -1;
      try {
        return Number(doc.contentsLevelAt(node));
      } catch {
        return -1;
      }
    },
  };

  if (addTextButton && addTextMenu) {
    // Rows are built when the menu opens, so the current level is the checked
    // one and every label is read from the catalogue that is installed now.
    const popover = registerPopover(addTextButton, addTextMenu, () => {
      const current = addText.current();
      addTextMenu.replaceChildren(
        ...ADD_TEXT_LEVELS.map((level) => {
          const row = document.createElement("button");
          row.type = "button";
          row.className = "menu-item";
          row.setAttribute("role", "menuitemradio");
          row.setAttribute("aria-checked", String(level === current));
          row.dataset.command = addTextCommand(level);
          row.textContent = addTextLabel(level);
          onButton(row, () => {
            popover.close();
            void addText.run(level);
          });
          return row;
        }),
      );
    });
  }

  // ---- Update -------------------------------------------------------------
  const updateDialog = el("tocUpdateDialog");
  const updateModal = updateDialog
    ? io.registerModal(updateDialog, {
        initialFocus: () => updateDialog.querySelector('input[name="tocUpdateMode"]:checked'),
        fallbackFocus: io.fallbackFocus,
      })
    : null;

  const update = {
    open() {
      if (!updateModal || !io.getDoc() || io.mutationBlocked()) return;
      updateModal.open();
    },

    close() {
      updateModal?.close();
    },

    /** The mode the dialog's radios are on. */
    chosenMode() {
      const picked = updateDialog?.querySelector('input[name="tocUpdateMode"]:checked');
      return UPDATE_MODES.includes(picked?.value) ? picked.value : UPDATE_MODES[0];
    },

    /**
     * Updates one table of contents.
     *
     * With exactly one in the document the empty id says so and no walk happens
     * at all. With several, the caret decides — which is Word's rule, and the
     * question `fieldRangeSpans()` exists to answer. That call is O(document)
     * and runs HERE, on a gesture, never on the enablement path.
     */
    async run(mode) {
      const doc = io.getDoc();
      if (!doc || io.mutationBlocked()) return;
      const id = targetTable(doc);
      if (id === null) return;
      let refusal = "";
      const applied = await io.applyEdit(() => {
        try {
          return doc.updateTableOfContents(id, mode);
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      });
      fields.refresh();
      if (refusal) io.status(refusal, "warn");
      else if (applied) {
        io.status(t(mode === "entire" ? "toc.rebuilt" : "toc.pageNumbersUpdated"));
      }
      if (applied) update.close();
    },
  };

  insertDialog?.querySelector("#tocForm")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void insert.run();
  });
  el("tocCancel")?.addEventListener("click", () => insert.close());
  el("tocClose")?.addEventListener("click", () => insert.close());
  updateDialog?.querySelector("#tocUpdateForm")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void update.run(update.chosenMode());
  });
  el("tocUpdateCancel")?.addEventListener("click", () => update.close());
  el("tocUpdateClose")?.addEventListener("click", () => update.close());

  return { fields, insert, update, remove, addText };
}
