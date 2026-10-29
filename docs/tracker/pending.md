# Pending work

Generated from the single open queue in [docs/109](../109-BACKLOG.md).
Edit the source row there, then run `node webapp/tools/build-tracker-views.mjs`.
A partial status records delivered work; it does not claim that a person is
currently assigned. Rows stay in their queue order.

| # | Id | Priority | Work | Status |
| ---: | --- | --- | --- | --- |
| 7 | HF-165 | P2 | Arrow keys skip a whole table whose cells lie outside the current column | Open |
| 8 | HF-065 | P2 | Re-opening the same file does nothing, and file read errors are completely silent | Open |
| 10 | HF-036 | P2 | Printing a mixed-orientation document silently clips the landscape pages | Open |
| 11 | HF-039 | P2 | Find highlights only the current match, so "7 of 23" cannot be answered by looking at the page | Open |
| 12 | HF-064 | P2 | No accessibility checker — the editor is accessible but never audits the document being written | Open |
| 13 | HF-070 | P2 | Ribbon popovers and Settings never take focus, and closing them loses the user's place | Open |
| 14 | HF-073 | P2 | No recent documents — the only way back into yesterday's file is the OS file picker | Open |
| 15 | HF-078 | P2 | Images are re-decoded from source bytes on every page repaint | Open |
| 16 | HF-090 | P2 | Three of four fuzz targets are built but never run, and no browser test opens a hostile document | Open |
| 17 | HF-068 | P2 | Re-verify the original durable version-history finding and close or narrow this row | Open — re-verification needed |
| 18 | HF-071 | P2 | The accessibility mirror is rebuilt wholesale on every edit, resetting the screen reader to the top | Open |
| 19 | HF-077 | P2 | Opening a heavy document freezes the tab with no budget, no progress and no cancel | Open |
| 21 | HF-121 | P2 | An empty centred or right-aligned paragraph parks the caret at the left margin while the text lands elsewhere | Open |
| 22 | HF-123 | P2 | Clicking past the last word of a soft-wrapped line, or Home/End there, teleports the caret to another visual line | Open |
| 23 | HF-132 | P2 | The emoji picker offers 355 glyphs against ~1,900 in Word/Docs/Slack, and its search is near-useless | Open (owner decision) |
| 24 | HF-190 | P2 | Accept/reject the change at the cursor reach only the command palette | Open |
| 25 | HF-185 | P2 | The galley cache holds two copies of the body paint tier | Open |
| 26 | HF-186 | P2 | `command-activation-contract`'s inert-control assertion cannot fail | Open |
| 27 | HF-189 | P2 | The site pages have no contrast sweep, and the shared docs shell fails AA | Open |
| 29 | HF-197 | P2 | Find cannot match across a non-breaking or soft hyphen, and an equation reaches a screen reader as silence | Open |
| 30 | HF-198 | P2 | The site has no localisation seam at all, so its English cannot be routed and its string debt cannot fall | Open |
| 31 | HF-204 | P2 | A page break inside a table cell is refused, where Word splits the row across the page boundary | Open |
| 34 | HF-215 | P2 | A host that composes away the ribbon and menu strands five granted capabilities with no affordance and no warning | Open |
| 35 | HF-218 | P2 | Tab in a table's last cell does nothing and says nothing — the only table gesture that refuses in silence | Open |
| 36 | HF-219 | P2 | Shading or aligning a selected row formats exactly one cell, silently | Open |
| 37 | HF-221 | P2 | No rectangular cell selection exists, and six other table gaps wait behind it | Open |
| 38 | HF-222 | P2 | Table chrome costs 80 whole-page-tree scans on a 20x5 table, inside a function with 64 call sites | Open |
| 39 | HF-227 | P2 | The default nested list scheme is 1. / 1.1. / 1.1.1. where Word and Docs give 1. / a. / i. | Open |
| 40 | HF-230 | P2 | Left and Right arrow keys are not mirrored for RTL anywhere in the chrome, and Arabic is a shipped locale | Open |
| 41 | HF-231 | P2 | `setReviewMode` always ends by focusing the editor, and two callers now undo that afterwards | Open |
| 42 | HF-255 | P2 | Suggesting mode refuses an underline style or colour edit, and nothing is known about whether it says so | Open |
| 44 | HF-260 | P2 | A chart cannot be floating, and the hit-test wildcard means one would be silently unselectable if it were | Open |
| 45 | FONT-01 | P2 | Arabic and Indic draw real glyphs but their SHAPING is unverified — no shaped-output guard exists for either family | Open |
| 46 | FONT-03 | P2 | The deployed demo still fetches CJK and colour emoji from a third-party CDN | Open (owner decision) |
| 47 | HF-271 | P2 | The owning-container lookup is still O(document) per structural interaction, so Enter and Backspace still walk the whole block list | Open |
| 48 | HF-272 | P2 | `casual-doc-wasm` still declares the inline container set itself, with a third spelling beside its own copy | Open |
| 49 | HF-279 | P2 | In the compact chrome on a laptop, Insert table… from the Insert MENU opens the size grid at the window's top-left, across the header | Open |
| 50 | HF-314 | P2 | A version restore in a shared room cannot travel as one transaction, so it is refused rather than applied for everyone | Open |
| 51 | HF-300 | P2 | The rest of the HF-296/HF-297 family: Math, NoteReference, a labelled EmbeddedObject and the soft/non-breaking hyphens are still charged differently by the edit/layout position space and by `inline_anchor_len` (the review, comment, hyperlink and clipboard space), so a suggestion, comment or link range beside one can land a position off | Open |
| 53 | HF-348 | P2 | Below an ordinary paragraph the table's column strip is under 24px, and under 16px it carries no `+` discs | Open |
| 54 | HF-167 | P3 | `Line::range` for a drawing-only paragraph is `[u32::MAX, 0]`, and an inline picture does not grow its line | Open |
| 55 | HF-091 | P3 | Clipboard failure messages are styled as ordinary status text | Open |
| 56 | HF-099 | P3 | Document Properties never shows the file's byte size | Open |
| 57 | HF-103 | P3 | macOS paragraph navigation: Option+Arrow is dead and Cmd+Arrow moves by paragraph | Open |
| 58 | HF-113 | P3 | Every pointermove re-queries and materializes all page wrappers | Open |
| 59 | HF-100 | P3 | Tab stops can only be created, moved or deleted with a mouse | Open |
| 60 | HF-101 | P3 | Undo parks the caret at the start of the paragraph | Open |
| 61 | HF-102 | P3 | Remove Link leaves the text blue and underlined | Open |
| 62 | HF-104 | P3 | The Help menu has one item, and there is no keyboard-shortcuts reference | Open |
| 63 | HF-105 | P3 | Print freezes the tab with no progress, cancel, or page-range control | Open |
| 64 | HF-107 | P3 | Changing a list marker writes numbering definitions outside the undo system | Open |
| 65 | HF-111 | P3 | Each suggested keystroke re-validates the entire document | Open |
| 66 | HF-187 | P3 | The palette and the ribbon leave focus in different places | Open |
| 67 | HF-188 | P3 | `one-axis-navigation`'s `VALUE_FAMILIES` is a second copy of `data-command-family` | Open |
| 68 | HF-200 | P3 | The unrouted-string scanner counts file paths, flags and shell commands as translatable prose | Open |
| 69 | HF-201 | P3 | `docs/40` cannot be published honestly because it has no opening prose to summarise | Open |
| 70 | HF-202 | P3 | `docs/83`'s heading makes an 85-character page title, so the SDK architecture cannot be published on the site | Open |
| 71 | HF-205 | P3 | A break inside a body-level content control is refused, where Word allows it | Open |
| 72 | HF-206 | P3 | A document with no `w:sectPr` supports neither Page setup nor a section break | Open |
| 73 | HF-207 | P3 | Which page the caret paints on after a break is unsettled, for line breaks as much as page breaks | Open |
| 74 | HF-216 | P3 | The ribbon slack figure that was wrong by 5x still survives in six comments, including one in the shipped embed page | Open |
| 75 | HF-223 | P3 | `docs/70` is the table interaction document a lane reads first, and seven of its eight deferrals have shipped | Open |
| 76 | HF-224 | P3 | The remaining thirty-one Google-Docs table interaction gaps, ranked in docs/141 | Open |
| 77 | HF-228 | P3 | The remaining thirty-four list and numbering gaps, ranked in docs/142 | Open |
| 78 | HF-229 | P3 | `docs/76` is the only list design document, is seven lines long, and its one behavioural claim is false | Open |
| 79 | HF-232 | P3 | Left-rail captions clip below about 420px, and the version entry makes it one worse | Open |
| 80 | HF-233 | P3 | On a phone the status toast covers the version panel's footer action | Open |
| 81 | HF-261 | P3 | An unclean shutdown can leave a room's journal unopenable: the discarded partial frame stays on disk and the next chunk is appended after it | Open |
| 82 | FONT-02 | P3 | About 30 script families still render as notdef boxes — no fallback bucket exists for them | Open |
| 83 | FONT-04 | P3 | A font-fetch refusal names an internal bucket key ("the sc font") rather than the face | Open |
| 84 | RFL-05 | P3 | In reflow, a selection dragged past the edge of a wide table's scroller does not scroll the table (`docs/151` §6.3c A-2) | Open |
| 85 | RFL-07 | P3 | Under DevTools device emulation (Chrome's device toolbar, Playwright's `deviceScaleFactor`) a reflow tile cut can show a faint light row | Open |
| 86 | HF-280 | P3 | At the phone rung with a MOUSE, a menu opened from the menus sheet appears under the resting pointer and hover-opens whichever submenu landed under it | Open |
| 87 | UX-OB-01 | P3 | Picture effects — shadow, glow, soft edges, reflection — are not modelled: import reports a non-empty `a:effectLst` as a loss and nothing can add one | Open |
| 88 | UX-OB-04 | P3 | A Ctrl-drag COPY of a chart, diagram, OLE object or inline group, or of anything whose look comes from the theme, is refused: only pictures and text boxes copy | Open |
| 89 | UX-OB-07 | P3 | There is no "Lock aspect ratio" control: a lock can come only from the file or from Insert ▸ Picture, and the properties panel's width and height do not follow it | Open |
| 90 | UX-OB-08 | P3 | A picture in a header, footer or footnote, or in a text box's own text, has no object box: it cannot be selected, so resize, crop, alt text and the drag never reach it | Open |
| 91 | HF-347 | P3 | An RTL (`w:bidiVisual`) table keeps its row strip and `+` discs on the left, its trailing side | Open |
| 92 | HF-349 | P3 | Row-border resize zones on short rows stay under 24px, and the touch boundary pills are 24px rather than 44 | Open |
| 95 | UX-004 | P1 | The two tests named for command-surface parity do not enforce it | Open |
| 96 | UX-022 | P1 | Engine boot and boot failure have no state design | Open |
| 98 | CQ-006 | P1 | Test surface has structural blind spots — one browser, no axe, 1 of 4 fuzz targets, no layout benchmark | Open |
| 99 | CQ-011 | P1 | `docs/14`'s per-slice statuses are stale and nothing re-derives them, so 115 rows claim a state no one has checked | Open |
| 100 | FID-P-02 | P1 | There is no Microsoft-Word-produced fixture anywhere in the repository | Open |
| 101 | FID-L-02 | P1 | No hyphenation at all | Open |
| 104 | UX-018 | P1 | Zero touch code — no `touchstart`/`pointerType`/`maxTouchPoints` anywhere | Open |
| 105 | UX-019 | P1 | No breakpoint below 620px; at 390px the editor is one dropdown | Open |
| 106 | UX-020 | P1 | The accessibility mirror is read-only and unanchored to the caret | Open |
| 107 | CQ-001 | P1 | Two god-files, one per side of the boundary | Open |
| 108 | CQ-002 | P1 | The live editing path bypasses the transaction engine, so ADR-005 is not honoured in practice | Open → `107` §2.1 |
| 109 | FID-L-06 | P1 | The shaper's paragraph base level cannot be forced | Open |
| 110 | FID-L-08 | P1 | Vertical and rotated text is entirely absent | Open |
| 111 | OO-001 | P1 | No table of contents, and no table of figures | Open |
| 112 | OO-005 | P1 | No captions and no cross-references | Open |
| 113 | UX-050 | P1 | Behaviour gaps against ONLYOFFICE Docs 10.0.0 found by running both editors (`docs/174`) that no lane is building yet: TA-02 select-all cannot clear a document that starts with a table; TA-03 incremental find lands on the last match so Replace hits the wrong one; TA-04 no all-match highlight; TA-05 no list auto-format while typing; TA-06 autocorrect is smart quotes only; TA-07 undo does not restore the selection; TA-08 selected text cannot be dragged; TA-09 double-click-drag does not extend by words; TA-10 caret leaves the viewport horizontally when zoomed; TA-11 HTML paste drops headings and font family silently; TA-12 copied HTML is flat; OL-05 wrap ignores rotation; OL-06 no built-in table styles or style options; OL-07 header/footer "Link to previous" cannot be changed; RC-09 version history is per browser | Open |
| 116 | UX-027 | P2 | Nothing decides whether a capability gets a dialog, a panel, a popover or a menu row | Open — audit done (#743), fixes to follow |
| 117 | UX-008 | P2 | `insert.table` is two different products behind one command id | Open |
| 118 | UX-013 | P2 | Print has no visible chrome | Open |
| 119 | UX-049 | P2 | Version history still has eight UX gaps a normal user meets, found by the 2026-10-09 audit and not fixed with UX-048 | Open |
| 120 | UX-023 | P2 | Invalid and mismatched ARIA on popup triggers | Open |
| 121 | CQ-009 | P2 | Design documents no longer describe the implementation | Re-opened (#542) |
| 123 | FID-L-15 | P2 | No OpenType feature control, and the `w:kern` threshold is unapplied | Open |
| 126 | UX-012 | P2 | No Table menu on the menu bar, and the palette hides table commands on complex tables | Open |
| 128 | UX-024 | P2 | Focus and target-size gaps | Open |
| 129 | UX-029 | P2 | Symbol, emoji, shape and text-box insertion is a modal panel that hides the document it is inserting into | Open |
| 130 | FID-L-10 | P2 | Watermarks do not appear | Open |
| 131 | FID-L-11 | P2 | `nextColumn` is treated as `continuous` | Open |
| 133 | FID-L-17 | P2 | `w:kinsoku` is cascaded and never consumed | Open |
| 134 | FID-L-21 | P2 | `rich` and `table-merges` bottom edge diverges ~240 twips from LibreOffice | Open |
| 135 | FID-L-23 | P2 | The line breaker splits a line that fits its measure exactly, so a cell can wrap one line short of its own intrinsic width | Open |
| 136 | FID-L-26 | P2 | The line breaker keeps a word that ends past the right margin, so a paragraph comes out a line short of LibreOffice's | Open |
| 137 | FID-L-27 | P2 | Font-parity residue against LibreOffice on the Calibri/Cambria substitutes: a different Carlito build, short Caladea lines, and an unexplained wrap in the font-families fixture | Open |
| 138 | FID-L-28 | P2 | A document that declares no font at all is shaped with Roboto here and Carlito by LibreOffice, so it is not oracle-comparable | Open |
| 140 | OO-008 | P2 | Table formulas are 4 functions over 2 directions; ONLYOFFICE has 18 over 4 | Open |
| 141 | OO-010 | P2 | No print dialog — no range, duplex, colour/mono, margins or preview | Open |
| 142 | OO-011 | P2 | No document protection, password, or digital signature | Open |
| 143 | OO-012 | P2 | No content-control authoring | Open |
| 144 | OO-024 | P2 | Note options are not authorable: no footnote/endnote conversion, no number format, restart or placement control | Open |
| 145 | OO-025 | P2 | A named style cannot be created or updated — the gallery applies styles the document already has | Open |
| 146 | OO-020 | P2 | Ribbon and UI breadth rows worth copying cheaply | Open |
| 147 | OO-021 | P2 | Specific Home and Insert controls absent here | Open |
| 148 | FID-L-07b | P2 | The rest of floating tables: nested/running-content positioning, and page splitting | Open |
| 149 | CQ-008 | P2 | A dependency port is blocked at scale — 910 measured quick-xml 0.42 compile errors | Open (`M-009`) |
| 150 | FID-R-08 | P2 | Charts, SmartArt and OLE are preserved but never drawn | Open |
| 151 | OO-007 | P2 | No document comparison or combine | Open |
| 152 | OO-009 | P2 | No equation editor | Open |
| 154 | FID-SH-01 | P2 | Text in a non-rectangular shape is laid out in the whole box, not the geometry's text rectangle (`a:rect`) | Open |
| 155 | FID-SH-04 | P2 | `insertShape` offers only the 22 typed presets, though every one of the 187 now draws | Open |
| 156 | FID-R-07 | P3 | Sub-part "retention" is not byte-exact | Open |
| 157 | UX-028 | P3 | Nothing asserts a `class="ms"` icon ligature exists in the self-hosted subset | Open |
| 158 | UX-039 | P3 | Word's built-in style names are shown in English in every locale | Open |
| 159 | UX-046 | P3 | The Keyboard shortcuts card is at capacity on a 1280x720 laptop — the next chord pushes it below the fold | Open |
| 160 | FID-L-19 | P3 | Character-grid snapping is not applied | Open |
| 161 | OO-016 | P3 | AutoCorrect has no math codes (`\alpha` → α) and no lettered or outline list triggers; list auto-format (HF-321) and dashes, symbols, typos and sentence capitalisation (HF-322) are built | Open |
| 162 | OO-019 | P3 | No freehand drawing (Draw tab) | Open |
| 163 | UX-045 | P3 | Ctrl+Alt+N refuses on a document that does not define Heading N; Word instantiates the built-in | Open |
| 164 | FID-L-20 | P3 | EMF/WMF metafiles and browser-build SVG paint a placeholder | Open |
| 165 | OO-013 | P3 | No mail merge | Open |
| 166 | OO-017 | P3 | No plugin or macro surface | Open |
| 167 | FID-L-24 | P3 | The `docs/44-COVERAGE-GAP-AUDIT.md` Tier-4 long tail is preserved opaque and never semantically modelled | Open |
| 168 | FID-SH-02 | P3 | No adjust-handle drag and no Edit Points for a shape | Open |
| 169 | FID-SH-03 | P3 | ODF export writes a preset or freeform shape as its bounding `draw:rect` | Open |
| 170 | FID-AT-12 | P3 | An edited save leaves out an `app.xml` statistic the editor refreshed when its current value happens to equal the source's, and names it stale | Open |
| 171 | FID-AT-13 | P3 | Under forms protection, a section whose `w:formProt` is false admits typing and paragraph edits but refuses a table, a block paste or an object outside a paragraph | Open |
| 172 | Q2 | — | Decide how to acquire a rights-cleared **Word-produced** corpus | Open |
| 173 | Q6 | — | Decide the mobile commitment — fund it, or downgrade the support matrix | Open |
| 174 | Q1 | — | Reconcile the D-6 embed-contract decision with the shipped host contract and the remaining in-process mount | Open — decision recorded in `docs/125` §8; queue closure pending |
| 175 | Q3 | — | Decide charts and SmartArt scope — render, or preserve-and-disclose | Open |
| 176 | Q7 | — | Decide the `.docm` policy — currently rejected at open, undecided | Open |
| 177 | Q8 | — | Decide the six architecture questions `99` §5 still lists as pending, which no ADR has answered | Open (owner decision) |
| 178 | RM-01 | — | Field evaluation engine — host-provided evaluation context, recalculation, Update field, Toggle field codes | Open |
| 179 | RM-02 | — | Consolidate the editor-proven commands and errors into a versioned public SDK boundary | Open |
| 180 | RM-03 | — | Host storage contract, in the opencalc shape | Open |
| 181 | RM-05 | — | Complete ODT beyond the current bounded subset | Open |
| 182 | RM-06 | — | HTML, Markdown, EPUB and FB2 import/export; DOTX/OTT templates | Open |
| 183 | RM-07 | — | Tagged PDF and PDF/A | Open |
| 185 | FOLD-003 | P2 | A windowed body cannot be folded at all, so folding is withheld on exactly the documents an outline matters most for | Open |
| 186 | FOLD-005 | P2 | RTF import drops every heading's identity, so the outline panel, folding, the TOC field and Go-to-heading are all empty on an imported `.rtf` | Open |
| 188 | RM-19 | P2 | Typing aids and special characters: autoCorrect engine, replace table and options dialog; AutoFormat as you type; Spaces, hyphens and shortcut characters; Symbols by code and math shorthand | Open |
| 193 | RM-24 | P2 | Footnotes and endnotes: note settings, and layout honouring format, restart and position; Convert footnotes to endnotes; Go to next or previous note; Separator customisation | Open |
| 196 | RM-27 | P2 | Tables: look and tools: table style options and built-in gallery; Table fill, width and margins; Borders: presets and the dialog with a preview; Eraser and draw table; Formula picker and recalculation | Open |
| 200 | RM-31 | P2 | Review: display modes: simple markup, original, final; Decide changes in a selection; resolve all comments; Suggesting table and list structure; table revisions listed and decided; Comments over several paragraphs; highlight drawn by the engine | Open |
| 201 | RM-32 | P2 | Protection: protect the document from the editor; Set a password (needs the ADR-052 decision); Editable ranges (w:permStart/w:permEnd): model, shade, edit; A tracked-changes restriction forces Suggesting | Open |
| 202 | RM-33 | P2 | Fields: field catalogue and date formats; Update all fields; Operate dropdown form fields; Cached field results wrap like text; Insert text from a file | Open |
| 203 | RM-34 | P2 | Content controls and forms: insert and configure a content control; Operate dropdown, combo and date controls; Control chrome and placeholder text; Protect a form | Open |
| 204 | RM-35 | P2 | Charts: chart type gallery and change type; Edit the data; Chart elements; Remaining chart families and 3-D | Open |
| 205 | RM-36 | P2 | Hyphenation: pattern hyphenator honouring zone, limit and caps; Hyphenation control; Optional hyphens shown and honoured | Open |
| 207 | RM-38 | P2 | Rendering fidelity: line numbers in table cells; Forced paragraph base level and embedding depth; SmartArt drawn from the cached drawing Word stores; VML positioning and custom paths; math typesetting; SVG text in the browser, EMF/WMF; shape effects and text rectangles (with the drawing lane) | Open |
| 208 | RM-39 | P2 | Editing complex objects: equation editing; Insert SmartArt; edit its text; WordArt and the picture watermark; Edit a VML shape by converting it; Embed fonts on save; drop-cap options; links on pictures | Open |
| 212 | RM-11 | P3 | No container image, compose file or deployment guide | Open |
| 213 | RM-12 | P3 | No desktop shell: Tauri and OS host fonts are deliberately after the browser-first work, and still unstarted | Open |
| 214 | RM-13 | P3 | No worker threading: layout and import run on the main thread, which is what makes every O(document) row a frozen tab | Open |
| 216 | FOLD-002 | P3 | A positioned table inside a collapsed heading's range is still placed, so a fold leaves one object behind | Open |
| 217 | FOLD-006 | P3 | The in-body fold chevron can only stay live by walking the document, so it paints while the outline panel is open and not before | Open |
| 219 | RM-40 | P3 | Tables: position and layout: table position dialog and wrapping; Cell noWrap, fitText, hideMark and style-provided row properties in layout; Positioned tables in headers, nested, and split across pages; Art and compound borders painted exactly | Open |
| 220 | RM-41 | P3 | Compare and versions: compare writes a merged document of tracked changes; Show what changed between two versions; Combine documents | Open |
| 221 | RM-42 | P3 | Proofing and language: language of a selection and of the document; Personal dictionary management; Word count dialog with a selection count; Grammar breadth | Open |
| 222 | RM-43 | P3 | View and navigation: ruler, status bar and formatting marks toggles; Regular-expression search; Several pages side by side; dark page; Ruler in the chosen unit; Rebind keyboard shortcuts | Open |
| 223 | RM-44 | P3 | Print and export: print preview and options; print the selection; RTF writer; PDF/A; EPUB | Open |
