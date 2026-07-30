# 81 — Comments and Suggestions Completeness Audit

**Status:** Accepted gap inventory.
**Date:** 2026-07-31.
**Scope:** `casual-doc-wasm`, `casual-doc-edit`, semantic DOCX export,
the web editor's review sidebar and editing event paths, automated review
coverage, and the sibling `docs/docx-editor` interaction reference.

## Verdict

The current review workflow is useful, but it is **not production-complete and
must not be described as Word/Google Docs parity**.

The completed baseline is real: comments create/reply/resolve/reopen/delete and
round-trip with valid thread join ids; comment references are zero-width;
insert/delete/replacement suggestions are reviewable and undoable; the exposed
run-format commands enter the review model; cards live in a dedicated sidebar;
and imported tracked moves now pair and decide atomically.

The audit nevertheless found correctness, interoperability, editing, scale,
accessibility, and code-debt gaps. P1G-REVIEW-035 closed REVIEW-GAP-001,
REVIEW-GAP-002, and REVIEW-GAP-017. P1G-REVIEW-036 closed REVIEW-GAP-003 and
REVIEW-GAP-006 and completed the formatting-delta half of REVIEW-GAP-016.
Twenty-four gaps remain; the remaining P0 rows below are release blockers for
any claim of complete tracked-change support.

The sibling `docs/docx-editor` repository was used only as the requested
interaction reference. Its card composition, reply controls, resolved markers,
cursor-driven expansion, host-supplied author, and tracked-change threading are
useful patterns. Its own comment-sidebar E2E suite is currently marked `fixme`,
so this audit does not treat that implementation as a correctness oracle.

## Confirmed gaps

Classification:

- **Incorrect** — present behavior can produce invalid output, wrong content,
  or a misleading result.
- **Partial** — the feature exists but excludes ordinary editing cases or lacks
  a required end-to-end part.
- **Deferred** — intentionally not implemented and still visible in the
  accepted design.
- **Debt** — duplicate/stale implementation that increases regression risk.

| ID | Severity | Class | Confirmed finding and impact | Required correction |
|---|---|---|---|---|
| REVIEW-GAP-001 | Closed | Resolved | P1G-REVIEW-035 gives editor-authored inline revisions decimal `w:id` values and stores typed editor groups separately from serialized revision identity. Imported ids remain lossless. | Closed by doc 82, model separation, numeric export regression coverage, and release build verification. |
| REVIEW-GAP-002 | Closed | Resolved | P1G-REVIEW-035 replaces whole-body review snapshots with paragraph-scoped atomic operations, coalesces one suggestion-typing gesture into one Undo/Redo action, and caps each history stack at 256 actions. | Closed by scoped apply/inverse/rollback, retained-inverse, one-gesture Undo/Redo, and history-cap tests. |
| REVIEW-GAP-003 | Closed | Resolved | P1G-REVIEW-036 authors character-format suggestions as one text copy with current run properties plus the standard `w:rPrChange` prior snapshot. Formatting cards expose structured old/new deltas, and imported property changes use the same inventory and decision path. | Closed by one-copy model/layout checks, multi-run atomic Accept/Reject and Undo coverage, `w:rPrChange` export/reopen, imported decision coverage, release WASM, and browser formatting-card tests. |
| REVIEW-GAP-004 | P0 | Incorrect | Suggesting mode is a JavaScript flag and only `runToolbarEdit` enforces it. Visible table style/width/sort/merge/split/insert actions, Find/Replace, command-palette Insert Table/Restart List, document properties, and page setup have direct `runEdit`/`runNodeEdit` paths. They can silently mutate while the mode still says Suggesting. | Route every mutation through one mode-aware command dispatcher. Each command must be tracked, explicitly allowed as non-trackable metadata, or rejected before mutation. Add a command-matrix test. |
| REVIEW-GAP-005 | P0 | Incorrect | Comment highlights have pointer events and stop propagation. Clicking commented text selects the whole comment range and opens its card instead of placing the caret where the user clicked. This directly damages the normal editing experience. | Keep document hit-testing authoritative. Derive card expansion from the resulting caret/range; use a non-blocking margin affordance when an explicit comment target is needed. |
| REVIEW-GAP-006 | Closed | Resolved | P1G-REVIEW-036 defines one Final-with-markup UTF-8 byte space: insertion/move-destination content contributes; deletion/move-source content is zero-width while its review metadata remains. Layout, hit-testing, edit ranges, copy, rich copy, Find, statistics, Outline, links, bookmarks, notes/floats, comments, and review anchors share the closed projection predicate, including nested hidden revisions. Original and expanded All-Markup editing remain explicitly deferred presentation modes. | Closed at the audit's documented minimum by doc 83, Original/Final predicate tests, projected layout/hit/edit/query tests, collapsed deletion/comment/review-anchor tests, release WASM, and browser deletion-card coverage. |
| REVIEW-GAP-007 | P0 | Partial | Review split/wrap helpers operate on top-level `Run` nodes. Typing inside an existing pending insertion, deleting a selection spanning pending and normal text, formatting a pending suggestion, or editing through a revision/hyperlink/inline-SDT boundary fails instead of behaving like a mature editor. | Implement revision-aware range splitting and normalization across supported inline wrappers, with exact inverse operations and mixed accepted/pending-author tests. |
| REVIEW-GAP-008 | P1 | Partial | Rich paste in Suggesting mode is flattened to plain text; multi-paragraph paste is rejected. Cut copies to the clipboard before discovering that a cross-paragraph tracked delete is unsupported, so the command partially executes from the user's perspective. | Stage validation before clipboard mutation, preserve supported rich runs in tracked insertions, and represent paragraph insertion/deletion safely. |
| REVIEW-GAP-009 | P1 | Deferred | Paragraph breaks, cross-paragraph replacement/deletion, list level/toggle/restart, indent, paragraph formatting, tables, and other structure cannot be tracked. Several are blocked, while REVIEW-GAP-004 identifies paths that bypass the block. | Define structural revision operations in small slices; keep one complete allow/track/block matrix until then. |
| REVIEW-GAP-010 | P1 | Partial | Comment creation requires a non-empty, single-paragraph, top-level text range. Comments across paragraphs, on hyperlink text, on mixed inline content, at a collapsed caret, and on pending suggested text are unsupported. Imported comments without a resolved anchor are omitted from the sidebar entirely. | Generalize stable range markers across supported inline/container boundaries, define point comments, and expose orphan/unresolved imported comments with an explicit warning rather than hiding them. |
| REVIEW-GAP-011 | P1 | Partial | The accepted design promises editing one's own comment, but there is no update-comment engine method or UI. Replies cannot be edited or individually deleted from the active sidebar. | Add author-aware update/delete commands for root comments and replies, exact Undo, and DOCX reopen coverage. |
| REVIEW-GAP-012 | P1 | Partial | Tracked-change cards have no reply composer and comments overlapping a revision are not threaded beneath that change. The requested reference supports both interaction patterns. | Add comment-to-revision overlap mapping and suggestion reply threads without changing DOCX comment ownership semantics. |
| REVIEW-GAP-013 | P1 | Incorrect | There is no host identity API. Authoring reads an input inside the permanently hidden legacy Review panel and falls back to `"You"`. Identity, initials, stable color, permissions, and mentionable users are not host-supplied as required by the design. | Add an explicit session/host identity configuration seam; remove the hidden DOM input as policy storage. |
| REVIEW-GAP-014 | P1 | Partial | The design specifies Editing/Suggesting/Viewing, but `setReviewMode` collapses every non-suggesting value to Editing. Viewing mode and read-only enforcement do not exist. | Implement a real three-state mode with command-level read-only enforcement, reflection, keyboard access, and tests. |
| REVIEW-GAP-015 | P1 | Partial | All authors use the same green/red overlay colors, avatars show only one letter, and inline changes have no author/date tooltip. Multiple reviewers are visually indistinguishable. | Deterministically assign accessible per-author colors, show initials and attribution tooltip, and test contrast in light/dark themes. |
| REVIEW-GAP-016 | P1 | Partial | P1G-REVIEW-036 closes the formatting half: cards now list structured before/after bold, italic, underline, strike, font, size, color, highlight, and vertical-alignment deltas. Move cards still identify generic original/new locations but provide no separate source/destination navigation controls. | Add precise, keyboard-accessible navigation for both ends of a move. |
| REVIEW-GAP-017 | Closed | Resolved | P1G-REVIEW-035 uses typed groups and validates membership, kind, author/date, top-level placement, paragraph scope, contiguity, and formatting text equivalence before atomic decisions. | Closed by incomplete, mixed-kind, and cross-paragraph fail-closed regressions. |
| REVIEW-GAP-018 | P1 | Deferred | Next/Previous, Accept All/Reject All, Open/Resolved/All filters, and a fuller Review surface are not part of the active UI. Hidden legacy controls still exist, which makes the deferral ambiguous. | Design the Review surface, then expose only supported controls. Until then remove unreachable controls and keep the deferral explicit. |
| REVIEW-GAP-019 | P1 | Partial | Resolved comments remain in the main stream with no active filter. The sidebar auto-opens for any review item and has no in-sidebar header/close control. Cursor navigation into a comment does not drive expansion; only overlay/card clicks do. | Add explicit sidebar state, resolved visibility/filter behavior, close/header affordance, and caret-driven single-card expansion. |
| REVIEW-GAP-020 | P1 | Incorrect | Every scroll-frame rebuilds every card, reparses the full review JSON, calls range geometry for all items, and replaces the sidebar DOM. There is no virtualization or retained item model. Documents with many revisions will jank, allocate heavily, and repeatedly discard focusable DOM. | Cache review inventory by model revision, retain keyed card DOM/state, virtualize offscreen items, and add 100/1,000-item scroll/edit performance gates. |
| REVIEW-GAP-021 | P1 | Partial | Stable-anchor behavior under insertion/deletion at comment boundaries, full deletion of commented text, paragraph joins/splits, table edits, accept/reject near comments, and imported malformed marker sets is largely untested. | Define anchor transformation policy and add mutation matrices plus export/reopen fixed-point tests. |
| REVIEW-GAP-022 | P1 | Debt | The design promises `listComments`, `listRevisions`, and `commentThread`; the implementation exposes one untyped JSON string through `reviewSummary`. Public SDK policy, compatibility, and error behavior are undocumented. | Finalize a typed, bounded query API or revise the design explicitly; add API tests and SDK documentation. |
| REVIEW-GAP-023 | P2 | Partial | Review cards are focusable generic `article` elements with button-like key handlers but no button role; the sidebar lacks a labelled header/close control; frequent DOM replacement can drop focus; status/error announcements are not review-specific live regions. | Complete keyboard, focus-retention, screen-reader, and high-contrast audits with automated accessibility smoke. |
| REVIEW-GAP-024 | P2 | Partial | The sidebar remains a fixed 300px at narrow widths and competes with the paged canvas. There is no mobile/tablet review presentation or touch-specific card/anchor behavior. | Define a breakpoint-specific drawer/sheet or review mode and test touch selection plus card actions. |
| REVIEW-GAP-025 | P2 | Debt | Review strings, dates, labels, error messages, and card verbs are hard-coded in English. | Route review copy through the editor's localization boundary and test long translated strings. |
| REVIEW-GAP-026 | P2 | Debt | The obsolete Review side panel, composer, filters, bulk controls, popover builder, card builder, styles, and event listeners remain in the shipped page even though the panel never opens. The hidden author input is still live state. | Delete the legacy implementation after the host identity seam lands; retain one review renderer and one action path. |
| REVIEW-GAP-027 | P1 | Partial | Automated coverage is narrow: no Word/LibreOffice open-save oracle for editor-authored suggestions; no schema validation of exported revisions; no suggested insertion/deletion export/reopen test; no comment-edit test; no mixed-revision editing matrix; no large-review performance test; and no review accessibility or narrow-viewport gate. | Add gates per remediation slice and record them in doc 15 before claiming completion. |
| REVIEW-GAP-028 | P2 | Debt | Doc 68 still describes pre-implementation “current state,” operation variants that were not built, query methods that do not exist, a small canvas composer contradicted by the sidebar correction, and verification gates that have not all run. Tracker rows 030–033 overstate completeness. | Rewrite the durable design around the implementation actually chosen and keep Done claims scoped to tested behavior. |
| REVIEW-GAP-029 | P2 | Deferred | @mentions, assignees, reactions, presence, notifications, collaborative conflict policy, and reviewer permissions remain host-owned future work. | Define host callbacks and policy only when a collaboration product slice is scheduled; do not bake a provider into the runtime. |

## Paired tracked moves closed in P1G-REVIEW-034

The move-specific deferral found before this audit is closed:

- `MoveFrom` and `MoveTo` are correlated through their shared move-range name,
  not their unrelated revision ids.
- The decision token contains the exact source/destination start-marker node ids.
- Ambiguous duplicate names and incomplete pairs are not guessed into a pair.
- One card highlights both locations.
- Accept keeps the destination and removes the source; Reject does the inverse.
- Both decisions remove all consumed range markers, form one Undo action, and
  survive export/reopen without orphan move markup.
- Accept All/Reject All uses pair-safe processing and rejects incomplete moves.

## Recommended execution order

1. **P1G-REVIEW-036 — Review projection and formatting representation
   (completed).** Closed REVIEW-GAP-003/006 with one documented
   Final-with-markup editing projection; Original/expanded All-Markup view
   switching remains a separately designed presentation feature.
2. **P1G-REVIEW-037 — Mode-safe command routing and editing coverage.**
   Fix REVIEW-GAP-004/005/007/008/009/010/014 with a tested command matrix.
3. **P1G-REVIEW-038 — Complete comment/reviewer workflow.**
   Fix REVIEW-GAP-011/012/013/015/016/019/021/022 and define host policy.
4. **P1G-REVIEW-039 — Scale, accessibility, cleanup, and interoperability.**
   Fix REVIEW-GAP-020/023/024/025/026/027/028 and run real-consumer oracles.

Completion means every remaining row is either closed with a test or retained as
an explicitly accepted product non-goal. A working happy path is not sufficient.
