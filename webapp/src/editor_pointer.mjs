// Pointer input: contents-entry links, the link chip, table chrome and gutter,
// hover, touch selection, pointer down / move / up, drag selection and copy.
// pageFromClientPoint stays in main.js. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { embedMarker, runsToHtml } from "./clipboard.mjs";
import { createTocNavigator } from "./toc_navigation.mjs";
import { edgeScrollStep } from "./edge_scroll.mjs";
import { t } from "./i18n.mjs";
import { createTouchSelection, pointerDragSelects } from "./touch_selection.mjs";
import { followExternalTarget } from "./link_targets.mjs";
import { createPointerHover } from "./pointer_hover.mjs";
import { createTableChrome } from "./table_chrome.mjs";
import { createTableGutter } from "./table_gutter.mjs";
import { installViewportFocusGuard } from "./viewport_focus.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createPointer(ed) {
  // -- Contents entries that were never hyperlinked -----------------------------
  // `toc_navigation.mjs` carries the finding, the measured evidence and the rule;
  // the navigator holds the two revision-keyed caches that keep a click O(1).
  const tocNavigator = createTocNavigator({ doc: () => ed.doc, revision: ed.documentVersion });

  /** The heading the contents entry at `node` names, or null. */
  const tocEntryTargetAt = (node, offset = null) => tocNavigator.targetAt(node, offset);

  /** Follows the contents entry at `node`, if it is one. Returns whether it did. */
  function followTocEntry(node, offset = null) {
    const target = tocEntryTargetAt(node, offset);
    if (!target) return false;
    ed.navigateToAnchor(target.node, 0, 0);
    ed.setStatus(t("toc.jumpedTo", { heading: target.label }));
    return true;
  }

  /** Copies a WASM-owned link hit into an ordinary JS value, then frees it. */
  function linkAt(page, event) {
    if (!ed.doc) return false;
    const { x, y } = ed.pointToTwip(page, event);
    const hit = ed.doc.linkAt(page.pageNumber, x, y);
    if (!hit) return null;
    const link = {
      kind: hit.kind,
      url: hit.url,
      anchor: hit.anchor,
      tooltip: hit.tooltip,
      startNode: hit.startNode,
      startOffset: hit.startOffset,
      endNode: hit.endNode,
      endOffset: hit.endOffset,
      targetNode: hit.targetNode,
      targetOffset: hit.targetOffset,
      targetPage: hit.targetPage,
    };
    hit.free();
    return link;
  }

  /** Clears the visible chip without changing the document selection. */
  function hideLinkChip() {
    linkChipShownBy = null;
    ed.activeLink = null;
    ed.linkChip.hidden = true;
  }

  /** The hover router (`docs/109` HF-179), so the pointer shape says what the
   *  thing under it will do. This is only the live state it reads. */
  /** The review-mode flags the pointer paths gate on, in one place: `editsBlocked`
   *  is Viewing (or read-only), `geometryBlocked` is anything but Editing, because
   *  a structural change has no tracked-change representation. */
  const pointerModeState = () => ({
    editsBlocked: ed.reviewMode === "viewing" || !!ed.readOnlyReason,
    geometryBlocked: ed.reviewMode !== "editing" || !!ed.readOnlyReason,
  });

  // The table chrome layer (`docs/141` D-1). It owns every pointer, keyboard and
  // touch gesture on a table BOUNDARY; `main.js` only routes events into it.
  const tableChrome = createTableChrome({
    doc: () => ed.doc,
    pointToTwip: ed.pointToTwip,
    scaleOf: ed.scaleOf,
    state: pointerModeState,
    runEdit: (fn, options) => ed.runEdit(fn, options),
    status: (text, kind) => ed.setStatus(text, kind),
    t,
    repaint: () => ed.drawSelection(),
  });

  // The table GUTTER (`docs/141` D-2). It owns the strips beside the table, the
  // hover insert discs and the drag that selects several rows or columns; it reads
  // the SAME memoised page chrome the boundary layer does, so a hover still makes
  // no engine call.
  const tableGutter = createTableGutter({
    doc: () => ed.doc,
    chromeOf: (page) => tableChrome.chromeOf(page),
    pointToTwip: ed.pointToTwip,
    scaleOf: ed.scaleOf,
    pageFromClientPoint: ed.pageFromClientPoint,
    pages: () => ed.pages,
    state: pointerModeState,
    range: ed.tableRange,
    runEdit: (fn, options) => ed.runEdit(fn, options),
    status: (text, kind) => ed.setStatus(text, kind),
    t,
    repaint: () => ed.drawSelection(),
    repaintOverlay: () => ed.paintOverlayLayer(),
  });

  const pointerHover = createPointerHover({
    doc: () => ed.doc,
    pages: () => ed.pages,
    materializedPages: ed.materializedPages,
    pointToTwip: ed.pointToTwip,
    linkAt,
    tocEntryAt: (node, offset) => !!tocEntryTargetAt(node, offset),
    pointInsideObject: ed.pointInsideObject,
    objectCapabilities: ed.objectCapabilities,
    tableBoundaryAt: (page, event) => tableChrome.targetKind(page, event),
    tableGutterAt: (page, event) => tableGutter.targetKind(page, event),
    state: () => ({
      formatPainting: !!ed.formatPainter,
      ...pointerModeState(),
      inRunningStory: !!ed.runningEditBand,
      insideObjectNode: ed.objectSelection?.mode === "editing" ? ed.objectSelection.node : null,
      resizeDrag: ed.objectResize.record(),
      cropDrag: ed.objectCropSession?.handleDrag ?? null,
      moveDrag: ed.objectMoveDrag ?? (ed.objectTextDrag.dragging() || null),
      tableDrag: tableChrome.dragKind(),
      tableStripDrag: tableGutter.dragKind(),
      textDrag: ed.dragging,
    }),
  });

  /** Shows a bounded link chip and selects the exact authored model range, making
   * the target discoverable without hijacking drag or Shift-selection behavior. */
  function showLinkChipAt(page, event) {
    const link = linkAt(page, event);
    if (!link) {
      hideLinkChip();
      return false;
    }
    // Selecting the link's own text is what a click on TEXT does. A click on a
    // linked OBJECT has already selected the object, and replacing that with a
    // text range would drop the handles the same gesture just put up.
    ed.selection = {
      anchor: { node: link.startNode, offset: link.startOffset },
      focus: { node: link.endNode, offset: link.endOffset },
    };
    ed.drawSelection();
    return showLinkChip(link, event);
  }

  /** The pointer event that opened the chip, so the light-dismiss listener below
   *  does not close it on the very event that asked for it.
   *
   *  A link on TEXT is offered on pointer-UP, after the dismiss listener has run
   *  on the matching pointer-down, so it never collided. A link on an OBJECT is
   *  resolved during pointer-DOWN (selecting it is a pointer-down gesture), so the
   *  chip opened and the same event closed it — which looked exactly like the
   *  engine not reporting the link. */
  let linkChipShownBy = null;

  /** Renders the chip for a link already resolved, leaving the selection alone. */
  function showLinkChip(link, event) {
    ed.activeLink = link;
    linkChipShownBy = event;
    ed.pendingFormat = null;

    const internal = link.kind === "internal";
    const resolved = !internal || (!!link.targetNode && !!link.targetPage);
    const target = internal ? `#${link.anchor}` : link.url;
    ed.linkChipKind.textContent =
      link.tooltip || (internal ? "Document bookmark" : "External link");
    ed.linkChipTarget.textContent = target;
    ed.linkChipTarget.title = target;
  ed.linkChipAction.textContent = internal ? (resolved ? "Jump" : "Missing") : "Open";
  ed.linkChipAction.disabled = !resolved;
  ed.linkChipEdit.hidden = internal;
  ed.linkChipRemove.hidden = internal;
    ed.linkChip.hidden = false;

    const width = ed.linkChip.offsetWidth;
    const height = ed.linkChip.offsetHeight;
    const left = Math.max(12, Math.min(event.clientX - 18, window.innerWidth - width - 12));
    let top = event.clientY + 14;
    if (top + height > window.innerHeight - 12) top = event.clientY - height - 14;
    ed.linkChip.style.left = `${Math.round(left)}px`;
    ed.linkChip.style.top = `${Math.max(12, Math.round(top))}px`;
    return true;
  }



  /** Activates a previously queried authored link. The runtime resolves
   * geometry/bookmarks; this host owns the external-scheme allowlist and browser
   * navigation. */
  function activateLink(link) {
    if (!link) return false;
    hideLinkChip();

    if (link.kind === "internal") {
      if (!link.targetNode || !link.targetPage) {
        ed.setStatus(`Bookmark “${link.anchor}” was not found`, "error");
        return true;
      }
      ed.navigateToAnchor(link.targetNode, link.targetOffset, link.targetPage);
      ed.setStatus(t("toc.jumpedTo", { heading: tocNavigator.headingAt(link.targetNode) ?? link.anchor }));
      return true;
    }

    followExternalTarget(link.url, ed.setStatus);
    return true;
  }

  /** Touch selection — the long press, the two handles, the loupe and the caret
   *  drag (`105` UX-018, `148` §9 item 2). Every tunable and the whole gesture
   *  live in the module; this is all the shell owes it. */
  const touchSelection = createTouchSelection({
    view: window,
    surface: ed.pagesEl,
    mount: document.body,
    makeEl: (tag) => document.createElement(tag),
    enabled: () => !!ed.doc && ed.chromeShows("caret"),
    pageAt: (x, y) => ed.pageFromClientPoint(x, y),
    scaleOf: ed.scaleOf,
    anchorAt: ed.anchorAt,
    caretRect: (at) => ed.doc.caretRect(at.node, at.offset),
    wordAt: (node, offset) => ed.doc.wordAt(node, offset),
    selection: () => ed.selection,
    setSelection: (next) => void (ed.selection = next),
    draw: ed.drawSelection,
    focus: ed.focusEditorSurface,
    cancelGesture: ed.resetPointerGesture,
  });

  function onPointerDown(page, event) {
    if (event.button !== 0) return;
    // No caret region: a press neither places an insertion point nor starts a
    // drag-selection. Scrolling and zooming are not gestures on the document.
    // This precedes the shape gate deliberately — a container offered no caret is
    // the page and nothing else, so it must not begin drawing one either.
    if (!ed.chromeShows("caret")) return;
    // An armed shape owns the gesture: this press is the first corner of the
    // rectangle being drawn, not a caret placement.
    if (ed.shapeDrawMode.tryBeginDraw(page, event)) return;
    // A pointerdown that reaches the page during a crop (a crop handle's own
    // pointerdown stops propagation) is a click-away → commit the crop, exactly as
    // Word/Docs do. This click is consumed by the commit; the next click interacts.
    if (ed.objectCropSession) {
      event.preventDefault();
      ed.commitCrop();
      return;
    }
    ed.focusEditorSurface();
    hideLinkChip();
    pointerHover.clear();
    ed.pointerGesture = null;
    // Object selection (docs/85 §3.1) takes precedence over a text caret: a click
    // on a drawing/image/text box selects it as a unit and shows its handles.
    const { x, y } = ed.pointToTwip(page, event);
    // An object you are INSIDE is a text surface, not a target. Selecting it as a
    // unit here meant that while editing a text box, a click in its own empty
    // space dropped you from editing back to "selected" — the box stopped being
    // something you were typing in and became something you had picked up.
    const editingHere =
      ed.objectSelection?.mode === "editing" &&
      ed.pointInsideObject(ed.objectSelection.node, page, x, y);
    const object = editingHere ? null : ed.doc.objectAt(page.pageNumber, x, y);
    // A SECOND click inside a group selects the shape under the pointer, the way
    // Word and PowerPoint do: the first click picks the group up as a unit, the
    // next one reaches into it.
    //
    // Without this a group was a wall. `objectAt` deliberately answers with the
    // group root — correct for the first click — and nothing ever asked a
    // different question, so clicking a shape inside a group selected the group,
    // every time, however many times you clicked. The only way in was Tab (and
    // only since #574) or a double-click, which skips selection entirely and
    // drops straight into typing. On the owner's Medical form, whose drawings are
    // one group of four, that is the whole of "they're all grouped and I can't
    // edit them".
    const descended = object ? ed.descendIntoSelectedGroup(object, page, x, y, event) : null;
    if (descended) {
      object.free?.();
      // On the owner's Medical form the link is on the CHILD — four of them, none
      // on the group — so without this the document that motivated the feature is
      // the one where it never appears.
      const childLink = linkAt(page, event);
      if (childLink) showLinkChip(childLink, event);
      event.preventDefault();
      return;
    }
    if (object) {
      const node = object.node;
      const kind = object.kind;
      const anchored = object.anchored;
      const descriptor = {
        ...ed.objectCapabilities(object),
        ...ed.objectReference(object, node),
      };
      object.free?.();
      // A caret at the nearest text slot is the object's surrounding-text anchor
      // (for the two-step Escape); fall back to the current caret.
      const anchor = ed.anchorAt(page, event) || ed.selection?.focus || null;
      // Ctrl/⌘+click ADDS to the selection instead of replacing it — Word,
      // PowerPoint and Docs all build a multi-object selection this way, and it is
      // the only way to reach Group with more than one object. The primary
      // selection stays the object clicked first; the modifier click keeps it and
      // records the new one alongside.
      if ((event.metaKey || event.ctrlKey) && ed.objectSelection?.mode === "selected"
          && ed.objectSelection.ref.root !== descriptor.root) {
        ed.objectArrange.toggleMember(descriptor);
        ed.setStatus(t("object.addedToSelection"));
        ed.drawSelection();
        event.preventDefault();
        return;
      }
      ed.objectArrange.clearMembers();
      ed.selectObject(node, kind, anchor, anchored, descriptor);
      // A linked picture offers its target as linked text does (`linkAt` answers
      // for both), but AFTER selection, so the handles stay and the chip is an
      // addition. The pointer stays `move`: `pointer_cursor.mjs` `object-movable`
      // records why, and ONLYOFFICE and Word agree.
      const objectLink = linkAt(page, event);
      if (objectLink) showLinkChip(objectLink, event);
      // The same gesture that selects an object drags it (a bare click commits
      // nothing): a floating one anywhere on the page, an in-line one to another
      // place in the text (UX-OB-02), and any other says why it cannot.
      if (descriptor.canMove) ed.startObjectMove(event, page, node);
      else if (!ed.objectTextDrag.arm(event, page, ed.objectSelection)) ed.refusedObjectDrag.arm(event, ed.objectTextIo.refusal({ ...descriptor, anchored }));
      event.preventDefault();
      return;
    }
    // A click that is not on an object deselects any selected object and proceeds
    // with ordinary text hit-testing. The chrome has to go with it: clearing the
    // variable alone left the handles, the context bar and `data-object-mode` on
    // screen, so a text box the user had clicked away from still looked — and to
    // anything reading the state, still was — the thing being edited.
    // ...unless the click is a caret placement INSIDE the object being edited, in
    // which case there is nothing to deselect: the object is the surface, not the
    // target. Clearing it here dropped the user out of the box on every click in
    // its own empty space.
    // A press on a table BOUNDARY is a resize, not a caret placement — and it works
    // on any table on the page, not only the one the caret is in, which is the whole
    // of `docs/141` D-1. Placed after the object path because an object drawn over a
    // table is still the thing under the pointer (the same order
    // `pointer_cursor.mjs` records), and before the caret path because the caret is
    // what this gesture replaces.
    // A press in the GUTTER beside the table selects a row or a column, or inserts
    // one at the `+` disc's boundary. BEFORE the boundary layer, because the two
    // zones overlap in a thin band and the gutter is the one the point is really
    // inside: a boundary zone reaches ±5px OUTSIDE the table's box, so without
    // this the disc on the table's bottom edge was stolen by a row resize unless
    // you aimed past the first 5px of the strip.
    if (tableGutter.tryBeginDrag(page, event)) return;
    if (tableChrome.tryBeginDrag(page, event)) return;
    // A TOUCH tap inside a table arms 24px boundary pills, because hover — the
    // affordance every branch above depends on — does not exist on touch. The first
    // `pointerType` read in the product (TBL-18).
    tableChrome.noteTouch(page, event);
    const wasSelected = ed.objectSelection !== null;
    if (!editingHere) {
      ed.objectSelection = null;
      ed.clearObjectStatus();
      if (wasSelected) {
        ed.updateObjectSelectionState();
        ed.updateObjectContextBar();
      }
    }
    // Which STORY the caret is in before the click resolves. `anchorAt` may leave a
    // header, footer or text box on the way to answering — that is the deliberate
    // way out of running content — and when it does, the existing selection anchor
    // belongs to a story the new focus does not.
    const storyBefore = ed.currentStoryKey();
    const anchor = ed.anchorAt(page, event);
    if (!anchor) {
      ed.updateObjectSelectionState();
      ed.updateObjectContextBar();
      return;
    }
    // A click on a FORM CHECKBOX ticks it rather than placing a caret beside a
    // glyph. Word and ONLYOFFICE both do this (`docs/118` §2), and it is the
    // difference between the owner's Medical Incident Report Form being a form
    // and being a picture of one: eight controls, none of them tickable.
    if (ed.toggleFormCheckboxAt(anchor.node, anchor.offset)) {
      event.preventDefault();
      return;
    }
    ed.pendingFormat = null; // a click moves the caret → disarm typing format
    ed.verticalGoal.clear(); // …and puts the caret in a column of its own choosing
    // …but a click INSIDE the accent fill of the row/column/table you just selected
    // keeps that selection (`docs/141` TBL-05). Every left-click cleared it, so a
    // user who selected a row and then clicked it to "confirm" lost it silently and
    // Merge cells went grey again. Google Docs keeps a cell block until you click
    // OUTSIDE it or start typing; the `contextmenu` handler already asks this exact
    // question through the same helper.
    //
    // O(1) when there is no table selection — the store returns on `!selection`
    // before it queries the engine — so the ordinary click path gains no document
    // walk (`docs/107` §4).
    if (!ed.tableRange.containsClientPoint(event.clientX, event.clientY)) ed.tableRange.clear();
    ed.dragging = true;
    ed.pointerGesture = {
      page,
      clientX: event.clientX,
      clientY: event.clientY,
      lastClientX: event.clientX,
      lastClientY: event.clientY,
      moved: false,
      shift: event.shiftKey,
      // A drag cannot construct a range across WordprocessingML stories. Retain
      // the surface that owned pointer-down so pointer-move can clip at its edge
      // instead of reusing click-away behavior and silently entering the body.
      runningBand: ed.runningEditBand,
      objectNode: editingHere ? ed.objectSelection.node : null,
      // The cell the press landed in, so a drag that leaves it becomes a CELL range
      // instead of a text range (`docs/141` D-3). One `inTable` per press, and the
      // drag re-asks only when the paragraph under the pointer changes.
      cellAnchor: ed.doc.inTable(anchor.node) ? anchor.node : "",
      lastCellProbe: "",
      cellRange: false,
    };
    // Shift+Click extends the current selection to the click (keeps the anchor) —
    // but only WITHIN one story. A range cannot span WordprocessingML stories, and
    // shift-clicking from an open header into the body used to build exactly that:
    // the anchor stayed in the header while the focus landed in the body, which
    // painted 422 highlight rectangles across every page of the document and then
    // made the editor swallow every keystroke, because no edit can apply to a range
    // whose ends live in different stories.
    //
    // `updateDragSelection` already clips a DRAG at the story edge; the click path
    // never got the same rule. Crossing stories with Shift is treated as a plain
    // click, which is what both Word and Docs do.
    const crossedStory = ed.currentStoryKey() !== storyBefore;
    ed.selection =
      event.shiftKey && ed.selection && !crossedStory
        ? { anchor: ed.selection.anchor, focus: anchor }
        : { anchor, focus: anchor };
    // Non-blocking secondary effect (REVIEW-GAP-005): surface the sidebar card
    // for a comment the click landed inside, without altering the caret/range
    // hit-testing just computed above.
    ed.syncActiveReviewCommentToCaret(anchor);
    ed.drawSelection();
    if (pointerDragSelects(event)) startSelectionAutoScroll(); // never under a finger
    event.preventDefault();
  }

  function onPointerMove(page, event) {
    if (ed.objectTextDrag.active()) return; // the window listener drives it, once per event
    if (ed.objectMoveDrag) {
      ed.updateObjectMove(event);
      return;
    }
    if (ed.objectRotate.active()) {
      ed.objectRotate.update(event);
      return;
    }
    if (ed.objectResize.active()) {
      ed.objectResize.update(event);
      return;
    }
    if (tableChrome.dragging()) {
      tableChrome.moveDrag(event);
      return;
    }
    if (tableGutter.dragging()) {
      tableGutter.moveDrag(event);
      return;
    }
    if (ed.dragging && event.buttons === 0) {
      ed.resetPointerGesture();
      return;
    }
    if (!ed.dragging) {
      // FIRST and unthrottled: a few dozen comparisons over the memoised page
      // chrome, no engine call, and a repaint only when the armed band changes.
      tableGutter.hover(page, event);
      pointerHover.schedule(page, event);
      return;
    }
    updateDragSelection(event);
  }

  function updateDragSelection(event) {
    if (!ed.dragging || !ed.pointerGesture || !ed.selection || !pointerDragSelects(event)) return;
    ed.pointerGesture.lastClientX = event.clientX;
    ed.pointerGesture.lastClientY = event.clientY;
    if (
      ed.pointerGesture &&
      Math.hypot(
        event.clientX - ed.pointerGesture.clientX,
        event.clientY - ed.pointerGesture.clientY,
      ) > 4
    ) {
      ed.pointerGesture.moved = true;
    }
    const page = ed.pageFromClientPoint(event.clientX, event.clientY);
    if (!page) return;
    const { x, y } = ed.pointToTwip(page, event);
    // Click-away is an entry/exit gesture; crossing a boundary while the button
    // is already down is not. Keep the last valid endpoint when the pointer leaves
    // the text-box story that owned pointer-down.
    if (
      ed.pointerGesture.objectNode &&
      !ed.pointInsideObject(ed.pointerGesture.objectNode, page, x, y)
    ) {
      return;
    }
    // `anchorAt` deliberately exits running-content editing for an ordinary body
    // click. Do not call that mutating path until the moving point is proven to be
    // in the same running story as pointer-down. Holding the last valid endpoint
    // clips the range at the story boundary and keeps the model selection valid.
    if (ed.pointerGesture.runningBand) {
      const hit = ed.doc.resolveClick(page.pageNumber, x, y);
      if (!hit) return;
      const sameStory = hit.band === ed.pointerGesture.runningBand && !!hit.node;
      hit.free?.();
      if (!sameStory) return;
    }
    const focus = ed.anchorAt(page, event);
    if (!focus) return;
    // A drag that crosses a CELL boundary selects the rectangle, the way Docs and
    // Word both do; one that stays inside a cell is ordinary text selection. Every
    // other drag supersedes a row/column/table selection, which is why the clear
    // lives here rather than in the "has moved" branch above: pointer-down inside
    // the fill keeps the selection (`docs/141` TBL-05), and the moment the gesture
    // turns into a drag it is no longer a confirming click.
    if (ed.tableRange.dragTo(ed.pointerGesture.cellAnchor, focus.node, ed.pointerGesture)) return syncSelectionToCellRange();
    ed.tableRange.clear();
    ed.selection = { anchor: ed.selection.anchor, focus };
    ed.drawSelection();
  }

  /** Follows the caret/selection to the cell rectangle the store now holds — the
   *  one line both the drag and Shift+Arrow need afterwards, so Copy and the
   *  toolbar see a range inside the selected cells rather than a stale caret. */
  function syncSelectionToCellRange() {
    const range = ed.tableRange.get();
    if (!range) return;
    ed.selection = {
      anchor: { node: range.anchorNode, offset: 0 },
      focus: { node: range.focusNode, offset: 0 },
    };
    ed.drawSelection();
  }

  function startSelectionAutoScroll() {
    if (ed.selectionAutoScrollFrame) return;
    const tick = () => {
      ed.selectionAutoScrollFrame = 0;
      if (!ed.dragging || !ed.pointerGesture) return;
      // Both axes (`edge_scroll.mjs`, shared with the in-text object drag).
      const { dx, dy } = edgeScrollStep(ed.viewportEl.getBoundingClientRect(), ed.pointerGesture.lastClientX, ed.pointerGesture.lastClientY);
      if (dy !== 0 || dx !== 0) {
        const beforeTop = ed.viewportEl.scrollTop;
        const beforeLeft = ed.viewportEl.scrollLeft;
        if (dy !== 0) ed.viewportEl.scrollTop = Math.max(0, beforeTop + dy);
        if (dx !== 0) ed.viewportEl.scrollLeft = Math.max(0, beforeLeft + dx);
        if (ed.viewportEl.scrollTop !== beforeTop || ed.viewportEl.scrollLeft !== beforeLeft) {
          updateDragSelection(ed.clientPointEvent(ed.pointerGesture.lastClientX, ed.pointerGesture.lastClientY));
        }
      }

      ed.selectionAutoScrollFrame = requestAnimationFrame(tick);
    };
    ed.selectionAutoScrollFrame = requestAnimationFrame(tick);
  }

  function onPointerUp(event) {
    document.body.style.cursor = ""; // the gesture no longer owns the cursor
    ed.refusedObjectDrag.end();
    if (ed.objectTextDrag.finish(event)) return;
    if (ed.shapeDrawMode.finish()) return;
    if (ed.finishObjectMove(event)) return;
    if (ed.objectRotate.finish(event)) return;
    if (ed.objectResize.finish(event)) return;
    if (tableChrome.finishDrag(event)) return;
    if (tableGutter.finishDrag(event)) return;
    const gesture = ed.pointerGesture;
    ed.resetPointerGesture();
    // Format painter: this pointer gesture landed on the document, so consume it as
    // the paint target (a drag's range, or the word under a bare click) instead of
    // the normal caret/link-chip behavior.
    if (ed.formatPainter && gesture) {
      void ed.paintFormatFromGesture(gesture, event);
      return;
    }
    if (
      gesture &&
      !gesture.shift &&
      !gesture.moved &&
      Math.hypot(event.clientX - gesture.clientX, event.clientY - gesture.clientY) <= 4
    ) {
      const link = linkAt(gesture.page, event);
      if (link?.kind === "internal" && link.targetNode && link.targetPage) {
        activateLink(link);
      } else if (!link && followTocEntry(ed.selection?.focus?.node, ed.selection?.focus?.offset)) {
        // A contents entry whose field carried no `\h`, so there is no authored
        // link to follow. The caret is already on the entry — this click placed it
        // — so the paragraph is known without a second hit-test.
      } else {
        showLinkChipAt(gesture.page, event);
      }
    }
  }

  function selectionText() {
    if (!ed.selection) return;
    const { anchor, focus } = ed.selection;
    return ed.doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
  }

  /** The selection as clipboard HTML: the exact `copyRichRuns` JSON embedded as a leading comment (a
   * lossless internal round-trip marker) plus a visible rendering built from the same runs and the
   * block outline (headings, lists, tables: what an external app sees, TA-12). `null` if nothing. */
  function selectionRichHtml() {
    if (!ed.selection) return null;
    const { anchor, focus } = ed.selection;
    const runsJson = ed.doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset);
    const runs = JSON.parse(runsJson);
    const outline = JSON.parse(ed.doc.copyBlockOutline(anchor.node, anchor.offset, focus.node, focus.offset));
    // When the selection spans block structure the flat runs flatten — a table or
    // a list — carry a structured payload for internal OpenDoc-to-OpenDoc paste
    // (`{ blocks, runs }`: the flat runs ride along so a Suggesting-mode paste, or
    // a structured paste the engine declines, still has the rich-run fallback).
    const structured = ed.doc.copyStructured(anchor.node, anchor.offset, focus.node, focus.offset);
    if (structured) {
      const blocks = JSON.parse(structured).blocks;
      return embedMarker(JSON.stringify({ blocks, runs })) + runsToHtml(runs, outline);
    }
    if (!runs.length) return null;
    return embedMarker(runsJson) + runsToHtml(runs, outline);
  }

  async function copySelection(event = null) {
    const text = selectionText();
    if (!text) return;
    const html = selectionRichHtml();
    if (event?.clipboardData) {
      event.preventDefault();
      event.clipboardData.setData("text/plain", text);
      if (html) event.clipboardData.setData("text/html", html);
      const n = text.length;
      ed.setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
      return true;
    }
    try {
      if (html && window.ClipboardItem) {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/plain": new Blob([text], { type: "text/plain" }),
            "text/html": new Blob([html], { type: "text/html" }),
          }),
        ]);
      } else {
        await navigator.clipboard.writeText(text);
      }
      const n = text.length;
      ed.setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
      return true;
    } catch (err) {
      console.warn("clipboard write failed:", err);
      ed.setStatus("Clipboard write was blocked by the browser", "err");
      return false;
    }
  }

  // Delegated pointer handling: resolve which page the event is over.
  function pageFromEvent(event) {
    const wrap = event.target.closest?.(".page-wrap");
    if (!wrap) return null;
    // The sheet's OWN page number, not its position among the sheets: the nth
    // sheet is the nth page only while the reader is at the top of the document,
    // so indexing by position hit-tests a scrolled-to click on the wrong page.
    const idx = ed.pageIndexOfWrap(wrap);
    return ed.pages[idx] ?? null;
  }

  ed.pagesEl.addEventListener("pointerdown", (e) => {
    // Only a hit inside a page places the caret: a gap click must not jump into the nearest fragment.
    const page = pageFromEvent(e);
    if (page) onPointerDown(page, e);
  });
  installViewportFocusGuard({ viewport: ed.viewportEl, pages: ed.pagesEl, lastSheet: () => ed.pages[ed.pages.length - 1]?.wrap, focus: ed.focusEditorSurface, caretToEnd: (extend) => ed.doc && ed.navToPosition(ed.doc.lastPosition(), extend) });
  // Focus is the moment the load-time insertion point stops being implicit: the
  // surface can now receive typing (`#pages` is tabindex="0"), so the blinking
  // caret is finally honest. This covers keyboard entry (Tab) as much as clicks,
  // and every `focusEditorSurface()` a command ends with — which is why an insert
  // run from the ribbon leaves a visible caret after the thing it inserted.
  /** Promotes the load-time insertion point to a real caret. Until the editing
   *  surface has focus the caret is implicit and deliberately unpainted — a
   *  blinking cursor you cannot type into is a lie (see `implicitCaretAt`).
   *
   *  Bound to BOTH focusable parts of the surface. It used to hang off `#pages`
   *  alone, which silently stopped firing when the editable proxy took over focus
   *  (docs/105 UX-001): the caret then stayed implicit forever and nothing was
   *  painted, which broke every test that reads `.overlay .caret`. */
  function promoteImplicitCaret() {
    if (!ed.implicitCaretAt) return;
    ed.implicitCaretAt = null;
    if (ed.doc) ed.drawSelection();
  }

  ed.pagesEl.addEventListener("focus", promoteImplicitCaret);
  if (ed.editorTextInputEl) ed.editorTextInputEl.addEventListener("focus", promoteImplicitCaret);
  ed.pagesEl.addEventListener("pointermove", (e) => {
    const page = pageFromEvent(e);
    if (page && !ed.dragging) onPointerMove(page, e);
    // The header/footer marker follows the pointer into a margin band.
    if (!ed.dragging) ed.updateRunningMarker(page, e);
  });
  ed.pagesEl.addEventListener("pointerleave", () => ed.hideRunningMarker());
  window.addEventListener("pointermove", (e) => {
    // A gesture in flight owns the cursor wherever the pointer goes, including
    // off the sheet it started on — so the router runs here too, not only over
    // `#pages`. It short-circuits on the drag kind and asks the engine nothing.
    if (pointerHover.dragKind()) pointerHover.schedule(null, e);
    if (ed.refusedObjectDrag.move(e)) return; // a drag of an object that cannot move says why
    if (ed.objectTextDrag.move(e)) return; // an in-line object follows the drop caret (UX-OB-02)
    if (ed.shapeDrawMode.dragging()) {
      ed.shapeDrawMode.update(e);
      return;
    }
    if (ed.objectMoveDrag) {
      ed.updateObjectMove(e);
      return;
    }
    if (ed.objectRotate.active()) {
      ed.objectRotate.update(e);
      return;
    }
    if (ed.objectResize.active()) {
      ed.objectResize.update(e);
      return;
    }
    if (tableChrome.dragging()) {
      tableChrome.moveDrag(e);
      return;
    }
    if (tableGutter.dragging()) {
      tableGutter.moveDrag(e);
      return;
    }
    if (ed.dragging) {
      if (e.buttons === 0) ed.resetPointerGesture();
      else updateDragSelection(e);
    }
  });
  ed.pagesEl.addEventListener("pointerleave", (e) => {
    pointerHover.clear();
    // The strips are hover chrome: leaving the sheet takes them down, or they sit
    // beside a table the pointer is nowhere near. A TOUCH pointer leaves the moment
    // the finger lifts, so honouring it there would undo the tap that armed them.
    if (e.pointerType !== "touch" && tableGutter.clear()) ed.paintOverlayLayer();
  });
  ed.pagesEl.addEventListener("dblclick", (e) => {
    const page = pageFromEvent(e);
    if (!page) return;
    // Double-click on an object enters its edit mode (container) or selects it
    // (leaf) — docs/85 §4.3; otherwise it selects the word under the caret.
    const { x, y } = ed.pointToTwip(page, e);
    // An object you are INSIDE is a text surface, not a target — the same rule
    // pointer-down already applies. Without it, double-clicking a word inside a
    // text box you are editing re-selected the BOX and never reached word
    // selection, so the word was never selected and the next keystroke inserted
    // instead of replacing.
    const editingHere =
      ed.objectSelection?.mode === "editing" &&
      ed.pointInsideObject(ed.objectSelection.node, page, x, y);
    const object = editingHere ? null : ed.doc?.objectAt(page.pageNumber, x, y);
    if (object) {
      let node = object.node;
      let kind = object.kind;
      let anchored = object.anchored;
      let descriptor = {
        ...ed.objectCapabilities(object),
        ...ed.objectReference(object, node),
      };
      // Initial hit testing selects a true multi-child group as one unit. A
      // double-click is the explicit descent gesture: ask the engine which leaf
      // owns this painted point and retain its root for structural commands.
      if (kind === "group") {
        const descendant = ed.doc.objectDescendantAt(descriptor.root, page.pageNumber, x, y);
        if (descendant) {
          node = descendant.node;
          kind = descendant.kind;
          anchored = descendant.anchored;
          descriptor = {
            ...ed.objectCapabilities(descendant),
            ...ed.objectReference(descendant, node),
          };
          descendant.free?.();
        }
      }
      object.free?.();
      ed.focusEditorSurface();
      if (!ed.objectSelection || ed.objectSelection.node !== node) {
        ed.selectObject(
          node,
          kind,
          ed.anchorAt(page, e) || ed.selection?.focus || null,
          anchored,
          descriptor,
        );
      }
      // A picture has no flowed body to put a caret in, so "enter the object"
      // means what it means in Google Docs: double-click an image and you are
      // cropping it. The direct-manipulation crop chrome was already right; only
      // its doorway was missing, so crop was reachable solely by finding the Crop
      // button. A second double-click applies it, as the button turns into Apply.
      if (ed.objectSelection?.kind === "chart" && ed.chartSurface.openData(ed.objectSelection.node)) return e.preventDefault();
      if (ed.objectSelection?.canCrop && !ed.objectSelection.canEditText) {
        ed.enterCropMode();
        e.preventDefault();
        return;
      }
      // A shape with no text body yet: double-click GIVES it one and puts the
      // caret inside, which is exactly what Word and Docs do. The engine keeps
      // the preset, its guides, its fill and its outline — a star stays a star.
      if (ed.objectSelection?.kind === "shape" && !ed.objectSelection.canEditText) {
        e.preventDefault();
        void ed.objectArrange.addText().then((added) => {
          // `addTextToShape` returns the new body's caret, and `refreshObject-
          // Capabilities` has by then re-read `canEditText`. Entering the object
          // is what makes the next keystroke go INTO the shape: while an object
          // is merely "selected" a printable key has nowhere to land.
          if (added) ed.enterObjectEditMode(ed.selection ? { ...ed.selection.focus } : null);
        });
        return;
      }
      let clicked = null;
      const inBox = ed.doc.textBoxHitTest(page.pageNumber, x, y);
      if (inBox) {
        clicked = { node: inBox.node, offset: inBox.offset };
        inBox.free?.();
      }
      ed.enterObjectEditMode(clicked);
      e.preventDefault();
      return;
    }
    // A double-click in the header/footer band enters that context — the gesture
    // every reference editor uses (docs/85 §10.2). Decided from the BAND GEOMETRY,
    // not from whether running content happens to exist: a document with no header
    // has nothing to hit-test, so keying off a hit meant double-clicking the top
    // margin fell through to word-selection and grabbed a word out of the BODY —
    // the opposite of what the gesture asks for.
    // Entering the band is what a double-click means from OUTSIDE it. Once you are
    // already editing that band, the same gesture means what it means everywhere
    // else — select the word — and re-entering the context you are already in
    // selected nothing at all. Triple-click at the identical pixel already worked,
    // which is what showed the hit-testing was fine and only this routing was not.
    const bandAtPoint = ed.runningBandAt(page, y);
    if (bandAtPoint && !(bandAtPoint === ed.runningEditBand && page.pageNumber === ed.runningEditPage)) {
      e.preventDefault();
      void ed.editRunningContent(bandAtPoint, page);
      return;
    }
    touchSelection.selectWord(page, e); // the SAME routine the long press uses
  });
  // Triple-click selects the paragraph (the click's `detail` is the click count).
  ed.pagesEl.addEventListener("click", (e) => {
    if (e.detail !== 3) return;
    const page = pageFromEvent(e);
    if (!page) return;
    const a = ed.anchorAt(page, e);
    if (!a) return;
    ed.focusEditorSurface();
    ed.selection = {
      anchor: { node: a.node, offset: 0 },
      focus: { node: a.node, offset: ed.doc.paragraphLength(a.node) },
    };
    ed.drawSelection();
  });
  window.addEventListener("pointerup", onPointerUp);
  /** Every pointer gesture the document owns, dropped. The four events below mean
   *  one thing — the pointer is gone and nothing will finish it — and were four
   *  copies of one list, so the touch machine would have been added to three of
   *  them and forgotten in the fourth. One list, named once. */
  function abortPointerGestures() {
    ed.cancelObjectMove();
    ed.objectTextDrag.cancel();
    ed.objectResize.cancel();
    ed.objectRotate.cancel();
    tableChrome.cancelDrag();
    tableGutter.cancelDrag();
    touchSelection.cancel();
    ed.resetPointerGesture();
  }
  window.addEventListener("pointercancel", abortPointerGestures);
  window.addEventListener("lostpointercapture", abortPointerGestures);
  window.addEventListener("blur", abortPointerGestures);
  document.addEventListener("visibilitychange", () => document.hidden && abortPointerGestures());

  ed.linkChip.addEventListener("mousedown", (event) => {
    // Keep the model selection visible while the host control receives the click.
    event.preventDefault();
  });
  ed.linkChipAction.addEventListener("click", () => activateLink(ed.activeLink));
  ed.linkChipEdit.addEventListener("click", () => {
    if (!ed.activeLink || !ed.selection || ed.activeLink.startNode !== ed.activeLink.endNode) return;
    const link = ed.activeLink;
    const text = ed.doc.copyText(link.startNode, link.startOffset, link.endNode, link.endOffset);
    hideLinkChip(); // the model range stays selected; the dialog owns it now
    ed.openLinkDialog({ node: link.startNode, start: link.startOffset, end: link.endOffset, link, text });
  });
  ed.linkChipRemove.addEventListener("click", () => {
    if (!ed.activeLink || !ed.selection || ed.activeLink.startNode !== ed.activeLink.endNode) return;
    ed.runToolbarEdit(() => ed.doc.removeHyperlink(ed.activeLink.startNode, ed.activeLink.startOffset, ed.activeLink.endOffset));
    hideLinkChip();
  });
  document.addEventListener("pointerdown", (event) => {
    if (event === linkChipShownBy) return; // this event is what opened it
    if (!ed.linkChip.hidden && !ed.linkChip.contains(event.target)) hideLinkChip();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      hideLinkChip();
      ed.closeReviewPopover();
      ed.closeReviewInlineCard();
    }
  });
  // Dismiss the inline accept/reject card on any pointerdown outside it and its
  // originating marker (the card's own buttons stopPropagation, so they are not
  // treated as "outside").
  document.addEventListener("pointerdown", (event) => {
    if (!ed.reviewInlineCard) return;
    if (ed.reviewInlineCard.contains(event.target)) return;
    if (event.target.closest?.("[data-review-revision-id]")) return;
    ed.closeReviewInlineCard();
  });
  /** Editing → Suggesting → Viewing → Editing, for keyboard access to all three
   *  (REVIEW-GAP-014). Google Docs cycles its three modes from one chord too; Word
   *  has no equivalent, so this is Docs' grammar rather than an invention. It is a
   *  named command rather than an inline keydown branch so that the chord is
   *  advertised on every surface (`109` UX-007). */
  function cycleReviewMode() {
    ed.setReviewMode(
      ed.reviewMode === "editing" ? "suggesting" : ed.reviewMode === "suggesting" ? "viewing" : "editing",
    );
  }
  ed.viewportEl.addEventListener("scroll", hideLinkChip, { passive: true });
  window.addEventListener("resize", hideLinkChip);

  return {
    copySelection, cycleReviewMode, followTocEntry, hideLinkChip, linkAt, pageFromEvent,
    pointerHover, syncSelectionToCellRange, tableChrome, tableGutter, tocEntryTargetAt,
    touchSelection,
  };
}
