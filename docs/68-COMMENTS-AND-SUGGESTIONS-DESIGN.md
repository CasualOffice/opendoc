# 68 — Comments and Suggestions (Track Changes) Editor UX Design

**Status:** Accepted; partially implemented through paired tracked-move review
(P1G-REVIEW-034). Remaining production gaps are tracked in doc 81.
**Date:** 2026-07-30.
**Depends on:** `v1::Definitions.comments`/`people` (Done, `P1A-031`/`P1F-10`/`P1F-35`), `v1::InlineNode::Revision` (Done, import/export passthrough only), `casual-doc-edit` op set (doc 59), interaction architecture (doc 58), extensibility seams I1–I4 (doc 45), design system (doc 63), toolbar/ribbon design (doc 64). Referenced by `docs/67-EDITOR-UX-GAP-ANALYSIS.md` row "Comments/revisions."

## Problem

Comments and tracked changes ("suggestions") are fully modeled and round-trip through import/export at the DOCX level, but the editor has **zero** authoring surface for either: no way to add/reply/resolve a comment, no "Suggesting mode," no accept/reject, no rendering of an anchored comment range or a tracked insertion/deletion. Doc 67 flags this explicitly and gates implementation behind a dedicated design: *"Design review data model and host policy first; do not fake tracked-changes support."* This doc is that gate.

Comments and suggestions are related (both are range-anchored, both are review workflows, both show in a right-hand review surface) but are **separable**: the comment data model is complete today, while tracked changes have no edit-time authoring support at all (revisions are currently pure passthrough — nothing in `casual-doc-edit` creates one). This doc designs both together for a coherent review experience, but stages them as independent implementation slices so comments can ship without waiting on the harder suggesting-mode work.

## Reference Baseline

Per doc 67's own convention of anchoring UX decisions to the two products opendoc targets parity with:

**Google Docs** — a top-right mode switch (Editing / Suggesting / Viewing). In Suggesting mode, edits render inline as colored, underlined insertions and colored strikethrough deletions, attributed to the suggester; each suggestion and each comment appears as a card in a right-margin gutter, vertically stacked near its anchor, connected by a highlight on the text. Cards support reply threads, resolve (checkmark, greys the highlight and collapses the thread into a "resolved" filter), reopen, and accept/reject (for suggestions). Comment creation is a "+"/bubble affordance on selection.

**Microsoft Word** — a Review ribbon tab: a Track Changes toggle, markup-view modes (Simple Markup / All Markup / No Markup / Original), and Show Markup filters (Insertions and Deletions / Comments / Formatting; Balloons vs. inline). Tracked insertions/deletions render inline in a per-author color (underline/strikethrough) or collapse to a clickable change bar in the margin under Simple Markup. Comments show as margin balloons or in a unified Reviewing Pane list. Accept/Reject work per-change (right-click, or ribbon Accept▼/Reject▼ with This Change/All Shown/All) plus Next/Previous navigation. Each distinct author gets a stable, auto-assigned color shown in a hover tooltip with name/date/description.

Both converge on the same shape: inline colored markup for the change itself, a right-hand list/gutter for comments and suggestions as discrete reviewable items, resolve/accept/reject as the terminal actions, and per-author color/identity as the attribution mechanism. This doc adopts that shape rather than inventing a new one.

## Goals

- Comments: create, reply, edit own comment, resolve/reopen, delete; anchored to a stable text range that survives edits; rendered as a highlighted span plus a right-panel thread list, matching the model doc 63 already reserves (region 4, "Comments — needs a comments model — a later phase").
- Suggestions: a Suggesting-mode toggle that routes ordinary text edits through tracked-insert/tracked-delete instead of direct mutation; inline rendering of pending insertions/deletions by author; accept/reject per change.
- Both features route exclusively through `casual_doc_edit::apply` (I1), extend the `Operation` enum with new, exactly-invertible variants (I2), anchor via `NodeId`/`Pos` using the same live-tree-walk resolution as bookmarks (I3), and keep all identity/presence data host-supplied (I4) — the engine never invents an author, a color, or a network identity.
- No stubbed/disabled UI: per doc 63's governing principle, ship a slice only when it is fully functional end to end.

## Non-Goals (this doc)

- Real-time multi-user collaboration transport, presence cursors, or conflict resolution — host-owned per I4/doc 45; this doc only defines the data/identity seam collaboration would plug into.
- Notification/email delivery for @mentions, assignee workflows, or approval routing.
- Legal redlining / compare-two-documents mode.
- Full Word ribbon "Review" tab (markup-view modes, Show Markup filters, Accept-All/Reject-All) — the first suggesting-mode slice ships a minimal Docs-style toggle; ribbon depth is a follow-up once usage warrants the extra chrome.

## Current State Recap

- **Comments** (`crates/casual-doc-model/src/v1/definitions.rs:744-793`): `Comment { blocks, author, initials, date, para_id, parent_para_id, done, durable_id, person }`, plus `Definitions.people: Vec<Person>`. Anchored by three inline markers (`v1/body.rs:1163-1195`): `CommentRangeStart{id, comment}`, `CommentRangeEnd{id, comment}`, `CommentReference{id, comment}`. Threading (`para_id`/`parent_para_id`) is a flat string join key against companion DOCX parts (`commentsExtended.xml`), not a `NodeId` structure.
- **Revisions** (`v1/body.rs:1197-1248`): `Revision { id, kind: Insertion|Deletion|MoveFrom|MoveTo, author, date, revision_id, inlines }`, a transparent wrapper range (depth-bounded, `MAX_REVISION_DEPTH = 8`); moves pair via `MoveRangeStart`/`MoveRangeEnd` correlated by `name`. Export (`casual-doc-export/src/semantic.rs`) re-emits whatever the model already carries; **nothing in `casual-doc-edit` or `casual-doc-wasm` creates, accepts, or rejects a `Revision` today** — confirmed zero hits repo-wide outside docs/model/import/export.
- **Anchoring precedent**: bookmarks (`BookmarkStart`/`BookmarkEnd`) are the same zero-width-point-pair shape as comment ranges, resolved by a live inline-tree walk with no persistent index (`resolve_bookmark`, `casual-doc-wasm/src/lib.rs:4422`). Both comments and suggestion ranges should resolve the same way — there is no anchor cache anywhere in the codebase, and this doc does not introduce one.
- **Edit surface**: `casual_doc_edit::Operation` (`lib.rs:117-276`) has no comment or revision variant. `SetHyperlink` is the closest existing analog for a "create/update/remove a range-scoped inline wrapper" op.
- **Host surface**: `casual-doc-wasm` (what `webapp/` actually drives) has no query API to enumerate comments/revisions and no mutation API for either. The legacy `casual-doc-sdk`/`casual-doc-transaction` v0 session is a separate, older layer with no v1 feature awareness at all and is not in scope here.

## Required Additions

### 1. Host identity seam (I4)

The engine has no concept of "current user." The host supplies a `CurrentUser { name: String, initials: Option<String>, color: Option<Rgba> }` at session start (WASM constructor option, mirroring how fonts are host-populated per the font-registry seam). If no color is supplied, the engine deterministically derives one from a hash of `name` (stable within a session, mirrors Word's auto-assigned author colors) — this is presentation only, never persisted into the model beyond the existing opaque `author: Option<String>` string. No network calls, no accounts, no server — consistent with AGENTS.md's "no mandatory server... dependency."

### 2. New `Operation` variants (closed set, I2)

Comments:
- `InsertComment { range: Range, body: Vec<BlockNode>, author: Option<String>, initials: Option<String>, date: Option<String> }` → creates a fresh `CommentId`, wraps `range` in `CommentRangeStart`/`End`, appends `CommentReference` at the end point, inserts the `Comment` into `Definitions.comments`. Inverse: `RemoveComment { comment }`.
- `ReplyToComment { parent: CommentId, body, author, initials, date }` → new `Comment` with `parent_para_id` set to the parent's synthesized `para_id` (see Open Questions — new in-editor comments must synthesize a stable `para_id` themselves, since that field is otherwise an import artifact). Inverse: `RemoveComment { comment }`.
- `UpdateCommentBody { comment: CommentId, body: Vec<BlockNode> }` — editing one's own comment text. Inverse carries the prior `body`.
- `SetCommentResolved { comment: CommentId, done: bool }`. Inverse is the same op with the prior value.
- `RemoveComment { comment: CommentId }` — deletes the `Comment` and its three range markers. Inverse is `InsertComment`/`ReplyToComment` reconstructed from the removed state.

Suggestions:
- A `tracking: Option<Attribution>` field (where `Attribution { author, date }`) threaded onto `InsertText`, `DeleteText`, and `FormatText`. When present, the op's normal mutation is wrapped in a `Revision{kind: Insertion|Deletion, author, date}` instead of applying directly — deletions keep the deleted content present-but-marked (as today's model already supports) rather than removing it. This reuses the existing three ops instead of forking a parallel "tracked" op family, keeping the op set additive without duplicating every mutation.
- `AcceptRevision { id: NodeId }` — materializes the wrapped change (drops deleted content for a `Deletion`, unwraps content for an `Insertion`) and removes the `Revision` wrapper. Inverse restores the wrapper (and, for a deletion, restores the removed content).
- `RejectRevision { id: NodeId }` — the opposite materialization (drops inserted content for an `Insertion`, restores content for a `Deletion`). Inverse is the paired accept semantics.

All five comment ops and three suggestion ops go through `casual_doc_edit::apply` exactly like every existing op, each returning its exact inverse per the existing contract — no new choke point, no bypass.

### 3. Anchoring, resolution, and query surface

Comment-range and revision-range resolution reuse `resolve_bookmark`'s live-walk strategy verbatim (generalize it to a shared `resolve_marker_pair(document, predicate) -> Option<ModelRange>` helper rather than three copies of the same walk). New `casual-doc-wasm` query methods:

- `listComments() -> Vec<CommentSummary>` (id, author, initials, date, done, para_id/parent_para_id, resolved anchor rect via the same rect-union technique the floating selection toolbar already uses on `.overlay .highlight` rects).
- `listRevisions() -> Vec<RevisionSummary>` (id, kind, author, date, resolved anchor rect).
- `commentThread(comment: CommentId) -> Vec<Comment>` (root + replies, ordered).

No caching layer is introduced; these re-walk the tree on demand, consistent with every other position query in the codebase today.

### 4. Threading model

`para_id`/`parent_para_id` are DOCX round-trip artifacts (paragraph ids in companion parts), not stable in-memory identity. For comments created in the editor, the design synthesizes a `para_id`-shaped value at creation time (same generation scheme as any other paragraph id) so export can always emit valid `commentsExtended.xml` threading — this is called out as an implementation risk in Open Questions rather than fully specified here, since it depends on how paragraph ids are minted elsewhere in the writer.

## UI/UX Design

### 2026-07-31 reference correction

The sibling `docs/docx-editor` implementation is the normative interaction
reference for this slice. Review cards occupy a dedicated 340px column beside
the document with a 12px page gap. Opening that column changes document
layout; cards must never fall back over the page canvas or into a fixed
viewport overlay. Comments and tracked changes share the column in document
order, with collision avoidance preserving their anchor order.

Only the active card is expanded. Collapsed cards retain author, date, and a
short body; expanded comment cards expose resolve/reopen, overflow actions,
thread replies, and an inline reply field. Expanded tracked-change cards
expose per-change Accept and Reject. Comment creation is a temporary card in
the same column, not a canvas popover. When the column is closed, the Review
rail/View control remains the entry point.

`CommentReference` is document metadata and is zero-width in layout. It must
not shape visible `[comment]`, `?`, superscript, or any other in-document
glyph. The canvas may show an interaction-only range highlight, while the
identity and actions live in the review column.

The compact Editing/Suggesting mode toggle remains in the ribbon. Previous,
Next, Accept All, and Reject All are not exposed as a second ad-hoc toolbar in
the Home ribbon; individual decisions belong to expanded cards until a
purpose-designed Review surface is implemented.

### 2026-07-31 workflow-completeness correction

The sidebar migration also exposed engine-level correctness gaps. Editor-authored
comments now receive unique eight-hex-digit `para_id` join keys; replies point to
the root's `para_id`, imported replies group through that same key, and deleting a
root cascades through descendants. This is covered by an author→export→reopen
threading test. Comment references are zero-width in layout and in every WASM
anchor walk, so no hidden `[comment]` bytes distort a later selection or revision
anchor.

Resolved cards collapse immediately and suppress their range highlight until the
user explicitly expands the card. Clicking the sidebar gutter collapses the active
card. Replacement and formatting suggestions use an opaque editor group id:
their deletion/insertion model pair appears as one card, is accepted or rejected
atomically, and is restored by one Undo. Consecutive suggestion typing shares one
group, and Backspace/Delete over the current author's pending insertion edits that
insertion rather than creating a deletion-of-an-insertion.

The tracked run-format surface covers bold, italic, underline, strike, font
family, size, text color, highlight, and superscript/subscript, including formatting
armed before typing. A visible Suggesting banner makes the mode persistent.
Structural operations that do not yet have a safe revision representation
(cross-paragraph replacement/cut/paste, paragraph breaks, and list/indent changes)
are rejected with an explanation instead of silently bypassing tracking.

### 2026-07-31 paired-move correction

Imported `MoveFrom`/`MoveTo` revisions are one logical suggestion, not two
independent changes. The producer's revision `w:id` is not a pairing key: the
source and destination commonly have different values. Pairing therefore uses
the shared `w:name` carried by their enclosing move-range starts and retains the
exact source/destination marker node ids as the decision token.

The sidebar renders one "Moved …" card containing both the source and destination
anchors. Accept removes the source and keeps the destination; Reject keeps the
source and removes the destination. Either choice removes all four consumed
move-range markers, commits as one review transaction, and is restored by one
Undo. Incomplete or ambiguous producer markup remains visible as individual
read-only-safe move items rather than being guessed into an unrelated pair.

**Dedicated review sidebar** hosts the unified comment/suggestion stream in document order. Each comment card shows author/initials, date, body, replies, and resolve/reopen. Suggestions appear as cards in the same stream rather than a separate panel. The sidebar plus exact model-anchor rects avoid coupling review UX to the bitmap canvas or searching for duplicate text.

**Comment creation**: selecting a range surfaces a "Comment" button on the existing floating selection toolbar (P1G-034 explicitly deferred this pending a comments model — now unblocked) alongside B/I/U/S/color/highlight. Clicking opens a small inline composer anchored at the selection; submitting creates the comment, highlights the range in a soft highlight distinct from selection/find (matching Docs' anchor-highlight convention), and opens/focuses the panel thread.

**Suggesting mode**: a segmented control (Editing / Suggesting / Viewing) in the top chrome, using doc 63 §3's existing `.segmented` component rather than a new pattern. In Suggesting mode, `InsertText`/`DeleteText`/`FormatText` automatically carry the host's `CurrentUser` as `tracking` attribution. Pending insertions render underlined in the author's color; pending deletions render as strikethrough in the author's color (content stays visible and in place, matching both references — no silent removal). Hover shows a tooltip ("Name • Date — inserted/deleted"), reusing the existing link-chip tooltip pattern. Viewing mode is read-only (no `Operation` reaches `apply`).

**Accept/Reject**: right-click a suggestion range for a context menu (reusing the existing table right-click-menu infrastructure) with Accept/Reject; a lightweight Next/Prev navigation pair for stepping through pending suggestions. Ribbon-style Accept All/Reject All is deferred to Slice C.

## Host Policy & Identity

Per AGENTS.md ("Host applications own policy: storage, network, auth, telemetry"): identity (`CurrentUser`), persistence of threads/documents, and any collaboration transport are entirely host-supplied. The engine's only obligation is to carry the opaque `author`/`initials`/`date` strings faithfully and never fabricate, look up, or phone-home for identity.

## Phasing

- **Slice A — Comments.** New comment `Operation` variants, WASM query/mutate surface, anchor rendering, right-panel thread UI, floating-toolbar Comment button. Independent of suggesting-mode work; ships first since the data model is already complete.
- **Slice B — Suggesting-mode foundation.** Host identity seam, `tracking` attribution on `InsertText`/`DeleteText`/`FormatText`, inline suggestion rendering, single-item Accept/Reject, mode toggle.
- **Slice C — Suggesting-mode depth.** Formatting-only tracked changes, Accept All/Reject All, resolved/pending filters, per-author color legend, Next/Prev navigation polish.
- **Slice D — Partially complete.** Paired move review is implemented. @mentions/assignees, reactions, and ribbon Review tab depth (markup-view modes, Show Markup filters) remain outside the current product slice.

This slots alongside doc 67's existing execution plan; it does not require reordering the already-accepted PR3 (structural delete/keyboard shortcuts).

## Decisions and Remaining Questions

1. **Resolved:** editor-authored comments allocate the next unused eight-hex-digit `para_id`; replies use the root key. Import/export remains opaque for producer-authored ids.
2. **Resolved:** character-format suggestions cover the complete exposed run-format surface and use one atomic deletion/insertion group.
3. **Resolved:** imported `MoveFrom`/`MoveTo` revisions pair through their
   shared move-range name plus exact source/destination marker identities. The
   card and decision are atomic and remove the consumed range markers.
4. **Resolved:** Comments owns a dedicated 340px sibling sidebar, not the generic region-4 properties panel and never a canvas overlay.
5. **Resolved for P1G-REVIEW-035:** `w:id` is producer-facing numeric revision
   identity. OpenDoc card/decision grouping is separate typed model metadata,
   and doc 82 defines its scoped history and validation contract.

## Verification Gates

- Unit tests: forward/inverse round trip for every new `Operation` variant (comment create/reply/update/resolve/remove; revision tracking wrap + accept + reject), including undo/redo.
- WASM tests: `listComments`/`listRevisions`/`commentThread` against fixtures with existing (imported) and newly-created comments/revisions.
- DOCX round-trip: create a comment and a tracked insertion/deletion in-editor, export, reimport, confirm anchors and threading survive (extends the existing fidelity-corpus harness).
- Browser Playwright specs: comment create/reply/resolve/delete; suggesting-mode toggle + insert/delete rendering + accept/reject; no console errors; undo after each action.
