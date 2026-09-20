# 109 — The Backlog

**Status:** Living record — **the single working queue.** **Opened:** 2026-09-20.
**Owner:** unassigned.

## What this is

Every piece of open work in this repository, in the order it will be worked, in one
ordered table. **This is the only queue to work from.** Pick the lowest `#` you can
start, work it, close it here and in its source tracker.

Until today the open work was spread across three documents, and picking "the next thing"
meant merge-sorting them by hand every session — which is how a row falls out of sight.
`104`, `105` and `106` are now **evidence and design archives, closed to new rows**. They
keep the audit evidence, the verification history and the roadmap rationale that this
document deliberately does not duplicate. This document holds the **order**; they hold the
**why**.

| Document | Still holds |
| --- | --- |
| `104-HOTFIX-TRACKER.md` | Per-defect detail sections, the refuted findings, the cross-cutting themes, the owner decisions |
| `105-AUDIT-2026-09-TRACKER.md` | The 2026-09 audit evidence, measurements, file/line citations, the ONLYOFFICE source analysis |
| `106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md` | Phase gates, sequencing rationale, the definition of "alternative", the effort estimates |
| **this document** | **The order of work, and nothing else** |

## How the order was decided

The owner's instruction, 2026-09-20:

> "lets complete the hotfix.. target those than audit and than roadmap"

So the global order is three **lanes**, in this order:

1. **Hotfix** — every open row of `104`. Defects in shipped code come first.
2. **Audit** — every open row of `105` (EV / UX / CQ / FID / OO).
3. **Roadmap** — the work in `106` that is *not* already a row in `104` or `105`, plus the
   open owner decisions that gate phases.

Within the Hotfix and Audit lanes, rows are ordered **by priority** (P0 → P1 → P2 → P3),
then **structural unblockers first**, then **cheapest first** (S → M → L → unsized), so an
unblocker is never queued behind the thing it unblocks. Where a row blocks another, the
`Notes / blocked-by` column says so — including the cases where the blocker sits *later* in
the queue because the lane rule outranks the dependency. Those are the rows to reorder
first if the owner wants throughput over lane discipline.

### How a row is graded

Added 2026-09-20, when the owner authorised re-grading. The principle, stated so the next
reader can apply it rather than guess it:

> **Priority = user-visible harm × how much else the row unblocks.**

The second factor is the one the old trackers never encoded, and its absence is why the
god-file sat at P2 while four rows waited on it, and why the embed row — the wedge, by the
roadmap's own words — sat at P3. A row that gates four others is not a P2 because its own
symptom is quiet. Conversely, a missing feature on an uncommon path is not a P1 because it
is annoying.

Where the grade changed, the row's `Notes / blocked-by` cell **states the old grade, the new
grade, the date, and the reason**, so a re-grade is auditable rather than a number that
silently moved. Six rows were re-graded on 2026-09-20: HF-011 and HF-045 up to P0, HF-085,
HF-109 and HF-114 up to P1, HF-051 down to P2. Nothing else was re-graded; candidates found
during verification are listed under
[Proposed further re-grades](#proposed-further-re-grades) for the owner instead of being
applied.

The **Roadmap lane is ordered by phase, not by priority**, because `106` states no
priority for these items and the whole content of `106` is an order. The open owner
decisions lead the lane: they are the cheapest unblockers in the queue, and each one gates
a phase. Their `Priority` cell reads `—` rather than a number invented here.

**The `Effort` scale is not comparable across lanes.** `104` grades S = under an hour,
M = up to a day, L = more than a day. `105` grades S = under a day, M = up to a week,
L = more than a week. Both are carried across verbatim; neither was rescaled.

## Rules for this document

1. **No id is ever renumbered.** `HF-011`, `UX-001`, `FID-L-03`, `CQ-002`, `OO-015` and
   `EV-005` keep their ids forever — commits and PRs cite them. This document *references*
   the source ids; it does not replace them.
2. **Status and priority are carried across from the source tracker unless this document
   says otherwise in the row itself.** The original rule was "verbatim, nothing re-graded",
   which is what a merge document should do on the day it is built. The owner lifted it on
   2026-09-20: see [How a row is graded](#how-a-row-is-graded). A re-grade is legitimate
   only when the row's own Notes cell records the old grade, the new grade, the date and
   the reason. Where a source row merely looks stale, or two sources disagree and the
   owner has not settled it, it is still listed under
   [Rows that need re-verification](#rows-that-need-re-verification) rather than silently
   corrected.
3. **Each piece of work appears exactly once.** Where the sources state that one row
   restates or supersedes another, the row appears under the id the sources call
   authoritative and the other id is named in the `Supersedes / see also` column. The merge
   rule, applied literally: **a row is merged only where a source tracker states the
   equivalence** — "this is `HF-085`", "Close under UX-004, not here", "Open → UX-005", or
   an OO row whose finding restates an HF row word for word. Where a `105` or `106` row is
   *broader* than the row it cites, both stay and the narrower id is named as the part that
   closes first.
4. **The counts are derived, never typed.** `webapp/tests/tracker_counts.test.mjs`
   re-derives every cell of the summary below from the rows in this file, fails if an id
   appears twice, fails if a row that is open in `104` or `105` is missing from here, and
   fails if the lane order or the within-lane priority order is broken. Re-run it rather
   than editing a number.

### How to add a row

1. Give it the next free id in its class — `HF-161` for a new defect (check the highest
   `HF-NNN` in this file *and* in `104`), or the next `UX-`/`CQ-`/`FID-`/`OO-` id if it
   belongs to an audit class. Roadmap work that carries no id in `106` gets the next
   `RM-NN`; those ids were minted by this document and are stable from now on.
2. Insert it at the position its lane and priority demand, then renumber the `#` column
   (it is a position, not an identity — the `Id` is the identity).
3. Do **not** add it to `104`, `105` or `106`: they are closed to new rows.
4. Run `cd webapp && npm run test:unit`. The guard will tell you if the summary, the
   ordering or the coverage no longer holds.

## Summary

**132 rows in the one queue: 46 Hotfix, 71 Audit, 15 Roadmap.**

Derived from the rows below by `webapp/tests/tracker_counts.test.mjs`. Do not edit these
cells by hand — re-derive them. (`104`'s summary drifted for exactly as long as nothing
re-derived it. Its two wrong per-section cells were corrected on 2026-09-20 and the guard
now asserts each section cell individually, because the two errors were equal and opposite
and the old sum-to-Total check could not see either.)

| Lane | Rows | P0 | P1 | P2 | P3 | Unprioritised |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hotfix | 46 | 0 | 9 | 24 | 13 | 0 |
| Audit | 71 | 0 | 32 | 31 | 8 | 0 |
| Roadmap | 15 | 0 | 0 | 0 | 0 | 15 |
| **Total** | **132** | **0** | **41** | **55** | **21** | **15** |

**There are two P0s again, and that is a correction, not a regression.** Every P0 *inherited*
from `104` and `105` is closed. HF-045 and HF-011 were re-graded into P0 on 2026-09-20
against `104`'s own definition of the grade — *data loss, corruption, unrecoverable state* —
after both were verified still live in the source. Neither is a new defect; both were simply
graded below the definition they meet.

## The queue

| # | Id | Lane | What | Priority | Effort | Status | Source | Supersedes / see also | Notes / blocked-by |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | HF-085 | Hotfix | `main.js` is 93% of the webapp with zero exports, which is why the apply paths diverged and why the embed surface is blocked | P1 | L | Partly fixed (#555) | 104 §P2 | UX-003, CQ-001 | First four seams out: `command_taxonomy`, `status_policy`, `units`, `text_rules` — 29 exports, 18,373 to 18,186 lines, unit tests 123 to 170, behind a line-count ratchet. **`main.js` itself still has zero exports and there is no mount seam, so HF-109 is NOT unblocked.** `editorCommands()` (~540 lines of closures over `doc`/`selection`) is deliberately left: lifting it needs a context/actions object, which is a design decision rather than a refactor. What IS unblocked is UX-004/UX-005 — `menu_taxonomy.test.mjs` parsed the taxonomy out of `main.js` as text with a brace matcher and now imports the structure, so parity can be set equality. |
| 2 | HF-025 | Hotfix | Every shortcut label is a hardcoded ⌘ glyph — Windows and Linux users are shown keys their keyboard does not have | P1 | M | Partly fixed | 104 §P1 | UX-009 | UX-009 measured it: 70 hardcoded glyphs, 52 in `main.js` and 18 in `editor.html`; only the labels are wrong, the handlers already accept both modifiers. Re-verified 2026-09-20 and **downgraded Open → Partly fixed**: `formatShortcut()` (`webapp/src/keyboard.mjs:128`) now runs on the palette, the app menu, the shortcuts dialog and ribbon tooltips, but the palette's own `⌘⇧P` chip is a static literal in `editor.html` and the paste-options `title` attributes sit outside `TIP_SELECTOR`, so raw ⌘ still reaches Windows and Linux |
| 3 | HF-088 | Hotfix | The comments column has no breakpoint below 860px and swallows the page | P1 | M | Partly fixed | 104 §P1 | UX-018 | The row's own evidence line is recorded as wrong in `104` §Progress: the 860px rung has never applied at any width. Re-verified 2026-09-20 — a 700px rung now exists and does apply (`style.css:4314`, `:4666`, `:4861`), so the column no longer swallows the page; the bottom-sheet behaviour the fix asks for is still unbuilt, which is why the row stays Partly fixed rather than closing |
| 4 | HF-162 | Hotfix | The layout is not windowed — `paginate_document` builds a galley and a display list for every page before the first one is shown, so a 1.3M-paragraph file is refused | P1 | L | Partly fixed (#555) | New 2026-09-20 (`111` §4 stage 2) | HF-158, HF-077 | Steps 2-3 of `113` landed: measure tier split from the paint tier, checkpoints (80 B) and `paginate_from`, one paginator generic over `Paginable` so the tiers cannot diverge. Layout half 6,991 to ~190 B per paragraph (37x); 1.3M projection 9.70 to 1.45 GiB. **Resident, not peak** — flow still emits full fragments and projects, so the owner's 1.3M-paragraph file is still refused and `MAX_VIEWER_BLOCKS` is deliberately unmoved. Steps 4-5 (flow emitting measures, byte-budgeted eviction, wiring the window to scroll, then re-measuring the constant) are in flight on `perf/windowed-layout-eviction`. |
| 5 | HF-035 | Hotfix | No spelling or grammar checking anywhere — less feedback than a plain `<textarea>` | P1 | L | Open | 104 §P1 | OO-003 | Unblocked: UX-001 (the editable focus owner) shipped in #537. ONLYOFFICE proves it is a client-side WASM problem |
| 6 | HF-030 | Hotfix | Print and "Save as PDF" emit a 150-DPI raster — no selectable, searchable or accessible text | P1 | L | Open | 104 §P1 | OO-010 | Blocked-by RM-04 (real-text PDF export) for the output half; the dialog half is OO-010 |
| 7 | HF-081 | Hotfix | No localization seam — every string is an English literal inside a 14.9k-line file | P1 | L | Open | 104 §P1 | CQ-005, UX-009 | **Blocked-by HF-085**, which now leads the P1 block ahead of it: there is no module to extract strings into. Verified still fully open 2026-09-20 — no `i18n.mjs` in `webapp/src`, no `t(`/`setMessages`/`relabel` anywhere, `lang="en"` hardcoded on `<html>`, and plurals still hand-rolled inline |
| 8 | HF-109 | Hotfix | Nothing is embeddable: no host-capability modes, no custom element, no package | P1 | L | Open | 104 §P3 | CQ-010 | **Re-graded P3 → P1, ordered after HF-085, 2026-09-20.** `106` Phase 4 is titled *Make it embeddable · this is the wedge* and SKILL.md §1 says embeddability is the product — a P3 embed row contradicted the stated product thesis outright. Resolves the HF-109 P3 / CQ-010 P1 conflict. **Blocked-by HF-085** (there is no module for a host to mount) and by Q1 (the D-6 embed contract). Verified still fully open 2026-09-20: zero `customElements.define`/`attachShadow` hits in `webapp/src`, no mount/unmount seam, no host-capability object (the only URL params are `demo`, `fixture`, `blank`), and `webapp/package.json` carries no `main`, `exports` or `files` |
| 9 | HF-114 | Hotfix | No collaboration, presence, sharing or roles — and no server for a second person to connect to | P1 | L | Open | 104 §P2 | OO-018 | **Re-graded P2 → P1 and ordered last in the P1 block, 2026-09-20**, matching OO-018's P1 — the two rows are the same work in the same words. Late rather than early because ADR-033 and `107` record that the live edit path bypasses the transaction engine, which has to be unified first: verified again 2026-09-20, `casual_doc_transaction` has **zero** references in `crates/casual-doc-wasm`, and the two parallel op sets are 5 variants against 49. So CQ-002 comes first, then RM-08/RM-09. `106` Phase 6.6/6.7. Verified still fully open: no websocket, presence or awareness code anywhere in `crates/**` or `webapp/**` |
| 10 | HF-055 | Hotfix | Smart quotes insert the wrong glyph after any non-ASCII character | P2 | S | Open | 104 §P2 | OO-016 | — |
| 11 | HF-058 | Hotfix | The object action bar stays frozen on screen while the object scrolls away | P2 | S | Open | 104 §P2 | — | — |
| 12 | HF-059 | Hotfix | Cmd+V never pastes an image, and says nothing | P2 | S | Open | 104 §P2 | — | — |
| 13 | HF-065 | Hotfix | Re-opening the same file does nothing, and file read errors are completely silent | P2 | S | Open | 104 §P2 | — | — |
| 14 | HF-066 | Hotfix | Pasted hyperlinks are stored with no scheme filter and re-exported | P2 | S | Partly fixed | 104 §P2 | — | — |
| 15 | HF-051 | Hotfix | No Word Count dialog and no selection-scoped counts | P2 | M | Open | 104 §P1 | OO-015 | **Re-graded P1 → P2, 2026-09-20.** A missing feature on an uncommon path, which is `104`'s own P2 definition, not user-visibly wrong behaviour on a common one; OO-015 grades the identical work P3, so P2 is the midpoint between the two sources rather than a demotion to nothing. It is cheap, so it still leads the M group of P2. UI only: the engine already exposes `document_stats`, `words`, `characters`, `characters_with_spaces` (OO-015). Verified still open 2026-09-20: no `tools.wordCount` command id exists anywhere, `APP_MENU_SECTIONS.tools` is still `[["tools.smartQuotes"], ["view.settings"]]`, and `updateStats()` does no selection scoping |
| 16 | HF-036 | Hotfix | Printing a mixed-orientation document silently clips the landscape pages | P2 | M | Open | 104 §P2 | OO-010 | — |
| 17 | HF-039 | Hotfix | Find highlights only the current match, so "7 of 23" cannot be answered by looking at the page | P2 | M | Open | 104 §P2 | — | — |
| 18 | HF-047 | Hotfix | Import/export data loss is reported as a bare number — the report naming what was lost is parsed and discarded | P2 | M | Open | 104 §P2 | FID-R-02 | Part of the Phase 2 reporting substrate |
| 19 | HF-097 | Hotfix | Tools and Help scroll out of the menu bar behind a hidden scrollbar | P2 | M | Partly fixed | 104 §P2 | — | — |
| 20 | HF-057 | Hotfix | Object properties panel shows stale geometry and Apply reverts a drag-resize | P2 | M | Open | 104 §P2 | — | — |
| 21 | HF-064 | Hotfix | No accessibility checker — the editor is accessible but never audits the document being written | P2 | M | Open | 104 §P2 | — | ONLYOFFICE has none either (`105` §4) — this is judged against Word |
| 22 | HF-070 | Hotfix | Ribbon popovers and Settings never take focus, and closing them loses the user's place | P2 | M | Open | 104 §P2 | UX-023 | UX-023 is the broader ARIA row and cites this one |
| 23 | HF-073 | Hotfix | No recent documents — the only way back into yesterday's file is the OS file picker | P2 | M | Open | 104 §P2 | UX-011, OO-002 | Needs a decision first: UX-011 records that `<input type=file>` yields no re-openable handle, so the obvious implementation would be a dead control |
| 24 | HF-078 | Hotfix | Images are re-decoded from source bytes on every page repaint | P2 | M | Open | 104 §P2 | — | — |
| 25 | HF-090 | Hotfix | Three of four fuzz targets are built but never run, and no browser test opens a hostile document | P2 | M | Open | 104 §P2 | CQ-006 | CQ-006 is the broader test-surface row and cites this one |
| 26 | HF-056 | Hotfix | Images cannot be rotated or flipped — a sideways phone photo has to be fixed outside the editor | P2 | L | Open | 104 §P2 | — | — |
| 27 | HF-068 | Hotfix | No version history — the document has no past that survives a reload | P2 | L | Open | 104 §P2 | OO-004 | `106` Phase 6.2, behind HF-011 and CQ-002 |
| 28 | HF-071 | Hotfix | The accessibility mirror is rebuilt wholesale on every edit, resetting the screen reader to the top | P2 | L | Open | 104 §P2 | UX-020 | UX-020 is the broader a11y row and cites this one as its clause (a) |
| 29 | HF-077 | Hotfix | Opening a heavy document freezes the tab with no budget, no progress and no cancel | P2 | L | Open | 104 §P2 | — | Partially relieved by the admission limits in #552; the budget/progress/cancel design is still owed. The memory ceiling underneath it is HF-161 and HF-162 (`111`), which now sit ahead of this row at P1 |
| 30 | HF-160 | Hotfix | No `.rtf` support — RTF is in neither the import nor the export registry, so an RTF file cannot be opened at all | P2 | L | Partly fixed (#553) | New 2026-09-20 (owner) | RM-06 | Work in flight on `feat/rtf-import`. RM-06 carries the remaining interchange formats (HTML, Markdown, EPUB, FB2, DOTX/OTT); this row is the RTF slice only Import landed and is reachable from the picker. **Export is not built**, and CJK is substantially broken — cp949/cp950 producers write raw DBCS and lose most of their text (`110` section 4). Both remain open under RM-06. |
| 31 | HF-121 | Hotfix | An empty centred or right-aligned paragraph parks the caret at the left margin while the text lands elsewhere | P2 | — | Open | 104 §Behavioural audit 2026-09-04 | — | Not repairable in the hit-test: the shaper must emit a zero-glyph run at the aligned origin, or `Line` must carry its resolved start |
| 32 | HF-123 | Hotfix | Clicking past the last word of a soft-wrapped line, or Home/End there, teleports the caret to another visual line | P2 | — | Open | 104 §Behavioural audit 2026-09-04 | — | `caret_start_line` has no affinity; measured with no tracked change present, so it is purely an affinity defect |
| 33 | HF-132 | Hotfix | The emoji picker offers 355 glyphs against ~1,900 in Word/Docs/Slack, and its search is near-useless | P2 | — | Open (owner decision) | 104 §Behavioural audit 2026-09-04 | — | Gated on a bundle-size and font-coverage decision by the owner |
| 34 | HF-091 | Hotfix | Clipboard failure messages are styled as ordinary status text | P3 | S | Open | 104 §P3 | — | — |
| 35 | HF-099 | Hotfix | Document Properties never shows the file's byte size | P3 | S | Open | 104 §P3 | — | — |
| 36 | HF-103 | Hotfix | macOS paragraph navigation: Option+Arrow is dead and Cmd+Arrow moves by paragraph | P3 | S | Open | 104 §P3 | — | — |
| 37 | HF-113 | Hotfix | Every pointermove re-queries and materializes all page wrappers | P3 | S | Open | 104 §P3 | — | — |
| 38 | HF-100 | Hotfix | Tab stops can only be created, moved or deleted with a mouse | P3 | M | Open | 104 §P3 | — | — |
| 39 | HF-101 | Hotfix | Undo parks the caret at the start of the paragraph | P3 | M | Open | 104 §P3 | — | — |
| 40 | HF-102 | Hotfix | Remove Link leaves the text blue and underlined | P3 | M | Open | 104 §P3 | — | — |
| 41 | HF-104 | Hotfix | The Help menu has one item, and there is no keyboard-shortcuts reference | P3 | M | Open | 104 §P3 | — | — |
| 42 | HF-105 | Hotfix | Print freezes the tab with no progress, cancel, or page-range control | P3 | M | Open | 104 §P3 | OO-010 | — |
| 43 | HF-106 | Hotfix | In crop mode arrow keys move the picture and a cancelled drag leaves crop stuck | P3 | M | Open | 104 §P3 | — | — |
| 44 | HF-107 | Hotfix | Changing a list marker writes numbering definitions outside the undo system | P3 | M | Open | 104 §P3 | CQ-002 | — |
| 45 | HF-111 | Hotfix | Each suggested keystroke re-validates the entire document | P3 | M | Open | 104 §P3 | CQ-002 | `106` names this the B1 prerequisite of Phase 6.0 — it is worked with CQ-002, not after it |
| 46 | HF-127 | Hotfix | Ctrl/Cmd+Enter (page break) is inert — the chord is swallowed before the Enter branch and no inline page-break op exists | P3 | — | Open | 104 §Behavioural audit 2026-09-04 | UX-006 | UX-006 carries the rest of the unbound chords |
| 47 | UX-021 | Audit | Four of five `role="radiogroup"` containers own toggle buttons, not radios | P1 | S | Open | 105 §2.3 | — | — |
| 48 | CQ-007 | Audit | Hand-maintained numbers drift, and have twice become false public claims | P1 | S | Partly fixed (#528) | 105 §2A | — | Still open: PR attribution is hand-written and nothing checks that a cited PR contains the change. This document's own summary is under the guard that closed the first half |
| 49 | FID-P-01 | Audit | No oracle reference is committed, so the geometry gate is inert | P1 | S | Partly (#532/#535/#541) | 105 §3.1 | — | `106` Phase 0. One dispatch plus a bless away from armed |
| 50 | FID-P-04 | Audit | "Modeled" is counted as done while nothing consumes it | P1 | S | Open | 105 §3.1 | — | The guard that stops the single most expensive recurring pattern in the project. Cheap, and it protects the whole FID-L lane |
| 51 | FID-L-05 | Audit | Footnote number format, restart, and position are entirely unconsumed | P1 | S | Partly fixed (#544) | 105 §3.2 | — | Remainder stated in the source row |
| 52 | FID-R-04 | Audit | Three parsers have zero reporting and their parts are regenerated, so loss is permanent and invisible | P1 | S | Partly fixed (#540/#541) | 105 §3.3 | — | Remainder stated in the source row |
| 53 | EV-005 | Audit | The matrix drift guard cannot detect an overstatement, and does not cover the page | P1 | M | In progress | 105 §1 | — | The last open EV row |
| 54 | UX-004 | Audit | The two tests named for command-surface parity do not enforce it | P1 | M | Open | 105 §2.2 | HF-076 | `104` closes HF-076 under this row, not under itself: the guard asserts a frozen 7-id `toContain` list, so it cannot detect an omitted entry. **Blocked-by HF-085**, which now precedes it in the queue. Verified still open 2026-09-20 — the literal 7-id array is unchanged, and there are still exactly 7 `contextMenu: true` declarations, so the snapshot is accurate today and structurally unable to fail tomorrow |
| 55 | UX-005 | Audit | Only ~23 of ~90 ribbon controls carry a command id | P1 | M | Open | 105 §2.2 | CQ-004 | **Blocked-by HF-085**, which now precedes it in the queue. CQ-004's status cell is literally "Open → UX-005". Re-counted 2026-09-20: **39 of 109** ribbon controls carry a command id — Insert, Layout, References and Review are stamped; Home (45), Table (19) and View (5) are not — so the row's "~23 of ~90" is stale while the defect is not |
| 56 | UX-006 | Audit | Standard word-processor shortcuts are unbound — 18 `shortcut:` declarations in total | P1 | M | Open | 105 §2.2 | HF-127 | — |
| 57 | UX-017 | Audit | The only feedback channel is `display:none` at phone widths — the editor refuses silently and inaudibly | P1 | M | Open | 105 §2.3 | — | — |
| 58 | UX-022 | Audit | Engine boot and boot failure have no state design | P1 | M | Open | 105 §2.3 | — | — |
| 59 | CQ-003 | Audit | Guards that cannot fail | P1 | M | Partly fixed | 105 §2A | — | Two of the three are the command-parity and IME guards Phase 1 depends on |
| 60 | CQ-006 | Audit | Test surface has structural blind spots — one browser, no axe, 1 of 4 fuzz targets, no layout benchmark | P1 | M | Open | 105 §2A | HF-090 | The missing repaint benchmark is how a fabricated "7 ms" reached a public page |
| 61 | FID-P-02 | Audit | There is no Microsoft-Word-produced fixture anywhere in the repository | P1 | M | Open | 105 §3.1 | — | Blocked-by Q2 (a licensing and privacy decision). Gates every fidelity claim |
| 62 | FID-P-03 | Audit | Round-trip tests are a fixed point and cannot detect lossy import | P1 | M | Open | 105 §3.1 | — | — |
| 63 | FID-L-02 | Audit | No hyphenation at all | P1 | M | Open | 105 §3.2 | OO-006 | Default-on in many European templates, and it changes pagination |
| 64 | FID-L-04 | Audit | ~180 DrawingML preset shapes collapse to bounding rectangles | P1 | M | Open | 105 §3.2 | OO-014 | Needs a path/Bézier primitive first, then the presets table-drive |
| 65 | FID-L-07 | Audit | Floating tables (`w:tblpPr`) render inline | P1 | M | Open | 105 §3.2 | — | — |
| 66 | FID-R-02 | Audit | `ModelOutcome` is hardcoded `Omitted` at all three construction sites | P1 | M | Open | 105 §3.3 | HF-047 | The reporting substrate A2 depends on |
| 67 | FID-R-03 | Audit | Unknown *attributes* are outside the report vocabulary entirely | P1 | M | Open | 105 §3.3 | — | — |
| 68 | OO-002 | Audit | No New document, no recent files, no templates, no backstage | P1 | M | Open | 105 §4.4 | UX-011 | Broader than HF-016 (New, shipped in #542) and HF-073 (recent); templates and the backstage IA are the remainder |
| 69 | UX-018 | Audit | Zero touch code — no `touchstart`/`pointerType`/`maxTouchPoints` anywhere | P1 | L | Open | 105 §2.3 | HF-088 | Blocked-by Q6 (the mobile commitment). `18` declares mobile supported |
| 70 | UX-019 | Audit | No breakpoint below 620px; at 390px the editor is one dropdown | P1 | L | Open | 105 §2.3 | — | Blocked-by Q6 |
| 71 | UX-020 | Audit | The accessibility mirror is read-only and unanchored to the caret | P1 | L | Open | 105 §2.3 | HF-071 | A screen-reader user can read the document but cannot verify an edit landed, so cannot edit |
| 72 | CQ-001 | Audit | Two god-files, one per side of the boundary | P1 | L | Open | 105 §2A | HF-085 | The `main.js` half is HF-085; this row additionally covers `casual-doc-wasm/src/lib.rs` at 26,374 lines |
| 73 | CQ-002 | Audit | The live editing path bypasses the transaction engine, so ADR-005 is not honoured in practice | P1 | L | Open → `107` §2.1 | 105 §2A | HF-111 | **The prerequisite for the whole collaboration lane** — blocks HF-011, HF-068, HF-114, RM-08, RM-09, and closes HF-045 and HF-107 on the way |
| 74 | FID-L-06 | Audit | The shaper's paragraph base level cannot be forced | P1 | L | Open | 105 §3.2 | — | Needs an upstream shaper API or pre-reordering |
| 75 | FID-L-08 | Audit | Vertical and rotated text is entirely absent | P1 | L | Open | 105 §3.2 | — | — |
| 76 | OO-001 | Audit | No table of contents, and no table of figures | P1 | L | Open | 105 §4.4 | — | Blocked-by RM-01 (the field evaluation engine), which sits later in this queue by the lane rule |
| 77 | OO-005 | Audit | No captions and no cross-references | P1 | L | Open | 105 §4.4 | — | Blocked-by RM-01 |
| 78 | OO-006 | Audit | No hyphenation, line numbering, watermark, or drop-cap authoring | P1 | L | Open | 105 §4.4 | FID-L-02, FID-L-10 | The authoring surface over the FID-L rendering rows |
| 79 | UX-007 | Audit | ⌘⇧E toggles Suggesting but is advertised nowhere | P2 | S | Open | 105 §2.2 | — | — |
| 80 | UX-008 | Audit | `insert.table` is two different products behind one command id | P2 | S | Open | 105 §2.2 | — | — |
| 81 | UX-013 | Audit | Print has no visible chrome | P2 | S | Open | 105 §2.2 | OO-010 | — |
| 82 | UX-014 | Audit | Menu taxonomy matches neither Word nor Docs | P2 | S | Open | 105 §2.2 | — | — |
| 83 | UX-023 | Audit | Invalid and mismatched ARIA on popup triggers | P2 | S | Open | 105 §2.3 | HF-070 | `aria-haspopup="region"` is not a valid token, so two buttons announce no popup at all |
| 84 | CQ-009 | Audit | Design documents no longer describe the implementation | P2 | S | Re-opened (#542) | 105 §2A | — | `63`/`64` drift is how the ARIA and radiogroup defects entered |
| 85 | FID-L-14 | Audit | Emphasis marks, outline, shadow, emboss, imprint and run borders are modeled and cascaded but unpainted | P2 | S | Partly fixed (#541) | 105 §3.2 | — | Remainder stated in the source row |
| 86 | FID-L-15 | Audit | No OpenType feature control, and the `w:kern` threshold is unapplied | P2 | S | Open | 105 §3.2 | OO-021 | — |
| 87 | FID-L-16 | Audit | `w:gutter` and `w:mirrorMargins` never reach the page configuration | P2 | S | Partly fixed (#536) | 105 §3.2 | — | Still open: `w:gutterAtTop` |
| 88 | FID-L-18 | Audit | `w:jc="distribute"` silently collapses to ordinary justification | P2 | S | Partly fixed (#541) | 105 §3.2 | — | The loss is now reported; real inter-character distribution is the remainder |
| 89 | UX-012 | Audit | No Table menu on the menu bar, and the palette hides table commands on complex tables | P2 | M | Open | 105 §2.2 | — | — |
| 90 | UX-015 | Audit | Single-surface capabilities — Pages panel, table style gallery, line/paragraph spacing, format painter, Settings | P2 | M | Partly fixed (#542) | 105 §2.2 | HF-094 | Still open: everything except the compact-ribbon toggle. Settings absent from the View ribbon is related to HF-159 |
| 91 | UX-024 | Audit | Focus and target-size gaps | P2 | M | Open | 105 §2.3 | HF-074 | Carries the visible focus indicator HF-074's skip-link fix left behind |
| 92 | FID-L-10 | Audit | Watermarks do not appear | P2 | M | Open | 105 §3.2 | OO-006 | — |
| 93 | FID-L-11 | Audit | `nextColumn` is treated as `continuous` | P2 | M | Open | 105 §3.2 | — | — |
| 94 | FID-L-12 | Audit | Tight/through wrap uses the square bounding box, not `wp:wrapPolygon` | P2 | M | Open | 105 §3.2 | — | — |
| 95 | FID-L-13 | Audit | Cell `noWrap`, `fitText`, `hideMark` and cell `textDirection` are unconsumed | P2 | M | Open | 105 §3.2 | — | — |
| 96 | FID-L-17 | Audit | `w:kinsoku` is cascaded and never consumed | P2 | M | Open | 105 §3.2 | — | — |
| 97 | FID-L-21 | Audit | `rich` and `table-merges` bottom edge diverges ~240 twips from LibreOffice | P2 | M | Open | 105 §3.2 | — | — |
| 98 | FID-R-05 | Audit | Retained opaque parts are never invalidated on edit | P2 | M | Open | 105 §3.3 | — | — |
| 99 | OO-008 | Audit | Table formulas are 4 functions over 2 directions; ONLYOFFICE has 18 over 4 | P2 | M | Open | 105 §4.4 | — | Blocked-by RM-01 for cell refs, ranges and bookmarks |
| 100 | OO-010 | Audit | No print dialog — no range, duplex, colour/mono, margins or preview | P2 | M | Open | 105 §4.4 | HF-030, HF-036, HF-105, UX-013 | Broader than the HF print rows; blocked-by RM-04 for real-text output. See re-verification: the split between this row and HF-030/HF-105 needs an owner call |
| 101 | OO-011 | Audit | No document protection, password, or digital signature | P2 | M | Open | 105 §4.4 | — | `106` Phase 7. The four restriction levels map onto the existing Editing/Suggesting/Viewing modes |
| 102 | OO-012 | Audit | No content-control authoring | P2 | M | Open | 105 §4.4 | — | `w:sdt` already models, round-trips and paints checkbox state |
| 103 | OO-020 | Audit | Ribbon and UI breadth rows worth copying cheaply | P2 | M | Open | 105 §4.4 | UX-003 | Blocked-by HF-085 (`commands.mjs`) |
| 104 | OO-021 | Audit | Specific Home and Insert controls absent here | P2 | M | Open | 105 §4.4 | FID-L-15 | — |
| 105 | CQ-008 | Audit | A dependency port is blocked at scale — 910 measured quick-xml 0.42 compile errors | P2 | L | Open (`M-009`) | 105 §2A | — | Do it with the Phase 2 parser reporting work, not separately |
| 106 | FID-R-08 | Audit | Charts, SmartArt and OLE are preserved but never drawn | P2 | L | Open | 105 §3.3 | OO-014 | Blocked-by Q3 (render vs preserve-and-disclose) |
| 107 | OO-007 | Audit | No document comparison or combine | P2 | L | Open | 105 §4.4 | — | `106` Phase 6.5 — the transform applied offline. Blocked-by RM-08 |
| 108 | OO-009 | Audit | No equation editor | P2 | L | Open | 105 §4.4 | — | `99` §2 requires an authority ADR first, so UI-only synthesis cannot silently replace unsupported math |
| 109 | OO-014 | Audit | Charts and SmartArt are not drawn, so there is nothing to author | P2 | L | Open | 105 §4.4 | FID-R-08, FID-L-04 | Blocked-by Q3 |
| 110 | UX-016 | Audit | Two mode controls with different labels for one state | P3 | S | Open | 105 §2.2 | — | — |
| 111 | FID-R-07 | Audit | Sub-part "retention" is not byte-exact | P3 | S | Open | 105 §3.3 | — | — |
| 112 | FID-L-19 | Audit | Character-grid snapping is not applied | P3 | M | Open | 105 §3.2 | — | — |
| 113 | OO-016 | Audit | AutoCorrect is smart quotes only — no math codes, no autoformat list triggers | P3 | M | Open | 105 §4.4 | HF-055 | Broader than HF-055, which is the smart-quote defect inside it |
| 114 | OO-019 | Audit | No freehand drawing (Draw tab) | P3 | M | Open | 105 §4.4 | — | Minimal even in ONLYOFFICE |
| 115 | FID-L-20 | Audit | EMF/WMF metafiles and browser-build SVG paint a placeholder | P3 | L | Open | 105 §3.2 | — | — |
| 116 | OO-013 | Audit | No mail merge | P3 | L | Open | 105 §4.4 | — | Needs a host data contract. Theirs is xlsx-only, portal-bound and 100-recipient capped |
| 117 | OO-017 | Audit | No plugin or macro surface | P3 | L | Open | 105 §4.4 | — | Open ABI decision; ADR-030 reserves the seam |
| 118 | Q2 | Roadmap | Decide how to acquire a rights-cleared **Word-produced** corpus | — | — | Open | 106 §9 | FID-P-02 | Gates Phase 0's exit, and therefore every fidelity claim. Recommendation on file: generate with a licensed copy, review for redistribution, keep sensitive documents local |
| 119 | Q6 | Roadmap | Decide the mobile commitment — fund it, or downgrade the support matrix | — | — | Open | 106 §9 | UX-018, UX-019 | `18` declares mobile/tablet browsers supported. Recommendation on file: fund it |
| 120 | Q1 | Roadmap | Decide the **D-6 embed contract** — what a host mounts, configures and receives | — | — | Open | 106 §9 | HF-109, CQ-010 | Gates Phase 4, which is the wedge. Recommendation on file: model the surface on `DocsAPI`, but local-first — no `callbackUrl`, no server-held key |
| 121 | Q4 | Roadmap | Decide **ADR-031** — PDF writer and font subsetter, build vs buy | — | — | Open | 106 §9 | RM-04 | On the critical path for a disqualifying gap. Decide early |
| 122 | Q3 | Roadmap | Decide charts and SmartArt scope — render, or preserve-and-disclose | — | — | Open | 106 §9 | OO-014, FID-R-08 | Recommendation on file: preserve-and-disclose for the v1 claim |
| 123 | Q7 | Roadmap | Decide the `.docm` policy — currently rejected at open, undecided | — | — | Open | 106 §9 | — | Recommendation on file: strip-and-open with an explicit finding; macros stay unexecuted |
| 124 | RM-01 | Roadmap | Field evaluation engine — host-provided evaluation context, recalculation, Update field, Toggle field codes | — | — | Open | 106 §6 Phase 3 | OO-001, OO-005, OO-008 | **Blocks the whole References tab.** ONLYOFFICE ships only 14 field codes, so the bar is low |
| 125 | RM-02 | Roadmap | Consolidate the editor-proven commands and errors into a versioned public SDK boundary | — | — | Open | 106 §6 Phase 4 | HF-109 | `99` order 5. Blocked-by Q1 and HF-085 |
| 126 | RM-03 | Roadmap | Host storage contract, in the opencalc shape | — | — | Open | 106 §6 Phase 4 | OO-004 | Owner decision 2026-09: storage YES |
| 127 | RM-04 | Roadmap | Real-text PDF export — a `casual-doc-pdf` backend transcribing the shared `DisplayList`, never rasterized | — | — | Open | 106 §6 Phase 5 | HF-030, OO-010 | Designed end to end in `98`. Blocked-by Q4 |
| 128 | RM-05 | Roadmap | Complete ODT beyond the current bounded subset | — | — | Open | 106 §6 Phase 5 | — | `95`/`96`/`97` |
| 129 | RM-06 | Roadmap | HTML, Markdown, EPUB and FB2 import/export; DOTX/OTT templates | — | — | Open | 106 §6 Phase 5 | HF-160 | RTF is carried separately as HF-160, which is already in flight. `106` §8 notes this set is the first thing to narrow if the schedule has to compress |
| 130 | RM-07 | Roadmap | Tagged PDF and PDF/A | — | — | Open | 106 §6 Phase 5 | RM-04 | Phased, per `98` |
| 131 | RM-08 | Roadmap | OT step 6.3 — T1 transform, tie-break by `(revision, site_id)`, TP1 property tests, the §4 budget benchmarks. No network | — | — | Open | 106 §6 Phase 6.3 | CQ-002 | Blocked-by CQ-002. Independently valuable: it is what makes OO-007 (compare/combine) fall out |
| 132 | RM-09 | Roadmap | OT step 6.4 — T2 anchor rebase and tombstoning with taxonomy reporting; T3 serialisation | — | — | Open | 106 §6 Phase 6.4 | FID-R-02 | Blocked-by RM-08. Every tombstone must be reported through the disposition taxonomy, never silently dropped |

## Rows that need re-verification

### Settled on 2026-09-20

**47 rows were re-read against the code they cite** — the 43 that were P1 when the sweep
started, plus HF-045, HF-085, HF-109 and HF-114, the four candidates for promotion —
because this document is now the only queue and a stale `Open` here is work nobody does.
Three closed and left the queue; two had their status or measurement corrected without
closing; the rest are still live exactly as stated. The five priority conflicts were
settled under the owner's authorisation to re-grade. Verdicts that would need a `105` edit
are under [Proposed further re-grades](#proposed-further-re-grades) instead, because this
document does not own `105`.

| Id | How it was settled |
| --- | --- |
| HF-016 | **Closed, removed from the queue.** The command is declared at `webapp/src/main.js:12468` (`{ id: "file.new", label: "New blank document", …, noDoc: true }`), implemented at `main.js:3261` (`newBlankDocument()`), and reachable from two surfaces — the File menu (`APP_MENU_SECTIONS.file`, `main.js:13005`) and the palette. `104` marked `Fixed (#542)`, matching `105` UX-011 |
| HF-022 | **Closed, removed from the queue.** The row's evidence sentence is no longer true of any of the three functions it names: `body_hit` (`casual-doc-wasm/src/lib.rs:1006`), `caret_rect` (`:2036`) and `selection_rects` (`:2052`) all build their snapshot from `painted_layout()` (`:11313`), and `view_pos`/`edit_pos` (`:11343`, `:11357`) map between the two byte spaces |
| HF-034 | **Closed, removed from the queue.** No `<label class="btn btn-primary file">` exists any more, and the remainder the row was held open for — the pre-document File menu — closed at `webapp/src/main.js:2630`, which unhides `#documentChrome` unconditionally at module scope. Open is now tab-reachable on a fresh editor from the File menu and the palette. Deliberate divergence: the header affordance was removed rather than rebuilt as a `<button>` |
| HF-085 / UX-003 | **Settled in favour of P1**, and moved to the head of the Hotfix P1 block. It blocks four rows; `106` Phase 1 agrees. Verified still fully open, with the measurement refreshed — 90.75%, not 93% |
| HF-109 / CQ-010 | **Settled in favour of P1**, ordered after HF-085. A P3 on the row `106` Phase 4 calls the wedge was the clearest grading error in the queue |
| HF-114 / OO-018 | **Settled in favour of P1**, ordered last in the P1 block because CQ-002 must unify the two op sets first |
| HF-051 / OO-015 | **Settled at P2**, between `104`'s P1 and OO-015's P3. Cheap, so it leads the M group of P2 |
| HF-088 | **Partly resolved.** A 700px rung now exists and applies (`style.css:4314`, `:4666`, `:4861`), so the swallow-the-page symptom is gone; the bottom-sheet behaviour is not built, so the row stays Partly fixed. The wrong 860px diagnosis in `104` is superseded rather than still open |
| `104` summary, P2 and Behavioural cells | **Corrected.** P2 now reads 23 and the behavioural section 4. The two errors were equal and opposite, so the old guard — which only checked the section column against the Total — was blind to both by construction. `tracker_counts.test.mjs` now asserts every section cell individually, and the `— N items` count in each heading, and was driven red against a re-introduced cancelling pair before being trusted |

### Still open for the owner

| Id | What needs settling | Why |
| --- | --- | --- |
| OO-010 vs HF-030 / HF-036 / HF-105 | Possible duplicate | OO-010 ("no print dialog") and the three HF print rows overlap on page range and progress. They are kept separate here because OO-010 also carries duplex, colour/mono and preview, which no HF row mentions. If the owner reads them as one piece of work, merge the HF rows into OO-010 |
| HF-132 | Blocked on an owner decision, not on engineering | Status is `Open (owner decision)` — a bundle-size and font-coverage call. It will sit in the queue forever until the decision is made |
| `106` phase tables | Cite rows that have since closed | Phases 0, 1 and 2 still list EV-001…EV-006, UX-001/UX-002, UX-010/UX-011 and FID-L-01/FID-L-03/FID-L-09 as work. They are closed. `106` is an archive now, so this is expected; do not work from its tables |

## Proposed further re-grades

Found during the 2026-09-20 verification sweep, **not applied**. Each needs an owner
decision, and the first two additionally need an edit to `105`, which this document does
not own — a `105` row that reads `Open` while `109` drops it would turn the coverage guard
red, correctly.

| Id | Proposal | Evidence |
| --- | --- | --- |
| FID-R-02 | **Propose Close.** The row's headline — "`ModelOutcome` is hardcoded `Omitted` at all three construction sites" — is no longer true | All nine `Disposition` variants are constructed (`casual-doc-import/src/report.rs:78-98`), retention is resolved per construct rather than per mode (`SourceRetention::resolve`, `:612`), `Degraded` is reached on the DOCX path via `report_attribute` from `body.rs:1892` and `theme.rs:326`, and the row's three "there is also no…" clauses are all built: `PreservationLedger` (`:254`), `LedgerId` (`:182`) and `CompatibilityReport::validate` (`:483`), called at `lib.rs:490` and `lib.rs:1277`. Only `Mapped` is still never constructed, and `report.rs:10-18` records that as deliberate — an import report enumerates only unrecovered meaning |
| FID-R-03 | **Propose Close.** The vocabulary the row says is missing exists and is used | `FeatureLocation.attribute` (`casual-doc-import/src/report.rs:157`), `FeatureLocation::attribute()` (`:172`) and `Reporter::report_attribute` (`:702`, whose doc comment cites FID-R-03 by name) emit `element/@attribute` as `Finding::Degraded`; callers at `body.rs:1892`, `:2568`, `:2572` and `theme.rs:326`, with `w:rsid*` given a bounded class identifier rather than being inexpressible |
| OO-006 | **Propose restating the row**, not re-grading it. Its headline ("no hyphenation, line numbering, watermark, or drop-cap **authoring**") is still true for all four clauses, but two of its sub-claims about the *renderer* have gone stale | Line numbering is now laid out and painted (`casual-doc-layout/src/line_number.rs`, wired at `document_layout.rs:885`, painted at `compose.rs:297`) and drop caps are laid out (`flow.rs:1104-1113`, `:1327-1347`). Hyphenation and watermarks have neither renderer nor UI |
| UX-005 | **Propose refreshing the measurement in place.** The defect stands; the number in the row's title does not | Re-counted 2026-09-20: **39 of 109** ribbon controls carry a command id, not "~23 of ~90". Insert, Layout, References and Review are stamped; Home (45), Table (19) and View (5) are not |
| CQ-003 | **Propose restating: two guards, not three** | The IME guard is fixed — `webapp/tests/e2e/ime-preedit.spec.mjs:41-45` now dispatches on `document.activeElement` and names CQ-003 in a comment. The command-surface and shortcut-coverage guards are both still structurally unable to fail |
| FID-L-05 | **Propose restating the citation.** The defect is real; the row's file reference is stale | Number format, start, restart and placement are genuinely consumed (`casual-doc-layout/src/note_numbering.rs:118-132`). What remains is the separator: synthesized from constants at `compose.rs:30-35`, and authored `w:separator`/`w:continuationSeparator` notes are skipped at import (`casual-doc-import/src/body.rs:5893`). The row cites `notes.rs:540 separator: None`, which is now an unrelated `SectionColumns` field |

## What was merged, and on whose authority

Ten rows do not appear above as rows of their own, because a source tracker states they
are the same work as a row that does. Each is named in the `Supersedes / see also` column
of the row that absorbed it, and the guard checks that every one of them is still reachable
from this document.

| Merged id | Into | The sentence that authorises it |
| --- | --- | --- |
| HF-076 | UX-004 | `104`: "Membership itself is superseded by docs/105 UX-004 … **Close under UX-004, not here**" |
| UX-003 | HF-085 | `105` UX-003: "**This is `HF-085`**, recorded here for the four UX consequences" |
| CQ-004 | UX-005 | CQ-004's status cell is literally "**Open → UX-005**" |
| CQ-005 | HF-081 | CQ-005's status cell is literally "**Open → UX-009**"; UX-009 in turn cross-refs `HF-081` as the i18n seam, and `104` stays authoritative where `105` restates an HF row |
| UX-009 | HF-025 | UX-009: "**Cross-ref `HF-025`, `HF-081`**" — and both trackers state that where `105` restates a `104` row, the HF row stays authoritative |
| CQ-010 | HF-109 | CQ-010's status cell is literally "**Open (`HF-109`, blocked on D-6)**" |
| OO-003 | HF-035 | OO-003's OpenDoc-state cell is "**Nothing. `HF-035`**", and its finding restates HF-035 word for word |
| OO-004 | HF-011 + HF-068 | OO-004's OpenDoc-state cell is "**Nothing. `HF-011`, `HF-068`**"; it names both halves, so it is recorded on both |
| OO-015 | HF-051 | OO-015's OpenDoc-state cell names "`HF-051`" and the two findings are the same sentence |
| OO-018 | HF-114 | OO-018's OpenDoc-state cell is "**Nothing … `HF-114`**", and the finding restates HF-114 word for word |

Rows that were **not** merged although they cite each other, because the citing row is
strictly broader: CQ-001 (adds the 26,374-line WASM file to HF-085), CQ-006 (adds the
browser matrix, axe and benchmarks to HF-090), UX-020 (adds caret anchoring and non-body
stories to HF-071), UX-023 (adds four ARIA defects to HF-070), UX-006 (adds ~15 chords to
HF-127), OO-002 (adds templates and the backstage IA to HF-016/HF-073), OO-010 (adds
duplex, colour and preview to HF-030/HF-036/HF-105), OO-016 (adds three AutoCorrect tabs to
HF-055), OO-006 (adds authoring UI to FID-L-02/FID-L-10), OO-014 (adds authoring to
FID-R-08/FID-L-04), and HF-160 (the RTF slice of RM-06).
