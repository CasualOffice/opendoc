// The ratchet on `main.js`, and the rules that keep an extracted module
// extracted.
//
// `109` HF-085 is not "main.js is big"; it is that main.js is the webapp, with
// no seam a test, a host page or a translator can reach. Extracting a few
// modules fixes that only if the file cannot quietly grow back — and it grew
// to 18k lines one reasonable commit at a time, with nothing objecting.
//
// So the ceiling below is a RATCHET, not a target. It is set to whatever the
// file measured when it was last lowered. A change that adds lines to main.js
// fails this test, and there are exactly two honest ways to go green: put the
// new code in a module, or take something else out. When you do take something
// out, lower the ceiling to the new measurement in the same commit — that is
// what makes the next change harder rather than easier.
//
// Never raise it. If a change genuinely cannot avoid growing main.js, that is
// a conversation with the owner and a recorded exception (SKILL.md §10: no
// file over ~2,000 lines without one), not a bumped number.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const SRC = new URL("../src/", import.meta.url);

/** Measured 2026-09-21 on the merge of two independent extraction rounds:
 *  the review-layout, review-label and card-reuse modules that came with
 *  HF-088/HF-025 and the vertical-navigation fix, and the page-band round
 *  (`docs/113` section 8.6) that took the scroll model, the print path, the
 *  accessibility mirror, the Pages navigator, the shortcut reference and the
 *  About dialog out. Each branch measured its own half — 18,134 and 18,158
 *  against a shared 18,186 base — so neither number is right for the merge
 *  and this one is re-measured from the merged file rather than picked.
 *  Lowered again to 18,107 by the sticky-goal-column fix (HF-164): the goal
 *  column and its clears cost lines, so the selection-ordering and
 *  position-comparison helpers moved into `caret_navigation.mjs` (renamed from
 *  `caret_probe.mjs`) to pay for them.
 *  Was 18,373 before the first HF-085 extraction.
 *  Lowered again to 17,408 by the one-axis navigation restructure (`109` UX-014,
 *  `docs/122`), which is the ratchet working as intended: the application-menu
 *  bar's ~150 lines of rendering and keyboard behaviour moved to
 *  `command_menu.mjs`, where the new File PAGE reuses the same row builder rather
 *  than growing a second copy of it, and that extraction paid for the File page,
 *  the tab wiring, the five new band buttons and three small fixes with four
 *  lines to spare. Re-measured from the merged file per the MERGE NOTE below: the
 *  base read 17,412 against a ceiling of 17,425, and neither number described
 *  this file.
 *  Lowered again to 17,801 by the spelling row (`109` HF-035, `docs/114`):
 *  the file was AT its ceiling with zero slack, so the 242 lines of curated
 *  Symbol/Emoji data moved to `glyph_sets.mjs` — pure data, already covered by
 *  the picker's own specs — to pay for the ~110 lines of spell-check wiring.
 *  The checker itself is in `spell_check.mjs` and `spelling.mjs`, which is also
 *  what makes its rules unit-testable without a browser.
 *  Lowered again to 17,658 by the compact-toolbar width/grouping fix: the bar's
 *  layout table, its grouping and its overflow fold moved to
 *  `compact_toolbar.mjs`, which is also what lets a test read the declared
 *  control set without a browser.
 *  Lowered again to 17,568 when grammar joined it (the owner raised grammar
 *  above spelling on 2026-09-23): the bookmark manager moved to
 *  `bookmark_manager.mjs`, keeping only the review-mode gate and the repaint,
 *  which are about review mode and about pages rather than about bookmarks.
 *  MERGE NOTE: both of the above came off the same 17,801 base and each
 *  lowered the ceiling to its own half's measurement, so NEITHER number is
 *  right here — exactly the trap SKILL.md §5 records. This one is re-measured
 *  from the merged file.
 *  Lowered again when localisation landed on top of the red-main fixes: the
 *  seam, the loader and the count sentences became modules, and so did the
 *  review-formatting labels. BOTH sides of that merge lowered the ceiling from
 *  17,362 to their own half's measurement — 17,324 and 17,330 — so neither
 *  number was right for the merged file, which is the same trap this note
 *  records above. Re-measured from the merged file.
 *  Lowered again to 17,314 by the right-margin comment affordance: the file was
 *  AT its ceiling, so the four DOM factories every comment card is built from
 *  (`reviewCardButton`, `reviewIconButton`, `autoGrowTextarea`,
 *  `attachComposerKeys`) moved to `review_chrome.mjs`, which is also where the
 *  affordance's own controller lives. None of the four read a single piece of
 *  application state, so they had no business being in here.
 *  Lowered again to 17,122 by the line-numbering work (`109` OO-022): the file
 *  was AT its ceiling with one line of slack, so Word's whole Layout ▸ Page
 *  Setup group — the geometry dialog's 212 lines, plus the new Line Numbers
 *  popover — moved to `page_setup.mjs`. The two belong in one module because
 *  they share one question, "which section is the caret in", and answering it
 *  twice is how the dialog came to show one section's margins while writing
 *  them to another.
 *  Lowered again to 17,081 by the watermark dialog (`109` OO-006): the file was
 *  AT its ceiling with zero slack, so the per-author review COLOUR — the palette,
 *  the author key and the FNV hash over it — moved to `review_labels.mjs`, where
 *  the rest of the pure per-review-item vocabulary already lives and where the
 *  mapping is provable without a browser. The dialog itself went into
 *  `page_setup.mjs`, beside the rest of Word's Page Setup group; what the handful
 *  of lines still here buy is the button binding, the `LAYOUT_SURFACE` row that
 *  makes the command reachable from the palette as well as the ribbon, and the
 *  font inventory handed to the module.
 *  Lowered to 17,054 after a NEW shape of the merge trap above turned `main` red
 *  with two green PRs and no conflict. At 17,081 the file had zero slack, so
 *  #613 (popover anchoring) and #614 (drop caps) each bought their one new line
 *  by deleting the SAME trailing blank line at the end of the file. Git applies
 *  that deletion once, so the merge landed at 17,082 and this guard failed on
 *  `main` while both branches had passed it. The lesson is not a number: paying
 *  the ratchet in cosmetic whitespace is not paying it, because whitespace is
 *  not per-branch currency. So the fix moves code out, as the failure message
 *  asks — the swatch vocabularies (`TEXT_STANDARD_COLORS`, `HIGHLIGHT_COLORS`,
 *  the name→hex/label maps) went to `palettes.mjs`, which also makes "every
 *  highlight the engine can write is offerable" provable in node — and leaves
 *  27 lines of slack so the next two concurrent PRs do not collide the same way.
 *  Lowered again to 17,040 by the Insert-band reachability round (Page number,
 *  Date and Comment as direct band buttons): the three `INSERT_SURFACE` /
 *  `REVIEW_SURFACE` rows and their rationale cost more than the 14 lines of
 *  slack that were left, so the field VOCABULARY — the kind table and the
 *  host-side result formatter — moved to `field_kinds.mjs`. It is data plus one
 *  pure function, so what a date field caches is now answerable in node, which
 *  is the ratchet buying a seam rather than just a smaller file.
 *  RE-MEASURED to 17,045 on rebase, and this is the SAME merge trap the two
 *  notes above record, caught before it landed rather than after. 17,040 was
 *  measured against a base that did not yet carry the Resolve/Delete comment
 *  commands' menu-and-palette entries; rebasing onto a main that does added
 *  five lines the branch could not have known about, and the ceiling it had
 *  lowered was suddenly five under the file. Re-measured from the MERGED file,
 *  which is the only number that was ever meaningful — and 17,045 is still nine
 *  below main's 17,054, so the round still paid for itself.
 *  Lowered again to 17,000 by the radio-group round (`109` UX-021): six segmented
 *  controls each hand-rolled the same "set the clicked one, clear the others"
 *  loop and only two of them ever published a radio's state, so the pattern
 *  moved to `radio_group.mjs` and the call sites now say what they mean
 *  (`reflect`, `select`, `value`) instead of querying `[aria-pressed="true"]`.
 *  That is the ratchet doing its job: the file shrank BECAUSE six copies of one
 *  rule became one, not because anything was trimmed.
 *  Lowered to 16,831 by the landscape-ruler fix — the biggest single drop so far,
 *  and the clearest case for why the ratchet is worth paying. The fix needed a
 *  dozen lines in a file with ZERO slack, so the whole 244-line ruler moved to
 *  `ruler.mjs`: the strip, the indent markers, the tab stops and both drag
 *  interactions, which are one cohesive thing keyed to one scale — and the scale
 *  was the thing that was wrong. It also came out with no English in it (the five
 *  titles are passed in), so the unrouted-string ceilings did not move at all.
 *  RE-MEASURED on the merge of UX-021 and the landscape ruler, which is the
 *  THIRD time this trap has been recorded here and the first time both halves
 *  were extractions rather than additions. UX-021 measured 17,000 and the ruler
 *  16,831, each honestly against a `main.js` of 17,045 — and NEITHER describes
 *  the merged file, because the two removed different code. Git applied both
 *  deletions, so the merge is smaller than either branch predicted. The number
 *  below is measured from the merged file; it is the only one that was ever
 *  meaningful.
 *  Lowered to 16,721 by the keymap round (`109` UX-006 + UX-007): four separate
 *  `keydown` handlers and a `FORMAT_KEYS` table became one declarative chord ->
 *  command map in `keymap.mjs`, which is also what makes a label and its binding
 *  ONE declaration instead of two tables that drifted apart.
 *  Measured and lowered AFTER the ratchet settled, not during: this branch was
 *  told to leave the number alone while #617, #618 and #622 were all in flight
 *  against a moving `main.js`, because each would otherwise have measured a file
 *  the others were about to change — the merge trap the notes above record three
 *  times. With those merged, 16,779 is the merged measurement and 16,721 is this
 *  round's. Leaving the 58 lines of slack would hand the next change free growth,
 *  which is the one thing a ratchet exists to refuse.
 *  Lowered to 16,629 by OO-005 (captions and cross-references). The file was AT
 *  its ceiling with ZERO slack and the round needed a References surface row, two
 *  dialog hosts and a shared object-menu row builder, so it paid with two
 *  extractions rather than with whitespace — the mistake the 17,054 note above
 *  records. What came out: the 156-line OBJECT right-click menu, which reached
 *  the application through nothing but functions and so became a pure module
 *  with its own node test (it had none), and the Outline panel's row builder,
 *  which is the same enumeration the References tab is about and took its one
 *  English sentence into the catalogue on the way. Both are worth more than the
 *  lines: 156 lines of menu policy now have a test, and `buildOutline` is
 *  answerable without a browser.
 *  Lowered to 16,616 by the section-properties round. The file was AT its ceiling
 *  again, and the round needed a surface row, a palette row, a button id and a
 *  module construction — so it paid the same way: the two running-content variant
 *  toggles (`runningVariantState` / `toggleRunningVariant`) moved into
 *  `header_footer_settings.mjs`, which is the dialog whose Options group is those
 *  same two switches. That is worth more than the lines. The dialog's checkboxes
 *  and the Insert band's buttons are now ONE implementation, so they cannot
 *  disagree about what `w:titlePg` says, and the four English sentences those
 *  functions built went into the catalogue on the way — which is why the unrouted
 *  ceiling below moved too. Re-measured from the file AFTER rebasing onto
 *  `origin/main` at e8cdf0e, not before: measuring first is the trap the notes
 *  above record three times.
 *  Lowered to 16,602 by the host contract (`docs/126` phase 2). The file was AT
 *  its ceiling with ZERO slack again, and the contract needs 20 lines of it: one
 *  `let`, five notification hooks on choke points the editor already has, and the
 *  session/bridge construction at the end, where the command registry finally
 *  exists. It paid with two extractions. Out: the SAVE SURFACE — which formats
 *  the Save selector offers, what the compatibility chip reads, and the one step
 *  that puts bytes on a visitor's disk — now `save_formats.mjs`, so "does the
 *  selector offer every exporter the engine registered, and does it default to
 *  the format the document was opened as" is a node question rather than a
 *  browser one. And the smart-quote decision, which moved into `text_rules.mjs`
 *  beside the rule it calls: its only hard part is that an engine offset is a
 *  UTF-8 BYTE index (`docs/104` HF-055), which is exactly what that module is
 *  for, and it is now answerable with a plain function instead of a document.
 *  Lowered to 16,574 by white-labelling and region composition (`docs/126` phase
 *  3). The round needed three imports, a region application at boot, a brand
 *  product in the tab title and a pinned-accent branch, and the file was at its
 *  ceiling with zero slack again — so it paid with two extractions rather than
 *  whitespace.
 *
 *  Out: the guarded `localStorage` helpers, now `prefs.mjs`. Worth more than the
 *  lines because `localStorage` here is HOST POLICY — touching it throws with
 *  site data blocked, in a cross-origin embed, or in some private modes, and an
 *  unguarded module-scope read once left the whole editor inert — so "what
 *  happens when storage is refused" is now a node question with a fake store,
 *  where before only a browser with site data blocked could ask it.
 *
 *  And out: the appearance policy, now `appearance.mjs`. That one is the phase's
 *  own defect. `applySettings()` wrote an INLINE `--accent` on `:root` at import
 *  and REMOVED a host's `data-theme`, so a white-labelled build had its brand
 *  overwritten before its first frame (`docs/125` §2 F4). The rule "a host's
 *  brand outranks a visitor's stored preference" is now a function of two inputs
 *  rather than six lines in the middle of a DOM reflection, and can be driven red
 *  in node.
 *
 *  RE-MEASURED from the merged file after rebasing onto `origin/main`, not
 *  before: the trap the notes above record three times.
 *  Lowered to 16,579 by the structured-paste loss fix (`docs/129` §2). The round
 *  needed an import, a `pasteLossMessage` call and the comment saying why a
 *  SUCCESSFUL paste reports as `"error"` — and the file was at its ceiling with
 *  zero slack again, so it paid by removing a duplicated mechanism rather than by
 *  whitespace: `pasteStructured` and `pasteExternalStructured` differed only in
 *  which engine method they named and became one `runStructuredPaste(insert)`.
 *  That is worth more than the lines. Two copies of a paste path is precisely
 *  where the loss report gets wired into one and forgotten in the other — the
 *  `expectEditorFocused` lesson (`SKILL` §10) applied before the second patch
 *  rather than after it. RE-MEASURED from the merged file after rebasing onto
 *  `origin/main` at aa91bd0: the first number this branch carried was 16,606,
 *  measured against a `main` that then landed the host contract and took the
 *  ceiling to 16,589 underneath it. That is the merge trap the notes above record
 *  three times, and it is why this number is a measurement of the merged file and
 *  never arithmetic on two branches. */
/** Lowered to 16,545 by the version history UI (`docs/139`, `docs/140`; HF-068 /
 *  `105` OO-004). The file was AT its ceiling with zero slack again, and the
 *  surface needed lines for the preview swap, the store's four seams and the
 *  command — so THREE extractions paid for it, every one of them worth having on
 *  its own rather than a file split to make room:
 *
 *    `confirm_dialog.mjs`    the application's one yes/no card
 *    `name_dialog.mjs`       "ask for one line of text", now the single
 *                            implementation behind BOTH the create-style card and
 *                            Name this version — the staged-result dance that
 *                            keeps Escape from leaving a promise pending is
 *                            subtle enough that a second copy is a second bug
 *    `document_metadata.mjs` the Document properties dialog, whole
 *
 *  Re-measured from the file AFTER rebasing onto #652 (SDK phase 3), which had
 *  lowered this number to 16,574 underneath the branch. Carrying the branch's own
 *  earlier figure forward would have been arithmetic on two branches' numbers,
 *  which the note above says is always wrong. */
/** Lowered to 16,489 by the cheap table-experience round (`docs/141` TBL-01 …
 *  TBL-05). The file was AT its ceiling with zero slack again, and the round
 *  needed lines in six places — the Tab boundary, the unmerge command, a
 *  cell-scoped refusal, the band's disabled reasons, the hover tooltip that was
 *  overwriting those reasons, and the pointer-down that stops destroying a row
 *  selection — so it paid with ONE extraction that is worth more than the lines:
 *
 *    `table_commands.mjs`   the whole 187-line `table.*` command tree
 *    `tableContextLabel`    the band's own context hint, into `table_band.mjs`
 *
 *  Worth more than the lines because that tree reached the application only
 *  through editor bindings it closed over, so nothing could ask it a question:
 *  "does Unmerge call `splitMergedCell` with ONE argument" — the entire content of
 *  TBL-02 — needed a browser and a merged table to answer, and is now a node
 *  question with a stand-in engine (`table_commands.test.mjs`). It also puts the
 *  command tree beside `table_band.mjs`'s enablement rules, which is the other
 *  half of the same subject: TBL-03 (nine band buttons disabled with no stated
 *  reason while the same commands explain themselves in the menu) is exactly the
 *  defect those two living apart produced.
 *
 *  Re-measured from the file AFTER rebasing onto `origin/main`, not before:
 *  arithmetic on two branches' numbers is the trap the notes above record
 *  four times. */
/** Lowered to 16,464 by the table chrome layer (`docs/141` D-1). The layer added
 *  a whole pointer/keyboard/touch gesture family and this file came DOWN, because
 *  the new code went into `table_chrome.mjs` and took the old column-drag with it:
 *  `startTableColumnResize`, `updateTableColumnResize`, `finishTableColumnResize`,
 *  `cancelTableColumnResize`, the `tableResizeDrag` variable and the handle
 *  painting all left. What stayed here is routing — one call per pointer event —
 *  which is what `main.js` should be.
 *
 *  RE-MEASURED from the merged file after rebasing onto `origin/main`, never
 *  carried forward from the branch: the branch carried 16,466 against a `main`
 *  that then landed the version-preview chrome fix and took its own number to
 *  16,487 underneath it, so 16,464 is a measurement of the merge and not
 *  arithmetic on two branches' figures. */
/** Lowered to 16,453 by the dead-binding guard below, which is the ratchet
 *  gaining an ally rather than the file being trimmed: the guard found three
 *  top-level bindings nothing read — `spacingMenu`, `styleCardsEnabled` and
 *  `reviewReplyParent` — and they went with the statements that only wrote to
 *  them, ten lines that had been sitting under this ceiling being paid for
 *  twice. One line went back in, the version panel's `capabilities` seam.
 *
 *  RE-MEASURED from the file after rebasing onto `origin/main`, which had moved
 *  under the branch twice while this was in flight. */
/** Lowered to 16,446 by the round that made PR #674's four engine capabilities
 *  reachable. It needed a References surface row, a contextual-group hook and a
 *  table-of-contents command host, and the file was AT its ceiling with zero
 *  slack — so it paid with two extractions rather than with whitespace, which is
 *  the mistake the 17,054 note above records. What came out: the two
 *  Layout/References ENABLEMENT rules, which are a pure decision over an O(1)
 *  state bag read by four callers (`ribbon_surface.mjs`); and `stepTableBand`,
 *  whose whole job was turning a caret node into the page and cell rectangle the
 *  table chrome layer already owns (`table_chrome.mjs` `stepCaretBand`).
 *
 *  MEASURED FROM THE MERGED FILE after rebasing onto `origin/main`, which had
 *  lowered this to 16,453 underneath the branch. */
/** Lowered to 16,355 by the footer language picker. The file was AT its ceiling
 *  with zero slack and the round needed three lines of it, so it paid by taking
 *  out the thing the round actually needed a seam on: the 90-line ANCHORED
 *  POPOVER MANAGER — `openPopover`, `closePopover`, `registerPopover`,
 *  `focusFirstIn`, `onButton` and the two document-level listeners that light-
 *  dismiss them — now `popover_manager.mjs`.
 *
 *  Worth more than the lines, and this is the HF-085 argument in its plainest
 *  form: the rule "a small menu hangs off a button until you point somewhere
 *  else" was reachable only from the file that happened to hold it, so every
 *  surface wanting it had to be built in `main.js` too, and the ones built in
 *  modules got it by having the manager handed down a function at a time
 *  (`page_setup.mjs` takes `registerPopover` as an argument). The footer's
 *  language picker lives in `locale_boot.mjs`, where the rest of the language
 *  lifecycle already is, and it now imports the same manager the ribbon's
 *  eighteen menus use instead of `main.js` growing a nineteenth copy of the
 *  behaviour. The one thing the manager asks the editor — "is there anything to
 *  act on" — is now an injected predicate, which is also what let the language
 *  picker opt OUT of it: it is chrome, not a document command, and it must not
 *  go dead while the engine is still loading.
 *
 *  RE-MEASURED FROM THE MERGED FILE after rebasing onto an `origin/main` that
 *  had meanwhile lowered this to 16,446 under the branch. Neither that number
 *  nor the branch's own 16,355 describes the merge — the two rounds took
 *  different code out and both deletions applied — which is the trap the notes
 *  above record five times.
 *
 *  RE-MEASURED AGAIN on the phone-chrome branch (docs/148). The responsive
 *  ladder — `REVIEW_SHEET_MAX_WIDTH`, its media query and its change handler —
 *  moved into `phone_chrome.mjs`, which paid for the phone tier's own wiring.
 *  The eight-grip work landed underneath it and had already extracted the
 *  modifier rules and the commit origin into `object_snap.mjs`, so neither
 *  branch's number is this tree's: 16341 is what that merged file counted.
 *  Carrying either side forward would publish a ceiling the file never had,
 *  which is the merge trap this block records five times over.
 *
 *  RE-MEASURED AGAIN on the rotation branch. The phone work lowered it to
 *  16,336 underneath, and rotation extracted its own gesture controller, so
 *  the merged file counts 16,208 — lower than either side, because both
 *  extractions landed together. Measured, not carried.
 *
 *  Lowered to 16,190 by proofing Increment B (`docs/146` §6, ADR-042). The file was
 *  AT its ceiling with zero slack and the round needed four lines of it — a
 *  Review-band button binding, a surface row, a palette row and an import — so it
 *  paid the way the 16,446 note above describes: by extracting the thing the round
 *  needed a seam on. What came out is the whole of proofing's WIRING —
 *  `createSpellChecker`'s `io` block, `replaceMisspelling`, and the two remembered
 *  switches `setSpellCheckEnabled`/`setGrammarCheckEnabled` — now
 *  `proofing_chrome.mjs`, which also hosts the pack lifecycle, the host providers
 *  the SDK boundary needs (ADR-042 §6) and the "Proofing languages" dialog's
 *  opener. Nothing about proofing is decided in `main.js` any more.
 *
 *  MEASURED from this tree after the extraction, not calculated: the io block came
 *  out and a shorter io block went back in, so the arithmetic on either half would
 *  have been wrong in both directions.
 *
 *  Lowered to 16,189 by the reading-measure cap (`docs/154` §5.1, ADR-048). The
 *  file was AT its ceiling again and the round needed two lines of it — the width
 *  control's four command rows, and a `setEnabled()` in the relabel hook, because
 *  the ribbon button's label is the chosen STEP's and not the markup's. It paid
 *  with the thing it needed a seam on: the four rows are generated inside
 *  `reflow_chrome.mjs` from the one step table and spread into the registry, so
 *  `main.js` carries one line for four commands and the three-line rationale that
 *  used to sit over `view.reflow` moved to live with the control. MEASURED from
 *  this tree, not carried from the branch.
 *
 *  Lowered to 16,168 by the chrome-capability-guards round, which added no lines
 *  to `main.js` at all and is paying down a ratchet that had gone STALE. #732
 *  moved the ribbon's whole tooltip subsystem into `ribbon_tooltip.mjs` and left
 *  this number where it was, so the file sat 21 lines under its own ceiling — and
 *  a ceiling with slack in it is not a ratchet, it is an allowance. RE-MEASURED
 *  FROM THE MERGED TREE at `origin/main` e8e7ed8b (`wc -l src/main.js` = 16168),
 *  which is the only number that describes this file: #732's extraction and the
 *  reflow round's both landed, so arithmetic on either branch's figure would be
 *  wrong in both directions — the merge trap the notes above record six times
 *  over.
 *
 *  RE-MEASURED AGAIN, to 16,178, and the re-measurement is itself the lesson.
 *  This branch had measured 16,168 from `origin/main` at e8e7ed8b; #736 then
 *  landed ten lines in `main.js` — a `measure: () => measurement` getter and a
 *  `region: () => navigator.language` — under a ceiling that was still 16,189,
 *  so it was legal there and inconsistent with this branch's number the moment
 *  the two met. Neither 16,168 nor #736's implicit 16,189 describes the merged
 *  file; `wc -l` does, and it says 16,178. That is the third time in one session
 *  this exact arithmetic has been wrong, which is why the rule is to measure
 *  LAST, after the rebase, and never to carry a number across one.
 *
 *  Lowered to 16,177 by the folding round (`109` FOLD-004), which is the ratchet
 *  doing its job with ONE line of headroom to work in. The chrome
 *  (`fold_chrome.mjs`, `fold_view.mjs`) was complete, translated into nineteen
 *  locales, and imported by nothing — `SKILL.md` §9 rule 4, "built" is not
 *  "reachable" — and reaching it costs five lines here plus a construction
 *  block. It paid with two rules that had no business being in this file and
 *  could not be tested from it:
 *    * Change case's cross-run RE-SLICE moved to `text_rules.mjs` beside
 *      `transformCase`, where four guards now cover the formatting-boundary
 *      case and the `ß → SS` length change that silently drops the end of a
 *      selection;
 *    * the font-size ladder and its stepper moved to `style_picker.mjs` beside
 *      `previewPx`, as `nextZoomStep`'s sibling, where the past-the-end
 *      behaviour and the `w:sz` clamps are asserted rather than pressed.
 *  MEASURED with `wc -l` on this tree AFTER the rebase onto `origin/main`
 *  564db686, not carried from the branch.
 *
 *  Lowered to 16,065 by the UX-fix round (Ctrl+H, the heading chords, F6 region
 *  cycling, the findings dialog, style display names, disabled-control reasons).
 *  The file was AT its ceiling with zero slack, and the round needed registry
 *  rows, a construction or two and its call sites — so it paid by extracting the
 *  Insert-table size grid whole into `table_grid_picker.mjs` (~120 lines), which
 *  was also the round's own subject: its size label disagreed with the Table
 *  band's, and the two now read one formatter. Every new behaviour lives in its
 *  own module (`region_focus`, `quick_styles`, `style_names`, `compat_findings`,
 *  `control_reasons`), so `main.js` carries their wiring and nothing else.
 *  MEASURED with `wc -l` from the tree AFTER rebasing onto `origin/main` 0e441fc
 *  (#809), not carried from the branch, which had read 16,066.
 *
 *  Lowered to 16,047 by the object-editing round (`109` HF-166, HF-214, HF-252,
 *  HF-254, HF-106, HF-259, HF-253). The file was AT its ceiling with zero slack
 *  again, and the round needed Change picture, the picture border, the crop
 *  keyboard, the refused-drag reason and the group-member Delete wired in. Every
 *  one of them is a module (`object_keys`, `object_refusal`, `picture_replace`,
 *  `text_box_body`), so `main.js` carries their construction and call sites; it
 *  paid by moving the image decode and the placeable-type list into
 *  `picture_replace.mjs` (Insert ▸ Picture imports them back, so Insert and
 *  Change cannot accept different files), by folding the crop session's
 *  Enter/Escape block into `handleCropKey`, and by replacing the right-click
 *  menu's hand-copied selected-object context with `selectedObjectContext()`.
 *  MEASURED with `wc -l` on this branch; re-measure after any rebase. */
const MAIN_JS_LINE_CEILING = 16047;

/** Modules that must stay free of the browser: they are the ones a unit test,
 *  a host page or a non-DOM runtime can use, and the only thing that keeps
 *  them that way is that adding a `document.` to one fails here. */
const PURE_MODULES = [
  // The browser's end of a shared session (ADR-063). Its socket opener, its
  // timers and its randomness are all injected, which is what lets a dropped
  // connection, an exhausted retry budget, a replayed outbound queue and the
  // whole backoff curve be driven in node. Purity is load-bearing twice here:
  // a module that reached for `WebSocket` could not be handed a host's own
  // transport, and a backoff only a stopwatch can observe is a backoff nothing
  // checks — which is `docs/107` §4's rule about counts rather than clocks,
  // one layer out. The DOM half is `main.js`'s status line.
  "collab_transport.mjs",
  // Holds the vertical goal column and nothing else: no DOM and no engine, so
  // the arrow-key rule is unit-testable as a plain state machine.
  "caret_navigation.mjs",
  "command_taxonomy.mjs",
  // The responsive ladder and the soft-keyboard inset (docs/148). Its window,
  // body and root are injected rather than reached for, which is the only
  // reason a keyboard-inset calculation can be driven from Node at all.
  "phone_chrome.mjs",
  // The touch-selection machine (docs/105 UX-018). Its window, its document
  // surface and its element factory are all injected, which is the only reason
  // the long-press threshold, the handle hit radius, the endpoint ordering and
  // the magnifier's flip at the top of the screen can be driven from node —
  // Playwright cannot put a finger on the top edge of a phone at will, and
  // arithmetic that only a browser can reach is arithmetic nothing checks.
  "touch_selection.mjs",
  // The Symbol / Emoji sets: literal data with no behaviour, so nothing in it
  // has any business reaching a global.
  "glyph_sets.mjs",
  // The localisation seam. Purity is the point: a locale's plural rules and
  // fallback chain are answerable without a document existing, which is what
  // lets `i18n.test.mjs` drive Russian's three plural forms in node. The DOM
  // half — walking `data-i18n` attributes — is `localize.mjs`.
  "i18n.mjs",
  "contrast.mjs",
  // The white-label contract: which tokens a host may set, what a refusal says,
  // and the stylesheet their choices produce. Purity is the whole reason it left
  // `tools/build-brand.mjs` — the configuration playground runs the SAME validator
  // in a browser so a host reads the refusal the command line would give them, and
  // a browser cannot import a module that opens with `node:fs`.
  "brand_contract.mjs",
  // How a palette is read out of stylesheet TEXT, with nothing that knows where
  // the text came from. Same split, same reason: the generator reads the file, the
  // page fetches it, and there is one parser.
  "palette_parse.mjs",
  // The proofing contract and the whole of what decides a finding. Purity is
  // load-bearing twice over here (`docs/146` §4, ADR-042): it is what lets the
  // Web Worker and the in-process fallback run the SAME function rather than
  // two implementations of one rule, and it is ADR-042 §6's requirement that
  // proofing be a separate optional package whose network and storage the HOST
  // injects — a module that reached for `fetch` could not be given to one.
  "proof_protocol.mjs",
  // The pack system's decisions (`docs/146` §6, ADR-042 §6): the manifest schema,
  // the URL policy, the 50 MB ceiling, the quota plan, the refusal codes and the
  // eight-step installer as a state machine over INJECTED effects. Purity is what
  // makes the failure cases testable at all — a corrupted download, an over-quota
  // origin and a host that refuses the download are three lines of node here and
  // three fixtures nobody writes otherwise. It is also why `digest` is injected
  // rather than reached through a crypto global: that would trip this guard, and it
  // would make a checksum MISMATCH undrivable without corrupting a real file.
  "proof_packs.mjs",
  // Reflow's cost model (`docs/151` §6.2): which widths count as the same width,
  // how long to wait before believing one, and when the engine will refuse. Those
  // decide whether a resize is O(1) or O(document) in the reader's file, which is
  // a `docs/107` §4 constraint and far too expensive a question to be answerable
  // only in a browser against a stopwatch. Its timers are INJECTED for the same
  // reason `proof_packs.mjs`'s digest is: reaching for `setTimeout` through a
  // global would trip this guard, and it would make "sixty resize events cost
  // nothing" undrivable without waiting on a wall clock. The DOM half is
  // `reflow_chrome.mjs`.
  "reflow_view.mjs",
  // The caption / cross-reference VOCABULARY: which "Insert reference to"
  // options a reference type offers, when Word offers "Include above/below",
  // and what a caption will read as. Word's own rules, with no widget attached,
  // so `cross_reference_model.test.mjs` can drive every type in node — and
  // ONLYOFFICE carries the same mapping as a switch inside a Backbone view,
  // where nothing can test it.
  "cross_reference_model.mjs",
  // The object right-click menu. It builds plain command descriptors and reaches
  // the application only through its `io`, which is what lets
  // `object_context_menu.test.mjs` assert what a picture, a shape and a text box
  // offer, in each review mode, without a browser.
  "object_context_menu.mjs",
  "edit_errors.mjs",
  // The field vocabulary: the kind table plus the host-side result formatter.
  // No DOM and no engine, so "what does a date field cache" is a node question.
  "field_kinds.mjs",
  // The whole target -> cursor mapping. Its DOM half is `pointer_hover.mjs`;
  // keeping them apart is what lets `pointer_cursor.test.mjs` drive the entire
  // hover cascade with plain objects, with no browser and no engine.
  "pointer_cursor.mjs",
  // The swatch vocabularies: literal tables and one name->hex lookup, so the
  // question "does the picker offer every highlight the engine can write?" is
  // answerable against the Rust source in node (`palettes.test.mjs`).
  "palettes.mjs",
  // The memoized "does this document hold an object?" answer (`109` HF-183). The
  // engine call is injected, which is what lets `object_presence.test.mjs` count
  // engine calls and payload bytes at n and 2n objects in node — a complexity
  // guard, where a browser could only offer a stopwatch.
  "object_presence.mjs",
  "popover_position.mjs",
  // The radio-group pattern. It is handed its container and never reaches for a
  // global one, which is what lets `radio_group.test.mjs` drive the arrow
  // arithmetic in node and what would let a host mount a segmented control of
  // its own on the same contract.
  "radio_group.mjs",
  // Which command each Home / View / Table ribbon control stands for (`109`
  // UX-005). Selectors and ids only: the caller supplies the root, so the table
  // is checkable in node and cannot quietly grow a DOM opinion.
  "ribbon_faces.mjs",
  // Which stored style Ctrl+Alt+2 means, and what a built-in style is CALLED.
  // Both are decisions over a list of names with no DOM and no engine, which is
  // what lets `quick_styles.test.mjs` and `style_names.test.mjs` drive them.
  "quick_styles.mjs",
  "style_names.mjs",
  "review_labels.mjs",
  "review_layout.mjs",
  // Takes nodes as arguments and never reaches for a global one, which is what
  // lets `shortcut_labels.test.mjs` drive the sweep with plain objects.
  "shortcut_labels.mjs",
  // Every rule that decides whether a word is misspelled and what to suggest
  // instead. The DOM half is `spell_check.mjs`; keeping these apart is what
  // lets `spelling.test.mjs` run the whole dictionary through them in node.
  "spelling.mjs",
  "status_policy.mjs",
  // The Table band's structural controls. Handed its root and its engine, so the
  // operation tables are set-comparable against `editor.html` in node.
  "table_band.mjs",
  // The `table.*` command tree. Every editor binding arrives in its `host`, so
  // "what does the Table menu offer on a merged table, and what does Unmerge
  // actually call" is a node question with a stand-in engine — which is the only
  // way TBL-02's real assertion (ONE argument to `splitMergedCell`) can be made
  // without a browser and a merged table.
  "table_commands.mjs",
  "text_rules.mjs",
  "units.mjs",
  // What the version timeline SAYS: one sentence per `HISTORY_STATUS` code, the
  // day grouping, a row's words, and the five different reasons the entry can be
  // disabled. Purity is the point — the store returns codes and no English, so
  // "does every refusal have a sentence" and "does a version written at 23:30
  // group under today" are node questions. The DOM half is `version_panel.mjs`,
  // the same split `status_policy.mjs` / `status_channel.mjs` already uses.
  "version_policy.mjs",
];

const BROWSER_GLOBALS = /\b(document|window|navigator|localStorage|sessionStorage|indexedDB|globalThis)\s*\./;

function read(name) {
  return readFileSync(new URL(name, SRC), "utf8");
}

/** Source with comments removed, so prose about "the open document." is not
 *  mistaken for a DOM access. */
function code(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/** Every script under `src/`, page entry points included. */
function scriptFiles() {
  return readdirSync(SRC).filter((f) => /\.(mjs|js)$/.test(f) && f !== "main.js");
}

/** The importable modules. The convention this encodes: `.mjs` is a module
 *  something imports, `.js` is a page's entry script (`main.js` for the
 *  editor, `fidelity.js` and `home-embed.js` for the site pages), which runs
 *  for its side effects and has nothing to export. */
function moduleFiles() {
  return readdirSync(SRC).filter((f) => f.endsWith(".mjs"));
}

test("main.js is at or below its ratchet", () => {
  const lines = read("main.js").split("\n").length - 1;
  assert.ok(
    lines <= MAIN_JS_LINE_CEILING,
    `main.js is ${lines} lines, ${lines - MAIN_JS_LINE_CEILING} over the ceiling of ` +
      `${MAIN_JS_LINE_CEILING}. Put the new code in a module under webapp/src/ and ` +
      "import it, or take something else out — do not raise this number.",
  );
});

test("the ceiling is not left stale after an extraction", () => {
  const lines = read("main.js").split("\n").length - 1;
  assert.ok(
    MAIN_JS_LINE_CEILING - lines <= 200,
    `main.js is now ${lines} lines but the ceiling still reads ${MAIN_JS_LINE_CEILING}. ` +
      "Lower it to the new measurement in the same commit, or the slack it leaves is " +
      "room for the file to grow back for free.",
  );
});

// A file in `src/` that exports nothing is another main.js in miniature: its
// contents are reachable only by loading it for its side effects.
test("every module beside main.js exports something", () => {
  const silent = moduleFiles().filter((f) => !/^\s*export[\s{]/m.test(read(f)));
  assert.deepEqual(
    silent,
    [],
    "a module with no exports cannot be unit-tested, mounted or reused — which " +
      "is the whole of HF-085, one file smaller",
  );
});

test("the pure modules stay free of the browser", () => {
  for (const name of PURE_MODULES) {
    const offending = code(read(name))
      .split("\n")
      .map((line, i) => [i + 1, line])
      .filter(([, line]) => BROWSER_GLOBALS.test(line));
    assert.deepEqual(
      offending,
      [],
      `${name} must stay pure: the DOM half belongs to the caller. Reaching a ` +
        "browser global here puts the decision back out of reach of a unit test.",
    );
  }
});

// main.js imports the modules; a module importing main.js back would re-couple
// everything to the 18k-line file and make the mount seam (HF-109) impossible.
test("no module imports main.js", () => {
  const cyclic = scriptFiles().filter((f) => /from\s+["']\.\/main\.js["']/.test(read(f)));
  assert.deepEqual(cyclic, [], "the dependency runs one way: main.js → modules");
});

// ── Dead module-level bindings in main.js ──────────────────────────────────
//
// The line ratchet above cannot find these, and that is the whole reason this
// guard exists: dead code sits under a ceiling quite happily, and it is paid for
// twice — once in the ratchet, where it occupies lines an extraction had to buy,
// and once by the next reader, who has to work out whether the binding matters
// before they can change anything near it. Three were found when this landed —
// `spacingMenu`, `styleCardsEnabled` and `reviewReplyParent` — one of them a
// `document.getElementById` for a menu that another module already owns, and two
// of them variables the file assigns to and never reads again.
//
// WHY THIS IS DECIDABLE AT ALL, and only here. `main.js` has ZERO exports and
// `editor.html` has ZERO inline event-handler attributes, so nothing outside the
// file can name anything inside it: not a test (there is no import), not the
// markup (there is no `onclick="…"`), not another module (the test above forbids
// the import). Both of those premises are ASSERTED below rather than asserted in
// prose, so the day either stops holding this guard fails instead of quietly
// becoming wrong. It is deliberately not run over the other modules: those DO
// export, and "unreferenced inside its own file" says nothing about an export.
//
// WHAT THE SCANNER SEES:
//   * comments, string BODIES and regular-expression bodies are blanked, so
//     prose and data cannot look like code;
//   * `${…}` inside a template literal is kept, because it IS code — a binding
//     read only by an interpolation is read;
//   * `{ name }` shorthand and `...name` spread count as reads;
//   * `foo.name` and `foo?.name` do NOT count: a property is not this binding.
//
// WHAT IT CANNOT SEE, and therefore never reports:
//   * a name inside a plain string — deliberately: `"spacingMenu"` in a
//     `getElementById` is an element id, not a reference to a variable that
//     happens to share its spelling, and treating it as one is what would make
//     this guard unable to find the very binding it did find;
//   * a name reached dynamically (`globalThis[x]`, `eval`) — nothing in this
//     file does that, and if something did, this guard would be the least of it;
//   * a binding declared by DESTRUCTURING at column 0 (`const { a } = …`): not
//     collected at all, so such a binding is never reported either way;
//   * anything below the top level. A dead local is a smaller problem and a much
//     harder scan, and a scanner that guessed at scope would produce exactly the
//     false positive that gets a working line deleted.
//
// A WRITE IS NOT A READ. `let x = 0;` followed only by `x = 1` is dead: the
// value is never observed, so the binding and both statements can go. That is
// how the two `let`s above were found, and a guard that only counted
// OCCURRENCES would have called them live.

/** Source with comments, string bodies and regex bodies blanked, and template
 *  substitutions kept. Character-for-character the same length as the input, so
 *  a reported line number is the real one.
 *
 *  A scanner and not a parser, for the reason `tools/string_sites.mjs` gives:
 *  a parser is a second implementation of JavaScript to maintain, and what this
 *  needs is determinism plus a bias towards calling something live. */
function blankLiterals(src) {
  const out = [];
  const nest = [];
  let prev = "";
  let i = 0;
  const blank = (ch) => (ch === "\n" ? "\n" : " ");
  // A `/` opens a regex unless the previous significant character could end an
  // expression. The classic ambiguity, with the classic heuristic; it matters
  // because a regex such as `/["']/` would otherwise open a string literal and
  // blank the rest of the file.
  const regexHere = () => prev === "" || !/[\w$)\]"'`]/.test(prev);
  while (i < src.length) {
    const ch = src[i];
    const two = src.slice(i, i + 2);
    if (nest.at(-1) === "template") {
      if (ch === "\\") {
        out.push(" ", blank(src[i + 1] ?? ""));
        i += 2;
      } else if (ch === "`") {
        out.push("`");
        nest.pop();
        prev = "`";
        i += 1;
      } else if (two === "${") {
        out.push("$", "{");
        nest.push("subst");
        prev = "{";
        i += 2;
      } else {
        out.push(blank(src[i]));
        i += 1;
      }
      continue;
    }
    if (two === "//") {
      while (i < src.length && src[i] !== "\n") out.push(blank(src[i++]));
      continue;
    }
    if (two === "/*") {
      const end = src.indexOf("*/", i + 2);
      const stop = end === -1 ? src.length : end + 2;
      while (i < stop) out.push(blank(src[i++]));
      continue;
    }
    if (ch === '"' || ch === "'") {
      out.push(ch);
      i += 1;
      while (i < src.length && src[i] !== ch) {
        if (src[i] === "\\") {
          out.push(" ", blank(src[i + 1] ?? ""));
          i += 2;
          continue;
        }
        out.push(blank(src[i++]));
      }
      if (i < src.length) out.push(src[i++]);
      prev = '"';
      continue;
    }
    if (ch === "`") {
      out.push("`");
      nest.push("template");
      prev = "`";
      i += 1;
      continue;
    }
    if (ch === "}" && nest.at(-1) === "subst") {
      out.push("}");
      nest.pop();
      prev = "}";
      i += 1;
      continue;
    }
    if (ch === "/" && regexHere()) {
      out.push(" ");
      i += 1;
      let inClass = false;
      while (i < src.length) {
        const c = src[i];
        if (c === "\\") {
          out.push(" ", " ");
          i += 2;
          continue;
        }
        if (c === "[") inClass = true;
        else if (c === "]") inClass = false;
        else if (c === "/" && !inClass) {
          out.push(" ");
          i += 1;
          break;
        } else if (c === "\n") break;
        out.push(blank(src[i++]));
      }
      while (i < src.length && /[a-z]/.test(src[i])) out.push(blank(src[i++]));
      prev = ")";
      continue;
    }
    out.push(ch);
    if (!/\s/.test(ch)) prev = ch;
    i += 1;
  }
  return out.join("");
}

/** Every binding declared at the TOP LEVEL — column zero — with its line. */
function topLevelBindings(blanked) {
  const found = [];
  blanked.split("\n").forEach((line, index) => {
    const at = index + 1;
    const value = /^(?:const|let|var)\s+([A-Za-z_$][\w$]*)\b/.exec(line);
    if (value) found.push({ name: value[1], line: at });
    const fn = /^(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)\s*\(/.exec(line);
    if (fn) found.push({ name: fn[1], line: at });
    const cls = /^class\s+([A-Za-z_$][\w$]*)\b/.exec(line);
    if (cls) found.push({ name: cls[1], line: at });
  });
  return found;
}

/** Assignment operators. An occurrence followed by one of these is a WRITE, and
 *  the declaration itself is a write by the same rule — which is what makes
 *  "zero reads" mean "nothing ever observes this". `=>` and the comparisons are
 *  excluded, or every arrow function would read as an assignment. */
const ASSIGNS =
  /^\s*(?:=(?![=>])|\+=|-=|\*=|\/=|%=|\*\*=|&&=|\|\|=|\?\?=|&=|\|=|\^=|<<=|>>=|>>>=|\+\+|--)/;

/** How many times a binding is READ in `body`. */
function readCount(body, name) {
  const pattern = new RegExp(`\\b${name.replace(/\$/g, "\\$")}\\b`, "g");
  let reads = 0;
  for (const hit of body.matchAll(pattern)) {
    if (ASSIGNS.test(body.slice(hit.index + name.length))) continue;
    reads += 1;
  }
  return reads;
}

test("main.js keeps no module-level binding nothing reads", () => {
  const source = read("main.js");
  // The two premises this guard stands on. Without them "nothing in this file
  // reads it" would not imply "nothing reads it", and the guard would be an
  // argument for deleting working code.
  assert.equal(
    /^\s*export[\s{]/m.test(blankLiterals(source)),
    false,
    "main.js has grown an export, so a binding may now be read from outside it and " +
      "this guard's premise no longer holds. Either take the export out or scope this " +
      "guard to what is still private.",
  );
  const markup = readFileSync(new URL("../editor.html", SRC), "utf8");
  assert.deepEqual(
    [...markup.matchAll(/\son[a-z]+\s*=\s*"/g)].map((m) => m[0].trim()),
    [],
    "editor.html has grown an inline event-handler attribute, which can name a " +
      "main.js binding this scanner cannot see. Bind the listener in script instead.",
  );

  const blanked = blankLiterals(source);
  // A property access is not a reference to a binding of the same name, so
  // `.foo` and `?.foo` are removed before counting — but `...foo` is a SPREAD and
  // must survive, which is what the lookbehind protects.
  const body = blanked.replace(/(?<![.])(\?\.|\.)\s*([A-Za-z_$][\w$]*)/g, ".");
  const dead = topLevelBindings(blanked)
    .filter((binding) => readCount(body, binding.name) === 0)
    .map((binding) => `main.js:${binding.line} ${binding.name}`);
  assert.deepEqual(
    dead,
    [],
    "a top-level binding in main.js that nothing reads. Delete it, and the " +
      "statements that only write to it: it costs lines against the ratchet above " +
      "and costs the next reader the work of proving it does not matter. If it IS " +
      "reachable in a way this scan cannot model, say how in a comment here rather " +
      "than leaving the guard to be argued with.",
  );
});
