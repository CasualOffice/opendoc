// The review margin: revision rectangles, the review mode and its controls,
// comment cards and their virtualized window. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { mayDecideReview, reviewDecisionRefusal } from "./review_authority.mjs";
import { t } from "./i18n.mjs";
import { aiAccessMayHaveChanged } from "./ai_live.mjs";
import { reflectReviewModeAccess } from "./capabilities.mjs";
import { orderedSelectionEnds } from "./caret_navigation.mjs";
import { reviewCardSignature, reviewCommentIsReplyTo, reviewStackHeight, stackReviewCards } from "./review_layout.mjs";
import { attachComposerKeys, autoGrowTextarea, createCommentAffordance, reviewCardButton, reviewIconButton } from "./review_chrome.mjs";
import { commentEditButton, commentEditor, commentQuoteElement } from "./review_comment_card.mjs";
import { attachCommentMentions } from "./mention_composer.mjs";
import { formatReviewDate, reviewAuthorColor, reviewAuthorKey, reviewCardAriaLabel, reviewChangeTypeLabel, reviewCommentTooltip, reviewFormattingDescription, reviewRevisionTooltip } from "./review_labels.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createReviewMargin(ed) {
  function paragraphRevisionRects(revision) {
    const { node, start, end } = revision.anchor ?? {};
    if (!node) return [];
    const caret = () => ed.doc.caretRect(node, Number(end) || 0);
    if (revision.kind !== "paragraph_format") {
      const at = caret();
      if (at.length < 5) return [];
      // A glyph-wide box starting at the caret, so the pilcrow has room to show.
      return [at[0], at[1], at[2], Math.max(at[4] * 0.6, 120), at[4]];
    }
    const lines = ed.doc.selectionRects(node, Number(start) || 0, node, Number(end) || 0);
    const pagesSeen = new Map();
    const include = (page, x, y, h) => {
      const box = pagesSeen.get(page) ?? { left: x, top: y, bottom: y + h };
      box.left = Math.min(box.left, x);
      box.top = Math.min(box.top, y);
      box.bottom = Math.max(box.bottom, y + h);
      pagesSeen.set(page, box);
    };
    for (let i = 0; i + 4 < lines.length; i += 5) include(lines[i], lines[i + 1], lines[i + 2], lines[i + 4]);
    if (!pagesSeen.size) {
      // An empty paragraph selects nothing; its caret still has a line box.
      const at = caret();
      if (at.length >= 5) include(at[0], at[1], at[2], at[4]);
    }
    // The bar belongs in the page margin, beside the text column, wherever the
    // paragraph's own text happens to sit. Anchoring it to the text's left edge put
    // it mid-page for any centred or right-aligned paragraph — the very paragraphs a
    // formatting change most often touches. The margin is the paragraph's own
    // section's, because a document can change margins between sections.
    const BAR_OFFSET = 200; // twips out from the text column
    const BAR_WIDTH = 40;
    const columnStart = paragraphColumnStart(node);
    const flat = [];
    for (const [page, box] of pagesSeen) {
      const edge = Number.isFinite(columnStart) ? columnStart : box.left;
      flat.push(page, Math.max(edge - BAR_OFFSET, 0), box.top, BAR_WIDTH, box.bottom - box.top);
    }
    return flat;
  }


  /** The text column's start edge (twips from the page edge) for the section that
   *  owns `node`, or NaN when the engine cannot say. */
  function paragraphColumnStart(node) {
    if (ed.reviewColumnStartMemo.has(node)) return ed.reviewColumnStartMemo.get(node);
    let start = Number.NaN;
    try {
      const raw = ed.doc.pageSetupSections(node);
      const list = raw === "null" ? null : JSON.parse(raw);
      const section = list?.sections?.find((item) => item.section === list.current) ?? list?.sections?.[0];
      start = Number(section?.pageMargins?.startTwips);
    } catch {
      start = Number.NaN;
    }
    ed.reviewColumnStartMemo.set(node, start);
    return start;
  }

  // Enable the three-state mode control (Editing / Suggesting / Viewing) once a
  // document is loaded (its per-button pressed state is owned by setReviewMode),
  // and reflect the review-sidebar workflow controls' availability: Next/Previous
  // and Accept-all/Reject-all need at least one tracked change, and bulk decisions
  // are hidden in read-only Viewing mode (REVIEW-GAP-018).
  function updateReviewControls() {
    // A document the engine will not let anyone edit cannot offer Editing or
    // Suggesting: the buttons are disabled WITH the reason, not left live. And the
    // banner's "Switch to editing" is withdrawn rather than disabled — an offer
    // that can never be taken is a dead control. Ahead of the `doc` guard: a
    // container's grant is knowable with no document open.
    reflectReviewModeAccess({
      buttons: ed.reviewModeButtons,
      bannerEdit: ed.viewingBannerEdit,
      capabilities: ed.HOST_CAPS,
      readOnlyReason: ed.readOnlyReason,
      withheldReason: t("capability.embedded"),
      participant: ed.SESSION.modeAuthority,
    });
    ed.reviewDisplay.reflect(); // enabled with a document, and the mode it derives
    if (!ed.doc) return;
    let count = 0;
    try { count = (JSON.parse(ed.doc.listRevisions()) ?? []).length; } catch { count = 0; }
    if (ed.reviewPrevious) ed.reviewPrevious.disabled = count === 0;
    if (ed.reviewNext) ed.reviewNext.disabled = count === 0;
    // HF-199: the container's own answer on deciding, with its reason on the
    // control — the sidebar's two buttons are a third face of Accept/Reject all.
    const refusal = reviewDecisionRefusal("review.acceptAll", ed.HOST_CAPS);
    const canDecide = count > 0 && ed.reviewMode !== "viewing" && !refusal;
    for (const button of [ed.reviewAcceptAll, ed.reviewRejectAll]) {
      if (!button) continue;
      button.disabled = !canDecide;
      if (refusal) button.title = refusal;
    }
    if (ed.reviewBulkActions) ed.reviewBulkActions.hidden = ed.reviewMode === "viewing";
  }

  /** The three review modes (docs/68 §"Suggesting mode"): `editing` applies
   *  edits directly, `suggesting` records them as tracked revisions, and
   *  `viewing` is fully read-only — no Operation reaches apply. Any unrecognized
   *  value falls back to `editing`. */
  function setReviewMode(mode, { restoreFocus = true } = {}) {
    const previous = ed.reviewMode;
    // A read-only document has one mode. Asked for another — by a shortcut, the
    // palette, or a stale click — it stays where it is and says why.
    if (ed.readOnlyReason && mode !== "viewing") {
      ed.setStatus(ed.readOnlyReason, "error");
      mode = "viewing";
    }
    ed.reviewMode =
      mode === "suggesting" ? "suggesting" : mode === "viewing" ? "viewing" : "editing";
    // The banners announce a MODE, so they show only where a mode can change: in a
    // container granted neither `edit` nor `comment` the mode IS the container, and
    // composition is silent (`docs/126`).
    ed.reflectReviewBanners(ed.containerCanEdit ? ed.reviewMode : "", ed.readOnlyReason);
    // The banner's "Switch to editing" offer is `updateReviewControls`'s, below,
    // so the segments and the banner cannot disagree about whether editing is possible.
    ed.reflectChrome(); // every edge — `readOnlyReason`, a preview's two, and the MODE
    aiAccessMayHaveChanged(); // Casual AI's effective access follows the mode (`docs/183` §23.3)
    for (const button of ed.reviewModeButtons) {
      button.setAttribute("aria-pressed", String(button.dataset.reviewMode === ed.reviewMode));
    }
    // Announce a genuine user mode change (not the load-time reset to Editing).
    if (ed.reviewMode !== previous) {
      ed.announceReview(`${ed.reviewMode[0].toUpperCase()}${ed.reviewMode.slice(1)} mode`);
      ed.disarmFormatPainter(); // a mode change disarms the painter (its apply path may differ)
    }
    // Entering Suggesting auto-enables the markup view so a reviewer immediately
    // sees struck deletions + author-colored insertions (Word/Docs behavior, Q1).
    // Leaving Suggesting keeps whatever the reader last chose — the toggle stays
    // manual in Editing/Viewing. `setShowingChanges` is idempotent and re-renders
    // only on a real change, so this is a no-op when markup is already shown.
    if (ed.reviewMode === "suggesting" && !ed.showingChanges) {
      void ed.setShowingChanges(true);
    }
    ed.reviewTracking.apply({ doc: ed.doc, mode: ed.reviewMode, byUser: restoreFocus && !ed.hostSession?.executing, caret: ed.selection?.focus, runEdit: ed.runEdit }); // HF-131; HF-283: a host's switch is not the reader's
    updateReviewControls();
    ed.drawSelection();
    // Toolbar controls must not retain focus after changing mode: clipboard,
    // typing, and deletion events are deliberately accepted only while the
    // canvas editor owns focus. `restoreFocus: false` is for callers where the mode
    // change is not the user's gesture at all — opening a document, a demo preset,
    // entering and leaving a version preview — because those arrive while the user
    // is working somewhere else (the version TIMELINE above all, which is how a
    // preview is left again); moving focus out from under them there is not a
    // safety measure, it is losing their place.
    if (restoreFocus) ed.focusEditorSurface();
  }

  function reviewRangeClientRect(startNode, startOffset, endNode, endOffset) {
    let rects = ed.doc?.selectionRects(startNode, startOffset, endNode, endOffset) ?? [];
    if (rects.length < 5 && startNode === endNode && startOffset === endOffset) {
      rects = ed.doc?.caretRect(startNode, startOffset) ?? [];
    }
    if (rects.length < 5) return null;
    const [pageNumber, x, y, width, height] = rects;
    const page = ed.pages[pageNumber - 1];
    if (!page) return null;
    const { rect: canvasRect, sx, sy } = ed.scaleOf(page);
    return {
      pageNumber,
      left: canvasRect.left + x * sx,
      right: canvasRect.left + (x + width) * sx,
      top: canvasRect.top + y * sy,
      bottom: canvasRect.top + (y + height) * sy,
      pageRight: canvasRect.right,
    };
  }

  function revisionRange(revision) {
    if (revision?.anchor?.node) {
      return {
        startNode: revision.anchor.node,
        startOffset: Number(revision.anchor.start) || 0,
        endNode: revision.anchor.node,
        endOffset: Number(revision.anchor.end) || Number(revision.anchor.start) || 0,
      };
    }
    const text = String(revision?.text || "");
    if (!ed.doc || !text) return null;
    const first = ed.doc.firstPosition();
    const match = ed.doc.findText(text, first.node, first.offset, true, false);
    first.free();
    if (!match.found) { match.free(); return null; }
    const range = {
      startNode: match.startNode,
      startOffset: match.startOffset,
      endNode: match.endNode,
      endOffset: match.endOffset,
    };
    match.free();
    return range;
  }

  /** The right-margin comment affordance (`review_chrome.mjs`). Placed against the
   *  caret's line — or a selection's FIRST line, endpoints put in document order
   *  so a backwards drag still anchors where the selection starts. A collapsed
   *  selection falls through to `caretRect`, which is what keeps the button on the
   *  page while nothing is selected: dimmed, since `review.comment` needs a
   *  range. */
  const commentAffordance = createCommentAffordance({
    button: ed.reviewMarginCommentBtn,
    viewport: ed.viewportEl,
    rect: () => {
      if (!ed.doc || !ed.selection) return null;
      const [start, end] = orderedSelectionEnds(ed.selection, ed.doc);
      return reviewRangeClientRect(start.node, start.offset, end.node, end.offset);
    },
  });

  function scheduleReviewMarginRender() {
    if (ed.reviewMarginFrame) return;
    ed.reviewMarginFrame = requestAnimationFrame(() => {
      ed.reviewMarginFrame = 0;
      renderReviewMarginItems();
    });
  }

  /** The in-place editor for `comment` when it is the one being edited (RC-11), else `null`. */
  function commentEditorFor(comment, kind) {
    const draft = ed.commentEdit.draftFor(comment.id);
    if (draft === null) return null;
    const done = () => { ed.commentEdit.end(); scheduleReviewMarginRender(); ed.focusEditorSurface(); };
    const editor = commentEditor({ text: draft, kind, t, onInput: (text) => ed.commentEdit.update(comment.id, text), onCancel: done,
      onSave: async (text) => {
        await ed.runEdit(() => ed.doc.updateComment(comment.id, text), { keepView: true });
        ed.announceReview(t(kind === "reply" ? "reviewCard.replyUpdated" : "reviewCard.commentUpdated"));
        done();
      } });
    requestAnimationFrame(() => { if (editor.root.isConnected && document.activeElement !== editor.field) editor.focus(); });
    return editor.root;
  }
  function renderReviewMarginItems() {
    ed.reviewSidebarBody.replaceChildren();
    if (!ed.doc || !ed.pages.length) {
      ed.reviewSidebar.hidden = true;
      commentAffordance.sync(false);
      ed.viewportEl.classList.remove("has-review-sidebar");
      ed.reviewLayout = [];
      ed.reviewCardCache.clear();
      return;
    }
    const summary = ed.readReviewData(ed.doc);
    const items = [];
    const comments = summary.comments ?? [];
    updateReviewControls();
    for (const comment of comments) {
      if (!comment.anchor?.node) continue;
      // Open / Resolved / All comment filter (REVIEW-GAP-018/019). A reply always
      // follows its root's visibility so a thread is never half-shown: match on the
      // root comment's resolved state when this is a reply.
      if (ed.reviewFilter !== "all") {
        const root = comment.parentParaId
          ? comments.find((c) => c.paraId === comment.parentParaId) ?? comment
          : comment;
        const resolved = !!root.resolved;
        if (ed.reviewFilter === "resolved" ? !resolved : resolved) continue;
      }
      const rect = reviewRangeClientRect(comment.anchor.node, Number(comment.anchor.start) || 0, comment.anchor.node, Number(comment.anchor.end) || 0);
      if (rect) items.push({ type: "comment", data: comment, rect, quote: ed.commentQuotes.read(ed.doc, comment) });
    }
    const revisionItems = [];
    const groupedRevisions = new Map();
    const groupedMoves = new Map();
    for (const revision of summary.revisions ?? []) {
      if (revision.movePair?.fromStart && revision.movePair?.toStart) {
        const moveKey = `${revision.movePair.fromStart}:${revision.movePair.toStart}`;
        const group = groupedMoves.get(moveKey) ?? [];
        group.push(revision);
        groupedMoves.set(moveKey, group);
        continue;
      }
      const groupId = String(revision.groupId || "");
      const groupKind = String(revision.groupKind || "");
      if (groupId && ["typing", "replacement", "formatting"].includes(groupKind)) {
        const group = groupedRevisions.get(groupId) ?? [];
        group.push(revision);
        groupedRevisions.set(groupId, group);
      } else {
        revisionItems.push(revision);
      }
    }
    for (const [groupId, revisions] of groupedRevisions) {
      const ranges = revisions.map(revisionRange).filter(Boolean);
      const node = ranges[0]?.startNode;
      if (!node || ranges.some((range) => range.startNode !== node || range.endNode !== node)) {
        revisionItems.push(...revisions);
        continue;
      }
      const deletions = revisions.filter((revision) => revision.kind === "deletion");
      const insertions = revisions.filter((revision) => revision.kind === "insertion");
      const formatting = revisions.filter((revision) => revision.kind === "formatting");
      const groupKind = String(revisions[0]?.groupKind || "");
      const kind = groupKind === "formatting"
        ? "formatting"
        : groupKind === "replacement" || deletions.length > 0
          ? "replacement"
          : "insertion";
      revisionItems.push({
        id: groupId,
        groupId,
        kind,
        author: revisions.find((revision) => revision.author)?.author,
        date: revisions.find((revision) => revision.date)?.date,
        text: (kind === "formatting" ? formatting : kind === "insertion" ? insertions : deletions)
          .map((revision) => String(revision.text || "")).join(""),
        oldText: deletions.map((revision) => String(revision.text || "")).join(""),
        newText: insertions.map((revision) => String(revision.text || "")).join(""),
        formattingDelta: formatting.flatMap((revision) =>
          Array.isArray(revision.formattingDelta) ? revision.formattingDelta : []),
        anchor: {
          node,
          start: Math.min(...ranges.map((range) => range.startOffset)),
          end: Math.max(...ranges.map((range) => range.endOffset)),
        },
        revisions,
      });
    }
    for (const [moveKey, revisions] of groupedMoves) {
      const source = revisions.filter((revision) => revision.kind === "move_from");
      const destination = revisions.filter((revision) => revision.kind === "move_to");
      const ranges = revisions.map(revisionRange).filter(Boolean);
      if (!source.length || !destination.length || ranges.length !== revisions.length) {
        revisionItems.push(...revisions);
        continue;
      }
      const movePair = revisions[0].movePair;
      revisionItems.push({
        id: `move:${moveKey}`,
        kind: "move",
        author: revisions.find((revision) => revision.author)?.author,
        date: revisions.find((revision) => revision.date)?.date,
        text: destination.map((revision) => String(revision.text || "")).join(""),
        oldText: source.map((revision) => String(revision.text || "")).join(""),
        newText: destination.map((revision) => String(revision.text || "")).join(""),
        anchor: source[0].anchor,
        destinationAnchor: destination[0].anchor,
        movePair,
        ranges,
        revisions,
      });
    }
    // Tracked changes are not "resolved" — the Resolved filter is comment-only, so
    // it hides revisions; Open and All show them (REVIEW-GAP-018/019).
    for (const revision of ed.reviewFilter === "resolved" ? [] : revisionItems) {
      const ranges = revision.ranges ?? [revisionRange(revision)].filter(Boolean);
      const positioned = ranges
        .map((range) => ({
          range,
          rect: reviewRangeClientRect(
            range.startNode,
            range.startOffset,
            range.endNode,
            range.endOffset,
          ),
        }))
        .filter((item) => item.rect)
        .sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);
      if (positioned.length) {
        items.push({
          type: "revision",
          data: revision,
          range: positioned[0].range,
          rect: positioned[0].rect,
          ranges: positioned.map((item) => item.range),
        });
      }
    }
    if (ed.reviewComposerState?.range) {
      const { start, end } = ed.reviewComposerState.range;
      const rect = reviewRangeClientRect(start.node, start.offset, end.node, end.offset);
      if (rect) items.push({ type: "composer", data: ed.reviewComposerState, rect });
    }
    items.sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);

    // Suggesting mode reserves the column whether or not a change exists yet.
    // Keying off `items.length` meant the gutter appeared with the FIRST tracked
    // change, so the whole page slid 85px sideways the moment a reviewer began
    // editing — and slid back on Undo. Everything the user was aiming at moved
    // under the pointer, which is the worst possible moment for the page to jump.
    const show = ed.reviewSidebarPreference ?? (items.length > 0 || ed.reviewMode === "suggesting");
    ed.reviewSidebar.hidden = !show;
    // One panel per side (approved design P1): the review sidebar is on the
    // right, so a left-hand Outline or Pages stays open beside it.
    // Reserve the comment column's width in the page stack only while the column
    // is shown, so pages stay centered-ish and the single `.viewport` scrollbar
    // sits past the comments (never between the canvas and the comments).
    ed.viewportEl.classList.toggle("has-review-sidebar", show);
    // HF-088: below `REVIEW_SHEET_MAX_WIDTH` there is no margin to put a column
    // in — a 320px column over a 390px window leaves the document about 70px and
    // covers the text it annotates. The column becomes a bottom sheet instead:
    // the page keeps its full width and the top half of the screen, and the
    // comments are a scrollable, dismissable list.
    const sheet = show && ed.reviewSheetMode();
    ed.viewportEl.classList.toggle("review-sheet", sheet);
    // The margin's affordance belongs to the margin's EMPTY state: shown exactly
    // when the column is not. Deliberately NOT also gated on `reviewSheetMode()`
    // — at the sheet's widths the page is already wider than the window, so
    // `commentAffordanceSpot` refuses on the geometry first. See its note.
    // …and only where this container annotates at all: the margin button is
    // `review.comment`'s second face, and a preview is a picture of a document
    // rather than a place to say something about one.
    commentAffordance.sync(!show && ed.HOST_CAPS.has("comment"), ed.bandOffset);
    ed.reviewBtn.setAttribute("aria-pressed", String(show));
    ed.railReview.setAttribute("aria-pressed", String(show));
    if (!show) {
      ed.reviewLayout = [];
      return;
    }

    // The comment layer rides inside `.viewport`'s single scroll context; its body
    // spans the page-stack height so the transparent margin remains click-to-
    // deselect and cards are never clipped. No scrollTop sync: one scroll owner.
    const viewportRect = ed.viewportEl.getBoundingClientRect();
    ed.reviewSidebarBody.style.height = `${Math.max(ed.pagesEl.scrollHeight, ed.viewportEl.clientHeight)}px`;
    // Cancel the sticky header's flow height so cards stay pixel-aligned to their
    // canvas anchors (the header floats above via `position: sticky`). In the
    // sheet there are no canvas anchors to align to and the header is the sheet's
    // own title bar, so that offset would hide the first card behind it.
    if (ed.reviewSidebarHeader) {
      ed.reviewSidebarBody.style.marginTop = sheet
        ? "0px"
        : `-${ed.reviewSidebarHeader.offsetHeight}px`;
    }

    if (!items.length) {
      // The sheet is only as tall as its content; the page-height reservation
      // above is a margin-column idea and would leave the sheet scrolling over
      // an empty area taller than the phone.
      if (sheet) ed.reviewSidebarBody.style.height = "auto";
      const empty = document.createElement("div");
      empty.className = "review-sidebar-empty";
      empty.innerHTML = '<span class="ms" aria-hidden="true">chat_bubble_outline</span><br>No comments or suggestions yet.<br>Select text and choose Add comment.';
      ed.reviewSidebarBody.appendChild(empty);
      ed.reviewLayout = [];
      ed.reviewCardCache.clear();
      return;
    }

    // Build (or reuse) each item's card, but do not mount them all: compute every
    // card's stacked document-scroll position once here, then mount only the
    // viewport window (`mountReviewWindow`). A collapsed card whose content
    // signature is unchanged is reused from `reviewCardCache` without rebuilding
    // or re-measuring; the active/expanded card and the composer always rebuild so
    // their live controls stay fresh (REVIEW-GAP-020).
    const built = [];
    ed.reviewAnchorIndex = [];
    for (const item of items) {
      const itemId = item.type === "composer" ? "composer" : `${item.type}:${item.data.id}`;
      const anchor = item.data?.anchor;
      if (item.type !== "composer" && anchor?.node) {
        ed.reviewAnchorIndex.push({
          itemId,
          type: item.type,
          dataId: item.data.id,
          node: anchor.node,
          start: Number(anchor.start) || 0,
          end: Number(anchor.end) || Number(anchor.start) || 0,
        });
      }
      const expanded = ed.activeReviewItemId === itemId;
      const sig = reviewCardSignature(item, comments, {
        activeItemId: ed.activeReviewItemId,
        deleteConfirmId: ed.reviewDeleteConfirmId,
        editingId: ed.commentEdit.id(),
      });
      let entry = ed.reviewCardCache.get(itemId);
      if (entry && entry.sig === sig && item.type !== "composer" && !expanded) {
        built.push({ itemId, item, entry });
        continue;
      }
      if (item.type === "composer") {
        const composer = document.createElement("article");
        composer.className = "review-margin-card review-margin-composer expanded";
        composer.setAttribute("role", "group");
        // Through the catalogue: the ribbon's own Add comment button already has
        // this sentence translated into all eighteen languages, and a second,
        // English-only spelling of one name is how two surfaces start to disagree.
        composer.setAttribute("aria-label", t("panelReview.addComment.label"));
        const textarea = document.createElement("textarea");
        textarea.rows = 3;
        textarea.maxLength = 4096;
        textarea.placeholder = "Add a comment…";
        textarea.dataset.testid = "review-comment-composer";
        const actions = document.createElement("div");
        actions.className = "review-composer-actions";
        const cancel = reviewCardButton("Cancel", () => { ed.closeReviewPopover(); ed.focusEditorSurface(); });
        cancel.dataset.testid = "review-comment-cancel";
        // `@` asks the host's directory, when there is one (`docs/175` §7). Attached
        // BEFORE the composer keys, so Enter picks a name while the list is open.
        const mentioning = attachCommentMentions(ed, textarea, t);
        const submit = reviewCardButton("Comment", async () => {
          const text = textarea.value.trim();
          if (!text || !ed.reviewComposerState?.range) return;
          const { start, end } = ed.reviewComposerState.range;
          const metadata = ed.currentReviewTimestamp();
          await mentioning.post(text, {
            plain: (doc) => doc.addComment(start.node, start.offset, end.node, end.offset, text, undefined, undefined, metadata.date),
            mentioning: (doc, spans) => doc.addCommentMentioning(start.node, start.offset, end.node, end.offset, text, spans, undefined, undefined, metadata.date),
          });
          ed.reviewComposerState = null;
          ed.reviewSidebarPreference = true;
          ed.announceReview("Comment added");
          ed.drawSelection();
          ed.focusEditorSurface();
        }, false, "Add comment");
        submit.dataset.testid = "review-comment-submit";
        // Enter submits, Shift+Enter is a newline, Esc cancels — the modern
        // Docs/Word comment interaction, identical to the reply composer (Q5).
        attachComposerKeys(textarea, {
          onSubmit: () => submit.click(),
          onCancel: () => cancel.click(),
        });
        autoGrowTextarea(textarea);
        actions.append(cancel, submit);
        composer.append(textarea, actions);
        entry = { el: composer, sig, height: 0, measured: false, focusTextarea: textarea, needsFocus: true };
        ed.reviewCardCache.set(itemId, entry);
        built.push({ itemId, item, entry });
        continue;
      }

      const card = document.createElement("article");
      const revisionKind = item.type === "revision" ? ` review-margin-${String(item.data.kind || "change").replaceAll("_", "-")}` : "";
      card.className = `review-margin-card review-margin-${item.type}${revisionKind}${item.data.resolved ? " resolved" : ""}${expanded ? " expanded" : ""}`;
      card.tabIndex = 0;
      card.dataset.reviewItemId = itemId;
      card.setAttribute("aria-expanded", String(expanded));
      // A labelled, expandable group so a screen reader announces what the card is
      // (who, what kind, a text snippet) and its expanded/collapsed state, instead
      // of a nameless generic article (REVIEW-GAP-023). `group` (not `button`)
      // because an expanded card contains real action buttons.
      card.setAttribute("role", "group");
      card.setAttribute("aria-label", reviewCardAriaLabel(item));
      const header = document.createElement("div");
      header.className = "review-margin-card-head";
      const avatar = document.createElement("span");
      avatar.className = "review-margin-avatar";
      const authorName = item.data.author || item.data.initials || "You";
      avatar.textContent = authorName.trim().slice(0, 1).toUpperCase() || "U";
      // Per-author color (docs/81 REVIEW-GAP-015): the avatar chip is filled with
      // the author's stable palette color so the same reviewer is recognizable
      // across their comments and tracked changes, matching the inline markers.
      const authorColor = reviewAuthorColor(reviewAuthorKey(item.data));
      avatar.style.setProperty("--review-author-color", authorColor);
      // Attribution tooltip on the whole card: author · type · date for a tracked
      // change, author · state · date for a comment (reuses the native `title`
      // tooltip pattern used across the editor's chrome).
      card.title = item.type === "revision"
        ? reviewRevisionTooltip(item.data)
        : reviewCommentTooltip(item.data);
      const title = document.createElement("div");
      title.className = "review-margin-title";
      const author = document.createElement("strong");
      author.textContent = authorName;
      const meta = document.createElement("small");
      meta.textContent = item.type === "revision"
        ? formatReviewDate(item.data.date)
        : `${item.data.resolved ? "Resolved · " : ""}${formatReviewDate(item.data.date)}`;
      title.append(author, meta);
      header.append(avatar, title);
      if (expanded && item.type === "comment") {
        header.append(
          reviewIconButton(item.data.resolved ? "undo" : "check", item.data.resolved ? "Reopen comment" : "Resolve comment", async () => {
            const resolving = !item.data.resolved;
            await ed.runEdit(() => ed.doc.setCommentResolved(item.data.id, resolving), { keepView: true });
            ed.announceReview(resolving ? "Comment resolved" : "Comment reopened");
            if (resolving) {
              ed.activeReviewItemId = null;
              ed.activeReviewCommentId = null;
              ed.reviewDeleteConfirmId = null;
              if (item.data.anchor?.node) {
                const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
                ed.selection = {
                  anchor: { node: item.data.anchor.node, offset: end },
                  focus: { node: item.data.anchor.node, offset: end },
                };
              }
            }
            ed.drawSelection();
            ed.focusEditorSurface();
          }),
          ...(item.data.resolved ? [] : [commentEditButton(t, () => {
            ed.commentEdit.begin(item.data.id, item.data.text);
            scheduleReviewMarginRender();
          })]),
          reviewIconButton("more_vert", "More options", () => {
            ed.reviewDeleteConfirmId = ed.reviewDeleteConfirmId === item.data.id ? null : item.data.id;
            scheduleReviewMarginRender();
          }),
        );
      }
      const body = document.createElement("p");
      body.className = "review-margin-body";
      if (item.type === "revision") {
        if (item.data.kind === "replacement") {
          body.textContent = `Replaced “${item.data.oldText}” with “${item.data.newText}”`;
        } else if (item.data.kind === "formatting") {
          const target = item.data.text || item.data.newText || item.data.oldText;
          const details = reviewFormattingDescription(item.data.formattingDelta);
          body.textContent = `Changed formatting for “${target}”${details ? `\n${details}` : ""}`;
        } else if (item.data.kind === "move") {
          body.textContent = `Moved “${item.data.newText || item.data.oldText}”`;
        } else if (item.data.kind === "paragraph_format") {
          const details = reviewFormattingDescription(item.data.formattingDelta);
          body.textContent = `Formatted paragraph${details ? `\n${details}` : ""}`;
        } else if (item.data.kind === "paragraph_mark_insertion") {
          body.textContent = "Added a paragraph break";
        } else if (item.data.kind === "paragraph_mark_deletion") {
          // Say what accepting will do: the break goes and the paragraph joins the
          // next one, which is not obvious from "deleted" alone.
          body.textContent = "Deleted a paragraph break — accepting joins this paragraph to the next";
        } else {
          const verb = item.data.kind === "deletion"
            ? "Deleted"
            : item.data.kind === "insertion"
              ? "Added"
              : item.data.kind === "move_from"
                ? "Unpaired move source"
                : item.data.kind === "move_to"
                  ? "Unpaired move destination"
                  : "Changed";
          body.textContent = `${verb} “${String(item.data.text || "")}”`;
        }
      } else {
        body.textContent = String(item.data.text || "");
      }
      const quote = item.type === "comment" ? commentQuoteElement(item.quote, t) : null;
      const bodyOrEditor = item.type === "comment" && expanded ? commentEditorFor(item.data, "comment") ?? body : body;
      const actions = document.createElement("div");
      actions.className = "review-margin-card-actions";
      if (item.type === "comment") {
        if (ed.reviewDeleteConfirmId === item.data.id) {
          const prompt = document.createElement("span");
          prompt.textContent = "Delete this thread?";
          actions.append(
            prompt,
            reviewCardButton("Delete", async () => {
              ed.reviewDeleteConfirmId = null;
              ed.activeReviewItemId = null;
              await ed.runEdit(() => ed.doc.deleteComment(item.data.id), { keepView: true });
              ed.drawSelection();
              ed.focusEditorSurface();
            }, true),
            reviewCardButton("Cancel", () => {
              ed.reviewDeleteConfirmId = null;
              scheduleReviewMarginRender();
            }),
          );
        }
      } else if (!["move_from", "move_to"].includes(item.data.kind) || item.data.movePair) {
        const changeLabel = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
        const decisionRefusal = reviewDecisionRefusal("review.accept", ed.HOST_CAPS);
        actions.append(
          reviewCardButton("Accept", async () => {
            if (!mayDecideReview(ed.HOST_CAPS, ed.setStatus)) return;
            await ed.runEdit(() => item.data.movePair
              ? ed.doc.decideMovePair(
                item.data.movePair.fromStart,
                item.data.movePair.toStart,
                true,
              )
              : item.data.groupId
                ? ed.doc.decideRevisionGroup(item.data.groupId, true)
                : ed.doc.decideRevision(item.data.id, true));
            ed.announceReview(`Accepted ${changeLabel}`);
            ed.drawSelection();
            ed.focusEditorSurface();
          }, false, `Accept this ${changeLabel}`),
          reviewCardButton("Reject", async () => {
            if (!mayDecideReview(ed.HOST_CAPS, ed.setStatus)) return;
            await ed.runEdit(() => item.data.movePair
              ? ed.doc.decideMovePair(
                item.data.movePair.fromStart,
                item.data.movePair.toStart,
                false,
              )
              : item.data.groupId
                ? ed.doc.decideRevisionGroup(item.data.groupId, false)
                : ed.doc.decideRevision(item.data.id, false));
            ed.announceReview(`Rejected ${changeLabel}`);
            ed.drawSelection();
            ed.focusEditorSurface();
          }, true, `Reject this ${changeLabel}`),
        );
        // HF-199: a container that may not edit may not decide. The buttons stay
        // visible and say why, rather than vanishing, so the card still reads as
        // a decision somebody else will make.
        if (decisionRefusal) {
          for (const button of actions.querySelectorAll(".review-margin-action")) {
            button.disabled = true;
            button.title = decisionRefusal;
          }
        }
      }
      const focus = () => {
        const expanding = ed.activeReviewItemId !== itemId;
        ed.activeReviewItemId = expanding ? itemId : null;
        ed.reviewDeleteConfirmId = null;
        if (item.type === "comment") {
          if (expanding) {
            ed.focusReviewComment(item.data, false);
          } else if (item.data.resolved && item.data.anchor?.node) {
            ed.activeReviewCommentId = null;
            const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
            ed.selection = {
              anchor: { node: item.data.anchor.node, offset: end },
              focus: { node: item.data.anchor.node, offset: end },
            };
            ed.drawSelection();
          }
        } else {
          ed.focusReviewRevision(item.data, false);
        }
        scheduleReviewMarginRender();
      };
      card.addEventListener("click", (event) => {
        if (event.target.closest("button, textarea, input, select")) return;
        focus();
      });
      card.addEventListener("keydown", (event) => {
        // Let inner controls (Accept/Reject, the move source/destination
        // navigation buttons, composers) handle their own Enter/Space instead
        // of also toggling the card's expansion — mirrors the click guard above.
        if (event.target.closest("button, textarea, input, select")) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          focus();
        }
      });
      card.append(header, ...(quote ? [quote] : []), bodyOrEditor);
      if (item.type === "revision" && item.data.kind === "move") {
        const path = document.createElement("div");
        path.className = "review-move-path";
        const source = ed.reviewMoveEndButton(
          "From",
          item.data.anchor,
          "Original location",
          "Go to the moved text's original location",
        );
        const arrow = document.createElement("span");
        arrow.className = "ms";
        arrow.setAttribute("aria-hidden", "true");
        arrow.textContent = "arrow_forward";
        const destination = ed.reviewMoveEndButton(
          "To",
          item.data.destinationAnchor,
          "New location",
          "Go to the moved text's new location",
        );
        path.append(source, arrow, destination);
        card.appendChild(path);
      }
      // REVIEW-GAP-012: thread comments that overlap this tracked change beneath
      // it, read-only, on an expanded revision card. `revisionThread` reports the
      // overlap; authoring a reply to a change is `addComment` over the change's
      // own range (unchanged DOCX comment ownership). Only rendered when the
      // overlap is non-empty, so a change with no related comment adds no chrome.
      if (item.type === "revision" && expanded && item.data.id) {
        let related = [];
        try {
          related = JSON.parse(ed.doc.revisionThread(item.data.id) || "[]");
        } catch {
          related = [];
        }
        if (related.length) {
          const relatedList = document.createElement("div");
          relatedList.className = "review-margin-revision-comments";
          const relatedLabel = document.createElement("small");
          relatedLabel.textContent = "Related comments";
          relatedList.appendChild(relatedLabel);
          for (const related_comment of related) {
            const relatedItem = document.createElement("div");
            relatedItem.className = "review-margin-revision-comment";
            const relatedAuthor = document.createElement("strong");
            relatedAuthor.textContent =
              related_comment.author || related_comment.initials || "You";
            const relatedText = document.createElement("p");
            relatedText.textContent = String(related_comment.text || "");
            relatedItem.append(relatedAuthor, relatedText);
            relatedList.appendChild(relatedItem);
          }
          card.appendChild(relatedList);
        }
      }
      if (item.type === "comment") {
        const replies = comments.filter((comment) => reviewCommentIsReplyTo(comment, item.data));
        if (replies.length) {
          const thread = document.createElement("div");
          thread.className = "review-margin-replies";
          for (const reply of replies) {
            const replyItem = document.createElement("div");
            replyItem.className = `review-margin-reply${reply.resolved ? " resolved" : ""}`;
            const replyHead = document.createElement("div");
            replyHead.className = "review-margin-reply-head";
            const replyAuthor = document.createElement("strong");
            replyAuthor.textContent = reply.author || reply.initials || "You";
            const replyDate = document.createElement("small");
            replyDate.textContent = `${reply.resolved ? "Resolved · " : ""}${formatReviewDate(reply.date)}`;
            replyHead.append(replyAuthor, replyDate);
            const replyBody = document.createElement("p");
            replyBody.textContent = String(reply.text || "");
            replyItem.append(replyHead, (expanded && commentEditorFor(reply, "reply")) || replyBody);
            // REVIEW-GAP-011: edit or delete this specific reply (not just the
            // whole thread). Only on an expanded, unresolved parent card, so the
            // controls stay out of the collapsed/resolved presentation.
            if (expanded && !item.data.resolved) {
              const replyActionRow = document.createElement("div");
              replyActionRow.className = "review-margin-reply-actions";
              const editReply = reviewCardButton(t("reviewCard.edit"), () => {
                ed.commentEdit.begin(reply.id, reply.text);
                scheduleReviewMarginRender();
              }, false, t("reviewCard.editReply"));
              const deleteReply = reviewCardButton(
                "Delete",
                async () => {
                  await ed.runEdit(() => ed.doc.deleteReply(reply.id));
                  ed.announceReview("Reply deleted");
                  ed.drawSelection();
                  ed.focusEditorSurface();
                },
                true,
                "Delete reply",
              );
              replyActionRow.append(editReply, deleteReply);
              replyItem.appendChild(replyActionRow);
            }
            thread.appendChild(replyItem);
          }
          card.appendChild(thread);
        }
        if (expanded && !item.data.resolved) {
          // Always-ready multi-line reply composer (Q5): a textarea the user can
          // type into immediately — no click-to-arm step — matching modern Docs/
          // Word comment threads. It auto-grows; Enter submits, Shift+Enter is a
          // newline, Esc clears. The action row surfaces only once the composer
          // is focused or has text, so a collapsed thread stays dense.
          const replyComposer = document.createElement("div");
          replyComposer.className = "review-reply-composer";
          const textarea = document.createElement("textarea");
          textarea.rows = 1;
          textarea.maxLength = 4096;
          textarea.placeholder = "Reply…";
          textarea.dataset.testid = "review-reply-composer";
          textarea.setAttribute("aria-label", "Reply to this comment");
          const replyActions = document.createElement("div");
          replyActions.className = "review-composer-actions";
          const resize = autoGrowTextarea(textarea);
          const clear = () => {
            textarea.value = "";
            resize();
            syncActions();
          };
          // Before the composer keys, as in the new-comment composer.
          const mentioning = attachCommentMentions(ed, textarea, t);
          const submit = reviewCardButton("Reply", async () => {
            const text = textarea.value.trim();
            if (!text) return;
            const metadata = ed.currentReviewTimestamp();
            await mentioning.post(text, {
              plain: (doc) => doc.replyToComment(item.data.id, text, undefined, undefined, metadata.date),
              mentioning: (doc, spans) => doc.replyMentioning(item.data.id, text, spans, undefined, undefined, metadata.date),
            });
            ed.announceReview("Reply added");
            ed.drawSelection();
            ed.focusEditorSurface();
          }, false, "Send reply");
          const cancel = reviewCardButton("Cancel", () => {
            clear();
            ed.focusEditorSurface();
          });
          replyActions.append(cancel, submit);
          const syncActions = () => {
            const active = document.activeElement === textarea || textarea.value.trim().length > 0;
            replyActions.hidden = !active;
          };
          syncActions();
          textarea.addEventListener("pointerdown", (event) => event.stopPropagation());
          textarea.addEventListener("input", syncActions);
          textarea.addEventListener("focus", syncActions);
          textarea.addEventListener("blur", () => requestAnimationFrame(syncActions));
          attachComposerKeys(textarea, {
            onSubmit: () => submit.click(),
            onCancel: () => cancel.click(),
          });
          replyComposer.append(textarea, replyActions);
          card.appendChild(replyComposer);
        }
      }
      if (actions.childElementCount) card.appendChild(actions);
      entry = { el: card, sig, height: 0, measured: false };
      ed.reviewCardCache.set(itemId, entry);
      built.push({ itemId, item, entry });
    }

    // Measure only the freshly built cards, batched: mount them all off-view, read
    // every height (one layout for all reads, since no DOM mutation interleaves),
    // then detach them. Reused cards keep their cached measured height, so a large
    // review set is not re-measured on every edit.
    const toMeasure = built.filter(({ entry }) => !entry.measured);
    for (const { entry } of toMeasure) {
      entry.el.style.top = "-99999px";
      ed.reviewSidebarBody.appendChild(entry.el);
    }
    for (const { entry } of toMeasure) {
      entry.height = entry.el.offsetHeight;
      entry.measured = true;
    }
    for (const { entry } of toMeasure) ed.reviewSidebarBody.removeChild(entry.el);

    // Position pass. `stackReviewCards` owns the arithmetic for both shapes: the
    // anchored margin column, and the narrow-width bottom sheet where cards are a
    // plain list (HF-088). Anchors are still computed for the margin shape in
    // document-scroll coordinates, from each item's on-canvas marker.
    const seen = new Set();
    // BAND coordinates, not scroll coordinates. `bandOffset` moves on every
    // scroll once a document is compressed (`page_scroll.mjs`), so an anchor
    // stored in scroll coordinates drifts from its marker by (scale - 1) × the
    // distance scrolled; subtracting it here and adding it back at mount time
    // pins the card. At scale 1 the offset is 0 and nothing changes.
    const anchorY = built.map(
      ({ item }) => item.rect.top - viewportRect.top + ed.viewportEl.scrollTop - ed.bandOffset,
    );
    const heights = built.map(({ entry }) => entry.height);
    const tops = stackReviewCards({
      anchorY,
      heights,
      activeIndex: ed.activeReviewItemId
        ? built.findIndex((b) => b.itemId === ed.activeReviewItemId)
        : -1,
      sheet,
    });
    const layout = built.map(({ itemId, entry }, i) => {
      seen.add(itemId);
      return { itemId, top: tops[i], entry };
    });
    // In the sheet the card body is the scrolled content, so it must be exactly as
    // tall as the stack; in the margin it spans the page stack (set above).
    if (sheet) {
      ed.reviewSidebarBody.style.height = `${Math.round(reviewStackHeight(tops, heights))}px`;
    }
    // Drop cache entries (and their retained DOM) for items no longer present.
    for (const key of [...ed.reviewCardCache.keys()]) {
      if (!seen.has(key)) ed.reviewCardCache.delete(key);
    }
    ed.reviewLayout = layout;
    mountReviewWindow();
  }

  /** Mounts only the cards whose precomputed position falls inside (or within
   *  `REVIEW_WINDOW_OVERSCAN` of) the viewport, and detaches the rest. Runs after
   *  a full render and on every scroll frame; it never re-parses the review
   *  payload, recomputes geometry, or rebuilds a card — it only attaches/detaches
   *  retained DOM by comparing cached positions to the current scroll band. This
   *  is what keeps the mounted card count bounded regardless of review size
   *  (REVIEW-GAP-020). */
  function mountReviewWindow() {
    if (ed.reviewSidebar.hidden) return;
    // Whichever element owns the scroll owns the window: the viewport in the
    // margin shape, the sheet itself when the sheet is the scroller (HF-088).
    const scroller = ed.reviewSheetMode() ? ed.reviewSidebar : ed.viewportEl;
    const scrollTop = scroller.scrollTop;
    // Stored in band coordinates; the live offset converts them back. Here
    // rather than in the layout pass because this runs on every scroll frame.
    // The bottom sheet scrolls itself and is never the compressed band, so its
    // offset is 0 — taking it unconditionally keeps one expression for both.
    const offset = ed.reviewSheetMode() ? 0 : ed.bandOffset;
    const bandTop = scrollTop - offset - ed.REVIEW_WINDOW_OVERSCAN;
    const bandBottom = scrollTop - offset + scroller.clientHeight + ed.REVIEW_WINDOW_OVERSCAN;
    for (const { itemId, top, entry } of ed.reviewLayout) {
      // The composer and the active/expanded card are always kept mounted: they
      // own live focus/controls the user is interacting with.
      const force = itemId === "composer" || itemId === ed.activeReviewItemId;
      const visible = force || (top + entry.height >= bandTop && top <= bandBottom);
      const mounted = entry.el.parentNode === ed.reviewSidebarBody;
      if (visible) {
        entry.el.style.top = `${Math.round(top + offset)}px`;
        if (!mounted) ed.reviewSidebarBody.appendChild(entry.el);
        if (entry.needsFocus && entry.focusTextarea) {
          entry.needsFocus = false;
          const textarea = entry.focusTextarea;
          requestAnimationFrame(() => textarea.focus({ preventScroll: true }));
        }
      } else if (mounted) {
        ed.reviewSidebarBody.removeChild(entry.el);
      }
    }
  }

  /** rAF-debounced `mountReviewWindow` for the scroll path: re-windowing is cheap
   *  (no parse/geometry/rebuild), but coalescing to one call per frame keeps a
   *  fast scroll from doing redundant DOM work. */
  function scheduleReviewWindow() {
    if (ed.reviewWindowFrame) return;
    ed.reviewWindowFrame = requestAnimationFrame(() => {
      ed.reviewWindowFrame = 0;
      commentAffordance.ride(ed.bandOffset);
      mountReviewWindow();
    });
  }

  // One scroll owner: the comment layer lives inside `.viewport` and rides its
  // scroll context natively, so cards stay pinned to their anchored text without
  // any scroll-sync or per-frame re-render (that eliminates the momentum drift and
  // the between-canvas scrollbar). Cards are positioned in document-scroll
  // coordinates; only content edits and resizes recompute those positions. On a
  // plain scroll we only re-window the already-computed layout (mount the cards
  // entering the viewport, detach those leaving it) — no re-parse, no geometry,
  // no rebuild — so a document with hundreds of comments scrolls with a bounded,
  // viewport-sized number of mounted cards (REVIEW-GAP-020).
  ed.viewportEl.addEventListener("scroll", scheduleReviewWindow, { passive: true });
  // In the sheet the cards ride the sheet's own scroll instead (HF-088).
  ed.reviewSidebar.addEventListener("scroll", scheduleReviewWindow, { passive: true });
  ed.reviewSidebarBody.addEventListener("click", (event) => {
    if (event.target !== ed.reviewSidebarBody || !ed.activeReviewItemId) return;
    ed.activeReviewItemId = null;
    ed.activeReviewCommentId = null;
    ed.reviewDeleteConfirmId = null;
    ed.drawSelection();
  });
  window.addEventListener("resize", scheduleReviewMarginRender);
  return {
    paragraphRevisionRects, reviewRangeClientRect, revisionRange, scheduleReviewMarginRender,
    setReviewMode, updateReviewControls,
  };
}
