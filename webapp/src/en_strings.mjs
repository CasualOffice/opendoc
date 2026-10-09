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
  // ---- Menu band names ------------------------------------------------------
  //
  // Every band of every menu, and every band of the File page, names itself
  // from here. A band is a `role="group"` with this string as its accessible
  // name, and on the File page it is also the visible heading — so these are
  // read aloud, not decoration, and an English literal in `command_taxonomy.mjs`
  // would have been an untranslated string a screen reader says in every locale.
  //
  // Where the ribbon already had a group of the same name the ENGLISH IS THE
  // SAME SENTENCE and each locale's value was copied from the catalogue entry
  // the ribbon's own `rgroup-label` uses (`panelInsert.illustrations`,
  // `panelReview.proofing`, and so on) rather than translated a second time.
  // One vocabulary across the ribbon, the menus and the File page; two words
  // for one band is how a surface starts reading as a different product.
  "menuGroup.allChanges": "All changes",
  "menuGroup.alignment": "Alignment",
  "menuGroup.captions": "Captions",
  "menuGroup.cellSize": "Cell size",
  "menuGroup.changeCase": "Change case",
  "menuGroup.changes": "Changes",
  "menuGroup.clearFormatting": "Clear formatting",
  "menuGroup.clipboard": "Clipboard",
  "menuGroup.comments": "Comments",
  // Word's Review tab reads Proofing | Comments | Tracking | Changes | Compare,
  // and Compare is its own rightmost group there.
  "menuGroup.compare": "Compare",
  "menuGroup.delete": "Delete",
  "menuGroup.document": "Document",
  "menuGroup.fields": "Fields",
  "menuGroup.find": "Find and replace",
  "menuGroup.font": "Font",
  "menuGroup.formattingMarks": "Formatting marks",
  "menuGroup.headerFooter": "Header & footer",
  "menuGroup.headings": "Document headings",
  "menuGroup.help": "Help",
  "menuGroup.history": "History",
  "menuGroup.illustrations": "Illustrations",
  "menuGroup.links": "Links",
  "menuGroup.lists": "Lists",
  "menuGroup.merge": "Merge",
  "menuGroup.mode": "Mode",
  "menuGroup.move": "Move",
  "menuGroup.navigation": "Navigation",
  "menuGroup.newAndOpen": "New and open",
  "menuGroup.notes": "Notes",
  "menuGroup.paragraph": "Paragraph",
  "menuGroup.print": "Print",
  "menuGroup.proofing": "Proofing",
  "menuGroup.properties": "Properties",
  "menuGroup.protect": "Protect",
  "menuGroup.rowsAndColumns": "Rows & columns",
  "menuGroup.save": "Save",
  "menuGroup.select": "Select",
  "menuGroup.selection": "Selection",
  "menuGroup.settings": "Settings",
  "menuGroup.show": "Show",
  "menuGroup.sort": "Sort",
  "menuGroup.style": "Style",
  "menuGroup.styles": "Styles",
  "menuGroup.symbols": "Symbols",
  "menuGroup.table": "Table",
  "menuGroup.text": "Text",
  "menuGroup.textColor": "Text color",
  "menuGroup.textWidth": "Text width",
  "menuGroup.tracking": "Tracking",
  "menuGroup.undo": "Undo",
  "menuGroup.zoom": "Zoom",
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
  // -- Reflow (pageless) ------------------------------------------------------
  // `docs/151` §6, ADR-046. Named "Reflow" and not "Reader mode", "Pageless" or
  // "Web Layout" — the three references' own words — because ours is the only
  // one of the four that is accurate for a view that is still EDITABLE and still
  // cut into tiles. "Reader mode" in particular would promise ONLYOFFICE's
  // read-only behaviour, which §3.2 rejects on purpose.
  //
  // The two command labels are whole sentences rather than a stem and a glued-on
  // "on"/"off": a language that puts the state before the noun, or inflects the
  // noun for it, cannot be served by concatenation. Same reason the spacing rows
  // above are two keys.
  "reflow.commandOn": "Reflow: on",
  "reflow.commandOff": "Reflow: off",
  "reflow.on": "Reflow is on: the document is laid out to the window rather than on pages.",
  "reflow.off": "Reflow is off: the document is laid out on its own pages again.",
  "reflow.withholds": "There are no pages in reflow, so the ruler and the Pages panel are unavailable.",
  "reflow.pagesWithheld": "The Pages panel shows pages, and there are none in reflow. Turn Reflow off to use it.",
  "reflow.rulerWithheld": "The ruler measures a page and its margins, and there are none in reflow. Tab stops and indents are on the Layout tab.",
  // The engine refuses reflow on a document it is showing one page-window at a
  // time, because reflow's promise is that the document stays editable in it and
  // a windowed body is already read-only. Disabled WITH this, never dead.
  "reflow.unavailable":
    "This document is too large to lay out whole, so it is shown one page-window at a time and cannot be reflowed.",
  // What the view is approximating, on demand (`docs/166` R-7). The command is
  // enabled in BOTH views, because "nothing" is an answer and a greyed row is
  // not: a reader asking what the page view is withholding has earned being told
  // that it is withholding nothing.
  //
  // The individual sentences are NOT here. They come from the engine
  // (`LayoutView::approximations`), which has no catalogue, so they are English
  // in every locale — a pre-existing gap recorded rather than papered over. Only
  // the label and the empty answer are routed.
  "reflowNotes.command": "What this view approximates",
  "reflowNotes.none":
    "Nothing: every part of this document is laid out the way the document asks for it.",
  // -- Text width (`docs/154` §5.1, ADR-048, `docs/151` §6.2a) ---------------
  // Five steps for one measure, each labelled by what it does rather than by a
  // number: "80" tells a reader nothing until they know it is WCAG 2.1 SC
  // 1.4.8's maximum, and "Wide" nothing until they know it is the page without
  // its margins, which is what the `.title` sentences are for. The `.row`
  // sentences and `textWidth.wide.short` (the default's label, on the button)
  // are declared in `editor.html` beside the markup that carries them —
  // `build-locale.mjs` refuses a key declared in both places — and
  // `reflow_view.test.mjs` asserts every step has all four, so the split cannot
  // rot into a half-labelled step.
  "textWidth.command": "Text width",
  "textWidth.narrow.short": "Narrow",
  "textWidth.reading.short": "Reading",
  "textWidth.fit.short": "Paper",
  "textWidth.full.short": "Full",
  "textWidth.narrow.title": "Narrow text: about 55 characters a line.",
  "textWidth.reading.title":
    "Reading width: 80 characters a line, which is the widest WCAG 2.1 SC 1.4.8 allows a block of text to be.",
  "textWidth.fit.title": "As wide as this document's own text column, so no line is longer than on paper.",
  "textWidth.wide.title":
    "As wide as this document's page, edge to edge — the page with its margins taken away, which is what pageless means.",
  "textWidth.full.title": "As wide as the window, however wide the window is.",
  "textWidth.narrow.command": "Text width: Narrow",
  "textWidth.reading.command": "Text width: Reading",
  "textWidth.fit.command": "Text width: Paper",
  "textWidth.wide.command": "Text width: Wide",
  "textWidth.full.command": "Text width: Full",
  "textWidth.pagedWithheld":
    "Text width applies in reflow. On pages the measure is the document's own, so turn Reflow on to choose one.",
  // -- Folding (ADR-049, `docs/157`) -----------------------------------------
  //
  // ONLYOFFICE has no folding at all — zero `collaps` hits across their 232-file
  // Word engine, and their exported outline API has no Collapse or Expand — so
  // every sentence here is written from Word rather than matched against theirs.
  //
  // The two toggle labels are whole verbs rather than one label plus a state,
  // for the reason the reflow pair above are two keys: a language that inflects
  // the noun for the action cannot be served by concatenation.
  "fold.collapseHeading": "Collapse heading",
  "fold.expandHeading": "Expand heading",
  "fold.collapseAll": "Collapse all headings",
  "fold.expandAll": "Expand all headings",
  "fold.treeLabel": "Document headings",
  // Two withholdings, two sentences. One shared "folding is unavailable" string
  // would tell a reader who pressed the shortcut inside a paragraph about a
  // document-level limitation that does not apply to them.
  "fold.noHeadingAtCaret":
    "The cursor is not in a heading. Folding collapses a heading and the content under it, so put the cursor in one first.",
  "fold.noDocument": "Open a document to fold its headings.",
  "fold.unavailable": "This document's headings cannot be folded.",
  // The level picker. "All levels" is the off position and folds nothing; level
  // 1 folds every heading, which is why the two are one mechanism.
  "fold.level.all": "Show all levels",
  "fold.level.n": "Show level {level}",
  // WORD SAYS THIS AND SO MUST WE. Collapsed content occupies no pages, so the
  // number on screen is lower than the number that prints. Print, PDF and DOCX
  // export are always fully expanded (ADR-049), so the printed numbers are the
  // true ones — and a page indicator that quietly means something else while
  // folded is the same class of lie as printing a tile index as a page number,
  // which `151` §6.5 refuses.
  "fold.pageCountNotPrinted.one":
    "One heading is collapsed, so the page count on screen is not the printed one. Printing and export always include collapsed content.",
  "fold.pageCountNotPrinted.other":
    "{count} headings are collapsed, so the page count on screen is not the printed one. Printing and export always include collapsed content.",
  // Reflow cuts the document into tiles, not pages, so "Page 3 of 12" would be
  // wrong in both halves (`docs/151` §6.5). How far through the reader is, is a
  // question the tile index can answer honestly.
  "status.readingPosition": "{percent}% through",
  // -- Line & paragraph spacing ---------------------------------------------
  // The verb on each one-gesture row is chosen from the caret paragraph's
  // current state, so the two spellings are two keys rather than one key and a
  // glued-on word: a language that inflects the noun after "add" and after
  // "remove" differently cannot be served by concatenation.
  "spacing.addSpaceBefore": "Add space before paragraph",
  "spacing.removeSpaceBefore": "Remove space before paragraph",
  "spacing.addSpaceAfter": "Add space after paragraph",
  "spacing.removeSpaceAfter": "Remove space after paragraph",
  // Shown under the line-spacing box when the number in it was INHERITED.
  // `paragraphSpacing` is style-resolved, so the box holds the effective value
  // and the preset above it is ticked; what this sentence adds is where the
  // value came from, which is the difference between "1.5" and "1.5, and
  // clearing it here would change nothing".
  "spacing.fromStyleHint": "This paragraph takes its line spacing from its style. Type a value to set it here.",
  // The other empty-box case, and a different fact: nothing anywhere in the
  // cascade sets line spacing, so there is no number to show and the layout
  // default is what the page is drawn with. Ticking "Single" here would be a
  // claim the document does not make.
  "spacing.lineUnset": "No line spacing is set anywhere in this paragraph's styles; the document default applies.",
  // The space-before/after twin of `fromStyleHint`. Its own sentence rather than
  // a reuse: "line spacing" is the wrong noun for a gap above a paragraph, and a
  // language that inflects the two differently cannot be served by one string.
  "spacing.spaceFromStyleHint": "This space comes from the paragraph style. Type a value to set it here.",
  // -- Tab stops (`docs/148` §9 item 7) --------------------------------------
  // The dialog's own labels are markup, where their English sits beside the key
  // in `editor.html`; what is here is what a SCRIPT composes — the command's
  // label, the list row, and the four sentences the dialog says when it refuses.
  "tabStops.command": "Tab stops…",
  // The list row. One string rather than a number with a word glued after it:
  // the two swap order in several languages, the unit belongs inside the
  // sentence a translator is shown, and an RTL catalogue can put them where its
  // readers expect without any call site changing.
  "tabStops.rowLabel": "{position} in — {align}",
  // Three refusals, three sentences, because they ask for three different
  // things. `inchesToTwips` answers 0 for a blank box AND for "abc"; a dialog
  // that silently placed a stop at the margin for either would be the "clamped
  // to something the user did not ask for" failure this dialog was told to
  // avoid.
  "tabStops.needPosition": "Type a position first.",
  // The example keeps a DOT in every language on purpose. `parsePosition` runs
  // on `Number()`, which accepts `1.5` and not `1,5`, so a French or German
  // catalogue writing `1,5` here would print an example the field then refuses —
  // a translation that makes the product wrong. The field shares that limit with
  // every other measurement box in the editor; localising the separator is a
  // change to `units.mjs`, not to a catalogue.
  "tabStops.badPosition": "That is not a position. Type a number of inches, like 1.5.",
  "tabStops.outOfRange": "A tab stop has to be between 0 and 22 inches from the margin.",
  // The two disabled buttons' reasons. A disabled control's one channel is its
  // title, and it must say what to DO next (`spacing.*` above records the same
  // rule).
  "tabStops.selectStop": "Choose a tab stop in the list first.",
  "tabStops.noneToClear": "These paragraphs have no tab stops.",
  // Following a contents entry whose field carried no `\\h`, so there was no
  // hyperlink to follow and the entry did nothing at all.
  // Generating and updating a table of contents. The three announcements are
  // separate keys rather than one with a verb substituted: "updated" and "rebuilt"
  // are different promises, and a language that inflects them differently cannot
  // be served by gluing a word on.
  "toc.inserted": "Table of contents inserted",
  "toc.pageNumbersUpdated": "Page numbers updated",
  "toc.rebuilt": "Table of contents rebuilt from the headings",
  // Only when the document holds MORE than one contents field, where the caret
  // is the only thing that can say which one was meant.
  "toc.whichTable": "Put the caret in the table of contents you want to update",
  "toc.noneToUpdate": "This document has no generated table of contents to update",
  "toc.jumpedTo": "Jumped to {heading}",
  "toc.notAnEntry": "Put the caret on a table-of-contents entry first",
  "paragraph.caretRequired": "Place the caret in a paragraph",
  // Why Restart / Continue numbering and Insert ▸ Link are unavailable. They were
  // the palette rows' English literals while the buttons for the same commands
  // greyed out saying nothing (`control_reasons.mjs`); one entry each now, read
  // by both surfaces.
  "list.reason.notNumbered": "Place the caret in a numbered list",
  "list.reason.nothingToContinue": "There is no earlier numbered list to continue",
  "insert.reason.linkNeedsText": "Select text to add a link",
  "insert.chart": "Chart",
  // A disabled control has one channel — its title — and it must say what to
  // DO, not repeat what the control would have done. This one used to borrow
  // the margin button's own label, which describes the action rather than the
  // precondition and leaves a reader who cannot click it none the wiser.
  "review.comment.needsRange": "Select the text you want to comment on",
  "toolbar.undo": "Undo",
  "toolbar.redo": "Redo",
  // The engine names the action a step will undo ("Typing", "Table structure").
  // A language that puts the verb after the object needs the whole sentence,
  // not "Undo" plus a noun glued on, which is why this is one key and not two.
  "toolbar.undoNamed": "Undo {name}",
  "toolbar.redoNamed": "Redo {name}",
  // Find and REPLACE as its own command, so ⌘H has an id to run — Word, Google
  // Docs and ONLYOFFICE all bind Ctrl+H to it. The find panel's own button says
  // the same word.
  "find.replaceCommand": "Replace",
  // The heading chords' palette rows (`quick_styles.mjs`). `{name}` is Word's UI
  // name for a built-in style, which is not localised yet (`style_names.mjs`).
  "style.apply.heading": "Apply Heading {level}",
  "style.apply.normal": "Apply Normal style",
  // The three heading chords as ONE row of File ▸ Shortcuts, the way Google
  // Docs' own reference lists them.
  "style.apply.headingRange": "Apply Heading 1–3",
  "style.reason.missing": "This document has no {name} style",
  // F6 / Shift+F6 (`region_focus.mjs`): Word's "move to the next pane", named for
  // what moves — the keyboard — rather than for any one pane.
  "region.next": "Move to the next region",
  "region.previous": "Move to the previous region",
  "region.move": "Move between regions",
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
  // The unit is a PARAMETER now, not the English word "in" baked into the
  // sentence. Every locale translated that word — "po" in French, "\u30a4\u30f3\u30c1" in
  // Japanese — which was correct while the only unit was inches and became a
  // wrong unit presented as a right one the moment the preference could move.
  // What is substituted is the engine's own suffix (`cm`, `mm`, `in`, `pt`, `pi`):
  // a unit SYMBOL, the same in every language, which is the convention
  // `borderWeightLabel` already prints `pt` under.
  "pageSetup.dimensions": "{width} \u00d7 {height} {unit}",
  // The object bar's hint while a crop session is live. It is its own key rather
  // than a reuse of the resize hint because it names a different gesture: the
  // grips under the pointer are crop grips, and the bar that said "Drag handles
  // to resize" over them was describing the mode it had just left.
  "object.cropHint": "Drag the edges to crop",
  // The two running-content variants. Their status lines and their palette rows
  // read as SWITCHES ("Different first page: on"), and a language that puts the
  // state before the name needs the whole sentence rather than a label with "on"
  // glued to it — which is why each state is its own key and not one key plus a
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
  // ---- Compare documents ----------------------------------------------------
  //
  // `docs/153` `review.compare-documents`. The whole surface is script-built, so
  // every sentence here is one a catalogue has to answer or the panel prints a
  // dotted key.
  "compare.command": "Compare with another document…",
  "compare.needsDocument": "Open a document to compare another one with",
  "compare.intro": "Pick a document to compare this one with. Nothing is uploaded.",
  "compare.chooseFile": "Choose a document…",
  // REPLACES `compare.writesTrackedChanges` (ADR-065): a comparison is SHOWN on
  // the page now, read-only, and written into this document only if the reader
  // keeps it. Said before a file is picked, because it is the reassurance that
  // matters to someone about to compare their own document.
  "compare.showsOnPage":
    "The differences are shown on the page. Your document is not changed unless you keep them as tracked changes.",
  // The redline on the canvas (ADR-065): its heading, its read-only reason (also
  // the sentence an attempted edit is refused with), and the three ways on.
  "compare.changesFrom": "Changes from {older} to {newer}",
  "compare.thisDocument": "this document",
  "compare.viewReadOnly": "You are looking at a comparison; close it to change the document",
  "compare.keep": "Keep as tracked changes",
  "compare.swap": "Swap order",
  "compare.closeView": "Close comparison",
  // The diff canvas's navigator and key (`diff_canvas.mjs`, ADR-065), shared by
  // version history and Compare. "{index} of {count}" is a position, not a
  // plural, so every language needs one form.
  "diffCanvas.position": "{index} of {count}",
  "diffCanvas.previous": "Previous change",
  "diffCanvas.next": "Next change",
  "diffCanvas.moved": "Moved",
  "diffCanvas.replaced": "Replaced",
  "diffCanvas.changesBy": "Changes by {name}",
  // A version captured with no author name recorded: versions live in this
  // browser, so they are the reader's own — the Settings name field's own
  // placeholder says the same.
  "diffCanvas.someone": "You",
  "diffCanvas.details": "What isn't highlighted",
  // After "Keep as tracked changes": a sentence about the document, with NO COUNT
  // in it — "1 differences" is what a count in a sentence rendered, and a
  // labelled number (`compare.changeCount`) needs one form per language where a
  // plural needs Arabic's six.
  "compare.marked": "The differences are now tracked changes in this document.",
  // And how to walk them: review's own next/previous, over the revisions Keep
  // just wrote.
  "compare.reviewNav":
    "Review ▸ Next and Previous move through them; Accept or Reject decides each one.",
  // The author a comparison's tracked changes are attributed to when the compared
  // document has no name to use — ADR-061 attributes them to the compared
  // document so the author colour tells a computed difference from a person's
  // suggestion.
  "compare.author": "Compared document",
  // WHAT THE COMPARISON FOUND AND COULD NOT MARK. `EditResult.pasteLoss`, routed
  // rather than swallowed: a comparison that silently applied nine of twelve
  // changes and reported success is the worst outcome available, and the engine
  // computes this report precisely so a host can say it.
  "compare.unmarkedTitle": "Found, but not marked in the document:",
  "compare.unmarked.blockDeletion":
    "Whole paragraphs the other document has and this one does not — a tracked change marks text that is here, and there is no paragraph here to mark.",
  "compare.unmarked.trackedMove":
    "The far half of a move: where the content came from is in the other document only.",
  // A removed paragraph is shown struck where it was (ADR-065), as its text: a
  // picture's bytes belong to the other document's package.
  "compare.unmarked.removedObject": "Pictures and other objects in removed paragraphs are not shown.",
  // The body only, and deliberately. A body path maps back to this document by
  // identity because the comparison's right-hand side is its own re-export; a
  // header, footer, note or comment story is paired by position or ordinal and
  // depends on the export writing the same section structure back, which is
  // unmeasured — and an unmeasured mapping would put a revision in the WRONG
  // header rather than refuse to.
  "compare.unmarked.otherStory":
    "Differences in a header, footer, note or comment: a comparison is written into the body only.",
  "compare.unmarked.truncatedText":
    "Removed text too long to record word for word, so the deletion is reported rather than reconstructed.",
  "compare.unmarked.incompleteComparison":
    "The comparison itself stopped short of exhaustive, so this is not everything that differs.",
  "compare.unmarked.notMarkable":
    "A real difference with no run of text here to mark it on.",
  // THE FOUR CODED REFUSALS `applyDiffAsRevisions` can return, each a sentence
  // this host routes by code so it is read in the reader's own language rather
  // than in the engine's English.
  //
  // The first is the one a reader will actually meet, and it is DELIBERATE: one
  // `reviewType` field cannot carry both "a person suggested this" and "a
  // comparison computed this" without the two deciding each other. ONLYOFFICE
  // accepts every existing change first, on consent (`Comparison.js:3910-3921`);
  // destroying a reviewer's suggestions to run a comparison is the loss
  // `AGENTS.md` puts first. So the sentence says what we refuse and why, and does
  // not apologise for it.
  "compare.refused.documentHasRevisions":
    "This document already has tracked changes. Accept or reject them first — a comparison writes its own tracked changes, and merging the two would decide someone else's suggestions for them.",
  "compare.refused.schemaUnsupported":
    "This comparison was made by a different version of the editor and cannot be applied to the document.",
  "compare.refused.sidecarUnreadable":
    "This comparison could not be read, so nothing was written into the document.",
  "compare.refused.authorRequired":
    "A comparison's tracked changes need an author name, and the compared document supplied none.",
  "compare.parsing": "Reading both documents…",
  "compare.comparing": "Comparing…",
  "compare.cancel": "Cancel",
  "compare.cancelled": "Comparison cancelled.",
  "compare.against": "Compared with {name}",
  "compare.identical": "No differences.",
  // What the comparison could NOT compare, aggregated by the engine — one row per
  // construct with a count, so forty thousand drawings are one line.
  "compare.findingsTitle": "Not fully compared:",
  "compare.finding.notCompared":
    "{construct} changed, and this build has no detailed comparison for it ({count})",
  "compare.finding.ambiguousMatch":
    "{construct} had two equally good matches, so it is reported as a removal and an addition ({count})",
  "compare.finding.missingResource":
    "{construct} is absent on one side, so it could not be compared by content ({count})",
  "compare.finding.truncated":
    "{construct} reached a limit, so the comparison of it stopped being exhaustive ({count})",
  // Deliberately NOT a plural family, the same decision the version-history
  // findings sentence records: it reports a labelled number, which reads the same
  // in every language and needs one form per locale instead of Arabic's six.
  "compare.changeCount": "Differences: {count}",
  "compare.cannotExport":
    "This document could not be written out, so there is nothing to compare.",
  // The engine's own sentence about this document — an admission limit, a corrupt
  // package — passed through, because "the comparison failed" without naming the
  // cause sends a reader looking for a problem with the wrong file. The same split
  // `editRefusalMessage` makes.
  "compare.failed": "The comparison failed: {reason}",
  // …and this is what the module's OWN internal outcomes say instead. "The
  // comparison failed: budget" would be the raw-token-in-the-status-bar defect.
  "compare.noAnswer": "The comparison stopped before it had an answer.",
  // The construct families `casual-doc-diff` reports, each with its count. One
  // key per family rather than one sentence with a family name interpolated: a
  // language that inflects the noun after a number cannot be served by a
  // template, and twelve short keys are cheaper than one wrong sentence.
  "compare.family.block": "Blocks added, removed or moved: {count}",
  "compare.family.text": "Text edits: {count}",
  "compare.family.formatting": "Formatting changes: {count}",
  "compare.family.style": "Style and list changes: {count}",
  "compare.family.table": "Table changes: {count}",
  "compare.family.object": "Object changes: {count}",
  "compare.family.section": "Section and page setup changes: {count}",
  "compare.family.definition": "Definition changes: {count}",
  "compare.family.resource": "Resource changes: {count}",
  "compare.family.comment": "Comment changes: {count}",
  "compare.family.review": "Tracked-change differences: {count}",
  "compare.family.metadata": "Metadata changes: {count}",
  // The kinds, in review's own vocabulary — the engine says so explicitly, and
  // reusing review's words is what stops a reader having to learn a second set.
  "compare.kind.insertion": "Added",
  "compare.kind.deletion": "Removed",
  "compare.kind.move_from": "Moved from here",
  "compare.kind.move_to": "Moved to here",
  "compare.kind.formatting": "Reformatted",
  // ---- Breaks ---------------------------------------------------------------
  //
  // The six NAMES are not here, and that is deliberate. They live beside their
  // `data-i18n` attributes on the popover rows in `editor.html`, which is where
  // markup's English belongs (`docs/124` §3.1) — and `build-locale.mjs` refuses a
  // key declared in both places, correctly, because two declarations are two
  // chances to disagree. `break_commands.mjs` reads them back through `t()` for
  // the command palette, which works for the same reason every markup key works:
  // the extractor puts them in `locales/en.json` and the fallback chain ends
  // there.
  //
  // They are FLAT names with the kind spelled out — Google Docs' Insert ▸ Break
  // rather than Word's "Section Breaks" heading over bare "Next Page" rows —
  // because the same label has to serve the dropdown AND the palette, where
  // "Next Page" alone says nothing about what it does.
  //
  // Said on SUCCESS, which a break needs more than most edits do: it is
  // invisible at the caret, and on a short document it may move nothing on
  // screen at all, so silence reads as "nothing happened" (`docs/67`). The
  // refusals come from the engine, which names the container it refused in.
  "break.inserted.page": "Page break inserted",
  "break.inserted.column": "Column break inserted",
  "break.inserted.section": "Section break inserted",
  "menuGroup.breaks": "Breaks",
  "filePane.export.label": "Export",
  // ---- Format names ---------------------------------------------------------
  //
  // ONE name per format, read by the File menu's `Export as …` rows, the File
  // page's Export tiles, the Save-as picker and the version-history download
  // (`format_io.mjs`). They were English literals in that frozen table, so every
  // format name was untranslated in eighteen languages — and, worse, adding a
  // format cost an unrouted-strings ceiling, which is exactly why Markdown, HTML
  // and the Word template shipped in the engine and reached the picker as
  // `text.markdown` and `org.openxmlformats.wordprocessingml.template`. A
  // capability that ships with no name is a capability that did not arrive.
  //
  // FOUR OF THE NINE ARE THE SAME IN EVERY LOCALE ON PURPOSE. PDF, DOCX, ODT and
  // Markdown are not words: the first three are file-format initialisms a person
  // reads on a Save dialog in every language, and Markdown is a product name. An
  // entry translated in eighteen catalogues would be eighteen chances to mistype
  // a format and no chance to improve a translation — the same decision this
  // catalogue already records for "GitHub" and the product name. They are still
  // KEYS rather than literals, because a key can be overridden by a host and a
  // literal cannot, and because the alternative is a table that is half routed.
  "format.pdf": "PDF",
  "format.docx": "DOCX",
  "format.dotx": "Word Template",
  "format.odt": "ODT",
  "format.rtf": "Rich Text Format",
  "format.html": "Web Page",
  "format.markdown": "Markdown",
  "format.text": "Plain text",
  "format.json": "Normalized JSON",
  // The File menu's export rows, composed from one pattern and the name above.
  // Nine rows, one sentence: Word's Save As and Google Docs' Download both name
  // the format once and let the surface supply the verb.
  "filePane.export.as": "Export as {format}…",
  // Said by an export row whose format this engine build registers no WRITER
  // for. RTF is the live case: it imports and cannot be written back
  // (`can_export: false`), so the row is disabled carrying this rather than
  // offering a save that throws. Distinct from `capability.notGranted`, which is
  // the host withholding downloads — "this cannot be done" and "not for you" are
  // different answers and a reader deserves the right one.
  "filePane.export.noWriter": "This build cannot write {format}",
  // The compatibility findings (`compat_findings.mjs`): what the header chip
  // counts, opened. The kinds are named for what HAPPENED to a construct, because
  // that is what a reader deciding whether to save over the original needs —
  // the engine's two outcome axes, folded into five plain answers.
  "findings.command": "Compatibility findings…",
  "findings.none": "This document has no compatibility findings",
  "findings.title": "Compatibility findings",
  "findings.importIntro": "What this editor could not represent exactly when it opened the document, grouped by what happened to it.",
  "findings.exportIntro": "What the last save could not write exactly, grouped by what happened to it.",
  "findings.kind.lost": "Not kept",
  "findings.kind.refused": "Refused for safety or size",
  "findings.kind.approximated": "Shown approximately; the original is kept in the file",
  "findings.kind.preserved": "Kept in the file, not shown or editable here",
  "findings.kind.unsorted": "Other findings",
  "findings.close": "Close",
  "findings.closeLabel": "Close compatibility findings",
  // What each finding IS, in words a reader shares (`109` FID-AT-05,
  // `findings_catalogue.mjs`). The engine's id stays on the row, muted, for
  // support; this is what is read first. Word's own UI names are used where
  // Word has one ("Lock aspect ratio", "Decimal symbol", "Page color"). None of
  // these says whether the thing was kept or lost — the group heading a row
  // sits under says that, from the engine's outcome, so a word here can never
  // contradict it.
  //
  // The tooltip on the row's muted id, which is what support matches on.
  "findings.featureId": "Identifier",
  // Word's own bookkeeping: a collapsed group inside each kind, kept out of the
  // headline count, every entry still listed when it is opened.
  "findings.bookkeeping.label": "Word's own bookkeeping",
  "findings.bookkeeping.note": "Records Word keeps about the file, not content you would see in it.",
  "findings.bookkeeping.only": "Nothing listed here is content you would see. It is all Word's own bookkeeping.",
  // Where a finding is. A part this list does not know is named as itself.
  "findings.where.body": "in the document body",
  "findings.where.header": "in a header",
  "findings.where.footer": "in a footer",
  "findings.where.footnotes": "in the footnotes",
  "findings.where.endnotes": "in the endnotes",
  "findings.where.comments": "in the comments",
  "findings.where.settings": "in the document settings",
  "findings.where.theme": "in the theme",
  "findings.where.styles": "in the styles",
  "findings.where.numbering": "in the list definitions",
  "findings.where.fonts": "in the fonts",
  "findings.where.chart": "in a chart",
  "findings.where.diagram": "in a SmartArt graphic",
  "findings.where.glossary": "in the building blocks",
  "findings.where.media": "in a picture or media file",
  "findings.where.embedding": "in an embedded file",
  "findings.where.webSettings": "in the web page settings",
  "findings.where.properties": "in the document properties",
  "findings.where.customXml": "in the custom XML data",
  "findings.where.part": "in the file part {part}",
  "findings.where.formatWord": "in the Word file",
  "findings.where.formatOdf": "in the OpenDocument file",
  "findings.where.formatRtf": "in the RTF file",
  "findings.where.formatMarkdown": "in the Markdown file",
  "findings.where.formatHtml": "in the HTML file",
  "findings.where.formatPdf": "in the PDF",
  "findings.where.formatText": "in the plain text",
  // The fallback for an id nobody has catalogued: what KIND of thing it is.
  "findings.unknown.element": "Other content or formatting",
  "findings.unknown.attribute": "A property of an item",
  "findings.unknown.part": "A separate part of the file",
  "findings.unknown.class": "Another kind of finding",
  "findings.unknown.setting": "A document setting",
  "findings.unknown.property": "A document property",
  // Word's own bookkeeping.
  "findings.feature.rsid": "Editing-session numbers Word adds on every save, for Compare and Combine",
  "findings.feature.docId": "Identifier Word assigns to the document",
  "findings.feature.thumbnail": "Thumbnail preview of the first page",
  "findings.feature.savePreviewPicture": "Setting to save a preview picture with the file",
  "findings.feature.webSettings": "Web page settings (Web Options)",
  "findings.feature.stylesWithEffects": "Copy of the styles kept for Word 2010",
  "findings.feature.hyperlinksChanged": "Note in the file properties that links have changed",
  "findings.feature.staleThumbnail": "Thumbnail preview of the first page, out of date after your edits",
  "findings.feature.staleStatistics": "Page, word and character counts in the file properties, out of date after your edits",
  "findings.feature.staleStylesWithEffects": "Word 2010 copy of the styles, out of date after your edits",
  "findings.feature.paraId": "Paragraph identifiers Word assigns",
  "findings.feature.textId": "Text-version identifiers Word assigns",
  "findings.feature.rowId": "Table row identifiers Word assigns",
  // Drawings and pictures.
  "findings.feature.objectName": "Object name shown in Word's Selection Pane",
  "findings.feature.objectLocks": "Object locks, such as Lock aspect ratio",
  "findings.feature.pictureLocks": "Picture locks, such as Lock aspect ratio",
  "findings.feature.drawing": "Drawing (picture, shape, chart or text box)",
  "findings.feature.legacyShape": "Shape in the older Word format (VML)",
  "findings.feature.legacyShapeType": "Shape template in the older Word format (VML)",
  "findings.feature.legacyWordArt": "WordArt text in the older Word format",
  "findings.feature.legacyShapePath": "Shape outline in the older Word format",
  "findings.feature.shapeFill": "Shape fill (color, gradient, pattern or picture)",
  "findings.feature.textBox": "Text box",
  "findings.feature.watermark": "Watermark",
  // Charts and embedded files.
  "findings.feature.chartTrendline": "Chart trendline",
  "findings.feature.chartDetail": "Other chart formatting or content",
  "findings.feature.chartColors": "Chart color style",
  "findings.feature.chartStyle": "Chart style",
  "findings.feature.embeddedFile": "Embedded file, such as the workbook behind a chart",
  // The theme.
  "findings.feature.themeName": "Document theme name",
  "findings.feature.themeFontsName": "Name of the theme fonts",
  "findings.feature.themeObjectDefaults": "Theme's default look for new shapes, lines and text boxes",
  "findings.feature.themeExtraColors": "Extra color sets stored in the theme",
  // Document settings.
  "findings.feature.decimalSymbol": "Decimal symbol used in calculations",
  "findings.feature.listSeparator": "List separator used in calculations",
  "findings.feature.defaultImageDpi": "Default resolution for pictures",
  "findings.feature.doNotCompressImages": "Do not compress images in file",
  "findings.feature.equationOptions": "Equation options",
  "findings.feature.settingsFragmentRefused": "A document setting that could not be written back safely",
  "findings.feature.shapeDefaults": "Default look for new shapes (older Word format)",
  "findings.feature.headerShapeDefaults": "Default look for new shapes in headers (older Word format)",
  "findings.feature.eastAsianLayout": "Layout option for East Asian and complex scripts",
  "findings.feature.formProtection": "Section protection for filling in forms",
  "findings.feature.colorMapping": "Theme color mapping",
  "findings.feature.characterSpacing": "Character spacing control for East Asian text",
  "findings.feature.chartTracking": "Properties follow chart data point",
  "findings.feature.attachedTemplate": "Attached template",
  "findings.feature.documentVariables": "Document variables",
  "findings.feature.mailMerge": "Mail merge data source and settings",
  "findings.feature.restrictEditingPassword": "Password for Restrict Editing",
  "findings.feature.modifyPassword": "Password to modify",
  // Data stored with the document.
  "findings.feature.customXml": "Custom XML data stored with the document",
  "findings.feature.customXmlProperties": "Identity and schemas of the custom XML data",
  // Fonts and media the engine could not use.
  "findings.feature.embeddedFontUnusable": "Embedded font that could not be used",
  "findings.feature.mediaUnreadable": "Picture or media file that could not be read",
  // What a save could not write.
  "findings.feature.pageColor": "Page color",
  "findings.feature.pictureDataMissing": "Picture whose image data is missing",
  "findings.feature.unusedHeader": "Header no section uses",
  "findings.feature.unusedFooter": "Footer no section uses",
  "findings.feature.embeddedObjectMissing": "Embedded object whose file is missing",
  "findings.feature.chartNotRewritten": "Chart this editor cannot fully rewrite",
  "findings.feature.chartWorkbookRewritten": "Workbook behind a chart, rewritten from the chart's data",
  "findings.feature.chartDetailNotWritten": "Chart details this editor cannot write",
  "findings.feature.watermarkSharedHeader": "Watermark in a header another section shares",
  "findings.feature.watermarkPictureMissing": "Picture watermark whose image data is missing",
  "findings.feature.embeddedFontMissing": "Embedded font whose data is missing",
  "findings.feature.retainedParts": "Extra parts carried from the original file",
  "findings.feature.sourceMismatch": "Extra parts of an original file that no longer matches this document",
  "findings.feature.overflow": "More findings than this list can hold",
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
  // The version Make a copy takes of the current document before the copy
  // replaces it. "Version created" said nothing about why it exists.
  "versionHistory.kind.manual": "Before making a copy",
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

  // WHAT CHANGED, per row. The owner's second report was about the timeline
  // itself: "still no improvement in versions". Every row read `Saved · 16 KB`,
  // so the panel answered *when* and never *what*, and nothing told two rows
  // apart but a clock. These four are everything the stored metadata can honestly
  // say without parsing a checkpoint — see `versionRowDeltas` for why each one is
  // derivable and what it deliberately does not claim.
  //
  // `sameAs` is the one the owner's FIRST report is about. A duplicate row can no
  // longer be created by an implicit capture, but the integrity captures can still
  // make one and older timelines already have them, so a duplicate names the
  // version it duplicates instead of leaving the reader to diff by eye.
  "versionHistory.row.sameAs": "Same content as {name}",
  "versionHistory.row.edits.one": "{count} edit",
  "versionHistory.row.edits.other": "{count} edits",
  "versionHistory.row.by": "by {name}",
  // Labelled numbers, not plural families, for the reason `compare.changeCount`
  // gives: one form per language instead of Arabic's six.
  "versionHistory.row.wordsAdded": "Words added: {count}",
  "versionHistory.row.wordsRemoved": "Words removed: {count}",

  // Said, not whispered. SKILL §10 forbids a silent no-op, and a capture that
  // found nothing new to keep used to be exactly that: no row appeared and
  // nothing explained why. This is the panel's line for it — the status channel
  // stays out of it on purpose, because a Save's own "Saved <name>" is the
  // sentence the reader needs at that moment and two sentences about one act
  // race each other.
  "versionHistory.unchangedNote": "No changes since {name}, so no new version was kept.",

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
  // The two ways out of a version that are not "replace my document with it"
  // (`docs/139` VH-007). Google Docs offers both at exactly this moment.
  "versionPanel.makeACopy": "Make a copy",
  "versionPanel.downloadThisVersion": "Download this version",
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
  "versionHistory.headNotDeletable": "This is the current version, so it can't be deleted",
  // CORRECTED 2026-10-01. This read "Comparing one version with another is not
  // built yet", and the comment above it said the structural diff was `docs/140`
  // H3 and not built. The diff was built the whole time — `casual-doc-diff` plus
  // `casual-doc-wasm/src/diff.rs` — and the PANEL was not, which `docs/140`
  // §8-13 already said. Show changes is live now, and this sentence is what it
  // says when the surface that shows a comparison is not available at all: an
  // embedded editor composed without the Compare panel, where the capability
  // genuinely is not there. `earliestNotComparable` is the other refusal.
  "versionHistory.action.showChangesUnavailable":
    "Comparing versions is not available in this editor",
  // RENAMED FROM `headNotComparable` 2026-10-05, English and all eighteen
  // translations, because the row that refuses has changed. That sentence read
  // "This is the current version — comparing it with itself would show nothing",
  // and it was true of a comparison against the document on screen. ADR-062
  // compares a version against its PREDECESSOR, so the head is now the most
  // useful row in the panel and the one with nothing to compare against is the
  // earliest version still kept.
  "versionHistory.earliestNotComparable":
    "This is the earliest version kept — there is nothing before it to compare with",

  // The counts and the policy, under the list. A person who cannot see the bound
  // cannot trust the promise (docs/139 §12).
  "versionHistory.kept.one": "{count} version kept",
  "versionHistory.kept.other": "{count} versions kept",
  "versionHistory.footerDetail": "{size} in this browser · {named} of {limit} named",
  // REWORDED 2026-10-09: "kept for at least {days} days, up to {count}" promised
  // a minimum the policy does not keep — the count ceiling removes younger
  // versions too. This says what happens.
  "versionHistory.retention":
    "Recent versions are kept for up to {days} days, {count} at most. Named versions are kept until you delete them.",

  // Why the entry point is disabled. Five different reasons, because the way out
  // of each is different and a reader can only act on the specific one.
  "versionHistory.disabled.noDocument": "Open a document to see its version history",
  // Settings has no version-history switch (only the host's stored preference
  // reaches it), so this no longer sends the reader to look for one.
  "versionHistory.disabled.setting": "Version history is turned off for this editor.",
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
    "Your document is replaced by “{name}”. What you have now is saved as a version first, so you can go back to it.",
  "versionHistory.restore.confirm": "Restore",
  "versionHistory.restore.cancel": "Cancel",
  "versionHistory.restore.note": "The restored document is unsaved until you save it to a file.",
  "versionHistory.restore.cannotKeepCurrent":
    "The document on screen could not be kept as a version ({message}), so it was not replaced.",
  "versionHistory.restore.failed": "That version could not be restored: {message}",
  "versionHistory.restored":
    "Restored the version from {when}. The document you replaced is still in the timeline.",

  // ---- Getting a version out (docs/139 VH-007) -----------------------------
  // A download hands over the checkpoint's own bytes, so it cannot differ from
  // what the preview showed. What CAN surprise a reader is what was already lost
  // when that artifact was written, and a format this build no longer knows —
  // both are said rather than left to be discovered.
  //
  // The findings sentence is deliberately NOT a plural family: a labelled number
  // needs one form per language instead of Arabic's six, and a count the reader
  // can act on reads the same either way.
  "versionHistory.downloaded": "Downloaded “{name}”.",
  "versionHistory.download.lossy":
    "Downloaded “{name}”. Compatibility findings recorded when this version was written: {count}.",
  "versionHistory.download.unknownFormat":
    "Downloaded “{name}”. This build does not recognise the format this version was written in ({format}), so the file is exactly the stored bytes.",

  "versionHistory.copy.name": "Copy of {name}",
  "versionHistory.copy.title": "Make a copy of this version?",
  "versionHistory.copy.message":
    "“{name}” opens here, from the version of {when}. The document on screen is kept as a version of its own first.",
  "versionHistory.copy.confirm": "Make a copy",
  "versionHistory.copy.cancel": "Cancel",
  "versionHistory.copy.note": "The copy is unsaved until you save it to a file, and starts its own history.",
  "versionHistory.copied": "Made a copy: “{name}”. It is unsaved until you save it to a file.",
  "versionHistory.copy.failed": "That version could not be copied: {message}",

  "versionHistory.named": "Named this version “{name}”.",
  "versionHistory.pinned": "This version is now kept until you say otherwise.",
  "versionHistory.unpinned": "This version may now be removed automatically, like any recent version.",
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

  // ---- Touch selection (docs/105 UX-018, docs/148 §9) -----------------------
  // The two drag handles a finger uses to make and adjust a selection. They are
  // the only visible selection affordance on a touch device, so they are named
  // rather than decorative: a screen reader explores a touchscreen by touch and
  // an unlabelled circle over a word says nothing. LOGICAL names — "start" is
  // the start of the selection in reading order, which in a right-to-left
  // paragraph is the handle on the right — so a translation must not turn them
  // into "left" and "right".
  "touchSelection.startHandle": "Selection start",
  "touchSelection.endHandle": "Selection end",

  // ---- Tables (docs/141) ---------------------------------------------------
  // The four reasons a Table-band control can be unavailable. They were already
  // the MENU's sentences, written as literals in `tableToolCommands`, while the
  // band kept its authored tooltip and explained nothing (TBL-03). One source
  // for one sentence, so band and menu cannot drift into disagreeing about the
  // same precondition.
  "table.reason.caretOutsideTable": "Place the caret in a table",
  "table.reason.merged": "Unavailable for merged or spanned tables",
  "table.reason.rowHeights": "Rows need a fixed or minimum height before distribution",
  // REWORDED with the cell range (`docs/141` D-3). It used to read "Select a
  // row, column, or table before merging", which named the three degenerate
  // rectangles `mergeTableSelection` could take — and was therefore a statement
  // about the old API rather than about the rule. `mergeTableCellRange` takes any
  // rectangle, so the rule is simply that one cell is not two: a one-cell
  // selection is a caret with a fill on it, and there is nothing to merge it
  // with. This is also the sentence a user sees when a drag selected exactly one
  // cell, which is the common way to meet it.
  "table.reason.mergeSelection": "Select two or more cells before merging",
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
  // A table's size, COLUMNS first — Word's own convention ("4x3 Table" over its
  // Insert grid for four columns and three rows). The Insert grid and the Table
  // band's hint both read this one sentence, because they used to print the
  // same table as "4 × 3" and "3×4 table". `sizeSpoken` is the grid cell's
  // accessible name, where a screen reader would read "×" as "times".
  "table.size": "{columns} × {rows} table",
  "table.sizeSpoken": "{columns} by {rows} table",
  "table.sizeGrid": "Table size",
  // The row/column/table selection's own status line. It was built as
  // `Selected table ${mode}`, the one table status line that was not localised
  // at all, and glueing a translated noun onto a fixed verb is what these three
  // keys exist to avoid.
  // The table chrome layer's refusal and its two announcements (`docs/141` D-1).
  //
  // `notTracked` is the sentence `tableToolCommands` already shows on a disabled
  // structural command, routed through a key so the boundary gesture and the menu
  // say the same thing in every language. A boundary in Suggesting is not armed at
  // all — the cursor stays `text` — so this is what a PRESS there says, because a
  // user reaching for a gesture every other mode has deserves to know why it is
  // absent rather than to find it silently missing.
  "table.reason.notTracked": "This structural change cannot be tracked in Suggesting mode",
  // A resize commits on release, and the only channel a screen-reader user has for
  // the result is the live region. The size is preformatted in inches by the
  // caller, the same two-decimal figure the properties inspector's own fields
  // show, so the announcement and the panel cannot disagree about the number.
  "table.rowHeightSet": "Row height {size} in",
  "table.columnWidthSet": "Column width {size} in",
  "table.selectedRow": "Row selected",
  "table.selectedColumn": "Column selected",
  "table.selectedTable": "Table selected",
  // The cell RANGE's own announcement (`docs/141` D-3). A plural family rather
  // than "{count} cells": the live region is the only channel a reader who
  // cannot see the fill has, and "1 cells selected" is the kind of thing a
  // catalogue exists to stop. `one` is reachable — a drag can shrink back to its
  // starting cell — even though merge refuses there.
  "table.cellsSelected.one": "{count} cell selected",
  "table.cellsSelected.other": "{count} cells selected",
  // Said when the engine GREW the rectangle past what was dragged, to contain
  // whole merged cells (`CellRangeInfo.expanded`). Without it an expanded
  // selection reads as the editor selecting more than you asked for; with it,
  // it reads as the rule it is — a merged cell is one cell and cannot be half
  // selected.
  "table.selectionExpanded": "expanded to whole merged cells",
  // The gutter's insert affordance (`docs/141` D-2). The tooltip is what the `+`
  // disc says before it is pressed and the announcement is what it says after,
  // because the disc is a pointer affordance and the live region is the only
  // channel that reports the result.
  "table.insertRowHere": "Insert a row here",
  "table.insertColumnHere": "Insert a column here",
  "table.rowInserted": "Row inserted",
  "table.columnInserted": "Column inserted",
  // ---- Drawing: the shapes gallery, wrap, position, arrange, rotate --------
  // The whole arrange surface, localised in one block because it ships as one
  // capability: a shape you can draw, place, wrap text around, stack, group and
  // rotate. The shape names are Word's own gallery names, which is what a person
  // who has used Word will search for.
  "shapes.group.lines": "Lines",
  "shapes.group.rectangles": "Rectangles",
  "shapes.group.basic": "Basic shapes",
  "shapes.group.arrows": "Block arrows",
  "shapes.group.stars": "Stars",
  "shapes.line": "Line",
  "shapes.rect": "Rectangle",
  "shapes.roundRect": "Rounded rectangle",
  "shapes.ellipse": "Oval",
  "shapes.triangle": "Isosceles triangle",
  "shapes.rtTriangle": "Right triangle",
  "shapes.diamond": "Diamond",
  "shapes.pentagon": "Regular pentagon",
  "shapes.hexagon": "Hexagon",
  "shapes.octagon": "Octagon",
  "shapes.trapezoid": "Trapezoid",
  "shapes.parallelogram": "Parallelogram",
  "shapes.plus": "Cross",
  "shapes.rightArrow": "Right arrow",
  "shapes.leftArrow": "Left arrow",
  "shapes.upArrow": "Up arrow",
  "shapes.downArrow": "Down arrow",
  "shapes.leftRightArrow": "Left-right arrow",
  "shapes.chevron": "Chevron",
  "shapes.homePlate": "Pentagon arrow",
  "shapes.star5": "Five-point star",
  "shapes.star4": "Four-point star",
  // The drag-to-draw gesture (Word's, and every drawing tool's). Said when the
  // gallery arms the pointer, and again when Escape disarms it, because a
  // crosshair with no explanation is a mode the user cannot see the edge of.
  "shape.drawHint": "Drag on the page to draw the shape · click for the default size · Esc to cancel",
  "shape.drawCancelled": "Drawing cancelled",
  "shape.added": "{shape} added",
  // Wrap. “In line” is the mode an inserted picture STARTS in, so it leads the
  // row — Google Docs puts it first for the same reason.
  "object.wrap.inline": "In line",
  "object.wrap.square": "Square",
  "object.wrap.tight": "Tight",
  "object.wrap.through": "Through",
  "object.wrap.topAndBottom": "Top & bottom",
  "object.wrap.behind": "Behind text",
  "object.wrap.front": "In front",
  "object.wrap.label": "Text wrapping",
  "object.wrap.notAnObject": "Select an image, shape or text box first",
  "object.wrap.groupChild": "A shape inside a group is positioned by its group",
  // Position — Word's nine-cell gallery, against the margin.
  "object.position": "Position",
  "object.position.topLeft": "Top left",
  "object.position.topCenter": "Top centre",
  "object.position.topRight": "Top right",
  "object.position.middleLeft": "Middle left",
  "object.position.middleCenter": "Middle centre",
  "object.position.middleRight": "Middle right",
  "object.position.bottomLeft": "Bottom left",
  "object.position.bottomCenter": "Bottom centre",
  "object.position.bottomRight": "Bottom right",
  "object.position.groupChild": "A shape inside a group is positioned by its group",
  // Stacking. One engine op behind four commands, because “forward” and “to
  // front” are different intentions even when a document has two objects.
  "object.arrange": "Arrange",
  "object.z.front": "Bring to front",
  "object.z.forward": "Bring forward",
  "object.z.backward": "Send backward",
  "object.z.back": "Send to back",
  "object.z.notStackable": "Only a floating object or a shape in a group can be restacked",
  // Grouping. The refusal the user sees is the ENGINE's sentence whenever it has
  // one; this is the one case it has none, because nothing was asked of it.
  "object.group": "Group",
  "object.ungroup": "Ungroup",
  "object.group.selectMore": "Hold {modifier} and click another object to group them",
  "object.ungroup.notAGroup": "Select a group to ungroup it",
  "object.grouped": "Objects grouped",
  "object.ungrouped": "Group ungrouped",
  "object.addedToSelection": "Added to the selection",
  // Rotation and flips — Word's Rotate menu, the same four rows.
  "object.rotate": "Rotate",
  "object.rotate.right90": "Rotate right 90°",
  "object.rotate.left90": "Rotate left 90°",
  "object.rotate.flipVertical": "Flip vertical",
  "object.rotate.flipHorizontal": "Flip horizontal",
  "object.rotate.unsupported": "This object's model carries no rotation — group it first",
  // The rotation HANDLE: the direct-manipulation half of the same capability.
  // It is a slider, not a button — it has a value and arrow keys that change
  // it — so it needs a name and a spoken value, and the value text is what a
  // screen reader reads while the object turns.
  "object.rotate.handle": "Rotate object",
  "object.rotate.degrees": "{degrees}°",
  "object.rotate.applied": "Rotated to {degrees}°",
  "object.rotate.notTracked": "Rotating an object is not tracked; switch to Editing to rotate it",
  "object.resize.notTracked": "Resizing an object is not tracked; switch to Editing to resize it",
  // Text inside a shape. Double-click is the gesture in Word and in Docs; the
  // menu row is the second surface, because a gesture nobody told you about is
  // not reachable.
  "object.addText": "Add text",
  "object.addText.notAShape": "Only a shape can hold text this way",
  // Word's Change Picture (`docs/109` HF-252). The status line says what was
  // KEPT, and asks for the one check a person must make: alt text written for
  // the old picture may not describe the new one.
  "object.changePicture": "Change picture",
  "object.changePicture.menu": "Change picture…",
  "object.changePicture.unsupported": "That file can’t be used as a picture. Choose a PNG, JPEG, GIF, BMP, TIFF or WebP image.",
  "object.changePicture.unreadable": "Could not read that image",
  "object.changePicture.done": "Picture changed — its size, position, wrap and border were kept",
  "object.changePicture.doneAltText": "Picture changed — check that its alt text still describes the new picture",
  // Word's Picture Border: a picture's outline, under Word's name for it (HF-254).
  "object.pictureBorder": "Picture border",
  "object.pictureBorder.short": "Border",
  // Crop mode's arrows move the kept area; with nothing cropped there is nothing
  // to move, and the key says so rather than doing nothing (`docs/104` HF-106).
  "object.crop.nothingToMove": "Crop an edge first — the arrow keys then move the cropped area",
  // The text box body's Apply refuses a bad inset rather than doing nothing (HF-253).
  "object.textBoxBody.invalid": "Each inset must be a number of inches, 0 or more",
  // Moving an in-line object to another place in the text (`docs/109` UX-OB-02):
  // the drag's hint, and Word's F2 "Move to where?" / Shift+F2 "Copy to where?".
  // `{key}` is the copy key's name for this keyboard (Ctrl, or ⌥ on a Mac).
  "object.dragInText.hint": "Release to drop it at the marker · hold {key} to copy · Esc cancels",
  "object.moveTo.menu": "Move to…",
  "object.moveTo.prompt": "Move to where? Put the insertion point where it should go, then press Enter. Esc cancels.",
  "object.copyTo.prompt": "Copy to where? Put the insertion point where the copy should go, then press Enter. Esc cancels.",
  "object.moveTo.cancelled": "Move cancelled",
  "object.moved": "Moved",
  "object.copied": "Copy placed",
  // The gutter's REORDER gesture (`docs/141` §4.2.3). The tooltip is on the band
  // once it is the selection — the moment it becomes a handle — and the two
  // announcements name both ends of the move, because the live region is the
  // only channel a pointer gesture has and "moved" alone does not say where to.
  // ---- Proofing (docs/114, docs/146, ADR-042) --------------------------------
  //
  // The two switches' announcements moved here from `main.js` when the wiring was
  // extracted into `proofing_chrome.mjs`: they were four English literals against
  // that file's unrouted-string ceiling and are now four keys, so the ceiling came
  // down by four rather than the debt moving to a new file.
  "proofing.spellCheckOn": "Spell check on",
  "proofing.spellCheckOff": "Spell check off",
  "proofing.grammarCheckOn": "Grammar check on",
  "proofing.grammarCheckOff": "Grammar check off",
  // "Proofing languages" — Word's File ▸ Options ▸ Language table, which is the
  // surface this was designed from (`proof_languages.mjs` carries the comparison
  // and the three places it deliberately differs). The STATE strings read as facts
  // about the language rather than as instructions, because the first question a
  // reader has is "does this language get checked".
  "proofLanguages.command": "Proofing languages…",
  "proofLanguages.builtIn": "Built in",
  "proofLanguages.installedWords": "Installed · {count} extra words",
  "proofLanguages.notInstalled": "Not installed",
  "proofLanguages.installing": "Installing… {percent}%",
  "proofLanguages.install": "Install",
  "proofLanguages.remove": "Remove",
  "proofLanguages.cancel": "Cancel",
  "proofLanguages.installLanguage": "Install proofing for {language}",
  "proofLanguages.removeLanguage": "Remove proofing for {language}",
  "proofLanguages.cancelLanguage": "Cancel the {language} download",
  "proofLanguages.megabytes": "{size} MB",
  "proofLanguages.installedLanguage": "Proofing for {language} is installed",
  "proofLanguages.removedLanguage": "Proofing for {language} was removed",
  "proofLanguages.storageUnavailable":
    "This browser is not storing data for this page, so a language pack cannot be kept for next time.",
  // Every refusal `proof_packs.mjs` can return, as a sentence. The map from code
  // to key is in `proof_languages.mjs` and a guard asserts it is total: a refusal
  // with no sentence is a dialog that closes having said nothing (SKILL.md §10).
  "proofPack.refusal.hostRefused":
    "The site that provides this editor does not allow language packs to be downloaded.",
  "proofPack.refusal.manifest": "The pack's description could not be read, so nothing was installed.",
  "proofPack.refusal.schema": "That pack was built for a different version of this editor.",
  "proofPack.refusal.locale": "That pack is for a different language.",
  "proofPack.refusal.assetUrl":
    "The pack is hosted somewhere this editor is not allowed to download from.",
  "proofPack.refusal.tooLarge": "That pack is larger than the {limit} MB one language may use.",
  "proofPack.refusal.quota": "There is not enough room left in this browser to store the pack.",
  "proofPack.refusal.network": "The pack could not be downloaded. Check the connection and try again.",
  "proofPack.refusal.cancelled": "The download was cancelled, and nothing changed.",
  "proofPack.refusal.digest": "The download did not match its checksum, so it was discarded.",
  "proofPack.refusal.bounds": "The download was not the size it declared, so it was discarded.",
  "proofPack.refusal.format": "The pack holds data this editor cannot read.",
  "proofPack.refusal.selfTest": "The pack failed its own check, so it was not switched on.",
  "proofPack.refusal.storage":
    "This browser refused to store the pack. Whatever was installed before is still in use.",
  "proofPack.refusal.notPublished": "There is no proofing pack for this language yet.",
  "proofPack.refusal.notInstalled": "There is no installed pack to remove for that language.",
  // The SDK's own refusals. `checkDocument` has no honest implementation today —
  // the scan is windowed and body-only, and whole-document enumeration is an engine
  // export that does not exist — so it refuses BY NAME rather than resolving to an
  // empty result, which would report a clean document by not looking at it.
  "proofDocument.notAvailable":
    "Checking the whole document at once is not available yet. Proofing checks the pages you are looking at, so scroll through the document to check all of it.",
  "proofDocument.outsideWindow":
    "That part of the document is not on screen, so it has not been checked yet.",
  "proofDocument.notConfigured": "Proofing is not set up for this document.",
  "proofDocument.disposed": "Proofing has been shut down for this document.",
  "table.moveSubmenu": "Move",
  "table.dragToMoveRow": "Drag to move this row",
  "table.dragToMoveColumn": "Drag to move this column",
  "table.rowMoved": "Row {from} moved to position {to}",
  "table.columnMoved": "Column {from} moved to position {to}",
  // ---- Four capabilities the engine had and the product could not reach -----
  //
  // The ¶ button (`docs/153` `shell.formatting-marks`), the measurement-unit
  // preference (`shell.measurement-units`), the border line style
  // (`table.border-width-style`) and Restrict Editing (`review.restrict-editing`).
  // Every ROW LABEL in the controls themselves is markup and carries its English
  // beside its key; what is here is only what a SCRIPT composes.

  // One sentence for every control that needs an open document and does not have
  // one. It was an unrouted English literal in six places in `main.js` before
  // this; these four capabilities route it, which is the direction the
  // unrouted-string ratchet only moves in.
  "command.needsDocument": "Open a document first",

  // The ¶ command reads as a switch, like `view.compactRibbon` and
  // `tools.smartQuotes`, so a reader sees the state without opening anything.
  "formattingMarks.commandOn": "Formatting marks: on",
  "formattingMarks.commandOff": "Formatting marks: off",
  // ONE composed pattern for all five individual switches rather than five
  // sentences: the mark's own name is already a declared markup string, so this
  // costs one catalogue entry instead of five and a translator sees the shape once.
  "formattingMarks.markSwitch": "{mark}: {state}",
  "formattingMarks.stateOn": "on",
  "formattingMarks.stateOff": "off",

  // The unit names a chooser offers. The engine supplies the id, the suffix, the
  // display precision and the spinner step; the NAME is chrome, and ONLYOFFICE's
  // own `cmbUnit` rows are names too ("Centimeter", "Point", "Inch").
  "units.cm": "Centimetres",
  "units.mm": "Millimetres",
  "units.inch": "Inches",
  "units.point": "Points",
  "units.pica": "Picas",
  "units.command": "Measurement units: {unit}",
  // A typed distance the engine could not read, said when the reader commits it
  // (`measurement_units.mjs` `refusalText`). `{field}` is the field's own label.
  "units.refused.field": "Measurement",
  "units.refused.empty": "{field}: type a number.",
  "units.refused.notANumber": "{field}: “{value}” is not a number. Type digits only, with no thousands separator.",
  "units.refused.unknownUnit": "{field}: “{value}” ends in a unit this editor does not know. Type the number alone, or end it with cm, mm, in, pt or pi.",
  "units.refused.tooPrecise": "{field}: “{value}” has more digits than a measurement can hold.",
  "units.refused.outOfRange": "{field}: “{value}” is too large for a measurement.",
  "units.refused.unreadable": "{field}: “{value}” could not be read as a measurement.",

  // The border line style's palette and context-menu row. The ellipsis is this
  // chrome's convention for a row that opens a control rather than acting.
  "table.borderStyleCommand": "Border line style\u2026",

  // Restrict Editing. The dialog's own strings are markup; these are the command
  // row, the two things the status bar says, and the one disabled reason.
  "protect.command": "Restrict editing\u2026",
  "protect.applied": "Editing restricted to: {level}",
  "protect.removed": "Editing is no longer restricted",
  // The SECOND axis applied on its own \u2014 `w:edit="none" w:formatting="1"`, which
  // is what Word writes when an author ticks the formatting box and no editing
  // box. Its own sentence rather than `protect.applied` with a level
  // interpolated, because the level would read "No restriction" and the sentence
  // would contradict itself.
  "protect.appliedFormatting": "Formatting is now limited to this document's unlocked styles",
  // Not `protect.enforce.disabled`: `protect.enforce` is a markup key, and a
  // catalogue entry that looks like a child of another key invites `isDeclared`'s
  // plural-family prefix rule to answer for it.
  "protect.enforceOff": "Choose a restriction before applying one",

  // ---- A PARTICIPANT'S GRANT, AND THE ROOM'S REFUSALS ----------------------
  //
  // `session_access.mjs` routes two code families to these, and it is ONE table
  // for both halves of the same question: the sentence a control carries while it
  // is disabled, and the sentence shown when the engine or the relay refuses the
  // gesture anyway. Two tables would drift, and a reader told two different
  // things about one permission learns that neither is trustworthy.
  //
  // The five `session.*` keys are `casual_doc_edit::access::AccessRefusal`'s own
  // classes. The engine writes an English fallback beside each code; these are
  // the translated ones, and they are what a reader actually sees. They say what
  // the participant MAY do rather than which operation was refused — naming the
  // operation tells them what the chrome happened to send.
  "session.readOnly": "You have read-only access to this document",
  "session.commentsOnly": "You can add comments to this document, but not change it",
  "session.suggestionsOnly": "You can comment and suggest changes, but not change the document directly",
  "session.reviewOnly": "You can accept or reject other people's changes and add comments, but not change this document yourself",
  "session.noProtectionChange": "You are not allowed to change how this document is protected",
  // `AccessChangeRefusal`'s five, which answer the OTHER question the engine's
  // access vocabulary asks: not "may this gesture touch the document" but "may
  // this participant change the room". They are in this one table for the reason
  // the block header gives, and `session.noAccessChange` is also what `ODC-7011`
  // resolves to — the wire's answer is deliberately undetailed, and the sentence
  // a reader sees must not differ depending on whether the chrome knew first.
  //
  // Four of the five are unreachable from an honest chrome: the surface is absent
  // without `manageAccess`, it omits the reader's own row, and it offers no role
  // outside the ceiling the relay reported. They are translated anyway, because
  // "unreachable from our chrome" is not "unreachable", and an untranslated
  // sentence is what a reader of eighteen languages would get the day it is.
  "session.noAccessChange": "You are not allowed to change what other people may do with this document",
  "session.ownAccessUnchangeable": "You cannot change your own access to this document",
  "session.aboveGrantCeiling": "That is more than this person was given access to do",
  "session.aboveOwnAccess": "You cannot give somebody access you do not have yourself",
  "session.notAParticipant": "That person is not in this shared document",
  // A grant this build could not read — an unknown capability name, or a
  // participant number that is not one. The chrome narrows to read-only and SAYS
  // SO, because silently granting less is a bug that looks like a working
  // read-only mode (`casual-doc-wasm`'s own reasoning, one layer up).
  "session.grantUnreadable": "This session's permissions could not be read, so the document is open read-only",
  // The connection itself, which is the chrome's to report because the engine
  // never sees one. An eviction IS a failed write, so there is no socket left to
  // refuse down and no wire code to route — see `session_access.mjs`'s
  // `CONNECTION_LOST`. It says only what is known: the connection is gone, so
  // this copy may be behind. It deliberately does not guess at why.
  "session.connectionLost": "The connection to this shared document was lost, so this copy may be behind",
  // The DOCUMENT's own `w:documentProtection`, which is the other authority and
  // asks the same of everyone (ADR-052). Reachable on a document opened from a
  // file with no room at all, and until these keys existed every one of them
  // reached a reader of all nineteen locales in the engine's English. They name
  // the DOCUMENT rather than the reader, which is the distinction `access.rs`
  // insists on: a host showing "you have read-only access" for a protected FILE
  // sends the reader to argue with the wrong party.
  "document.protectedReadOnly": "This document is protected against changes",
  "document.protectedCommentsOnly": "This document is protected: only comments can be added",
  "document.protectedTrackedChangesOnly": "This document is protected: changes must be tracked, and tracked changes cannot be accepted or rejected",
  "document.protectedFormsOnly": "This document is protected: only its form fields can be edited",
  // `w:formatting` and `w:locked` — the other axis, whose refusals say what is
  // still possible as well as what was refused, because a reader who is told only
  // "no" tries the same gesture again. Neither says secure, encrypted or
  // password-protected: `w:documentProtection` is plain-text XML and the standard
  // says in its own note that it "is not intended as a security feature", so a
  // sentence a reader would read as one would be a false claim in the place they
  // are most likely to believe it. The engine asserts that in
  // `every_protection_refusal_carries_a_distinct_routable_code_and_sentence`.
  "document.protectedFormatting": "This document is protected: its formatting can only be changed by applying one of its styles",
  "document.protectedStyleLocked": "This document is protected and that style is locked, so it cannot be applied",
  // The `ODC-7xxx` collaboration family (`docs/20`), every row of it. Each says
  // what the reader should DO, because that is the difference between the codes:
  // one invites a retry, one asks them to copy their work out, one is terminal.
  "collab.conflict": "That one change could not be merged with everyone else's — try it again",
  "collab.protocolVersion": "This editor and the document's server speak different versions — reload the page",
  "collab.notAuthorised": "This session is not authorised to open this document",
  "collab.readOnly": "You are reading this shared document and cannot change it",
  "collab.notSaving": "This shared session cannot save right now — copy your work out before closing",
  "collab.tooFarBehind": "This session fell too far behind to catch up, and unsent changes were lost — reload to continue",
  "collab.malformed": "A message from the server could not be read, so it was ignored",
  "collab.idCollision": "A change from someone else named something this copy already has, so it was not applied",
  "collab.staleBase": "The document moved on while that change was in flight — it is being sent again",
  "collab.roomFull": "This document already has as many people editing as it allows — try again shortly",
  // The three connection states `collab_transport.mjs` reports, which are a
  // different question from the refusals above: a refusal explains one message,
  // and these describe where the reader's typing is going right now. Each names
  // the consequence rather than the mechanism — "not shared yet" is what a
  // reader can act on, "the socket closed" is not. `reconnecting` covers a first
  // connection that has not landed as well as a lost one, because to the reader
  // they are the same fact.
  "collab.connected": "Shared — your changes are reaching everyone in this document",
  "collab.reconnect": "Reconnect",
  "collab.standalone":
    "This document is not shared, so there is nothing to reconnect to",
  "collab.reconnecting": "Connection lost — reconnecting. Changes you make now are not shared yet",
  "collab.stopped": "Not connected to this shared document — your changes are staying on this device",
  // Managing what OTHER people may do — the rights surface, which exists only in
  // a shared session (`session_rights.mjs`). The ROLE names are the vocabulary
  // `casual_doc_edit::access`'s presets already use, deliberately: a second set
  // of words for one permission model is how two parts of a product come to
  // disagree about what somebody is.
  "rights.command": "Manage access…",
  "rights.role.viewer": "Viewer",
  "rights.role.commenter": "Commenter",
  "rights.role.suggester": "Suggester",
  "rights.role.reviewer": "Reviewer",
  "rights.role.editor": "Editor",
  "rights.role.owner": "Owner",
  // A grant the host composed by hand rather than from a preset, which `143` §10
  // exists to allow. Named rather than rounded to the nearest role: a person
  // shown a role they do not hold has been told the wrong thing about their own
  // access.
  "rights.role.custom": "Custom",
  // A NUMBER, because that is all the protocol carries — `Identity` is opaque to
  // this engine and presence reports a participant number and nothing else.
  // Inventing a display name would be inventing an identity system.
  "rights.participant": "Participant {number}",
  // Outside a room there is no other participant whose permissions could change,
  // so the command ships DISABLED WITH THIS REASON rather than absent — the
  // absent thing is a session, not a permission. The same shape as
  // `collab.standalone`, which is the sibling case.
  "rights.standalone": "This document is not shared, so there are no other people's permissions to change",
  "rights.notSent": "Not connected to this shared document, so the access change was not sent",
  // What the RELAY decided, reported when its answer arrives rather than when the
  // request leaves. Announcing a success at the point of asking would be
  // reporting the request and calling it the result.
  "rights.applied": "{who} is now {role}",
  // The persistent access indicator (`access_badge.mjs`), which answers "what can
  // I do with this document" from first paint, in every mode — including the
  // standalone and embedded ones, where it is the only thing that says so.
  //
  // The LEVEL and the SOURCE are two strings because they answer two questions a
  // reader asks in sequence: what can I do, and who said so. Naming the wrong
  // authority is the failure the `session.*` / `document.*` split exists to
  // prevent — "this document is protected" sends a read-only guest to look at the
  // wrong thing.
  "access.level.full": "Full access",
  "access.level.suggest": "Suggesting only",
  "access.level.comment": "Comments only",
  "access.level.read": "Read only",
  // The one state where a reader may write and may not reformat — `w:formatting`
  // with no editing restriction. Short enough for the footer badge at phone
  // width, where the second half of the badge is shed.
  "access.level.noFormatting": "No formatting changes",
  // Each source names WHERE the limit was decided, in the reader's terms rather
  // than the mechanism's: "set when this document was opened" is something a
  // reader can act on (ask whoever opened it), "the container grant withheld
  // edit" is not.
  "access.source.engine": "this document cannot be edited here",
  "access.source.document": "set in the document itself",
  "access.source.shared": "in this shared document",
  "access.source.host": "set when this document was opened",
  "access.source.local": "on this device, where you are the only authority",

  // The chart panel (`chart_data.mjs`): a chart's data grid, its type, its
  // title and its legend. Word's Edit Data and Docs' chart editor, one panel.
  // The two `chart.refused.*` sentences route the engine's coded refusals
  // (`casual-doc-wasm/src/chart.rs`) so a non-English reader is told why in
  // their own language.
  "chart.panelTitle": "Chart",
  "chart.panelIntro": "Changes apply to the chart as you make them.",
  "chart.close": "Close chart panel",
  "chart.typeHeading": "Chart type",
  "chart.kind.column": "Clustered column",
  "chart.kind.bar": "Clustered bar",
  "chart.kind.line": "Line",
  "chart.kind.area": "Area",
  "chart.kind.pie": "Pie",
  "chart.kind.doughnut": "Doughnut",
  "chart.kind.scatter": "Scatter",
  "chart.titleField": "Title",
  "chart.legend.none": "None",
  "chart.legend.right": "Right",
  "chart.legend.top": "Top",
  "chart.legend.left": "Left",
  "chart.legend.bottom": "Bottom",
  "chart.legend.topRight": "Top right",
  "chart.dataHeading": "Data",
  "chart.dataHint": "Rows are categories and columns are series. Paste cells from a spreadsheet to fill many at once.",
  "chart.categories": "Categories",
  "chart.xValues": "X values",
  "chart.seriesNameLabel": "Name of series {n}",
  "chart.rowNameLabel": "Name of row {n}",
  "chart.cellLabel": "{series}, {label}",
  "chart.seriesFallback": "Series {n}",
  "chart.rowFallback": "Row {n}",
  "chart.newSeries": "Series {n}",
  "chart.newCategory": "Category {n}",
  "chart.removeSeries": "Remove series {name}",
  "chart.removeRow": "Remove row {name}",
  "chart.lastSeries": "A chart needs at least one series",
  "chart.lastRow": "A chart needs at least one row",
  "chart.addRow": "Add row",
  "chart.addSeries": "Add series",
  "chart.rowLimit": "A chart can hold at most {n} rows",
  "chart.seriesLimit": "A chart can hold at most {n} series",
  "chart.notANumber": "“{value}” in row {row} of “{series}” is not a number. Type a number like 4.3, or clear the cell to leave a gap.",
  "chart.pasteClipped": "Cells that did not fit and were left out: {count}",
  "chart.editData": "Edit data",
  "chart.editDataTitle": "Edit the chart's data, type, title and legend",
  "chart.replacesWorkbook": "This chart came from the file you opened. Changing its data replaces the chart's embedded workbook with one holding just the data shown here.",
  "chart.refused.partial": "This chart uses features this editor cannot rewrite yet, so its data is shown read-only.",
  "chart.refused.combo": "This chart combines more than one chart type, and its data cannot be edited here yet.",
  // The Chart tab, the Chart settings panel and the Chart Data dialog
  // (`chart_surface.mjs`, `chart_panel.mjs`, `chart_data.mjs`).
  "chart.kind.columnStacked": "Stacked column",
  "chart.kind.columnPercent": "100% stacked column",
  "chart.kind.lineMarkers": "Line with markers",
  "chart.kind.lineStacked": "Stacked line",
  "chart.kind.linePercent": "100% stacked line",
  "chart.kind.barStacked": "Stacked bar",
  "chart.kind.barPercent": "100% stacked bar",
  "chart.kind.areaStacked": "Stacked area",
  "chart.kind.areaPercent": "100% stacked area",
  "chart.kind.scatterSmooth": "Scatter with smooth lines",
  "chart.familyColumn": "Column",
  "chart.familyLine": "Line",
  "chart.familyPie": "Pie",
  "chart.familyBar": "Bar",
  "chart.familyArea": "Area",
  "chart.familyScatter": "X Y (scatter)",
  "chart.familyOther": "Other",
  "chart.titleState.none": "None",
  "chart.titleState.above": "Above chart",
  "chart.titleState.overlay": "Centered overlay",
  "chart.legendOverlay": "Show the legend over the chart",
  "chart.labels.none": "None",
  "chart.labels.show": "Show values",
  "chart.labels.outsideEnd": "Outside end",
  "chart.labels.insideEnd": "Inside end",
  "chart.labels.center": "Center",
  "chart.labels.insideBase": "Inside base",
  "chart.labels.top": "Above",
  "chart.labels.bottom": "Below",
  "chart.labels.left": "Left",
  "chart.labels.right": "Right",
  "chart.noAxes": "This chart type has no axes",
  "chart.axis.horizontal": "Horizontal axis",
  "chart.axis.vertical": "Vertical axis",
  "chart.gridlines.horizontal": "Horizontal gridlines",
  "chart.gridlines.vertical": "Vertical gridlines",
  "chart.elements": "Chart elements",
  "chart.element.title": "Chart title",
  "chart.element.legend": "Legend",
  "chart.element.labels": "Data labels",
  "chart.element.axes": "Axes",
  "chart.element.gridlines": "Gridlines",
  "chart.style": "Chart style",
  "chart.settings": "Chart settings…",
  "chart.dataTitle": "Chart data",
  "chart.closeData": "Close chart data",
  "chart.dataUndoNote": "Changes apply to the chart as you make them. Each change can be undone.",
  "chart.done": "Done",
  "chart.axis.auto": "Auto",
  "chart.axis.minimum": "Minimum",
  "chart.axis.maximum": "Maximum",
  "chart.axis.reverse": "Values in reverse order",
  "chart.readOnly": "This chart cannot be changed",
  "chart.typeLabel": "Type",
  "chart.axis.verticalHeading": "Vertical axis",
  "chart.axis.horizontalHeading": "Horizontal axis",
  "chart.dataSummary": "Series: {series} · Categories: {rows}",
  "chart.editDataEllipsis": "Edit data…",
  "chart.reason.viewing": "Turn on Editing to change this chart",
  "chart.reason.suggesting": "Chart changes cannot be tracked in Suggesting mode",
  "chart.reason.noChartSelected": "Select a chart to use these tools",
  "chart.defaultTitle": "Chart Title",
  "chart.palette.colorful": "Colorful",
  "chart.palette.mono1": "Monochrome 1",
  "chart.palette.mono2": "Monochrome 2",
  "chart.palette.mono3": "Monochrome 3",
  "chart.palette.mono4": "Monochrome 4",
  "chart.palette.mono5": "Monochrome 5",
  "chart.palette.mono6": "Monochrome 6",
});
