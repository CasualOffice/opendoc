# 104 — Hotfix Tracker

**Status:** Living record. **Opened:** 2026-09-02. **Owner:** unassigned.

**Scope:** every confirmed UX, UI, and correctness defect currently known in this
repository, ranked in fix-first order. This is the *defect* queue. It does not
replace `14-EXECUTION-TRACKER.md` (per-slice execution state), `99-REMAINING-WORK-AUDIT.md`
(unfinished capability), or the fidelity audits (`46`/`55`/`60`) — it is the list of
things that are wrong in code that already shipped.

## How this list was produced

Two parallel multi-agent audits, each finder followed by an adversarial verifier that
re-read the cited code and was instructed to refute the claim by default when uncertain.
Findings that failed verification were discarded and are not listed here.

| Audit | Lenses | Raised | Confirmed | Refuted | Rows after merge |
| --- | --- | --- | --- | --- | --- |
| Internal defect audit | editor UX · accessibility/keyboard · CSS/visual · webapp JS · WASM bindings · Rust core · import/export · layout/render · security + command parity | 125 | 107 | 18 | 89 |
| Sibling gap audit | design system · opencalc UX parity · ProseMirror-editor UX parity · platform/architecture · written-spec compliance | 60 | 48 | 12 | 27 |

The sibling gap audit compares this repository against two references, on the owner's
instruction that both are ahead of opendoc on UX/UI:

- **opencalc** — `../sheets` (remote `CasualOffice/opencalc`). Same architecture, started
  later, materially further ahead; its webapp is ~20 focused modules against this repo's
  single 14.9k-line `webapp/src/main.js`.
- **docs (ProseMirror)** — `CasualOffice/docs`, the predecessor this runtime succeeds.
  Its content UX is still better than opendoc's today. It also carries
  `CROSS_EDITOR_CONSISTENCY.md`, a spec written *for this editor*.

Both are reference-only. Nothing in them was modified.

## Priority definitions

| Priority | Meaning |
| --- | --- |
| P0 | Data loss, corruption, unrecoverable state, hang, or a security hole. Fix before other work. |
| P1 | User-visibly wrong behaviour on a common path, an accessibility blocker, or an interaction clearly below the Word/Docs bar. |
| P2 | Real defect on a less common path, or a polish/breadth gap the siblings already close. |
| P3 | Minor. Worth fixing, not worth reordering for. |

**Effort:** S = under an hour · M = up to a day · L = more than a day, or needs a design decision first.
**Status vocabulary:** Open · In progress · In review · Fixed · Won't fix (with reason).
A `Fixed` row names the PR that closed it, so the claim can be checked rather than trusted.

## Summary

| Priority | Count |
| --- | --- |
| P0 | 7 |
| P1 | 33 |
| P2 | 52 |
| P3 | 22 |
| **Total** | **114** |

### Progress

Closed so far, all with a regression test proven to fail against the reintroduced bug:

- **#495** — HF-004, HF-007, and the unload half of HF-002.
- **#497** — HF-001, HF-003, HF-006, HF-010, HF-044, plus a colour-glyph placement defect
  found while investigating an emoji report and not previously in this list: bitmap emoji
  were painted at a fraction of their offset, landing in the page corner.
- **In review** — HF-005, HF-008, HF-020 (engine); HF-021, HF-033, HF-043, HF-062, HF-063,
  HF-067, HF-086, HF-089, HF-092, HF-093 and the discard-confirm half of HF-002 (webapp).

Every P0 is now closed or in review.

## Queue

### P0 — 7 items

| ID | Title | Area | Effort | Source | Status |
| --- | --- | --- | --- | --- | --- |
| HF-001 | Saving fails outright when one image is used twice (header logo + body) | import-export | S | Internal audit | Fixed (#497) |
| HF-002 | Nothing guards unsaved work: no beforeunload prompt, and Open silently replaces a dirty document | data-safety | S | Sibling gap (opencalc + docs) | Fixed (#495 unload guard; discard confirm in review) |
| HF-003 | Backspace at a paragraph start leaves the document invalid, disabling footnotes, bookmarks, comments and suggestions | rust-core | S | Internal audit | Fixed (#497) |
| HF-004 | A failed Open destroys the document already on screen and leaves the editor pointed at freed wasm memory | data-safety | S | Sibling gap (opencalc) | Fixed (#495) |
| HF-005 | Typing in Suggesting mode is invisible — the page repaints from a frozen markup layout | wasm | M | Internal audit | In review |
| HF-006 | Pagination never terminates on a table whose repeated header fills the page | layout | M | Internal audit | Fixed (#497) |
| HF-007 | Four copy-pasted edit-apply paths have diverged — table and list edits never mark the document dirty | code-structure | S | Sibling gap (opencalc) | Fixed (#495) |

### P1 — 33 items

| ID | Title | Area | Effort | Source | Status |
| --- | --- | --- | --- | --- | --- |
| HF-008 | Control characters in text are written raw into document.xml, producing a file nothing can reopen | import-export | M | Internal audit | In review |
| HF-009 | Images in headers, footers, footnotes and comments lose their relationship on save | import-export | M | Internal audit | Open |
| HF-010 | Duplicate relationship Id in document.xml.rels makes the saved package invalid | import-export | S | Internal audit | Fixed (#497) |
| HF-011 | No autosave, draft, or crash recovery — a tab crash or OS kill is unrecoverable | data-safety | L | Sibling gap (opencalc + docs) | Open |
| HF-012 | Numbering, note, comment and bookmark ids are exported as 20-digit numbers Word cannot accept | import-export | M | Internal audit | Open |
| HF-013 | Rich paste in Suggesting mode scrambles or drops text containing any non-ASCII character | webapp-js | S | Internal audit | Open |
| HF-014 | Rendering hangs forever on dashed/dot-dash underline with a font reporting zero underline thickness | render | S | Internal audit | Open |
| HF-051 | No Word Count dialog and no selection-scoped counts — the code flags this hole itself | word-count | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-015 | A six-byte non-ASCII color string panics the WASM module and poisons the session | wasm | S | Internal audit | Open |
| HF-016 | There is no "New blank document" — the only way to get a document is to open someone else's file | file | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-017 | Table column count is unbounded on one side and collapses to 1 twip on the other | layout | M | Internal audit | Open |
| HF-018 | Blocking site data leaves the editor completely dead (blank, no handlers) | webapp-js | S | Internal audit | Open |
| HF-019 | Right-click "Open link" bypasses the URL scheme allowlist the click path enforces | security | S | Internal audit | Open |
| HF-060 | No coarse-pointer sizing and 13px inputs — every menu row is mouse-sized and iOS Safari zooms on every field focus | a11y | M | Sibling gap (opencalc + docs) | Open |
| HF-020 | Opening a tracked-changes document can throw mid-render and leave the page list blank | wasm | S | Internal audit | In review |
| HF-021 | Track-changes UI hardcodes light-mode Google hexes — suggestions and the mode switch are unreadable in dark mode | design-system | M | Sibling gap (opencalc + docs) | Open |
| HF-022 | With changes shown, clicking places the caret in the wrong place and selection highlights miss the text | wasm | L | Internal audit | Open |
| HF-023 | Every table command is enabled but always fails for tables in headers, footers, notes and text boxes | parity | M | Internal audit | Open |
| HF-024 | Down arrow in a table jumps sideways to the next cell instead of the row below | editor-ux | M | Internal audit | Open |
| HF-025 | Every shortcut label is a hardcoded ⌘ glyph — Windows and Linux users are shown keys their keyboard does not have | i18n | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-026 | Everything pasted from Google Docs arrives bold | import-export | S | Internal audit | Open |
| HF-027 | Accept All / Reject All silently leaves tracked changes in headers, footers, notes and text boxes | wasm | M | Internal audit | Open |
| HF-028 | Alt text never reaches the accessibility tree, and figure paragraphs and table headers vanish | accessibility | M | Internal audit | Open |
| HF-029 | The status line is the only error channel and is not a live region — every failure is silent to screen readers | accessibility | S | Internal audit | Open |
| HF-030 | Print and "Save as PDF" emit a 150-DPI raster — no selectable, searchable or accessible text, and a long document exhausts the tab | print | L | Sibling gap (docs (ProseMirror)) | Open |
| HF-081 | No localization seam — every string is an English literal inside a 14.9k-line file | i18n | L | Sibling gap (opencalc + docs) | Open |
| HF-031 | Command palette announces nothing while arrowing through results | accessibility | S | Internal audit | Open |
| HF-083 | Pages are squashed horizontally in any window narrower than the sheet | layout | M | Internal audit | Open |
| HF-032 | Insert-table grid picker is pointer-only and exposes 80 unnamed buttons | accessibility | S | Internal audit | Open |
| HF-033 | Dark theme fails contrast on focused menu rows, review chips and error text | css | M | Internal audit | Open |
| HF-088 | The comments column has no breakpoint below 860px and swallows the page | responsive | M | Internal audit | Open |
| HF-034 | The header "Open" button cannot be focused or activated by keyboard | accessibility | S | Internal audit | Open |
| HF-035 | No spelling or grammar checking anywhere — less feedback than a plain `<textarea>` | spellcheck | L | Sibling gap (docs (ProseMirror)) | Open |

### P2 — 52 items

| ID | Title | Area | Effort | Source | Status |
| --- | --- | --- | --- | --- | --- |
| HF-036 | Printing a mixed-orientation document silently clips the landscape pages | import-export | M | Internal audit | Open |
| HF-037 | ODT to DOCX conversion writes every image as a zero-byte file | import-export | M | Internal audit | Open |
| HF-038 | Copying or format-painting dark-highlighted text silently repaints it yellow | wasm | S | Internal audit | Open |
| HF-039 | Find highlights only the current match, so "7 of 23" cannot be answered by looking at the page | find-replace | M | Sibling gap (opencalc) | Open |
| HF-040 | Pasting a table from the web silently drops the sentences around it | import-export | S | Internal audit | Open |
| HF-041 | Multi-valued custom document properties collapse to their last value and change type | import-export | M | Internal audit | Open |
| HF-042 | Charts and ink are dropped even when the file carries a fallback the model supports | import-export | L | Internal audit | Open |
| HF-043 | Nine hand-rolled dialogs behave differently — Split cell ignores Escape, backdrop clicks and Enter, and leaks focus behind its own modal | dialogs | M | Sibling gap (opencalc) | Open |
| HF-044 | An unreadable image is exported as a zero-byte part with no loss reported | import-export | S | Internal audit | Fixed (#497) |
| HF-045 | A failed edit or undo leaves the document half-changed and can lose the undo step | rust-core | M | Internal audit | Open |
| HF-046 | The paste-options chip undoes an unrelated edit if you press Cmd+Z first | clipboard | S | Internal audit | Open |
| HF-047 | Import/export data loss is reported as a bare number — the report naming what was lost is parsed and discarded | error-handling | M | Sibling gap (opencalc) | Open |
| HF-048 | Changing underline style or double-strike leaves the page showing the old decoration | layout | S | Internal audit | Open |
| HF-049 | A comment anchored in a header or footnote can never be deleted | wasm | M | Internal audit | Open |
| HF-050 | A footnote inserted outside the body can never be undone or removed | rust-core | M | Internal audit | Open |
| HF-052 | Bookmarks in headers, footers and notes cannot be created or deleted, and report "invalid name" | rust-core | M | Internal audit | Open |
| HF-097 | Tools and Help scroll out of the menu bar behind a hidden scrollbar | responsive | M | Internal audit | Open |
| HF-053 | Toolbar formatting state is wrong for a caret in a header, footer or note — so Bold toggles the wrong way | rust-core | S | Internal audit | Open |
| HF-054 | Pasting from Word or a web page inserts a blank paragraph before the content | import-export | S | Internal audit | Open |
| HF-055 | Smart quotes insert the wrong glyph after any non-ASCII character | webapp-js | S | Internal audit | Open |
| HF-056 | Images cannot be rotated or flipped — a sideways phone photo has to be fixed outside the editor | images | L | Sibling gap (docs (ProseMirror)) | Open |
| HF-057 | Object properties panel shows stale geometry and Apply reverts a drag-resize | editor-ux | M | Internal audit | Open |
| HF-058 | The object action bar stays frozen on screen while the object scrolls away | layout | S | Internal audit | Open |
| HF-059 | Cmd+V never pastes an image, and says nothing | clipboard | S | Internal audit | Open |
| HF-061 | A zero-height table row paints its text across the rest of the page | render | S | Internal audit | Open |
| HF-062 | Split cell dialog cannot be dismissed by keyboard, and closing it kills typing | editor-ux | S | Internal audit | Open |
| HF-063 | Cmd+F inside a modal steals focus out of the dialog and opens Find behind it | editor-ux | S | Internal audit | Open |
| HF-064 | No accessibility checker — opendoc makes the editor accessible but never audits the document being written | a11y | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-065 | Re-opening the same file does nothing, and file read errors are completely silent | editor-ux | S | Internal audit | Open |
| HF-066 | Pasted hyperlinks are stored with no scheme filter and re-exported | security | S | Internal audit | Open |
| HF-067 | The menu bar has no visible focus indicator — keyboard navigation is blind | a11y | S | Internal audit | Open |
| HF-068 | No version history — the document has no past that survives a reload | versions | L | Sibling gap (opencalc + docs) | Open |
| HF-069 | Toolbar and menu commands fire on mouse-down, so a mis-press cannot be aborted | editor-ux | M | Internal audit | Open |
| HF-070 | Ribbon popovers and Settings never take focus, and closing them loses the user's place | editor-ux | M | Internal audit | Open |
| HF-071 | The accessibility mirror is rebuilt wholesale on every edit, resetting the screen reader to the top | accessibility | L | Internal audit | Open |
| HF-072 | The floating selection toolbar never shows Bold/Italic/Underline state, so clicking B un-bolds | editor-ux | S | Internal audit | Open |
| HF-073 | No recent documents — the only way back into yesterday's file is the OS file picker | file | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-074 | No skip link: reaching the document means tabbing past ~150 chrome controls | accessibility | S | Internal audit | Open |
| HF-075 | Colour pickers for accent and table borders have no accessible name | a11y | S | Internal audit | Open |
| HF-076 | Right-click menu is missing Paste-without-formatting and Select all; checklist missing from menus | parity | M | Internal audit | Open |
| HF-077 | Opening a heavy document freezes the tab with no budget, no progress and no cancel | architecture | L | Sibling gap (opencalc) | Open |
| HF-078 | Images are re-decoded from source bytes on every page repaint | perf | M | Internal audit | Open |
| HF-079 | The incremental galley cache is inert for every imported document | perf | L | Internal audit | Open |
| HF-080 | Every pointer move and caret query flattens the whole document's lines | perf | M | Internal audit | Open |
| HF-082 | The galley cache never evicts entries for deleted paragraphs | perf | S | Internal audit | Open |
| HF-084 | Object properties panel covers the ribbon, including the overflow "..." button | layout | S | Internal audit | Open |
| HF-085 | main.js is 93% of the webapp with zero exports, which is why the apply paths diverged and why the embed surface is blocked | code-structure | L | Sibling gap (opencalc) | Open |
| HF-086 | Undefined --bg-1 makes the header/footer band label unreadable and the "Add header" chip transparent | css | S | Internal audit | Open |
| HF-087 | Marketing site navigation disappears on phones with no replacement | responsive | M | Internal audit | Open |
| HF-089 | Review popover and inline accept/reject card paint above modal dialogs | css | S | Internal audit | Open |
| HF-090 | Three of four fuzz targets are built but never run, and no browser test opens a hostile document | testing | M | Sibling gap (opencalc) | Open |
| HF-114 | No collaboration, presence, sharing or roles — and no server for a second person to connect to | collab | L | Sibling gap (opencalc + docs) | Open |

### P3 — 22 items

| ID | Title | Area | Effort | Source | Status |
| --- | --- | --- | --- | --- | --- |
| HF-091 | Clipboard failure messages are styled as ordinary status text | css | S | Internal audit | Open |
| HF-092 | Validation error text is unreadable in two of the six theme/OS combinations | css | S | Internal audit | Open |
| HF-093 | Keyboard-shortcut hints and empty-state prose sit at ~3.3:1 in both themes | accessibility | M | Internal audit | Open |
| HF-094 | Compact-chrome toggle is reachable only from the ribbon chevron — not in View, not in the palette | chrome | S | Sibling gap (docs (ProseMirror)) | Open |
| HF-095 | Outline panel's active-row colour is defeated for Heading 3 and deeper | css | S | Internal audit | Open |
| HF-096 | Left rail buttons have a no-op hover state | editor-ux | S | Internal audit | Open |
| HF-098 | Image resize grips are 9px with no expanded hit area | editor-ux | S | Internal audit | Open |
| HF-099 | Document Properties never shows the file's byte size | dialogs | S | Sibling gap (docs (ProseMirror)) | Open |
| HF-100 | Tab stops can only be created, moved or deleted with a mouse | accessibility | M | Internal audit | Open |
| HF-101 | Undo parks the caret at the start of the paragraph | editor-ux | M | Internal audit | Open |
| HF-102 | Remove Link leaves the text blue and underlined | parity | M | Internal audit | Open |
| HF-103 | macOS paragraph navigation: Option+Arrow is dead and Cmd+Arrow moves by paragraph | parity | S | Internal audit | Open |
| HF-104 | The Help menu has one item, and there is no keyboard-shortcuts reference | onboarding | M | Sibling gap (docs (ProseMirror)) | Open |
| HF-105 | Print freezes the tab with no progress, cancel, or page-range control | perf | M | Internal audit | Open |
| HF-106 | In crop mode arrow keys move the picture and a cancelled drag leaves crop stuck | editor-ux | M | Internal audit | Open |
| HF-107 | Changing a list marker writes numbering definitions outside the undo system | wasm | M | Internal audit | Open |
| HF-108 | setObjectExtent and insertImage accept NaN and collapse the object to 1 EMU | wasm | S | Internal audit | Open |
| HF-109 | Nothing is embeddable: no host-capability modes, no custom element, no package | embedding | L | Sibling gap (opencalc + docs) | Open |
| HF-110 | Every keystroke walks and materializes the whole document's text twice | wasm | M | Internal audit | Open |
| HF-111 | Each suggested keystroke re-validates the entire document | perf | M | Internal audit | Open |
| HF-112 | flow_blocks recomputes the running galley height for every paragraph | perf | S | Internal audit | Open |
| HF-113 | Every pointermove re-queries and materializes all page wrappers | perf | S | Internal audit | Open |

## Cross-cutting themes

Each theme below is one root cause behind several rows. Fixing the theme is cheaper and
safer than fixing its rows one at a time, and closes the class rather than the instance.

**T-01 · Body-only mutation behind all-surface authoring**

Read paths, UI gating and forward operations resolve paragraphs across every surface (headers, footers, footnotes, endnotes, text boxes, cells) while the corresponding mutations call doc.body_mut(). The result is the same shape every time: a control that looks enabled and then fails, silently no-ops, or reports a misleading error. One systemic fix — route all remaining ops through blocks_owning_mut / on_owning_surface_mut / surface_block_lists, teach the *_mut walks to descend into paragraph inlines, and add a single operation x surface matrix test — closes all of them and stops the next one landing.

Rows: HF-023, HF-027, HF-049, HF-050, HF-052, HF-053

**T-02 · Two layouts, one renderer: markup vs editing layout is never reconciled**

active_layout() decides what is painted, but page_size, hit-testing, caret and selection geometry, dirtyPages and page_count are each wired to a different layout, and markup_layout is never invalidated on edit. That single inconsistency produces invisible typing, a blank page list on open, and a misplaced caret. Making one layout the authority (rebuilt in finish_edit, read by every page-indexed and geometry API) fixes three separate user-facing failures at once.

Rows: HF-005, HF-020, HF-022

**T-03 · No single choke point for untrusted URLs**

Link targets arrive from imported documents, pasted HTML and the link dialog, and are followed from at least two surfaces — but the scheme allowlist exists in exactly one of them (activateLink). One shared resolveExternalTarget(url) helper used by every follow path, plus the same predicate at the two ingestion points, closes both the parity break and the storage of hostile targets in exported files.

Rows: HF-019, HF-066

**T-04 · Missing modal/popover primitive: focus, dismissal and stacking**

Dialogs, popovers and panels each hand-roll their focus behaviour, so the ones that were written later lack Escape, backdrop dismissal, focus-in, focus restore, and coverage in syncModalLock — and the z-index ladder lets review chrome paint above modals while Cmd+F walks focus out of one. A single dialog/popover primitive (open: focus in; Escape/backdrop: close and restore; Tab: trap; global shortcuts: no-op while open; one z-index ladder with modals on top) closes the whole cluster.

Rows: HF-062, HF-063, HF-067, HF-070, HF-089

**T-05 · Silent loss with no disposition recorded**

Several paths drop or downgrade content and report nothing — unreadable media becomes a zero-byte part, ODT images vanish on conversion, vector custom properties collapse to a scalar, an mc:Fallback the model supports is discarded, and loose text beside a pasted table disappears. The repo's own contract says unsupported data must be reported explicitly. A shared disposition-recording helper (Reporter entry: Omitted/NotRetained + FeatureLocation) wired into every lossy branch turns silent corruption into a visible compatibility line.

Rows: HF-037, HF-040, HF-041, HF-042, HF-044

**T-06 · OPC relationship and media emission has no owner or shared allocator**

Media parts, part relationships and extra-part relationships are emitted by three independent pieces of code that do not know about each other: one writes duplicate ZIP entries, one puts every image's relationship in document.xml.rels regardless of owning part, and one mints ids that collide with already-minted ones. Introducing per-part media ownership plus one relationship-id allocator per part, with a test asserting Id uniqueness and target resolvability across every generated .rels, fixes all four and prevents the next variant.

Rows: HF-001, HF-009, HF-010, HF-044

**T-07 · Unbounded loops and allocation driven by document content**

Three independent places let a document control a loop bound or an allocation size with no ceiling: table pagination that cannot prove progress, a dash period that can be zero, and a column count derived from summed gridSpan values. Any of them turns an uploaded file into a frozen tab or an OOM. The systemic answer is the same in each: floor or clamp every content-derived quantity at the point it is read, and require a progress or size invariant for every loop over document data.

Rows: HF-006, HF-014, HF-017

**T-08 · Color values that are not theme-aware**

Colors are defined in four different ways — theme-independent :root tokens, hardcoded literals in feature CSS, media-query-only overrides, and a runtime-written accent — so dark mode fails contrast on the focused menu row, the review mode pill, tracked-change text, and validation errors, and explicit Light under a dark OS fails too. One pass that promotes every semantic color to a token defined in all three palette blocks (and has applySettings derive rather than write raw) closes the cluster and makes the next color addition safe by default.

Rows: HF-033, HF-086, HF-092, HF-093

**T-09 · UTF-16 vs UTF-8 offsets at the JS/WASM boundary**

The engine speaks UTF-8 byte offsets and JavaScript speaks UTF-16 code units; where the frontend computes an offset itself instead of using one the engine returned, the two diverge on any non-ASCII character — dropping pasted content in one place and picking the wrong quote glyph in another. The durable fix is a rule plus a helper: never synthesize an engine offset in JS; use the caret from the previous EditResult, or convert explicitly (byteOffsetToStringIndex already exists for the reverse direction).

Rows: HF-013, HF-055

**T-10 · Clipboard HTML importer is additive-only and structure-naive**

htmlToRuns can only ever turn formatting on, treats entering a block container as content, and drops top-level text nodes — so the three most common paste sources each corrupt the result in a different way (everything bold from Docs, a leading blank paragraph from Word, missing sentences around a table). All three live in one small module and want one pass: explicit inline style is authoritative, breaks are emitted only when content was contributed, and every top-level node type produces a block.

Rows: HF-026, HF-040, HF-054

**T-11 · Command-surface parity: capabilities reachable from only one surface**

The repo already tracks this class, and it recurs here in three forms: a security guard present on one surface and absent on another, commands declared for the context menu that the builder never reads, and list commands present on the ribbon/palette but missing from the menus. The playbook fix is the same each time — a SURFACE table naming every surface a command must appear on, plus a parity test asserting the registry and each built surface agree in both directions.

Rows: HF-019, HF-032, HF-076

**T-12 · Incremental layout caches that are inert, unbounded, or stale**

The galley cache is bypassed entirely for real documents, never evicts entries for deleted paragraphs, and its invalidation hash omits live decoration fields — so today it costs memory without helping, and the moment the bypass is lifted it starts rendering stale formatting. These three must be fixed together: broaden the fast path, add the per-build mark-and-sweep, and complete the hash with a test proven to go red.

Rows: HF-048, HF-079, HF-082

**T-13 · Operations that mutate before they can fail**

Several ops mutate the document and then return an error without restoring, and undo pops its history entry before applying it — so a refused edit can still destroy content and a failed undo leaves a half-reverted document with no way back. Numbering definitions written outside the command choke point are the same shape. One atomicity contract (compute into a clone or snapshot, commit only on success; pop history only after a successful apply; every mutation through an operation with an inverse) enforced with a debug_assert covers all of them.

Rows: HF-003, HF-045, HF-107

**T-14 · Unsaved work has no owner anywhere in the stack**

Six of the incoming gaps were the same hole seen from three lenses: the document exists only in wasm heap, `documentState` is written by eight call sites and read by NOBODY, and no code path — unload, open, drop — asks whether work would be lost. One systemic fix (make the dirty flag authoritative, then consume it in one guard) closes the data-loss tier; a second (one IndexedDB seam) closes the recovery tier. Building drafts, versions and recent files against three separate stores would be the expensive mistake here.

Rows: HF-002, HF-007, HF-011, HF-068, HF-073

**T-15 · Built in the engine, unreachable in the UI**

A repeating pattern where the hard part is already done and only the surface is missing, which makes these the cheapest rows per unit of user value in the whole list: find already computes the full match set and throws it away; the importer already writes a full findings report behind `importReportJson` with zero webapp readers; the accessibility tree is already projected but never audited; the model already stores image rotation and flip for render but exposes no edit op. Sweep for the pattern rather than fixing them one at a time.

Rows: HF-039, HF-047, HF-056, HF-064

**T-16 · One capability, one surface — the recurring command-parity defect**

opendoc keeps landing capabilities wired to a single control instead of a single descriptor: the compact-ribbon toggle exists only on the chevron, word count only in a hover tooltip, Help only as an alias for the palette. The repo already has the harness for this (webapp/tests/e2e/ribbon-only-commands.spec.mjs) and the rule (one descriptor → menu + ribbon + palette); enforcing it in the parity spec is a smaller fix than the sum of the rows.

Rows: HF-051, HF-064, HF-094, HF-104

**T-17 · No shared design-system layer: literals where tokens belong**

The review surface hardcodes light-mode Google hexes with no dark values, and both sibling repos solved this identically with semantic status tokens redefined per theme. opendoc additionally has TWO dark entry points (prefers-color-scheme and [data-theme="dark"]) and every existing dark patch covers only the first, so explicit Dark on a light OS is broken across the board. Defining the token set once in both blocks plus a CI check that fails on a bare hex fixes the shipped bug and prevents the next one.

Rows: HF-021

**T-18 · main.js is the monolith behind several unrelated-looking rows**

14,859 lines, 93% of webapp/src, zero exports. That single fact produced the diverged apply paths (a real data-loss bug), the nine hand-rolled dialogs with three different dismissal contracts, the absence of any boot seam for the docs/83 embed element, and the retrofit cost that got the i18n row downgraded. Extracting the mutation funnel, a dialog primitive and a mount(el, config) seam is the same work as three of these rows.

Rows: HF-007, HF-043, HF-085, HF-109

**T-19 · The chrome assumes one platform and one language**

63 hardcoded ⌘ glyphs on Tier-1 Windows/Linux, ~500 inline English strings, 13px inputs that trip iOS focus-zoom, 30px targets under the 44px floor. Every one is a display-layer assumption baked at the call site, and all four are cheap while the file is being touched for other reasons — the shortcut sweep and the i18n key extraction hit exactly the same label sites, so sequencing them together roughly halves the work.

Rows: HF-025, HF-060, HF-081

**T-20 · Untrusted input is the product's whole job, and it is the least guarded path**

Open is where a hostile or merely broken file meets the engine, and it currently has: no time budget or cancel, no failure isolation (a failed parse frees the live document), three of four fuzz targets never executed including the entire ODT importer, and no browser test that opens a hostile fixture. These read as four separate rows but they are one hardening pass on one code path.

Rows: HF-004, HF-077, HF-090

## Target design

`template.png` in the repository root is the agreed target design for the editor
("Vellum"), annotated with six regions: **1** tabbed ribbon (single row, no wrap),
**3** navigation rail (Outline, Pages, Comments, Bookmarks, Search), **4** context panel
(Comments, Properties, Styles), **5** status bar (page, word count, language, mode, view,
zoom, sync state), **6** command palette and quick access.

The shells for the rail and the context panel already exist (`webapp/editor.html:1010`
`.rail` / `:1015` `.side-panel`), so the design is the direction this editor is already
travelling — not a rewrite. Where it settles a question a row below was hedging, the row
now says so. Three elements of the design have no implementation at all today and are
recorded as such: the status-bar word count (HF-051), the language control (HF-081), and
the presence/Share/Synced cluster (HF-114).

Rows are not re-derived from the design here. Reconciling the full design against the
editor is its own pass, and should produce a design-conformance list separate from this
defect queue.

## Owner decisions — resolved

Answered by the owner on 2026-09-02. Five of the six are settled; **D-6 remains open.**
Each answer changes rows above, and the affected rows carry a **Decision.** line saying how.

**D-1 · Document bytes in browser storage**

- *Decision:* **Yes — follow opencalc.** Port the sibling shape: an IndexedDB store owned by `webapp/` as the host, keeping drafts, version snapshots and a recent list.
- *Consequence:* Unblocks HF-011 (autosave/crash recovery), HF-068 (version history) and HF-073 (recent documents). Build **one** store and put all three on it; three separate stores is the expensive mistake. HF-002's `beforeunload` guard still ships first — it needs no storage at all. Retention posture (how long full document buffers persist on-device, and the visible clear/disable control) still needs writing down before this lands.

**D-2 · Touch and mobile**

- *Decision:* **Supported.** Mobile is a target, not a deferral.
- *Consequence:* Raises HF-060 (coarse-pointer sizing, 16px inputs), HF-083 (page canvas on narrow screens), HF-088 (comments column breakpoint) to P1, and HF-097 to P2. `webapp/src/style.css:4905` still states "The desktop editor remains the primary target" and `docs/18-SUPPORT-MATRIX.md` lists mobile UI as out of scope for v1 — **both now contradict this decision and must be updated**, or the next agent will re-derive desktop-first from them. opencalc has the reference implementation: a documented `@media (pointer: coarse)` block, 17 touch handlers in `editor.core.js`, and four dedicated specs (`editor.touch.spec.mjs`, `editor.touch-targets.spec.mjs`, `editor.mobile-menus.spec.mjs`, `editor.mobile-editing-viewport.spec.mjs`). opendoc has **zero** coarse-pointer rules, **no** touch-specific code at all (no `touchstart`/`touchmove`, no `pointerType` branching, no pinch-zoom, no selection handles) and no mobile test. It is not starting from nothing, though: 54 `pointerdown`/`pointermove`/`pointerup` handlers already receive touch events, and object handles already set `touch-action: none`, so tap-to-place-caret and drag-to-select largely reach the editor today. The work is touch *disambiguation* and sizing, not new plumbing.

**D-3 · Print and PDF**

- *Decision:* **Real text is required.** Take Position A — a PDF writer in the engine export registry.
- *Consequence:* HF-030 stays P1 and its scope is now fixed: vector text with embedded fonts under opendoc's control, reachable through `availableExportFormats()` / `exportDocumentAs`, per `docs/98`. Accessible tagged PDF is not reachable through browser Save-as-PDF, which is why the registry path wins. The canvas-retention sub-fix (~8MB per page held until teardown) is independent and should land immediately.

**D-4 · Localization**

- *Decision:* **Ship it.** i18n is a capability, not a seam.
- *Consequence:* Raises HF-081 to P1 and confirms HF-025 (hardcoded ⌘ glyphs). Do them in one sweep — they touch the same label sites. The target design shows a language control in the status bar; there is no language surface in the editor today. Engine bidi text layout is already handled and is unrelated to chrome mirroring.

**D-5 · Collaboration**

- *Decision:* **Adopt opencalc's architecture and release model.**
- *Consequence:* HF-114 becomes an accepted roadmap epic rather than a parity row. Engine-authoritative (the crates own the transform; the JS layer owns only socket, reconnect and backoff), which matches opencalc's split and keeps the closed op set as the single choke point. Copy its operation-level deny-by-default `Access`/`permits(op)` model verbatim — permissions enforced per operation, not by hiding chrome. This depends on D-1 (there is nothing to share without persistence) and on a server, which opendoc does not yet have. Multi-quarter; keep it out of the hotfix ladder.

**D-6 · Embed / SDK contract**

- *Decision:* **OPEN — still needs an answer.**
- *Consequence:* opencalc's shipped `<opencalc-sheet>` / `@opencalc/sheet` shape, or opendoc's own approved `docs/83` `@casualoffice/document-runtime` (tracked as SDK-001)? Recommendation unchanged: `docs/83` is canonical and should not be replaced by a spreadsheet's element contract, but borrow two mechanisms that transfer cleanly — the capability-preset model with a framed page defaulting to `embedded`, and the hand-written `.d.ts` plus a consumer type-check job so the declaration cannot drift. HF-109 is blocked until this is answered.

### Rejected

**The `@schnsrw/design-system` package is rejected** (owner, 2026-09-02). It is not adopted,
not referenced, and not to be re-raised from `CROSS_EDITOR_CONSISTENCY.md` — treat that
note's design-system section as dead. opendoc keeps its own `:root` token system in
`webapp/src/style.css`. Token defects (bare hexes, missing dark values — HF-021, HF-033,
HF-086, HF-092) are fixed against opendoc's own tokens; none of them requires an external
package, and none of their fixes changes as a result of this rejection.

## Detail

Every row, in queue order. Locations were verified against the code at audit time.

### HF-001 — Saving fails outright when one image is used twice (header logo + body)

**P0** · import-export · bug · effort S · source: Internal audit · **Status:** Fixed (#497)

**Symptom.** A document whose header and body show the same logo cannot be saved at all — the export aborts with an opaque "package" error and the whole editing session is stuck with no way to get the file out.

**Location.** `crates/casual-doc-export/src/semantic.rs:592`

**Evidence.** One ZIP entry pushed per MediaReference with no de-dup; import mints a fresh MediaId per relationship into a shared table, so two rows can carry the same part_name; zip returns "Duplicate filename" and semantic.rs:614 maps it to ExportError::Package.

**Fix.** De-duplicate media by part_name before emitting ZIP entries (write each distinct part once, keep one relationship row per MediaReference). Add a round-trip test with the same media part referenced from a header and the body.

### HF-002 — Nothing guards unsaved work: no beforeunload prompt, and Open silently replaces a dirty document

**P0** · data-safety · bug · effort S · source: Sibling gap vs opencalc + docs · **Status:** Fixed (#495 unload guard; discard confirm in review)

**Symptom.** You type for an hour, hit Cmd+R or Cmd+W (or open a second file), and every edit is gone — no "Leave site?" prompt, no confirmation, no trace. The engine has no server and no local copy, so the wasm heap was the only copy that existed.

**In opendoc.** Absent. `rg beforeunload|onbeforeunload` across all of webapp/ → 0 hits; boot() (webapp/src/main.js:2396-2424) installs no unload guard. A dirty signal already exists and has ZERO readers: `documentState` (webapp/src/main.js:2251) / setDocumentState() (:2304-2315) is written at :2624/4615/7421/7486/11182/11500/11558/13066 and read by nothing but its own setter. `rg 'confirm\('` over main.js → 0 hits, so handleFile (:14185-14192) → openBytes (:2490) replaces a dirty document with no prompt on both the picker and drop paths (:14324-14326).

**In the sibling.** sheets/webapp/editor.core.js:10912-10924 (beforeunload armed as the first statement of main(), gated on isDirty(), with the rationale comment "The document lives in wasm memory and nowhere else… Closing the tab, reloading, or pressing Back discarded it without a word."); sheets/webapp/editor.core.js:9590-9611 (File ▸ New confirms first: "The most destructive verb in the application, and the only one that did not ask"); docs-repo/docx-editor/packages/react/src/components/AutosaveRestoreBanner.tsx

**Also.** grep for beforeunload in webapp/ returns nothing; openBytes does `if (doc) doc.free(); doc = open(bytes);` with no guard; no persistence/autosave of the open document exists.

**Location.** `webapp/src/main.js:2495; webapp/src/main.js:14198; webapp/src/main.js:14324`

**Fix.** Two small changes in main.js. (1) In boot() register `window.addEventListener("beforeunload", e => { if (documentState !== "edited") return; e.preventDefault(); e.returnValue = ""; })` — this is the first real consumer of the dirty flag; prefer asking the engine for its monotonic RevisionId (crates/casual-doc-transaction) over the UI pill, and follow opencalc's fail-dirty convention (sheets/webapp/editor.sheets.js:534-542): a failed read reports dirty, because a needless warning costs a click and the other mistake costs the document. (2) Route file.open (main.js:10637) and the drop handler through one `confirmDiscardIfEdited()` using the existing dialog shell. Gate both with an e2e that edits, reloads mid-edit, and asserts the prompt. NOTE: HF-007 must land with or before this, or the guard stays silent for every table and list edit.

### HF-003 — Backspace at a paragraph start leaves the document invalid, disabling footnotes, bookmarks, comments and suggestions

**P0** · rust-core · bug · effort S · source: Internal audit · **Status:** Fixed (#497)

**Symptom.** After one ordinary paragraph join, inserting a footnote or field, deleting an image, editing document properties, creating a style, page setup, bookmarks and every tracked-change/comment operation start failing with misleading errors, and ODT export refuses the document.

**Location.** `crates/casual-doc-edit/src/lib.rs:949; crates/casual-doc-edit/src/lib.rs:4767`

**Evidence.** JoinParagraphs is the only inline-mutating arm that never coalesces; validate_inlines rejects adjacent equal-property runs; every validate-and-rollback op (UpdateReviewState, CreateBookmark, InsertNote, SetCoreProperties) then fails permanently.

**Fix.** Call coalesce_adjacent_runs on the merged inlines in the JoinParagraphs arm (the SplitParagraph inverse is unaffected — split_at is the pre-merge byte length). Add a regression test asserting validate().is_ok() after joining two default-styled paragraphs, plus a debug_assert at the end of apply.

### HF-004 — A failed Open destroys the document already on screen and leaves the editor pointed at freed wasm memory

**P0** · data-safety · bug · effort S · source: Sibling gap vs opencalc · **Status:** Fixed (#495)

**Symptom.** You pick the wrong file, or a slightly corrupt one, from the Open dialog. You get a one-line red status message — and the document you were editing is dead: it is still on screen, but every keystroke, toolbar click and Save now throws. There is no undo and no recovery.

**In opendoc.** webapp/src/main.js:2495-2497 — `if (doc) doc.free();` runs BEFORE `doc = open(bytes);`. The catch at :2603-2605 only does console.error + setStatus; `doc` is never reset (`doc = null` appears only at the declaration, :2155), so it keeps a freed wrapper (ptr 0) while every page stays painted and every control stays enabled. Trigger is any corrupt or unsupported-content .docx/.odt/.json/.txt that passes the extension filter in handleFile (:14185-14192) and fails inside the engine.

**In the sibling.** sheets/webapp/editor.sheets.js:1055-1099 — openBytes wraps `wasm.session_open_as(ext, bytes)` in try/catch, sets ok=false, and never tears down the session before the parse succeeds: "Only a successful open renames the window. A failed one leaves the previous document's name in place, because the previous document is what is still on screen."

**Also.** openBytes frees then opens; catch at 2603 only logs and sets status; boot() enables the file input before awaiting the sample fetch, and the drop handler is bound at module scope.

**Location.** `webapp/src/main.js:2495; webapp/src/main.js:2603`

**Fix.** Parse into a local and swap only on success: `const next = open(bytes); if (doc) doc.free(); doc = next;`. In the catch, leave the previous doc, selection and page list untouched and say so (opencalc's friendlyOpenError + "the previous document is still open" wording). Add an e2e spec that opens a deliberately corrupt fixture over a live document and asserts the original is still editable afterwards.

### HF-005 — Typing in Suggesting mode is invisible — the page repaints from a frozen markup layout

**P0** · wasm · bug · effort M · source: Internal audit · **Status:** In review

**Symptom.** In Suggesting mode (or any document opened with tracked changes, where markup is on by default) nothing the user types ever appears on the page. The review card and Find both see the text, but the canvas only catches up after an unrelated zoom or resize.

**Location.** `crates/casual-doc-wasm/src/lib.rs:521; webapp/src/main.js:7311`

**Evidence.** markup_layout is assigned only in set_show_changes; finish_edit rebuilds self.layout only; page_count reads active_layout so the host's `newCount !== pages.length` full-rebuild escape hatch can never fire.

**Fix.** Rebuild markup_layout inside finish_edit/repaginate whenever it is Some, and compute pageCount and dirtyPages against the layout the renderer actually reads — not the editing layout.

### HF-006 — Pagination never terminates on a table whose repeated header fills the page

**P0** · layout · bug · effort M · source: Internal audit · **Status:** Fixed (#497)

**Symptom.** Opening a legitimate Word document (repeated header row taller than the usable content area, e.g. a full-width logo in the header cell) freezes the browser tab forever and grows memory until the tab or the host process is killed.

**Location.** `crates/casual-doc-layout/src/paginate.rs:766; crates/casual-doc-layout/src/paginate.rs:826`

**Evidence.** split_table_row escapes on used==0 only when placed is empty; repeat_headers_if_needed refills placed and advances cursor_y past content_bottom with no fit check; pages.len() is never bounded and halted is None on the full-pagination path.

**Fix.** Guarantee progress: skip header repetition when header_total leaves less than one line of room, and/or bail to the existing overflow-in-place branch when remaining() did not increase across a flush. Back it with a hard iteration/page cap.

### HF-007 — Four copy-pasted edit-apply paths have diverged — table and list edits never mark the document dirty

**P0** · code-structure · bug · effort S · source: Sibling gap vs opencalc · **Status:** Fixed (#495) · related: HF-002

**Symptom.** Change a table's borders, shade a cell, insert a text box or convert a list, and the header still says "Opened". Once the unsaved-work guard from HF-002 ships, those edits will be discarded on close without a prompt — the guard will be silently wrong for exactly the edits users are least able to redo.

**In opendoc.** webapp/src/main.js — `runNodeEdit` (:9375-9396) calls neither setDocumentState("edited") nor clearFindParagraphCache(); its 14 call sites are every table/list structural edit (:4862 toggleChecklistItem, :9067 setListFormat, :9525-9544 cell shading/valign/borders, :9642 calculateTableFormula, :9721, :10746). `applyEditResult` (:7295-7323) clears the find cache but never marks edited — and its callers at :4209 (page setup), :4247 (footnote/endnote insert), :4284 (running content), :11599 (insertTextBox), :11652 (insertShape) inherit that. `runToolbarEdit` (:7463-7486) marks edited but skips the cache; only `applyBookmarkEdit` (:11170-11183) does both.

**In the sibling.** sheets/webapp/ funnels every mutation through one `tryEdit` in editor.core.js, which is why isDirty() is asked of the engine rather than tallied per call site; tests/browser/editor.unsaved-work.spec.mjs:10-15 states the reason: a rule that enumerates its subjects "is one omission from being wrong, and the omission is the write path somebody adds last."

**Fix.** Collapse the four routines into one `applyResult(res, {moveCaret, stats})` that always clears the find cache, always marks edited, and always schedules the chrome refresh, with the differences passed as options rather than re-implemented. Add a unit/e2e assertion per apply path that the dirty flag flips. Mutate the code to prove the guard goes red before shipping it.

### HF-008 — Control characters in text are written raw into document.xml, producing a file nothing can reopen

**P1** · import-export · bug · effort M · source: Internal audit · **Status:** In review

**Symptom.** Paste text containing a vertical tab, form feed, or NUL (PDF/terminal/odd clipboard sources), save, and the resulting .docx is not well-formed XML: Word reports problems with the contents and opendoc's own importer fails with MalformedXml — the entire document is lost.

**Location.** `crates/casual-doc-export/src/semantic.rs:4011; crates/casual-doc-odf/src/export.rs:3829`

**Evidence.** BytesText::new writes run.text verbatim; quick-xml escapes only < > & ' "; the WASM entry point normalizes CR/CRLF only and no sanitizer exists on the text path in edit/model/wasm.

**Fix.** Sanitize at the model boundary (Operation::InsertText / plain_text_ops): allow only XML 1.0 Char (0x09, 0x0A, 0x0D, >=0x20, no surrogates/FFFE/FFFF), mapping U+000B/U+000C to a real LineBreak/page break. Add a defence-in-depth guard in both writers.

### HF-009 — Images in headers, footers, footnotes and comments lose their relationship on save

**P1** · import-export · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Save a document with a logo in the header and Word shows a missing-image placeholder or reports invalid content; re-importing the exported file drops the picture entirely. If the same header also has a hyperlink, the image's r:embed resolves to that hyperlink instead. Nothing is reported to the user.

**Location.** `crates/casual-doc-export/src/semantic.rs:1067; crates/casual-doc-export/src/semantic.rs:1365; crates/casual-doc-export/src/semantic.rs:1161`

**Evidence.** document_rels_xml emits /image for every entry of the shared definitions.media; per-part writers build RelBuilder::new(BTreeSet::new()) and part_rels_xml writes only hyperlink relationship types.

**Fix.** Track the owning part for each MediaReference, emit its /image relationship into that part's own _rels, and seed each part's RelBuilder with that part's reserved media ids instead of an empty set. Only body media belongs in document.xml.rels. Add a header-with-image round-trip test (none exists).

### HF-010 — Duplicate relationship Id in document.xml.rels makes the saved package invalid

**P1** · import-export · bug · effort S · source: Internal audit · **Status:** Fixed (#497)

**Symptom.** An ordinary document (a header logo plus one external hyperlink) saves with two `<Relationship Id="rId2">` entries; Word declares the file corrupt and offers to repair it, and if it recovers, the hyperlink points at styles.xml instead of the URL.

**Location.** `crates/casual-doc-export/src/semantic.rs:1092`

**Evidence.** The extras loop starts at `next = entries.len()` and skips only the locally rebuilt media/embedded reserved set, never the hyperlink ids already minted past that index.

**Fix.** Seed the extras allocator's reserved set with the ids already used by `entries` (or mint every id from one shared allocator / a distinct prefix). Add a test asserting Id uniqueness across every generated .rels part.

### HF-011 — No autosave, draft, or crash recovery — a tab crash or OS kill is unrecoverable

**P1** · data-safety · architecture · effort L · source: Sibling gap vs opencalc + docs · **Status:** Open · related: HF-002

**Symptom.** A browser OOM kill, an OS restart, or a wasm crash takes the whole session with it. Even after HF-002 lands, the only recovery for anything that is not a deliberate close is nothing at all — there is no snapshot to come back to.

**In opendoc.** Absent. `rg -i indexedDB|IDBDatabase|autosave|draft|recover` over webapp/src + editor.html → nothing document-related (the two apparent hits are Material-Symbols ligature strings). The only durable storage in the whole webapp is four localStorage preference writes: main.js:286/296 (ribbonCollapsed), :13657/13664 (smart quotes), :14344/14352 (opendoc.settings). No drafts/recovery spec among the 93 e2e specs. docs/03-HLD.md:185 already anticipates a StorageProvider "only for optional autosave".

**In the sibling.** sheets/webapp/editor.drafts.js (1026 lines) — IndexedDB draft store on a quiesce-5s / ceiling-60s cadence copied from the collab server, a meta+bytes two-row split so the recovery bar costs kilobytes (:136-150), a cross-tab slot lease (:258-303), a recovery bar that OFFERS and never applies (:667-780), writes on visibilitychange rather than beforeunload (:635-655), suppressed in host-owned modes (:31-35). docs-repo/docx-editor/packages/react/src/utils/autosave.ts (single-slot IDB ArrayBuffer) + components/AutosaveRestoreBanner.tsx (24h age gate, same-doc gate, Restore/Discard) + hooks/useAutoSave.ts + file-source/AutosaveStatus.tsx ("Saved 2 min ago" pill).

**Fix.** Port editor.drafts.js's shape (not a new invention): an `opendoc-drafts` IndexedDB with a meta row and a bytes row per tab slot, serialized via the existing `doc.exportAs("org.casualoffice.normalized-json")` (main.js:10641); write on quiesce (5s idle, 60s ceiling) keyed on the engine revision moving; write on visibilitychange, not beforeunload; on boot show a bar that names the document, its age, and how far ahead of the last download it is, and OFFERS the draft rather than applying it; delete on successful Save; adopt docs-ref's 24h age gate and same-doc gate rather than inventing new ones. Do not autosave in a host iframe. This is the one IndexedDB seam ranks 17 and 18 also build on. BLOCKED ON the privacy decision below.

**Decision.** **Unblocked by D-1** (browser storage approved, opencalc shape). Build the IndexedDB seam once; HF-068 and HF-073 sit on top of it.

### HF-012 — Numbering, note, comment and bookmark ids are exported as 20-digit numbers Word cannot accept

**P1** · import-export · parity · effort M · source: Internal audit · **Status:** Open

**Symptom.** Re-exporting any document with a bulleted list, footnote, comment or bookmark writes ids like 18446744073709551620 into ST_DecimalNumber attributes; Word either refuses the file or silently drops the list numbering, notes and bookmarks. opendoc's own round-trip tests never notice because it re-reads them as opaque strings.

**Location.** `crates/casual-doc-export/src/semantic.rs:2270; crates/casual-doc-export/src/semantic.rs:1347; crates/casual-doc-export/src/semantic.rs:4153`

**Evidence.** abstract_id_token / num_id_token / note_id_token / comment_id_token / bookmarkStart all stringify node_id().as_u128(); NodeId is (namespace << 64) | counter with default namespace 1.

**Fix.** Maintain a per-export dense mapping internal id -> 1..n for every ST_DecimalNumber attribute and write that; keep NodeId in the model only. Assert every emitted value fits in i32. Acceptance = the exported file opens clean in Word and LibreOffice.

### HF-013 — Rich paste in Suggesting mode scrambles or drops text containing any non-ASCII character

**P1** · webapp-js · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Pasting ordinary prose copied from Word or Docs (curly quotes, em dashes, accents, any non-Latin script) while suggesting either silently loses the rest of the paste behind "That edit isn't supported for this selection yet" or interleaves the runs at the wrong positions.

**Location.** `webapp/src/main.js:13284`

**Evidence.** `offset += run.text.length` (UTF-16 units) between successive suggestStyledInsert calls, while the engine's offsets are UTF-8 bytes (Pos::new(node, offset + text.len())); main.js already carries byteOffsetToStringIndex for the reverse direction.

**Fix.** Stop recomputing the insertion point in JS — use the caret returned by each EditResult (applyEditResult already sets selection from it). If an offset must be computed, advance by UTF-8 byte length, not String.length.

### HF-014 — Rendering hangs forever on dashed/dot-dash underline with a font reporting zero underline thickness

**P1** · render · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** A page containing dashed-underlined text in a subsetted or symbol font (post.underlineThickness == 0, or negative in a malformed font) never finishes rendering — the tab or host process spins indefinitely.

**Location.** `crates/casual-doc-render/src/lib.rs:1117; crates/casual-doc-render/src/lib.rs:1183`

**Evidence.** The dashed closure advances cursor += on + off with no floor and both derive from thickness; skrifa sets underline whenever post parses, unclamped, so the fallback never fires. DotDash has the identical zero-period defect.

**Fix.** Sanitize once at the read site: replace the metrics pair with (offset, thickness.abs().max(size_px * 0.06).max(0.5)) so every decoration branch sees a positive thickness; also floor the dash period before the loop.

### HF-051 — No Word Count dialog and no selection-scoped counts — the code flags this hole itself

**P1** · word-count · parity · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** The most common counting need — "how many words is this passage" — is unavailable, and the full breakdown is hidden in a hover tooltip that disappears at narrow widths and is unreachable by keyboard or touch. Writers on a word budget hit this daily.

**In opendoc.** webapp/src/main.js:2376-2379 admits it in a comment: "Word keeps the full set one gesture away in its Word Count dialog; until we have that dialog, the whole region carries every figure so nothing shed becomes unobtainable" — the fallback is a `title` tooltip on the footer stats span. updateStats (:2358-2382) calls doc.documentStats() and never consults selection. APP_MENU_SECTIONS.tools (:10963) has no entry. Engine-side, `document_stats` (crates/casual-doc-wasm/src/lib.rs:6081-6096) walks all surfaces with no range parameter, so selection-scoped counting has no API today.

**In the sibling.** docs-repo/docx-editor/packages/react/src/components/dialogs/WordCountDialog.tsx:5-21, :80-105 — pages, words, characters with and without spaces, paragraphs and reading time, opened from the menu or Ctrl+Shift+C (the Google Docs binding); components/StatusBar.tsx:19 also surfaces readability and reading time.

**Fix.** Add a `tools.wordCount` command (⌘⇧C) on the shared dialog shell (style.css:672) showing pages / words / chars with spaces / chars without spaces / paragraphs / reading time, plus a leading "X of Y words" row when hasRange() (main.js:7136) is true. Register it once in APP_MENU_SECTIONS.tools (:10963) so menu, ribbon and palette all inherit it. Decide in design whether selection counts come from a new ranged stats binding or a JS-side count over the existing copyText(selection) path — the binding does not exist today.

**Decision.** **Raised by the target design:** word count is a permanent status-bar element in `template.png`, not an optional dialog.

### HF-015 — A six-byte non-ASCII color string panics the WASM module and poisons the session

**P1** · wasm · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** A crafted clipboard payload (or a host passing a user-typed color) traps the engine: every subsequent call throws "unreachable executed" and the open, possibly unsaved document is unrecoverable.

**Location.** `crates/casual-doc-wasm/src/lib.rs:16001`

**Evidence.** parse_hex_color checks byte length then slices &h[0..2]/[2..4]/[4..6]; executing it on a 6-byte multibyte string panics on a char boundary. Reached from pasteRichRuns, pasteExternalStructured, setUnderlineColor, shape_rgba and review_format_delta, none of which sanitize.

**Fix.** Validate before slicing — `if h.len() != 6 || !h.is_ascii() { return None; }` (or iterate bytes/chars). Add a unit test with a 6-byte multibyte input and mutate the guard to prove it goes red.

### HF-016 — There is no "New blank document" — the only way to get a document is to open someone else's file

**P1** · file · parity · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** You open the editor to write something new and there is no way forward — you have to find or fabricate a .docx elsewhere first. The empty state reads as a viewer, not an editor.

**In opendoc.** Absent at both layers. The complete File command set is webapp/src/main.js:10637-10644 (open / save / 4 exports / print / properties) and APP_MENU_SECTIONS.file (:10927-10932) renders exactly those. More decisively, the WASM surface has no document constructor: the only entry points are `open(bytes)` and `openAs(bytes, formatId)` (crates/casual-doc-wasm/src/lib.rs:451,457); `Document::new` exists only inside #[cfg(test)]. `?blank=1` (main.js:2413-2423) boots an empty editor with no document and no way to make one.

**In the sibling.** docs-repo/docx-editor/packages/react/src/components/DocxEditor.tsx:4059-4066 binds Mod+N to `shortcutActionsRef.current.new()`; DocxEditor.tsx:534 documents the `onNew` prop ("Callback invoked when the user picks File → New"); components/Toolbar.tsx:236. sheets/webapp/editor.core.js:9590-9611 has the same verb with a mandatory dirty confirm.

**Fix.** Engine + webapp, not a menu wiring job. Add a WASM export constructing an empty single-section document with the default style set, then register a `file.new` descriptor (⌘/Ctrl+N) in the command list at main.js:10637 so the File menu, the palette and any future surface inherit it from one place, route it through the same `confirmDiscardIfEdited()` as HF-002, and add a "Blank document" primary action to the drop-zone empty state (editor.html:1033) beside the demo presets.

### HF-017 — Table column count is unbounded on one side and collapses to 1 twip on the other

**P1** · layout · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** A small uploaded .docx with a grid-less table of many large-gridSpan cells drives multi-gigabyte allocation and an OOM abort. Conversely, a converter-produced table with more cells than w:tblGrid columns renders the extra cells as an unreadable one-character-per-line sliver that blows up the row height — Word renders both correctly.

**Location.** `crates/casual-doc-layout/src/flow.rs:1975; crates/casual-doc-layout/src/flow.rs:1348`

**Evidence.** ncols is grid.len() when a grid exists (so out-of-range cells get a clamped 1-twip slot) and the uncapped max span-sum when it does not; cols and edges are both allocated from it; import caps gridSpan and depth but not cells-per-row.

**Fix.** Derive ncols as max(grid.len(), max row span-sum) and clamp it to one shared MAX_TABLE_COLUMNS, clamping col/col+span in both the constraint loop and flow_table's edges; synthesize content-sized constraints for the extra columns and report truncation. Cap cells-per-row at import alongside the existing gridSpan and depth caps.

### HF-018 — Blocking site data leaves the editor completely dead (blank, no handlers)

**P1** · webapp-js · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** With cookies/site data blocked, or when the editor is embedded cross-origin from the marketing site, the page loads and nothing works at all — no ribbon wiring, no keyboard, only a console error.

**Location.** `webapp/src/main.js:13657; webapp/src/main.js:13664`

**Evidence.** Module-scope `localStorage.getItem(SMART_QUOTE_PREF)` with no try/catch, while every other access in the file is wrapped with a "private mode / storage disabled" comment; a module-scope throw aborts evaluation of the whole file.

**Fix.** Route all five storage accesses through a guarded readPref/writePref helper (defaulting smart quotes to on), removing the duplicated try/catch at 285/295/14342 at the same time.

### HF-019 — Right-click "Open link" bypasses the URL scheme allowlist the click path enforces

**P1** · security · security · effort S · source: Internal audit · **Status:** Open

**Symptom.** A hyperlink in an untrusted document whose target is javascript:, data: or file: is correctly blocked when clicked, but opens unchecked from the context menu — the two surfaces disagree about the same capability, and the menu path also leaks the referrer.

**Location.** `webapp/src/main.js:5884; webapp/src/main.js:5052`

**Evidence.** openContextLink calls window.open(link.url, "_blank", "noopener") with no parsing; activateLink parses new URL() and permits only http/https/mailto. link.url comes verbatim from the model (import performs no scheme filtering).

**Fix.** Extract the URL parse + http/https/mailto allowlist into one resolveExternalTarget helper and call it from openContextLink, activateLink and the link chip; pass "noopener,noreferrer" consistently. Add a menu-vs-click parity test for a javascript: target.

### HF-060 — No coarse-pointer sizing and 13px inputs — every menu row is mouse-sized and iOS Safari zooms on every field focus

**P1** · a11y · a11y · effort M · source: Sibling gap vs opencalc + docs · **Status:** Open · related: HF-098

**Symptom.** On a touchscreen laptop or tablet, tapping any text field zooms Safari in and leaves the page scrolled sideways, and every menu row and ribbon button is a 30px target against the 44px floor — with destructive commands two rows from their neighbours.

**In opendoc.** `rg 'pointer: *coarse|hover: *none|prefers-contrast|forced-colors'` over webapp/ → 0 hits. --h-control is 30px (style.css:53) and --fs-body 13px (:48) propagates to every field via `button, select, input { font: inherit }` (:195-199). The complete @media inventory is prefers-color-scheme (104, 1444, 1494, 4511), prefers-reduced-motion (3365) and six max-width breakpoints (3814, 4874, 4880, 4889, 4908, 4955) that only shed or condense chrome. playwright.config.mjs:53 has one Desktop Chrome project with no hasTouch; no touch/mobile spec among 93. Partial credit: --h-rail-control is already 44px (:57) and touch-action:none is already set on five overlay handle classes (:4235, 4354, 4397, 4524, 4557), so touch DRAGS do reach the pointer handlers — the gap is target sizing, input font size, and any coarse-pointer test.

**In the sibling.** sheets/webapp/editor.css:2181-2225 `@media (pointer: coarse)` — 44px min-height on .menu-drop/.menu-sub/.ctx-menu/.tb-more-flyout buttons, 44x44 icons, restored band heights, with the rationale at :2150-2180: keyed on modality not width "because the question is what is doing the pointing and not how wide the window is", and "'Clear formatting' and 'Merge cells' are two apart". Gated by tests/browser/editor.touch.spec.mjs (real CDP touch, 390x844), editor.touch-targets, editor.mobile-menus, editor.narrow-screens. docs-repo/docx-editor/packages/react/src/styles/editor.css:924-985 clamps to 100vw, forces 44px HIG minimums, and sets `font-size:16px` on the title input at :961 ("iOS Safari won't zoom in on focus at ≥16 px"); plus components/ui/MobileFormatBar.tsx and hooks/usePinchZoom.ts.

**Fix.** Take the free part now (S): `@media (pointer: coarse) { input, select, textarea { font-size: 16px } }` — it removes the iOS focus-zoom and cannot regress desktop density. Then the coarse-pointer block: 44px min-height on .app-menu-item / menu rows / context-menu rows / ribbon overflow rows and 44x44 on icon-only buttons, spending the growth on floating surfaces (menus, popovers) that cost the page zero height, as opencalc does. Add a hasTouch Playwright project with one spec driving select → context menu → resize handle so the floor cannot ratchet back. Fold this into the existing REVIEW-GAP-024 / P1G-REVIEW-039 row rather than opening a parallel track, and see the mobile-scope decision below before scheduling the rest (pinch zoom, mobile format bar, phone reflow).

**Decision.** **Raised by D-2:** mobile is now a supported target, so the coarse-pointer floor is required, not hygiene.

### HF-020 — Opening a tracked-changes document can throw mid-render and leave the page list blank

**P1** · wasm · bug · effort S · source: Internal audit · **Status:** In review

**Symptom.** Any document whose struck-through deletions push content onto an extra page fails to render on open — pageSize throws "page index N out of range", renderAll aborts before replacing the page list, and the user sees a blank or stale viewer. No user action required; markup is on by default for such files.

**Location.** `crates/casual-doc-wasm/src/lib.rs:8496; webapp/src/main.js:2953`

**Evidence.** page_count and render_page_of read active_layout, but page_size_inner is hard-wired to self.layout.pages; markup materializes deleted text that the editing layout gives zero width, so the markup page count can exceed the editing count.

**Fix.** Make page_size_inner use active_layout() so pageCount, renderPage and pageSize all agree on one layout (or expose the markup count separately); guard the webapp loop with try/catch as a backstop.

### HF-021 — Track-changes UI hardcodes light-mode Google hexes — suggestions and the mode switch are unreadable in dark mode

**P1** · design-system · ui · effort M · source: Sibling gap vs opencalc + docs · **Status:** Open · related: HF-033, HF-086, HF-092

**Symptom.** In dark mode the deletion text in the review margin is effectively invisible (~1.4:1) and the always-visible Editing/Suggesting/Read-only segment in the footer is unreadable. Users cannot read their own tracked changes — the flagship Word-parity feature.

**In opendoc.** webapp/src/style.css: literal light-mode hexes throughout the review surface — :3568 `.review-mode-seg[aria-pressed="true"] {color:#137333; border-color:#188038}`, :3702 `.review-margin-insertion p {border-left-color:#188038; color:#137333}`, :3703 `.review-margin-deletion p {color:#b3261e}`, :3563/:3565 inline-bar hovers, :3583-3586 suggesting banner (#f9ab00/#fbbc04/#b06000), :4322 #188038. The file's only dark blocks are :104, :1444, :1494 and :4511 and none contains a `review-*` selector. Against the dark surface #212429 (:108/:129), #137333 is ~2.5:1 and #b3261e ~1.4:1.

**In the sibling.** docs-repo/docx-editor/packages/react/src/styles/tokens.css:40-47 defines semantic status tokens (--color-danger/-soft, --color-success/-soft, --color-warning/-soft, --color-info/-soft) and :99-106 redefines every one for dark. sheets/webapp/editor.css:89 --oc-success-color / :95 --oc-danger-color with dark values at :148 and :193 and derived rings via color-mix at :121/:170/:215.

**Fix.** Add the missing semantic status tokens (--success/--warning/--info/--danger + -soft) to the :root light block (style.css:76-98) mirroring docs-ref tokens.css, and define them in BOTH dark entry points — `@media (prefers-color-scheme: dark) :root:not([data-theme])` (:104) and `:root[data-theme="dark"]` (:126). Every existing dark patch in the file (:1444, :1494, :4511) only covers the media query, so a user who explicitly picks Dark on a light OS currently gets none of them — fix those in the same pass. Then replace every literal in the review-* rules (3563-3752, 4322), amber banner included, with tokens, and add a CI check that fails on a bare hex outside the token blocks.

**Decision.** **Note:** fix against opendoc's own `:root` tokens in `webapp/src/style.css`. The external `@schnsrw/design-system` package is rejected by the owner (2026-09-02) and must not be introduced.

### HF-022 — With changes shown, clicking places the caret in the wrong place and selection highlights miss the text

**P1** · wasm · bug · effort L · source: Internal audit · **Status:** Open

**Symptom.** On exactly the documents the review feature exists for — even with zero edits — clicking a word lands the caret several characters or lines away, and drag-selection paints rectangles that do not cover the glyphs underneath. Suggesting mode forces this state on.

**Location.** `webapp/src/main.js:2933; crates/casual-doc-wasm/src/lib.rs:872`

**Evidence.** renderAll paints via active_layout, but body_hit, caret_rect and selection_rects all build LayoutSnapshot::new(&self.layout); nothing in the frontend gates click-to-place or typing on showingChanges.

**Fix.** Enforce the engine's stated contract: make the markup view read-only (no click-to-place, no typing) while showingChanges is true — or add markup-layout hit-test/geometry entry points and use them whenever it is on.

### HF-023 — Every table command is enabled but always fails for tables in headers, footers, notes and text boxes

**P1** · parity · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Click into a header layout table (very common in imported .docx) and the whole Table ribbon group lights up — then insert/delete row, insert/delete column, cell shading, borders, table properties, delete table and merge/split all fail with an internal error. The commands can never succeed there.

**Location.** `crates/casual-doc-edit/src/lib.rs:1132; crates/casual-doc-edit/src/lib.rs:1407; crates/casual-doc-wasm/src/lib.rs:4708`

**Evidence.** InsertRow/DeleteRow/InsertColumn/DeleteColumn/DeleteTable/SetTableCellProperties/SetTableProperties/ReplaceTable all pass doc.body_mut(), while the read side (find_table, locate_table_row, in_table) walks surface_block_lists.

**Fix.** Route the table ops through on_owning_surface_mut as the object ops already do, and teach find_table_mut/find_cell_mut/remove_table to descend into paragraph inlines (text boxes, shape groups). Add a surface-matrix test asserting each table op succeeds on every surface locate_table_row can return.

### HF-024 — Down arrow in a table jumps sideways to the next cell instead of the row below

**P1** · editor-ux · parity · effort M · source: Internal audit · **Status:** Open

**Symptom.** In a 2x2 table, pressing Down at the bottom of the top-left cell moves the caret up and to the right into the top-right cell; the cell directly below can never be reached with the arrow keys. Word and Docs both move down within the column.

**Location.** `crates/casual-doc-layout/src/hittest.rs:415; crates/casual-doc-layout/src/hittest.rs:785`

**Evidence.** move_vertical selects by flow index (cur ± 1) into a flat line list, and the TableRow fragment emits all of cell 1's lines before cell 2's. Same defect applies to multi-column sections.

**Fix.** Make move_vertical geometric: pick among line_boxes the candidate whose band is strictly below (Up: above) the current line, minimizing (vertical gap, |x - affinity|) and preferring lines whose cell x-range contains the affinity; fall back to flow order only when nothing exists in that direction.

### HF-025 — Every shortcut label is a hardcoded ⌘ glyph — Windows and Linux users are shown keys their keyboard does not have

**P1** · i18n · ui · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** On Windows and Linux every tooltip, menu row and the command palette advertises ⌘S, ⌘Z, ⌘⇧P — symbols on no key of the user's keyboard. The shortcuts actually work with Ctrl; only the labels lie, which teaches the product wrong.

**In opendoc.** 18 literal ⌘ occurrences in webapp/editor.html (:62 `title="Search commands (⌘⇧P)"`, :69 `<kbd>⌘⇧P</kbd>`, :215, :216, :227, :231, :232, :260-263, :330-331, :375, :926, :960-962, :980) and 45 in webapp/src/main.js (:6169 "⌘K", :6193 "⌘⌥M", :7792-7793, and the whole palette table at :10638-10808). Rendered on three surfaces — palette (:12430), app menu (:11019), context menu (:6608-6614) — while the handler binds `e.metaKey || e.ctrlKey`. The platform IS already detected: keyboard.mjs:9 `keyboardPlatform()` / main.js:2260 EDITOR_KEYBOARD_PLATFORM, but consumed only for caret and word/line deletion direction (:13857, :13913, :13950). Windows/Linux desktop browsers are Tier 1 in docs/18-SUPPORT-MATRIX.md:43.

**In the sibling.** docs-repo/docx-editor/packages/react/src/lib/platform.ts:29-45 — `formatShortcut(keys)` takes a portable string (`Ctrl+Shift+L`) and renders ⌘/⌥/⇧ only on Mac; the file header records the exact bug it fixed: "the Toolbar hard-coded ⌘ symbols even on Windows." Consumed at components/FormattingBar.tsx:397/406/490/501/512 and components/PanelRail.tsx:171.

**Fix.** Add a display-side `formatShortcut(portable)` beside the existing `keyboardPlatform()` in webapp/src/keyboard.mjs (port platform.ts:29 verbatim), store shortcuts as portable strings (`CmdOrCtrl+Shift+P`), and render every `shortcut:`, `title=` and `<kbd>` through it. Sweep all 63 literals. Add the assertion to webapp/tests/keyboard.test.mjs that a `standard` platform never emits ⌘/⌥/⇧.

**Decision.** **Confirmed by D-4.** Do this in the same sweep as HF-081 — both touch the same label sites.

### HF-026 — Everything pasted from Google Docs arrives bold

**P1** · import-export · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Copying normal-weight paragraphs from Google Docs — the most common external paste source — makes every pasted run bold in the document and bold in the exported .docx.

**Location.** `webapp/src/clipboard.mjs:125; webapp/src/clipboard.mjs:283`

**Evidence.** Reproduced against the shipped module: a `<b style="font-weight:normal">` wrapper yields runs with bold:true, because applyInlineStyle can only ever turn bold on.

**Fix.** Make explicit inline style authoritative rather than additive: font-weight normal/<600 clears bold, font-style:normal clears italic, text-decoration:none clears underline/strike. Add a clipboard test for the Docs `<b style="font-weight:normal">` guid wrapper.

### HF-027 — Accept All / Reject All silently leaves tracked changes in headers, footers, notes and text boxes

**P1** · wasm · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** The UI reports all changes applied while the revision count stays non-zero and the exported .docx still carries redlines recipients will see. Deciding one revision of a group outside the body also splits the group in half — the state the atomicity guard exists to prevent — and if every revision lives outside the body the user gets a generic "made no change" failure.

**Location.** `crates/casual-doc-wasm/src/lib.rs:7398; crates/casual-doc-wasm/src/lib.rs:7241; crates/casual-doc-wasm/src/lib.rs:7210`

**Evidence.** decide_all mutates document.body().to_vec() while collection uses *_all; decide_revision resolves anchors all-surface but reads its group guards from the body only. Reachable with no authoring — an imported DOCX with w:ins in a header hits it.

**Fix.** Replace the body-only walk and both atomicity guards (plus decide_all's group precheck) with the all-surface lookups already available (surface_block_lists / find_review_*_all), building one multi-paragraph UpdateReviewState across every changed surface. Test: author a revision in a header and a grouped suggestion in a text box.

### HF-028 — Alt text never reaches the accessibility tree, and figure paragraphs and table headers vanish

**P1** · accessibility · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** A screen reader reading a figure-heavy document hears the paragraph before the chart and the one after it — the image and the alt text the user carefully typed are simply absent. Tables read with no column or row headers.

**Location.** `crates/casual-doc-wasm/src/lib.rs:8138; crates/casual-doc-wasm/src/lib.rs:10929; webapp/src/main.js:9855`

**Evidence.** collect_a11y_blocks derives every block from node_plain_text and continues on empty text; append_node_plain_text falls through for drawings; A11yBlockJson has no image variant and flattens table cells to strings.

**Fix.** Add an Image { alt, title } variant to A11yBlockJson, emit it for paragraphs containing a drawing (from docPr/@descr) instead of dropping empty-text paragraphs, and render it as `<img alt>`/`<figure role="img">`. Emit table header rows as `<th scope="col">`.

### HF-029 — The status line is the only error channel and is not a live region — every failure is silent to screen readers

**P1** · accessibility · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** A blind user opening a corrupt file, failing a save, hitting a blocked link, or trying an edit refused in Viewing mode hears nothing at all — a failure is indistinguishable from the app doing nothing.

**Location.** `webapp/editor.html:1270; webapp/src/main.js:2290`

**Evidence.** #status has no role/aria-live in markup or at runtime, while the less critical #compatibilityStatus, #reviewLiveRegion, note fields and mode banners all do.

**Fix.** Add role="status" aria-live="polite" aria-atomic="true" to #status, route kind==="error" through assertive semantics (second sr-only region or role=alert), and clear before writing so repeated identical messages re-announce — the announceReview helper already implements that trick.

### HF-030 — Print and "Save as PDF" emit a 150-DPI raster — no selectable, searchable or accessible text, and a long document exhausts the tab

**P1** · print · bug · effort L · source: Sibling gap vs docs (ProseMirror) · **Status:** Open · related: HF-105

**Symptom.** "Save as PDF" — how most people produce a shareable document — yields an image-only PDF: no text selection, no Ctrl+F, no copy, no screen reader, and a huge file. Printing 200 pages can also kill the tab on memory.

**In opendoc.** webapp/src/main.js:2839 `const PRINT_DPI = 150;` and printDocument() at :2872-2923 render every page via `doc.renderPage(i, PRINT_DPI)` and blit ImageData into a `<canvas class="print-page">`, then window.print() prints those bitmaps. The RGBA buffer is freed at :2905 but every full-size sheet canvas is retained in #printContainer until teardown (~8 MB/page at 150 DPI Letter). No PDF exporter exists: `rg -ni '\bpdf\b'` over crates/ and webapp/src hits only the `kw:` search string on file.print (:10643); availableExportFormats (crates/casual-doc-wasm/src/lib.rs:8268) backs only DOCX/ODT/text/normalized-JSON.

**In the sibling.** docs-repo/docx-editor/packages/react/src/hooks/usePrintFlow.ts:10, :163-174 — the print flow clones the painted DOM pages (forceRenderAllPages / restoreVirtualization) so print and PDF carry real text runs, and `handleExportPdf` reuses that pipeline and sets the print-window title so the browser's Save-as-PDF destination gets the right filename. components/ui/PrintPreview.tsx:24-40 wraps it with page-range / margins / background / scale options.

**Fix.** Two separable pieces. (a) Immediate (S): release each page canvas incrementally in printDocument() instead of holding all of them until teardown (main.js:2845-2923), and raise PRINT_DPI. (b) The parity requirement is that print output carries real text — see the decision below on whether that is a vector/text print path or a PDF writer registered in the export-format registry so `availableExportFormats()` surfaces it and exportDocumentAs (:13041) picks it up with no UI change.

**Decision.** **Scoped by D-3:** real-text PDF is required, via a PDF writer in the export registry (Position A), not browser Save-as-PDF. Take the canvas-retention sub-fix immediately; schedule the writer as its own engine project (docs/98).

### HF-081 — No localization seam — every string is an English literal inside a 14.9k-line file

**P1** · i18n · architecture · effort L · source: Sibling gap vs opencalc + docs · **Status:** Open

**Symptom.** opendoc can only ever ship in English, and a host embedding it cannot supply its own language. A translator has nowhere to put a string.

**In opendoc.** No layer at all. `rg -n 'i18n|locale|translat'` over webapp/src/main.js matches only localeCompare (:8840, :11193) and Intl.DateTimeFormat (:11459/:14500); webapp/src holds eight modules (clipboard, context_menu, fidelity, format_io, home-embed, keyboard, main, web_fonts) and none is i18n. Every label is inline — the command registry (:10630-10800), status text, all nine dialogs in editor.html — and plurals are hand-rolled per site (`word${words === 1 ? "" : "s"}`, :2369-2374). editor.html:2 is `<html lang="en">`, hardcoded. `rg 'dir="rtl"|bidi'` over webapp → 0 (engine bidi layout is separate and unblocked by chrome mirroring).

**In the sibling.** sheets/webapp/editor.i18n.js (206 lines) — "Locale, message lookup and the user-facing wording of errors and tips": `t(key, fallback)`, `setMessages(forLocale, map)` with a `relabel()` pass over the live DOM, `setLocalePicker`/`syncLocalePicker`, locale-aware number formatting and human-readable error rewriting; hosts supply their own catalog through the SDK rather than forking. docs-repo/docx-editor/packages/react/i18n ships de/en/he/pl/pt-BR/ru/tr/zh-CN.json behind src/i18n/LocaleContext.tsx `useTranslation()` with typed TranslationKey, plus i18n:validate/fix/new/status scripts and CI that fails on a missing key; docs/i18n.md:7-20.

**Fix.** Already tracked as REVIEW-GAP-025 (docs/81:102, P2 Debt) under P1G-REVIEW-039 — widen that row from review copy to the whole chrome instead of opening a parallel one. Add webapp/src/i18n.mjs with `t(key, fallback)` + `setMessages` + a `relabel()` DOM pass, and convert the highest-density surfaces first (command registry, app menu labels, status/error text) — the fallback argument makes the conversion partial-and-correct at every step, so day-one behaviour is byte-identical. Set `<html lang>` from the locale and mirror `dir` on chrome containers. Add the sibling's CI validate check so it cannot rot. See the i18n-scope decision below on whether translations are in scope.

**Decision.** **Raised by D-4:** i18n is a shipping capability, not a seam. The target design puts a language control ("English (US)") in the status bar; opendoc has no language surface at all (0 hits).

### HF-031 — Command palette announces nothing while arrowing through results

**P1** · accessibility · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** A screen-reader user types in the palette, presses Down three times, hears silence, presses Enter and gets whatever command happened to be selected. Disabled-command reasons in the hint column are never announced either — and the palette is the documented keyboard fallback for controls with no other keyboard route.

**Location.** `webapp/src/main.js:12422; webapp/src/main.js:12437; webapp/editor.html:925`

**Evidence.** Options carry role=option but no id and no aria-selected; selection is the CSS class .sel only; #cmdInput is a bare text input; moveCmdSelection moves nothing focusable.

**Fix.** Give each option a stable id and aria-selected in setCmdSel; make #cmdInput role=combobox with aria-expanded/aria-controls/aria-autocomplete and aria-activedescendant tracking the selection; set tabIndex=-1 on option buttons.

### HF-083 — Pages are squashed horizontally in any window narrower than the sheet

**P1** · layout · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** In a 700px window (split screen, tablet, or the home-page embed) the page keeps its full height but is clamped to 96vw wide, so every glyph is compressed about 18% — the document renders at the wrong aspect ratio, and no error is reported.

**Location.** `webapp/src/style.css:4271`

**Evidence.** .page-wrap has max-width: 96vw with no aspect compensation while both width and height are set inline from twips and .page is 100%/100%; scaleOf derives sx and sy independently so nothing detects the distortion. All e2e specs run at >=1280px.

**Fix.** Drop max-width: 96vw and let the already-scrollable .viewport scroll horizontally as Word does; or scale both axes together (transform: scale with top-center origin, or default to fit-width below the page width) so width and height stay proportional.

**Decision.** **Raised by D-2:** phone/tablet reflow of the page canvas is now in scope.

### HF-032 — Insert-table grid picker is pointer-only and exposes 80 unnamed buttons

**P1** · accessibility · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** A keyboard user opens Insert ▸ Table, tabs into the grid, hears "button" 80 times with no size, and Enter/Space inserts nothing. The only keyboard route to a table is the fixed 3x3 palette command.

**Location.** `webapp/src/main.js:9748; webapp/editor.html:663`

**Evidence.** The 8x10 loop creates bare `<button class="gc">` with only data-r/data-c; the only listeners are pointermove/pointerleave/pointerdown. Every e2e table test drives it with a synthesized click, so CI stays green while the keyboard path is dead.

**Fix.** Give each cell aria-label "{c} by {r} table" and roving tabindex with arrow navigation, and move insertion from the container's pointerdown onto each cell's click so pointer and keyboard share one path; make #gridPicker role=grid rather than leaving the menu role=menu over non-menuitems.

### HF-033 — Dark theme fails contrast on focused menu rows, review chips and error text

**P1** · css · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** In dark mode the menu row under the keyboard cursor becomes the least legible thing on screen (~2.2:1), the error status line is unreadable, and the tracked-change text plus the Suggesting/Editing mode pill — the control that tells the user whether typing is being recorded — sit around 2.5-3:1.

**Location.** `webapp/src/style.css:20; webapp/src/style.css:25; webapp/src/style.css:3569; webapp/src/style.css:3702`

**Evidence.** --accent #3355c4 and --error #c0392b live in the theme-independent :root; neither dark block redefines them. The review literals #137333/#188038/#b3261e appear in no dark block at all.

**Fix.** Move --accent and --error into the per-theme blocks with lightened dark values, have applySettings write a theme-appropriate derivation of a user-chosen accent instead of the raw swatch, and promote the hardcoded review greens/reds to --review-insert/--review-delete/--review-accent defined in all three palette blocks.

### HF-088 — The comments column has no breakpoint below 860px and swallows the page

**P1** · responsive · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Opening Review at tablet-portrait or phone width leaves about 74px for the document while the comment column covers the viewport, and the page overflows horizontally — the flagship comments feature is unusable there.

**Location.** `webapp/src/style.css:3814; webapp/src/style.css:4104`

**Evidence.** The only rung is 860px (300px sidebar, 316px padding-right); the 900px and 620px blocks never mention the sidebar, and .page-wrap is still up to 96vw so the sheet collides with the column.

**Fix.** Add a rung around 700px that turns the sidebar into a bottom overlay sheet (fixed, full width, max-height 50vh) and resets .viewport.has-review-sidebar .pages padding-right to 0, mirroring how the 900px block relocates the properties and glyph panels.

**Decision.** **Raised by D-2.** The target design has a persistent comments panel, so its narrow-width behaviour is design-required.

### HF-034 — The header "Open" button cannot be focused or activated by keyboard

**P1** · accessibility · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** Tabbing through a freshly loaded editor skips the primary Open button entirely — it is a label wrapping a hidden input — and the File menu that offers Open… is hidden until a document exists. Screen-reader users never encounter the app's main entry point.

**Location.** `webapp/editor.html:72; webapp/editor.html:31`

**Evidence.** `<label class="btn btn-primary file">` wraps a `hidden` file input with no keydown wiring; #appMenuBar sits inside #documentChrome, which is hidden until a document opens. The palette (⌘⇧P → file.open, noDoc:true) is the only working route.

**Fix.** Replace the hidden-input-in-label with a real `<button id="openBtn">` that calls fileEl.click(), move the input off-screen with the existing .sr-only clip pattern plus tabindex=-1, and unhide the File menu before a document is loaded.

### HF-035 — No spelling or grammar checking anywhere — less feedback than a plain `<textarea>`

**P1** · spellcheck · parity · effort L · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** Typos are never flagged. No red underline, no suggestions, no Add to dictionary — and because the page is a canvas, the browser cannot fill the gap either. For a word processor this is the first missing thing a writer notices.

**In opendoc.** No checker at any layer. webapp: the only `spellcheck` tokens are `spellcheck="false"` on chrome inputs (editor.html:38 canvas host, :680, :782, :804, :860, :869, :879, :925, :942, :951, :1083, :1096, :1295) plus `spellcheck="true"` on the alt-text textarea (:907) — so it is not a blanket opt-out, but body text is rastered by paintPageCanvas (main.js:2778) so the browser's native checker cannot reach it either. crates/ DOES contain spelling code, but only OOXML proofing STATE round-trip: `w:noProof` (casual-doc-model/src/v1/properties.rs), `w:proofState` (v1/definitions.rs), importer (casual-doc-import/src/settings.rs), writer (casual-doc-export/src/semantic.rs). docs/01-ORD.md FR-9 already lists "spell/grammar providers" as an SDK requirement.

**In the sibling.** docs-repo/docx-editor/packages/react/src/lib/spellcheck/service.ts:5-22 — lazily streamed Hunspell (nspell + en_US, ~500 KB) singleton behind a Tools toggle, `isMisspelled()` returns false until loaded so first paint is clean; lib/grammar/{rules,service}.ts; components/SpellSuggestionsMenu.tsx (bolded suggestions, Ignore, Add to dictionary, arrow-key nav); components/GrammarSuggestionsMenu.tsx; components/dialogs/DictionaryDialog.tsx; decorations in paged-editor/DecorationLayer.tsx.

**Fix.** Build it as the host-pluggable provider seam FR-9 already promises, not a hardcoded nspell import. Run the checker over the same paragraph text the find pipeline extracts (paragraphTextForFind, main.js:12563), paint squiggles into the existing review-marker overlay pass (paintReviewMarkers, main.js:3221), and hang Suggestions / Ignore / Add to dictionary off the existing context-menu builder (buildContextCommands, main.js:6081 + context_menu.mjs). Ship the dictionary lazily and no-op until loaded, as service.ts:8-13 does. Honour the `w:noProof` run flag the model already carries. Persist the custom dictionary alongside opendoc.settings.

### HF-036 — Printing a mixed-orientation document silently clips the landscape pages

**P2** · import-export · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** A report whose page 1 is portrait and page 4 is landscape (a wide table) prints or exports to PDF with the right 2.5 inches of that page — the last columns — cut off, with no warning.

**Location.** `webapp/src/main.js:2883; webapp/src/main.js:2867`

**Evidence.** printDocument reads only doc.pageSize(0) and buildPrintStyle emits a single @page with margin 0, while each canvas is given its true physical size with no shrink-to-fit.

**Fix.** Emit named @page rules per distinct size with .print-page[data-page-kind] selecting them, or as a guaranteed fallback scale each oversized canvas into the sheet box (max-width/max-height/object-fit: contain) and surface a note.

### HF-037 — ODT to DOCX conversion writes every image as a zero-byte file

**P2** · import-export · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Open an .odt with photographs and export as .docx: every image is destroyed silently, with no entry in the compatibility report — on a headline capability of the format registry.

**Location.** `crates/casual-doc-io/src/odt.rs:101; crates/casual-doc-export/src/semantic.rs:593`

**Evidence.** OdtAdapter::import hardcodes DocumentResources::default() while picture bytes go only into the ODT-private retained side-table; the DOCX writer then hits media.get(...).unwrap_or_default().

**Fix.** Populate DocumentResources in OdtAdapter::import from the media parts the model references (reusing the retained_media_parts read loop) and renormalize part names/content types for the DOCX writer; have write_odt accept request.resources too.

### HF-038 — Copying or format-painting dark-highlighted text silently repaints it yellow

**P2** · wasm · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Text highlighted dark red, dark yellow, dark blue, dark green, dark cyan or dark magenta (routine in imported Word documents) turns YELLOW when copied and pasted internally or picked up with the format painter — a visible, undetectable formatting change.

**Location.** `crates/casual-doc-wasm/src/lib.rs:16040`

**Evidence.** parse_highlight accepts 11 names and falls through to Yellow, while the engine's own highlight_name emits darkblue/darkcyan/darkgreen/darkmagenta/darkred/darkyellow, which round-trip back through setHighlight and pasteRichRuns.

**Fix.** Teach parse_highlight the six dark names so it is the exact inverse of highlight_name, and return Result for genuinely unknown tokens instead of defaulting to Yellow (keep the Yellow default only where the highlighter button means it).

### HF-039 — Find highlights only the current match, so "7 of 23" cannot be answered by looking at the page

**P2** · find-replace · ux · effort M · source: Sibling gap vs opencalc · **Status:** Open

**Symptom.** The panel says "7 of 23" and shows you hit 7. To see where the other 22 are you press Enter twenty-two times, and before Replace All you cannot see what is about to change. Word, Docs and opencalc all tint every hit.

**In opendoc.** The match set is already computed and thrown away: `scanAllMatches()` (webapp/src/main.js:12654) has exactly three callers — :12696 (the count readout), :12760 (Find Next stepping), :12807 (Replace All). `selectTextMatch()` (:12735) sets one selection and calls drawSelection() (:3294). No find-highlight layer exists in style.css and no search highlighting in crates/ (`rg -rni 'search_highlight|find_match|highlightMatches'` → 0).

**In the sibling.** sheets/webapp/editor.core.js:3133-3141 paints every match in the draw pass: "Find highlights: every match on this sheet gets a soft tint, so '3 of 47' is answerable by looking at the sheet rather than by stepping through it. The current match keeps the ordinary selection, which stays distinct."

**Fix.** Paint-layer addition, no new logic: cache the existing scanAllMatches() result on the find state, invalidate on any edit or query change (this also stops the full re-walk on every keystroke), and tint each match in the page paint pass alongside the comment/revision marker layer, leaving the current match as the ordinary selection. Tint the scoped region too when "in selection" is on (findScope, main.js:12636).

### HF-040 — Pasting a table from the web silently drops the sentences around it

**P2** · import-export · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Copy "Intro sentence `<table>`…`</table>` Closing sentence" from a page or Word and paste: the table appears and both sentences are gone, with no warning — the module's own "never silently dropped" contract broken.

**Location.** `webapp/src/clipboard.mjs:182`

**Evidence.** The top-level TEXT branch calls htmlToRuns(node), whose walk iterates childNodes — a text node has none — so it returns [] and runsToParagraphBlocks filters the empty block away.

**Fix.** Emit a paragraph block for a top-level text node (`out.push({ kind: "paragraph", runs: [{ text: node.textContent }] })`) or pass a synthetic wrapper to htmlToRuns; same for text beside a `<ul>`/`<ol>`.

### HF-041 — Multi-valued custom document properties collapse to their last value and change type

**P2** · import-export · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** A SharePoint/DMS property like Reviewers = [Ann, Bo, Cy] imports as the single string "Cy" and exports as a scalar — the other values are gone with no compatibility-report entry at all.

**Location.** `crates/casual-doc-import/src/metadata.rs:367`

**Evidence.** `vt` is a single Option slot overwritten by each nested Start; each `</vt:lpstr>` overwrites pending_value; `</vt:vector>` finds vt None and does nothing. parse_custom takes no Reporter.

**Fix.** Track container nesting in parse_custom: model a CustomValue::Vector { base_type, items } the exporter can re-emit with size/baseType, or at minimum discard the property through a Reporter so it is dispositioned Omitted/NotRetained rather than silently downgraded.

### HF-042 — Charts and ink are dropped even when the file carries a fallback the model supports

**P2** · import-export · bug · effort L · source: Internal audit · **Status:** Open

**Symptom.** A document with a modern chart or ink annotation loses it entirely on import, although the mc:Fallback branch holds a DrawingML/VML equivalent opendoc can represent — Word takes that fallback.

**Location.** `crates/casual-doc-import/src/body.rs:1787`

**Evidence.** The AlternateContent frame treats the first of Choice/Fallback as selected regardless of content; "Requires" appears nowhere in casual-doc-import outside a test fixture, and the body parser dispatches on local name only.

**Fix.** Read mc:Choice/@Requires, resolve the prefixes to namespace URIs through in-scope declarations, and select the first Choice whose namespaces are all supported; otherwise take the Fallback, still reporting the skipped branch.

### HF-043 — Nine hand-rolled dialogs behave differently — Split cell ignores Escape, backdrop clicks and Enter, and leaks focus behind its own modal

**P2** · dialogs · ux · effort M · source: Sibling gap vs opencalc · **Status:** Open · related: HF-062, HF-063, HF-070

**Symptom.** You open Split cell, press Escape — the universal cancel — and nothing happens. Tab walks you out of the dialog into the ribbon behind the dimmed backdrop while the dialog still claims to be modal, and Enter in the number field does not confirm. Every dialog behaves slightly differently, so there is no rule to learn.

**In opendoc.** Nine aria-modal dialogs (webapp/editor.html:94, 531, 746, 769, 788, 821, 846, 893, 922) and no shared primitive. `trapModalFocus` (helper at main.js:14569, syncModalLock at :14562) is wired at only six sites (:4659, 11433, 11734, 12400, 14630, 14849) and backdrop handlers exist at seven (:551, 4651, 11425, 11726, 12388, 14622, 14841). #splitCellDialog's entire implementation is main.js:9489-9521 — no keydown listener, no backdrop check, no trapModalFocus, no syncModalLock, no Enter-to-confirm, and there is no generic document-level Escape handler for .dialog-overlay. #styleNameDialog (:546-561) has Escape only on the input, so Escape stops working once you Tab to a button.

**In the sibling.** sheets/webapp/editor.dialogs.js:2694-2785 is the house modal contract, stated at :2701: "Cancel, ✕, backdrop and Escape are one path." It also implements Tab cycling to keep the aria-modal promise (:2749-2778 — nothing was keeping it and Tab walked out into the toolbar behind the backdrop), Enter as default button except on Cancel (:2766-2770), a stale-handler guard (:2757-2759), and inline refusal that keeps the typed text (:2705-2715). `confirmModal()` at :2028 gives the whole app one promise-based confirm.

**Fix.** Extract `openModal({title, body, onConfirm})` into webapp/src/dialog.mjs implementing the editor.dialogs.js contract once — Escape/backdrop/✕/Cancel as one path, Tab cycling over visible enabled focusables, Enter as default unless Cancel holds focus, focus restored to the invoking control, syncModalLock(), inline validation that keeps typed text — then migrate all nine. The load-bearing half is the Playwright spec asserting Escape, backdrop click and Tab containment for EVERY element carrying aria-modal; webapp/tests/e2e has no dialog-dismissal spec today, so a new dialog can still ship without them.

### HF-044 — An unreadable image is exported as a zero-byte part with no loss reported

**P2** · import-export · bug · effort S · source: Internal audit · **Status:** Fixed (#497) · related: HF-047

**Symptom.** Import a file with one corrupt or over-size image part and save: the package advertises an image it cannot supply, Word draws a broken-image box, and the export reports no loss at all — the user only finds out on reopening.

**Location.** `crates/casual-doc-io/src/docx.rs:90; crates/casual-doc-export/src/semantic.rs:593`

**Evidence.** docx.rs uses `if let Ok(bytes)` with no else arm and no reporter; semantic.rs writes media.get(...).cloned().unwrap_or_default() alongside a valid /image relationship and content-type Default.

**Fix.** Record a CompatibilityEntry (Omitted/NotRetained, part name in FeatureLocation) for each unreadable media part at read time, and either error or emit no part + no relationship when bytes are absent rather than an empty part.

### HF-045 — A failed edit or undo leaves the document half-changed and can lose the undo step

**P2** · rust-core · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** An operation that reports failure can still have deleted characters (no inverse recorded, nothing to undo), and a failed undo/redo pops the history entry first — so the document is half-reverted and the next Ctrl+Z reverts the previous action on top of it, with no way back.

**Location.** `crates/casual-doc-edit/src/lib.rs:901; crates/casual-doc-wasm/src/lib.rs:8203; crates/casual-doc-wasm/src/lib.rs:8224`

**Evidence.** DeleteText clones `old` then runs ensure_run_boundary/remove_covered_range with `?` and never restores; remove_covered_in removes inlines as it scans before erroring on a straddling atomic leaf. undo()/redo() pop first and call apply_group with no snapshot, while apply_action_as does snapshot for multi-op actions.

**Fix.** Give DeleteText/FormatText/ClearFormatting/InsertNote a real rollback (compute into a clone and commit on success, as SetHyperlink does) plus reject_partial_atomic before any mutation; snapshot before undo/redo and only pop the entry after a successful apply, and give apply_group its own snapshot/restore.

### HF-046 — The paste-options chip undoes an unrelated edit if you press Cmd+Z first

**P2** · clipboard · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Type a sentence, paste, press Cmd+Z to drop the paste (the chip stays visible), then click "Keep text only" — the typed sentence is silently deleted and replaced by the clipboard text, and redo cannot cleanly recover it.

**Location.** `webapp/src/main.js:13553; webapp/src/main.js:13565; webapp/src/main.js:13607`

**Evidence.** Both switch handlers do `if (doc.canUndo) await runEdit(() => doc.undo())` with no check that the paste is still on top; the dismissal keydown filter explicitly skips modifier combos and hidePasteOptions is never called from runEdit/applyEditResult.

**Fix.** Capture the undo-stack identity when the chip is offered and refuse to act (hiding the chip) if it no longer matches; drop the modifier exclusion so Cmd+Z/Cmd+Y dismiss the chip like any other edit.

### HF-047 — Import/export data loss is reported as a bare number — the report naming what was lost is parsed and discarded

**P2** · error-handling · ux · effort M · source: Sibling gap vs opencalc · **Status:** Open · related: HF-044, HF-037

**Symptom.** You save and are told "12 compatibility findings" with no way to learn what the 12 are, whether any matters, or which part of your document changed. On open you are told nothing at all, even though the engine already wrote the report.

**In opendoc.** webapp/src/format_io.mjs:41-52 `compatibilityOccurrenceCount()` parses `report.entries` — the per-finding records — and reduces them to an integer, discarding every entry. main.js:13004-13014 puts that integer into a chip whose `title` is the same integer; editor.html:78 confirms it is a bare `<span role="status">`, not a button. Called on every save (:13057/13067). The import side is worse: crates/casual-doc-wasm/src/lib.rs:8262-8265 exposes an `importReportJson` getter (populated at :15803/15829) with ZERO references anywhere in webapp/. docs/35-DISPOSITION-TAXONOMY.md exists precisely to name these findings.

**In the sibling.** sheets/webapp/editor.dialogs.js:2075-2087 `reportImportIssues()` pulls `wasm.session_import_summary()` and appends the actual summary text to the status bar as a .warn span, deliberately via textContent because the summary quotes names out of an untrusted file. The rationale at editor.core.js:6803-6804: "Anything the importer had to drop or degrade, said once, plainly. The report exists in the engine; nothing surfaced it, so a lossy import looked clean."

**Fix.** Make the chip a button opening a report panel that lists `entries` grouped by disposition with occurrence counts, rendered via textContent (entries quote strings from an untrusted file). Wire the same panel to the import side by reading the existing `importReportJson` getter — no Rust work needed on either half.

### HF-048 — Changing underline style or double-strike leaves the page showing the old decoration

**P2** · layout · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Switching an already-underlined range to double underline, changing underline colour, or striking already-struck text appears to do nothing on screen — yet the change is real and lands in the exported file, so what the user sees and what they save disagree.

**Location.** `crates/casual-doc-layout/src/flow.rs:778; crates/casual-doc-layout/src/flow.rs:836`

**Evidence.** The hash covers underline and strikethrough only; all three omitted fields are consumed by the renderer and carried inside the cached fragment, and finish_edit passes an empty DirtySet so the hash is the only invalidation.

**Fix.** Hash underline_style, underline_color and double_strike in both the Run and Field arms of paragraph_hash. Add a regression test that mutates only underline_style and asserts shaped_last_build() == 1 — and prove it goes red.

### HF-049 — A comment anchored in a header or footnote can never be deleted

**P2** · wasm · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Comment on text in a page header, then try to delete the thread: it is refused with "That edit isn't supported for this selection yet" — an error unrelated to the cause — and the comment is permanently stuck in the document.

**Location.** `crates/casual-doc-wasm/src/lib.rs:6605`

**Evidence.** add_comment resolves via find_paragraph_any so markers land in any surface, but delete_comment strips only document.body(); the surviving reference trips DanglingCommentRef, rolling the edit back as EditError::ValueTooLarge.

**Fix.** Strip comment markers across every surface (surface_block_lists) and emit one UpdateReviewState carrying every changed paragraph. Add a test that comments in a header and asserts delete succeeds.

### HF-050 — A footnote inserted outside the body can never be undone or removed

**P2** · rust-core · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Insert a footnote with the caret in a text box or header, press Ctrl+Z: undo reports failure, the note stays in the document forever, and the history entry is consumed so it can never be undone again.

**Location.** `crates/casual-doc-edit/src/lib.rs:2032`

**Evidence.** InsertNote resolves through blocks_owning_mut (any surface) while its inverse calls remove_note_reference(doc.body_mut(), ..), whose walk handles only body-level Paragraph/Table/Sdt and returns None.

**Fix.** Mirror RemoveField — locate the reference via surface_block_lists and mutate through blocks_owning_mut, teaching the walk to descend into text-box inlines the way find_paragraph_in_inlines_mut does.

### HF-052 — Bookmarks in headers, footers and notes cannot be created or deleted, and report "invalid name"

**P2** · rust-core · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** A bookmark on a header selection refuses to be created, and an imported bookmark anchored in a header is listed in the bookmark manager but can never be deleted — both failing with a name error that has nothing to do with the real cause.

**Location.** `crates/casual-doc-edit/src/lib.rs:1616; crates/casual-doc-edit/src/lib.rs:1663`

**Evidence.** CreateBookmark pre-checks all-surface then inserts into body_mut; DeleteBookmark locates markers across surfaces but removes them from the body only, and the DanglingBookmarkRef validation turns the no-op into InvalidName.

**Fix.** Use blocks_owning_mut for the marker insert, removal and rollback so the surface that find_paragraph_any/locate_bookmark_markers found is the one mutated, and add a distinct EditError for an unsupported surface instead of reusing InvalidName.

### HF-097 — Tools and Help scroll out of the menu bar behind a hidden scrollbar

**P2** · responsive · ux · effort M · source: Internal audit · **Status:** Open

**Symptom.** Below about 500px wide the last two menus disappear with no fade, no chevron and no visible scrollbar, and a mouse user cannot scroll a horizontal-only container — so those menus become undiscoverable.

**Location.** `webapp/src/style.css:345`

**Evidence.** .app-menu-bar is overflow-x:auto with scrollbar-width:none and a display:none webkit scrollbar, inside a max-width:500px chrome container with flex:0 0 auto buttons; updateRibbonOverflow operates only on .rgroup elements and no media query touches the bar.

**Fix.** Give the menu bar the ribbon's treatment: measure and collapse trailing menus into a "..." overflow button, or at minimum add a right-edge fade, a visible scroll affordance, and wheel-to-horizontal handling.

**Decision.** **Raised by D-2.**

### HF-053 — Toolbar formatting state is wrong for a caret in a header, footer or note — so Bold toggles the wrong way

**P2** · rust-core · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Put the caret in bold 14pt red footer text: the toolbar shows Bold off and default size/colour, so pressing Bold bolds already-bold text instead of unbolding it, and "Update style to match selection" captures the wrong formatting.

**Location.** `crates/casual-doc-edit/src/lib.rs:3523`

**Evidence.** Line 3523 uses find_paragraph(document.body(), node) while the sibling run_properties_in_range was explicitly fixed to find_paragraph_any and carries a comment describing this defect; the fallback is RunProperties::default().

**Fix.** Use find_paragraph_any in caret_run_properties, matching run_properties_in_range and paragraph_properties. Add a test placing a caret in a directly-bolded footer run.

### HF-054 — Pasting from Word or a web page inserts a blank paragraph before the content

**P2** · import-export · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Every paste whose HTML is wrapped in a div — Word and most web-page copies — pushes the caret down a line and leaves an empty paragraph the user has to delete each time.

**Location.** `webapp/src/clipboard.mjs:116`

**Evidence.** Reproduced: a WordSection1 div yields a leading {paragraphBreak}; DIV is in BLOCK_TAGS and entering the container sets sawParagraph without contributing text. paste_rich_runs emits a SplitParagraph for every marker unconditionally.

**Fix.** Emit a paragraphBreak only when a block actually contributed content (track text pushed since the last break) instead of setting a flag on container entry, and collapse whitespace-only text nodes between blocks.

### HF-055 — Smart quotes insert the wrong glyph after any non-ASCII character

**P2** · webapp-js · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Typing an apostrophe after "Müller", "café", or any Cyrillic/CJK word produces an opening quote instead of an apostrophe — auto-correct visibly damages punctuation in every non-English document, and nested quotes degrade too.

**Location.** `webapp/src/main.js:13679`

**Evidence.** copyText(node, offset-1, node, offset) hits slice_bytes, whose clamp snaps a non-boundary index forward, so a >= b and it returns "" — which the caller treats as start-of-paragraph and emits the opening quote.

**Fix.** Stop guessing the previous byte: read doc.copyText(node, 0, node, offset) and take the last code point, or expose the engine's prev_char_boundary helper as a WASM entry point.

### HF-056 — Images cannot be rotated or flipped — a sideways phone photo has to be fixed outside the editor

**P2** · images · parity · effort L · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** A photo inserted sideways — the commonest problem with phone-camera images — cannot be corrected in the editor; you leave, fix it elsewhere and re-insert. Given that opendoc already has drag, resize handles and direct-manipulation crop, the missing rotate reads as a bug rather than a scope decision.

**In opendoc.** No rotate/flip anywhere in the UI: the object context bar (webapp/src/main.js:3905-3930) offers Alt text, Properties, shape fill/outline, crop, wrap and delete, and the object inspector (:3801) exposes Position/Size/Wrap/TextBox body/Accessibility/Appearance. `rg -ni 'rotate|flip'` over main.js + editor.html returns only unrelated matches (:6639 submenu flip, :7574 boolean). The engine models rotation and flip_h/flip_v for import and render only (casual-doc-wasm/src/lib.rs:14841-14973; casual-doc-edit/src/lib.rs:5351+) — there is no rotate/flip EDIT operation and no wasm binding (js_name list has setObjectExtent/resizeObject/setObjectWrap/setImageCrop and nothing rotational).

**In the sibling.** docs-repo/docx-editor/packages/react/src/components/ui/ImageTransformDropdown.tsx:13-26 — an icon-grid dropdown offering rotateCW, rotateCCW, flipH, flipV, alongside ImageWrapDropdown.tsx and dialogs/ImagePositionDialog.tsx.

**Fix.** Not a toolbar-button job. Add a rotate/flip operation to the closed op set (invariant I2, docs/45) WITH its inverse so undo works, expose a wasm binding, then surface rotate-90-CW/CCW and flip-H/V on the object context bar (main.js:3918) and the object context menu (buildObjectContextCommands, :6323), routed through runEdit so they are tracked. The Word-standard free-rotation handle above the top-center resize handle is the follow-on; the four discrete transforms cover the common case and match the reference.

### HF-057 — Object properties panel shows stale geometry and Apply reverts a drag-resize

**P2** · editor-ux · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** With Properties open, resizing an image by dragging leaves the panel showing the old numbers — nudging or clicking Apply snaps the object back. Selecting a different object leaves the previous object's numbers, alt text and wrap in the panel, so Apply writes one object's geometry onto another.

**Location.** `webapp/src/main.js:3748; webapp/src/main.js:3805; webapp/src/main.js:3859`

**Evidence.** toggleObjectInspector populates the fields only on the opening call; updateObjectContextBar (run on every repaint) never refreshes them; the Apply handler reads all four fields and calls doc.resizeObject.

**Fix.** Extract the field-population block into reflectObjectInspector() and call it from updateObjectContextBar and from finishObjectResize/finishObjectMove/nudgeSelectedObject whenever the panel is open, skipping any focused input.

### HF-058 — The object action bar stays frozen on screen while the object scrolls away

**P2** · layout · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Select an image, scroll, and the "Image · wrap · Alt text · Crop · Delete" bar stays at its original screen position over unrelated paragraphs or the ribbon — and its Delete button still destroys the object the user can no longer see. Same after a window resize or zoom change.

**Location.** `webapp/src/main.js:3954`

**Evidence.** The bar is body-level with fixed viewport coordinates and updateObjectContextBar runs only from drawSelection; none of the five viewport scroll listeners touches it and the page IntersectionObserver never calls drawSelection.

**Fix.** Add passive scroll and resize listeners that call updateObjectContextBar, and hide the bar when the object's rect leaves the viewport — matching the existing linkChip and selToolbar behaviour.

### HF-059 — Cmd+V never pastes an image, and says nothing

**P2** · clipboard · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Take a screenshot, click into the document, press Cmd+V — nothing happens and no message appears. Right-click ▸ Paste does not exist either (the custom menu owns right-click), so image paste has no standard gesture. In Safari/Firefox the same interception turns every paste into a permission prompt.

**Location.** `webapp/src/main.js:13824; webapp/src/main.js:13461`

**Evidence.** The Cmd+V keydown cancels the browser ClipboardEvent and routes to the async path, whose loop skips any item without text/html; the insertImageFromBlob branch exists only in the event.clipboardData path. ⌘⇧V has the same silent no-op.

**Fix.** Stop preventDefault-ing the Cmd+V keydown and let the native paste event (already handled) do the work, keeping the async path for the menu/palette. If interception must stay, add the image branch to the async path and setStatus an error when nothing pasteable was found.

### HF-061 — A zero-height table row paints its text across the rest of the page

**P2** · render · ui · effort S · source: Internal audit · **Status:** Open

**Symptom.** A row authored with an exact height of 0 — which Word collapses to nothing — instead paints its full paragraph on top of every row below it and the body text, garbling the page.

**Location.** `crates/casual-doc-render/src/lib.rs:223`

**Evidence.** resolve_row_height returns (Twip(0), clip=true) for hRule=exact val=0; rect_path fails for a zero-height rect so PushClip pushes nothing when the clip stack is empty, and the cell's blocks paint unclipped.

**Fix.** When build_clip_mask returns None, push an empty mask (clip everything) rather than skipping the push, in both the parented and unparented branches — a zero-area clip must clip everything, not nothing.

### HF-062 — Split cell dialog cannot be dismissed by keyboard, and closing it kills typing

**P2** · editor-ux · a11y · effort S · source: Internal audit · **Status:** Open · related: HF-043

**Symptom.** Escape, Enter and clicking the backdrop all do nothing in the Split cell modal, and Tab walks out behind the scrim into the ribbon the dialog claims is inert. Cancel or Split then drops focus onto a hidden button, so the editor appears frozen — every keystroke is ignored until the user clicks the page.

**Location.** `webapp/src/main.js:9489; webapp/src/main.js:9496; webapp/editor.html:746`

**Evidence.** Only Close/Cancel/Confirm click handlers are registered — no keydown, no overlay mousedown — while peer dialogs all use trapModalFocus; splitCellBtn lives in a hidden contextual ribbon panel so .focus() is a no-op and activeElement falls back to body.

**Fix.** Wrap the fields in a form so Enter submits; add the Escape + trapModalFocus + backdrop-mousedown block every peer dialog already uses and include every .dialog-overlay in syncModalLock; on close route focus to focusEditorSurface() rather than a possibly-hidden ribbon button.

### HF-063 — Cmd+F inside a modal steals focus out of the dialog and opens Find behind it

**P2** · editor-ux · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Typing in the Document properties dialog and pressing Cmd+F opens the Find panel behind the modal overlay and moves focus into it — the user cannot see where their keystrokes are going and the modal's focus trap is defeated.

**Location.** `webapp/src/main.js:12880`

**Evidence.** The Cmd/Ctrl+F handler is registered on document with no interactive-target guard and no modal check; trapModalFocus only reflects Tab when activeElement is the modal's first or last focusable element, so it never fires once focus has left.

**Fix.** Guard the handler like Cmd+K does (`if (isInteractiveChromeTarget(e.target)) return;`) and no-op while any modal is open, extending syncModalLock to cover the alt-text, link, bookmark and field dialogs too.

### HF-064 — No accessibility checker — opendoc makes the editor accessible but never audits the document being written

**P2** · a11y · a11y · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** You can publish a document with every image undescribed and headings jumping H1→H4 and get no signal. Alt text is per-object and opt-in, so finding the images that lack it means clicking each one — which is exactly the images nobody clicked.

**In opendoc.** The reading side exists — buildAccessibilityTree() (webapp/src/main.js:9793) builds an off-screen structural mirror from accessibilityTree(), and alt text is now editable from TWO surfaces (object context bar :3918 and the object inspector's Accessibility fieldset :3801). The auditing side does not: `rg -ni 'checkAccessibility|missing alt|heading order|accessibility check'` over main.js → 0, and APP_MENU_SECTIONS.tools (:10963) is only pageSetup/paragraph/smartQuotes/properties/settings. Every ingredient is present: doc.objectDescr (:3761/4578) and navigateToNode (:9919).

**In the sibling.** docs-repo/docx-editor/packages/react/src/components/dialogs/AccessibilityDialog.tsx:5-30 — Tools ▸ Accessibility lists every issue checkAccessibility finds (images missing alt text, heading-order jumps), each row with a "Go to" that moves the caret to the offending element and closes the dialog.

**Fix.** Assembly of existing pieces: walk the same projection buildAccessibilityTree consumes, collect two issue classes (objects whose doc.objectDescr is empty, and heading-level jumps), render them in a Tools ▸ Accessibility dialog on the shared .dialog-overlay shell with a "Go to" per row reusing navigateToNode, and surface a live count beside the compatibility chip from HF-047.

### HF-065 — Re-opening the same file does nothing, and file read errors are completely silent

**P2** · editor-ux · ux · effort S · source: Internal audit · **Status:** Open

**Symptom.** Choosing the same file again to discard changes and reload the pristine copy silently does nothing. Dropping a file that fails to read (moved, permission denied, too large) also does nothing at all — no status, no error — because the read rejection is never handled.

**Location.** `webapp/src/main.js:14198; webapp/src/main.js:14324; webapp/src/main.js:14185`

**Evidence.** fileEl.value is never reset anywhere, so no change event fires for an identical path; both listeners discard handleFile's promise and `await file.arrayBuffer()` sits outside any try/catch; the 64 MiB bound is applied only inside open(bytes).

**Fix.** Reset fileEl.value after each change, pre-check file.size against the engine's admission limit (exported or mirrored) with a clear "opens files up to 64 MB" message, and wrap the handleFile call in .catch(setStatus(..., "error")) on both the picker and drop listeners.

### HF-066 — Pasted hyperlinks are stored with no scheme filter and re-exported

**P2** · security · security · effort S · source: Internal audit · **Status:** Open

**Symptom.** Copying a paragraph from a malicious page brings a javascript:/data: link into the document, which is then written into the exported .docx and handed to whatever app the user pastes it into next — and it is executable through the context-menu Open link path.

**Location.** `webapp/src/clipboard.mjs:131; webapp/src/main.js:12310`

**Evidence.** htmlToRuns takes getAttribute("href") verbatim in the module that documents itself as sanitizing; applyLinkDialog rejects only an empty target; no scheme predicate exists at any ingestion point.

**Fix.** Add one scheme allowlist at the ingestion choke point: in htmlToRuns resolve the href and keep it only for http/https/mailto or a same-document #anchor (keeping the text otherwise), and apply the same predicate in applyLinkDialog.

### HF-067 — The menu bar has no visible focus indicator — keyboard navigation is blind

**P2** · a11y · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** Arrowing across File/Edit/View and down through their items produces a background change of about 1.1:1 — invisible on a laptop screen and identical to hover, so a keyboard user cannot tell where they are on the app's primary command surface.

**Location.** `webapp/src/style.css:367; webapp/src/style.css:523`

**Evidence.** Both rules group :hover with :focus-visible and set `outline: none`; --bg-2 on the white/near-black --surface parent computes to 1.13:1, and style.css has no global :focus-visible fallback.

**Fix.** Split hover from focus-visible on both rules, keep --bg-2 for hover and give focus-visible the project's existing ring (outline: 2px solid var(--accent); outline-offset: 1px) or the accent-soft treatment .menu-item:focus-visible already uses. A global :focus-visible fallback would cover the other 20+ outline:none sites.

### HF-068 — No version history — the document has no past that survives a reload

**P2** · versions · parity · effort L · source: Sibling gap vs opencalc + docs · **Status:** Open

**Symptom.** "Get back the version from before I restructured chapter 3" is unanswerable. Ctrl+Z is the only recovery mechanism and it does not survive a reload.

**In opendoc.** Absent: `rg -ni 'version history|versionHistory|restoreVersion|namedVersion|snapshot'` over webapp/ and crates/ hits only main.js:7940 and :13238, both about formatting/keystroke coalescing; no versions module among webapp/src's eight files; no version spec among 93 e2e specs; the File menu (main.js:10926-10932) has no entry. docs/71 — the only "history" design in the repo — is undo/redo action labels, not snapshots. The only history is the in-memory undo stack, which dies with the page.

**In the sibling.** sheets/webapp/editor.versions.js (195 lines) — a reload-surviving IndexedDB version store, "A history you lose by pressing F5 is not a history", compressed via CompressionStream off the main thread (17.82 MiB → 1.61 MiB measured), meta/bytes split so opening the panel costs kilobytes; persistVersions/loadVersions/forgetVersions; the panel at editor.core.js:5245-5265 renders when / label(Saved|Named|Autosave) / size / Restore plus a byte-budget line. Gated by tests/browser/editor.version-history.spec.mjs and editor.version-persistence.spec.mjs. docs-ref: version-history/store.ts, useVersionHistoryCapture.ts, useLiveVersionList.ts, versionDiff.ts and components/sidebar/VersionHistoryPanel.tsx (day-grouped timeline, pinned Current version, per-row preview/rename/restore/delete, changes-vs-previous diff).

**Fix.** BLOCKED ON HF-011 — build on the same IndexedDB seam, not a second store. Capture on explicit Save, on a named "Keep this version" action, and on a slow tick; store metadata and gzipped bytes as separate rows (CompressionStream off the main thread rather than compressing in wasm); retain against a byte budget, not a count; surface as File ▸ Version history with the opencalc row shape (timestamp, kind label, size, Restore) reusing the existing right-panel pattern (main.js:9930/10025/10545), and make Restore go through the same confirm as a destructive open. No Rust needed — exportAs("org.casualoffice.normalized-json") is already wired (:10641). Scope v1 to preview + restore; the word-level diff can follow using the paragraph walk find already does (:12563).

**Decision.** **Unblocked by D-1.** Same store as HF-011 — not a second one. The target design shows a Saved/Synced state, so this surface is design-required.

### HF-069 — Toolbar and menu commands fire on mouse-down, so a mis-press cannot be aborted

**P2** · editor-ux · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** Pressing on "Clear direct formatting", Delete row, or Reject all and dragging away before releasing still runs the command — there is no way to back out of a mis-press, which matters most for users with tremor or imprecise pointing.

**Location.** `webapp/src/main.js:7863`

**Evidence.** onButton runs handler() from mousedown and the click listener early-returns unless e.detail === 0; 44 call sites plus every registerPopover trigger route through it. WCAG 2.5.2 is Level A and down-event completion is not essential here.

**Fix.** Keep the mousedown listener for its preventDefault (that is what preserves the document selection) and move handler() to an unconditional click listener, dropping the e.detail !== 0 early return. Regression-pass the 44 call sites.

### HF-070 — Ribbon popovers and Settings never take focus, and closing them loses the user's place

**P2** · editor-ux · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** Opening the colour, highlight, bullet, spacing, table-style, shape or zoom menus by keyboard leaves focus on the trigger, so reaching the swatches means tabbing through the rest of the ribbon; pressing Escape while inside collapses focus to the top of the page. Settings behaves the same way, and its Theme group announces "radio group" over three toggle buttons where arrow keys do nothing.

**Location.** `webapp/src/main.js:8222; webapp/src/main.js:14431; webapp/editor.html:160`

**Evidence.** closePopover only sets hidden/aria-expanded and openPopover only positions; both are declared role=dialog yet never focused. toggleSettings flips hidden with no focus in or restore, and #themeSeg is a radiogroup of aria-pressed buttons.

**Fix.** Focus the first (or checked) descendant in openPopover and restore to p.btn on close, reusing the existing closeAppMenu({ restoreFocus: true }) pattern and toggleProperties' return-focus check for Settings; give the theme buttons role=radio/aria-checked with roving tabindex, or downgrade the group to role=group.

### HF-071 — The accessibility mirror is rebuilt wholesale on every edit, resetting the screen reader to the top

**P2** · accessibility · a11y · effort L · source: Internal audit · **Status:** Open

**Symptom.** A screen-reader user browsing a 60-page report loses their place and jumps back to the top of the document on every committed edit, and the same rebuild puts a whole-document DOM re-creation on the typing path for large files.

**Location.** `webapp/src/main.js:2341; webapp/src/main.js:9873`

**Evidence.** chromeRefreshA11y ||= a11y || outline couples the rebuild to every outline refresh, and applyEditResult schedules one for every edit; buildAccessibilityTree re-parses the whole document and ends in replaceChildren on a live role=document region.

**Fix.** Patch only changed blocks using the dirtyPages the engine already reports (or key nodes by paragraph id and reuse elements); at minimum decouple the a11y rebuild from the outline refresh and skip it while a typing session is open.

### HF-072 — The floating selection toolbar never shows Bold/Italic/Underline state, so clicking B un-bolds

**P2** · editor-ux · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Select already-bold text: the distant ribbon B lights up, but the bar floating directly over the selection — the one the user is looking at — shows neutral, so clicking B removes bold instead of applying it. Screen readers hear no pressed state at all.

**Location.** `webapp/editor.html:960; webapp/src/main.js:7617`

**Evidence.** The four .fmt[data-fmt] buttons have aria-label and no aria-pressed; the only JS touching them is the toggleFormat wiring, and the state-sync loop writes aria-pressed to the ribbon ids only — leaving `.sel-toolbar .fmt[aria-pressed="true"]` as dead CSS.

**Fix.** Add aria-pressed to the four .sel-toolbar buttons and fan the existing fmtButtons sync (including the mixed case) out to every button carrying that format key.

### HF-073 — No recent documents — the only way back into yesterday's file is the OS file picker

**P2** · file · ux · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** Reopening yesterday's document means navigating the OS file dialog from scratch every session; the editor remembers nothing about what you have been working on.

**In opendoc.** The only "recent" machinery is the session-remembered colour swatches (webapp/src/main.js:8316-8320, 8378, 8409 — recentTextColors / recentHighlights / recentUnderlineColors). The entire open path is the hidden `<input id="file" hidden>` (editor.html:73) plus boot()'s startup loader (main.js:2407-2424), which only ever loads sample.docx, a demo preset or an e2e fixture. opendoc writes only to localStorage today — there is no IndexedDB of any kind.

**In the sibling.** docs-repo/docx-editor/packages/react/src/utils/recent-files.ts:1-45 — an IndexedDB recent-files store, MAX_ENTRIES 10, STALE_AFTER_MS 60 days, holding name + buffer + size + openedAt so the Home screen reopens in one click; explicitly distinguished from autosave (single-slot crash recovery) and version history (per-doc timeline): "this is 'what docs have I been working with lately, across sessions'."

**Fix.** Downstream of HF-011: add a `recent` store to the same IndexedDB (name, buffer, size, openedAt; cap 10, prune past 60 days on record, per recent-files.ts:20-45), record on every successful open and download, and surface a "Recent" section on the empty state and in the File menu. Carries the same retention decision as HF-011 — full document buffers held on-device for 60 days needs an explicit clear/disable control.

**Decision.** **Unblocked by D-1.** Same store as HF-011.

### HF-074 — No skip link: reaching the document means tabbing past ~150 chrome controls

**P2** · accessibility · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** After using the ribbon or returning to the page, a keyboard user must Tab through Search, Save, Properties, Settings, eight menus, five ribbon tabs and the entire Home band to get the caret back into the document; a screen-reader user re-hears all of it.

**Location.** `webapp/editor.html:25; webapp/_partials/site-header.html:1`

**Evidence.** body opens straight into the header bar; a repo-wide grep for "skip" matches only test files. #pages is already tabindex=0. WCAG 2.4.1 Level A on all four pages.

**Fix.** Add `<a class="skip-link" href="#pages">Skip to the document</a>` as the first child of body (and href="#main" in the site header partial) using the existing .sr-only clip pattern plus a :focus rule that brings it on screen.

### HF-075 — Colour pickers for accent and table borders have no accessible name

**P2** · a11y · a11y · effort S · source: Internal audit · **Status:** Open

**Symptom.** Screen-reader users hear an unnamed "colour picker" for the custom accent, and in the borders popover the cell-border and table-border pickers sit side by side and are indistinguishable by ear.

**Location.** `webapp/editor.html:175; webapp/editor.html:649; webapp/editor.html:659`

**Evidence.** Each type=color input is wrapped in a label whose only content is the input and whose only annotation is title; no aria-label is set at runtime. The sibling swatch buttons in the same group do carry aria-label.

**Fix.** Put aria-label on each input ("Custom accent color", "Cell border color", "Table border color") — the title on the wrapping label contributes nothing to the control's name.

### HF-076 — Right-click menu is missing Paste-without-formatting and Select all; checklist missing from menus

**P2** · parity · parity · effort M · source: Internal audit · **Status:** Open

**Symptom.** Right-clicking after copying styled text offers no "Paste without formatting" — a top-5 action in both Word and Docs — and no Select all. A user browsing the Format menu or the right-click list submenu sees only Bulleted and Numbered and concludes the editor has no checklists; list restart/continue are likewise menu-invisible.

**Location.** `webapp/src/main.js:6100; webapp/src/main.js:10959; webapp/src/main.js:6215`

**Evidence.** contextMenu:true is declared on seven commands and never read; buildContextCommands hand-picks edit.cut/copy/paste only. The checklist command exists on the ribbon (#checkList) and in the palette but in neither menu.

**Fix.** Make the declared contextMenu flag load-bearing (derive the clipboard group from editorCommands(context).filter(c => c.contextMenu)), add paragraph.list.checklist plus restart/continue to the Format section and the list submenu, and add the SURFACE-table parity test asserting contextMenu:true ↔ built menu in both directions.

### HF-077 — Opening a heavy document freezes the tab with no budget, no progress and no cancel

**P2** · architecture · ux · effort L · source: Sibling gap vs opencalc · **Status:** Open · related: HF-078, HF-079, HF-080, HF-110

**Symptom.** A 300-page document that is inside every size limit but simply heavy locks the tab on a frozen "Opening…", with Chrome's kill-page dialog as the only exit.

**In opendoc.** No deadline anywhere: `rg 'time_budget|timeBudget|budget|deadline'` over webapp/src/main.js and crates/casual-doc-wasm/src/lib.rs → 0. The only admission control is the static viewer_limits() at crates/casual-doc-wasm/src/lib.rs:83-90 (64 MiB input, 256 MiB expanded). openBytes (main.js:2490-2606) calls `open(bytes)` synchronously on the main thread; status reads "Opening …" at :2492 and the thread is gone. No cancellation spec among the 93 e2e specs.

**In the sibling.** sheets/webapp/editor.sheets.js:1064-1070 sets `wasm.session_set_time_budget_ms(...)` around the open and clears it in a finally; :1090-1093 then calls `offerKeepWaiting("opening …", () => openBytes(raw, name, -1))` so the limit is an offer, not a refusal. tests/browser/editor.cancellation.spec.mjs:1-16 names the scenario: "a workbook inside every limit and simply enormous holding the only thread the browser has until it finishes… A capability can be fully built, fully tested through the SDK, and reach nobody."

**Fix.** Thread a wall-clock budget through the wasm open/relayout entry points (the layout crate already walks in bounded loops, so a cancel check at the page boundary is cheap), surface it as a status-bar offer with "Keep waiting" that re-runs unbounded, and add the cancellation e2e. This needs new cancellation plumbing in the layout crate, so it is not a webapp-only change. Longer term it is the argument for moving the engine to a Worker (README.md:178, not started).

### HF-078 — Images are re-decoded from source bytes on every page repaint

**P2** · perf · perf · effort M · source: Internal audit · **Status:** Open

**Symptom.** Typing on a page holding a few large photos stalls for hundreds of milliseconds per character while the images are decoded again, and scrolling re-decodes every image each time a page re-enters the viewport.

**Location.** `crates/casual-doc-render/src/lib.rs:326`

**Evidence.** decode_to_pixmap runs a full ImageReader::decode + to_rgba8 + per-pixel premultiply on every paint; there is no decoded-image cache in the render crate or in casual-doc-wasm, and repaintPage re-calls renderPage per edit.

**Fix.** Add a bounded decoded-image cache keyed by (MediaId, source hash) holding premultiplied pixmaps with an LRU byte budget — the "Aggregate decoded image cache" docs/21 already reserves — invalidated only when the media bytes change.

### HF-079 — The incremental galley cache is inert for every imported document

**P2** · perf · perf · effort L · source: Internal audit · **Status:** Open

**Symptom.** Typing one character in any real .docx re-shapes every paragraph in the document, so keystroke latency grows with document length — the exact cost the incremental layout work was built to remove.

**Location.** `crates/casual-doc-layout/src/document_layout.rs:1017`

**Evidence.** build_section_runs_cached bails to the uncached builder whenever definitions().sections is non-empty, and every Word-produced document ends w:body with a w:sectPr, so sections is non-empty for essentially every imported file.

**Fix.** Make the cached builder section-aware (per-section build keyed on (section_id, flow_width)); at minimum take the fast path when there is a single trivial body-level section, which covers the overwhelming majority of real files.

### HF-080 — Every pointer move and caret query flattens the whole document's lines

**P2** · perf · perf · effort M · source: Internal audit · **Status:** Open

**Symptom.** Drag-selecting in a 500-page document allocates roughly 1.6 MB twice per mouse-move before any hit-testing happens, making selection visibly janky on long files.

**Location.** `crates/casual-doc-layout/src/hittest.rs:696`

**Evidence.** line_boxes walks every page and allocates a Vec of every line, then hit_test filters it down to one page; running_line_boxes already demonstrates the per-page walk pattern.

**Fix.** Add line_boxes_for_page(page) walking only the target page and use it from hit_test/hit_test_running/hit_test_text_box; index NodeId -> (page, fragment) once per snapshot for caret_rect_on and move_vertical (do this together with the geometric move_vertical fix).

### HF-082 — The galley cache never evicts entries for deleted paragraphs

**P2** · perf · perf · effort S · source: Internal audit · **Status:** Open

**Symptom.** Memory grows monotonically through a long editing session — every Enter, join and undo/redo cycle leaves shaped fragments behind for paragraphs that no longer exist, which in a browser tab is a hard limit rather than mere pressure.

**Location.** `crates/casual-doc-layout/src/incremental.rs:236`

**Evidence.** entries is only inserted into or cleared wholesale on a wrap-width change; there is no per-build sweep, size cap, or removal for nodes that left the document.

**Fix.** Mark-and-sweep per build: record the node ids touched (store + reusable hits) and retain only those at the end of build_galley_cached — free, since the build already visits every live paragraph. Assert with the existing len().

### HF-084 — Object properties panel covers the ribbon, including the overflow "..." button

**P2** · layout · ui · effort S · source: Internal audit · **Status:** Open

**Symptom.** With an object selected and Properties open, the 280px panel is painted over the ribbon's right edge — the overflow "..." button and the rightmost group of whatever tab is active become unclickable, so relocated ribbon commands are unreachable while editing an object.

**Location.** `webapp/src/style.css:3470`

**Evidence.** The body-level panel is fixed at top:86px, z-index 65, right:8px, bottom:8px, while the ribbon body starts around 93px at z-index 2 and .ribbon-overflow-btn sits at the same right edge.

**Fix.** Replace the hardcoded top: 86px with the computed offset the 900px media query already uses (calc(--h-header + --h-tab + --h-control + 48px)) and recompute it for the collapsed ribbon.

### HF-085 — main.js is 93% of the webapp with zero exports, which is why the apply paths diverged and why the embed surface is blocked

**P2** · code-structure · architecture · effort L · source: Sibling gap vs opencalc · **Status:** Open · related: HF-007

**Symptom.** Engineering-internal, but users feel it as HF-007: behaviour drifts between code paths that should be identical, and capabilities cannot be reached from surfaces that were not hand-wired.

**In opendoc.** webapp/src/main.js is 14,859 of 16,040 webapp/src lines (93%) with 104 top-level let/var, 444 top-level const, 478 top-level functions and ZERO export statements. Direct consequences already measured elsewhere in this list: the four diverged apply paths (HF-007), and docs/83's `<opendoc-editor>` element having no `mount(el, config)` boot seam to wrap (HF-109).

**In the sibling.** sheets/webapp/ splits the editor into 20 modules with a shared mutation funnel — editor.core.js 11127 lines, editor.dialogs.js 2969, editor.selection.js 1863, editor.sheets.js 1109, editor.drafts.js 1026, editor.paint.js 1006 (48% of 23,293 in the largest file), with find, dialogs, drafts, presence and i18n each behind a module boundary.

**Fix.** After HF-007's funnel lands, lift the self-contained regions out behind named exports, mirroring opencalc's boundaries: find/replace (~main.js:12500-12800), bookmarks (~11166-11300), tables (~9371-9750), dialogs (HF-043's dialog.mjs is the first slice), and review/comments. Extracting a `mount(el, config)` boot seam is the cheapest sub-item and unblocks HF-109.

### HF-086 — Undefined --bg-1 makes the header/footer band label unreadable and the "Add header" chip transparent

**P2** · css · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Double-clicking into a header shows a "Header"/"Footer" label at about 1.5:1 on its accent chip — the only cue telling the user which layer their keystrokes go to — and the "Add header" marker loses its background entirely, sitting directly on the page raster.

**Location.** `webapp/src/style.css:5911; webapp/src/style.css:5936; webapp/src/style.css:5947`

**Evidence.** grep for --bg-1 returns exactly three uses and no definition; the defined tokens are --bg and --bg-2. An invalid color inherits --ink; an invalid background resolves to transparent. header-marker.spec.mjs asserts no colours, so it passes.

**Fix.** Replace the three var(--bg-1) uses with the intended tokens: var(--accent-ink) for text on the accent fill, var(--surface) for the marker background.

### HF-087 — Marketing site navigation disappears on phones with no replacement

**P2** · responsive · ux · effort M · source: Internal audit · **Status:** Open

**Symptom.** On a 390px phone the nav and GitHub link are hidden, so Docs and Fidelity cannot be reached from any other page — only the browser back button or typing a URL works.

**Location.** `webapp/src/marketing.css:2507; webapp/_partials/site-header.html:8`

**Evidence.** .site-nav is display:none at <=820px and .brand-badge/.header-link at <=600px; the partial has only a static nav and grep for nav-toggle/hamburger/mobile-nav across html/css/build-site.py returns nothing. The 390px visual spec asserts only overflow and CTA visibility.

**Fix.** Add a mobile disclosure (nav-toggle button with aria-expanded/aria-controls revealing a stacked panel under 820px), or collapse the links into an overflow menu; at minimum keep them visible and let the header wrap.

### HF-089 — Review popover and inline accept/reject card paint above modal dialogs

**P2** · css · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** Pin a tracked-change card, then open the command palette or a dialog: the card floats on top of the scrim with live Accept/Reject buttons, so a change can be accepted while a blocking modal is supposedly in control, and the dialog is visually occluded.

**Location.** `webapp/src/style.css:3524; webapp/src/style.css:3503`

**Evidence.** z-index ladder: .cmd-overlay 60, .dialog-overlay 70, .review-popover 80, .review-inline-card 85, all body-level fixed. Dismissal is only outside-pointerdown or Escape; syncModalLock tracks only two panels and openCmd closes no review chrome.

**Fix.** Renumber so modals top the stack (.cmd-overlay/.dialog-overlay at 90/100, review layers at 60/65) and have the dialog and palette open paths call closeReviewPopover()/closeReviewInlineCard() the way the Escape handler already does.

### HF-090 — Three of four fuzz targets are built but never run, and no browser test opens a hostile document

**P2** · testing · architecture · effort M · source: Sibling gap vs opencalc · **Status:** Open

**Symptom.** Not user-visible today, but a crash or hang in the ODT importer would ship unnoticed, and the escaping discipline around document-derived strings is asserted only by code review — if a future edit drops an escapeHtml, nothing in CI turns red.

**In opendoc.** fuzz/fuzz_targets/ holds four targets and .github/workflows/ci.yml:123-127 builds all four (bounded_package, docx_package, odt_package, odt_content) — but the only campaign is .github/workflows/security.yml:65, `cargo fuzz run docx_package … -max_total_time=60`. The ODT importer (crates/casual-doc-odf/), an untrusted-input parser, is fuzzed by nothing. No security spec among the 93 e2e specs. NOTE: the innerHTML half of the original finding does not hold — every one of the 18 innerHTML sites in main.js that interpolates a document-derived string already escapes it (:7850 style name, :8861 font family, :12431 command label all wrap in escapeHtml, defined :34); the remaining 15 interpolate literals or fixed icon names.

**In the sibling.** sheets/.github/workflows/security.yml:53-64 runs a scheduled campaign over four targets for 420s each (`for target in bounded_package ooxml_xml formula_parse number_format`), with :45-50 explaining why the scheduled run must exceed the per-PR one. tests/browser/editor.security.spec.mjs:1-33 opens a hostile fixture through the real engine path and asserts no script executes and no request leaves the origin: "A workbook is untrusted input… any path that turns workbook text into markup runs script in the editor's origin."

**Fix.** Loop security.yml:65 over all four targets with opencalc's longer per-target budget. Add webapp/tests/e2e/security.spec.mjs that opens a fixture carrying script payloads in a style name, a font name, a bookmark name and an author name, then asserts zero pageerrors and zero outbound requests — that spec is what keeps the existing escapeHtml calls honest.

### HF-114 — No collaboration, presence, sharing or roles — and no server for a second person to connect to

**P2** · collab · architecture · effort L · source: Sibling gap vs opencalc + docs · **Status:** Open

**Symptom.** Two people cannot open the same document, there is no Share, no cursors, no room status. With no server-side save there is also nothing for a second person to open.

**In opendoc.** Nothing: `rg -ni 'presence|collaborat|awareness|websocket|desync|yjs'` over webapp/src → 0; no server/ directory; no collab/presence/share e2e spec. The only per-author machinery is offline review metadata (author identity at editor.html:183-184, per-author-color.spec.mjs) and .review-margin-avatar (style.css:3696), a comment-author initial chip, not presence. This is a deliberate, designed deferral: README.md:228-229 puts Phase 5 "Collaboration adapters and product migration" at Planned, and the seams already exist (docs/45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md / ADR-030: closed op set, NodeId/ModelPos anchors, sidecar).

**In the sibling.** sheets/webapp/collab.js:66 `export function collaborate({url, token, document, wasm, onStatus, onDocument, onPresence, recalcBudgetMs})` — websocket transport with heartbeat, exponential backoff, half-open detection and one-hop redirect on a full node (:88-100); editor.presence.js:249 renderPresence(), :338 jumpToParticipant(), :588 shareDialog(), :60 offerKeepWaiting() for the desync window; server-side enforcement at server/casual-calc-collab-server/src/token.rs:45 `pub enum Access { View, Comment, Edit }` and net.rs:2411 `.all(|wire| effective.permits(&wire.op))` — deny-by-default at the operation level, not by hiding chrome; the desync UX is itself a shipped fix (d27efce). docs-repo/docx-editor/packages/react/src/collab/useCollab.ts — Yjs + Hocuspocus, awareness cursors, per-user undo, IndexedDB persistence.

**Fix.** A phased program, not a hotfix — file as a roadmap epic pointing at docs/45 + docs/83 §3, and do NOT let it sit above four-line fixes in the same list. The only near-term actionable slices are (a) the ordering decision below, and (b) stating the deferral in the editor UI rather than only in the README. Explicitly strike the "interim single-writer guard" idea from the original finding: with no network layer there is no divergence class to guard against.

**Decision.** **Accepted by D-5** as a roadmap epic, not a hotfix: adopt opencalc's collaboration architecture and release model, engine-authoritative, with its operation-level deny-by-default `Access` model. The target design already shows presence avatars, Share and a Synced chip. Track the epic against docs/45 + docs/83 §3; this row stays here only as the pointer.

### HF-091 — Clipboard failure messages are styled as ordinary status text

**P3** · css · ui · effort S · source: Internal audit · **Status:** Open

**Symptom.** When the browser blocks a copy or paste, the message looks identical to "Copied 42 characters", so the user reads a failure as a confirmation and assumes the clipboard was empty.

**Location.** `webapp/src/main.js:5450; webapp/src/main.js:13479; webapp/src/main.js:13496`

**Evidence.** setStatus writes `status ${kind}` and style.css defines only .status and .status.error — there is no .status.err rule; every other failure path in the file passes "error".

**Fix.** Change the three "err" arguments to "error", and have setStatus normalize or warn on an unknown kind (a STATUS_KINDS set) so a free-form typo cannot silently drop error styling again.

### HF-092 — Validation error text is unreadable in two of the six theme/OS combinations

**P3** · css · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** With the OS light and the app set to Dark, the "why your link/bookmark/alt text was rejected" note stays dark red on a dark surface (~2.3:1); with the OS dark and the app set to Light it goes pale pink on white (~1.9:1). Both are reachable from the Settings theme control.

**Location.** `webapp/src/style.css:1444; webapp/src/style.css:1494; webapp/src/style.css:4511`

**Evidence.** The palette uses the correct three-way pattern, but these three later blocks are bare prefers-color-scheme with no :not([data-theme="light"]) guard and no explicit-dark twin, over hardcoded #b3261e light values.

**Fix.** Guard these blocks as `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) … }` with a `:root[data-theme="dark"]` twin, mirroring the palette pattern — better, replace the literals with the existing --error token plus a dark override.

### HF-093 — Keyboard-shortcut hints and empty-state prose sit at ~3.3:1 in both themes

**P3** · accessibility · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** Shortcut hints in every menu, "No matching commands" in the palette, and the Outline/font-menu empty states are drawn as small grey text that low-vision users cannot read — exactly the prose a first-time user needs most.

**Location.** `webapp/src/style.css:87; webapp/src/style.css:129`

**Evidence.** --faint is #8a8e95 on #ffffff = 3.29:1 and #6b7178 on #212429 = ~3.15:1, used as a plain text colour by .menu-item-hint, .cmd-hint, .cmd-empty, .outline-empty and .font-menu-empty.

**Fix.** Sweep the 26 --faint uses: darken to about #6f747c light / lighten to #8b9199 dark, or move the text roles to --muted and reserve --faint for non-text decoration and genuinely disabled rows (which are exempt).

### HF-094 — Compact-chrome toggle is reachable only from the ribbon chevron — not in View, not in the palette

**P3** · chrome · ux · effort S · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** You want a denser UI, open View — the obvious place — and conclude the product has no such option. And once the ribbon is collapsed the only way back is one unlabelled chevron.

**In opendoc.** webapp/src/main.js:269-291 `setRibbonCollapsed()` (persisted to opendoc.ribbonCollapsed) has exactly one user entry point, the chevron click at :294; the only other caller is auto-expand on ribbon-tab click (:236). APP_MENU_SECTIONS.view (:10940-10941) is outline/review.toggle/showChanges/zoomIn/zoomOut and the registry's view.* ids (:10780-10784) contain no compact entry, so neither the menu nor the palette can reach it.

**In the sibling.** The cross-editor consistency note (§4) requires a compact chrome toggle in the View menu specifically — the requirement is about which menu the command lives in, not merely that collapsing is possible.

**Fix.** Register a `view.compact` command (label "Compact ribbon", checkable, keywords "density compact collapse ribbon") calling setRibbonCollapsed(!ribbonViewCollapsed), add it to APP_MENU_SECTIONS.view beside view.outline, and let the palette pick it up from the same registry. Extend webapp/tests/e2e/ribbon-only-commands.spec.mjs — the spec written for exactly this defect class (checklist, line spacing, marker galleries) — with the case.

### HF-095 — Outline panel's active-row colour is defeated for Heading 3 and deeper

**P3** · css · ui · effort S · source: Internal audit · **Status:** Open

**Symptom.** With the caret in an H3, the Outline row gets the tinted background and bolder weight but its text stays grey instead of accent, and hovering deep rows changes no colour — so the current-location cue is weaker for deep headings than shallow ones.

**Location.** `webapp/src/style.css:4015`

**Evidence.** :hover and .is-active are both (0,2,0) and the lvl-3..lvl-6 rules are also (0,2,0) and declared later, so --muted overrides the accent colour; main.js emits lvl-1..lvl-6.

**Fix.** Move the depth rules above the state rules, or scope them (`.outline-item.lvl-3:not(:hover):not(.is-active)`) so they cannot win on source order.

### HF-096 — Left rail buttons have a no-op hover state

**P3** · editor-ux · ui · effort S · source: Internal audit · **Status:** Open

**Symptom.** Hovering Outline / Pages / Review produces no button-shaped fill — only a faint icon recolour — so the rail does not read as a set of buttons the way every other icon control in the shell does.

**Location.** `webapp/src/style.css:3431`

**Evidence.** .rail is background: var(--bg-2) and .rail-btn:hover sets background: var(--bg-2) — identical in every theme, so the declaration is dead. The pressed state works because it uses --accent-soft.

**Fix.** Use a token that actually contrasts with the rail's own --bg-2 background: background: var(--surface), or color-mix(in srgb, var(--ink) 6%, transparent).

### HF-098 — Image resize grips are 9px with no expanded hit area

**P3** · editor-ux · ux · effort S · source: Internal audit · **Status:** Open

**Symptom.** Resizing an image means landing the pointer inside a 9x9 px square — fiddly with a trackpad and effectively out of reach with a finger or pen, where Word and Docs both expand the invisible target well past the drawn grip.

**Location.** `webapp/src/style.css:4344; webapp/src/style.css:4388`

**Evidence.** .object-handle is 9x9 with pointer-events:auto and touch-action:none and no ::after expansion; .object-crop-handle is 12x12, likewise unexpanded. WCAG 2.5.8 AA asks for 24x24.

**Fix.** Add `.overlay .object-handle::after { content:""; position:absolute; inset:-8px; }` and the same for .object-crop-handle so the visual grip stays 9px while the target reaches ~25x25 — the pattern .ruler-marker::after already establishes.

### HF-099 — Document Properties never shows the file's byte size

**P3** · dialogs · ux · effort S · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** You open File ▸ Document properties to answer "how big is this file?" before emailing or uploading it, and the dialog answers everything except that.

**In opendoc.** webapp/editor.html has 17 `id="meta*"` fields and none is size; `rg 'metaSize|fileSize|byteLength|fmtBytes'` over main.js + editor.html → 0. The sanitization half is already correct: displayMetadataValue() (main.js:14489-14495) renders "Not set" for empty values and formatMetadataDate() (:14497-14504) guards NaN instead of printing "Invalid Date". The file NAME is also already first-class and editable — #docTitle (editor.html:35), bound to currentName (main.js:2617/2633) and used as the download name (:13063).

**In the sibling.** The cross-editor consistency note (§4) asks the Properties dialog to show the real file — actual on-disk byte size, not an in-memory serialization estimate — alongside sanitized metadata. NOTE: sheets/webapp/editor.dialogs.js:2879-2925 does not show size either, so this is a shared improvement rather than a divergence.

**Fix.** Capture File.size at load (the input at editor.html:73 already hands over a File) and render one read-only row at the top of the dialog via a fmtBytes helper, labelled "as opened" so a document edited since load does not read as a stale current size — explicitly the real byte count of the last opened/saved blob, not a re-serialization estimate.

### HF-100 — Tab stops can only be created, moved or deleted with a mouse

**P3** · accessibility · a11y · effort M · source: Internal audit · **Status:** Open

**Symptom.** A keyboard-only user cannot add a right-aligned tab stop for a dot-leader entry or move an inherited one — there is no control in the ribbon, menus, paragraph panel or command palette, so the whole capability is invisible to them.

**Location.** `webapp/src/main.js:7015; webapp/src/main.js:7036`

**Evidence.** renderTabStops builds plain divs with only a pointerdown handler; creation is a ruler pointerdown and delete/move/retype live inside startTabDrag. The only setTabStop/removeTabStop/moveTabStop call sites in the file are those four.

**Fix.** Give each .tab-glyph tabindex="0", role="button" and a keydown handler (arrows move, Enter cycles type, Delete removes) plus a keyboard "add" affordance; longer term add Word's Tabs… dialog wired to the same setTabStop/moveTabStop/removeTabStop ops and surface it from the paragraph panel and palette.

### HF-101 — Undo parks the caret at the start of the paragraph

**P3** · editor-ux · ux · effort M · source: Internal audit · **Status:** Open

**Symptom.** Ctrl+Z after deleting a formatted phrase, after Bold, after Clear Formatting, or after adding/removing a link restores the content but drops the caret at the top of the paragraph, so the next keystroke lands in the wrong place. Word and Docs restore the affected range.

**Location.** `crates/casual-doc-wasm/src/lib.rs:17062`

**Evidence.** caret_after maps SetInlines/SetParagraphProperties to Pos::new(node, 0), and SetInlines is the inverse vehicle for multi-run DeleteText, FormatText, ClearFormatting and SetHyperlink; undo/redo take the caret straight from apply_group.

**Fix.** Derive the caret from the range of the op being inverted rather than the SetInlines vehicle; fuller fix is a selection-restoring history entry with an apply_action_caret-style override in undo()/redo().

### HF-102 — Remove Link leaves the text blue and underlined

**P3** · parity · parity · effort M · source: Internal audit · **Status:** Open

**Symptom.** Text that was plain before Insert ▸ Link stays link-coloured and underlined after Remove Link, with no link behind it, and exports that way — Word and Docs both return it to its surrounding appearance.

**Location.** `crates/casual-doc-edit/src/lib.rs:1093`

**Evidence.** Link creation stamps direct underline + #0563c1 via get_or_insert while the removal branch only unwraps and coalesces. Current behaviour is documented as intentional, so this is a deliberate parity change.

**Fix.** Apply the link look through the Hyperlink character style (RunProperties::style_ref) so unwrapping removes it; do not blanket-strip direct underline/colour on removal, which would also strip an author's deliberate formatting.

### HF-103 — macOS paragraph navigation: Option+Arrow is dead and Cmd+Arrow moves by paragraph

**P3** · parity · parity · effort S · source: Internal audit · **Status:** Open

**Symptom.** On a Mac, Option+Down does nothing at all, and Cmd+Down moves one paragraph instead of jumping to the end of the document — the two most-used Mac caret chords.

**Location.** `webapp/src/keyboard.mjs:49; webapp/tests/keyboard.test.mjs:47`

**Evidence.** ArrowUp/ArrowDown short-circuit on altKey on both platforms and map apple+metaKey to paragraphUp/Down, while the same file already honours the correct split for Option+Left/Right and Cmd+Home/End.

**Fix.** On the Apple keymap map Option+Up/Down to paragraphUp/paragraphDown and Cmd+Up/Down to docStart/docEnd, keeping Ctrl+Arrow as paragraph movement elsewhere. The existing keyboard test asserts today's Cmd+Up behaviour and must be updated with the fix.

### HF-104 — The Help menu has one item, and there is no keyboard-shortcuts reference

**P3** · onboarding · ux · effort M · source: Sibling gap vs docs (ProseMirror) · **Status:** Open

**Symptom.** "What can I press?" is answered by a fuzzy-search box that requires already knowing the command's name, and the editing keys most worth learning appear nowhere. There is no About, no docs link and no way to report a problem.

**In opendoc.** webapp/src/main.js:10964 is literally `help: [["help.commands"]]`, and :10787 defines help.commands as `{ label: "Keyboard shortcuts and commands", shortcut: "⌘⇧P", run: () => openCmd() }` — it opens the command palette. The palette does render a shortcut per command (:12428), so a reference of sorts exists; what it cannot show is raw keymap bindings that are not command descriptors — Tab/Shift+Tab indent (:13758), caret navigation (:7425), smart quotes, ⌘\ clear formatting. `rg -ni 'about dialog|report issue|report bug'` over webapp → 0.

**In the sibling.** docs-repo/docx-editor/packages/react/src/components/dialogs/KeyboardShortcutsDialog.tsx (1019 lines) — categorized, searchable, platform-aware reference bound to Ctrl+/; alongside AboutDialog.tsx, ExploreDialog.tsx and report-bug.ts / reportIssue.ts in the same help surface.

**Fix.** Add a `help.shortcuts` command (Ctrl+/ or ⌘/) opening a categorized, searchable dialog on the shared .dialog-overlay shell, GENERATED from the existing command descriptors (editorCommands(), main.js:10633) at zero maintenance cost, plus a small hand-maintained table for the raw keymap entries — that table is the natural home for the not-yet-discoverable editing keys. Keep the palette on ⌘⇧P and repoint :10787 at the new dialog. Add About (version + license) and a link to the published docs site. Renders through HF-025's formatShortcut.

### HF-105 — Print freezes the tab with no progress, cancel, or page-range control

**P3** · perf · perf · effort M · source: Internal audit · **Status:** Open · related: HF-030

**Symptom.** Cmd+P on a 300-page document locks the tab for tens of seconds with no spinner or message while every page is rasterized and retained (~8 MB each), often ending in a blank print or a dead tab.

**Location.** `webapp/src/main.js:2874`

**Evidence.** printDocument loops every page calling renderPage(i, 150) synchronously and retains one full-resolution canvas per page before window.print(), with no setStatus and no cancel. (Transient WASM buffers are freed, so this is scalability and feedback, not a leak.)

**Fix.** Show "Preparing pages for print…" immediately, build in rAF/setTimeout chunks with a page N of M counter and a Cancel control, and stream pages as releasable Blob-backed images; add a page-range field beyond a threshold.

### HF-106 — In crop mode arrow keys move the picture and a cancelled drag leaves crop stuck

**P3** · editor-ux · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Pressing an arrow key to nudge a crop moves the floating image instead, leaving the dim strips and all eight handles painted over the picture's old position. On touch or pen, a gesture the browser cancels leaves the crop rectangle following the pointer until Escape.

**Location.** `webapp/src/main.js:13713; webapp/src/main.js:3434; webapp/src/main.js:3509`

**Evidence.** The crop keydown block handles only Enter/Escape and falls through to the nudge map; paintObjectCrop draws from the box cached at enterCropMode; startCropHandleDrag removes its window listeners only inside its own pointerup.

**Fix.** Handle arrows inside the crop session (adjust s.crop by a fixed fraction, Shift for a larger step) and return before the object grammar; re-read doc.objectRect into s.box at the top of paintObjectCrop; and clear the crop session from the four existing global cancel handlers (pointercancel, lostpointercapture, blur, visibilitychange).

### HF-107 — Changing a list marker writes numbering definitions outside the undo system

**P3** · wasm · bug · effort M · source: Internal audit · **Status:** Open

**Symptom.** Each change-marker/undo cycle leaves an orphan abstract numbering definition and instance behind, so repeated use bloats the exported numbering.xml with definitions nothing references.

**Location.** `crates/casual-doc-wasm/src/lib.rs:4397; crates/casual-doc-wasm/src/lib.rs:4159`

**Evidence.** set_list_format and restart_list write abstract_numbering/numbering through definitions_mut outside any Operation, so no inverse exists — contradicting the I1 choke-point invariant documented in the same file. (ensure_list/ensure_checklist are memoized and are not affected.)

**Fix.** Route definition creation through an operation (an InsertNumberingDefinition with a removing inverse) or at minimum defer the definitions_mut writes until after validation and fold them into the same undo entry.

### HF-108 — setObjectExtent and insertImage accept NaN and collapse the object to 1 EMU

**P3** · wasm · bug · effort S · source: Internal audit · **Status:** Open

**Symptom.** A host computing an aspect-preserving size from a zero natural width gets success back while the image silently vanishes from the page, and the undo entry records the collapse as a legitimate resize.

**Location.** `crates/casual-doc-wasm/src/lib.rs:1140; crates/casual-doc-wasm/src/lib.rs:1491`

**Evidence.** Both clamp (v.round() as i64) to 1..MAX_EMU, and NaN saturates to 0 then clamps to 1, while the sibling resize_object rejects the same inputs via bounded_emu. No in-repo caller triggers it — the exposure is third-party embedders.

**Fix.** Use bounded_emu(..., MIN_OBJECT_EMU, MAX_EMU, "object width")? in both, exactly as resize_object does, so non-finite and out-of-range values error instead of clamping.

### HF-109 — Nothing is embeddable: no host-capability modes, no custom element, no package

**P3** · embedding · architecture · effort L · source: Sibling gap vs opencalc + docs · **Status:** Open

**Symptom.** No host can embed, theme, gate or drive opendoc; the only way to reach it is to navigate to editor.html. Note the concrete opencalc defect does NOT currently occur here — the only embed is opendoc's own same-origin marketing iframe with no host document to replace — so this is a missing product surface, not a live security hole.

**In opendoc.** `rg 'customElements|attachShadow'` over webapp/ (excluding pkg) → 0. The only embed surface is webapp/src/home-embed.js, which iframes the whole standalone editor (`./editor.html?demo=1`) with full chrome. The entire URL surface in boot() (main.js:2407-2424) is demo/fixture/blank — no mode, chrome or capability parameter — and the only readOnly-ish concept is the Review Viewing/Editing/Suggesting gate (a document state, not a host permission). No sdk/, server/, desktop/ or deploy/ at the repo root; webapp/package.json is `"private": true` with no build or exports; .github/workflows has no release or packaging job. This is a documented deferral: webapp/README.md:3-7 calls the editor "a pre-release developer surface, not a stable SDK or supported product" and docs/83 is the approved spec, tracked as SDK-001 (docs/14:89).

**In the sibling.** sheets/webapp/embed.js:565 `customElements.define("opencalc-sheet", OpenCalcSheet)` — a shadow-DOM element with `all: initial` on :host, typed surface in embed.d.ts:52-92/163-205 (chrome(regions), commands(rules), run(id), theme(tokens), setColorScheme, configure, open, save, `Access = "edit"|"view"|"preview"`), published as sdk/packages/{engine,react,sheet} with five worked examples and a compile-checked consumer (sdk/types/consumer.ts) plus release workflows. Behind it, editor.core.js:947-970 defines CAPABILITIES = [canOpen, canSaveAs, canPrint, canShare, ownsFile, chrome, readOnly] with standalone/desktop/embedded/wopi/viewer presets and :1015-1035 askedMode() defaults a framed editor to `embedded`, fixing a measured defect: a framed editor.html "resolved to standalone, so File ▸ New and File ▸ Open were listed and runnable and a visitor could replace the host's document from inside the host's own page." docs-ref: packages/{core,react,vue} + embed-runtime/ + embed/EmbedHostTransport.ts.

**Fix.** Two separable pieces against docs/83 (not against opencalc's @opencalc/sheet shape — see the decision below). (a) Cheap and worth doing early: a CAPABILITIES set beside the existing review modes with standalone/embedded/viewer presets resolved from `?mode=`, defaulting a framed page to `embedded`, gating the File menu, download entries and branding before first paint. (b) Larger: extract a `mount(el, config)` boot seam out of main.js (blocked on HF-085), then ship the `<opendoc-editor>` shadow-DOM element with open/save/configure/theme/run over the existing command registry (:10643), a hand-written .d.ts beside it, and a consumer type-check job so the declaration cannot drift.

**Decision.** **Still blocked on D-6** — the one decision not yet made.

### HF-110 — Every keystroke walks and materializes the whole document's text twice

**P3** · wasm · perf · effort M · source: Internal audit · **Status:** Open

**Symptom.** Typing latency grows with document length on a 200-page file — thousands of string allocations per committed character — even though pagination itself is incremental.

**Location.** `crates/casual-doc-wasm/src/lib.rs:8999; crates/casual-doc-wasm/src/lib.rs:3116`

**Evidence.** ordered_paragraphs allocates a String per paragraph across every surface; type_text calls order_endpoints and then selection_delete_ops, which calls order_endpoints again — two full document text extractions per collapsed-caret keystroke.

**Fix.** Thread the already-computed ordered list / (start, end) from type_text and insert_plain_text_action into selection_delete_ops instead of re-resolving from node strings; cache the ordered (NodeId, len) index and invalidate it in finish_edit.

### HF-111 — Each suggested keystroke re-validates the entire document

**P3** · perf · perf · effort M · source: Internal audit · **Status:** Open

**Symptom.** Typing with track changes on a 150-page document runs a full-document walk with per-run grapheme segmentation per character, so suggesting-mode typing gets slower with document length while ordinary typing does not.

**Location.** `crates/casual-doc-edit/src/lib.rs:1475`

**Evidence.** The UpdateReviewState arm calls doc.validate() after swapping inlines; suggest_insert applies one UpdateReviewState per typed character, and Document::validate walks the body plus every header, footer and note counting graphemes per run.

**Fix.** Scope the post-mutation check to what the op can break — the replaced paragraphs' inlines plus the comment map — preserving the exact rollback guarantees (adjacent-equal runs, dangling refs, id uniqueness); keep the whole-document check behind debug_assertions. Benchmark on the large-document corpus first.

### HF-112 — flow_blocks recomputes the running galley height for every paragraph

**P3** · perf · perf · effort S · source: Internal audit · **Status:** Open

**Symptom.** Laying out a long document spends hundreds of millions of redundant additions on a value that is free to maintain — paid again on every uncached rebuild — and risks an i32 overflow panic in debug on pathological documents.

**Location.** `crates/casual-doc-layout/src/flow.rs:1039`

**Evidence.** The paragraph arm folds BlockFragment::height over the whole galley unconditionally before the wrap-carry check, and paragraph height itself folds over every line, making the loop quadratic in document size.

**Fix.** Move the fold inside the `if let Some(clearance) = paragraph_wrap_carries(..)` that is its only consumer (wrap carries are rare), or maintain a running_height alongside galley using saturating_add.

### HF-113 — Every pointermove re-queries and materializes all page wrappers

**P3** · perf · perf · effort S · source: Internal audit · **Status:** Open

**Symptom.** Moving the mouse over a 300-page document rebuilds a 300-element node list 60-120 times a second before any hit-testing, adding avoidable jank and GC pressure during drag-selection.

**Location.** `webapp/src/main.js:5461`

**Evidence.** pageFromEvent spreads pagesEl.querySelectorAll('.page-wrap') into an array and indexOf's it on every pointermove/pointerdown, while pageIndexOfWrap reads wrap.__pageIndex, populated for every page by observePages.

**Fix.** Replace the body of pageFromEvent with `const idx = pageIndexOfWrap(wrap); return idx < 0 ? null : pages[idx];` — an exactly equivalent O(1) lookup that also removes the stale-index window during a rebuild.

