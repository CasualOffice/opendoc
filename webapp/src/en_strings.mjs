// The English a script-side string is written down in, once (docs/124 §3.1).
//
// Markup declares its own English beside its `data-i18n` attribute, where it
// is readable in place. A script has nowhere to put it, so it goes here: one
// file a translator's tooling reads, one file `build-locale.mjs` merges into
// `locales/en.json`, and one place a reviewer can see every sentence the
// editor can say without reading 17,000 lines to find them.
//
// A `t("key")` whose key is absent from here fails the build. That is what
// stops a call site from inventing a key no translator will ever be shown.
//
// PLURALS are families, not strings: `.one`, `.other`, and whatever else a
// language needs. English declares the two it has; a Russian catalogue adds
// `.few` and `.many` without any call site changing, which is the entire
// reason plural selection lives in the seam (`i18n.mjs`).
export const EN_STRINGS = Object.freeze({
  // The one sentence white-labelling needs (docs/126 phase 3). A deployment whose
  // `brand.json` pinned the accent owns it, so the Settings colour controls are
  // disabled WITH A REASON rather than removed — a control inside a surface the
  // visitor was offered, which is where "never a dead control" applies. Word greys
  // a policy-managed setting and says who set it; a control that vanishes cannot
  // be told from a bug.
  "branding.themeSetByHost": "The colour is set by the site that provides this editor.",
  "status.words.one": "{count} word",
  "status.words.other": "{count} words",
  "status.characters.one": "{count} character",
  "status.characters.other": "{count} characters",
  "status.paragraphs.one": "{count} paragraph",
  "status.paragraphs.other": "{count} paragraphs",
  "status.characters.withSpaces": "{count} characters (with spaces)",
  // Reassigned from script on every render, so the markup sweep never sees it.
  "status.pageOf": "Page {page} of {total}",
  "toolbar.undo": "Undo",
  "toolbar.redo": "Redo",
  // The engine names the action a step will undo ("Typing", "Table structure").
  // A language that puts the verb after the object needs the whole sentence,
  // not "Undo" plus a noun glued on, which is why this is one key and not two.
  "toolbar.undoNamed": "Undo {name}",
  "toolbar.redoNamed": "Redo {name}",
  // The document-state pill. The words live in `status_policy.mjs`, which is
  // DOM-free and knows nothing of catalogues; these are the same strings, keyed.
  "status.state.opened": "Opened",
  "status.state.edited": "Edited",
  "status.state.downloaded": "Downloaded",
  "table.styleNamed": "Table style: {name}",
  "status.characters.noSpaces": "{count} characters (no spaces)",
  "settings.language.systemDefault": "System default ({name})",
  // Page Setup builds two of its strings rather than declaring them in markup:
  // the Section dropdown's options are one per section, and the preview's label
  // reads out whatever the size fields currently say.
  "pageSetup.sectionNumber": "Section {number}",
  "pageSetup.dimensions": "{width} \u00d7 {height} in",
  // The object bar's hint while a crop session is live. It is its own key rather
  // than a reuse of the resize hint because it names a different gesture: the
  // grips under the pointer are crop grips, and the bar that said "Drag handles
  // to resize" over them was describing the mode it had just left.
  "object.cropHint": "Drag the edges to crop",
  // The two running-content variants. Their status lines and their palette rows
  // read as SWITCHES ("Different first page: on"), and a language that puts the
  // state before the name needs the whole sentence rather than a label with "on"
  // glued to it \u2014 which is why each state is its own key and not one key plus a
  // word. They moved out of `main.js` with `header_footer_settings.mjs`, which is
  // where the same two commands' checkbox faces live.
  "headerFooter.firstPageOn": "Different first page on",
  "headerFooter.firstPageOff": "Different first page off",
  "headerFooter.evenOddOn": "Different odd & even pages on",
  "headerFooter.evenOddOff": "Different odd & even pages off",
  "headerFooter.firstPageSwitch": "Different first page: {state}",
  "headerFooter.evenOddSwitch": "Different odd & even pages: {state}",
  "headerFooter.stateOn": "on",
  "headerFooter.stateOff": "off",
  "headerFooter.changeFailed": "Could not change the page setup: {message}",
  // Word's own word for this group: the Header & Footer tab's OPTIONS group.
  // It was "settings", which is the word the Settings command owns — typing
  // "settings" in the palette then put this row ABOVE Settings itself, which is
  // a worse answer to that query than the one it displaced.
  "headerFooter.settingsCommand": "Header and footer options\u2026",
  // The Watermark dialog's Font list is built from the editor's font inventory,
  // so only its first entry is a word: the one that means "whatever face the
  // document already uses", which is what an absent `w:rFonts` resolves to.
  "watermark.fontDefault": "Document default",
  "dropCap.applied": "Drop cap applied",
  "dropCap.command": "Drop cap…",
  "dropCap.readError": "Drop cap settings could not be read",
  "dropCap.removed": "Drop cap removed",
  // The File page. Every row name and description the page renders was an
  // English literal in `file_pane.mjs`, so the one full-window surface in the
  // product stayed in English in all eighteen languages — the rest of the
  // chrome localises from `data-i18n`, but this page builds its rows in script.
  "filePane.export.label": "Export",
  "filePane.settings.label": "Settings",
  "filePane.settings.blurb": "Appearance, your reviewer identity, autosave and proofing.",
  "filePane.properties.label": "Document properties",
  "filePane.properties.blurb": "Title, author and the other metadata saved with the file.",
  "filePane.pageSetup.label": "Page setup",
  "filePane.pageSetup.blurb": "Size, orientation, margins and columns for this section.",
  "filePane.shortcuts.label": "Keyboard shortcuts",
  "filePane.shortcuts.blurb": "Every command that has one.",
  "filePane.about.label": "About OpenDoc",
  "filePane.about.blurb": "Version, licence and where the source lives.",
  "filePane.new.label": "New document",
  "filePane.commands.label": "Find a command",
  "filePane.commands.blurb": "Search everything the editor can do.",
  // Resolve / Delete comment, which act on the comment the caret is inside.
  "reviewComment.resolve": "Resolve comment",
  "reviewComment.delete": "Delete comment",
  "reviewComment.needsCaret": "Put the caret in a comment first",
  // Why New/Open are unavailable when the editor is framed by a host
  // application. A reason, not a silent absence — the UI floor forbids a
  // control that is simply missing with no explanation.
  "capability.embedded": "This editor is embedded in another application",
  "capability.notGranted": "The host has not granted this",
  // The Outline panel's empty state. It moved into the catalogue when the panel's
  // rows moved to `outline_panel.mjs`: the module takes its vocabulary as input,
  // so this is the one place the sentence is written down.
  "outline.noHeadings": "No headings yet. Apply a Heading style to build an outline.",
  // References ▸ Insert caption (OO-005). The dialog's own labels are in the
  // markup beside their English; these are the ones a script composes.
  "captionDialog.headingLevel": "Heading {level}",
  "captionDialog.labelNameRequired": "Type a name for the new label.",
  "captionDialog.labelNotRemovable":
    "That label is either built in or already used by a caption in this document.",
  "caption.inserted": "Caption inserted.",
  // References ▸ Cross-reference. The reference TYPES and the "For which …"
  // heading are script-built because the type list includes one row per caption
  // label the document uses, which is document data rather than chrome.
  "crossRefDialog.type.heading": "Heading",
  "crossRefDialog.type.bookmark": "Bookmark",
  "crossRefDialog.type.footnote": "Footnote",
  "crossRefDialog.type.endnote": "Endnote",
  "crossRefDialog.forWhich.caption": "For which caption",
  "crossRefDialog.forWhich.heading": "For which heading",
  "crossRefDialog.forWhich.bookmark": "For which bookmark",
  "crossRefDialog.forWhich.footnote": "For which footnote",
  "crossRefDialog.forWhich.endnote": "For which endnote",
  // "Insert reference to". The SAME underlying reference is worded differently
  // per type, exactly as Word words it — "Heading number" and "Paragraph number"
  // are one engine value seen from two reference types.
  "crossRefDialog.refTo.entireCaption": "Entire caption",
  "crossRefDialog.refTo.labelAndNumber": "Only label and number",
  "crossRefDialog.refTo.captionText": "Only caption text",
  "crossRefDialog.refTo.pageNumber": "Page number",
  "crossRefDialog.refTo.aboveBelow": "Above/below",
  "crossRefDialog.refTo.headingText": "Heading text",
  "crossRefDialog.refTo.headingNumber": "Heading number",
  "crossRefDialog.refTo.headingNumberNoContext": "Heading number (no context)",
  "crossRefDialog.refTo.headingNumberFullContext": "Heading number (full context)",
  "crossRefDialog.refTo.bookmarkText": "Bookmark text",
  "crossRefDialog.refTo.paragraphNumber": "Paragraph number",
  "crossRefDialog.refTo.paragraphNumberNoContext": "Paragraph number (no context)",
  "crossRefDialog.refTo.paragraphNumberFullContext": "Paragraph number (full context)",
  "crossRefDialog.refTo.footnoteNumber": "Footnote number",
  "crossRefDialog.refTo.endnoteNumber": "Endnote number",
  // Why "Include above/below" is greyed. Two different reasons, because "Word
  // does not offer it here" and "the engine cannot do it yet" are not the same
  // answer and a reader can act on one of them.
  "crossRefDialog.aboveBelowNotForThis":
    "Word does not offer above/below for this kind of reference",
  "crossRefDialog.aboveBelowUnavailable":
    "Including above/below needs an argument the engine’s insert operation does not take yet",
  "crossReference.inserted": "Cross-reference inserted.",
  // References ▸ Update caption numbers, and the one place Insert caption refuses.
  // Both are DISABLED-STATE reasons, which is why they are sentences a reader can
  // act on rather than error text.
  "caption.numbersAlreadyRight": "Every caption already shows the right number",
  "caption.bodyOnly": "A caption can only go in the document body, not in a header, footer or note",
  "caption.numbersUpdated": "Caption numbers updated.",
  // What a structured paste says when it carried the content but not every
  // reference. Five families cannot be duplicated inside one document — a
  // bookmark name is unique, one comment has one anchored range, Word duplicates
  // a footnote rather than re-pointing at it, a duplicate field-range marker is
  // invalid, and one tracked move has one destination. `paste_loss.mjs` names
  // them; `casual-doc-edit/src/clone.rs` records why each one cannot come across.
  //
  // One sentence with an `{items}` list rather than five sentences: which families
  // a given paste degraded is not knowable in advance, and over-reporting is as
  // false as under-reporting.
  "paste.loss": "Pasted, without {items} — those cannot be duplicated inside one document.",
  "paste.loss.bookmark": "bookmarks",
  "paste.loss.comment": "comments",
  "paste.loss.note": "footnote references",
  "paste.loss.fieldRange": "field codes",
  "paste.loss.trackedMove": "tracked moves",

  // ── Version history (docs/139, docs/140, ADR-038/ADR-039) ──────────────────
  //
  // `version_history.mjs` returns status CODES and no English at all, because a
  // storage layer that hard-codes sentences cannot be localised. This is the
  // other half of that promise: one sentence per code, and the mapping lives in
  // `version_policy.mjs` as literal `t()` calls so `build-locale.mjs` can see
  // them.
  //
  // Every refusal below names the WAY OUT, because that is the whole reason the
  // store distinguishes them. A quota-exhausted browser and a budget wedged by
  // named versions are both "no new version was kept", and the answers are
  // "free some space" and "unname one" — a single generic sentence would send
  // half the readers to the wrong place.
  "history.status.recorded": "Version saved to this document’s history.",
  "history.status.notDue": "No new version yet — versions are kept at intervals, not on every edit.",
  "history.status.unchanged": "Nothing has changed since the last version.",
  "history.status.pruned": "Older versions were released to stay inside the retention policy.",
  "history.status.restorePrepared":
    "Ready to restore — the document on screen has been kept as a version first.",
  "history.status.restoreCommitted":
    "Restored. The version you replaced is still in this document’s history.",
  "history.status.fullPinned":
    "Version history is full and every version left is named, so none can be released. Stop keeping one to make room.",
  "history.status.overBudget":
    "This document is larger than all the space version history is allowed. Raise the storage budget in Settings to keep versions of it.",
  "history.status.quotaExhausted":
    "This browser is out of storage, so the version was not kept. Free some space, or save the document to a file.",
  "history.status.storeUnavailable":
    "This browser would not open local storage, so no versions are being kept.",
  "history.status.evicted":
    "This browser cleared its local storage, so the versions kept for this document are gone.",
  "history.status.staleHead":
    "Another tab changed this document’s history. Close version history and open it again to see where it is now.",
  "history.status.missingCheckpoint":
    "That version’s contents are no longer stored, so it cannot be opened or restored.",
  "history.status.corruptCheckpoint":
    "That version’s contents are damaged and were not opened. The rest of the timeline is unaffected.",
  "history.status.nameRejected": "Give the version a name of between 1 and 120 characters.",
  "history.status.pinLimit":
    "As many versions are being kept as the policy allows. Stop keeping one before naming another.",
  "history.status.unknownVersion": "That version is no longer in this document’s history.",
  "history.status.unknownOperation": "That restore is no longer in progress. Start it again from the timeline.",
  // The fallback, and a REFUSAL rather than a confirmation: the cost of an
  // unnecessary toast is small and the cost of silence about somebody's lost
  // work is not.
  "history.status.unknownFailure": "That version could not be saved, and the reason is not known.",

  // Why a version exists. The words are the reader's, not the store's: the kind
  // is `import` and the sentence is "Opened".
  "versionHistory.kind.import": "Opened",
  "versionHistory.kind.saved": "Saved",
  "versionHistory.kind.named": "Named",
  "versionHistory.kind.auto": "Autosaved",
  "versionHistory.kind.manual": "Version created",
  "versionHistory.kind.preRestore": "Before a restore",
  "versionHistory.kind.restore": "Restored",
  "versionHistory.kind.recovery": "Recovered",

  // The two relative day headings everybody recognises, and no others: "3 days
  // ago" as a group heading makes a timeline harder to read, and docs/139 §14
  // forbids a relative label standing in for the exact date.
  "versionHistory.day.today": "Today",
  "versionHistory.day.yesterday": "Yesterday",

  "versionHistory.command": "Version history",
  "versionHistory.current": "Current version",

  // The row menu: the five actions a single version can have done to it, and
  // the name of the ⋮ that opens them.
  //
  // They were five buttons in `editor.html` until the actions moved onto the row
  // they act on, so the KEYS are unchanged and every catalogue already answers
  // them — what moved is where the English is written down, which is here for a
  // string a script builds. The ⋮'s name carries the version's own timestamp,
  // because "More actions" repeated once per row names nothing.
  "versionHistory.rowActions": "Actions for the version from {when}",
  "versionPanel.restoreThisVersion": "Restore this version",
  "versionPanel.nameThisVersion": "Name this version…",
  "versionPanel.keepThisVersion": "Keep this version",
  "versionPanel.showChanges": "Show changes",
  "versionPanel.deleteThisVersion": "Delete this version",
  "versionPanel.actionsSelectedVersion.label": "Actions for the selected version",
  "versionHistory.isNamed": "Named",
  "versionHistory.empty":
    "No versions yet. Versions are kept as you edit, and whenever you save.",
  "versionHistory.emptyNamed": "No named versions. Name a version to find it here later.",
  "versionHistory.needsSelection": "Select a version in the list first",
  "versionHistory.headNotRestorable": "This is the current version — there is nothing to restore",
  "versionHistory.headNotDeletable":
    "This is the current version — it is the only one that still describes the document",
  // Present, disabled, and honest. Comparing two versions is docs/140's H3 and
  // is not built; a button that silently did nothing would be worse than this.
  "versionHistory.action.showChangesUnavailable":
    "Comparing one version with another is not built yet",

  // The counts and the policy, under the list. A person who cannot see the bound
  // cannot trust the promise (docs/139 §12).
  "versionHistory.kept.one": "{count} version kept",
  "versionHistory.kept.other": "{count} versions kept",
  "versionHistory.footerDetail": "{size} in this browser · {named} of {limit} named",
  "versionHistory.retention":
    "Versions are kept for at least {days} days, up to {count} of them. Named versions are kept until you delete them.",

  // Why the entry point is disabled. Five different reasons, because the way out
  // of each is different and a reader can only act on the specific one.
  "versionHistory.disabled.noDocument": "Open a document to see its version history",
  "versionHistory.disabled.setting": "Version history is off. Turn it on in Settings.",
  "versionHistory.disabled.autosave":
    "Version history follows autosave, which is off. Turn autosave on in Settings.",
  "versionHistory.disabled.embedded":
    "Version history is off in an embedded editor — the page that embeds it owns storage.",
  "versionHistory.disabled.noStore": "This browser refused local storage ({message})",

  // Preview. The banner names WHEN, because a read-only canvas with no date on
  // it is indistinguishable from a locked document.
  "versionHistory.preview.banner": "Previewing the version from {when} — read-only.",
  "versionHistory.preview.readOnly":
    "You are looking at an earlier version; go back to current to change the document",
  "versionHistory.preview.failed": "That version could not be opened: {message}",

  // Restore. The confirmation is where the reader is TOLD that restoring is not
  // destructive, which is why it is a sentence and not a warning glyph.
  "versionHistory.restore.title": "Restore this version?",
  "versionHistory.restore.message":
    "“{name}” replaces what is on screen. Nothing is lost: the document you have now is kept as a version of its own first, and every version after this one stays in the timeline.",
  "versionHistory.restore.confirm": "Restore",
  "versionHistory.restore.cancel": "Keep current",
  "versionHistory.restore.note": "The restored document is unsaved until you save it to a file.",
  "versionHistory.restore.cannotKeepCurrent":
    "The document on screen could not be kept as a version ({message}), so it was not replaced.",
  "versionHistory.restore.failed": "That version could not be restored: {message}",
  "versionHistory.restored":
    "Restored the version from {when}. The document you replaced is still in the timeline.",

  "versionHistory.named": "Named this version “{name}”.",
  "versionHistory.pinned": "This version is now kept until you say otherwise.",
  "versionHistory.unpinned": "This version can now be released by the retention policy.",
  "versionHistory.deleted": "Deleted that version.",

  "versionHistory.delete.title": "Delete this version?",
  "versionHistory.delete.message":
    "The contents of “{name}” are deleted from this browser. This cannot be undone.",
  "versionHistory.delete.confirm": "Delete version",
  "versionHistory.delete.cancel": "Keep it",

  "versionHistory.clear.title": "Delete every version of this document?",
  "versionHistory.clear.message":
    "The whole timeline for this document is deleted from this browser, named versions included. This cannot be undone.",
  "versionHistory.clear.confirm": "Delete history",
  "versionHistory.clear.cancel": "Keep the history",
  "versionHistory.clear.done.one": "Deleted {count} version and freed {size}.",
  "versionHistory.clear.done.other": "Deleted {count} versions and freed {size}.",

  // ---- Tables (docs/141) ---------------------------------------------------
  // The four reasons a Table-band control can be unavailable. They were already
  // the MENU's sentences, written as literals in `tableToolCommands`, while the
  // band kept its authored tooltip and explained nothing (TBL-03). One source
  // for one sentence, so band and menu cannot drift into disagreeing about the
  // same precondition.
  "table.reason.caretOutsideTable": "Place the caret in a table",
  "table.reason.merged": "Unavailable for merged or spanned tables",
  "table.reason.rowHeights": "Rows need a fixed or minimum height before distribution",
  "table.reason.mergeSelection": "Select a row, column, or table before merging",
  // Unmerge is offered whenever the table HAS a merge somewhere, because
  // `TableInfo` reports no per-cell merge state (TBL-20) — `regular` is a
  // whole-table boolean. A table with no merge at all can therefore be refused
  // before the engine is asked.
  "table.reason.noMergedCells": "This table has no merged cells",
  // Tab in the last cell appends a row, the way Word and Google Docs do. The
  // caret lands in the new row's first cell, which is a jump worth announcing:
  // the live region is the only channel a reader who cannot see it has.
  "table.rowAppended": "Row added at the end of the table",
  "table.atFirstCell": "The caret is already in the first cell of the table",
  // INTERIM (TBL-08). Cell shading, vertical alignment and the cell-border
  // presets act on the caret's cell alone, so with a row or column selected the
  // gesture formatted one cell and nothing said so. Refusing is the honest
  // interim until a cell range can be formatted as one.
  "table.cellFormatOneCell":
    "Shading, alignment and cell borders apply to one cell — put the caret in the cell to format it",
  // The row/column/table selection's own status line. It was built as
  // `Selected table ${mode}`, the one table status line that was not localised
  // at all, and glueing a translated noun onto a fixed verb is what these three
  // keys exist to avoid.
  "table.selectedRow": "Row selected",
  "table.selectedColumn": "Column selected",
  "table.selectedTable": "Table selected",
});
