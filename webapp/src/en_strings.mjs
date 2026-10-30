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
  "collab.retainedDocuments": "Retained local documents",
  "collab.downloadDocument": "Download document {number}",

  "collab.share": "Share",
  "collab.shareLink": "Link to this shared document",
  "collab.close": "Close",
  "collab.participant": "Participant {number}",
  // The avatar strip (`presence_strip.mjs`, `docs/109` HF-330). Until a host
  // identity contract exists a participant is the relay's number, and the chrome
  // says so rather than letting "P2" pass for a name.
  // @mention (`docs/175` §7): the picker over the host's directory, and what the
  // host answered when told who was mentioned.
  "mention.listLabel": "People matching “{query}”",
  "mention.noMatch": "No one matches “{query}”",
  "mention.notified": "{names} will be told about this comment",
  "mention.needsAccess": "{names} can’t open this document",
  "mention.declined": "{names} won’t be told about this comment",
  "collab.presence.label": "People in this document",
  "collab.presence.initials": "P{number}",
  "collab.presence.unnamed": "{name} — numbered by the room, because this host does not share names",
  "collab.presence.unverified": "{name} — unverified: this name was given by the person, not confirmed by the host",
  "collab.presence.duplicate": "{name} ({count})",
  "collab.presence.you": "You ({name})",
  "collab.presence.youAlone": "You",
  "collab.presence.goTo": "{name}. Go to their cursor",
  "collab.presence.more": "+{count}",
  "collab.presence.moreLabel.one": "{count} more person in this document",
  "collab.presence.moreLabel.other": "{count} more people in this document",
  "collab.presence.everyone.one": "{count} person in this document",
  "collab.presence.everyone.other": "{count} people in this document",
  "collab.presence.joined": "{names} joined.",
  "collab.presence.left": "{names} left.",
  "collab.presence.noCaret": "{name} has no cursor on a laid-out page yet.",
  "collab.shareCopied": "Share link copied.",
  "collab.shareCopyManually": "Copy this link to invite someone.",
  "collab.shareHostRequired": "Your host must configure document sharing.",
  "collab.shareDocumentChanged": "The document changed while creating the room. Share again to include your latest edits.",
  "collab.shareInvalid": "The host returned an invalid share link.",
  "collab.recoveryNeeded": "Your changes are kept on this device while the latest shared copy is loaded.",
  "collab.recoveryFailed": "The latest shared copy could not be loaded. Your changes are still kept on this device — retry from Review.",
  "collab.recoveryNotNeeded": "This document has no shared recovery to retry.",
  "collab.recoveryPending": "Shared document recovery is already in progress.",
  "collab.noRetainedDocuments": "There are no retained local documents to download.",
  "collab.snapshotInvalid": "The recovery snapshot is invalid or older than the required revision.",
  "collab.recoveryRetained": "Replayed {replayed} local edits. {retained} uncertain edits remain in retained document backups; download them from Review.",
  "collab.recoveryComplete": "Recovered the shared document and replayed {replayed} local edits.",
  "collab.recover": "Retry shared document recovery",
  "collab.downloadRetained": "Download retained local documents",
  "collab.checkpointFailed": "The host could not save the latest collaboration checkpoint.",

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
  // References ▸ Notes ▸ the Next / Previous footnote and endnote flyout.
  "menuGroup.goToNote": "Go to a note",
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
  // Review ▸ Display for review, the flyout holding the four review views.
  "menuGroup.displayForReview": "Display for review",
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
  // Word's status bar while text is selected: "23 of 1,204 words" (HF-051).
  // Plural on the document's total, which is the noun the sentence ends on.
  "status.wordsOfTotal.one": "{selected} of {count} word",
  "status.wordsOfTotal.other": "{selected} of {count} words",
  // Why Accept and Reject are unavailable in a container that may not edit
  // (HF-199): deciding a tracked change writes the document, which is editing.
  "review.decision.needsEdit": "Accepting or rejecting a change edits the document, and this editor may only comment here.",
  // Page borders (HF-193): the palette's label, and what the dialog says.
  "pageBorders.command": "Page borders…",
  "pageBorders.noSection": "This document has no page setup to carry a border.",
  "pageBorders.applied": "Page borders applied",
  "pageBorders.removed": "Page borders removed",
  // Check accessibility (HF-064): the command, the summary, and each rule's
  // title and fix, written as Word's checker words them.
  "a11yCheck.command": "Check accessibility…",
  "a11yCheck.summary": "Errors: {errors} · Warnings: {warnings}",
  "a11yCheck.none": "No accessibility issues found.",
  "a11yCheck.severity.error": "Error",
  "a11yCheck.severity.warning": "Warning",
  "a11yCheck.goTo": "Go to",
  "a11yCheck.goToNamed": "Go to: {name}",
  "a11yCheck.goToUnavailable": "That item cannot hold the cursor.",
  "a11yCheck.where.picture": "Picture {n}",
  "a11yCheck.where.table": "Table {n}",
  "a11yCheck.where.levels": "level {from} to level {to}",
  "a11yCheck.rule.missingAlt": "Missing alternative text",
  "a11yCheck.fix.missingAlt": "Describe the picture in its properties, so a screen reader can say what it shows.",
  "a11yCheck.rule.tableHeader": "Missing table header",
  "a11yCheck.fix.tableHeader": "Mark the first row as a header row in Table properties, so each cell is announced with its column.",
  "a11yCheck.rule.headingSkip": "Skipped heading level",
  "a11yCheck.fix.headingSkip": "Use heading levels in order, so the outline reads as a structure.",
  "a11yCheck.rule.mergedCells": "Merged cells",
  "a11yCheck.fix.mergedCells": "Merged cells are hard to follow with a screen reader; keep the grid regular where you can.",
  "a11yCheck.rule.emptyRow": "Blank table row",
  "a11yCheck.fix.emptyRow": "Remove empty rows used for spacing; a screen reader announces each one.",
  "a11yCheck.rule.lowContrast": "Hard-to-read text contrast",
  "a11yCheck.fix.lowContrast": "Darken the text or lighten what is behind it, so it reaches 4.5:1 (3:1 for large text).",
  "a11yCheck.where.contrast": "“{text}” — contrast {ratio}:1",
  // Content controls (`109` RM-34, `content_controls.mjs`). The kind names and
  // the command labels are the Controls dropdown's markup; these are the rest.
  // The three placeholders are Word's own prompts, which a new empty control holds.
  "controls.placeholder.text": "Click or tap here to enter text.",
  "controls.placeholder.choose": "Choose an item.",
  "controls.placeholder.date": "Click or tap to enter a date.",
  "controls.insertCommand": "Insert content control: {kind}",
  "controls.inserted": "Content control inserted",
  "controls.removed": "Content control removed; its contents stay",
  "controls.filled": "Picture placed",
  "controls.propertiesApplied": "Content control properties changed",
  "controls.oneParagraph": "A content control wraps text in one paragraph. Select within one paragraph.",
  "controls.pictureFailed": "The placeholder picture could not be drawn in this browser.",
  "controls.notInControl": "Place the caret in a content control",
  "controls.notFillable": "This field is locked, or it is for another role to fill.",
  "controls.notInSuggesting": "Content controls cannot be changed in Suggesting mode",
  "controls.typeToFill": "Type in this content control to fill it.",
  "controls.chooseItem": "Choose an entry, or type one.",
  "controls.chooseDate": "Choose a date.",
  "controls.noEntries": "This list has no entries yet. Add them in its properties.",
  "controls.props.itemDisplay": "Display name",
  "controls.props.itemValue": "Value",
  "controls.props.removeItem": "Remove",
  "controls.props.datePreview": "For example: {example}",
  // Protect form and form roles (`form_roles.mjs`).
  "forms.protected": "Form protected: only its fields can be filled",
  "forms.unprotected": "Form protection removed",
  "formRoles.anyone": "Anyone",
  "formRoles.allFields": "Every field",
  "formRoles.colour": "Role colour",
  "formRoles.name": "Role name",
  "formRoles.remove": "Remove",
  "formRoles.newRole": "Role {n}",
  "formRoles.noFields": "No fields.",
  "formRoles.saved": "Form roles saved",
  "formRoles.fillingAs": "Filling the form as {role}",
  // The Word count command's label on the palette and the Review menu (HF-051).
  "wordCount.command": "Word count…",
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
  // The settings dialog, Remove, Add text and the table of figures (RM-25).
  "toc.styleLevel": "Contents level for {style}",
  "toc.noStylesChosen": "Give at least one style a contents level",
  "toc.caretInOtherTable": "The caret is in another kind of table; move it out to insert this one",
  "toc.figures.inserted": "Table of figures inserted",
  "toc.figures.command": "Insert table of figures…",
  "toc.remove.command": "Remove table of contents",
  "toc.removed": "Table of contents removed",
  "toc.update.command": "Update table…",
  "toc.addText.none": "Do not show in table of contents",
  "toc.addText.level": "Level {level}",
  "toc.addText.paletteNone": "Add text: do not show in table of contents",
  "toc.addText.paletteLevel": "Add text to table of contents: level {level}",
  "toc.addText.done": "Listed in the table of contents at level {level}",
  "toc.addText.removed": "No longer listed in the table of contents",
  "paragraph.caretRequired": "Place the caret in a paragraph",
  // Why Restart / Continue numbering and Insert ▸ Link are unavailable. They were
  // the palette rows' English literals while the buttons for the same commands
  // greyed out saying nothing (`control_reasons.mjs`); one entry each now, read
  // by both surfaces.
  "list.reason.notNumbered": "Place the caret in a numbered list",
  "list.reason.nothingToContinue": "There is no earlier numbered list to continue",
  "insert.reason.linkNeedsText": "Select text to add a link",
  "insert.chart": "Chart",
  "insert.chartOfType": "Insert chart: {type}",
  // A disabled control has one channel — its title — and it must say what to
  // DO, not repeat what the control would have done. This one used to borrow
  // the margin button's own label, which describes the action rather than the
  // precondition and leaves a reader who cannot click it none the wiser.
  "review.comment.needsRange": "Select the text you want to comment on",
  // The comment card's own controls (`review_comment_card.mjs`): editing the
  // thread's ROOT comment as well as its replies (RC-11), and the quote of the
  // commented text that stays — marked deleted — after the text itself is gone
  // (RC-12). Built when the card is drawn, never at boot (SKILL §11).
  "reviewCard.edit": "Edit",
  "reviewCard.editComment": "Edit comment",
  "reviewCard.editReply": "Edit reply",
  "reviewCard.commentText": "Comment text",
  "reviewCard.replyText": "Reply text",
  "reviewCard.save": "Save",
  "reviewCard.saveComment": "Save comment",
  "reviewCard.saveReply": "Save reply",
  "reviewCard.cancel": "Cancel",
  "reviewCard.quoteDeleted": "Text deleted",
  "reviewCard.commentUpdated": "Comment updated",
  "reviewCard.replyUpdated": "Reply updated",
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
  // Link to Previous, Remove and the page-number positions (HF-192, RM-23). The
  // link rows read as switches with their state, like the two variant toggles,
  // and each state is a whole sentence for the reason given above.
  "headerFooter.linkHeaderOn": "Header: linked to previous section",
  "headerFooter.linkHeaderOff": "Header: not linked to previous section",
  "headerFooter.linkFooterOn": "Footer: linked to previous section",
  "headerFooter.linkFooterOff": "Footer: not linked to previous section",
  "headerFooter.removeHeader": "Remove header",
  "headerFooter.removeFooter": "Remove footer",
  "headerFooter.firstSectionNoPrevious": "The first section has no previous section to link to",
  "headerFooter.noHeaderToRemove": "This page has no header to remove",
  "headerFooter.noFooterToRemove": "This page has no footer to remove",
  "headerFooter.headerLinked": "This section's header now follows the previous section's",
  "headerFooter.headerUnlinked": "This section's header is its own now; editing it leaves the previous section's alone",
  "headerFooter.footerLinked": "This section's footer now follows the previous section's",
  "headerFooter.footerUnlinked": "This section's footer is its own now; editing it leaves the previous section's alone",
  // `{items}` is the paste-loss family list ("bookmarks and comments"), the same
  // nouns a structured paste names, because it is the same deep copy.
  "headerFooter.copyLoss": "The copy could not carry {items}; they stay with the previous section's.",
  "headerFooter.headerRemoved": "Header removed from this section; Undo brings it back",
  "headerFooter.footerRemoved": "Footer removed from this section; Undo brings it back",
  "headerFooter.notTrackable": "Header and footer changes cannot be tracked yet; switch to Editing",
  "headerFooter.pageNumberInHeader": "Page number added to the header",
  "headerFooter.pageNumberInFooter": "Page number added to the footer",
  "headerFooter.pageNumberEdgeTable": "This band starts or ends with a table; put the caret where the number goes and use Page number at the cursor",
  "headerFooter.bandChangeFailed": "Could not change the header or footer: {message}",
  "pageNumber.topLeft": "Page number: top left",
  "pageNumber.topCenter": "Page number: top center",
  "pageNumber.topRight": "Page number: top right",
  "pageNumber.bottomLeft": "Page number: bottom left",
  "pageNumber.bottomCenter": "Page number: bottom center",
  "pageNumber.bottomRight": "Page number: bottom right",
  "menuGroup.pageNumbers": "Page numbers",
  // Word's "Page X of Y". The PATTERN is document text, split around its two
  // placeholders (`splitPageOfTotal`): keep `{page}` before `{pages}`.
  "pageNumber.pageOfTotal": "Page number: Page X of Y",
  "pageNumber.ofTotalPattern": "Page {page} of {pages}",
  "pageNumber.atCursor": "Page number at the cursor",
  // The band's own chrome: the marker that opens an absent band, the status
  // line while one is open, the label Word draws on the band ("Header
  // -Section 2-"), and the footer half of the bar's Link to previous title (the
  // header half is the markup's).
  "runningBand.addHeader": "+ Header",
  "runningBand.addFooter": "+ Footer",
  "runningBand.editHeader": "Edit the page header",
  "runningBand.editFooter": "Edit the page footer",
  "runningBand.editingHeader": "Editing the header — press Esc to return to the document",
  "runningBand.editingFooter": "Editing the footer — press Esc to return to the document",
  "runningBand.header": "Header",
  "runningBand.footer": "Footer",
  "runningBand.headerSection": "Header -Section {number}-",
  "runningBand.footerSection": "Footer -Section {number}-",
  "runningBand.headerSectionLinked": "Header -Section {number}- · Same as Previous",
  "runningBand.footerSectionLinked": "Footer -Section {number}- · Same as Previous",
  "runningBand.linkFooterTitle": "Show the previous section's footer here, or give this section its own",
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
  "menuGroup.contentControls": "Content controls",
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
  // A PDF's reconstruction (`docs/182` P2): grouped by what the IMPORT did,
  // because the original file is never written over.
  "findings.kind.pdfOmitted": "Not brought into the editable document",
  "findings.kind.pdfDegraded": "Rebuilt approximately",
  "findings.kind.pdfInferred": "Inferred from the page layout",
  "findings.where.pdfPage": "first seen on page {page}",
  // The disclosure `docs/182` and `docs/131` §16.2 ask for, said before the
  // findings: processed here, a reconstruction, the original untouched.
  "findings.pdf.title": "This is a reconstruction of a PDF",
  "findings.pdf.local": "It was converted on this device. Nothing was uploaded.",
  "findings.pdf.reconstruction": "Each page was rebuilt as text frames, pictures and shapes you can edit. It is not the PDF itself.",
  "findings.pdf.original": "Your original file is untouched. Saved as PDF, it is that file byte for byte until you edit; after an edit, your changes are added to it as an update where that is safe, and the PDF is regenerated where it is not.",
  "findings.pdf.none": "Nothing was inferred, approximated or left out.",
  "findings.feature.pdfActiveContent": "Scripts and actions in the PDF (never run)",
  "findings.feature.pdfAnnotations": "Annotations drawn on the PDF, such as comments and highlights",
  "findings.feature.pdfAttachments": "Files attached to the PDF",
  "findings.feature.pdfClip": "Clipping of drawn shapes",
  "findings.feature.pdfEncrypted": "Password protection",
  "findings.feature.pdfFontEmbedded": "Fonts carried in the PDF, used for its text",
  "findings.feature.pdfFontSubstituted": "Fonts replaced by a similar font",
  "findings.feature.pdfForms": "Form fields",
  "findings.feature.pdfImageClip": "Clipping of pictures",
  "findings.feature.pdfImageReencoded": "Pictures converted to PNG",
  "findings.feature.pdfImageUndecodable": "Pictures that could not be decoded",
  "findings.feature.pdfInvisibleText": "Invisible text, such as a scanned page's text layer",
  "findings.feature.pdfTextUnidentified": "Text whose characters the PDF does not identify",
  "findings.feature.pdfLayers": "Layers, shown as one",
  "findings.feature.pdfPageSize": "Page sizes outside the supported range",
  "findings.feature.pdfParagraphs": "Where paragraphs begin and end",
  "findings.feature.pdfPatterns": "Pattern and gradient fills",
  "findings.feature.pdfReadingOrder": "The reading order of text blocks",
  "findings.feature.pdfTags": "Accessibility tags",
  "findings.feature.pdfTextEffects": "Outline and clipping text effects",
  "findings.feature.pdfRotatedText": "Rotated text, shown upright",
  "findings.feature.pdfTransparency": "Transparency and blending",
  "findings.feature.pdfType3": "Characters drawn as pictures (Type 3 fonts)",
  "findings.feature.pdfVectorSplit": "Complex drawings split into several shapes",
  "findings.feature.pdfOcrConflict": "Text the scan reading disagreed with",
  "findings.feature.pdfOcrApplied": "Text read from scanned pages",
  "findings.feature.pdfOcrLimit": "Scanned pages not read because of a size limit",
  "findings.feature.pdfOcrLowConfidence": "Scanned text read with low confidence",
  "findings.feature.pdfOcrScanCovered": "Scanned ink covered under recognised text",
  "findings.feature.pdfOcrUnavailable": "Scanned pages not read (no text recognition available)",
  "findings.feature.pdfArtifacts": "Page decorations left out",
  "findings.feature.pdfBookmarks": "Bookmarks recreated from the PDF's links",
  "findings.feature.pdfColumns": "Columns inferred from the page layout",
  "findings.feature.pdfComments": "Comments recreated from PDF notes",
  "findings.feature.pdfDehyphenation": "Words rejoined across line breaks",
  "findings.feature.pdfFootnotes": "Footnotes inferred from the page layout",
  "findings.feature.pdfHeaderFooter": "Headers and footers inferred from repeated text",
  "findings.feature.pdfHeadings": "Headings inferred from text size",
  "findings.feature.pdfHyperlinks": "Links recreated from the PDF",
  "findings.feature.pdfLinksUnresolved": "Links whose destination could not be found",
  "findings.feature.pdfLists": "Lists inferred from bullets and numbers",
  "findings.feature.pdfStyles": "Styles inferred from the text's formatting",
  "findings.feature.pdfTables": "Tables inferred from the page layout",
  "findings.feature.pdfTablesSimplified": "Tables rebuilt with a simpler structure",
  "findings.feature.pdfVectorsOmitted": "Drawings left out",
  "findings.feature.pdfFootnoteMarkers": "Footnote marks renumbered",
  "findings.feature.pdfJustification": "Justified text inferred from line widths",
  "findings.feature.pdfPageBreaks": "Page breaks inferred from where pages end",
  "findings.feature.pdfFloats": "Pictures and drawings floated beside or behind their text",
  "findings.feature.pdfVectorsPlaced": "Drawings placed as shapes",
  // A PDF opened as a flowing document, and the characters the reader made
  // fit for editing (`docs/182` §4, §18.8).
  "findings.feature.pdfLayoutInferred": "Layout rebuilt as a flowing document",
  "findings.feature.pdfExactSaveFixedOnly": "Saved as PDF, a new file (only the exact page layout saves into the original)",
  "findings.feature.pdfLigatures": "Ligatures split into their letters",
  "findings.feature.pdfSymbolFonts": "Symbol-font characters mapped to standard characters",
  "findings.feature.pdfMarksComposed": "Accents joined to their letters",
  "findings.feature.pdfQuotes": "Curly quotes restored from the font",
  "findings.feature.pdfTableContinued": "Tables split by a page break joined",
  "findings.feature.pdfFlowingUnavailable": "Opened with its exact page layout: the flowing rebuild failed",
  "pdfImport.introFlowing": "Rebuilding the PDF as an editable document of paragraphs, headings, lists and tables, on this device.",
  "pdfImport.keepLayout": "Keep exact page layout",
  "pdfImport.keepLayoutHint": "Open every page as it is drawn, in positioned frames, instead of a flowing document.",
  "pdfFixed.command": "Keep exact page layout",
  "pdfFixed.title": "Opening {name} with its exact page layout",
  "pdfFixed.cancelled": "Reopening {name} was cancelled — the document you had open is still here",
  "pdfFixed.failed": "Could not reopen {name}: {reason}",
  "pdfFixed.notPdf": "Only a document opened from a PDF can be reopened with its exact page layout.",
  "pdfFixed.already": "This PDF is already open with its exact page layout.",
  "pdfReflow.alreadyFlowing": "This PDF is already open as a flowing document.",
  "findings.pdfFlowing.keepLayout": "To see each page exactly as the PDF draws it, choose Keep exact page layout.",
  "findings.pdfFlowing.save": "It saves as a Word document. Saved as PDF it is a new file; the original PDF is unchanged.",
  // Opening a PDF (`pdf_import.mjs`): the progress dialog over the staged import.
  "pdfImport.title": "Opening {name}",
  "pdfImport.openFailed": "Could not open {name}: {message}",
  "pdfImport.intro": "Rebuilding the PDF's pages as editable frames, on this device.",
  "pdfImport.reading": "Reading the file…",
  "pdfImport.progress": "Page {done} of {total}",
  "pdfImport.finishing": "Laying out the pages…",
  "pdfImport.cancelling": "Cancelling…",
  "pdfImport.cancel": "Cancel",
  "pdfImport.cancelLabel": "Cancel opening this PDF",
  "pdfImport.busy": "A PDF is already opening. Wait for it, or cancel it first.",
  "pdfImport.cancelled": "Opening {name} was cancelled",
  "pdfImport.cancelledKept": "Opening {name} was cancelled — the document you had open is still here",
  "pdfImport.encrypted": "{name} is password-protected. Password-protected PDFs cannot be opened yet.",
  // The lazily loaded PDF module, Convert to flowing document and text
  // recognition (`pdf_import.mjs`, `pdf_commands.mjs`, `docs/182` §17a).
  "pdfImport.loadingModule": "Loading the PDF reader…",
  "pdfImport.moduleProgress": "{done} of {total} MB",
  "pdfImport.moduleFailed": "The PDF reader could not be loaded: {reason}",
  "pdfReflow.command": "Convert to flowing document",
  "pdfReflow.title": "Converting {name}",
  "pdfReflow.intro": "Rebuilding the PDF as paragraphs, headings, lists and tables in a new document, on this device.",
  "pdfReflow.cancelLabel": "Cancel converting this PDF",
  "pdfReflow.cancelled": "Converting {name} was cancelled — the PDF is still open",
  "pdfReflow.failed": "Could not convert {name}: {reason}",
  "pdfReflow.notPdf": "Only a document opened from a PDF can be converted to a flowing document.",
  "pdfReflow.fileName": "{name} (flowing).docx",
  "findings.pdfReflow.title": "A flowing document rebuilt from a PDF",
  "findings.pdfReflow.new": "This is a new document. The PDF it was rebuilt from is unchanged; open it again to return to its pages.",
  "findings.pdfReflow.inferred": "Headings, lists, tables, columns and footnotes were inferred from the page layout. Check them before you rely on them.",
  "findings.pdfReflow.source": "It was built from the PDF as it was opened: edits made to the PDF's pages are not in it.",
  "pdfOcr.command": "Recognise text in scanned pages",
  "pdfOcr.notPdf": "Text recognition applies to a document opened from a PDF.",
  "pdfOcr.noScans": "This PDF has no scanned pages to read.",
  "pdfOcr.done": "The scanned pages of this PDF have already been read.",
  "pdfOcr.noModels": "Text recognition is unavailable: this editor has no recognition models. The site that provides the editor can add them.",
  "pdfOcr.consentTitle": "Download text recognition?",
  "pdfOcr.consentMessage": "Reading the scanned pages needs recognition models: {size} MB, downloaded once from {host} and kept in this browser. The PDF itself stays on this device.",
  "pdfOcr.consentConfirm": "Download and recognise",
  "pdfOcr.consentCancel": "Not now",
  "pdfOcr.title": "Recognising text in {name}",
  "pdfOcr.intro": "Reading the scanned pages on this device. Nothing is uploaded.",
  "pdfOcr.downloading": "Downloading recognition models…",
  "pdfOcr.loading": "Preparing text recognition…",
  "pdfOcr.cancelLabel": "Cancel recognising text",
  "pdfOcr.cancelled": "Text recognition was cancelled — the document is unchanged",
  "pdfOcr.failed": "Text recognition failed: {reason}",
  "findings.pdf.ocrTitle": "Scanned pages",
  "findings.pdf.ocrPending": "Pages whose text is only a picture: {count}. Their text is not in the document yet.",
  "findings.pdf.ocrApplied": "The text of the scanned pages was recognised on this device ({provider}). Check it against the page before you rely on it.",
  "menuGroup.pdf": "PDF",
  "findings.feature.pdfIncrementalUpdate": "Edited pages, added to the original PDF as an update",
  "findings.feature.pdfPageCarriedOver": "Pages kept exactly as in the original PDF",
  "findings.feature.pdfEarlierRevision": "Replaced text, still recoverable from the PDF's earlier version",
  "findings.feature.pdfSignatureEarlier": "Signatures, which cover only the PDF's earlier version",
  "findings.feature.pdfCertificationBroken": "The PDF's certification, which the edit invalidates",
  "findings.feature.pdfOriginalWrittenBack": "The original PDF, written back unchanged",
  "findings.feature.pdfPreserveFallback": "Edits that could not be added to the original PDF, so the PDF was regenerated",
  // The open path's status lines (`editor_file_open.mjs`, `editor_file_pick.mjs`).
  "fileOpen.ready": "Ready — open a .docx, .odt, .rtf, .pdf, .md, .html, .json or .txt",
  "fileOpen.opening": "Opening {name}…",
  "filePick.unsupported": "Please choose a .docx, .odt, .rtf, .pdf, .md, .html, .json or .txt file",
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
  "findings.feature.legacyRect": "Rectangle in the older Word format (VML)",
  "findings.feature.legacyRoundRect": "Rounded rectangle in the older Word format (VML)",
  "findings.feature.legacyOval": "Oval in the older Word format (VML)",
  "findings.feature.legacyLine": "Line in the older Word format (VML)",
  "findings.feature.legacyPolyline": "Freeform line in the older Word format (VML)",
  "findings.feature.legacyCurve": "Curve in the older Word format (VML)",
  "findings.feature.legacyArc": "Arc in the older Word format (VML)",
  "findings.feature.picture": "Picture",
  "findings.feature.legacyPicture": "Picture in a shape, in the older Word format (VML)",
  "findings.feature.legacyGroup": "Group of shapes in the older Word format (VML)",
  "findings.feature.legacyStroke": "Shape line style in the older Word format (VML)",
  "findings.feature.legacyWrap": "Text wrapping around a shape, in the older Word format",
  "findings.feature.legacyTextBox": "Text box in the older Word format (VML)",
  "findings.feature.legacyShadow": "Shadow on a shape or on text",
  "findings.feature.legacyEffect3d": "3-D effect on a shape in the older Word format (VML)",
  "findings.feature.legacySkew": "Skew effect on a shape in the older Word format (VML)",
  "findings.feature.editingLock": "Editing lock (on a shape or a content control)",
  "findings.feature.legacyCallout": "Callout line settings in the older Word format (VML)",
  "findings.feature.legacyShapeFormulas": "Shape geometry formulas in the older Word format (VML)",
  "findings.feature.legacyShapeHandles": "Shape adjustment handles in the older Word format (VML)",
  "findings.feature.vmlRegenerated": "Shape saved in the newer drawing format because it was edited",
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
  "findings.feature.customXmlStoreIdentity": "Identity of a custom XML store (an id and the schemas it uses)",
  "findings.feature.bibliographyStyleOnly": "Citation style for a bibliography that has no sources yet",
  "findings.feature.sharePointContentType": "SharePoint content type the file was saved with",
  "findings.feature.sharePointForms": "SharePoint form templates of the document library",
  "findings.feature.bibliographySources": "Bibliography sources (References ▸ Manage Sources)",
  "findings.feature.sharePointProperties": "SharePoint document properties (library columns)",
  "findings.feature.coverPageProperties": "Cover page fields (abstract, publish date, company details)",
  "findings.feature.sensitivityLabel": "Sensitivity label (Microsoft Purview)",
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
  "findings.feature.styleUnknownType": "A style of a type this editor cannot apply yet",
  "findings.feature.styleDuplicateParagraph": "A second definition of a paragraph style that disagrees with the first",
  "findings.feature.styleDuplicateCharacter": "A second definition of a character style that disagrees with the first",
  "findings.feature.styleDuplicateTable": "A second definition of a table style that disagrees with the first",
  "findings.feature.styleDuplicateNumbering": "A second definition of a list style that disagrees with the first",
  "findings.feature.tableCellMargins": "Default cell margins of a table",
  "findings.feature.tableStyleCellMargins": "Cell margins set by a table style",
  "findings.feature.cellMargins": "Margins of a single cell",
  "findings.feature.tableIndent": "Table indent",
  "findings.feature.tableStyleIndent": "Table indent set by a table style",
  "findings.feature.tableWidth": "Table width",
  "findings.feature.tableStyleWidth": "Table width set by a table style",
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
  // Foreign HTML (docs/174 TA-11): structure rebuilt only at a body caret
  // outside Suggesting; elsewhere it arrives as paragraphs, and pictures in the
  // HTML are never fetched. Said rather than done silently.
  "paste.flattened": "Pasted, but {items} became ordinary paragraphs here.",
  "paste.flattened.heading": "headings",
  "paste.flattened.list": "lists",
  "paste.flattened.table": "tables",
  "paste.dropped.pictures": "Pictures in the pasted content were not brought across.",

  // AutoCorrect options (docs/174 TA-05, TA-06; `autocorrect.mjs`). Three
  // switches, one composed label and one composed status line, so a translator
  // sees each shape once.
  "autocorrect.smartQuotes": "Smart quotes",
  "autocorrect.lists": "Automatic lists",
  "autocorrect.command": "{name}: {state}",
  "autocorrect.stateOn": "on",
  "autocorrect.stateOff": "off",
  "autocorrect.statusOn": "{name} on",
  "autocorrect.statusOff": "{name} off",
  // The rest of Word's switches and the AutoCorrect options dialog (`docs/109`
  // RM-19; `autocorrect_dialog.mjs`, `autocorrect_table.mjs`).
  "autocorrect.replace": "Replace text as you type",
  "autocorrect.capitalise": "Capitalise first letters",
  "autocorrect.capitaliseCells": "Capitalise table cells",
  "autocorrect.dashes": "Hyphens to dashes",
  "autocorrect.hyperlinks": "Automatic hyperlinks",
  "autocorrect.optionsCommand": "AutoCorrect options…",
  "autocorrect.entry": "{from} → {to}",
  "autocorrect.addEntry": "Add",
  "autocorrect.replaceEntry": "Replace",
  "autocorrect.entryRefused": "That entry cannot be stored: it must be one line of at most 255 characters.",
  "autocorrect.entryAdded": "“{from}” will be replaced with “{to}”",
  "autocorrect.entryDeleted": "“{from}” is no longer replaced",
  "autocorrect.exceptionAdded": "No capital letter after “{word}”",
  "autocorrect.exceptionDeleted": "“{word}” is no longer an exception",
  "autocorrect.listsReset": "The lists for {language} are back to their defaults",
  "autocorrect.languageShown": "Showing the lists for {language}",

  // The language of the selected text (`docs/109` RM-42; `language_selection.mjs`).
  "language.command": "Set language…",
  "language.none": "No language set (the default, {language})",
  "language.withSpelling": "{language} — spelling checked",
  "language.checked": "Text in {language} is checked for spelling.",
  "language.notChecked": "There is no spelling dictionary for {language} yet, so text in it is not checked.",
  "language.applied": "Language set to {language}",
  "language.cleared": "Language cleared; the default, {language}, applies",
  "language.mixed": "Mixed languages",
  "language.statusTip": "Text language: {language}. Click to change it.",
  "language.property": "Language",

  // Non-breaking space and the Symbol dialog's special characters (RM-19).
  "insert.nonBreakingSpace": "Non-breaking space",
  "glyph.special": "Special characters",
  "glyph.nonBreakingSpace": "No-break space",
  "glyph.narrowNonBreakingSpace": "Narrow no-break space",
  "glyph.enSpace": "En space",
  "glyph.emSpace": "Em space",
  "glyph.thinSpace": "Thin space",

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
  // ADR-068: a restore in a shared room would change this reader's copy alone.
  "versionHistory.restore.sharedRefused":
    "This document is shared, so restoring a version would change your copy and not everyone else's. Make a copy of this version or download it instead",
  // Shared version history in a room (docs/175 §11, HF-362): the room's versions,
  // kept and authored by the host, beside this device's own capture.
  "versionHistory.section.shared": "Shared versions",
  "versionHistory.section.device": "On this device",
  "versionHistory.section.deviceOnly": "Versions on this device only",
  "versionHistory.shared.loading": "Loading shared versions…",
  "versionHistory.shared.unavailable": "Shared versions could not be loaded",
  "versionHistory.shared.empty": "No shared versions yet. Save a version for everyone to start this document's shared history.",
  "versionHistory.shared.listFailed": "Shared versions could not be loaded: {message}",
  "versionHistory.shared.limit": "Shared versions shown: {count}, the most this panel holds. Older versions stay with the host.",
  "versionHistory.shared.retry": "Try again",
  "versionHistory.shared.loadOlder": "Load older shared versions",
  "versionHistory.shared.save": "Save a version for everyone",
  "versionHistory.shared.saving": "Saving a version for everyone…",
  "versionHistory.shared.saved": "Version saved for everyone in this document",
  "versionHistory.shared.savedNamed": "“{name}” saved for everyone in this document",
  "versionHistory.shared.saveFailed": "The version was not saved for everyone: {message}",
  "versionHistory.shared.cannotSave": "This host does not save shared versions",
  "versionHistory.shared.cannotRead": "This host does not provide the contents of shared versions",
  "versionHistory.shared.malformed": "The host's answer about shared versions could not be read",
  "versionHistory.shared.readFailed": "This shared version could not be opened: {message}",
  "versionHistory.shared.left": "This document is no longer shared, so its shared versions cannot be opened",
  "versionHistory.shared.hostKeeps": "The host keeps shared versions, so they cannot be kept or deleted from here",
  "versionHistory.shared.nameAtSave": "A shared version is named when it is saved. Use Save a version for everyone",
  "versionHistory.shared.loadOlderToCompare": "Load older shared versions to compare this one with the version before it",
  "versionHistory.shared.nothingToSave": "There is no document to save",
  "versionHistory.shared.noRoom": "Only a shared document whose host keeps versions can save a version for everyone",
  "versionHistory.shared.prompt.title": "Save a version for everyone",
  "versionHistory.shared.prompt.description": "Everyone in this document will see this version, under the name the host knows you by.",
  "versionHistory.shared.prompt.note": "Saving a version changes nothing in the document. The name is optional.",
  "versionHistory.shared.prompt.confirm": "Save version",

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
  "table.reason.cellNotMerged": "This cell is not merged",
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
  // What every OTHER structural table edit says once it lands, from the band,
  // the Table menu, the context menu and the palette alike (HF-346,
  // `table_band.mjs` `TABLE_EDIT_ANNOUNCEMENTS`). They used to run in silence.
  "table.rowDeleted": "Row deleted",
  "table.columnDeleted": "Column deleted",
  "table.tableDeleted": "Table deleted",
  "table.rowsDistributed": "Rows distributed evenly",
  "table.columnsDistributed": "Columns distributed evenly",
  "table.sortedAscending": "Table sorted ascending",
  "table.sortedDescending": "Table sorted descending",
  "table.cellsMerged": "Cells merged",
  "table.cellsUnmerged": "Cells unmerged",
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
  // Shape Outline ▸ Dashes and ▸ Arrows (`shape_outline_style.mjs`, RM-30): Word's names.
  "shape.dashes": "Dashes",
  "shape.dash.solid": "Solid",
  "shape.dash.roundDot": "Round dot",
  "shape.dash.squareDot": "Square dot",
  "shape.dash.dash": "Dash",
  "shape.dash.dashDot": "Dash dot",
  "shape.dash.longDash": "Long dash",
  "shape.dash.longDashDot": "Long dash dot",
  "shape.dash.longDashDotDot": "Long dash dot dot",
  "shape.arrows": "Arrows",
  "shape.arrows.none": "No arrows",
  "shape.arrows.end": "Arrow at end",
  "shape.arrows.start": "Arrow at start",
  "shape.arrows.both": "Arrows at both ends",
  "shape.arrows.openEnd": "Open arrow at end",
  "shape.arrows.stealthEnd": "Stealth arrow at end",
  "shape.arrows.diamondEnd": "Diamond at end",
  "shape.arrows.ovalEnd": "Round end",
  // Shape Fill ▸ Gradient (`shape_gradient.mjs`, RM-30): Word's directions.
  "shape.gradient": "Gradient",
  "shape.gradient.linearDown": "Linear down",
  "shape.gradient.linearUp": "Linear up",
  "shape.gradient.linearRight": "Linear right",
  "shape.gradient.linearLeft": "Linear left",
  "shape.gradient.linearDiagonal": "Linear diagonal",
  "shape.gradient.fromCenter": "From center",
  // Shape Effects ▸ Shadow (`shape_shadow.mjs`, `docs/153` object.shape-shadow):
  // Word's outer presets, named by where the shadow falls.
  "shape.shadow": "Shadow",
  "shape.shadow.none": "No shadow",
  "shape.shadow.bottomRight": "Offset: bottom right",
  "shape.shadow.bottom": "Offset: bottom",
  "shape.shadow.bottomLeft": "Offset: bottom left",
  "shape.shadow.right": "Offset: right",
  "shape.shadow.center": "Offset: center",
  "shape.shadow.left": "Offset: left",
  "shape.shadow.topRight": "Offset: top right",
  "shape.shadow.top": "Offset: top",
  "shape.shadow.topLeft": "Offset: top left",
  "shape.shadow.options": "Shadow options…",
  "shape.shadow.bad.color": "Choose a colour for the shadow.",
  "shape.shadow.bad.transparency": "Transparency is a percentage from 0 to 100.",
  "shape.shadow.bad.angle": "Enter the angle in degrees.",
  "shape.shadow.bad.distance": "Distance is from 0 to 200 pt.",
  "shape.shadow.bad.blur": "Blur is from 0 to 100 pt.",
  // Layout ▸ More layout options (`object_layout_options.mjs`, `docs/153`
  // object.advanced-layout-options).
  "object.layoutOptions": "More layout options…",
  "object.layoutOptions.inline": "Choose a text wrapping other than In line first",
  "object.layoutOptions.badDistance": "Distance from text is from 0 to 1584 pt.",
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
  // Arrange ▸ Align (`object_align.mjs`, RM-28): Word's and ONLYOFFICE's rows.
  "object.align": "Align",
  "object.align.left": "Align left",
  "object.align.center": "Align center",
  "object.align.right": "Align right",
  "object.align.top": "Align top",
  "object.align.middle": "Align middle",
  "object.align.bottom": "Align bottom",
  "object.align.distributeHorizontally": "Distribute horizontally",
  "object.align.distributeVertically": "Distribute vertically",
  "object.align.toPage": "Align to page",
  "object.align.toMargin": "Align to margin",
  "object.align.toSelected": "Align selected objects",
  "object.align.aligned": "Objects aligned",
  "object.align.distributed": "Objects distributed",
  "object.align.reason.selectMore": "Hold {modifier} and click another object to align them to each other",
  "object.align.reason.selectThree": "Hold {modifier} and click to select three or more objects to distribute them",
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
  // Change Shape (RM-30): Word's Edit Shape ▸ Change Shape.
  "object.changeShape": "Change shape",
  "object.changeShape.done": "Shape changed",
  "object.addText.notAShape": "Only a shape can hold text this way",
  // Word's Change Picture (`docs/109` HF-252). The status line says what was
  // KEPT, and asks for the one check a person must make: alt text written for
  // the old picture may not describe the new one.
  "object.changePicture": "Change picture",
  "object.changePicture.menu": "Change picture…",
  // The object toolbar (`object_bar.mjs`, `docs/172`): its accessible name per
  // kind, and the actions that were hard-coded English.
  "object.bar.image": "Image actions",
  "object.bar.textbox": "Text box actions",
  "object.bar.shape": "Shape actions",
  "object.bar.group": "Group actions",
  "object.bar.chart": "Chart actions",
  "object.bar.object": "Object actions",
  "object.bar.altText": "Edit alt text",
  "object.bar.properties": "Open object properties",
  "object.bar.crop": "Crop image",
  "object.bar.cropApply": "Apply crop (Enter)",
  "object.bar.delete": "Delete object",
  "object.changePicture.unsupported": "That file can’t be used as a picture. Choose a PNG, JPEG, GIF, BMP, TIFF or WebP image.",
  "object.changePicture.unreadable": "Could not read that image",
  "object.changePicture.done": "Picture changed — its size, position, wrap and border were kept",
  "object.changePicture.doneAltText": "Picture changed — check that its alt text still describes the new picture",
  // Word's Picture Border: a picture's outline, under Word's name for it (HF-254).
  "object.pictureBorder": "Picture border",
  "object.pictureBorder.short": "Border",
  // The picture tools beside Crop (`picture_tools.mjs`, RM-29): ONLYOFFICE's
  // Crop ▸ Fill / Fit, Reset crop and Actual Size.
  "object.picture.cropFill": "Crop to fill",
  "object.picture.cropFit": "Crop to fit",
  "object.picture.resetCrop": "Reset crop",
  "object.picture.resetSize": "Reset to actual size",
  "object.picture.cropFilled": "Picture cropped to fill its frame",
  "object.picture.cropFitted": "Whole picture fitted inside its frame",
  "object.picture.cropReset": "Crop removed",
  "object.picture.sizeReset": "Picture set to its actual size",
  "object.picture.reason.noNaturalSize": "This picture's original size cannot be read from its file",
  "object.picture.reason.notCropped": "The picture is not cropped",
  "object.picture.saveAs": "Save as picture",
  "object.picture.fileName": "Picture",
  "object.picture.reason.noFile": "This picture's image file is not in the document",
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
  // The table tools (`table_tools.mjs`, `docs/153` table rows).
  "table.tools.insertCellsCommand": "Insert cells…",
  "table.tools.deleteCellsCommand": "Delete cells…",
  "table.tools.splitTableCommand": "Split table",
  "table.tools.convertToTextCommand": "Convert to text…",
  "table.tools.lookSubmenu": "Style options",
  "table.tools.directionSubmenu": "Text direction",
  "table.tools.insertDialogCommand": "Insert table by size…",
  "table.tools.textToTableCommand": "Convert text to table…",
  "table.look.headerRow": "Header row",
  "table.look.firstColumn": "First column",
  "table.look.totalRow": "Total row",
  "table.look.lastColumn": "Last column",
  "table.look.bandedRows": "Banded rows",
  "table.look.bandedColumns": "Banded columns",
  "table.textDirection.horizontal": "Horizontal text",
  "table.textDirection.rotateDown": "Rotate text down",
  "table.textDirection.rotateUp": "Rotate text up",
  "table.tools.inserted": "Table inserted",
  "table.tools.convertedFromText": "Text converted to a table",
  "table.tools.convertedToText": "Table converted to text",
  "table.tools.split": "Table split",
  "table.tools.cellsInserted": "Cells inserted",
  "table.tools.cellsDeleted": "Cells deleted",
  "table.tools.columnsRange": "Enter a whole number of columns from 1 to {max}.",
  "table.tools.rowsRange": "Enter a whole number of rows from 1 to {max}.",
  "table.tools.otherEmpty": "Type a character to separate the text at.",
  "table.tools.selectParagraphs": "Select paragraphs outside a table to convert them to a table.",
  "table.tools.lookOn": "Turned on:",
  "table.tools.lookOff": "Turned off:",
  "table.tools.directionSet": "Text direction:",
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

  // Borders and shading (`table_borders_dialog.mjs`): the Table menu, palette
  // and context-menu row (the ellipsis: it opens a dialog), and what the status
  // line says once it applies.
  "table.bordersShadingCommand": "Borders and shading\u2026",
  "tableBorders.applied": "Borders and shading applied",

  // Text wrapping and Positioning (`table_position_dialog.mjs`).
  "tablePosition.wrapAround": "Text now wraps around the table",
  "tablePosition.wrapNone": "The table is back in line with the text",
  "tablePosition.applied": "Table position updated",
  "tablePosition.needsAround": "Set Text wrapping to Around to position the table",
  "tablePosition.invalid": "Enter a measurement of no more than 22 inches",

  // Draw table (`table_draw_mode.mjs`): the command row and the status line.
  "tableDraw.command": "Draw table",
  "tableDraw.armed": "Draw table: drag on the page to draw a table, or across a table to split its cells. Press Esc to stop.",
  "tableDraw.off": "Draw table off",
  "tableDraw.drawn": "Table drawn",
  "tableDraw.split": "Cells split",

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
  // Appended when the restriction just changed carried a Word password, which a
  // change made here removes (ADR-052, updated 2026-10-09). `{status}` is one of
  // the sentences above.
  "protect.withPasswordRemoved": "{status}. The password set in Word was removed.",
  // The password (`docs/153` `review.protection-password`, ADR-052 as amended
  // 2026-10-30): the status after a passworded restriction is installed, and the
  // three refusals said beside the dialog's fields. `protect.wrongPassword` is
  // also the routed sentence for the engine's `document.protection-wrong-password`.
  "protect.appliedWithPassword": "Editing restricted to: {level}, with a password",
  "protect.passwordMismatch": "The two new passwords don't match",
  "protect.passwordRequired": "Enter the password to change or stop this restriction",
  "protect.wrongPassword": "That isn't the password this restriction was set with",

  // Display for review (`review_display.mjs`, `docs/153` review.display-mode-*).
  // The four mode names and the trigger's tip are markup keys; these are the
  // Original view's read-only reason (the banner a preview shows), a change
  // bar's tooltip, and the refusal when a version preview is already up.
  "reviewDisplay.originalReadOnly": "Original: the document as it was before its tracked changes. Choose another display for review to edit.",
  "reviewDisplay.changeBar": "Changed here. Click to show all markup",
  "reviewDisplay.previewOpen": "Close the version preview before showing the original",

  // Footnotes and endnotes (`note_commands.mjs`, `docs/153` page.note.*). The
  // menu rows and both dialogs are markup keys; these are what the status line
  // says, and the reason the selection-only box is disabled.
  "notes.noMoreFootnotes": "There are no more footnotes that way",
  "notes.noMoreEndnotes": "There are no more endnotes that way",
  "notes.footnoteSettingsApplied": "Footnote options applied",
  "notes.endnoteSettingsApplied": "Endnote options applied",
  "notes.converted": "Notes converted",
  "notes.convertNeedsSelection": "Select the text whose notes you want to convert first",

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
  // only WHY — never what became of the reader's work, because one code arrives
  // by more than one route and each route does something different to it. The
  // consequence is a separate clause, chosen from the outcome the engine actually
  // returned (`session_access.mjs`'s `refusalConsequence`), so it cannot claim a
  // loss that did not happen. RC-13: the `ODC-7006` sentence used to say "unsent
  // changes were lost — reload to continue" while the engine kept every one of
  // them, and reloading was the one thing that would have lost them.
  // `collab_refusal_messages.test.mjs` fails a cause sentence that starts
  // claiming an outcome again.
  "collab.conflict": "A change from someone else could not be combined with yours",
  "collab.protocolVersion": "This editor and the document's server use different versions",
  "collab.notAuthorised": "This session is not authorised for this shared document",
  "collab.readOnly": "You can read this shared document but not change it",
  "collab.notSaving": "The shared session can no longer store changes",
  "collab.tooFarBehind": "This copy fell too far behind the shared document to catch up by itself",
  "collab.malformed": "A message between this editor and the shared session could not be read",
  "collab.idCollision": "A change named something this document already has",
  "collab.staleBase": "The document moved on while that change was on its way",
  "collab.roomFull": "This document already has as many people in it as it allows",
  // The CONSEQUENCE clauses. Each asserts exactly the facts `CONSEQUENCE_CLAIMS`
  // lists for it, and `collab_refusal_outcomes.json` — driven through the real
  // engine by `every_collaboration_refusal_does_what_the_fixture_says` — is what
  // makes those facts true for every route that selects it.
  "collab.outcome.recovering": "Your changes are kept on this device while the latest shared copy is loaded",
  "collab.outcome.recoveryUnavailable": "Your changes are kept on this device, but this host cannot supply the latest shared copy — save a copy of your work",
  "collab.outcome.stopped": "This copy is no longer shared, and your changes stay on this device — save a copy before you close it",
  "collab.outcome.notShared": "That change stays on this device and was not shared, so this copy no longer matches everyone else's",
  "collab.outcome.notApplied": "It was not applied, so this copy may no longer match everyone else's",
  // Cause, then consequence. A template rather than concatenation, so a language
  // that joins two clauses differently says so in its own catalogue.
  "collab.refusal": "{cause}. {consequence}.",
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
  // The sync chip (`collab_sync.mjs`, `docs/109` HF-313): what has become of the
  // reader's last change, driven by the relay's acknowledgements rather than by
  // whether the socket is open. `collab.connected` above is the acknowledged state.
  "collab.syncPending": "Shared — sending your changes…",
  "collab.syncSent": "Shared — waiting for the server to confirm your changes",
  "collab.syncStuck":
    "Shared — your latest changes have not been confirmed for {seconds} s. They are kept on this device and will be sent when the server answers",
  "collab.leftShared": "You left the shared document. Changes you make now stay in this copy",
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
  "chart.element.axisTitles": "Axis titles",
  "chart.element.errorBars": "Error bars",
  "chart.element.trendline": "Trendline",
  "chart.trendline.none": "None",
  "chart.trendline.linear": "Linear",
  "chart.trendline.exp": "Exponential",
  "chart.trendline.log": "Logarithmic",
  "chart.trendline.power": "Power",
  "chart.trendline.poly": "Polynomial",
  "chart.trendline.movingAvg": "Moving average",
  "chart.trendline.order": "Order",
  "chart.trendline.period": "Period",
  "chart.trendline.equation": "Display equation",
  "chart.trendline.rSquared": "Display R-squared value",
  "chart.errorBars.none": "None",
  "chart.errorBars.stdErr": "Standard error",
  "chart.errorBars.percentage": "Percentage",
  "chart.errorBars.stdDev": "Standard deviation",
  "chart.errorBars.fixedVal": "Fixed value",
  "chart.errorBars.cust": "Custom (from the file)",
  "chart.errorBars.custReadOnly": "These error bars use per-point lengths from the file, which can be kept or removed but not edited here.",
  "chart.errorBars.amountFixed": "Value",
  "chart.errorBars.amountPercent": "Percentage (%)",
  "chart.errorBars.amountStdDev": "Standard deviations",
  "chart.errorBars.direction": "Direction",
  "chart.errorBars.both": "Both",
  "chart.errorBars.plus": "Plus",
  "chart.errorBars.minus": "Minus",
  "chart.dash.solid": "Solid",
  "chart.dash.roundDot": "Round dot",
  "chart.dash.squareDot": "Square dot",
  "chart.dash.dash": "Dash",
  "chart.dash.dashDot": "Dash dot",
  "chart.dash.longDash": "Long dash",
  "chart.dash.longDashDot": "Long dash dot",
  "chart.dash.longDashDotDot": "Long dash dot dot",
  "chart.dash.dot": "Dot",
  "chart.dash.sysDashDot": "Short dash dot",
  "chart.dash.sysDashDotDot": "Short dash dot dot",
  "chart.combo.columnLine": "Clustered column – line",
  "chart.combo.columnLineSecondary": "Clustered column – line on secondary axis",
  "chart.combo.areaColumn": "Stacked area – clustered column",
  "chart.combo.custom": "Custom combination…",
  "chart.combo.needsTwoSeries": "A combination chart needs at least two series",
  "chart.defaultAxisTitle": "Axis Title",
  "chart.refused.notCombinable": "Only column, line and area types can be combined in one chart.",
  "chart.refused.noTrendline": "This chart type cannot show a trendline.",
  "chart.refused.noErrorBars": "This chart type cannot show error bars.",
  "chart.noTrendline": "Pie and doughnut charts have no trendlines",
  "chart.noErrorBars": "Pie and doughnut charts have no error bars",
  "chart.moreOptions": "More options…",
  "chart.numFmt.general": "General",
  "chart.numFmt.integer": "Number",
  "chart.numFmt.decimal": "Number, 2 decimals",
  "chart.numFmt.thousands": "Thousands separator",
  "chart.numFmt.thousandsDecimal": "Thousands separator, 2 decimals",
  "chart.numFmt.percent": "Percentage",
  "chart.numFmt.percentDecimal": "Percentage, 2 decimals",
  "chart.numFmt.dollar": "Currency (dollar)",
  "chart.numFmt.euro": "Currency (euro)",
  "chart.numFmt.scientific": "Scientific",
  "chart.numFmt.custom": "Custom…",
  "chart.numFmt.code": "Format code",
  "chart.numFmt.label": "Number format",
  "chart.numFmt.values": "Value format",
  "chart.font.wholeChart": "Whole chart",
  "chart.font.element": "Apply to",
  "chart.font.face": "Font",
  "chart.font.size": "Size (pt)",
  "chart.font.style": "Font style",
  "chart.font.bold": "Bold",
  "chart.font.italic": "Italic",
  "chart.font.color": "Font color",
  "chart.axisTitle.none": "No title",
  "chart.axisTitle.label": "Axis title",
  "chart.color.automatic": "Automatic",
  "chart.seriesHeading": "Series",
  "chart.seriesPicker": "Series to format",
  "chart.seriesType": "Chart type",
  "chart.secondaryAxis": "Plot on secondary axis",
  "chart.lineColor": "Line color",
  "chart.fillColor": "Fill color",
  "chart.seriesColor": "Series color",
  "chart.lineWidth": "Width (pt)",
  "chart.dashType": "Dash type",
  "chart.textHeading": "Text",
  "chart.palette.colorful": "Colorful",
  "chart.palette.mono1": "Monochrome 1",
  "chart.palette.mono2": "Monochrome 2",
  "chart.palette.mono3": "Monochrome 3",
  "chart.palette.mono4": "Monochrome 4",
  "chart.palette.mono5": "Monochrome 5",
  "chart.palette.mono6": "Monochrome 6",
  "listAdvanced.oneSymbol": "Enter one non-whitespace bullet symbol.",
  "listAdvanced.pictureBounds": "Choose a PNG, JPEG, WebP or GIF image up to 16 MiB.",
  "listAdvanced.selectPicture": "Choose a picture first.",
  "styleAdvanced.keep": "Keep the current style",
  "break.inserted.blankPage": "Blank page inserted",
  // `editor.ai`'s private preview (`ai_assistant.mjs`, `docs/181` §4.4): an in-page
  // assistant's proposal, shown only to the person who asked until they decide.
  "ai.panel.title": "Assistant suggestion",
  "ai.panel.private": "Only you can see this until you accept it.",
  "ai.panel.suggest": "Accepting adds these changes as suggestions by {author}.",
  "ai.panel.direct": "Accepting changes the document directly, as {author}.",
  "ai.panel.changes": "Proposed changes",
  "ai.panel.newParagraph": "New paragraph",
  "ai.panel.inserted": "Inserted:",
  "ai.panel.deleted": "Deleted:",
  "ai.panel.accept": "Accept",
  "ai.panel.discard": "Discard",
  "ai.panel.cannotApply": "This suggestion can’t be applied.",
  "ai.accepted": "Assistant suggestion accepted",
  "ai.discarded": "Assistant suggestion discarded",
  "ai.refusal.access": "Your access to this document doesn’t allow this change.",
  "ai.refusal.protected": "This document is protected against this change.",
  "ai.refusal.anchor": "The text this change refers to could not be found.",
  "ai.refusal.stale": "The document changed after this suggestion was prepared. Ask the assistant again.",
  "ai.refusal.gone": "This suggestion is no longer open.",
  "ai.refusal.noDocument": "Open a document first.",
  "ai.refusal.generic": "This change can’t be applied.",
  // Picture from URL and Paste as picture (`picture_url_dialog.mjs`, `paste_picture.mjs`).
  "insert.imageFromUrl": "Picture from URL…",
  "insert.imageFromUrl.keywords": "image picture online web address link url download internet",
  "edit.pasteImage": "Paste as picture",
  "edit.pasteImage.keywords": "paste special image snapshot screenshot picture rasterize",
  "pictureUrl.downloading": "Downloading the picture…",
  "pictureUrl.received": "Downloaded {size}…",
  "pictureUrl.sizeMb": "{size} MB",
  "pictureUrl.sizeKb": "{size} KB",
  "pictureUrl.error.invalid": "Enter a web address that starts with http:// or https://.",
  "pictureUrl.error.credentials": "Remove the user name and password from the address.",
  "pictureUrl.error.denied": "This editor’s host does not allow pictures to be downloaded from the web.",
  "pictureUrl.error.network": "The picture couldn’t be downloaded. The site may be unreachable, or it may not let other sites use its pictures.",
  "pictureUrl.error.http": "The site answered with error {status}; no picture was downloaded.",
  "pictureUrl.error.notImage": "That address is not a picture.",
  "pictureUrl.error.unsupported": "That picture’s format can’t be used. Use a PNG, JPEG, GIF, BMP, TIFF or WebP picture.",
  "pictureUrl.error.tooLarge": "That picture is larger than 20 MB.",
  "pictureUrl.error.empty": "The site sent an empty file.",
  "pictureUrl.error.aborted": "The download was cancelled.",
  "pastePicture.empty": "There is nothing visible on the clipboard to paste as a picture.",
  "pastePicture.tooLarge": "The clipboard is too large to paste as one picture.",
  "pastePicture.failed": "The clipboard couldn’t be pasted as a picture.",
  "pastePicture.nothing": "There is nothing on the clipboard to paste as a picture.",
  "pastePicture.blocked": "The browser did not allow the clipboard to be read.",
  // Edit Points on a shape (`shape_edit_points.mjs`).
  "object.editPoints": "Edit points",
  "object.editPoints.hint": "Drag a point to reshape the outline · Enter to keep · Esc to cancel",
  "object.editPoints.done": "Shape outline changed",
  "object.editPoints.cancelled": "Points not changed",
  "object.editPoints.suggesting": "Editing a shape’s points cannot be tracked yet; switch to Editing to change it",
  "object.editPoints.unavailable": "This shape’s outline can’t be edited",
  "object.editPoints.rotated": "Rotate this shape back to 0° to edit its points",
  // ---- View and text extras (`editor_view_text.mjs`, `docs/153`) -------------
  // Find with a regular expression: the pattern's own error, inline.
  "findPanel.regexInvalid": "Not a valid pattern: {reason}",
  // Dark document and Multiple pages: view switches, never the file.
  "darkDocument.commandOn": "Dark document: on",
  "darkDocument.commandOff": "Dark document: off",
  "darkDocument.on": "Dark document on — the file is unchanged",
  "darkDocument.off": "Dark document off",
  "multiplePages.commandOn": "Multiple pages: on",
  "multiplePages.commandOff": "Multiple pages: off",
  "multiplePages.on": "Multiple pages on",
  "multiplePages.off": "Multiple pages off",
  "multiplePages.reflow": "Reflow has no pages to show side by side",
  "menuGroup.pageDisplay": "Page display",
  // Print selection.
  "printSelection.command": "Print selection",
  "printSelection.needsRange": "Select the text to print first",
  // The personal dictionary.
  "dictionaryDialog.command": "Personal dictionary…",
  "dictionaryDialog.remove": "Remove",
  "dictionaryDialog.removeWord": "Remove “{word}” from your dictionary",
  "dictionaryDialog.added": "Added “{word}”",
  "dictionaryDialog.removed": "Removed “{word}” — it is checked again",
  "dictionaryDialog.cleared": "Your dictionary is empty",
  "dictionaryDialog.notSaved": "Changed for this session only — your dictionary could not be saved",
  "dictionaryDialog.oneWord": "Type one word, with no spaces",
  // Alt+X.
  "unicodeToggle.command": "Toggle character code",
  "unicodeToggle.nothing": "Nothing before the insertion point to convert",
  // Insert text from file.
  "insertFile.command": "Text from file…",
  "insertFile.done": "Inserted the content of {name}",
  "insertFile.loss": "Inserted {name}, without {items} — those belong to the other document.",
  "paste.loss.sectionBreak": "section breaks",
  "paste.loss.embeddedObject": "embedded objects and charts",
  // The AI provider contract and the plugin platform (`docs/183`).
  "aiProvider.command": "AI provider settings…",
  "aiProvider.none": "No AI provider is set up, so no text is sent anywhere.",
  "aiProvider.disclosure.endpoint": "Text you send to AI features goes to {endpoint}.",
  "aiProvider.disclosure.host": "Text you send to AI features goes to the service of the application you opened this in.",
  "aiProvider.disclosure.mcp": "Text you send to AI features goes to the model your AI app lends this editor.",
  "aiProvider.disclosure.framed": "AI for this editor is managed by the application you opened it in.",
  "aiProvider.disclosure.off": "AI features are turned off for this editor.",
  "aiProvider.problem.url": "Enter the service’s endpoint URL.",
  "aiProvider.problem.insecure": "The endpoint must start with https://, or be a server on this computer.",
  "aiProvider.problem.model": "Enter the model name.",
  "aiProvider.problem.key": "Enter the API key for this service.",
  "aiProvider.key.saved": "A key is saved on this device. Leave the field empty to keep it.",
  "aiProvider.key.local": "A model on this computer usually needs no key.",
  "aiProvider.key.device": "The key is stored on this device only and sent only to this endpoint.",
  "aiProvider.saved": "AI provider saved on this device.",
  "aiProvider.savedPage": "This browser would not store it, so the AI provider is set for this page only.",
  "aiProvider.forgotten": "The AI provider and its key were removed from this device.",
  "aiProvider.testing": "Asking {endpoint}…",
  "aiProvider.testOk": "Connected: {model} answered.",
  "aiProvider.error.auth": "The service refused the key.",
  "aiProvider.error.rate": "The service is limiting requests. Try again shortly.",
  "aiProvider.error.network": "The service could not be reached. A server on this computer must allow this page’s address (CORS).",
  "aiProvider.error.timeout": "The service did not answer in time.",
  "aiProvider.error.tooLong": "The text is too long for this model.",
  "aiProvider.error.server": "The service had a problem. Try again shortly.",
  "aiProvider.error.budget": "This page’s AI allowance is used up. Reload the page to reset it.",
  "aiProvider.error.unknown": "The service refused the request ({code}).",
  "plugins.manage.command": "Manage add-ins…",
  "plugins.commandLabel": "{plugin}: {command}",
  "plugins.notRunning": "{name} is not running.",
  "plugins.turnOnFirst": "Turn on {name} in Manage add-ins first.",
  "plugins.notify": "{name}: {message}",
  "plugins.navigated": "{name} was closed because it tried to leave its sandbox.",
  "plugins.unavailable": "{name} could not be loaded.",
  "plugins.panel.close": "Close {name}",
  "plugins.grant.title": "Allow {name}?",
  "plugins.grant.from": "From {publisher}, served by {origin}.",
  "plugins.permission.read": "Read this document",
  "plugins.permission.comment": "Propose comments",
  "plugins.permission.suggest": "Propose changes as suggestions",
  "plugins.permission.edit": "Change this document",
  "plugins.permission.network": "Connect to: {origins}",
  "plugins.permission.ai": "Use your AI provider (text goes to {endpoint})",
  "plugins.permission.aiHost": "the service of the application you opened this in",
  "plugins.permission.aiNone": "no provider is set up yet",
  "plugins.permissionShort.read": "read",
  "plugins.permissionShort.comment": "comment",
  "plugins.permissionShort.suggest": "suggest",
  "plugins.permissionShort.edit": "edit",
  "plugins.permissionShort.network": "network",
  "plugins.permissionShort.ai": "AI",
  "plugins.permissionsList": "Asks to: {list}",
  "plugins.permissionsNone": "Asks for no permissions.",
  "plugins.source.firstParty": "By Casual Office · version {version}",
  "plugins.source.host": "Chosen by your application · {origin} · version {version}",
  "plugins.source.installed": "Installed by you · {origin} · version {version}",
  "plugins.state.onByHost": "On, set by your application",
  "plugins.state.offByHost": "Off, set by your application",
  "plugins.turnOn": "Turn on",
  "plugins.turnOff": "Turn off",
  "plugins.remove": "Remove",
  "plugins.forgetGrant": "Forget permissions",
  "plugins.empty": "No add-ins are available yet.",
  "plugins.noneHere": "Add-ins are not available in this editor.",
  "plugins.menuEmpty": "No add-ins are turned on.",
  "plugins.catalogueProblem": "Some add-ins could not be listed.",
  "plugins.install.badUrl": "That is not a URL.",
  "plugins.install.checking": "Checking the add-in…",
  "plugins.install.refused": "This add-in cannot be installed: {reason}",
});
