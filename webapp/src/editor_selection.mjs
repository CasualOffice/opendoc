// Selection and its overlays (docs/58): anchors, story keys, review markers,
// the form checkbox toggle and the point-to-twip mapping. scaleOf stays in
// main.js. Moved out of `main.js` unchanged (docs/177); it reaches the rest of
// the editor only through the context `ed`.
import { toggleTextCheckboxAt } from "./text_checkbox.mjs";
import { smallestContaining } from "./review_anchor.mjs";
import { reviewAuthorColor, reviewAuthorKey, reviewCommentTooltip, reviewRevisionTooltip } from "./review_labels.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSelection(ed) {
  // ---- Selection & copy (doc 58 pipeline: hit-test → selection → draw → copy) ---


  /** A pointer event on a page's overlay/canvas → that page's local twip point. */
  function pointToTwip(page, event) {
    const { rect, sx, sy } = ed.scaleOf(page);
    return {
      // Clamp edge/gap clicks into the page box. The layout hit tester then
      // resolves the nearest caret on that page instead of returning no hit for
      // a point just outside the rasterized sheet.
      x: Math.max(0, Math.min(page.wTwip, Math.round((event.clientX - rect.left) / sx))),
      y: Math.max(0, Math.min(page.hTwip, Math.round((event.clientY - rect.top) / sy))),
    };
  }

  /** Resolve a pointer event to a model anchor, or null if it misses content. */
  /** Identifies the story the caret is currently in — the body, a running band, or
   *  a specific text box. Two positions belong to the same range only if this
   *  agrees for both; a range across stories is not representable in
   *  WordprocessingML and no edit can apply to one. */
  function currentStoryKey() {
    if (ed.objectSelection?.mode === "editing") return `object:${ed.objectSelection.node}`;
    if (ed.runningEditBand) return `running:${ed.runningEditBand}:${ed.runningEditPage}`;
    return "body";
  }

  /** Ticks the form checkbox at the caret position `node`/`offset`, and reports
   *  whether it did.
   *
   *  The engine answers "is this inside a checkbox control", so the host never
   *  has to know where content controls are — it asks about whatever the pointer
   *  or the caret landed on. Gated through `runEdit` like every other mutation,
   *  so Viewing refuses it and Suggesting tracks it. */
  function toggleFormCheckboxAt(node, offset) {
    if (!ed.doc || !node) return false;
    let control = null;
    try {
      control = ed.doc.formCheckboxAt(node, offset ?? 0) ?? null;
    } catch {
      return false;
    }
    if (!control) return toggleTextCheckboxAt(ed.doc, node, offset, ed.runEdit);
    ed.runEdit(() => ed.doc.toggleFormCheckbox(control));
    return true;
  }

  function anchorAt(page, event) {
    const { x, y } = pointToTwip(page, event);
    // While a header or footer is open, a point inside a running band belongs to
    // THAT content: without this every click and drag in the band resolved through
    // the body walk, so the caret jumped into the body and text in the header
    // could not be selected at all. Outside the context the body walk still owns
    // margin clicks, which is what keeps entering a header deliberate.
    // Editing a text box: a point inside it belongs to the box's text, so clicks
    // and drags select within it instead of jumping out to the body.
    if (ed.objectSelection?.mode === "editing") {
      // Geometry decides the story, exactly as it does for a header band: the
      // box's own rect says whether this point belongs to the box. `textBoxHitTest`
      // snaps to the nearest line across the WHOLE page, so asking it first meant
      // a click far down in the body still resolved inside the box — the caret
      // moved into the box's text while the user was aiming at a paragraph, and
      // there was no way to click out at all.
      if (pointInsideObject(ed.objectSelection.node, page, x, y)) {
        const inBox = ed.doc.textBoxHitTest(page.pageNumber, x, y);
        if (inBox) {
          const anchor = { node: inBox.node, offset: inBox.offset };
          inBox.free?.();
          return anchor;
        }
        return null; // inside the box but it has no line to land on
      }
      // Outside the box: leave it, then resolve as an ordinary body click — the
      // same deliberate way out that leaving a header uses.
      exitObjectEditMode();
    }
    if (ed.runningEditBand) {
      // One rule, in the engine: geometry decides the story, content decides only
      // the offset inside it. The host used to infer "the user left the header"
      // from a hit-test finding no glyph, so clicking the empty space to the right
      // of a header line — or below its last line, still inside the band — threw
      // the user out of the context they were typing in.
      const hit = ed.doc.resolveClick(page.pageNumber, x, y);
      if (!hit) return null;
      const anchor = { node: hit.node, offset: hit.offset };
      const band = hit.band;
      hit.free?.();
      if (band) {
        // Still in running content: stay there, and follow the user if they moved
        // to the other band or to another page's copy.
        if (!anchor.node) {
          // That band has no content on this page. Entering it is the ask — the
          // same one the double-click makes — so probe it, and create it when the
          // document has none, rather than letting the click do nothing.
          void ed.editRunningContent(band, page);
          return null;
        }
        if (band !== ed.runningEditBand || page.pageNumber !== ed.runningEditPage) {
          ed.setRunningContext(band, page);
        }
        return anchor;
      }
      // A click in the body area leaves the context — the deliberate way back, as
      // in Word, Docs, OnlyOffice and LibreOffice.
      ed.exitRunningEdit();
      return anchor;
    }
    const hit = ed.doc.hitTest(page.pageNumber, x, y);
    if (!hit) return null;
    const anchor = { node: hit.node, offset: hit.offset };
    hit.free(); // release the WASM-owned payload; we copied out its fields
    return anchor;
  }

  /** Whether a page-local point falls inside `node`'s placed rectangle.
   *
   *  Asked of the engine's own geometry (`objectRect`), so the host never carries
   *  a second opinion about where an object is. */
  function pointInsideObject(node, page, x, y) {
    if (!ed.doc || !node) return false;
    let rect = [];
    try {
      rect = ed.doc.objectRect(node); // [page, x, y, w, h] in twips
    } catch {
      return false;
    }
    if (rect.length < 5 || rect[0] !== page.pageNumber) return false;
    return x >= rect[1] && x <= rect[1] + rect[3] && y >= rect[2] && y <= rect[2] + rect[4];
  }

  /** Leaves a text box's editing context, dropping the object selection with it —
   *  a click in the body is a click in the body, not a half-exit that leaves the
   *  box's chrome on screen. */
  function exitObjectEditMode() {
    if (!ed.objectSelection) return;
    ed.objectSelection = null;
    ed.updateObjectSelectionState();
    ed.updateObjectContextBar();
  }

  /** Clears caret/selection chrome while retaining previews of held object gestures. */
  function clearOverlays() {
    for (let i = ed.pageWindow.first; i <= ed.pageWindow.last; i++) {
      const overlay = ed.pages[i]?.overlay;
      overlay?.replaceChildren(...overlay.querySelectorAll(":scope > .object-resize-preview, :scope > .object-rotate-preview"));
    }
  }

  /** Reads the current comment list once (JSON round-trip), or `[]` on
   *  failure. Shared by marker rendering and by the caret-driven activation
   *  below so both agree on the same anchors. */
  function reviewComments() {
    if (!ed.doc) return [];
    try { return JSON.parse(ed.doc.listComments()) ?? []; } catch { return []; }
  }

  /** The comment (if any) whose anchor range contains `anchor` (a `{node,
   *  offset}` model position), matching the same open/resolved visibility rule
   *  `paintReviewMarkers` uses. Resolved comments only match while their card
   *  is already explicitly expanded. */
  function reviewCommentAtAnchor(anchor) {
    if (!anchor?.node) return null;
    for (const comment of reviewComments()) {
      const explicitlyOpen = ed.activeReviewItemId === `comment:${comment.id}`;
      if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
      if (comment.anchor.node !== anchor.node) continue;
      const start = Number(comment.anchor.start) || 0;
      const end = Number(comment.anchor.end) || 0;
      if (anchor.offset >= start && anchor.offset <= end) return comment;
    }
    return null;
  }

  /** Non-blocking side effect of an authoritative caret placement
   *  (REVIEW-GAP-005, docs/81): if the resulting caret lands inside a
   *  commented range, expand/surface that comment's card. This never touches
   *  `selection` — document hit-testing (docs/tracker/completed/retired-documents.md#doc-80) is always the sole source
   *  of truth for where the caret lands; card expansion is derived from the
   *  resulting caret, never the other way around, and never blocks or delays
   *  caret placement. */
  /** The review item (comment or revision) whose anchor range EXACTLY equals the
   *  current non-collapsed selection, or null. A review target is focused by
   *  selecting its exact range, so an exact match uniquely identifies the item the
   *  reviewer picked — even where a suggestion shares a boundary with (or nests
   *  inside) a comment, which the point-based `reviewCommentAtAnchor` /
   *  smallest-containing scans would otherwise resolve to the wrong item. */
  function reviewItemForExactSelection() {
    const a = ed.selection?.anchor;
    const f = ed.selection?.focus;
    if (!a || !f || a.node !== f.node) return null;
    const lo = Math.min(a.offset, f.offset);
    const hi = Math.max(a.offset, f.offset);
    if (lo === hi) return null; // a collapsed caret has no range to match exactly
    for (const entry of ed.reviewAnchorIndex) {
      if (entry.node === a.node && entry.start === lo && entry.end === hi) return entry;
    }
    return null;
  }

  function syncActiveReviewCommentToCaret(anchor) {
    // A freshly focused review target selects its own exact range: activate THAT
    // item so the clustered sidebar layout (REVIEW-GAP-019) anchors the selected
    // card to its own marker. Without this, selecting a suggestion that shares a
    // boundary with a comment activated the comment instead, and the suggestion's
    // card drifted from its marker (they moved together on scroll, never closing
    // the gap).
    const exact = reviewItemForExactSelection();
    if (exact) {
      ed.activeReviewItemId = exact.itemId;
      ed.activeReviewCommentId = exact.type === "comment" ? exact.dataId : null;
      ed.reviewSidebarPreference = true;
      ed.scheduleReviewMarginRender();
      return;
    }
    const comment = reviewCommentAtAnchor(anchor);
    if (comment) {
      ed.activeReviewCommentId = comment.id;
      ed.activeReviewItemId = `comment:${comment.id}`;
      ed.reviewSidebarPreference = true;
      ed.scheduleReviewMarginRender();
      return;
    }
    // No comment under the caret — surface a tracked-change card whose anchor range
    // contains the caret, so caret-driven expansion works for suggestions too
    // (REVIEW-GAP-019). The smallest containing range wins when several stack.
    if (!anchor?.node) return;
    const best = smallestContaining(ed.reviewAnchorIndex, anchor.node, Number(anchor.offset) || 0);
    if (!best || best.itemId === ed.activeReviewItemId) return;
    ed.activeReviewItemId = best.itemId;
    ed.reviewSidebarPreference = true;
    ed.scheduleReviewMarginRender();
  }

  /** Paint comment ranges in the existing overlay layer. This never touches the
   * canvas or document layout; it is the same interaction layer used by the
   * selection highlight and caret. These markers are a pure visual affordance:
   * they intentionally carry no click handler of their own, so a pointer event
   * always falls through to the page's normal pointerdown hit-testing (see
   * `onPointerDown` / `syncActiveReviewCommentToCaret`) instead of hijacking
   * caret placement (REVIEW-GAP-005). */
  function paintReviewMarkers() {
    if (!ed.doc) return;
    ed.reviewColumnStartMemo = new Map();
    const summary = ed.readReviewData(ed.doc);
    for (const comment of summary.comments ?? []) {
      const explicitlyOpen = ed.activeReviewItemId === `comment:${comment.id}`;
      if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
      const { node, start, end } = comment.anchor;
      const rects = ed.doc.selectionRects(node, Number(start) || 0, node, Number(end) || 0);
      const color = reviewAuthorColor(reviewAuthorKey(comment));
      const tooltip = reviewCommentTooltip(comment);
      for (let i = 0; i < rects.length; i += 5) {
        const el = ed.place(rects.slice(i, i + 5), ed.activeReviewCommentId === comment.id ? "review-comment-marker review-comment-marker-active" : "review-comment-marker");
        if (el) {
          el.dataset.reviewCommentId = comment.id;
          el.style.setProperty("--review-author-color", color);
          el.title = tooltip;
        }
      }
    }
    for (const revision of summary.revisions ?? []) {
      const range = ed.revisionRange(revision);
      if (!range) continue;
      const paragraphLevel = revision.kind === "paragraph_format"
        || revision.kind === "paragraph_mark_insertion"
        || revision.kind === "paragraph_mark_deletion";
      const deletionLike = revision.kind === "deletion"
        || revision.kind === "move_from"
        || revision.kind === "paragraph_mark_deletion";
      let rects = paragraphLevel
        ? ed.paragraphRevisionRects(revision)
        : ed.doc.selectionRects(range.startNode, range.startOffset, range.endNode, range.endOffset);
      if (!paragraphLevel && rects.length < 5 && deletionLike && range.startNode === range.endNode) {
        rects = ed.doc.caretRect(range.startNode, range.startOffset);
      }
      const moveItemId = revision.movePair?.fromStart && revision.movePair?.toStart
        ? `revision:move:${revision.movePair.fromStart}:${revision.movePair.toStart}`
        : null;
      // Highlight the marker whenever ITS sidebar item is active. A revision can be
      // surfaced under any of three ids depending on how the sidebar groups it — a
      // move pair, a typing/replacement/formatting group (keyed by groupId), or an
      // ungrouped revision (keyed by its own id) — so match against all three. This
      // makes a selected inline suggestion show the active state (and lets
      // `scrollReviewSelectionIntoView` target its marker), not only moves.
      const activeIds = [
        moveItemId,
        revision.groupId ? `revision:${revision.groupId}` : null,
        `revision:${revision.id}`,
      ];
      // `activeIds` holds `null` for any revision that is not a move or in a group,
      // and `null` is also what `activeReviewItemId` holds when NOTHING is active —
      // so `includes` matched and every such marker rendered active the moment a
      // document opened. Only a real, selected id may activate a marker.
      const active = ed.activeReviewItemId != null && activeIds.filter(Boolean).includes(ed.activeReviewItemId)
        ? " review-revision-marker-active"
        : "";
      const kind = `${revision.kind === "paragraph_format"
      ? "review-revision-marker review-paragraph-format-bar"
      : paragraphLevel
        ? `review-revision-marker review-paragraph-mark-marker ${deletionLike ? "review-paragraph-mark-deleted" : "review-paragraph-mark-inserted"}`
        : deletionLike
          ? "review-revision-marker review-deletion-marker"
          : "review-revision-marker review-insertion-marker"}${active}`;
      const color = reviewAuthorColor(reviewAuthorKey(revision));
      const tooltip = reviewRevisionTooltip(revision);
      for (let i = 0; i < rects.length; i += 5) {
        const el = ed.place(rects.slice(i, i + 5), kind);
        if (el) {
          el.style.setProperty("--review-author-color", color);
          el.title = tooltip;
          // Inline accept/reject affordance (Q2): hovering a tracked-change marker
          // previews a compact card; pressing on it pins the card open. The
          // pointerdown is NOT swallowed, so it still bubbles to the page's
          // hit-testing and caret placement is unaffected (REVIEW-GAP-005). Opening
          // on pointerdown (before the caret repaint detaches this marker) keeps the
          // pinned card reliable; the card itself lives on document.body and
          // survives the repaint.
          el.dataset.reviewRevisionId = String(revision.id ?? "");
          el.addEventListener("mouseenter", () => ed.showReviewInlineCard(revision, el, false));
          el.addEventListener("mouseleave", () => ed.scheduleReviewInlineCardHide());
          el.addEventListener("pointerdown", () => ed.showReviewInlineCard(revision, el, true));
        }
      }
    }
  }

  return {
    anchorAt, clearOverlays, currentStoryKey, paintReviewMarkers, pointInsideObject, pointToTwip,
    syncActiveReviewCommentToCaret, toggleFormCheckboxAt,
  };
}
