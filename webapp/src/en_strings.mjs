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
});
