// Review decisions: the inline accept/reject card, one Next/Previous over
// comments and changes, and the single-change decisions at the caret
// (Accept/Reject and Next). Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { mayDecideReview, reviewDecisionRefusal } from "./review_authority.mjs";
import { formatReviewDate, reviewAuthorColor, reviewAuthorDisplay, reviewAuthorKey, reviewChangeTypeLabel } from "./review_labels.mjs";
import { reviewCurrentTargetIndex } from "./review_target.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createReviewDecisions(ed) {
  // --- Inline accept/reject card (Q2) ------------------------------------------

  let reviewInlineCardRevisionId = null;
  let reviewInlineCardPinned = false;
  let reviewInlineHideTimer = 0;

  /** One-line summary of a tracked change for the inline card and screen-reader
   *  announcements. */
  function reviewRevisionSummary(revision) {
    const text = String(revision?.text || "");
    const quoted = text ? `“${text}”` : "";
    switch (revision?.kind) {
      case "deletion": return `Deleted ${quoted}`.trim();
      case "insertion": return `Added ${quoted}`.trim();
      case "formatting": return `Formatting change ${quoted}`.trim();
      case "paragraph_format": return "Formatted paragraph";
      case "paragraph_mark_insertion": return "Added a paragraph break";
      case "paragraph_mark_deletion": return "Deleted a paragraph break";
      case "move_from": case "move_to": case "move": return `Moved ${quoted}`.trim();
      case "replacement": return `Replaced ${quoted}`.trim();
      default: return `Changed ${quoted}`.trim();
    }
  }

  function closeReviewInlineCard() {
    clearTimeout(reviewInlineHideTimer);
    reviewInlineHideTimer = 0;
    ed.reviewInlineCard?.remove();
    ed.reviewInlineCard = null;
    reviewInlineCardRevisionId = null;
    reviewInlineCardPinned = false;
  }

  /** Hide a hover-opened card after a short grace period, so moving the pointer
   *  from the marker onto the card itself does not dismiss it. A pinned (clicked)
   *  card ignores this. */
  function scheduleReviewInlineCardHide() {
    if (reviewInlineCardPinned) return;
    clearTimeout(reviewInlineHideTimer);
    reviewInlineHideTimer = window.setTimeout(() => {
      if (!reviewInlineCardPinned) closeReviewInlineCard();
    }, 220);
  }

  function showReviewInlineCard(revision, anchorEl, pinned) {
    if (!revision || !anchorEl) return;
    const revisionId = String(revision.id ?? "");
    clearTimeout(reviewInlineHideTimer);
    reviewInlineHideTimer = 0;
    // Re-hovering / re-clicking the marker already showing keeps the one card;
    // a click on an already-open hover card just pins it.
    if (ed.reviewInlineCard && reviewInlineCardRevisionId === revisionId) {
      if (pinned) reviewInlineCardPinned = true;
      return;
    }
    closeReviewInlineCard();
    reviewInlineCardRevisionId = revisionId;
    reviewInlineCardPinned = !!pinned;

    const card = document.createElement("div");
    card.className = "review-inline-card";
    card.setAttribute("role", "dialog");
    card.setAttribute("aria-label", "Tracked change");
    card.tabIndex = -1;

    const head = document.createElement("div");
    head.className = "review-inline-head";
    const dot = document.createElement("span");
    dot.className = "review-inline-dot";
    dot.style.background = reviewAuthorColor(reviewAuthorKey(revision));
    const who = document.createElement("div");
    who.className = "review-inline-who";
    const author = document.createElement("strong");
    author.textContent = reviewAuthorDisplay(revision);
    const meta = document.createElement("small");
    meta.textContent = [reviewChangeTypeLabel(revision.kind), formatReviewDate(revision.date)]
      .filter(Boolean).join(" · ");
    who.append(author, meta);
    head.append(dot, who);

    const body = document.createElement("p");
    body.className = "review-inline-body";
    body.textContent = reviewRevisionSummary(revision);
    card.append(head, body);

    // Accept/Reject are edits; hide them in read-only Viewing mode, leaving the
    // card as a summary the reader can still see.
    // And not where the container withholds deciding (HF-199): the card is then
    // a summary of a change somebody with edit access will decide.
    if (ed.reviewMode !== "viewing" && !reviewDecisionRefusal("review.accept", ed.HOST_CAPS)) {
      const bar = document.createElement("div");
      bar.className = "review-inline-bar";
      const makeBtn = (icon, label, danger, handler) => {
        const button = document.createElement("button");
        button.type = "button";
        if (danger) button.className = "danger";
        const glyph = document.createElement("span");
        glyph.className = "ms";
        glyph.setAttribute("aria-hidden", "true");
        glyph.textContent = icon;
        button.append(glyph, document.createTextNode(label));
        button.setAttribute("aria-label", `${label} this ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
        button.title = button.getAttribute("aria-label");
        button.addEventListener("click", async (event) => {
          event.stopPropagation();
          await handler();
        });
        return button;
      };
      const decide = async (accept) => {
        await ed.decideContextRevision(revision, accept);
        ed.announceReview(`${accept ? "Accepted" : "Rejected"} ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
        closeReviewInlineCard();
        ed.focusEditorSurface();
      };
      bar.append(
        makeBtn("check", "Accept", false, () => decide(true)),
        makeBtn("close", "Reject", true, () => decide(false)),
      );
      card.appendChild(bar);
    }

    // The card is interactive: entering it cancels the hover-hide timer, leaving
    // it reschedules the hide (unless pinned).
    card.addEventListener("mouseenter", () => clearTimeout(reviewInlineHideTimer));
    card.addEventListener("mouseleave", () => scheduleReviewInlineCardHide());

    document.body.appendChild(card);
    ed.reviewInlineCard = card;

    // Position just below the marker, clamped to the viewport.
    const markerRect = anchorEl.getBoundingClientRect();
    const left = Math.max(12, Math.min(window.innerWidth - card.offsetWidth - 12, markerRect.left));
    const below = markerRect.bottom + 6;
    const top = below + card.offsetHeight + 12 > window.innerHeight
      ? Math.max(12, markerRect.top - card.offsetHeight - 6)
      : below;
    card.style.left = `${left}px`;
    card.style.top = `${top}px`;

    // A pinned card takes focus so it is keyboard-dismissable and its buttons are
    // immediately reachable by Tab; a hover card must not steal the caret.
    if (pinned) card.focus({ preventScroll: true });
  }

  // --- Unified Next/Previous over comments AND changes (Q4) --------------------
  // Word's Review "Previous / Next" walks every comment and tracked change in one
  // document-ordered loop. `reviewNavTargets` builds that merged list (honoring
  // the active Open/Resolved/All filter the same way the sidebar does) and orders
  // it by on-screen position — the same page/top/left ordering the comment column
  // uses — so Next/Previous visit exactly what the reader sees, in reading order.

  /** Whether rect `a` sits strictly after `ref` in document (reading) order. */
  function reviewRectIsAfter(a, ref) {
    if (!a || !ref) return false;
    if (a.pageNumber !== ref.pageNumber) return a.pageNumber > ref.pageNumber;
    if (Math.abs(a.top - ref.top) > 2) return a.top > ref.top;
    return a.left > ref.left + 2;
  }

  /** The current selection's leading/trailing on-screen positions, so navigation
   *  can skip the item the caret is already on regardless of selection direction. */
  function reviewSelectionRects() {
    if (!ed.selection) return null;
    const a = ed.selection.anchor;
    const f = ed.selection.focus;
    const ra = ed.reviewRangeClientRect(a.node, a.offset, a.node, a.offset);
    const rf = ed.reviewRangeClientRect(f.node, f.offset, f.node, f.offset);
    if (!ra && !rf) return null;
    if (!ra) return { start: rf, end: rf };
    if (!rf) return { start: ra, end: ra };
    return reviewRectIsAfter(rf, ra) ? { start: ra, end: rf } : { start: rf, end: ra };
  }

  /** The merged, document-ordered list of navigable review items — root comments
   *  (per filter) plus tracked changes — each with its model range and on-screen
   *  rect. Replies are represented by their root; changes are individual (their
   *  grouped sidebar card is still surfaced via the caret→card anchor index). */
  function reviewNavTargets() {
    if (!ed.doc) return [];
    const { comments, revisions } = ed.readReviewData(ed.doc);
    const targets = [];
    for (const comment of comments ?? []) {
      if (!comment.anchor?.node || comment.parentParaId) continue;
      if (ed.reviewFilter !== "all") {
        const resolved = !!comment.resolved;
        if (ed.reviewFilter === "resolved" ? !resolved : resolved) continue;
      }
      const startOffset = Number(comment.anchor.start) || 0;
      const endOffset = Number(comment.anchor.end) || startOffset;
      const rect = ed.reviewRangeClientRect(comment.anchor.node, startOffset, comment.anchor.node, endOffset);
      if (rect) {
        targets.push({
          type: "comment",
          data: comment,
          range: { startNode: comment.anchor.node, startOffset, endNode: comment.anchor.node, endOffset },
          rect,
        });
      }
    }
    // Tracked changes are not comment-"resolved": show them under Open and All,
    // hide them only under the comment-only Resolved filter (mirrors the sidebar).
    if (ed.reviewFilter !== "resolved") {
      for (const revision of revisions ?? []) {
        // A zero-length revision is normally nothing to visit. Formatting changes
        // and paragraph-level revisions are the exceptions: a paragraph mark has no
        // text of its own, and skipping it made Next/Previous step straight past
        // every paragraph break a reviewer suggested (docs/108).
        if (!String(revision.text || "").length && !ed.REVIEW_TEXTLESS_KINDS.has(revision.kind)) continue;
        const range = ed.revisionRange(revision);
        if (!range) continue;
        const rect = ed.reviewRangeClientRect(range.startNode, range.startOffset, range.endNode, range.endOffset);
        if (rect) targets.push({ type: "revision", data: revision, range, rect });
      }
    }
    targets.sort((a, b) =>
      a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top || a.rect.left - b.rect.left);
    return targets;
  }

  /** Moves the caret/selection to a navigation target, scrolls it into view, and
   *  surfaces its sidebar card (via the caret→card anchor index, which handles the
   *  grouped/move cards correctly). Announces its position + kind for AT. */
  function focusReviewTarget(target, index, total) {
    ed.reviewSidebarPreference = true;
    const { range } = target;
    ed.selection = {
      anchor: { node: range.startNode, offset: range.startOffset },
      focus: { node: range.endNode, offset: range.endOffset },
    };
    // Resolve the active item FIRST (from the exact selected range), so the markers
    // paint with the correct item active and the scroll targets that item's own
    // marker — not the first highlight or a boundary-sharing neighbour.
    ed.syncActiveReviewCommentToCaret(ed.selection.focus);
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollReviewSelectionIntoView();
    const who = reviewAuthorDisplay(target.data) || "You";
    const label = target.type === "comment"
      ? `Comment by ${who}`
      : `${reviewChangeTypeLabel(target.data.kind)} by ${who}`;
    ed.announceReview(`${index + 1} of ${total}: ${label}`);
  }

  /** The index of the target the caret is currently on: an exact match to a just-
   *  navigated selection, else the item whose range contains the caret. `-1` when
   *  the caret is not on any item (a fresh navigation from arbitrary text). This
   *  is what lets Next/Previous step by list position, so two items that share a
   *  boundary (a change starting exactly where a comment ends) still advance. */

  /** Next (`+1`) / Previous (`-1`) across the unified comment+change list, wrapping
   *  at the ends. When the caret already sits on an item, it steps by list index
   *  (robust to boundary-adjacent items); otherwise it lands on the nearest item
   *  after/before the caret in reading order. */
  function navigateReview(direction) {
    if (!ed.doc) return;
    const targets = reviewNavTargets();
    if (!targets.length) {
      // The Review surface deliberately keeps Next/Previous enabled whatever the
      // document holds, on the stated grounds that "the commands themselves
      // report why nothing happened" (see REVIEW_SURFACE). This one did not: with
      // no comments and no tracked changes it returned in silence, so Review ▸
      // Next was a control that did nothing on a clean document — the same dead
      // control Tools ▸ Settings was, by a different route.
      ed.setStatus("This document has no comments or tracked changes", "", { timeout: 3000 });
      return;
    }
    const current = reviewCurrentTargetIndex(targets, ed.selection);
    let index;
    if (current >= 0) {
      index = (current + (direction > 0 ? 1 : -1) + targets.length) % targets.length;
    } else {
      const caret = reviewSelectionRects();
      if (!caret) {
        index = direction > 0 ? 0 : targets.length - 1;
      } else if (direction > 0) {
        // First item at or after the caret, so a caret sitting exactly on an
        // item's start (e.g. document start === a comment's leading edge) lands on
        // that item rather than skipping it. "At or after" == not strictly before.
        const found = targets.findIndex((t) => !reviewRectIsAfter(caret.end, t.rect));
        index = found === -1 ? 0 : found;
      } else {
        let found = -1;
        for (let i = 0; i < targets.length; i++) {
          if (reviewRectIsAfter(caret.start, targets[i].rect)) found = i;
        }
        index = found === -1 ? targets.length - 1 : found;
      }
    }
    focusReviewTarget(targets[index], index, targets.length);
  }

  // --- Single-change decisions at the caret + Accept/Reject ▸ Next (Q3) --------

  /** The tracked change under the caret (its focus, else its anchor), or null. */
  function reviewRevisionAtCaret() {
    return ed.reviewContextAt(ed.selection?.focus).revision
      || ed.reviewContextAt(ed.selection?.anchor).revision;
  }

  /** Accepts (or rejects) the tracked change at the caret, using the same
   *  grouped/move-aware decision path as the sidebar and context menu. Returns
   *  whether a change was found and decided. */
  async function decideReviewAtCaret(accept) {
    const revision = reviewRevisionAtCaret();
    if (!revision) {
      ed.setStatus("Place the caret inside a tracked change to accept or reject it", "error");
      return false;
    }
    await ed.decideContextRevision(revision, accept);
    ed.announceReview(`${accept ? "Accepted" : "Rejected"} ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
    ed.drawSelection();
    ed.focusEditorSurface();
    return true;
  }

  /** Word's core review loop: accept/reject the change at the caret, then advance
   *  the caret to the next change/comment. Advances even when the caret was not on
   *  a change, so the shortcut always makes progress through the document. */
  async function decideReviewAndAdvance(accept) {
    const decided = await decideReviewAtCaret(accept);
    navigateReview(1);
    return decided;
  }

  /**
   * Moves the caret/selection to one end of a tracked move — its source
   * (`move_from`) or destination (`move_to`) anchor — and scrolls that location
   * to the centre of the viewport. This is the keyboard-accessible "go to the
   * original / new location" navigation a move review card exposes for both ends
   * of the move (REVIEW-GAP-016), so a reviewer can jump to precisely where the
   * text came from and where it went.
   */
  function navigateToReviewAnchor(anchor) {
    if (!ed.doc || !anchor?.node) return;
    ed.reviewSidebarPreference = true;
    const start = Number(anchor.start) || 0;
    const rawEnd = Number(anchor.end);
    const end = Number.isFinite(rawEnd) ? rawEnd : start;
    ed.selection = {
      anchor: { node: anchor.node, offset: start },
      focus: { node: anchor.node, offset: end },
    };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollReviewSelectionIntoView();
  }

  /**
   * A keyboard-accessible navigation control for one end of a tracked move.
   * The visible secondary line is the precise page the end sits on (from the
   * same range geometry the sidebar uses to place cards), falling back to a
   * generic location label when geometry is unavailable (e.g. an off-screen or
   * not-yet-laid-out anchor). Activating it jumps the caret to that end.
   */
  function reviewMoveEndButton(endLabel, anchor, fallbackLocation, action) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "review-move-end";
    const start = Number(anchor?.start) || 0;
    const rawEnd = Number(anchor?.end);
    const end = Number.isFinite(rawEnd) ? rawEnd : start;
    const rect = anchor?.node
      ? ed.reviewRangeClientRect(anchor.node, start, anchor.node, end)
      : null;
    const locationText = rect ? `Page ${rect.pageNumber}` : fallbackLocation;
    const label = document.createElement("b");
    label.textContent = endLabel;
    const location = document.createElement("span");
    location.textContent = locationText;
    button.append(label, location);
    const description = rect ? `${action} (page ${rect.pageNumber})` : action;
    button.title = description;
    button.setAttribute("aria-label", description);
    button.disabled = !anchor?.node;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      navigateToReviewAnchor(anchor);
    });
    return button;
  }

  function focusReviewRevision(revision, expand = true) {
    if (!ed.doc || !revision?.text) return;
    ed.reviewSidebarPreference = true;
    if (expand) ed.activeReviewItemId = `revision:${revision.id}`;
    // Clicking a card scrolls the canvas to that change's anchor (card→canvas
    // sync, REVIEW-GAP-019). For a tracked move, the destination is the sensible
    // default landing spot; its per-end buttons still jump to source/destination.
    if (revision.kind === "move" && revision.destinationAnchor?.node) {
      const dest = revision.destinationAnchor;
      ed.selection = {
        anchor: { node: dest.node, offset: Number(dest.start) || 0 },
        focus: { node: dest.node, offset: Number(dest.end) || Number(dest.start) || 0 },
      };
      ed.drawSelection();
      ed.focusEditorSurface();
      ed.scrollReviewSelectionIntoView();
      ed.scheduleReviewMarginRender();
      return;
    }
    const range = ed.revisionRange(revision);
    if (range) {
      ed.selection = {
        anchor: { node: range.startNode, offset: range.startOffset },
        focus: { node: range.endNode, offset: range.endOffset },
      };
      ed.drawSelection();
      ed.focusEditorSurface();
      ed.scrollReviewSelectionIntoView();
      ed.scheduleReviewMarginRender();
      return;
    }
    const first = ed.selection?.focus || ed.doc.firstPosition();
    const match = ed.doc.findText(String(revision.text), first.node, first.offset, true, false);
    if (first.free) first.free();
    if (!match.found) { match.free(); return; }
    ed.selection = {
      anchor: { node: match.startNode, offset: match.startOffset },
      focus: { node: match.endNode, offset: match.endOffset },
    };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollReviewSelectionIntoView();
    ed.scheduleReviewMarginRender();
    match.free();
  }

  function toggleReview(open) {
    const show = open ?? ed.reviewSidebar.hidden;
    ed.reviewSidebarPreference = show;
    if (!show) {
      ed.activeReviewItemId = null;
      ed.activeReviewCommentId = null;
      ed.reviewComposerState = null;
    }
    ed.reanchorPageOverlays();
    // Focus management (REVIEW-GAP-023): closing the sidebar returns focus to the
    // rail toggle that owns it, so keyboard/AT users are not stranded.
    if (!show) ed.railReview?.focus?.({ preventScroll: true });
  }
  ed.reviewBtn.addEventListener("click", () => toggleReview());
  ed.railReview.addEventListener("click", () => toggleReview());
  ed.reviewClose.addEventListener("click", () => toggleReview(false));
  /** Accept or reject every tracked change, for whichever surface asked.
   *
   *  The Review menu and the Review ribbon used to reach this by clicking the
   *  sidebar's own button — and `updateReviewControls` DISABLES that button when
   *  the document holds no changes. A disabled button dispatches no click, so the
   *  menu row and the ribbon button did nothing at all, and said nothing either:
   *  the same dead control Tools ▸ Settings was, reached by a different route.
   *  Surfaces call this instead, and it answers for itself. Viewing mode is still
   *  refused by `runEdit`, which reports that in its own words. */
  async function decideAllReviewChanges(accept) {
    if (!ed.doc) return;
    if (!mayDecideReview(ed.HOST_CAPS, ed.setStatus)) return; // HF-199
    // One reading of "does this document carry a change", not two: this had its own
    // copy of `documentHasTrackedChanges`'s parse-and-count (SKILL §8).
    if (!ed.documentHasTrackedChanges()) {
      ed.setStatus("This document has no tracked changes", "", { timeout: 3000 });
      return;
    }
    await ed.runEdit(() => ed.doc.decideAllRevisions(accept));
    ed.announceReview(accept ? "All changes accepted" : "All changes rejected");
    ed.scheduleReviewMarginRender();
  }
  ed.reviewAcceptAll.addEventListener("click", () => void decideAllReviewChanges(true));
  ed.reviewRejectAll.addEventListener("click", () => void decideAllReviewChanges(false));
  ed.reviewPrevious.addEventListener("click", () => navigateReview(-1));
  ed.reviewNext.addEventListener("click", () => navigateReview(1));
  // The visible mode control (`#reviewModeControl`) is a three-button segmented
  // group; each button carries `data-review-mode` and is wired below.
  ed.suggestingBannerEdit.addEventListener("click", () => ed.setReviewMode("editing"));
  if (ed.viewingBannerEdit) ed.viewingBannerEdit.addEventListener("click", () => ed.setReviewMode("editing"));
  function openReviewComposer(parent = null) {
    if (!ed.doc || (!parent && (!ed.hasRange() || !ed.selection))) return;
    ed.reviewSidebarPreference = true;
    if (parent) {
      ed.activeReviewItemId = `comment:${parent}`;
      ed.reviewComposerState = null;
      ed.scheduleReviewMarginRender();
      return;
    }
    // A comment can span paragraphs (Word/Docs). Order the endpoints in document
    // order: same node compares offsets directly; otherwise `selectionEdge`
    // returns the earlier/later endpoint so the start marker lands in the start
    // paragraph and the end marker in the end paragraph.
    const { anchor, focus } = ed.selection;
    let start;
    let end;
    if (anchor.node === focus.node) {
      const forward = anchor.offset <= focus.offset;
      start = forward ? { ...anchor } : { ...focus };
      end = forward ? { ...focus } : { ...anchor };
    } else {
      const s = ed.doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, false);
      const e = ed.doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, true);
      start = { node: s.node, offset: s.offset };
      end = { node: e.node, offset: e.offset };
      s.free();
      e.free();
    }
    ed.reviewComposerState = { range: { start, end } };
    ed.activeReviewItemId = null;
    ed.scheduleReviewMarginRender();
  }
  // Comment authoring/reply uses the in-sidebar composer (`openReviewComposer` →
  // `reviewComposerState`, rendered by `renderReviewMarginItems`); the legacy
  // hidden side-panel composer was removed (docs/81 REVIEW-GAP-018/026).
  ed.selComment.addEventListener("mousedown", (event) => event.preventDefault());
  ed.selComment.addEventListener("click", () => openReviewComposer());
  for (const filter of ed.reviewFilters) {
    filter.addEventListener("click", () => {
      ed.reviewFilter = filter.dataset.reviewFilter;
      for (const button of ed.reviewFilters) {
        button.setAttribute("aria-pressed", String(button === filter));
      }
      const label = { open: "Open comments", resolved: "Resolved comments", all: "All comments" }[ed.reviewFilter] || ed.reviewFilter;
      ed.announceReview(`Filter: ${label}`);
      ed.scheduleReviewMarginRender();
    });
  }
  for (const mode of ed.reviewModeButtons) {
    mode.addEventListener("click", () => ed.setReviewMode(mode.dataset.reviewMode));
  }

  return {
    closeReviewInlineCard, decideAllReviewChanges, decideReviewAndAdvance, decideReviewAtCaret,
    focusReviewRevision, navigateReview, openReviewComposer, reviewMoveEndButton,
    scheduleReviewInlineCardHide, showReviewInlineCard, toggleReview,
  };
}
