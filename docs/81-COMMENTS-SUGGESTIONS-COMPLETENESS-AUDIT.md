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
P1G-REVIEW-041 closed REVIEW-GAP-005. P1G-REVIEW-042 closed REVIEW-GAP-004's
mode-bypass, though it only blocks the affected commands in Suggesting mode —
REVIEW-GAP-009's structural-tracking backlog those commands still lack
remains open. P1G-REVIEW-043 closed REVIEW-GAP-022 with first-class typed
`listComments`/`listRevisions`/`commentThread` query methods, replacing the
untyped combined `reviewSummary` blob as the primary review query surface.
The `setActiveAuthor`/`activeAuthor` host identity seam closed REVIEW-GAP-013.
P1G-REVIEW-046 closed REVIEW-GAP-014 with a real read-only Viewing mode that
fails every document mutation closed at the shared command entry points.
REVIEW-GAP-008's collapsed-caret single-paragraph rich-paste slice narrows it
but does not close it; verifying that fix also surfaced a new toolbar
format-reflection gap, REVIEW-GAP-030. P1G-REVIEW-047 closed REVIEW-GAP-011
with `updateComment`/`deleteReply` engine methods plus sidebar controls, and
advanced REVIEW-GAP-012 by adding the `revisionThread` comment-to-revision
overlap mapping (a first-class reply composer on a change, and commenting on
zero-width deletion content, remain deferred). P1G-REVIEW-049 closed
REVIEW-GAP-016 by adding keyboard-accessible per-end navigation with precise
page labels to tracked-move cards, on top of the structured formatting deltas
P1G-REVIEW-036 already delivered. Nineteen gaps remain; the remaining P0 rows
below are release blockers for any claim of complete tracked-change support.
format-reflection gap, REVIEW-GAP-030, which P1G-REVIEW-049 then closed by
making the toolbar-reflection queries descend into pending tracked revisions.
P1G-REVIEW-047 closed REVIEW-GAP-011 with `updateComment`/`deleteReply` engine
methods plus sidebar controls, and advanced REVIEW-GAP-012 by adding the
`revisionThread` comment-to-revision overlap mapping (a first-class reply
composer on a change, and commenting on zero-width deletion content, remain
deferred). Nineteen gaps remain; the remaining P0 rows below are release
blockers for any claim of complete tracked-change support.

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
| REVIEW-GAP-004 | Closed | Resolved | P1G-REVIEW-042 factors `runToolbarEdit`'s fail-closed Suggesting-mode gate into a shared `blockUntrackedInSuggesting()` check and applies it to every direct `runEdit`/`runNodeEdit` mutation path that previously had none: table style/width/properties/formula/sort/merge/split/insert/row/column/delete, ribbon and command-palette Insert Table, ribbon and command-palette Restart Numbering, Find/Replace apply (single and All), document properties, page setup, and a rich-paste fallback that could reach `pasteRichRuns` untracked. `runNodeEdit` now fails closed unconditionally (every call site is a table/list structural op). The concurrent context-aware-menus work already disabled the equivalent right-click table commands under Suggesting via a UI `enabled` flag; the same mutation-layer gate was added to those `run` closures too, so the block does not depend solely on a menu item staying disabled. This closes the *bypass*, not the underlying tracking gap: none of these commands gained a tracked-revision representation, so they are blocked outright in Suggesting mode with the existing status message rather than applied. That remaining work is REVIEW-GAP-009's structural-tracking backlog, still open. | Closed by a single shared gate reused across every mutation path, `runNodeEdit` failing closed by default, and a dedicated Playwright command-matrix regression (`suggesting-mode-gate.spec.mjs`). |
| REVIEW-GAP-005 | Closed | Resolved | P1G-REVIEW-041 removes the marker's click handler entirely; `onPointerDown`'s hit-test is the only path that ever sets `selection`, so clicking on or inside commented text places the caret exactly like clicking anywhere else. Card expansion/highlight is now derived from the resulting caret (`syncActiveReviewCommentToCaret`) as a non-blocking side effect, never the other way around. | Closed by removing the marker click listener/`stopPropagation`, deriving card expansion from the post-hit-test caret, and a Playwright regression that clicks a specific offset inside a commented range and asserts the caret (not a full-range selection) lands there. |
| REVIEW-GAP-006 | Closed | Resolved | P1G-REVIEW-036 defines one Final-with-markup UTF-8 byte space: insertion/move-destination content contributes; deletion/move-source content is zero-width while its review metadata remains. Layout, hit-testing, edit ranges, copy, rich copy, Find, statistics, Outline, links, bookmarks, notes/floats, comments, and review anchors share the closed projection predicate, including nested hidden revisions. Original and expanded All-Markup editing remain explicitly deferred presentation modes. | Closed at the audit's documented minimum by doc 83, Original/Final predicate tests, projected layout/hit/edit/query tests, collapsed deletion/comment/review-anchor tests, release WASM, and browser deletion-card coverage. |
| REVIEW-GAP-007 | P0 | Partial | Review split/wrap helpers operate on top-level `Run` nodes. Typing inside an existing pending insertion, deleting a selection spanning pending and normal text, formatting a pending suggestion, or editing through a revision/hyperlink/inline-SDT boundary fails instead of behaving like a mature editor. | Implement revision-aware range splitting and normalization across supported inline wrappers, with exact inverse operations and mixed accepted/pending-author tests. |
| REVIEW-GAP-008 | P1 | Partial | Rich paste at a collapsed caret, single paragraph, in Suggesting mode now chains one tracked `suggestStyledInsert` per clipboard run under one gesture (`webapp/src/main.js` `pasteTrackedRichRuns`), so pasted bold/italic/underline/strike/color/highlight/size/font survive as one review card and one Undo step instead of flattening to plain text. Still flattened/rejected, unchanged: a rich paste that also replaces an existing selection (no tracked multi-run *replacement* group exists — `RevisionGroupKind::Replacement` requires exactly one deletion plus one insertion) falls back to one flattened plain-format tracked replacement; multi-paragraph paste is still rejected outright (REVIEW-GAP-009's structural-tracking backlog). A pasted run's hyperlink (`href`) is not carried into the tracked insertion either, same as before. Cut still copies to the clipboard before discovering that a cross-paragraph tracked delete is unsupported, so the command partially executes from the user's perspective — unchanged, not in this slice. | Stage validation before clipboard mutation, and represent paragraph insertion/deletion safely, remain open. Extending the tracked-replacement group model to support multiple insertion runs (for a rich paste that both replaces a selection and preserves formatting) is future work. |
| REVIEW-GAP-009 | P1 | Deferred | Paragraph breaks, cross-paragraph replacement/deletion, list level/toggle/restart, indent, paragraph formatting, tables, and other structure cannot be tracked. All are now blocked in Suggesting mode rather than silently applied (P1G-REVIEW-042 closed REVIEW-GAP-004's remaining bypasses), but none of it is actually trackable yet. | Define structural revision operations in small slices; keep one complete allow/track/block matrix until then. |
| REVIEW-GAP-010 | P1 | Partial | Comment creation requires a non-empty, single-paragraph, top-level text range. Comments across paragraphs, on hyperlink text, on mixed inline content, at a collapsed caret, and on pending suggested text are unsupported. Imported comments without a resolved anchor are omitted from the sidebar entirely. | Generalize stable range markers across supported inline/container boundaries, define point comments, and expose orphan/unresolved imported comments with an explicit warning rather than hiding them. |
| REVIEW-GAP-011 | Closed | Resolved | P1G-REVIEW-046 adds `updateComment(id, text)` (edits a thread root's or any reply's body text in place, preserving its `para_id`/`parent_para_id` threading, resolved state, author identity, and anchor markers) and `deleteReply(id)` (removes one reply and its nested sub-replies, fails closed on a thread-root id so a whole anchored thread is never dropped by mistake — `deleteComment` still owns thread deletion). Both are one undoable review action; the reply edit survives DOCX export/reopen. The primary review sidebar gains per-reply Edit/Delete controls. Residuals are host-owned or separately tracked, not this row: reviewer edit *permission* ("edit your own") is host policy per the identity seam (REVIEW-GAP-029), and editing rewrites a comment body to a single paragraph (imported multi-paragraph note bodies collapse on edit). | Closed by the `updateComment`/`deleteReply` engine methods, six native unit tests (reply edit + reopen, root edit + Undo, empty/unknown-id rejection, single-reply delete keeping root+siblings, nested-reply cascade, root-id rejection), the sidebar Edit/Delete wire-up, and a Playwright reply edit/delete regression. |
| REVIEW-GAP-012 | P1 | Partial | P1G-REVIEW-046 adds the overlap-mapping half: `revisionThread(revisionId)` returns the comments whose anchor ranges overlap a tracked change in the shared final-with-markup byte space, and the review sidebar threads those comments (read-only) beneath an expanded revision card, so a comment on changed text is visibly associated with its change. This changes no DOCX comment-ownership semantics — it invents no comment↔revision model link, only reports where existing anchor ranges coincide. Still deferred: a dedicated reply composer *on* a tracked-change card (today, replying to a change is `addComment` over the change's own anchor range, then `revisionThread` surfaces it), and commenting on a pure deletion, whose content is zero-width in the final-with-markup projection and so cannot carry a new same-range comment anchor. | Deferred remainder: a first-class suggestion reply composer and a comment anchor over zero-width deletion content, both without changing DOCX comment ownership semantics. |
| REVIEW-GAP-013 | Closed | Resolved | `WasmDocument::setActiveAuthor(name, initials?, id?)`/`activeAuthor()` give the host an explicit, typed identity seam (docs/68 §"Host identity seam"): `addComment`, `replyToComment`, and every `suggest*` tracked-change call now fall back to the active identity when their own `author`/`initials` argument is omitted, an explicit per-call value still overrides it, and a blank name clears it. The demo webapp's Settings panel exposes real Name/Initials fields wired to this API; the hidden legacy `#reviewAuthor` input inside the permanently-hidden Review panel is deleted, not left as a second path. Stable color, permissions, and mentionable users remain host-owned future work (REVIEW-GAP-015, REVIEW-GAP-029). | Closed by the `setActiveAuthor`/`activeAuthor` WASM API, its Rust unit tests, the webapp identity settings UI replacing the hidden input, and Playwright coverage that changing the host identity changes a newly created comment's attribution without rewriting earlier comments. |
| REVIEW-GAP-014 | Closed | Resolved | `setReviewMode` now models all three modes (`editing`/`suggesting`/`viewing`); `viewing` is fully read-only per docs/68 ("no Operation reaches apply"). A single shared `blockMutationInViewing()` gate — mirroring `blockUntrackedInSuggesting()` — fails closed at the three mutation entry points (`runEdit`, `runToolbarEdit`, `runNodeEdit`), so every mutating path (typing, backspace/delete, Enter, paste, IME commit, toolbar/paragraph formatting, table ops, list ops, undo/redo, and comment/revision decisions) is blocked without depending on any individual command or menu item being disabled. `cut`/`paste` also fail closed up front so no clipboard side effect or partial execution occurs. The visible ribbon control is a three-button segmented group (`#reviewModeControl`, `data-review-mode="editing|suggesting|viewing"`), auto-wired by the existing `reviewModeButtons` machinery; a read-only banner (`#viewingBanner`) and the ⌘⇧E cycle (Editing→Suggesting→Viewing) give keyboard access. Navigation, selection, scroll, Find (search only), and copy remain available. | Closed by the three-state `setReviewMode`, the shared read-only mutation gate at every entry point, the segmented control + banner + keyboard cycle, and Playwright coverage (`viewing-mode-gate.spec.mjs`) proving typing/deletion/paste/formatting/table-insert are blocked with the document unchanged while selection and copy still work. Per-author color and richer review-decision affordances remain other rows (REVIEW-GAP-015/018). |
| REVIEW-GAP-015 | Mostly closed | Resolved (color + tooltip) | P1G-REVIEW-048 assigns each distinct author a deterministic, stable color from a fixed 10-hue palette (keyed by a hash of the author's name — the stable identity the projection exposes; webapp presentation only, never persisted, per docs/68 §50) and applies it to the inline insertion/deletion **and** comment markers and the sidebar card's avatar chip, so overlapping reviewers are now visually distinguishable and one author renders in one consistent color across their changes and comments. A native attribution tooltip (author · change-type · date for a revision; author · state · date for a comment) shows on hover of both the inline marker and its sidebar card, reusing the editor's existing `title` tooltip pattern. Degrades cleanly: a single author looks unchanged-clean, and an unattributed change falls back to "You" with a neutral grey. **Still open:** multi-letter avatar initials beyond the single first letter, and an automated light/dark contrast gate (the palette is chosen for legibility over the white canvas and as a white-on-color chip in both themes, but contrast is not yet asserted by a test). | Remaining: multi-character initials and an automated contrast check in light/dark themes. |
| REVIEW-GAP-016 | Closed | Resolved | P1G-REVIEW-036 closed the formatting half (cards list structured before/after bold, italic, underline, strike, font, size, color, highlight, and vertical-alignment deltas). P1G-REVIEW-049 closes the move half: a move card's "From" and "To" ends are now keyboard-focusable `<button>` navigation controls that each carry a precise location (the page that end sits on, from the same range geometry the sidebar uses) instead of a static "Original/New location" label, and activating either one jumps the caret to that end (source and destination resolve to their own distinct anchors) and scrolls it to centre without toggling the card. | Closed by structured formatting deltas (P1G-REVIEW-036) and per-end move navigation with precise page labels (P1G-REVIEW-049). |
| REVIEW-GAP-017 | Closed | Resolved | P1G-REVIEW-035 uses typed groups and validates membership, kind, author/date, top-level placement, paragraph scope, contiguity, and formatting text equivalence before atomic decisions. | Closed by incomplete, mixed-kind, and cross-paragraph fail-closed regressions. |
| REVIEW-GAP-018 | P1 | Deferred | Next/Previous, Accept All/Reject All, Open/Resolved/All filters, and a fuller Review surface are not part of the active UI. Hidden legacy controls still exist, which makes the deferral ambiguous. | Design the Review surface, then expose only supported controls. Until then remove unreachable controls and keep the deferral explicit. |
| REVIEW-GAP-019 | P1 | Partial | Resolved comments remain in the main stream with no active filter. The sidebar auto-opens for any review item and has no in-sidebar header/close control. Cursor navigation into a comment does not drive expansion; only overlay/card clicks do. | Add explicit sidebar state, resolved visibility/filter behavior, close/header affordance, and caret-driven single-card expansion. |
| REVIEW-GAP-020 | P1 | Partial | P1G-REVIEW-046 removed the per-scroll-frame rebuild: the comment column now shares the canvas's single scroll context (Google Docs model), so scrolling no longer re-parses the review JSON, recomputes geometry, or replaces the sidebar DOM — cards ride the canvas natively and only content edits/resizes re-render. Still open: a retained item model (edits and resizes still `replaceChildren` the whole list, reparse the JSON, and recompute all geometry) and offscreen virtualization; large-review documents still allocate heavily and discard focusable DOM on every edit. | Cache review inventory by model revision, retain keyed card DOM/state, virtualize offscreen items, and add 100/1,000-item scroll/edit performance gates. |
| REVIEW-GAP-021 | P1 | Partial | Stable-anchor behavior under insertion/deletion at comment boundaries, full deletion of commented text, paragraph joins/splits, table edits, accept/reject near comments, and imported malformed marker sets is largely untested. | Define anchor transformation policy and add mutation matrices plus export/reopen fixed-point tests. |
| REVIEW-GAP-022 | Closed | Resolved | P1G-REVIEW-043 adds first-class `listComments`, `listRevisions`, and `commentThread` methods to `casual-doc-wasm`, each backed by a named `serde`-derived Rust struct (not an ad hoc JSON literal) with a documented, additive-only field set; `reviewSummary` remains unchanged for existing callers. `commentThread` resolves any thread member to its root and returns root-first ordered replies, erring on an unknown/invalid id. | Closed by typed-schema WASM tests cross-checked against `reviewSummary`'s legacy output, a nested reply-to-a-reply thread-resolution test, an invalid/unknown-id error test, `webapp/src/main.js` calling the typed methods directly, and the full Playwright `review-margin` suite exercising the rewritten call sites. |
| REVIEW-GAP-023 | P2 | Partial | Review cards are focusable generic `article` elements with button-like key handlers but no button role; the sidebar lacks a labelled header/close control; frequent DOM replacement can drop focus; status/error announcements are not review-specific live regions. | Complete keyboard, focus-retention, screen-reader, and high-contrast audits with automated accessibility smoke. |
| REVIEW-GAP-024 | P2 | Partial | The sidebar remains a fixed 300px at narrow widths and competes with the paged canvas. There is no mobile/tablet review presentation or touch-specific card/anchor behavior. | Define a breakpoint-specific drawer/sheet or review mode and test touch selection plus card actions. |
| REVIEW-GAP-025 | P2 | Debt | Review strings, dates, labels, error messages, and card verbs are hard-coded in English. | Route review copy through the editor's localization boundary and test long translated strings. |
| REVIEW-GAP-026 | P2 | Debt | The obsolete Review side panel, composer, filters, bulk controls, popover builder, card builder, and styles remain in the shipped page even though the panel never opens. The host identity seam has landed (REVIEW-GAP-013 is closed) and the legacy hidden author input inside this panel is deleted, but the rest of the dead panel/composer/listeners remain. | Delete the remaining legacy implementation now that the host identity seam has landed; retain one review renderer and one action path. |
| REVIEW-GAP-027 | P1 | Partial | Automated coverage is narrow: no Word/LibreOffice open-save oracle for editor-authored suggestions; no schema validation of exported revisions; no suggested insertion/deletion export/reopen test; no comment-edit test; no mixed-revision editing matrix; no large-review performance test; and no review accessibility or narrow-viewport gate. | Add gates per remediation slice and record them in doc 15 before claiming completion. |
| REVIEW-GAP-028 | P2 | Debt | Doc 68 still describes pre-implementation “current state,” operation variants that were not built, query methods that do not exist, a small canvas composer contradicted by the sidebar correction, and verification gates that have not all run. Tracker rows 030–033 overstate completeness. | Rewrite the durable design around the implementation actually chosen and keep Done claims scoped to tested behavior. |
| REVIEW-GAP-029 | P2 | Deferred | @mentions, assignees, reactions, presence, notifications, collaborative conflict policy, and reviewer permissions remain host-owned future work. | Define host callbacks and policy only when a collaboration product slice is scheduled; do not bake a provider into the runtime. |
| REVIEW-GAP-030 | Closed | Resolved | P1G-REVIEW-049 makes the two toolbar-reflection queries in `casual-doc-edit` — `run_properties_in_range` (a selection) and `caret_run_properties` (a collapsed caret) — descend into final-with-markup-contributing `Revision`/`Hyperlink`/`Sdt` wrappers via a shared `flatten_run_segments`, the same way `copyRichRuns`'s `walk_inlines_rich` already walks, so `format_state`/`caret_format` (and the `selectionFormat`/`formatAt` WASM path built on them) now see a run inside a pending tracked insertion. The Home ribbon reflects bold/italic/etc. for suggested text where it previously stayed unpressed. The editing/split paths keep using top-level `run_segments` because revision-aware run splitting is separate work (REVIEW-GAP-007). | Closed by `flatten_run_segments`, a `casual-doc-edit` regression (`format_state`/`caret_format` bold inside a pending insertion + mixed-selection sanity), a `casual-doc-wasm` `effective_run_properties_in_range` regression, and a Playwright test asserting `#bold` reflects a selection and caret inside a suggested bold run. |

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
   Fix the remaining REVIEW-GAP-007/008/010/014 with a tested command matrix
   (REVIEW-GAP-005 was closed separately by P1G-REVIEW-041; REVIEW-GAP-004's
   mode-bypass half is closed by P1G-REVIEW-042; REVIEW-GAP-009's
   structural-tracking backlog stays open).
3. **P1G-REVIEW-038 — Complete comment/reviewer workflow.**
   Fix REVIEW-GAP-011/012/013/015/016/019/021 and define host policy.
   REVIEW-GAP-022 was closed separately by P1G-REVIEW-043; REVIEW-GAP-013 by
   the host identity seam; REVIEW-GAP-011 by P1G-REVIEW-046, which also
   advanced REVIEW-GAP-012's overlap-mapping half.
4. **P1G-REVIEW-039 — Scale, accessibility, cleanup, and interoperability.**
   Fix REVIEW-GAP-020/023/024/025/026/027/028 and run real-consumer oracles.

Completion means every remaining row is either closed with a test or retained as
an explicitly accepted product non-goal. A working happy path is not sufficient.
