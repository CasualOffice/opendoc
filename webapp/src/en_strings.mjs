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
});
