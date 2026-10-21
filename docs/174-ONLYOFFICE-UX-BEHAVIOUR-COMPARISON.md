# 174 — ONLYOFFICE Docs 10.0.0 compared with OpenDoc, interaction by interaction

**Status:** dated investigation, 2026-10-22. Measured against `main` at `e8133641`..`7ab5b24a`.
Not a queue: the work it found is tracked in `docs/109` (row UX-050 for the findings no lane
has taken yet, and the rows each build lane adds when it lands a fix).
**Bar:** Word and Google Docs, not ONLYOFFICE. A row can conclude that OpenDoc is already better.

## Why this exists, and how it differs from `153`

`153` is the derived **capability** matrix: does a feature exist on each side. This document
is the **behaviour** comparison the matrix cannot make: for features OpenDoc already ships,
what actually happens when a person uses them, in both editors, on the same document.

## Method

- ONLYOFFICE Docs **10.0.0** (`onlyoffice/documentserver`, arm64), run locally in Docker
  with its example host. OpenDoc from a local build of `main`.
- Three investigation lanes, one per area: text authoring, review and collaboration,
  objects and layout. Each drove both editors through the same scripted Playwright
  sequences and the same input documents. Collaboration was tested with two real
  clients in each product: ONLYOFFICE's own co-editing, and OpenDoc's relay.
- Every OpenDoc claim comes from **running** it (SKILL §9.4). Where a row cites
  `main.js:N` it was read at the commits above.
- ONLYOFFICE is AGPL-3.0. This document describes behaviour and cites identifiers and
  shipped-file line numbers only. **No ONLYOFFICE code is reproduced here.** REA
  (reverse-engineering MCP) evidence ids are cited where its page and source captures
  worked. Its deep bundle analysis of ONLYOFFICE's 9 MB engine script hung or dropped
  the connection, so engine behaviour is evidenced by running it.
- Paths written `(local) …` are the investigation's own scripts, logs and screenshots.
  They were not committed.

## P0s and who is fixing them

| Id | What | Verified by | Owner |
| --- | --- | --- | --- |
| TA-01 | Shift+Enter leaves the caret before the new line break, so `one`⇧↵`two` saves as `onetwo` + `<w:br/>`; the existing spec cannot fail | saved DOCX re-read; `insertLineBreakAtSelection` | tables/shell lane (owns `main.js`) |
| OL-01 | A click in the empty page area below the last paragraph places no caret; typing is silently discarded | `main.js` hit-test miss returns `null` | tables/shell lane |
| OL-02 | A picture floated in OpenDoc saves `wp:anchor` without the schema-required `relativeHeight`, a wrap element without `wrapText`, and a new tight/through wrap without its polygon (candidate: Word not available to confirm) | `semantic.rs` writer read | object lane |
| RC-01 | Concurrent typing in one paragraph reverses the later typist's run on both clients | two clients through the relay; `paintArrival` documents "the caret is NOT mapped (`107` P-4)" | co-editing lane |
| RC-02 | A remote insert earlier in your paragraph leaves your caret at a stale offset | same root cause as RC-01 | co-editing lane |
| RC-03 | Undo in a shared session removes the other participant's text | two clients | co-editing lane |
| RC-04 | Restoring a version while shared silently forks the document | two clients | co-editing lane |

Findings already with a build lane:
- **Tables/shell lane:** OL-03 (Tab selects the cell's contents).
- **Object lane:** OL-04 (wrap change reflows at once), OL-13 (an inserted picture is selected), OL-14 (side-handle hit area), OL-16 (undo labels).
- **Co-editing lane:** RC-08 (the sync status is driven by acknowledgements).
- **Review lane:** RC-13 (the ODC-7006 message claims lost work that the recovery design keeps), RC-11 (edit the root comment), RC-12 (quote the anchored text), RC-15 and RC-16 (focus after posting; Cmd/Ctrl+Enter submits).

Every other P1 is listed in `docs/109` UX-050. Participant identity (RC-05, RC-06, RC-07) and @mention (RC-10) wait on a host identity-and-directory contract that has not been designed yet.

---

## Text authoring: ONLYOFFICE Docs 10.0.0 vs opendoc

Lane: text authoring (typing, caret, selection, navigation, clipboard, undo, find/replace, formatting shortcuts, auto-format, autocorrect, spell check, context menu, zoom/scroll). Date: 2026-10-22. Bar = Word and Google Docs, not ONLYOFFICE.

### Method

- **Same input in both editors.** Both opened ONLYOFFICE's own blank template (`document-templates/new/en-US/new.docx`, copied to `(local) ta-docs/ta-blank.docx`). Identical Playwright scripts drove both, with headless Chromium 1248, macOS key mapping and a 1440x900 viewport. Every opendoc claim below comes from running it at http://localhost:8120/editor.html, not from reading code.
- **ONLYOFFICE host.** The example host's editor page was broken during this run: its `api.js` `src` was rendered as `http://localhost:8090/`, so it never loaded. ONLYOFFICE was therefore opened through a minimal scratch host (`(local) ta-host/host.html`, served on :8177). That host calls `DocsAPI.DocEditor` against the same Document Server, fetches the document from the example's `/example/download`, and has no save callback. No container or service was restarted.
- **How state was read.**
  - ONLYOFFICE selection: its exported `asc_GetSelectedText` (sdk-all-min.js `prototype.asc_GetSelectedText`), plus `asc_getCanUndo`.
  - ONLYOFFICE toolbar state: the `.active` class on `#id-toolbar-btn-bold` and the other toolbar buttons.
  - opendoc selection: its own `copy` handler (main.js:14109, through `copySelection` at main.js:6327).
  - opendoc document text: the `#a11yDocument` mirror.
  - opendoc toolbar state: `aria-pressed`.
  - Caret: `.overlay .caret` in opendoc and `#id_target_cursor` in ONLYOFFICE. Caret positions were proven by typing a marker and reading the text back.
- **Artefacts.**
  - Scripts: `(local) ta-scripts/*.mjs`
  - Raw logs: `(local) ta-scripts/*-od.log` / `*-oo.log`
  - Screenshots: `(local) oo-ux/ta-shots/{od,oo}-*.png`
  - Saved files: `(local) ta-docs/out/*.docx`
- **REA evidence.** All three runs are static analysis of the shipped web-apps sources (ONLYOFFICE's UI layer), cited with file:line:
  - `ev_bf49b95c66b5abc6166d36b258411b405cd80c2992f8c5d8ba270f20fb6727b0`: web-apps `common/main/lib/controller/PasteOptions.js`
  - `ev_1dd267af85a69f88d1925c9d001aefb24e08cf40a892ce89595d3723c8197d99`: `documenteditor/main/app/controller/Search.js` + `DocumentHolderExt.js`
  - `ev_ebd15c8791b2c72d6736817a578c3d77e2ed41aec0dde5c4e618d3df91a26d89`: `documenteditor/main/app/controller/DocumentHolder.js` + `Main.js`
  - `ev_b76a5b06076c971dbde23d57d8e41d699d9745fb4ac7a6c97d06fcb27fba8290`: `common/main/lib/view/AutoCorrectDialog.js`

  `sdk-all.js` is minified: only the exported `asc_*` names survive, so those are cited by name. Live listener inspection did not work (see "Not tested").
- ONLYOFFICE is AGPL-3.0. This document describes its behaviour and cites identifiers and line numbers only. No ONLYOFFICE code is reproduced.

Priority key: **P0** data loss or correctness · **P1** daily-use friction · **P2** polish · **P3** nice-to-have.
Domains (SKILL §7):
- webapp = `webapp/**` (main.js has a single owner)
- wasm = `crates/casual-doc-wasm/**`
- layout = `crates/casual-doc-layout/**`
- export = `crates/casual-doc-export/**`, `crates/casual-doc-io/**`

### Ranked findings

| id | P | interaction | ONLYOFFICE behaviour + evidence | opendoc behaviour + evidence | verdict | recommended change | domain |
|---|---|---|---|---|---|---|---|
| TA-01 | P0 | Shift+Enter (soft line break) | Breaks the line at the caret. "Line1" and "line2" render on two lines (`oo-softbreak.png`). | **The break is put in the wrong place.** Typing `one`, Shift+Enter, `two` gives "onetwo" on one line. The saved DOCX holds `<w:t>onetwo</w:t><w:r><w:br w:type="textWrapping"/></w:r>`: the break sits **after** the text typed following it. Reproduced in both the blank template and `?fixture=rich` (`ta-docs/out/se-blank.docx`, `se-rich.docx`, `od-softbreak.png`).<br>Undo is labelled "Line break", so the op exists, but the caret does not advance past the break.<br>The console logs `edit ignored: at document start` twice: the warn at main.js:8519/8662/10467 is reached, which suggests a second insert path (the beforeinput handler for `insertLineBreak`, main.js:14158) also fires.<br>Code path: keydown Shift+Enter → `insertLineBreakAtSelection` (main.js:14580 → 9474-9490 → `doc.insertLineBreak`).<br>`tests/e2e/line-break.spec.mjs:36` passes anyway: it asserts the mirror reads "onetwo", which is true with the break in either place. | ONLYOFFICE better; opendoc below standard. Silent misplacement that persists into the saved file. | After `insertLineBreak`, set the caret **after** the inserted break, the way `insertInlineObject` callers do. Swallow the duplicate `beforeinput` insertLineBreak once keydown has handled it. Fix line-break.spec so it can fail: assert the break's offset or the two-line layout, and check the saved `w:br` position. | wasm (caret after insert) + webapp main.js |
| TA-02 | P1 | Select all + Delete in a document whose **first block is a table** | Cmd+A then Backspace or Delete empties the document, table included (`td3-oo.log`). | Cmd+A inside a cell selects that cell's contents only ("Cell contents selected — choose Select All again", main.js:13599). A second Cmd+A selects the document.<br>Backspace then clears the text but **leaves an empty table**. A further Cmd+A, Delete is refused with "That edit isn't supported for this selection yet" (edit_errors.mjs:14). From that point the user cannot clear the document (`td3-od.log`, `od-selall-del-table.png`, `od-ctx-sel.png`). With the table **not** first, select-all + delete works (`td-od.log`). | ONLYOFFICE better | A whole-document range that starts or ends inside a table must delete the table (Word: Ctrl+A, Delete leaves one empty paragraph). Treat the document-start table as deletable structure, and never refuse a select-all delete. | wasm (range delete over tables) + webapp |
| TA-03 | P1 | Incremental find while typing the query, then Replace | Typing "fox" leaves the counter at **1/4**: the first match after the caret stays current (Search.js:163-167 `onSearchNext` → `asc_findText`). Replace then changes `fox1` (`p0-oo.log`). REA ev_1dd267af… | Every keystroke in the query runs `findFromSelection(true)` (main.js:13327-13330), which steps **forward from the previous match**. "f", "fo", "fox" end on **4 of 4**. Replace then changes `fox4`, giving `fox1 fox2 fox3 CAT4` (`p0-od.log`, `b3-od.log`). | ONLYOFFICE better; opendoc replaces a match the user did not navigate to. | Anchor incremental search at the position where find was opened (or the current match start). Re-search from that anchor on each input, and advance only on Enter or Next. | webapp main.js |
| TA-04 | P1 | Find: highlight every match | All matches are painted yellow, the current one in a distinct colour, with an `n/N` counter in the box (`oo-find-next.png`). Paint is driven by `asc_selectSearchingResults` (Search.js:472). REA ev_1dd267af… | Only the current match is selected. There is no all-match highlight, though the status shows "1 of 4" (`od-find-next.png`). | ONLYOFFICE better; Word and Docs both highlight all matches. | Paint every match as an overlay layer, reusing the find result list already computed for "n of N". | webapp + layout (overlay rects) |
| TA-05 | P1 | Auto-format lists while typing ("1. ", "- ", "* ", "a. ", "1) ", "> ") | Converts to a real list on space. Enter continues the list, Tab nests ("a." level), Enter on an empty item ends the list. **Cmd+Z right after the conversion restores the literal "1. "**. Backspace at an empty item removes the bullet (`edit-oo.log`, `oo-autolist-*.png`). Toggled by `asc_SetAutomaticBulletedLists` / `asc_SetAutomaticNumberedLists` (Main.js:3340-3344, AutoCorrectDialog.js:300). REA ev_ebd15c87…, ev_b76a5b06… | None: the text stays literal "1. first", and so on (`edit-od.log`). No such feature in `text_rules.mjs` or main.js. | ONLYOFFICE better; Word and Docs both do this. | Add an auto-format-as-you-type rule set: list triggers on space after the marker. Make it one undo step whose immediate Cmd+Z reverts to the literal text. Add a setting toggle. | webapp (input rules) + wasm (apply list) |
| TA-06 | P1 | Autocorrect while typing | "--" → em dash and first letter of a sentence capitalised ("hello. world" → "Hello. World"); Cmd+Z undoes the capitalisation. Double space → ". " (`asc_SetAutoCorrectHyphensWithDash`, `…FirstLetterOfSentences`, `…DoubleSpaceWithPeriod`, Main.js:3352-3387).<br>Does **not** convert (c), (tm), 1/2, "teh" or 1st (`edit-oo.log`). REA ev_ebd15c87… | Only smart quotes (text_rules.mjs:112, 218). "--", (c), (tm), 1/2, "teh", 1st and sentence capitalisation are all left unchanged (`b3-od.log`). | Both below Word (Word does every one of these); ONLYOFFICE ahead. | Add an autocorrect table: `--` → dash, (c) ©, (tm) ™, (r) ®, fractions, ordinals, common-typo list, sentence capitalisation. Make each step undoable as one action, with a settings panel. | webapp (+ settings) |
| TA-07 | P1 | Undo restores the selection | After bold on "quick" and moving to the end, Cmd+Z re-selects "quick ". After deleting a 2-word selection, Cmd+Z re-selects "quick brown " (`edit-oo.log`). | Undo restores the content but **the selection is gone**: an empty selection in both cases (`edit-od.log`). Path: `runEdit(() => doc.undo())` (main.js:1174, 11545). | ONLYOFFICE better; Word and Docs both restore it. | Record the selection before each transaction and restore it on undo and redo (selection-after state for redo). | wasm (history carries selection) + webapp |
| TA-08 | P1 | Drag and drop selected text (move) | Dragging a selected word moves it: "The brown fox jumps quick over…" (`edit-oo.log`, `oo-dnd2-mid.png`). | Not supported. Pressing on a selection starts a new character selection (`od-dnd-mid.png`). `object_text_move.mjs` handles only inline objects. | ONLYOFFICE better (Word and Docs both move text) | Extend the object-move gesture to text ranges: threshold, drop caret, one-transaction move, Option or Ctrl to copy, Esc to cancel. | webapp + wasm (move range op) |
| TA-09 | P1 | Double-click then drag extends by whole words | Selects "quick brown fox jumps " (`mouse2-oo.log`). | Drag after a double-click is character-wise, and **on release only the word under the pointer remains** ("jumps"); the anchor word is lost (`mouse2-od.log`, `od-dbldrag-mid.png`). dblclick → `touchSelection.selectWord` (main.js:6473, touch_selection.mjs:341). The drag runs through char-wise `updateDragSelection` (main.js:6177). Triple-click is handled only in `click` with detail 3 (main.js:6579), so a triple-click drag cannot extend by paragraphs either. | ONLYOFFICE better | Keep a selection granularity (char, word or paragraph) taken from the click count at pointerdown. Snap both ends to that unit while dragging. | webapp main.js |
| TA-10 | P1 | Caret kept visible horizontally when zoomed | At 170%, Cmd+Right to line end keeps the caret on screen (`oo-zoom-caret.png`). | At 310%, Cmd+Right to line end leaves the caret at x=2288 in a 1440-wide viewport, off screen (`b2-od.log`, `od-zoom-caret.png`). `scroll_into_view.mjs:27` `scrollTargetFor` computes `scrollTop` only. | opendoc below standard. ONLYOFFICE was at a lower zoom, so this is not a like-for-like; Word and Docs both follow the caret horizontally. | Add a horizontal pass to `scrollCaretIntoView` (main.js:8095), using the same scale-aware arithmetic. | webapp (scroll_into_view.mjs) |
| TA-11 | P1 | External HTML paste fidelity (H1, font family, list, table, link) | Keeps H1 as Heading 1, the Georgia face, link styling, the list and the table (`oo-paste-external.png`, combined view `combo-paste.png`). | Keeps bold, italic, colour, size, the list, the table and the link. **Silently drops** the H1 (it becomes body text) and the `font-family:Georgia` (`od-paste-external.png`). No compatibility finding is raised. | ONLYOFFICE better. No content is lost, but formatting is, and silently; the owner's P0-hunt category, ranked P1 here because the text survives. | Map `h1`–`h6` to Heading 1–6 and keep `font-family` when the face exists or can be provisioned. Report anything dropped through the existing paste-loss channel (`paste_loss.mjs`). | webapp (clipboard.mjs) + wasm |
| TA-12 | P1 | Copy out to other apps: HTML flavour | Not observable: ONLYOFFICE's copy needs a native clipboard event, which headless mode could not drive. | Copying a list, heading or table writes `<p>` per block to `text/html` (`<p>item one</p><p>item two</p><p>c1</p><p>c2</p>`, `b2-od.log`). A paste into Word or Docs loses the list, heading and table structure. Structure travels only in the internal marker (`selectionRichHtml`, main.js:6309-6324). | opendoc below standard (ONLYOFFICE unverified) | Render `ul/ol/li`, `h1-6` and `table` in `runsToHtml` from the structured payload that is already computed. | webapp (clipboard.mjs / main.js) |
| TA-13 | P2 | Option+Up/Down and Option+Shift+Up/Down (macOS paragraph movement) | Treated as plain Up/Down (moves one line); Option+Shift+Down selects one line (`selnav2-oo.log`). | Unbound: no-op. `keyboard.mjs:49,54` returns null when `altKey` is set. Cmd+Up/Down does move by paragraph (Word for Mac) (`selnav2-od.log`). | Both below the macOS and Docs-on-Mac convention; opendoc's Cmd binding is correct for Word. | Alias Option+Up/Down (and the Shift variants) to paragraphUp/paragraphDown on Apple platforms. | webapp (keyboard.mjs) |
| TA-14 | P2 | Cmd+0 (reset zoom) | Resets to 100% (DocumentHolder.js:425-429 `api.zoom(100)`). REA ev_ebd15c87… | No binding: zoom stays at 90% (`b2-od.log`). The keymap has only ⌘= and ⌘- (keymap.mjs:220-221). | ONLYOFFICE better | Add ⌘0 / Ctrl+0 → 100%. | webapp (keymap.mjs) |
| TA-15 | P2 | Paste-options affordance | A "(Ctrl)" button appears **at the end of the pasted range**. Options: Keep source formatting / Keep text only, plus nest-table, list-merge and others by context (DocumentHolderExt.js:1435-1459 `_arrSpecialPaste`, PasteOptions.js:343). REA ev_bf49b95c…, ev_1dd267af… | The chip offers "Merge formatting" and "Keep text only" (main.js:13971, 14010, 14058). It is drawn at the **start** of the paste and **covers the pasted text** (`od-paste-external.png`). | Mixed: opendoc's Merge formatting is Word's option and ONLYOFFICE has none; placement is worse. | Anchor the chip after the pasted range's end rect. Add an explicit "Keep source formatting" so the default can be chosen again after switching. | webapp |
| TA-16 | P2 | Find next and previous from the document (Cmd+G / Shift+Cmd+G) | No binding (counter unchanged, `b3-oo.log`). | No binding (`b3-od.log`). Enter and Shift+Enter in the box work (main.js:13342). | Both below the Mac and Docs standard | Bind ⌘G / ⇧⌘G (F3 / Shift+F3 off Apple) to next and previous match, without needing the panel focused. | webapp (keymap.mjs) |
| TA-17 | P2 | Tab typed in body text, on save | n/a (ONLYOFFICE cannot save in this harness) | A typed tab is saved as a literal U+0009 inside `<w:t>` ("fine.Tab⇥here", `ta-docs/out/rt-ta-blank.docx`), not as `<w:tab/>`. Text round-trips inside opendoc. | opendoc risk (confirm in Word) | Emit `<w:tab/>` for U+0009 in runs. | export |
| TA-18 | P2 | Save → reopen: paragraph style ids | n/a | Saving the `?fixture=rich` document rewrote every `w:styleId` and `w:pStyle` to numbers such as `18446744073709551619`; the source used `Heading1` and `TextBody` (`ta-docs/out/se-rich.docx`). Consistent within the file, but style identity is not round-tripped verbatim. Side finding outside this lane. | opendoc risk (round-trip fidelity) | Preserve the source `styleId` string and allocate synthetic ids only for new styles. | export |
| TA-19 | P3 | Cmd+B with a collapsed caret inside a word | Bolds the whole word ("**quicZk**", Word behaviour) (`oo-bold-collapsed-typed.png`). | Bolds only what is typed next ("qui**Z**ck", Docs behaviour) (`edit-od.log`). | Equivalent, by different conventions; Word is the stated bar. | Optional: apply to the word under the caret when it is strictly inside a word. | webapp/wasm |
| TA-20 | P3 | Click in the left margin selects the line | No (`selnav2-oo.log`) | No (`selnav2-od.log`) | Both below Word | Margin click → line, double-click → paragraph, triple-click → document. | webapp |
| TA-21 | P3 | Typing undo granularity | One step per word ("Alpha beta gamma delta" → "…gamma " → "…gamma" → "…beta "). | One step per typing burst, split at a ~1 s pause ("…delta" → "alpha beta gamma" → ""). | Equivalent; Docs is word-ish, Word batches. | none | — |
| TA-22 | P3 | Shift+click after a double-click | Extends by words ("quick brown fox "). | Extends by characters ("quick brown f"). | ONLYOFFICE closer to Word | Falls out of TA-09's granularity state. | webapp |

### Where opendoc is already better

- **Mac word movement (Option+Left/Right, Option+Shift+Right)** stops at word ends ("The|", "The quick"), as macOS and Word for Mac do. ONLYOFFICE uses the Windows convention on a Mac ("The |quick", "The quick ") (`selnav2-*.log`).
- **Cmd+Backspace** deletes to the start of the line, as on macOS. ONLYOFFICE deletes one character ("brownfox", `selnav2-oo.log`).
- **Cmd+Shift+Down** selects to the end of the paragraph (Word for Mac). ONLYOFFICE selects only to the end of the line.
- **Spell-check menu.** opendoc's suggestions are better (The / This / Thus for "Ths") and it offers Ignore once, Ignore all and **Add to dictionary**. ONLYOFFICE offers "Th, Th's, Ohs, More variants…, Ignore, Ignore all, Select language" (`b3-*.log`, `od-spell-ctx2.png`).
- **Context menu.** opendoc shows each entry's shortcut and includes "Paste without formatting ⇧⌘V". ONLYOFFICE's text menu has no plain-paste entry. Both keep the selection on right-click.
- **Floating formatting bubble.** A selection gets a B / I / U / S / colour / highlight / comment bubble (like Docs and Word's mini toolbar). ONLYOFFICE has none (`od-dbldrag-mid.png`).
- **Paste "Merge formatting".** opendoc offers it, as Word does; ONLYOFFICE does not.
- **Cmd+Shift+V** pastes as plain text. In ONLYOFFICE it was not observable headless.
- **Strikethrough shortcut ⌘⇧X** works, as in Docs (`edit-od.log`).
- **Ctrl+wheel zoom** is continuous (90% → 99%), like a pinch. ONLYOFFICE steps by 10%.

### Equivalent (no action)

- Triple-click selects the paragraph in both.
- Shift+click and plain drag select by character in both.
- Selecting and then typing replaces the selection; undo restores the text.
- Collapse on Left/Right goes to the ordered edge.
- Sticky goal column through a short line.
- Meta+Shift+End / Shift+PageDown selection.
- Smart quotes, plus undo of a smart quote. ONLYOFFICE's undo restores the straight quote; opendoc's single Cmd+Z removes the whole burst.
- Plain and multi-line paste, with the caret after the paste, in the middle or at the end of a paragraph; a multi-paragraph paste undoes as one step.
- Cut.
- Replace-all with a single-step undo; whole-word and match-case options.
- IME: compose and commit, compose over a selection, cancel, and fast Korean followed by Latin (`a日本b`, `仮名 world`, `abZ`, `하난abc` in both, `p0-*.log`).
- Full undo-all / redo-all round trip of typing, bold, rich paste and a paragraph join is byte-identical in both (33 and 38 steps).
- Typing past the bottom of the viewport keeps the caret visible in both.
- Toolbar state reflects bold, italic and underline for a selection, at a caret inside formatted text, and shows "off" for a mixed selection, in both.

### Not tested, and why

- **Live listener inspection of ONLYOFFICE input (keyboard, IME, clipboard)**, `inspect_web_event_listeners`: the editor runs in a same-site iframe, so it is not a separate target. With a cross-site host the iframe became its own target, but REA's call on that iframe target hung for 30 minutes and was aborted. `observe_web_execution` was not attempted after that. What was established without REA: input goes to the hidden `<textarea id="area_id">` inside the editor iframe.
- **REA tracing of `sdk-all.js`** (word-boundary, undo grouping and autocorrect internals): the file is 9 MB of minified code with only `asc_*` exports. After the hang above, analysis was limited to the readable web-apps sources (4 evidence ids above). Behaviour is evidenced by running it.
- **ONLYOFFICE through the real OS clipboard** (Cmd+C / Cmd+V / Cmd+Shift+V, cross-document copy and paste): its Mac copy and paste rely on native clipboard events, which headless Playwright does not produce. Paste was tested with a synthetic `ClipboardEvent` (it works), but ONLYOFFICE's copy-out HTML could not be captured.
- **ONLYOFFICE save → reopen**: the scratch host has no save callback, and the example host's editor page was broken (its `api.js` `src` was rendered as `http://localhost:8090/`). It was not repaired or restarted, per instructions.
- **PageUp/PageDown in a long document, spell-check language switching, and the format painter shortcut** were inconclusive. Format painter: neither ⌥⌘C/⌥⌘V applied anything.
- **Touch selection handles, and paragraph-wise triple-click-drag**: not exercised. The code indicates no paragraph-wise drag (main.js:6579).

---

## Review and collaboration: ONLYOFFICE 10.0.0 vs opendoc

Final, 2026-10-22. Investigation only; no repository was changed.

**Setup.** Screenshots are in `oo-ux/rc/` (`oo-*` = ONLYOFFICE, `od-*` = opendoc); scripts are in `rc-scripts/`.

- **opendoc build.** The `opendoc-wt-site` webapp at `adfd5e7b`, which contains main `e8133641` (#834).
- **opendoc host.** The repo's own `examples/share-host.mjs` on 127.0.0.1:8137. Its relay was built from main with `cargo build -p opendoc-relay --locked` into a scratchpad target directory.
- **ONLYOFFICE.** The `onlyoffice-ref` container, reached through a scratchpad proxy on 127.0.0.1:8139. The proxy only patches the example page: the real `api.js`, document and callback URLs the container can reach, and edit/comment/review rights. The example host could not reach its own public origin from inside the container.
- **Not running.** `localhost:8120` was down after the session restart. I did not restart it.

**Evidence ids.**

- `ev_10273932c080f09f3d592f192dea676a7d9386b7f1ad3fc9cda87be89f4cbea0` is the REA static inventory of the shipped `documenteditor/main` bundle; its parent is `ev_1bcfacdf…49e48c`. The bundle's `app.js` is only a loader. The logic is minified `sdk-all.js`, so I cite behaviour from shipped locale keys and runtime runs instead.
- REA `capture_browser_scenario` produced no evidence. The first run hung for 1,800 s; the second returned "Connection closed".
- Playwright `page.on("websocket")` also saw no ONLYOFFICE frames. ONLYOFFICE's wire protocol is therefore **not captured**.

### Ranked findings

| id | interaction | ONLYOFFICE behaviour + evidence | opendoc behaviour + evidence | verdict | recommended change | domain |
|---|---|---|---|---|---|---|
| **RC-01 (P0)** | Two people typing in the same paragraph at the same time | Fast mode: both inserts keep their order. A typed "Ay " at the start while B typed " Bee" at the end, and the result was `Ay Bravo … users. Bee` (`oo-co-fast-A-03-A-typed-same-para.png`). | **The later typist's text comes out reversed, identically on both clients.** A typed `abcdef` at the start of P2 while B typed `UVWXYZ` at the end. Both replicas show `abcdefBravo … users.ZYXWVU`. This reproduced at 60 ms and at 250 ms per key (`od-race.mjs`, `od-race-A-s1-*.png`). With roles swapped, A's `ghij` became `jihg` (`od-race2.mjs` T2). Different paragraphs are fine (T3). Cause: `paintArrival` in `main.js:8320-8321` says "The caret is NOT mapped (`107` P-4)". After each remote arrival the local caret sits *before* the character just typed. | opendoc worse. Silent text corruption, accepted into both replicas. | Map the local selection through every arrival's position map, including the rollback/replay path in `ClientSession::receive`. Return the mapped caret from the wasm `collab` outcome. Guard with a two-client e2e that interleaves keystrokes in one paragraph and asserts order. P-4 (`docs/107`:128) is the tracked prerequisite; Share should not be presented as ready while it is open. | wasm `collab.rs` + transaction; webapp `main.js` (`paintArrival`) |
| **RC-02 (P0)** | Typing after a remote insert earlier in the same paragraph | Not separately run. Fast-mode merges in RC-01 kept position. | B's caret sat after "Bravo" (offset 5). A inserted "pp" at offset 0, then B typed "ww". The result was `ppBrawwvo`: B's keystrokes landed 2 characters early (`od-race2.mjs` T4). Same root cause as RC-01. | opendoc worse | Same fix as RC-01. Assert the mapped caret offset in the same e2e. | wasm + webapp |
| **RC-03 (P0)** | Undo with two users | Fast mode: A's Ctrl+Z removed only A's own last character ("Ay " became "Ay"). B's text was untouched (`oo-co-fast-A-06-A-undo.png`). | **Undo removes the other user's text.** A typed `111`, then B typed `222` elsewhere. A pressed ⌘Z twice and got `…end.22`, then `…end.2`; A's own `111` stayed. The change reached B as well (`od-race.mjs` S2). | opendoc worse; it crosses users (owner's P0 list) | Selective per-participant undo: invert only this replica's own committed transactions, transformed over later remote ones (`docs/107` commit-log undo). If an inverse cannot be transformed, refuse with a reason rather than undoing someone else. | wasm/transaction + webapp undo wiring |
| **RC-04 (P0)** | Restoring a version while shared | Not testable here: saves to storage fail in this container (`docservice/out.log`: `ECONNREFUSED 127.0.0.1:8090`), so no versions exist. The shipped UI has `Common.Views.History.textRestore`, "Show detailed changes" and "Highlight deleted". Product design: history is server-side for every participant. | A restored an older version from the timeline while B was connected. A became `[Alpha…, Bravo…, Charlie…]` (original), but **B kept** `v2-edit` and `B-edit`. A's status dropped "Shared…", so A left the room without saying so (`od-versions.mjs`, `od-ver-04-A-restored.png`). Nothing is lost locally, because A's previous state is saved as a version, but the two people now edit different documents. | opendoc worse: silent divergence | Inside a room, either replay the restore as a transaction everyone receives, or refuse it with a reason ("Restoring replaces the document for everyone…" plus a confirm). Never silently drop out of the room. | webapp `version_history.mjs`/`version_panel.mjs` + collab shell |
| RC-05 (P1) | Who wrote this comment or suggestion (shared session) | Real user name and avatar on every comment and change (`oo-cm2-06-panel.png`; the popover reads "John Smith … Inserted: …", `oo-tc-04`) | B's comment and B's tracked insertion both show **"You"** on A's screen (`od-cocomment.mjs` log: `A cards: Y You … Comment from B`, `Added " suggested-by-B"`). The author comes from local settings, not from the session identity. | ONLYOFFICE better; review attribution is wrong | Stamp the comment and revision author from the host-asserted session identity. Render "You" only when it is this participant. | webapp + wasm (author on the transaction) |
| RC-06 (P1) | Remote caret identity | Strict mode labels a locked paragraph with the user's name, e.g. "Mark Pottato" (`oo-co-strict-A-03`). Fast mode shows a coloured caret with no flag in screenshots. | The label is "Participant {n}" (`collab_presence.mjs:55`; `od-co-A-*`); docs/169 §Presence says the same | ONLYOFFICE better | Carry a host-asserted display name and colour on Welcome/awareness | wasm roster + `collab_presence.mjs` + relay |
| RC-07 (P1) | Who is here (presence list) | A header users button with a count badge ("2") and a list (`oo-co-fast-A-03`, `#tlb-box-users`) | No roster UI. The only signal is the status sentence (`en_strings.mjs:1523`) | ONLYOFFICE better | An avatar stack beside Share that lists participants and jumps to each caret | webapp |
| RC-08 (P1) | Sync and save indicator during a network stall | The status bar shows "All changes saved", or "You have updates" in Strict (`oo-co-strict-A-05`), driven by acknowledgement | With the relay SIGSTOPped for 5 s, both clients kept saying "Shared — your changes are reaching everyone in this document" while edits were unacknowledged (`od-disconnect.mjs`). On SIGCONT the edits converged. On kill it switched correctly to "Connection lost — reconnecting…". | opendoc overstates (§13) | Tie the sentence to the pending-acknowledgement count: "Saving…" or "Not yet shared (n)", and "Shared" only at zero pending | webapp `collab_shell.mjs`/`collab_transport.mjs` |
| RC-09 (P1) | Version history in a shared document | Server-side history shared by everyone, per version and author, with "Show detailed changes" and "Highlight deleted" (locale keys `Common.Views.History.textShowAll` and `textHighlightDeleted`; panel header seen at runtime). Restore was not runnable (see RC-04). | The timeline is per browser. B's panel shows only "Opened", even though A saved a version (`od-ver-02-B-panel.png`). Rows carry no author. | ONLYOFFICE better | In a room, read and write versions through the host provider (`publishSnapshot`/a `versions()` hook), with author and revision. Keep local history for standalone use. | webapp + host contract |
| RC-10 (P1) | @mention in a comment | `@` opens a people picker supplied by the host (`oo-cm-04-mention-list.png`) | Not supported (`grep mention webapp/src`: no hits; `od-cm-04`) | ONLYOFFICE better | Host user directory, `@` picker, notify hook | webapp + host contract |
| RC-11 (P1) | Edit the root comment | Pencil on the root and on replies (`oo-cm-08`, `oo-cm2-06`) | Only replies have Edit (`main.js:2161`). The root has only Resolve and Delete (`main.js:1935`, `:1953`, `:1999`). | ONLYOFFICE better | Root Edit through `doc.updateComment` | webapp |
| RC-12 (P1) | Comment after its text is deleted | The panel quotes the anchored text ("brown fox", `oo-cm2-06`). The deleted case itself was not run. | The card survives with a zero-width anchor and **no quote** (`od-cm2-08`; `main.js:1992`). Undo restores it (`od-cm2-09`). | ONLYOFFICE better | Quote the anchor on cards, and show "Original text deleted" | webapp + wasm payload |
| RC-13 (P1, confirmed by the coordinator) | ODC-7006 message | n/a | "…unsent changes were lost" (`session_access.mjs:166`, `en_strings.mjs:1511`). That contradicts docs/169's preserving recovery. | False statement | Already assigned to a build lane | webapp, locales |
| RC-14 (P2) | Track-changes display modes | Four modes: "Markup and balloons", "Only markup", "Final (all accepted, preview)" and "Original (all rejected, preview)" (`oo-tc-05-display-menu.png`, `oo-tc-06-mode-*`) | A binary "Show changes (read-only)" (`main.js:11733`). Turned off, deletions vanish but insertions **stay underlined in colour** (`od-tc-04`), which is neither Final nor Original. The "(read-only)" label is wrong: typing still works. | ONLYOFFICE better | Four modes: All markup / Simple markup / No markup (final) / Original. Fix the label. | webapp + layout markup flag |
| RC-15 (P2) | Focus after posting a comment | The popover stays; a click resumes typing | `document.activeElement` is BODY after Enter, so keystrokes go nowhere (`od-comments2` log) | opendoc worse | Return focus to the editor (`focus_contract` class, SKILL §10) | webapp |
| RC-16 (P2) | Submit chord in the composer | Ctrl+Enter submits (`oo-cm2-02`) | Enter submits; Cmd/Ctrl+Enter is ignored (`review_chrome.mjs:72`, `od-cm-05`) | Partial | Also accept Mod+Enter | webapp |
| RC-17 (P2) | Comment with no selection | Context menu "Add comment", plus a document-level comment (`oo-cm2-07`) | Disabled, with the toast "Select text to comment on" (`od-cm-02`, `main.js:11412`) | ONLYOFFICE better (Word comments the word at the caret) | Comment the word at the caret | webapp |
| RC-18 (P2) | Sort and filter comments | Newest/Oldest, Author, Position, filter by group (`oo-cm2-07`) | Open/Resolved/All only (`main.js:1628`) | ONLYOFFICE better | Author and "mine" filters | webapp |
| RC-19 (P2) | Formatting-change wording | Popover "Formatted: …" (not captured verbatim) | "Changed formatting for "Second" / Bold: inherited → on" (`od-tc-02`). "inherited" is engine vocabulary. | Polish | "Formatted: Bold", as Docs does | webapp |
| RC-20 (P3) | Page jumps when the review column opens | n/a | Shifts about 180 px left (`od-cm-02` vs `od-cm-05`) | Polish | Reserve the gutter | webapp |
| RC-21 (P3) | Format bubble stays over the composer | n/a | `od-cm-05` | Polish | Hide it while the composer has focus | webapp |

### Where opendoc is already better (verified by running)

- **No silent loss under contention or disconnection.** ONLYOFFICE Strict mode silently swallows keystrokes:
  - A's "Ay " typed into a paragraph Mark had locked never appeared, and no message said why (`oo-co-strict-A-03`).
  - B's typing while offline in Strict mode was dropped (`oo-co-strict-B-07`).
  - The shipped string `DE.Controllers.Main.errorCoAuthoringDisconnect` reads "Server connection lost. The document cannot be edited right now."

  opendoc has no locks. When the relay was killed it kept editing locally, said "Connection lost — reconnecting. Changes you make now are not shared yet", and kept the text (`od-disconnect.mjs`, `od-dc-A-relay-killed.png`). The caveat is RC-01/02/03 once edits do merge.
- **Merge semantics outside RC-01.**
  - Same-offset races resolve deterministically and without interleaving: `Charlie111222` on both clients.
  - Typing inside a range another user deletes keeps the typed text: `XYZ one for…`.
  - A 5 s relay stall converged after resume.
- **Remote selections are painted** (`.remote-selection`; `od-co-A-04-B-selection.png`). In my ONLYOFFICE Fast run, A saw only B's caret, not B's selection (`oo-co-fast-A-04`).
- **Comments and suggestions sit beside the text.** opendoc anchors cards in the right margin, one per change, and describes each change ("Deleted "lazy "", "Added "…"", "Changed formatting…"). It also offers the margin "+" and the selection-bubble entry points (`od-tc-02`, `od-cm2-02`). ONLYOFFICE shows a change or comment only in a click popover, or in a separate left panel (`oo-tc-04`, `oo-cm-08`, `oo-cm2-06`).
- **Keyboard review.** opendoc has ⌘⇧E to cycle modes (`keymap.mjs:247`; verified, it switches to Suggesting) and ⌘⌥⏎ / ⌘⌥⌫ to accept or reject and advance. In ONLYOFFICE, Ctrl+Shift+E and ⌘⇧E did not change the Track Changes button state in my run.
- **Resolved threads and deletes.** opendoc offers a Resolved filter with Reopen and confirms "Delete this thread?". ONLYOFFICE keeps resolved threads highlighted inline.
- **Accepting is shared.** A's Accept all on B's suggestion removed the card on B too (`od-cocomment.mjs`).

### Not tested, and why

- **ONLYOFFICE co-editing wire protocol** (how changes, locks and cursors travel; Strict vs Fast on the wire). REA capture hung for 1,800 s and then returned "Connection closed". Playwright saw no ONLYOFFICE WebSocket frames.
- **ONLYOFFICE version history, compare and restore.** Storage saves fail inside this container: docservice fetches its output from `localhost:8090`, which does not exist inside the container. As a result, no version exists. Fixing it means changing the shared container, which the coordinator ruled out.
- **ONLYOFFICE offline in Fast mode.** Playwright's `setOffline` did not cut the existing socket, so typing propagated as if online; the result is inconclusive.
- **ONLYOFFICE comment on a deleted range, and per-group review/comment permissions** (Mark/Hamish groups). There was no time; the users' permission descriptions are in the example page config.
- **opendoc in-editor permission roles** (the `Full access` badge and grants) were not exercised beyond the default.
- **Track-changes context menu.** Right-click Accept/Reject was inconclusive in both editors, because the click position may have missed the change. opendoc's code supports it (`main.js:6715`).

---

## ONLYOFFICE Docs 10.0.0 vs opendoc: objects and page layout UX

Scope: tables, images, shapes and text boxes, headers and footers, hyperlinks, page setup, breaks,
rulers, and the contextual settings surface. Investigation only: no repository was changed.

### Method

- **Same input in both editors.** The input was `ol-data/blank.docx`, a generated six-paragraph US Letter document with 1-inch margins, plus a 400x300 4:3 PNG (`ol-data/pic.png`).
  - ONLYOFFICE ran from private copies (`ol-*.docx`) in the example storage, with autosave off, so every run started clean.
  - opendoc loaded the same file through `#file`.
- **Playwright at 1440x900.** Scripts are in `(local) ol-scripts/`. Screenshots are in `oo-ux/ol-shots/`: `oo-*` is ONLYOFFICE, `od-*` is opendoc, and `m-*` are side-by-side montages.
- **REA evidence from the live ONLYOFFICE editor** (`inspect_web_page`, with the DOM and accessibility tree):
  - `ev_ecc91a29847cd45d08534962edeeda0928039cb6d129429a3b4e0c50fbf57a17`: image selected, Image settings panel open.
  - `ev_6eaf7f851597cbdfeefdd045c271bce25f690297fb15c09b727f4a9dddf24ba6`: inside the header (Header & Footer tab).
  - `ev_1a081216423d8361466fd4cc74e4a186c09d12552c11b8964beddd6935f6f12d`: cursor in a table, Table settings panel open.
- **REA limits.** `analyze_web_bundle` on the live editor sent no response for 30 minutes and was aborted; `sdk-all.js` is several MB. `analyze_javascript_application` on `web-apps/.../main/app` closed the connection.
  - So there are no `trace_javascript_semantics` traces. ONLYOFFICE code is cited by identifier, file and line in the shipped files, which is grep evidence.
- **Licence.** ONLYOFFICE behaviour is described, never copied.
- **Round trips.** opendoc saved with Cmd+S (download). The file was reopened in opendoc and also opened in ONLYOFFICE (`od-rt*`, `oo-open-od-roundtrip*`).
- **Undo integrity.** After a 7-step object edit sequence, undoing everything restored the original text exactly and removed the image (`od_rt2.mjs`).

Verdict key:
- **OO+**: ONLYOFFICE is better.
- **OD+**: opendoc is better.
- **EQ**: equivalent.
- **BOTH-**: both are below Word / Google Docs.

### Ranked findings

| id | P | interaction | ONLYOFFICE behaviour + evidence | opendoc behaviour + evidence | verdict | recommended change | domain |
|---|---|---|---|---|---|---|---|
| OL-01 | **P0** | Click or drag in the empty part of a page below the last paragraph, then type | Caret lands on the last line at the clicked x, and typing goes in (`m-below.png`; typed `ZQBELOW` appeared in paragraph 6). This is the Word behaviour. | **Typed text and shortcuts are silently dropped.** Focus moves to `<body>`, the text never reaches the document, nothing is reported, and Cmd+Z does nothing either (an undo loop of 30 presses made no change until focus was restored). This happens in the empty part of a page below the last paragraph. A click in the left margin, or a drag that starts in the text, is fine. The engine `hitTest` returns nothing below the last line, and the caller returns `null` without placing a caret: `webapp/src/main.js:3821-3822`. Repro script `ol-scripts/od_lost2.mjs`: click-below-text and drag-below-text lost the typing; drag-within-text, drag-text-to-below and click-left-margin kept it. | OO+ | A miss below the content should snap to the nearest line (the last line of the page's last block) at the clicked x, and the editable proxy should keep focus. Add a guard: click below the content, type, and assert the text landed. Apply the same rule to every page, including below a table at the end of a page. | wasm (hitTest) + webapp |
| OL-02 | **P0-candidate (needs Word)** | Save after making an inline picture float (Square / Tight / Through) | Not applicable. ONLYOFFICE opened opendoc's file without complaint (`oo-open-od-roundtrip2.png`). | The exported `wp:anchor` has **no `relativeHeight`**, and `wp:wrapSquare` has **no `wrapText`**. Both attributes are `use="required"` in ECMA-376 (CT_Anchor and CT_WrapSquare). The XML is in `ol-data/od-roundtrip2.docx`. Code: `crates/casual-doc-export/src/semantic.rs:7716-7718` (relativeHeight written only when the model holds one) and `semantic.rs:8702-8722` (wrapText written only when the source authored it). A newly created anchor has neither. The same omission applies to the picture writer at `semantic.rs:7620-7624` and the group/shape writer at `9021`. A new `wrapTight`/`wrapThrough` without a polygon is written as an empty element, but the schema requires `wp:wrapPolygon`. | — | Keep the verbatim-retention rule for opened files. For anchors the user **creates**, write schema-required attributes: `relativeHeight` as the next z-order value, `wrapText="bothSides"`, and a rectangle polygon for tight/through. Validate the export against the WordprocessingML XSD in CI. **Check in desktop Word whether it reports "unreadable content".** | export/io |
| OL-03 | P1 | Tab / Shift+Tab between table cells | Moves to the cell and **selects its contents**, so typing replaces them, as in Word. Shift+Tab then typing Y replaced "I". `oo-tbl-10-shifttab-selects.png`, `m-tbl-a.png` | Puts the caret at the **start** of the target cell. Shift+Tab then Q turned "h" into "Qh" (a11y mirror `g Qh i`). `main.js:14382-14408`: `moveTableCell`, then `navToPosition(target,false)`. | OO+ | After a cell move, select the target cell's whole content; Word, Docs and ONLYOFFICE all do. | webapp (`main.js`) |
| OL-04 | P1 | Switch an inline picture to Square wrap | Text reflows around the picture at once, in place (`m-img-f.png`, top). | The picture stays laid out as inline, with an empty band to its right and text below, **until the picture is dragged**. After the drag it wraps correctly. `od-img-22-square.png` against `od-img-24-moved.png`. Path: `main.js:5179-5192` (`setObjectWrap` / `setObjectAnchorKind`). | OO+ | Converting inline to floating should anchor the picture at its current page position with an effective wrap, so text flows immediately. Add a layout assertion that text occupies the gap to the right. | wasm + layout |
| OL-05 | P1 | Text wrap around a rotated floating picture | Not measured. ONLYOFFICE placed the same file's picture below the text, so there was no overlap to judge. | Square wrap uses the **unrotated** box, so text in paragraphs 4 and 5 runs underneath the rotated corners. This is visible live and after reopening (`m-rt2.png`). Word wraps around the rotated bounding box. | BOTH- (opendoc verified wrong) | Use the rotated extent (the axis-aligned box of the rotated rectangle) as the Square/Top-and-bottom exclusion. | layout |
| OL-06 | P1 | Built-in table styles and style options | The right panel has a template gallery with rendered previews; Header, Total and Banded rows; First, Last and Banded columns (tblLook); border buttons per edge; background colour; distribute; "Repeat as header row"; Add formula; Convert table to text. REA `ev_1a081216…` and `oo-tbl-17-rightpanel-table.png`. | The style chooser lists **only the styles already defined in the document**, plus "No table style". On a new document that is the only entry; e2e `table-editing-ux.spec.mjs` asserts a count of 1. It uses a generic three-bar swatch, has no previews, and has **no tblLook toggles**. `main.js:9097-9113`. | OO+ | Ship Word's built-in Grid Table and List Table set with rendered previews and the six tblLook toggles, applied through the existing `applyTableStyle`. | wasm + webapp |
| OL-07 | P1 | Header/footer "Link to previous" (multi-section) | A checkbox on the Header & Footer tab, enabled in sections after the first (`ev_6eaf7f85…` lists the checkbox "Link to previous"). | The checkbox is **read-only by design**: "Unlinking has to copy the inherited header into this section, which the engine cannot do yet" (`webapp/editor.html:1539`). A multi-section document therefore cannot get a different header per section through the UI. | OO+ | Implement unlink (copy the inherited header and footer parts into the section) and relink. | wasm + webapp |
| OL-08 | P2 | Header/footer editing context | Double-clicking the top margin opens a contextual **Header & Footer** tab with Header from top, Footer from bottom, Different first page, Different odd and even pages, Link to previous, Page Number, Date & Time, Field, Image and Close. The body is dimmed while you edit the header, and the header is greyed while you edit the body (`m-hf-a.png`, `m-hf-c.png`, `ev_6eaf7f85…`). | Enters header mode with a "Header" tag, a dashed boundary and the status line "Editing the header — press Esc". The ribbon stays on Home with **no contextual tab**, and the options sit in a modal dialog (`od-hf-03-settings.png`). Neither the body nor the header is dimmed (`m-hf-a.png`, `m-hf-c.png`). Esc and a double-click in the body both exit. | OO+ | Add a contextual Header & Footer ribbon tab while in running content, and dim the inactive layer as Word does. | webapp |
| OL-09 | P2 | Insert page number | A gallery of six positions (top or bottom; left, centre or right), plus "To current position", "Number of pages" and a Page Numbering dialog. `oo-hf-02-pagenum-menu.png`. | One click inserts a PAGE field **at the caret**, with no choice of position, so the user must align it by hand (`od-hf-02-pagenum-menu.png`; `main.js:7734`, `insertFieldAtCaret("page")`). The format and restart options exist in the header/footer dialog. | OO+ | Add a position gallery that creates the header or footer when it is missing, inserts the field aligned left, centre or right, and offers "Page X of Y". | webapp + wasm |
| OL-10 | P2 | Margins / Orientation / Size / Columns | Each button opens a one-click preset gallery: Normal, Narrow, Moderate and Wide margins plus Custom; a Portrait/Landscape toggle; a size list with dimensions; One/Two/Three/Left/Right columns plus Custom. `m-page-a.png`, `oo-page-size.png`. | All four buttons open the **same modal Page setup dialog** (`aria-haspopup="dialog"`, `editor.html:811-814`). Switching to landscape takes open, click and close. The dialog itself is complete: apply-to-section, mirror margins, gutter, vertical alignment, page colour and column spacing (`od-page-margins.png`). | OO+ for speed, OD+ for completeness | Put a preset dropdown on each button, with the dialog as its "Custom…" row. | webapp |
| OL-11 | P2 | Ruler: drag margins, vertical ruler | Horizontal and vertical rulers. Dragging the vertical ruler's top-margin boundary moved the margin with a dashed guide (`m-ruler.png`). The engine entry points `SetDocumentMargin_Hor` and `SetDocumentMargin_Ver` are at `sdkjs/word/sdk-all-min.js:1032` (byte offsets 557325 and 557436). Indent drag also shows a full-height guide (`m-page-b.png`). | Horizontal ruler only. It has indent markers and tab stops (click to add), and the margin zones are shaded but **not draggable**. There is **no vertical ruler** (`webapp/src/ruler.mjs:1-18`; grep finds no vertical ruler). | OO+ | Make the margin boundaries draggable, with a live guide and a size readout, and add a vertical ruler for top and bottom margins. | webapp |
| OL-12 | P2 | Follow a hyperlink | Hover shows the URL and "Press ⌘ and click link"; Cmd+click opened `https://example.com/a` in a new tab (`m-link-a.png`, log in `link.mjs`). | A plain click shows a link chip with Edit, Remove and Open, which is good (`m-link-b.png`). **Cmd+click does not follow** the link (no new page in the same test), and hover shows **no tooltip**. The click handler only follows internal links directly (`main.js:6280-6295`). | OO+ on follow, OD+ on the chip | Follow on Ctrl/Cmd+click through `followExternalTarget` (`link_targets.mjs:55-64`), and add a hover tooltip with the URL and the modifier hint. | webapp |
| OL-13 | P2 | State after Insert > Picture | The picture is selected: 8 handles, the rotate handle, and the Image panel icon enabled (`oo-img-01-inserted.png`). | The picture is inserted but **not selected**; the caret goes after it (`main.js:12349-12376` ends with `focusEditorSurface()`; `od-img-01-inserted.png`). | OO+ (Word selects too) | After `insertImage`, select the new object as `insertTextBoxObject` already does (`main.js:12414-12419`). | webapp |
| OL-14 | P2 | Hit tolerance of a side resize handle | A drag that started 4px off the E handle's centre resized the picture (`oo-img-05-side-drag.png`). | A drag that started 5px off the E handle's centre, along the edge, **moved** the picture (undo label "Object move"). The hit area grows outward only; along the edge it stays 9px (`webapp/src/style.css:8060-8125`, whose comment names the distance-arbitration fix). | OO+ | Arbitrate handles by distance in `pointerdown`, or grow side handles by ±8px along the edge. | webapp |
| OL-15 | P2 | Shape catalogue | About 170 shapes in Recently used, Basic, Figured arrows, Math, Charts (flowchart), Stars & ribbons, Callouts and Lines (connectors, curves, freeform) (`oo-shape-01-gallery.png`). | About 25 shapes: one line, 2 rectangles, 11 basic shapes, 7 block arrows and 2 stars. No callouts, flowchart, connectors, curve or freeform (`od-shape-01-gallery.png`; `webapp/src/shape_catalogue.mjs`). | OO+ | Grow the catalogue by DrawingML preset family (the renderer already draws `prstGeom`), starting with callouts, flowchart and line arrows. | webapp + layout |
| OL-16 | P3 | Undo labels for object edits | Not applicable (ONLYOFFICE undo is unlabelled). | Rotation, a wrap change and a move are all labelled "Undo Object move". Inserting a text box is labelled "Undo Insert image". There is no rotate, wrap or text-box edit kind (`crates/casual-doc-wasm/src/lib.rs:860-885`; logs in `od_img_undo.mjs` and `od_rt2.mjs`). | — | Add the ObjectRotate, ObjectWrap and InsertTextBox kinds. | wasm |
| OL-17 | P3 | Picture: Actual size / Fit to margin / Opacity | All three are present in the Image panel (`ev_ecc91a29…`: "Actual Size", "Fit to margin", the "Opacity" spinbutton). | Not found (`object_bar.mjs`, `object_inspector.mjs`). Flip and rotate ±90 exist (`en_strings.mjs:1248-1253`). | OO+ | Add "Reset size" and "Fit to margins". Opacity is a lower priority. | webapp + wasm |
| OL-18 | P3 | Cell text direction | A "Text direction" submenu in the table context menu (`m-tbl-c.png`). | Not offered. | OO+ | Add `tcPr/textDirection`. | wasm + webapp |
| OL-19 | P3 | Whole-table move and resize handles | A move handle top-left and a resize square bottom-right (`m-tbl-a.png`). | Neither handle was seen (`m-tbl-b.png`). | OO+ | Add Word's table move handle (select and drag) and corner resize. | webapp |
| OL-20 | P3 | Alt text | The advanced dialog has an "Alternative text" tab with Title and Description (`oo-img-33-advanced.png`). | The Alt text dialog has Description only (`od-img-34-alttext.png`); there is no "Mark as decorative" (a Word 365 option). | EQ | Add "Mark as decorative". | webapp |

### Equivalent (verified by running both)

- **Table insert and navigation.** Both have a grid picker with hover preview and size label. In both, Tab in the last cell appends a row; opendoc also says "Row added at the end of the table" (`od-tbl-04-tabnav.png`).
- **Column and row border drags.** Both resize and compensate the neighbouring column.
  - ONLYOFFICE draws a full-page dashed guide with a ruler marker; opendoc draws an in-table line.
  - A resize round-trips: `tblGrid` 1695/4545/3120 was saved, reopened identically, and rendered the same in ONLYOFFICE (`m-rt3.png`).
- **Cells and pictures.** Both support drag-selecting cells and merge or split from the context menu, and corner resize keeps the 4:3 aspect in both.
- **Breaks menu.** Both offer page, column, and section breaks (next page, continuous, even page, odd page) (`od-page-breaks.png`; ONLYOFFICE `web-apps/.../app/view/Toolbar.js:2369-2384`).
- **Hyperlink dialog.** Both have Web address / Place in document, Text to display and ScreenTip (`m-link-a.png`).
- **Header/footer options.** Both offer different first page, different odd and even pages, distance from top and bottom, and Esc to exit.
- **Round trips.**
  - A floating text box with text, a table, a header with a PAGE field and a hyperlink all reopen identically in opendoc and render correctly in ONLYOFFICE (`m-rt.png`, `m-rt3.png`).
  - A rotated, resized, floating picture also reopens identically, apart from OL-02's schema question and OL-05's wrap.

### Where opendoc is already better

- **Live feedback.** A size readout appears during resize ("13.76 × 10.32 cm") and during crop ("8.20 × 7.94 cm"), and an angle badge during rotate ("51°"). ONLYOFFICE shows only a ghost image (`m-img-b.png`, `m-crop.png`, `od-img-08-rotate-drag.png`).
- **Crop.** The cropped area is dimmed, there is an explicit Apply, and handles show the result. ONLYOFFICE's crop drag moved the picture inside the frame instead, which is easy to do by accident (`m-crop.png`). ONLYOFFICE does add Fill and Fit crop modes.
- **Hyperlink chip.** One click gives Edit / Remove / Open, as in Docs, and blocked schemes are refused with a message (`link_targets.mjs:49-64`).
- **Table context menu.** It adds Move row up/down, Autofit & sort, Cell formatting, Border line style, and Table properties with caption and description for accessibility (`m-tbl-c.png`, `m-tbl-d.png`).
- **Wrap modes** are one click away on the object bar. ONLYOFFICE needs right-click, then Wrapping style, then a submenu, or the side panel.
- **Page setup dialog** is more complete in one place: apply to section, mirror margins, gutter, vertical alignment and page colour.
- **Accessibility.** opendoc keeps a document mirror (`#a11yDocument`). ONLYOFFICE's accessibility tree has **no document text** (REA `ev_ecc91a29…`: 0 nodes match "Paragraph 1 of body"). Only its 65 toolbar and panel buttons are exposed.
- **Insert text box** lands the caret inside the box, ready to type (`main.js:12414-12419`). ONLYOFFICE needs a draw gesture first.

### For the hover/merged-cell lanes (brief, observational)

- **Object bar placement.** The floating object bar sits above the object and covers the paragraph lines just above it (`od-img-22-square.png`, `od-img-24-moved.png`, `m-rt2.png`). During crop it changes to an Image / Alt text / Properties / Border / Apply / Delete set.
- **Column-border handle needs a hover pause.** It is created on hover, so pressing the mouse immediately after moving onto the border, with no pause, starts a text gesture instead of a resize. With a 400ms hover it worked (`od_rt3.mjs`: "Undo Typing" without the pause, "Undo Table resize" with it).
- **Mini format toolbar position.** It appears over the selection and overlapped the link chip and the table context menu in screenshots (`m-tbl-c.png`, `m-link-b.png`).
- **Merged cells** were not exercised beyond "merge two cells in a row, then split" from the context menu, which worked in both.

### Could not test, and why

- **Desktop Word.** Not available, so OL-02 (required attributes) is unconfirmed against Word. ONLYOFFICE tolerates the file.
- **REA bundle tracing** of `sdk-all.js` timed out after 30 minutes or dropped the connection. ONLYOFFICE function behaviour is evidenced by running it, plus shipped-file identifiers.
- **ONLYOFFICE horizontal-ruler margin drag.** My press landed on the indent marker; only the vertical-ruler margin drag was verified live.
- **Multi-page and multi-section behaviour** was not exercised. The blank document has one section and one page. This covers different first page and odd/even rendering across pages, a per-section orientation change, column flow after Columns = 2, and "Link to previous" when enabled in ONLYOFFICE.
- **Not tested:** touch and mobile interaction, dragging objects across pages, and the shape draw-by-drag mode in opendoc (`shape_draw_mode.mjs`).
