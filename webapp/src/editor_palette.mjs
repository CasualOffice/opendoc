// The command palette (Cmd/Ctrl+Shift+P) and `editorCommands()`, the one
// registry of real editor actions the palette, the menus and the ribbon read.
// Moved out of `main.js` unchanged (docs/177); it reaches the rest of the
// editor only through the context `ed`.
import { narrowDecisionsToContainer } from "./review_authority.mjs";
import { exportCommands } from "./export_commands.mjs";
import { mutationBlockedMessage } from "./edit_errors.mjs";
import { buildObjectContextCommands } from "./object_context_menu.mjs";
import { FONT_SIZE_STEPS } from "./style_picker.mjs";
import { styleDisplayName } from "./style_names.mjs";
import { printDocument } from "./print.mjs";
import { shortcutForCommand } from "./keymap.mjs";
import { t } from "./i18n.mjs";
import { openPopover } from "./popover_manager.mjs";
import { hostCapabilities } from "./capabilities.mjs";
import { collabCommands } from "./collab_chrome.mjs";
import { flattenCommandTree, tableCommandLabel, tableMenuPlaceholders } from "./command_taxonomy.mjs";
import { FIELD_KINDS } from "./field_kinds.mjs";
import { insertChartTypeCommands } from "./chart_insert.mjs";
import { tableTools } from "./table_tools.mjs";
import { tableDrawMode } from "./table_draw_mode.mjs";
import { revealOnSettings } from "./surface_reveal.mjs";
import { quickStyleCommands } from "./quick_styles.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createPalette(ed) {
  // ---- Command palette (⌘⇧P) — fuzzy search over real editor actions ----------
  const cmdPalette = document.getElementById("cmdPalette");
  const cmdInput = document.getElementById("cmdInput");
  const cmdList = document.getElementById("cmdList");
  const searchTrigger = document.getElementById("searchTrigger");


  /** Shared command descriptors for search and contextual surfaces. Dynamic
   * entries are rebuilt so document styles and availability never go stale. */
  /** Whether the caret sits in a numbered list — the precondition the Restart and
   *  Continue numbering controls share, on every surface that offers them. */
  function numberedListAtCaret() {
    return !!ed.doc && !!ed.selection && ed.doc.listStyleAt(ed.selection.focus.node) === "numbered";
  }

  function editorCommands(context = { surface: "palette" }) {
    const fmt = (k) => () => ed.toggleFormat(k);
    const align = (a) => () => ed.runToolbarEdit((s, o, e, f) => ed.doc.setAlignment(s, o, e, f, a), { paragraphLevel: true });
    const cmds = [
      // `noDoc`, and first: with no document open this is the only File command
      // that can run, and it is the one a user arriving with nothing to open needs.
      // No keyboard shortcut is claimed — ⌘N/Ctrl+N belongs to the browser window
      // and cannot be intercepted, and a shortcut hint the editor cannot honour
      // would be a lie printed in the palette.
      { id: "file.new", label: "New blank document", group: "File", kw: "new blank empty create start untitled document", noDoc: true, enabled: hostCapabilities().has("new"), disabledReason: t("capability.embedded"), run: () => void ed.newBlankDocument() },
      { id: "file.open", label: "Open…", group: "File", kw: "load docx odt json txt", noDoc: true, enabled: hostCapabilities().has("open"), disabledReason: t("capability.embedded"), run: () => ed.fileEl.click() },
      { id: "file.save", label: "Save", group: "File", kw: "export download", enabled: ed.HOST_CAPS.has("save"), disabledReason: t("capability.notGranted"), run: () => ed.saveDocument() },
      ...exportCommands(ed.exportDocumentAs, ed.HOST_CAPS.has("download"), t("capability.notGranted"), ed.doc ? ed.doc.availableExportFormats() : null),
      // Reachable with no document open, because the case it exists for is
      // arriving at a fresh tab after a crash (HF-011). Disabled WITH A REASON
      // when the store is empty — never a control that silently does nothing.
      {
        id: "file.recoverDrafts",
        label: "Recover unsaved work…",
        group: "File",
        kw: "draft autosave recover crash restore unsaved backup",
        noDoc: true,
        enabled: ed.draftOffers.length > 0,
        disabledReason: ed.AUTOSAVE_ALLOWED_HERE
          ? "No unsaved work to recover"
          : "Autosave is off in an embedded editor",
        run: () => ed.showDraftRecovery(),
      },
      { id: "file.print", label: "Print", group: "File", kw: "print pages paper hard copy pdf", enabled: ed.HOST_CAPS.has("print"), disabledReason: t("capability.notGranted"), run: () => void printDocument(ed.doc).finally(() => { void ed.renderAll(); ed.scheduleChromeRefresh({ stats: true, outline: true }); }) },
      { id: "file.properties", label: "Document properties", group: "File", kw: "metadata title author", run: () => ed.propertiesUi.toggle(true) },
      // Version history. `docs/139` §8.1's first entry point, and the primary one:
      // File is where Docs, ONLYOFFICE and Word all keep it, and a File row costs
      // no ribbon width at all. The View band carries the second durable surface.
      //
      // NEVER a dead control: with no document, with the preference off, with
      // autosave off, in a framed editor or with a store the browser refused, this
      // row is present and DISABLED WITH THE REASON — which is a different sentence
      // in each of those five cases, because the way out of each one is different.
      //
      // A WITHHELD REGION is the one case that is silent instead, and that is the
      // distinction `docs/126` draws: "never, for you" is composition and says
      // nothing, "not right now" is state and explains itself. Taking the row out
      // of the REGISTRY rather than hiding a button is what makes the composition
      // complete — the palette and the ⌘⌥⇧H chord read the registry, and neither
      // belongs to a region CSS could reach.
      ...(ed.HOST_CHROME.editing.has("history")
        ? [
            {
              id: "file.versionHistory",
              label: t("versionHistory.command"),
              group: "File",
              kw: "version history timeline earlier previous restore revert named checkpoint past revision",
              enabled: !!ed.doc && ed.versionHistory.available(),
              disabledReason: ed.versionHistory.unavailableReason(),
              run: () => void ed.versionHistory.toggle(),
            },
            // The second surface of the panel's "Save a version for everyone"
            // (`docs/175` §11): disabled with the reason outside a room whose host
            // keeps versions, never absent.
            {
              id: "file.saveSharedVersion",
              label: t("versionHistory.shared.save"),
              group: "File",
              kw: "save shared version everyone room checkpoint named history snapshot",
              enabled: !ed.versionHistory.saveSharedRefusal(),
              disabledReason: ed.versionHistory.saveSharedRefusal(),
              run: () => void ed.versionHistory.saveShared(),
            },
          ]
        : []),
      {
        id: "edit.undo",
        label: ed.doc?.undoLabel ? `Undo ${ed.doc.undoLabel}` : "Undo",
        group: "Edit",
        kw: "revert",
        contextMenu: true,
        enabled: !!ed.doc?.canUndo,
        disabledReason: "Nothing to undo",
        run: () => ed.runEdit(() => ed.doc.undo()),
      },
      {
        id: "edit.redo",
        label: ed.doc?.redoLabel ? `Redo ${ed.doc.redoLabel}` : "Redo",
        group: "Edit",
        kw: "",
        contextMenu: true,
        enabled: !!ed.doc?.canRedo,
        disabledReason: "Nothing to redo",
        run: () => ed.runEdit(() => ed.doc.redo()),
      },
      {
        id: "edit.cut",
        label: "Cut",
        group: "Clipboard",
        kw: "",
        contextMenu: true,
        enabled: context.hasRange ?? ed.hasRange(),
        disabledReason: "Select content to cut",
        run: () => ed.cut(),
      },
      {
        id: "edit.copy",
        label: "Copy",
        group: "Clipboard",
        kw: "",
        contextMenu: true,
        enabled: context.hasRange ?? ed.hasRange(),
        disabledReason: "Select content to copy",
        run: () => ed.copySelection(),
      },
      {
        id: "edit.paste",
        label: "Paste",
        group: "Clipboard",
        kw: "",
        contextMenu: true,
        enabled: !!ed.doc && !!ed.selection,
        disabledReason: "Place the caret before pasting",
        run: () => ed.paste(),
      },
      {
        id: "edit.pasteText",
        label: "Paste without formatting",
        group: "Clipboard",
        kw: "plain text unformatted keep text only",
        contextMenu: true,
        enabled: !!ed.doc && !!ed.selection,
        disabledReason: "Place the caret before pasting",
        run: () => ed.pasteAsText(),
      },
      {
        id: "edit.pasteImage",
        label: t("edit.pasteImage"),
        group: "Clipboard",
        kw: t("edit.pasteImage.keywords"),
        contextMenu: true,
        enabled: !!ed.doc && !!ed.selection,
        disabledReason: t("paragraph.caretRequired"),
        run: () => void ed.objectInsert.pasteAsPicture(),
      },
      {
        id: "edit.selectAll",
        label: "Select all",
        group: "Clipboard",
        kw: "selection document",
        contextMenu: true,
        enabled: !!ed.doc,
        run: () => ed.selectAll(),
      },
      { id: "edit.find", label: "Find and replace", group: "Edit", kw: "search replace", run: () => ed.openFind() },
      { id: "edit.replace", label: t("find.replaceCommand"), group: "Edit", kw: "find substitute swap change text", run: () => ed.openFind({ replace: true }) },
      { id: "format.bold", label: "Bold", group: "Format", kw: "strong", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: fmt("bold") },
      { id: "format.italic", label: "Italic", group: "Format", kw: "emphasis", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: fmt("italic") },
      { id: "format.underline", label: "Underline", group: "Format", kw: "", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: fmt("underline") },
      { id: "format.strike", label: "Strikethrough", group: "Format", kw: "strike", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: fmt("strike") },
      { id: "format.superscript", label: "Superscript", group: "Format", kw: "raise exponent", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: () => ed.superBtn.click() },
      { id: "format.subscript", label: "Subscript", group: "Format", kw: "lower", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: () => ed.subBtn.click() },
      { id: "format.clear", label: "Clear direct formatting", group: "Format", kw: "reset defaults", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: () => ed.clearFormattingBtn.click() },
      { id: "format.painter", label: "Format painter", group: "Format", kw: "copy formatting paint brush clone style match", enabled: !!ed.selection, disabledReason: "Place the caret or select text to copy its formatting", run: () => ed.armFormatPainter(false) },
      // HF-147 — the face and the exact size were ribbon chrome with no command
      // id, so nothing but a mouse on that one control could set either. These two
      // open the control's own picker; the exact values are generated further down
      // as `format.family.<name>` and `format.size.<pt>`.
      {
        id: "format.family",
        label: "Font…",
        group: "Format",
        kw: "typeface face family font name",
        enabled: !!ed.selection,
        disabledReason: "Place the caret or select text",
        run: () => ed.fontFamilyBtn.click(),
      },
      {
        id: "format.size",
        label: "Font size…",
        group: "Format",
        kw: "points pt exact size font",
        enabled: !!ed.selection,
        disabledReason: "Place the caret or select text",
        run: () => {
          ed.fontSizeSel.focus();
          ed.fontSizeSel.select();
        },
      },
      { id: "format.grow", label: "Increase font size", group: "Format", kw: "grow bigger larger font", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: () => ed.stepFontSize(1) },
      { id: "format.shrink", label: "Decrease font size", group: "Format", kw: "shrink smaller font", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: () => ed.stepFontSize(-1) },
      { id: "format.color", label: "Text color…", group: "Format", kw: "font foreground colour", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: (anchor) => openPopover(ed.textColorPopover, { anchor }) },
      { id: "format.highlight", label: "Highlight color…", group: "Format", kw: "marker colour", enabled: !!ed.selection, disabledReason: "Place the caret or select text", run: (anchor) => openPopover(ed.highlightPopover, { anchor }) },
      { id: "format.case.upper", label: "Change case: UPPERCASE", group: "Format", kw: "capitals uppercase", enabled: (context.hasRange ?? ed.hasRange()), disabledReason: "Select text to change case", run: () => ed.applyChangeCase("upper") },
      { id: "format.case.lower", label: "Change case: lowercase", group: "Format", kw: "lowercase", enabled: (context.hasRange ?? ed.hasRange()), disabledReason: "Select text to change case", run: () => ed.applyChangeCase("lower") },
      { id: "format.case.title", label: "Change case: Capitalize Each Word", group: "Format", kw: "title case capitalize", enabled: (context.hasRange ?? ed.hasRange()), disabledReason: "Select text to change case", run: () => ed.applyChangeCase("title") },
      { id: "format.case.sentence", label: "Change case: Sentence case", group: "Format", kw: "sentence capitalize", enabled: (context.hasRange ?? ed.hasRange()), disabledReason: "Select text to change case", run: () => ed.applyChangeCase("sentence") },
      { id: "format.case.toggle", label: "Change case: tOGGLE cASE", group: "Format", kw: "toggle invert case", enabled: (context.hasRange ?? ed.hasRange()), disabledReason: "Select text to change case", run: () => ed.applyChangeCase("toggle") },
      { id: "paragraph.align.start", label: "Align left", group: "Paragraph", kw: "", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: align("start") },
      { id: "paragraph.align.center", label: "Align center", group: "Paragraph", kw: "centre", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: align("center") },
      { id: "paragraph.align.end", label: "Align right", group: "Paragraph", kw: "", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: align("end") },
      { id: "paragraph.align.justify", label: "Justify", group: "Paragraph", kw: "align", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: align("justify") },
      { id: "paragraph.list.bullet", label: "Bullet list", group: "Paragraph", kw: "unordered", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.runToolbarEdit((s, o, e, f) => ed.doc.toggleList(s, o, e, f, "bullet"), { paragraphLevel: true }) },
      { id: "paragraph.list.numbered", label: "Numbered list", group: "Paragraph", kw: "ordered", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.runToolbarEdit((s, o, e, f) => ed.doc.toggleList(s, o, e, f, "numbered"), { paragraphLevel: true }) },
      { id: "paragraph.list.checklist", label: "Checklist", group: "Paragraph", kw: "checklist checkbox todo task tick check box", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.toggleChecklistCommand() },
      // Restart/continue take the SAME predicate the ribbon buttons take
      // (`updateToolbar`), so a row can never be live in the menu while the button
      // for it is greyed — they are now on both surfaces (docs/104 HF-076).
      { id: "paragraph.list.restart", label: "Restart numbering", group: "Paragraph", kw: "list restart 1", enabled: numberedListAtCaret(), disabledReason: t("list.reason.notNumbered"), run: () => ed.selection && ed.runNodeEdit(() => ed.doc.restartList(ed.selection.focus.node)) },
      { id: "paragraph.list.continue", label: "Continue numbering", group: "Paragraph", kw: "list continue resume", enabled: numberedListAtCaret() && ed.doc.canContinueList(ed.selection.focus.node), disabledReason: t(numberedListAtCaret() ? "list.reason.nothingToContinue" : "list.reason.notNumbered"), run: () => ed.selection && ed.runNodeEdit(() => ed.doc.continueList(ed.selection.focus.node)) },
      { id: "paragraph.indent.increase", label: "Increase indent", group: "Paragraph", kw: "", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.adjustIndentCommand(360) },
      { id: "paragraph.indent.decrease", label: "Decrease indent", group: "Paragraph", kw: "outdent", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.adjustIndentCommand(-360) },
      // Insert commands take their `enabled` from `insertCommandEnabled`, the same
      // predicate the Insert ribbon buttons use, so a command can never be live on
      // one surface and greyed on another. None of them asks for a prior click:
      // an open document already has an insertion point (Word/Docs), so the only
      // Insert precondition left is a real one — Link needs text to link.
      // HF-148 — the id used to BE the 3x3 default, label and all, so no caller
      // could ask for any other size. The grid picker remains what a user without
      // a size in mind gets; the argument is what a host or a keyboard caller
      // needs.
      {
        id: "insert.table",
        label: "Insert table…",
        group: "Insert",
        kw: "grid rows columns",
        enabled: ed.insertCommandEnabled("insert.table"),
        run: () => ed.insertTableBtn.click(),
      },
      // Word's Insert Table… and Convert Text to Table… (`table_tools.mjs`): a
      // typed size beyond the 8 x 10 grid, and the selected paragraphs as a table.
      {
        id: "insert.tableDialog",
        label: t("table.tools.insertDialogCommand"),
        group: "Insert",
        kw: "table rows columns size dialog custom number",
        enabled: ed.insertCommandEnabled("insert.tableDialog"),
        run: () => tableTools.openDialog("insertTable"),
      },
      // Draw Table: toggles the pencil (`table_draw_mode.mjs`).
      {
        id: "insert.drawTable",
        label: t("tableDraw.command"),
        group: "Insert",
        kw: "draw table pencil pen lines split cells outline",
        enabled: ed.insertCommandEnabled("insert.drawTable"),
        run: () => tableDrawMode.toggle(),
      },
      {
        id: "insert.textToTable",
        label: t("table.tools.textToTableCommand"),
        group: "Insert",
        kw: "convert text table tabs commas separator",
        enabled: ed.insertCommandEnabled("insert.textToTable"),
        run: () => tableTools.openDialog("textToTable"),
      },
      // Shift+Enter was reachable ONLY from the keyboard — no id, so no palette
      // row, no menu row, and nothing in the shortcut reference. It is placed in
      // the palette here; a ribbon/menu home (Word files it under Insert ▸ Break)
      // is deliberately deferred, because the Insert ribbon and the Insert menu
      // are held at exact parity by `insert-surface.spec.mjs` and adding a control
      // to both is a chrome change, not this fix.
      { id: "insert.lineBreak", label: "Line break", group: "Insert", kw: "soft line break newline same paragraph shift enter", enabled: !!ed.selection && ed.reviewMode !== "suggesting", disabledReason: ed.reviewMode === "suggesting" ? "Line breaks cannot be tracked yet" : "Place the caret where the break belongs", run: () => void ed.insertLineBreakAtSelection() },
      { id: "insert.link", label: "Add or edit link", group: "Insert", kw: "hyperlink url bookmark toc", enabled: ed.insertCommandEnabled("insert.link", context), disabledReason: t("insert.reason.linkNeedsText"), run: () => ed.editSelectionLink() },
      { id: "layout.firstPageVariant", label: t("headerFooter.firstPageSwitch", { state: t(ed.headerFooterSettings.variantState().firstPage ? "headerFooter.stateOn" : "headerFooter.stateOff") }), group: "Layout", kw: "different first page header footer title page cover", enabled: !!ed.doc, disabledReason: "Open a document first", run: () => ed.headerFooterSettings.toggleVariant("firstPage") },
      { id: "layout.evenOddVariant", label: t("headerFooter.evenOddSwitch", { state: t(ed.headerFooterSettings.variantState().evenOdd ? "headerFooter.stateOn" : "headerFooter.stateOff") }), group: "Layout", kw: "different odd even pages header footer mirrored", enabled: !!ed.doc, disabledReason: "Open a document first", run: () => ed.headerFooterSettings.toggleVariant("evenOdd") },
      { id: "layout.headerFooterSettings", label: t("headerFooter.settingsCommand"), group: "Layout", kw: "header footer position from top bottom distance page numbering number format start at continue link to previous", enabled: !!ed.doc, disabledReason: "Open a document first", run: () => ed.headerFooterSettings.open(true) },
      { id: "insert.footnote", label: "Footnote", group: "Insert", kw: "footnote note reference citation bottom of page", enabled: !!ed.selection, disabledReason: "Place the caret where the note belongs", run: () => ed.insertNote("footnote") },
      { id: "insert.endnote", label: "Endnote", group: "Insert", kw: "endnote note reference citation end of document", enabled: !!ed.selection, disabledReason: "Place the caret where the note belongs", run: () => ed.insertNote("endnote") },
      { id: "insert.header", label: "Edit header", group: "Insert", kw: "header running title page top margin", enabled: !!ed.doc, disabledReason: "Open a document first", run: () => ed.editRunningContent("header") },
      { id: "insert.footer", label: "Edit footer", group: "Insert", kw: "footer running page number bottom margin", enabled: !!ed.doc, disabledReason: "Open a document first", run: () => ed.editRunningContent("footer") },
      { id: "insert.bookmark", label: "Bookmark…", group: "Insert", kw: "bookmark manager navigate create rename delete go to", enabled: ed.insertCommandEnabled("insert.bookmark"), run: () => ed.openBookmarkManager() },
      { id: "insert.field", label: "Field…", group: "Insert", kw: "field placeholder page number of pages date time file name author auto update", enabled: ed.insertCommandEnabled("insert.field"), run: () => ed.openFieldDialog() },
      { id: "insert.dropCap", label: t("dropCap.command"), group: "Insert", kw: "initial letter dropped margin lines paragraph", enabled: ed.insertCommandEnabled("insert.dropCap"), run: () => ed.dropCapDialog.open() },
      { id: "insert.image", label: "Picture…", group: "Insert", kw: "image picture insert photo file png jpeg jpg gif paste", enabled: ed.insertCommandEnabled("insert.image"), run: () => ed.insertImageFromFile() },
      { id: "insert.imageFromUrl", label: t("insert.imageFromUrl"), group: "Insert", kw: t("insert.imageFromUrl.keywords"), enabled: ed.insertCommandEnabled("insert.imageFromUrl"), run: () => ed.objectInsert.openPictureFromUrl() },
      { id: "insert.shape", label: "Shape…", group: "Insert", kw: "shape drawing autoshape rectangle rounded ellipse circle triangle diamond line arrow callout", enabled: ed.insertCommandEnabled("insert.shape"), run: () => ed.openShapeGallery() },
      { id: "insert.chart", label: t("insert.chart"), group: "Insert", kw: "chart graph column bar line area scatter pie doughnut plot data series", enabled: !!ed.selection, disabledReason: t("paragraph.caretRequired"), run: () => void ed.insertChartWithData() },
      // One row per chart type, the rows the Insert ▸ Chart picker offers
      // (HF-258), so a type is reachable without the ribbon.
      ...insertChartTypeCommands(t, (kind) => void ed.insertChartWithData(kind)).map((row) => ({
        id: row.id,
        label: t("insert.chartOfType", { type: row.label }),
        group: "Insert",
        kw: `chart graph plot ${row.id.slice("insert.chart.".length).replace(/-/g, " ")}`,
        enabled: !!ed.selection,
        disabledReason: t("paragraph.caretRequired"),
        run: row.run,
      })),
      { id: "insert.textbox", label: "Text box", group: "Insert", kw: "text box textbox callout caption floating frame", enabled: ed.insertCommandEnabled("insert.textbox"), run: () => void ed.insertTextBoxObject() },
      { id: "insert.symbol", label: "Symbol…", group: "Insert", kw: "symbol special character glyph currency math greek arrow fraction diacritic omega degree unicode", enabled: ed.insertCommandEnabled("insert.symbol"), run: () => ed.openSymbolPicker() },
      { id: "insert.emoji", label: "Emoji…", group: "Insert", kw: "emoji emoticon smiley face reaction sticker unicode", enabled: ed.insertCommandEnabled("insert.emoji"), run: () => ed.openEmojiPicker() },
      // RM-19: Word's and ONLYOFFICE's Ctrl+Shift+Space; also in Symbol ▸ Special characters.
      { id: "insert.nonBreakingSpace", label: t("insert.nonBreakingSpace"), group: "Insert", kw: "non-breaking space nbsp hard space keep together same line ctrl shift space", enabled: !!ed.doc && !!ed.selection && ed.reviewMode !== "viewing", disabledReason: t("paragraph.caretRequired"), run: () => void ed.pasteText("\u00a0", "typing") },
      // The per-kind field shortcuts share Field…'s precondition; they are the
      // same dialog's choices reached directly from the palette.
      ...FIELD_KINDS.map((f) => ({
        id: `insert.field.${f.kind}`,
        label: `Insert field: ${f.label}`,
        group: "Insert",
        kw: `field ${f.kw}`,
        enabled: ed.insertCommandEnabled("insert.field"),
        run: () => ed.insertFieldAtCaret(f.kind),
      })),
      // The outline lives in the RAIL, so a container that withheld the rail is not
      // handed a command that opens it — the palette and the chord belong to no
      // region, the hole `history` closed the same way ("no panels and nothing").
      // `view.pages` beside it, and gated the same way for the same reason. The
      // two panels are the rail's, so a HOST that withheld the rail region has
      // withheld the panels with it and must not be handed a command that opens
      // one. A PHONE is a different question and the opposite answer: it stops
      // painting the rail strip while keeping both panels, which is why this had
      // to become a command rather than staying a tile (`docs/148` §5.3a). The
      // label is the catalogue's existing `pagesPanel.pages`, so this adds no
      // English literal to a file that is at an unrouted-string ceiling and no
      // key to eighteen catalogues that already answer this one.
      ...(ed.HOST_CHROME.editing.has("rail")
        ? [
            { id: "view.outline", label: "Toggle outline", group: "View", kw: "headings navigation", run: () => ed.toggleOutline() },
            { id: "view.pages", label: t("pagesPanel.pages"), group: "View", kw: "pages panel thumbnails navigator go to page jump browse", enabled: !ed.reflowView.withheldReason(), disabledReason: ed.reflowView.withheldReason(), run: () => ed.pagesPanelView.toggle() },
          ]
        : []),
      { id: "view.showChanges", label: t("panelReview.showChanges.label"), group: "View", kw: "tracked changes markup deletions insertions review redline", run: () => ed.toggleShowChanges() },
      { id: "view.reflow", label: t(ed.reflowView.isOn() ? "reflow.commandOn" : "reflow.commandOff"), group: "View", kw: "reflow pageless continuous column mobile phone reader web layout wrap width", enabled: !ed.readOnlyReason, disabledReason: t("reflow.unavailable"), run: () => ed.reflowView.toggle() },
      ...ed.reflowView.commands(),
      // Fold at the caret, Collapse/Expand All and the level rungs, generated from
      // the module's one table so no two surfaces can offer different sets.
      ...ed.foldView.commands(),
      ...collabCommands({ transport: () => ed.collab, t }),
      ...ed.collabFeatureCommands(),
      // The ¶ toggle and its five switches, the measurement-unit preference, and
      // Restrict Editing. Each module generates its own rows from its own table, so
      // the palette, the menu and the control cannot offer different sets — and
      // `main.js` carries one line per capability instead of eight command literals.
      ...ed.formattingMarks.commands(),
      ...ed.measurement.commands(),
      ...ed.documentProtection.commands(),
      ...ed.reviewDisplay.commands(), // Display for review: Simple, All, No markup, Original
      ...ed.accessChrome.commands(),
      { id: "view.zoomIn", label: "Zoom in", group: "View", kw: "", run: () => ed.stepZoom(1) },
      { id: "view.zoomOut", label: "Zoom out", group: "View", kw: "", run: () => ed.stepZoom(-1) },
      // Ribbon density (docs/104 HF-094). The choice was already real and already
      // persisted, but the ONLY way to reach it was a 28px chevron at the right
      // end of the tab strip — so a user who wanted Docs-style compact chrome had
      // to find an unlabelled arrow. The label carries the current state, the same
      // shape `tools.smartQuotes` uses, so the palette and the View menu both read
      // as a switch rather than as an action with an unknown effect.
      { id: "view.compactRibbon", label: `Compact ribbon: ${ed.ribbonViewCollapsed ? "on" : "off"}`, group: "View", kw: "compact ribbon collapse expand band density toolbar chrome docs word full", noDoc: true, run: () => ed.setRibbonCollapsed(!ed.ribbonViewCollapsed) },
      // Settings OPENS the panel; it does not synthesize a click on the gear
      // button. Clicking a control on the user's behalf inherits that control's
      // event semantics — the gear's handler stops propagation of ITS OWN click,
      // which says nothing about the menu row's click that is still bubbling —
      // and it also inherits the gear's toggle, so picking "Settings" from a menu
      // while the panel was open would have closed it. `noDoc` because theme,
      // accent and reviewer identity are host preferences that do not need a
      // document open, and the gear is the only other way to reach them.
      { id: "view.settings", label: "Settings", group: "View", kw: "theme accent dark appearance preferences identity author name initials", noDoc: true, run: () => revealOnSettings(ed.SETTINGS_SURFACE) },
      { id: "layout.pageSetup", label: "Page setup", group: "Layout", kw: "margins orientation paper size", run: () => ed.pageSetup.open(true) },
      { id: "layout.paragraph", label: "Paragraph properties", group: "Layout", kw: "spacing borders shading indent", enabled: !!ed.selection, disabledReason: "Place the caret in a paragraph", run: () => ed.toggleParagraphProperties(true) },
      { id: "layout.tabStops", label: t("tabStops.command"), group: "Layout", kw: "tab tabs stop stops ruler decimal bar align position", enabled: !!ed.selection && ed.reviewMode !== "viewing", disabledReason: ed.selection ? mutationBlockedMessage({ editingUnavailableReason: ed.readOnlyReason }) : t("paragraph.caretRequired"), run: () => ed.tabStopsDialog.open() },
      // The Layout and References tabs' own rows, generated from the SAME tables
      // their buttons are built from, so a tab button and its palette row can
      // never disagree about whether the command is available or why it is not.
      // Rows without a `label` (the indent pair) already have a palette entry
      // above and only borrow the table's button wiring.
      ...ed.LAYOUT_SURFACE.filter((entry) => entry.label).map((entry) => ({
        id: entry.command,
        label: entry.label,
        group: "Layout",
        kw: entry.kw,
        enabled: ed.ribbonSurfaceEnabled(entry),
        disabledReason: ed.ribbonSurfaceReason(entry),
        run: entry.run ?? (() => {}),
      })),
      ...ed.REFERENCE_SURFACE.filter((entry) => entry.label).map((entry) => ({
        id: entry.command,
        label: entry.label,
        group: "References",
        kw: entry.kw,
        enabled: ed.ribbonSurfaceEnabled(entry),
        disabledReason: ed.ribbonSurfaceReason(entry),
        run: entry.run ?? (() => {}),
      })),
      { id: "help.commands", label: "Find a command…", group: "Help", kw: "help command palette search run", noDoc: true, run: () => ed.openCmd() },
      { id: "help.shortcuts", label: "Keyboard shortcuts", group: "Help", kw: "help shortcuts keys chords reference cheat sheet", noDoc: true, run: () => ed.toggleShortcutsReference(true) },
      { id: "help.about", label: "About OpenDoc", group: "Help", kw: "about version licence license apache build source repository issue report credits", noDoc: true, run: () => ed.toggleAbout(true) },
      // Resolve/Delete must have a menu home, not only a ribbon button:
      // `docs/105` command-surface parity forbids a capability reachable from one
      // surface, and `review-surface.spec.mjs` enforces it.
      { id: "review.comment.resolve", label: t("reviewComment.resolve"), group: "Review", kw: "resolve close comment thread done", enabled: !!ed.activeReviewCommentId && ed.HOST_CAPS.has("comment"), disabledReason: t("reviewComment.needsCaret"), run: () => void ed.reviewCommentActions.resolve() },
      { id: "review.comment.delete", label: t("reviewComment.delete"), group: "Review", kw: "delete remove comment thread", enabled: !!ed.activeReviewCommentId && ed.HOST_CAPS.has("comment"), disabledReason: t("reviewComment.needsCaret"), run: () => void ed.reviewCommentActions.remove() },
      {
        id: "review.comment",
        label: "Add comment",
        group: "Review",
        kw: "annotate note",
        enabled: context.hasRange ?? ed.hasRange(),
        disabledReason: "Select text to comment on",
        run: () => ed.openReviewComposer(),
      },
      { id: "tools.smartQuotes", label: ed.autoCorrect.label("smartQuotes"), group: "Tools", kw: "smart curly typographic quotes apostrophe straight autocorrect", noDoc: true, run: () => ed.autoCorrect.toggle("smartQuotes") },
      { id: "tools.autoFormatLists", label: ed.autoCorrect.label("lists"), group: "Tools", kw: "automatic lists bullet numbered autoformat as you type 1. dash asterisk", noDoc: true, run: () => ed.autoCorrect.toggle("lists") },
      { id: "tools.autocorrect", label: ed.autoCorrect.label("autocorrect"), group: "Tools", kw: "autocorrect replace text as you type typos copyright trademark registered table", noDoc: true, run: () => ed.autoCorrect.toggle("autocorrect") },
      // RM-19: the rest of Word's AutoCorrect switches, and the dialog that holds them all.
      { id: "tools.autoCapitalise", label: ed.autoCorrect.label("capitalise"), group: "Tools", kw: "autocorrect capitalise capitalize first letter sentence table cell", noDoc: true, run: () => ed.autoCorrect.toggle("capitalise") },
      { id: "tools.autoFormatDashes", label: ed.autoCorrect.label("dashes"), group: "Tools", kw: "autoformat hyphens dash em en -- typographic", noDoc: true, run: () => ed.autoCorrect.toggle("dashes") },
      { id: "tools.autoFormatHyperlink", label: ed.autoCorrect.label("hyperlinks"), group: "Tools", kw: "autoformat hyperlink url web address email link automatic internet", noDoc: true, run: () => ed.autoCorrect.toggle("hyperlinks") },
      { id: "tools.autoCorrectOptions", label: t("autocorrect.optionsCommand"), group: "Tools", kw: "autocorrect options autoformat as you type replace with table exceptions settings", noDoc: true, run: () => ed.autoCorrectDialog.open() },
      // RM-42: Word's Review ▸ Language ▸ Set Proofing Language.
      { id: "format.language", label: t("language.command"), group: "Format", kw: "language proofing spelling set language selection text french german w:lang", enabled: !!ed.doc && !!ed.selection && ed.reviewMode !== "viewing", disabledReason: t("paragraph.caretRequired"), run: () => ed.languageSelection.open() },
      // Reads as a switch, the shape `tools.smartQuotes` already uses, so the menu
      // row and the palette row both say which state it is in rather than being an
      // action with an unknown effect. Never disabled: off has to be reversible
      // from the same place it was set (SKILL.md §10).
      { id: "tools.spellCheck", label: `Spell check: ${ed.settings.spellCheck === false ? "off" : "on"}`, group: "Tools", kw: "spelling spell check squiggle dictionary misspelled proofing language red underline glossary", noDoc: true, run: () => ed.setSpellCheckEnabled(ed.settings.spellCheck === false) },
      { id: "tools.grammarCheck", label: `Grammar check: ${ed.settings.grammarCheck === false ? "off" : "on"}`, group: "Tools", kw: "grammar check agreement doubled word article a an punctuation capitalisation capitalization proofing blue underline", noDoc: true, run: () => ed.setGrammarCheckEnabled(ed.settings.grammarCheck === false) },
      // HF-051. Word's Review ▸ Proofing ▸ Word Count, and its ⌘⇧G.
      { id: "tools.wordCount", label: t("wordCount.command"), group: "Tools", kw: "word count words characters paragraphs pages statistics length how many", enabled: !!ed.doc, disabledReason: t("command.needsDocument"), run: () => ed.wordCount.open() },
      { id: "tools.languages", label: t("proofLanguages.command"), group: "Tools", kw: "language pack proofing spelling dictionary install download offline locale manage languages", noDoc: true, run: () => ed.proofing.openLanguages() },
      { id: "review.toggle", label: "Toggle comments & suggestions", group: "Review", kw: "sidebar review panel", run: () => ed.toggleReview() },
      { id: "review.compare", label: t("compare.command"), group: "Review", kw: "compare diff difference changes two documents review merge", enabled: !!ed.doc, disabledReason: t("compare.needsDocument"), run: () => ed.comparePanel.open() },
      // ⌘⇧E worked before this row and was advertised NOWHERE — no palette row, no
      // menu row, no reference entry — because the binding was a hand-written
      // keydown branch and only descriptors carry labels (`109` UX-007). It exists
      // as a command now, so the chord is discoverable on every surface. Google Docs
      // cycles its three modes from one chord too; Word has no equivalent.
      { id: "review.mode.cycle", label: "Next review mode", group: "Review", kw: "review mode cycle switch editing suggesting viewing next", run: () => ed.cycleReviewMode() },
      { id: "review.mode.editing", label: "Editing mode", group: "Review", kw: "review mode edit", run: () => ed.setReviewMode("editing") },
      { id: "review.mode.suggesting", label: "Suggesting mode (track changes)", group: "Review", kw: "review mode track changes suggest", run: () => ed.setReviewMode("suggesting") },
      { id: "review.mode.viewing", label: "Viewing mode (read-only)", group: "Review", kw: "review mode view read only", run: () => ed.setReviewMode("viewing") },
      { id: "review.next", label: "Next comment or change", group: "Review", kw: "revision suggestion comment navigate forward", run: () => ed.navigateReview(1) },
      { id: "review.previous", label: "Previous comment or change", group: "Review", kw: "revision suggestion comment navigate back", run: () => ed.navigateReview(-1) },
      { id: "review.acceptAtCaret", label: "Accept change at cursor", group: "Review", kw: "revision suggestion approve current", run: () => ed.decideReviewAtCaret(true) },
      { id: "review.rejectAtCaret", label: "Reject change at cursor", group: "Review", kw: "revision suggestion discard current", run: () => ed.decideReviewAtCaret(false) },
      { id: "review.acceptNext", label: "Accept change and move to next", group: "Review", kw: "revision suggestion approve next advance", run: () => ed.decideReviewAndAdvance(true) },
      { id: "review.rejectNext", label: "Reject change and move to next", group: "Review", kw: "revision suggestion discard next advance", run: () => ed.decideReviewAndAdvance(false) },
      { id: "review.acceptAll", label: "Accept all changes", group: "Review", kw: "revision suggestion approve", run: () => void ed.decideAllReviewChanges(true) },
      { id: "review.rejectAll", label: "Reject all changes", group: "Review", kw: "revision suggestion discard", run: () => void ed.decideAllReviewChanges(false) },
    ];
    const styleTarget = ed.currentParagraphStyleName();
    cmds.push(
      {
        id: "style.updateFromSelection",
        label: styleTarget ? `Update “${styleDisplayName(styleTarget)}” to match selection` : "Update style to match selection",
        group: "Style",
        kw: "redefine modify match formatting paragraph style",
        enabled: !!styleTarget,
        disabledReason: "Place the caret in a paragraph that uses a named style",
        run: () => ed.updateStyleFromSelection(styleTarget),
      },
      {
        id: "style.createFromSelection",
        label: "Create style from selection…",
        group: "Style",
        kw: "new define save formatting paragraph style",
        enabled: !!ed.selection,
        disabledReason: "Place the caret in a paragraph first",
        run: () => ed.createStyleFromSelection(),
      },
    );
    // Line spacing and the list-marker galleries were ribbon-only: they exist as
    // popover controls on the Home tab and had no command entry at all, so neither
    // the palette nor any app menu could reach them. Docs gives line spacing a
    // whole top-level submenu (Format ▸ Line & paragraph spacing) and both products
    // put the marker galleries behind a searchable menu path.
    //
    // The rows are generated FROM the popovers' own markup rather than restated
    // here: the presets carry their percentage and label, and each gallery cell
    // carries the spec it applies plus the assistive-technology name it already
    // needs. A preset or bullet style added to the markup therefore gets its
    // command for free, and none of these labels can drift from the control they
    // mirror. Both run the same functions the popovers run.
    // The line-spacing presets and the two one-gesture space rows, generated from
    // the popover's own markup by `spacing_menu.mjs` so a preset added there gets
    // its command — and its ⌘1 / ⌘5 / ⌘2 chord's meaning — for free.
    cmds.push(...ed.spacingMenuControl.commands({ enabled: !!ed.doc && !!ed.selection }));
    if (ed.doc) {
      for (const [menu, noun] of [[ed.bulletGalleryMenu, "Bullet style"], [ed.numberGalleryMenu, "Numbering format"]]) {
        for (const cell of menu?.querySelectorAll(".list-gallery-cell") ?? []) {
          const spec = cell.dataset.spec;
          cmds.push({
            id: `paragraph.listFormat.${spec}`,
            label: `${noun}: ${cell.getAttribute("aria-label")}`,
            group: "Paragraph",
            kw: `list marker bullet numbering ${cell.getAttribute("aria-label")}`.toLowerCase(),
            enabled: !!ed.selection,
            disabledReason: "Place the caret in a list",
            run: () => ed.applyListFormatCommand(spec),
          });
        }
      }
    }
    // HF-149 / HF-150 — the nine zoom presets and the underline-style menu were
    // chrome with no ids, so the palette, the app menu and any host driving the
    // editor could step zoom but never ASK for 150%, and could turn underline on
    // but never make it wavy. Generated from the same markup and the same label
    // map the controls themselves use, so a preset or a style added there gets its
    // command for free and no label can drift from the control it mirrors.
    for (const preset of ed.zoomMenu?.querySelectorAll(".zoom-preset") ?? []) {
      const factor = Number(preset.dataset.zoom);
      cmds.push({
        id: `view.zoom.${Math.round(factor * 100)}`,
        label: `Zoom: ${preset.querySelector(".menu-item-label")?.textContent?.trim() ?? `${Math.round(factor * 100)}%`}`,
        group: "View",
        kw: "zoom scale magnify percent",
        run: () => ed.setZoom(factor),
      });
    }
    for (const fit of ed.zoomMenu?.querySelectorAll(".zoom-fit") ?? []) {
      const mode = fit.dataset.zoomMode;
      cmds.push({
        id: `view.zoom.${mode === "fit-width" ? "fitWidth" : "fitPage"}`,
        label: `Zoom: ${fit.querySelector(".menu-item-label")?.textContent?.trim() ?? mode}`,
        group: "View",
        kw: "zoom fit width page whole",
        run: () => ed.setZoomMode(mode),
      });
    }
    // The open-ended one: any percentage, not just the nine on the menu. With no
    // argument it focuses the field the user would have typed into anyway.
    cmds.push({
      id: "view.zoom",
      label: "Zoom to…",
      group: "View",
      kw: "zoom percent custom exact scale",
      run: (percent) => {
        const value = Number(percent);
        if (!Number.isFinite(value) || value <= 0) {
          ed.zoomEl.focus();
          ed.zoomEl.select();
          return;
        }
        ed.setZoom(value / 100);
      },
    });
    // The exact values behind the two "…" commands above. Generated from the same
    // inventory the font menu renders and the same step table Grow/Shrink walks,
    // so "Font: Georgia" and "Font size: 14 pt" mean exactly what picking them off
    // the ribbon means, and a face added to the inventory gets its command free.
    if (ed.selection) {
      for (const name of ed.fontInventory()) {
        cmds.push({
          id: `format.family.${name}`,
          label: `Font: ${name}`,
          group: "Format",
          kw: `font typeface family ${name}`.toLowerCase(),
          run: () => ed.applyFontFamily(name),
        });
      }
      for (const points of FONT_SIZE_STEPS) {
        cmds.push({
          id: `format.size.${points}`,
          label: `Font size: ${points} pt`,
          group: "Format",
          kw: `font size point ${points}`,
          run: () => ed.applyFontSize(points, { report: false }),
        });
      }
    }
    for (const [style, label] of ed.UNDERLINE_STYLE_LABELS) {
      if (style === "none") continue; // "no underline" is `format.underline` off
      cmds.push({
        id: `format.underline.${style}`,
        label: `Underline: ${label}`,
        group: "Format",
        kw: `underline ${label} line style`.toLowerCase(),
        enabled: !!ed.selection,
        disabledReason: "Place the caret or select text",
        run: () => ed.applyUnderlineStyle(style),
      });
    }
    if (ed.doc) {
      cmds.push(...ed.fontAdvanced.commands(), ...ed.paragraphAdvanced.commands(), ...ed.listAdvanced.commands(), ...ed.styleAdvanced.commands());
      cmds.push(...quickStyleCommands({ styles: () => ed.doc.listStyles(), hasCaret: () => !!ed.selection, apply: (name) => ed.runToolbarEdit((s, o, e, f) => ed.doc.setParagraphStyle(s, o, e, f, name), { paragraphLevel: true }) }));
      for (const name of ed.doc.listStyles()) {
        cmds.push({
          id: `style.${name}`,
          label: `Style: ${styleDisplayName(name)}`,
          group: "Style",
          kw: `paragraph heading ${name}`.toLowerCase(),
          run: () => ed.runToolbarEdit((s, o, e, f) => ed.doc.setParagraphStyle(s, o, e, f, name), { paragraphLevel: true }),
        });
      }
    }
    // Table structure, when the caret is in a table. These commands existed only on
    // the contextual Table ribbon tab and the right-click menu, so searching the
    // palette for "insert row" or "merge cells" found nothing — the same
    // one-surface-only defect that left Picture off the Insert ribbon. Word reaches
    // every one of them from Tell Me; Docs files them under Format ▸ Table. Built
    // from the caret's own context so enablement and the "why not" reasons are the
    // menu's, not a second opinion, and flattened out of their submenus with the
    // parent's name kept ("Table: Insert row above") so the palette reads as a flat
    // searchable list. `surface` is checked because the context menu composes these
    // rows itself — it must not receive them twice.
    if (ed.doc && context.surface !== "context" && ed.selection && ed.plainTableInfo(ed.selection.anchor.node)) {
      cmds.push(
        ...flattenCommandTree(ed.tableToolCommands(ed.contextAt(ed.selection.anchor)), (entry, trail) => ({
          label: tableCommandLabel(entry.id, entry.label, trail, context.surface),
          group: "Table",
          kw: `table ${trail} ${entry.label}`.toLowerCase(),
        })),
      );
      // Table STYLE, generated from the engine's own list — the very list the band's
      // chooser renders (`populateTableStyles`). It was the one table capability with
      // a single surface: the `#tableStyleBtn` popover and nothing else, no menu row
      // and no palette row, so applying a table style was mouse-only (`109` UX-005).
      // Generated rather than enumerated, for the same reason `style.<name>` is: a
      // style the document defines gets its command without anyone writing it down.
      const applyTableStyle = (name) =>
        ed.runEdit(() => ed.doc.applyTableStyle(ed.selection.focus.node, name), { gate: true });
      cmds.push({ id: "table.style.none", label: ed.NO_TABLE_STYLE, group: "Table", kw: "table style clear none plain remove banding", run: () => applyTableStyle("") });
      for (const name of ed.doc.listTableStyles?.() ?? []) {
        cmds.push({
          id: `table.style.${name}`,
          label: `Table style: ${name}`,
          group: "Table",
          kw: `table style banding grid ${name}`.toLowerCase(),
          run: () => applyTableStyle(name),
        });
      }
    } else if (context.surface === "menu" || context.surface === "compact") {
      // The Table MENU must exist even when the caret is not in a table: an empty
      // popover says the editor cannot edit tables (UX-012). `command_taxonomy`
      // builds the greyed rows from the same label map the live rows use, and
      // `menu_taxonomy` fails if that map and the real command set disagree.
      // The COMPACT bar's Table dropdown is the same surface by the same argument
      // (TBL-18): in compact mode there is no ribbon, so this dropdown is where a
      // user learns tables are editable at all, and an empty one says they are not.
      cmds.push(
        // The same sentence the BAND shows for the same precondition, from the one
        // catalogue entry (`docs/141` TBL-03).
        ...tableMenuPlaceholders(ed.doc ? t("table.reason.caretOutsideTable") : "Open a document first"),
      );
    }
    // Object commands, on the same terms. Everything a selected image, shape or
    // text box can do lived on the floating bar and (mostly) the right-click menu
    // and nowhere else: the palette had no object command at all, and "Object
    // properties" had exactly one route in the entire product. A capability
    // reachable from one surface is this repo's recurring defect, and tables were
    // already fixed by exactly the flattening above.
    if (ed.objectSelection && ed.objectSelection.mode === "selected") {
      // Flattened the same way as the table rows above, because some object
      // commands are submenus (Wrap text) and a palette entry whose `run` is a
      // submenu is a dead row.
      cmds.push(
        ...flattenCommandTree(buildObjectContextCommands(ed.selectedObjectContext(), ed.objectContextMenuHost), (entry, trail) => ({
          label: trail ? `Object: ${trail} ${entry.label}` : `Object: ${entry.label}`,
          group: "Object",
          kw: `object image picture shape text box ${trail} ${entry.label}`.toLowerCase(),
        })),
      );
    }
    // Reaching an object at all is a command too, and it is the one that has to
    // work with no object selected — otherwise every row above is unreachable
    // without a mouse.
    cmds.push(
      {
        id: "object.selectNext",
        label: "Select next object",
        group: "Object",
        kw: "object image picture shape text box select next navigate keyboard",
        enabled: ed.documentHasObjects(),
        disabledReason: "This document has no images, shapes or text boxes",
        run: () => ed.traverseObjects(1),
      },
      {
        id: "object.selectPrevious",
        label: "Select previous object",
        group: "Object",
        kw: "object image picture shape text box select previous navigate keyboard",
        enabled: ed.documentHasObjects(),
        disabledReason: "This document has no images, shapes or text boxes",
        run: () => ed.traverseObjects(-1),
      },
    );
    // The chord a command advertises comes from the keymap, never from a literal
    // written here (`109` UX-006/UX-007). This one line is what makes a label that
    // disagrees with its binding unrepresentable: the palette hint, the menu hint,
    // the compact bar's tooltip and the shortcut reference all read
    // `command.shortcut`, and the only thing that can set it is the table the
    // dispatcher matches against.
    cmds.push(...ed.compatFindings.commands(), ...(ed.pdfCommands?.commands() ?? []), ...ed.regionCycle.commands());
    for (const command of cmds) command.shortcut = shortcutForCommand(command.id, ed.EDITOR_KEYBOARD_PLATFORM);
    // Narrowed to the ROOM's grant HERE, once, for every surface this registry
    // feeds — palette, menu bar, compact bar, context menus — rather than by a
    // clause in each governed row. A review command added later is governed by the
    // id it already has instead of shipping ungated (`SKILL` §10: fix the pattern).
    // Then to the CONTAINER's grant for deciding a tracked change, which the host
    // contract has always required `edit` for (HF-199, `review_authority.mjs`).
    return narrowDecisionsToContainer(ed.SESSION.narrow(cmds.filter((command) => ed.doc || command.noDoc)), ed.HOST_CAPS);
  }

  return { cmdInput, cmdList, cmdPalette, editorCommands, searchTrigger };
}
