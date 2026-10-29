// The Insert, Layout, References and Review command surfaces: one declaration
// per command, read by the ribbon, the menus and the palette alike. Moved out
// of `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { createPageBorders } from "./page_borders.mjs";
import { t } from "./i18n.mjs";
import { reviewDecisionRefusal } from "./review_authority.mjs";
import { breakSurfaceRows } from "./break_commands.mjs";
import { noteSurfaceRows } from "./note_commands.mjs";
import { objectMenuRows } from "./reference_commands.mjs";
import { ribbonSurfaceEnabled as surfaceEnabled, ribbonSurfaceReason as surfaceReason } from "./ribbon_surface.mjs";
import { arrangeSurfaceRows } from "./object_arrange_commands.mjs";
import { createReviewCommentActions } from "./review_comment_actions.mjs";
import { tableTools } from "./table_tools.mjs";
import { tableDrawMode } from "./table_draw_mode.mjs";
import { tocSurfaceRows } from "./toc_commands.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createCommandSurfaces(ed) {
  // ---- Insert surface: one declaration per Insert command ---------------------
  // The Insert ribbon tab, the Insert app menu, and the command palette are three
  // faces of the SAME command set. Their membership used to be authored three
  // times — hand-written HTML buttons, an id list in APP_MENU_SECTIONS, and
  // bespoke `someBtn.disabled = …` lines in `updateToolbar` — with nothing
  // reconciling them, and they drifted: Picture, Symbol, Emoji, Bookmark and Field
  // shipped to the menu and the palette while the ribbon still showed only Table
  // and Link, so a user looking at the Insert tab could not insert a picture at
  // all. This table is the single declaration the ribbon is built from: each row
  // names the command it mirrors, the button that renders it, what the command
  // genuinely requires, and how the button activates it.
  // It unifies enablement and activation, not membership: the buttons are still
  // authored in editor.html and the menu roster is still its own id list, so a new
  // command CAN still reach one surface and miss another. What closes that gap is
  // the test — each row stamps its command id onto its button, and
  // `insert-surface.spec.mjs` asserts the ribbon's id set equals the Insert menu's,
  // so the omission that shipped Picture unreachable now fails CI instead.
  //
  // `buttons` is a LIST because one command can legitimately have more than one
  // ribbon face: Word shows Bookmark on both Insert ▸ Links and References ▸
  // Navigation, and Insert field on both Insert ▸ Text and References ▸ Fields.
  // Declaring the faces here keeps enablement and activation single-sourced —
  // a second `someBtn.disabled = …` line written next to the new tab is exactly
  // how the Insert ribbon drifted out of sync with its menu the first time.
  const INSERT_SURFACE = [
    // "doc" is Word's rule: an open document has an insertion point, so the
    // command is live the moment a document loads (see `implicitCaretAt`).
    // Viewing/Suggesting are deliberately NOT expressed here — every run path
    // already fails closed through `blockMutationInViewing` /
    // `blockUntrackedInSuggesting`, which tells the user WHY the insert did not
    // happen. A greyed button would only say "no".
    { command: "insert.table", buttons: [ed.insertTableBtn], requires: "doc", activate: null },
    // Insert Table by size and Convert Text to Table, beside the grid (Word keeps
    // both in the Table dropdown). The two dialogs are `table_tools.mjs`'s.
    { command: "insert.tableDialog", buttons: [document.getElementById("insertTableDialogBtn")].filter(Boolean), requires: "doc", activate: () => tableTools.openDialog("insertTable") },
    { command: "insert.textToTable", buttons: [document.getElementById("textToTableBtn")].filter(Boolean), requires: "doc", activate: () => tableTools.openDialog("textToTable") },
    // Draw Table (`table_draw_mode.mjs`): the Table band's pencil toggles the mode;
    // the Insert table popover's own row is wired by `editor_table_props.mjs`,
    // because a row inside a popover is not a ribbon face.
    { command: "insert.drawTable", buttons: [document.getElementById("tableDrawBtn")].filter(Boolean), requires: "doc", activate: () => tableDrawMode.toggle() },
    { command: "insert.image", buttons: [ed.insertPictureBtn], requires: "doc", activate: () => ed.insertImageFromFile() },
    // Link is the one Insert command that genuinely needs a range: it hyperlinks
    // selected text. A cross-paragraph range still enables the button and is
    // answered by `editSelectionLink`'s "Links must stay within one paragraph",
    // which is more useful than a silently dead control.
    // The gallery popover registers its own toggle on this button (like the list
    // galleries), so wiring `activate` here too would open it on mousedown and
    // immediately close it again.
    { command: "insert.shape", buttons: [ed.insertShapeBtn], requires: "doc", activate: null },
    // Insert ▸ Chart opens the type picker first, as ONLYOFFICE's button does
    // (`109` HF-258); each row inserts that type and opens its data.
    { command: "insert.chart", buttons: [ed.insertChartBtn], requires: "doc", activate: () => ed.chartSurface.openInsertMenu(ed.insertChartBtn, (kind) => void ed.insertChartWithData(kind)) },
    { command: "insert.textbox", buttons: [ed.insertTextBoxBtn], requires: "doc", activate: () => void ed.insertTextBoxObject() },
    { command: "insert.link", buttons: [ed.insertLinkBtn], requires: "range", activate: () => ed.editSelectionLink() },
    { command: "insert.bookmark", buttons: [ed.insertBookmarkBtn, ed.refBookmarkBtn], requires: "doc", activate: () => ed.openBookmarkManager() },
    { command: "insert.field", buttons: [ed.insertFieldBtn, ed.refFieldBtn], requires: "doc", activate: () => ed.openFieldDialog() },
    // Page number and Date are the two field kinds the competition puts on the
    // Insert TAB rather than behind a generic field picker: Word's Insert tab has
    // Page Number in its Header & Footer group and Date & Time in its Text group,
    // and ONLYOFFICE's Insert tab has both as their own buttons (their groups 5
    // and 6). Here they were one level deeper — Insert ▸ Field, then a row in the
    // picker — which is a reachability gap, not a missing capability.
    //
    // They are the SAME commands the palette already registers per kind
    // (`insert.field.<kind>`, generated from FIELD_KINDS) running the SAME
    // `insertFieldAtCaret`, so there is no second insert path to keep in step:
    // this table supplies the enablement and the click, exactly as it does for the
    // picker's own button. The other four kinds (number of pages, time, file name,
    // author) stay in the picker, where Word and ONLYOFFICE also keep them.
    { command: "insert.field.page", buttons: [ed.insertPageNumberBtn], requires: "doc", activate: () => ed.insertFieldAtCaret("page") },
    { command: "insert.field.date", buttons: [ed.insertDateBtn], requires: "doc", activate: () => ed.insertFieldAtCaret("date") },
    { command: "insert.dropCap", buttons: [ed.insertDropCapBtn], requires: "doc", activate: () => ed.dropCapDialog.open() },
    // Notes live on References only, as they do in Word. The app-menu row and the
    // palette entry are untouched, so the command keeps three surfaces.
    { command: "insert.footnote", buttons: [ed.refFootnoteBtn], requires: "doc", activate: () => ed.insertNote("footnote") },
    { command: "insert.endnote", buttons: [ed.refEndnoteBtn], requires: "doc", activate: () => ed.insertNote("endnote") },
    { command: "insert.header", buttons: [ed.insertHeaderBtn], requires: "doc", activate: () => ed.editRunningContent("header") },
    { command: "insert.footer", buttons: [ed.insertFooterBtn], requires: "doc", activate: () => ed.editRunningContent("footer") },
    { command: "insert.symbol", buttons: [ed.insertSymbolBtn], requires: "doc", activate: () => ed.openSymbolPicker() },
    { command: "insert.emoji", buttons: [ed.insertEmojiBtn], requires: "doc", activate: () => ed.openEmojiPicker() },
  ];

  // ---- Layout and References surfaces ----------------------------------------
  // The same seam as INSERT_SURFACE/REVIEW_SURFACE, for the two tabs the ribbon
  // did not have (docs/105 UX-010, and the IA half of OO-001/OO-005). Each row is
  // one command: the palette entry, the ribbon button, and the enablement rule all
  // read from this table, so there is nowhere to write a second opinion.
  //
  // `requires` values:
  //   "doc"      — an open document is the only precondition.
  //   "caret"    — a paragraph to act on (the caret, wherever it is).
  //   "object"   — a selected image, shape or text box.
  //   "missing"  — the command is REAL as a user intention but has no engine
  //                operation behind it. It ships permanently disabled carrying the
  //                reason, because a control that silently does nothing is the one
  //                thing docs/63 forbids outright, and hiding it would make the
  //                gap invisible to the person deciding what to build next.
  const BREAK_IO = { doc: () => ed.doc, caret: () => ed.selection?.focus ?? null, runEdit: (thunk, options) => ed.runEdit(thunk, options), setStatus: (text) => ed.setStatus(text) };
  // Page borders (HF-193): Word's Borders and Shading ▸ Page Border.
  const pageBorders = createPageBorders({
    getDoc: () => ed.doc,
    selectionNode: () => ed.selection?.focus?.node ?? null,
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    fallbackFocus: () => document.getElementById("pageBordersBtn") ?? ed.pagesEl,
  });
  const LAYOUT_SURFACE = [
    // Page setup: one dialog, four fieldsets. Word's four buttons are four routes
    // into the same section geometry; each one opens the dialog with its own
    // fieldset focused rather than pretending to be a separate dialog.
    { command: "layout.margins", label: "Page margins", kw: "margins page setup top bottom left right gutter", buttons: () => [ed.layoutMarginsBtn], requires: "doc", run: () => ed.pageSetup.open(true, "margins") },
    { command: "layout.orientation", label: "Page orientation", kw: "orientation portrait landscape rotate page setup", buttons: () => [ed.layoutOrientationBtn], requires: "doc", run: () => ed.pageSetup.open(true, "orientation") },
    { command: "layout.size", label: "Page size", kw: "size paper a4 letter legal width height page setup", buttons: () => [ed.layoutSizeBtn], requires: "doc", run: () => ed.pageSetup.open(true, "size") },
    { command: "layout.columns", label: "Text columns", kw: "columns newspaper two three spacing separator page setup", buttons: () => [ed.layoutColumnsBtn], requires: "doc", run: () => ed.pageSetup.open(true, "columns") },
    // The button's click is the popover's own (`registerPopover`), so this row
    // declares `ownsClick` and contributes only the command id, the enablement
    // rule and the palette row — the same split INSERT_SURFACE spells `activate:
    // null` for `insert.table`. Without it the wiring below would attach a handler
    // that re-clicks the button the popover manager is already listening on.
    { command: "layout.lineNumbers", label: "Line numbers", kw: "line numbers numbering margin legal pleading count suppress", buttons: () => [ed.lineNumbersBtn], requires: "doc", ownsClick: true, run: () => ed.pageSetup.openLineNumbers() },
    // Watermark. NOT `ownsClick`: this one opens a modal the module registers
    // itself, so the click belongs to this table's own wiring — the popover
    // manager is not involved, and `page_setup.mjs` deliberately binds no listener
    // of its own to the button.
    // Page borders: a modal this table's own wiring opens, the watermark's shape.
    // `label` is a getter so the palette reads it after the catalogue lands.
    { command: "layout.pageBorders", get label() { return t("pageBorders.command"); }, kw: "page border borders box frame outline around page art", buttons: () => [document.getElementById("pageBordersBtn")].filter(Boolean), requires: "doc", run: () => pageBorders.open() },
    { command: "layout.watermark", label: "Watermark", kw: "watermark draft confidential sample stamp diagonal background behind text", buttons: () => [ed.watermarkBtn], requires: "doc", run: () => ed.pageSetup.openWatermark(true) },
    // Paragraph: the existing relative nudges, plus the two fieldsets of the
    // paragraph-properties panel that hold the absolute indent and spacing values.
    // The ribbon deliberately does NOT carry its own numeric fields: the panel
    // reflects engine state and applies through the gated edit path, and a second
    // pair of inputs would be a second answer to "what is this paragraph's indent".
    { command: "paragraph.indent.decrease", buttons: () => [ed.layoutIndentDecBtn], requires: "caret", run: () => ed.adjustIndentCommand(-360) },
    { command: "paragraph.indent.increase", buttons: () => [ed.layoutIndentIncBtn], requires: "caret", run: () => ed.adjustIndentCommand(360) },
    { command: "layout.indent", label: "Indentation…", kw: "indent left right first line hanging exact fields paragraph", buttons: () => [ed.layoutIndentFieldsBtn], requires: "caret", run: () => ed.toggleParagraphProperties(true, () => ed.indentLeftInput) },
    { command: "layout.spacing", label: "Paragraph spacing…", kw: "spacing line before after leading paragraph fields", buttons: () => [ed.layoutSpacingFieldsBtn], requires: "caret", run: () => ed.toggleParagraphProperties(true, () => ed.paraLineSpacing) },
    // Arrange: the object inspector's own wrap and geometry sections, reached from
    // a durable affordance instead of only the floating bar that appears on hover.
    // The two running-content variants. No `label`/`kw`: `editorCommands` already
    // declares both rows (their labels read as switches — "Different first page:
    // on"), and a second declaration here would put two rows for one command in
    // the palette. What they needed was a RIBBON FACE: they were reachable from
    // the Insert menu and the palette only, so a chrome with no menu bar left them
    // palette-only. `pressed` is what makes the button say which way the switch is
    // set, the way the Review toggles do.
    { command: "layout.firstPageVariant", buttons: () => [ed.insertFirstPageVariantBtn], requires: "doc", pressed: () => ed.headerFooterSettings.variantState().firstPage, run: () => ed.headerFooterSettings.toggleVariant("firstPage") },
    { command: "layout.evenOddVariant", buttons: () => [ed.insertEvenOddVariantBtn], requires: "doc", pressed: () => ed.headerFooterSettings.variantState().evenOdd, run: () => ed.headerFooterSettings.toggleVariant("evenOdd") },
    // Header and footer settings. No `label`/`kw` for the same reason as the two
    // above: `editorCommands` declares the palette row, and a second declaration
    // here would put two rows for one command in the palette.
    { command: "layout.headerFooterSettings", buttons: () => [ed.headerFooterSettingsBtn], requires: "doc", run: () => ed.headerFooterSettings.open(true) },
    { command: "layout.arrange.wrap", label: "Wrap text around object", kw: "wrap text square tight through behind front object image shape arrange", buttons: () => [ed.layoutWrapBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-inspector-wrap-select]") },
    { command: "layout.arrange.position", label: "Object position and size", kw: "position size move object image shape arrange exact geometry", buttons: () => [ed.layoutPositionBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-prop=left]") },
    ...breakSurfaceRows(BREAK_IO),
    // Link to Previous, Remove and the six page-number positions (HF-192, RM-23):
    // palette and Insert-menu rows from `editor_header_footer.mjs`, whose band
    // bar is their in-place face.
    ...ed.headerFooterBand.surfaceRows(),
    ...arrangeSurfaceRows(ed.objectArrange, {
      bringForward: () => ed.layoutBringForwardBtn,
      sendBackward: () => ed.layoutSendBackwardBtn,
      group: () => ed.layoutGroupBtn,
      ungroup: () => ed.layoutUngroupBtn,
      rotate: () => ed.layoutRotateBtn,
      inLine: () => ed.setObjectWrap("inline"),
    }),
  ];

  const REFERENCE_SURFACE = [
    {
      command: "reference.tableOfContents",
      label: "Table of contents",
      kw: "table of contents toc outline headings index navigation",
      buttons: () => [ed.refTocBtn],
      // Body-only, and for the engine's own reason: `insertTableOfContents`
      // refuses a caret outside the body, as Word does.
      requires: "bodyCaret",
      run: () => ed.tocCommands.insert.open(),
    },
    // The keyboard half of following a contents entry. The pointer half is a plain
    // click on the entry, and `contextMenu: true` puts the same row on the
    // right-click menu, so the capability is reachable from three surfaces and from
    // none of them only.
    {
      command: "reference.goToHeading",
      label: "Go to the heading this entry names",
      kw: "toc table of contents entry heading navigate go to jump follow outline",
      buttons: () => [ed.refGoToHeadingBtn],
      requires: "tocEntry",
      // NOT `contextMenu: true` — that flag puts a row on the OBJECT and table
      // right-click menus, and a contents entry is text. Its right-click row is
      // built with the other text rows, beside Open link, which is the same
      // capability for the case where the author's field did hyperlink it.
      run: () => void ed.followTocEntry(ed.selection?.focus?.node),
    },
    // Word's Captions group. A caption attaches to the caret's block — which is
    // also the block that holds a selected picture or table, since selecting an
    // object puts the caret at its anchor — and a cross-reference is inserted at
    // the caret. `contextMenu: true` is what puts Insert caption on the object and
    // table right-click menus, the way ONLYOFFICE does
    // (`DocumentHolderExt.js:46`), from this one declaration.
    {
      command: "reference.caption",
      // English, like every other row here: these labels are read at import, before
      // a catalogue exists, so `t()` would put the KEY in the palette. The
      // context-menu row is built on demand and does route through the catalogue.
      label: "Insert caption",
      kw: "caption figure table equation label numbering sequence seq chapter",
      buttons: () => [ed.refCaptionBtn],
      requires: "bodyCaret",
      contextMenu: true,
      run: () => ed.referenceCommands.caption.open(),
    },
    {
      command: "reference.crossReference",
      label: "Cross-reference",
      kw: "cross reference ref heading bookmark figure numbered item caption footnote endnote",
      buttons: () => [ed.refCrossRefBtn],
      requires: "caret",
      run: () => ed.referenceCommands.crossReference.open(),
    },
    // Word recomputes fields on print, on F9 and on open; ours are not recomputed
    // on a Backspace, deliberately, because renumbering there would make a keystroke
    // O(document). So the product has to OFFER the fix, or "dirty rather than
    // silently stale" is just "stale".
    {
      command: "reference.updateCaptionNumbers",
      label: "Update caption numbers",
      kw: "update caption numbers renumber stale seq figure table refresh fix",
      buttons: () => [ed.refUpdateCaptionsBtn],
      requires: "staleCaptions",
      run: () => void ed.referenceCommands.numbering.update(),
    },
    // Word's Update Table, and its two-radio dialog. The id stays
    // `reference.updateFields` because it is a published host-contract id; what
    // changed is that it now runs something. The two MODES are commands of their
    // own, so neither is reachable only from inside a modal — the same
    // >=2-surface rule the rest of this table follows.
    { command: "reference.updateFields", label: "Update table of contents", kw: "update fields refresh recalculate table of contents toc page numbers rebuild", buttons: () => [ed.refUpdateFieldsBtn], requires: "tocField", run: () => ed.tocCommands.update.open() },
    { command: "reference.updateToc.pageNumbers", label: "Update table of contents: page numbers only", kw: "toc contents update page numbers only repaginate refresh", buttons: () => [], requires: "tocField", run: () => void ed.tocCommands.update.run("pageNumbers") },
    { command: "reference.updateToc.entire", label: "Update table of contents: entire table", kw: "toc contents update entire rebuild regenerate headings refresh", buttons: () => [], requires: "tocField", run: () => void ed.tocCommands.update.run("entire") },
    // Remove, Table of figures and Add text (`docs/153` RM-25), with their
    // labels read from the catalogue when the palette builds (`toc_commands.mjs`).
    ...tocSurfaceRows(() => ed.tocCommands),
    // Go to a note, Note settings, Convert notes (`docs/153` page.note.*): the Notes
    // group's dropdown, declared in `note_commands.mjs` so its menu and these rows
    // are one table.
    ...noteSurfaceRows(ed),
  ];

  /** The object/table right-click rows the References surface declares — see
   *  `objectMenuRows`, which is where the rule and its citations live. */
  const referenceObjectMenuRows = () =>
    objectMenuRows(REFERENCE_SURFACE, ribbonSurfaceEnabled, ribbonSurfaceReason);

  /** The O(1) state the two surface rules read. `tocEntryTargetAt` returns
   *  immediately unless the document holds a TOC field and the heading index
   *  behind it is built once per revision; the two counts are caches. Nothing
   *  here walks the document, because this runs on every keystroke. */
  function ribbonSurfaceState(entry) {
    return {
      hasDoc: !!ed.doc,
      objectSelected: !!(ed.objectSelection && ed.objectSelection.mode === "selected"),
      hasCaret: !!ed.selection,
      inRunningStory: !!ed.runningEditBand,
      staleCaptions: ed.referenceCommands.numbering.count,
      tocFields: ed.tocCommands.fields.count,
      onTocEntry:
        entry.requires === "tocEntry" && !!ed.selection && !!ed.tocEntryTargetAt(ed.selection.focus.node),
    };
  }

  const ribbonSurfaceEnabled = (entry) => surfaceEnabled(entry, ribbonSurfaceState(entry));
  const ribbonSurfaceReason = (entry) => surfaceReason(entry, ribbonSurfaceState(entry));

  /** Opens the object inspector focused on one control — Layout ▸ Wrap text lands
   *  on the wrap control, not on the panel's first field. `object_inspector.mjs`. */
  const openObjectInspectorAt = (selector) => ed.objectInspector.openAt(selector);

  /** The one enablement rule for an Insert command, shared by its ribbon button,
   *  its app-menu row, and its palette entry. `context.hasRange` lets a surface
   *  that already computed the selection state (the context menu) pass it in. */
  // ---- Review surface: one declaration per Review command ---------------------
  // Word's Review tab — Tracking, Changes, Comments. Every command below already
  // existed and was reachable ONLY from the command palette, or from buttons
  // living inside the review sidebar, which exist only while that sidebar is open.
  // So a user with a document full of tracked changes had no durable affordance
  // for accepting one. Each row names the command its buttons run and whether the
  // button is a toggle, and `review-surface.spec.mjs` asserts the ribbon's id set
  // equals the Review menu's, so a command cannot reach one surface and miss the
  // other.
  //
  // `buttons`, plural, and the same shape LAYOUT_SURFACE/REFERENCE_SURFACE
  // already use — one command can have more than one face. `review.comment` has
  // two: the Review band's button and the right-margin affordance beside the
  // selection. They are declared together on purpose. A margin button wired up on
  // its own would be a second way to open the comment composer, free to drift
  // from the first in what it runs and in when it is available, which is the
  // command-surface defect class this table exists to prevent (`105` UX-004).
  //
  // Enablement is deliberately just "a document is open": the commands
  // themselves report why nothing happened (no change at the cursor, none left to
  // accept), which is more use than a button that is silently dead.
  const reviewCommentActions = createReviewCommentActions({ runEdit: ed.runEdit, getDoc: () => ed.doc, commentId: () => ed.activeReviewCommentId, announce: (m) => ed.announceReview(m), afterChange: () => ed.drawSelection() });
  const REVIEW_SURFACE = [
    { command: "review.mode.suggesting", buttons: () => [ed.reviewTrackBtn], run: () => ed.setReviewMode(ed.reviewMode === "suggesting" ? "editing" : "suggesting"), pressed: () => ed.reviewMode === "suggesting" },
    { command: "view.showChanges", buttons: () => [ed.reviewShowChangesBtn], run: () => ed.toggleShowChanges(), pressed: () => ed.showingChanges },
    { command: "review.previous", buttons: () => [ed.reviewPrevBtn], run: () => ed.navigateReview(-1) },
    { command: "review.next", buttons: () => [ed.reviewNextBtn], run: () => ed.navigateReview(1) },
    { command: "review.acceptNext", buttons: () => [ed.reviewAcceptBtn], refusal: () => reviewDecisionRefusal("review.acceptNext", ed.HOST_CAPS), run: () => ed.decideReviewAndAdvance(true) },
    { command: "review.rejectNext", buttons: () => [ed.reviewRejectBtn], refusal: () => reviewDecisionRefusal("review.rejectNext", ed.HOST_CAPS), run: () => ed.decideReviewAndAdvance(false) },
    { command: "review.acceptAll", buttons: () => [ed.reviewAcceptAllBtn], refusal: () => reviewDecisionRefusal("review.acceptAll", ed.HOST_CAPS), run: () => void ed.decideAllReviewChanges(true) },
    { command: "review.rejectAll", buttons: () => [ed.reviewRejectAllBtn], refusal: () => reviewDecisionRefusal("review.rejectAll", ed.HOST_CAPS), run: () => void ed.decideAllReviewChanges(false) },
    // Three faces, and the third is on the INSERT band: ONLYOFFICE carries Comment
    // on both Insert and Collaboration, and Word's Insert tab has a Comments group
    // of its own. Commenting was already in this editor's Insert MENU (Google
    // Docs files it at Insert ▸ Comment, which is where `command_taxonomy.mjs`
    // took it from) but had no Insert BAND button, so in the ribbon chrome — which
    // has no menu bar (docs/122) — the Insert tab could not reach it at all.
    // Declared here rather than beside the new button for the reason the comment
    // above gives: a second wiring is free to drift in what it runs and in when it
    // is available.
    { command: "review.comment", buttons: () => [ed.reviewCommentBtn, ed.insertCommentBtn, ed.reviewMarginCommentBtn], requires: "range", reasonKey: "review.comment.needsRange", run: () => ed.openReviewComposer() },
    // `requires: "comment"` — the caret is inside a commented range. Word and
    // ONLYOFFICE both target that comment rather than a sidebar selection, so a
    // reviewer never has to open a panel to resolve what they are reading.
    { command: "review.comment.resolve", buttons: () => [ed.reviewResolveBtn], requires: "comment", run: () => void reviewCommentActions.resolve() },
    { command: "review.comment.delete", buttons: () => [ed.reviewDeleteBtn], requires: "comment", run: () => void reviewCommentActions.remove() },
    { command: "review.toggle", buttons: () => [ed.reviewPanelBtn], run: () => ed.toggleReview(), pressed: () => !ed.reviewSidebar.hidden },
    // TWO faces, one command. This table owns their DISABLED state;
    // `compare_documents.mjs` owns the clicks and the pressed states, hence
    // `ownsClick`. Listing one face only shipped the rail entry dead (`105` UX-004).
    { command: "review.compare", buttons: () => [document.getElementById("reviewCompareBtn"), document.getElementById("railCompare")].filter(Boolean), requires: "doc", reasonKey: "compare.needsDocument", ownsClick: true, run: () => ed.comparePanel.open() },
    // Proofing. Both switches were the whole content of a `Tools` menu, which is
    // one top-level name for two toggles — and one of the two names that scrolled
    // off the end of the menu bar (`109` HF-097). Word's Review tab opens with a
    // Proofing group, so that is where they go now that the ribbon is the only
    // navigation axis in this chrome. `requires: "always"`: both are `noDoc`
    // commands — they are preferences, and switching one with no document open is
    // meaningful and already supported.
    { command: "tools.spellCheck", buttons: () => [ed.reviewSpellCheckBtn], requires: "always", pressed: () => ed.settings.spellCheck !== false, run: () => ed.setSpellCheckEnabled(ed.settings.spellCheck === false) },
    { command: "tools.grammarCheck", buttons: () => [ed.reviewGrammarCheckBtn], requires: "always", pressed: () => ed.settings.grammarCheck !== false, run: () => ed.setGrammarCheckEnabled(ed.settings.grammarCheck === false) },
    { command: "tools.smartQuotes", buttons: () => [ed.reviewSmartQuotesBtn], requires: "always", pressed: () => ed.autoCorrect.enabled("smartQuotes"), run: () => ed.autoCorrect.toggle("smartQuotes") },
    // Word count (HF-051): a document fact, so it needs a document and nothing else.
    { command: "tools.wordCount", buttons: () => [document.getElementById("reviewWordCountBtn")].filter(Boolean), requires: "doc", reasonKey: "command.needsDocument", run: () => ed.wordCount.open() },
    { command: "tools.languages", buttons: () => [ed.reviewProofLanguagesBtn], requires: "always", run: () => ed.proofing.openLanguages() },
    // Restrict Editing (ADR-052, ADR-059). Declared HERE rather than beside the
    // button for the reason this table gives twice over: one owner for `disabled`,
    // for the reason it carries while disabled, and for the pressed state. The
    // module owns the dialog and the engine call and nothing about the button, so
    // the two cannot disagree about whether a restriction is in force.
    //
    // `pressed` reads the ENGINE, not what the dialog last asked for — ONLYOFFICE's
    // Protect button is a state too — so a document that arrives protected shows the
    // button pressed before anyone opens anything.
    { command: "review.restrictEditing", buttons: () => [document.getElementById("reviewProtectBtn")].filter(Boolean), requires: "doc", reasonKey: "command.needsDocument", pressed: () => ed.documentProtection.isActive(), run: () => ed.documentProtection.open() },
    // Manage access. `requires: "doc"` because there is nothing to manage access TO
    // without one; whether the button is PRESENT at all is `accessChrome.reflect`'s
    // answer and not this table's, because absence is a permission decision and
    // this table only knows about enablement. Same group as Restrict Editing, which
    // is Word's own grouping of the two authority questions.
    { command: "review.manageAccess", buttons: () => [document.getElementById("reviewManageAccessBtn")].filter(Boolean), requires: "doc", reasonKey: "command.needsDocument", run: () => ed.accessChrome.open() },
  ];

  function insertCommandEnabled(commandId, context = {}) {
    const entry = INSERT_SURFACE.find((candidate) => candidate.command === commandId);
    if (!entry || !ed.doc) return false;
    if (entry.requires === "range") return !!(context.hasRange ?? ed.hasRange());
    return true;
  }

  /** Re-raster a single page after an edit — the incremental repaint that keeps
   *  editing latency to one page, not the whole document. If the page is on screen
   *  it is re-rendered in place; if it is virtualized off-screen, its stale canvas
   *  is dropped so it re-renders fresh (from current model state) when scrolled in. */
  return {
    BREAK_IO, INSERT_SURFACE, LAYOUT_SURFACE, REFERENCE_SURFACE, REVIEW_SURFACE,
    insertCommandEnabled, referenceObjectMenuRows, reviewCommentActions, ribbonSurfaceEnabled,
    ribbonSurfaceReason,
  };
}
