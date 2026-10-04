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
  // -- Text width (`docs/154` §5.1, ADR-048) ---------------------------------
  // Four steps for one measure, each labelled by what it does rather than by a
  // number: "Wide" tells a reader nothing and "80" tells them nothing until they
  // know it is WCAG 2.1 SC 1.4.8's maximum, which is what the `.title` sentences
  // are for. The `.row` sentences and `textWidth.reading.short` are declared in
  // `editor.html` beside the markup that carries them — `build-locale.mjs`
  // refuses a key declared in both places — and `reflow_view.test.mjs` asserts
  // every step has all four, so the split cannot rot into a half-labelled step.
  "textWidth.command": "Text width",
  "textWidth.narrow.short": "Narrow",
  "textWidth.fit.short": "Paper",
  "textWidth.full.short": "Full",
  "textWidth.narrow.title": "Narrow text: about 55 characters a line.",
  "textWidth.reading.title":
    "Reading width: 80 characters a line, which is the widest WCAG 2.1 SC 1.4.8 allows a block of text to be.",
  "textWidth.fit.title": "As wide as this document's own text column, so no line is longer than on paper.",
  "textWidth.full.title": "As wide as the window, however wide the window is.",
  "textWidth.narrow.command": "Text width: Narrow",
  "textWidth.reading.command": "Text width: Reading",
  "textWidth.fit.command": "Text width: Paper",
  "textWidth.full.command": "Text width: Full",
  "textWidth.pagedWithheld":
    "Text width applies in reflow. On pages the measure is the document's own, so turn Reflow on to choose one.",
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
  // REPLACES `compare.notTrackedChanges`, which said "The differences are listed
  // here, not written into a document — they cannot be accepted or rejected."
  // That was true of the change list and is false of ADR-061: the differences are
  // written into the open document as tracked changes now, and the key's own name
  // said the opposite. Renamed rather than reworded in place, so a maintainer
  // reading `notTrackedChanges` cannot be told the opposite of what it carries.
  //
  // Said BEFORE a file is picked, because it is the warning that matters: Word and
  // Google Docs build a merged THIRD document; we mutate the one on screen, which
  // is ONLYOFFICE's answer (`docs/158` §2.2).
  "compare.writesTrackedChanges":
    "The differences are written into this document as tracked changes, which you can then accept or reject.",
  // The answer, FIRST and as a sentence about the document rather than a number
  // about the panel. The owner's report on the previous shape was that a count
  // told them nothing: "i cant even see what is being changed".
  "compare.marked": "{count} differences are now tracked changes in this document.",
  // And how to walk them. Review's own next/previous navigate the revisions the
  // comparison just wrote, which is the route that cannot land on the wrong
  // paragraph — unlike a click on a list entry, whose anchor belongs to the
  // comparison's throwaway re-import (`compare_documents.mjs`, "still blocked").
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
  // NOT "No differences". A document whose drawings this build has no typed
  // comparison for can produce zero changes AND a loss report, and claiming the
  // two files agree about something the engine never looked at is the silent loss
  // `SKILL` §12 forbids. The finding wins and the sentence narrows.
  "compare.identicalPartly": "No differences in what could be compared.",
  // What the comparison could NOT compare, aggregated by the engine — one row per
  // construct with a count, so forty thousand drawings are one line. Shown before
  // the changes themselves: a reader deciding whether to trust the list needs its
  // limits first.
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
  // CORRECTED 2026-10-04, English and all eighteen translations. This read "This
  // comparison did not finish, so the list below is incomplete", on a comment in
  // `compare_documents.mjs` claiming `complete: false` happens only on a cancelled
  // job. `record.rs` says the opposite in as many words — `complete` is "False
  // whenever `findings` is non-empty" — so a perfectly ordinary comparison that
  // met one construct this build cannot compare in detail was telling the reader
  // it had broken. Measured on bolding one word in the demo document: four
  // differences found, and "did not finish" printed above them. A comparison that
  // finished and skipped something is not a comparison that did not finish, and
  // the findings list directly below already names what was skipped.
  "compare.partial":
    "Some of what differs could not be characterised; the list below says which.",
  // WHAT A ROW IS ABOUT when there is no text and no typed field to name — the
  // bracketed convention ONLYOFFICE uses, where theirs reads `<Image>`, `<Shape>`,
  // `<Chart>` or `<Equation>`.
  //
  // Ours can only be as specific as the sidecar, and `family_of` in
  // `casual-doc-diff/src/job.rs` maps a table row or cell to `table` and
  // EVERYTHING else to `block` — so a deleted image-only paragraph is `<Block>`
  // here and `<Image>` there. That gap is the engine's, it is reported as the
  // engine's, and it is not papered over by printing `<Image>` on the grounds that
  // images are the commonest untexted block: a plausible guess presented as a fact
  // is what this repository has published by accident twice.
  //
  // All twelve families, enumerated rather than defaulted (SKILL §9.3), and
  // `compare_documents.test.mjs` fails if the engine grows a family with no entry.
  "compare.object.block": "<Block>",
  "compare.object.text": "<Text>",
  "compare.object.formatting": "<Formatting>",
  "compare.object.style": "<Style>",
  "compare.object.table": "<Table>",
  "compare.object.object": "<Object>",
  "compare.object.section": "<Page setup>",
  "compare.object.definition": "<Definition>",
  "compare.object.resource": "<Resource>",
  "compare.object.comment": "<Comment>",
  "compare.object.review": "<Tracked change>",
  "compare.object.metadata": "<Document property>",
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
  "compare.kind.property": "Property changed",
  // Where a change is, when it is not in the body. Saying "in the body" on every
  // row of a body-only comparison would be noise, so the body says nothing.
  "compare.story.header": "in the header of section {section}",
  "compare.story.footer": "in the footer of section {section}",
  "compare.story.footnote": "in footnote {number}",
  "compare.story.endnote": "in endnote {number}",
  "compare.story.comment": "in a comment",
  "compare.story.definitions": "in the document's definitions",
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
  "versionHistory.row.grew": "{size} larger",
  "versionHistory.row.shrank": "{size} smaller",
  "versionHistory.row.by": "by {name}",

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
  "versionHistory.headNotDeletable":
    "This is the current version — it is the only one that still describes the document",
  // CORRECTED 2026-10-01. This read "Comparing one version with another is not
  // built yet", and the comment above it said the structural diff was `docs/140`
  // H3 and not built. The diff was built the whole time — `casual-doc-diff` plus
  // `casual-doc-wasm/src/diff.rs` — and the PANEL was not, which `docs/140`
  // §8-13 already said. Show changes is live now, and this sentence is what it
  // says when the surface that shows a comparison is not available at all: an
  // embedded editor composed without the Compare panel, where the capability
  // genuinely is not there. `headNotComparable` is the other refusal.
  "versionHistory.action.showChangesUnavailable":
    "Comparing versions is not available in this editor",
  "versionHistory.headNotComparable":
    "This is the current version — comparing it with itself would show nothing",

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

  // The border line style's palette and context-menu row. The ellipsis is this
  // chrome's convention for a row that opens a control rather than acting.
  "table.borderStyleCommand": "Border line style\u2026",

  // Restrict Editing. The dialog's own strings are markup; these are the command
  // row, the two things the status bar says, and the one disabled reason.
  "protect.command": "Restrict editing\u2026",
  "protect.applied": "Editing restricted to: {level}",
  "protect.removed": "Editing is no longer restricted",
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
  // A grant this build could not read — an unknown capability name, or a
  // participant number that is not one. The chrome narrows to read-only and SAYS
  // SO, because silently granting less is a bug that looks like a working
  // read-only mode (`casual-doc-wasm`'s own reasoning, one layer up).
  "session.grantUnreadable": "This session's permissions could not be read, so the document is open read-only",
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

});
