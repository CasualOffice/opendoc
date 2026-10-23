// Objects: selection, crop, resize, rotate, arrange, the inspector, group
// traversal and the object context bar. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { capabilityRefusal, createRefusedDrag, readCapabilityReasons } from "./object_refusal.mjs";
import { createKeyboardObjectMove, createObjectTextDrag } from "./object_text_move.mjs";
import { canChangePicture } from "./picture_replace.mjs";
import { OBJECT_LABELS, escapeClimbsToGroup, groupClickAction, nextObjectIndex, traversalAnnouncement, traversalRoot } from "./object_traversal.mjs";
import { reflectEnablement } from "./control_reasons.mjs";
import { formatShortcut } from "./keyboard.mjs";
import { registerModal } from "./modal.mjs";
import { beginGripDrag, cropFromDrag, keptRect, paintCropChrome } from "./object_crop_chrome.mjs";
import { createObjectResizeDrag, sizeLabel as resizeSizeLabel } from "./object_resize_drag.mjs";
import { createObjectRotateDrag } from "./object_rotate_drag.mjs";
import { createObjectArrangeCommands } from "./object_arrange_commands.mjs";
import { createObjectBar } from "./object_bar.mjs";
import { createObjectInspector } from "./object_inspector.mjs";
import { readPosition, readTransform } from "./object_arrange.mjs";
import { SHAPE_PRESETS } from "./shape_catalogue.mjs";
import { createShapeDrawMode } from "./shape_draw_mode.mjs";
import { reflectObjectSelection, reflectShapeFormat } from "./object_selection_state.mjs";
import { t } from "./i18n.mjs";
import { insertChartAtCaret } from "./chart_insert.mjs";
import { createChartSurface } from "./chart_surface.mjs";
import { createObjectPresence } from "./object_presence.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createObjects(ed) {
  // ---- Image crop (direct-manipulation, Word/Docs standard) -------------------
  // The chrome, the clamping rule and the crop's own prose are in
  // `object_crop_chrome.mjs`. What stays here is entering, committing and
  // cancelling the MODE: crop is picture-only and, like resize, is not
  // representable as a tracked revision, so it is blocked in Suggesting/Viewing.
  // Double-clicking the picture is the doorway (Docs' gesture); Crop is surface 2.

  /** Enters crop mode on the selected image (or, if already cropping, commits). */
  function enterCropMode() {
    if (!ed.doc || !ed.objectSelection || !ed.objectSelection.canCrop) return;
    if (ed.objectCropSession) {
      commitCrop();
      return;
    }
    if (ed.reviewMode === "viewing") {
      ed.blockMutationInViewing();
      return;
    }
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Cropping an image is not tracked; switch to Editing to crop it", "error");
      return;
    }
    const rect = ed.doc.objectRect(ed.objectSelection.node); // [page, x, y, w, h] twips
    if (rect.length < 5) return;
    const [, x, y, w, h] = rect;
    const authoredCrop = ed.doc.objectCrop?.(ed.objectSelection.node);
    const cropValues = authoredCrop && authoredCrop.length === 4
      ? authoredCrop
      : [0, 0, 0, 0];
    ed.objectCropSession = {
      node: ed.objectSelection.node,
      box: [x, y, w, h],
      crop: { l: cropValues[0], t: cropValues[1], r: cropValues[2], b: cropValues[3] },
      handleDrag: null,
    };
    ed.focusEditorSurface();
    ed.drawSelection();
    updateObjectContextBar();
    ed.setStatus("Drag the handles to crop · Enter to apply · Esc to cancel");
  }

  /** Paints the crop chrome — the image outline, the dimmed region being cut, the
   *  kept rectangle with its live size, and eight draggable grips. The geometry
   *  and the clamping rule live in `object_crop_chrome.mjs`; what stays here is
   *  resolving which page and which scale, which only the editor can answer. */
  function paintObjectCrop() {
    const s = ed.objectCropSession;
    const rectFlat = ed.doc.objectRect(s.node);
    if (rectFlat.length < 5) return;
    s.box = rectFlat.slice(1, 5); // the picture as placed NOW, not at enterCropMode (HF-106)
    const [bx, by, bw, bh] = s.box;
    const pageNumber = rectFlat[0];
    const page = ed.pages[pageNumber - 1];
    if (!page?.overlay) return;
    ed.place([pageNumber, bx, by, bw, bh], "object-outline");
    const kept = keptRect(s.box, s.crop);
    paintCropChrome(page.overlay, s.box, s.crop, ed.scaleOf(page), {
      sizeLabel: sizeLabel(kept.w, kept.h),
      onGripDown: (event, kind) => startCropHandleDrag(event, page, kind),
    });
  }

  /** Begins dragging a crop handle. The kept rectangle updates live; the model is
   *  untouched until commit. */
  function startCropHandleDrag(event, page, handleKind) {
    if (!ed.objectCropSession) return;
    ed.objectCropSession.handleDrag = {
      handleKind,
      page,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startCrop: { ...ed.objectCropSession.crop },
    };
    beginGripDrag(event, {
      onMove: updateCropHandleDrag,
      onEnd: (cancelled) => {
        if (!ed.objectCropSession) return;
        // A gesture the browser cancelled leaves the crop where it started.
        if (cancelled) ed.objectCropSession.crop = ed.objectCropSession.handleDrag?.startCrop ?? ed.objectCropSession.crop;
        ed.objectCropSession.handleDrag = null;
        if (cancelled) ed.drawSelection();
      },
    });
  }

  /** Updates the kept rectangle from a crop-grip drag. The clamping rule is
   *  `cropFromDrag`; the repaint carries the new kept size with it. */
  function updateCropHandleDrag(event) {
    const s = ed.objectCropSession;
    if (!s || !s.handleDrag) return;
    const drag = s.handleDrag;
    const [, , bw, bh] = s.box;
    const { sx, sy } = ed.scaleOf(drag.page);
    const dxFrac = bw > 0 ? (event.clientX - drag.startClientX) / sx / bw : 0;
    const dyFrac = bh > 0 ? (event.clientY - drag.startClientY) / sy / bh : 0;
    s.crop = cropFromDrag(drag.startCrop, drag.handleKind, dxFrac, dyFrac);
    ed.drawSelection();
    event.preventDefault();
  }

  /** Commits the crop as one SetImageCrop op (or clears it when nothing is cropped),
   *  then exits crop mode. */
  function commitCrop() {
    const s = ed.objectCropSession;
    if (!s) return;
    const { l, t, r, b } = s.crop;
    const node = s.node;
    const cropped = l > 0.0005 || t > 0.0005 || r > 0.0005 || b > 0.0005;
    ed.objectCropSession = null;
    ed.runEdit(() => ed.doc.setImageCrop(node, cropped ? [l, t, r, b] : null), { gate: true }).then(() => {
      updateObjectContextBar();
      ed.setStatus(cropped ? "Image cropped" : "");
    });
  }

  /** Exits crop mode with no change. */
  function cancelCrop() {
    if (!ed.objectCropSession) return;
    ed.objectCropSession = null;
    ed.drawSelection();
    updateObjectContextBar();
    ed.setStatus("");
  }

  /** The two direct-manipulation geometry gestures. Both are modules: a grip
   *  drag that now reasons in the object's own space, and the rotation handle.
   *  What stays here is the application state they read and the gated edit path
   *  they write through. */
  const gestureIo = {
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    scaleOf: (page) => ed.scaleOf(page),
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    t,
    drawSelection: () => ed.drawSelection(),
    focusEditorSurface: () => ed.focusEditorSurface(),
    reviewMode: () => ed.reviewMode,
    blockMutationInViewing: () => ed.blockMutationInViewing(),
  };
  const objectResize = createObjectResizeDrag({
    ...gestureIo,
    // A GETTER, not the module: `measurement` is constructed further down this
    // file, so naming it in this object literal would read it in its temporal
    // dead zone. The drag only asks during a pointer move, by which time it
    // exists.
    measure: () => ed.measurement,
    snapContextFor: (page) => ed.snapContextFor(page),
    paintSnapGuides: (drag, guides) => ed.paintSnapGuides(drag, guides),
    clearSnapGuides: (drag) => ed.clearSnapGuides(drag),
    hideLinkChip: () => ed.hideLinkChip(),
    resetPointerGesture: () => ed.resetPointerGesture(),
  });
  const objectRotate = createObjectRotateDrag(gestureIo);
  /** A drag on an object that cannot move says why, once (HF-259). */
  const refusedObjectDrag = createRefusedDrag({ setStatus: (text, kind) => ed.setStatus(text, kind) });
  /** Dragging, or F2 ("Move to where?"), an in-line object to a new place in the text (`109` UX-OB-02). */
  const objectTextIo = {
    doc: () => ed.doc, pageAt: ed.pageFromClientPoint, pageByNumber: (n) => ed.pages[n - 1], pointToTwip: ed.pointToTwip, scaleOf: ed.scaleOf, platform: ed.EDITOR_KEYBOARD_PLATFORM, t, viewport: () => ed.viewportEl,
    blocked: () => ed.objectEditBlocked(), runEdit: (thunk, options) => ed.runEdit(thunk, options), setStatus: (text, kind) => ed.setStatus(text, kind),
    select: (node, held) => selectObject(node, held.kind, null, false, { ...held, ref: { ...held.ref, root: node, subject: node } }),
    caret: () => ed.selection?.focus ?? null,
    selection: () => (ed.objectSelection?.mode === "selected" ? ed.objectSelection : null),
    leaveObject: () => { ed.objectSelection = null; ed.clearObjectStatus(); ed.drawSelection(); },
    refusal: (held) => capabilityRefusal(held, held.anchored ? "canMove" : "canMoveInText", ed.SESSION.sentenceFor),
  };
  const objectTextDrag = createObjectTextDrag(objectTextIo);
  const keyboardObjectMove = createKeyboardObjectMove(objectTextIo);
  /** The crop session's keyboard (`object_keys.mjs`). */
  const cropKeyIo = {
    get session() { return ed.objectCropSession; },
    commit: () => commitCrop(),
    cancel: () => cancelCrop(),
    redraw: () => ed.drawSelection(),
    nothingToMove: () => ed.setStatus(t("object.crop.nothingToMove")),
  };
  const sizeLabel = (widthTwip, heightTwip) =>
    resizeSizeLabel(t, widthTwip, heightTwip, ed.measurement);

  /** Reflects the object-selection state onto `#pages` as data attributes so the
   *  host (and tests) can observe the grammar state machine without reading into
   *  the overlay. */
  function updateObjectSelectionState() {
    reflectObjectSelection(ed.pagesEl, ed.objectSelection, OBJECT_CAPABILITY_KEYS);
    reflectShapeFormatState();
  }

  /** Reflects a selected shape's own fill and outline onto `#pages`. */
  function reflectShapeFormatState() {
    reflectShapeFormat(ed.pagesEl, ed.objectSelection?.canFill || ed.objectSelection?.canStroke ? ed.selectedShapeFormat() : null);
  }

  /** The object properties panel. Its 170 lines are `object_inspector.mjs`; what
   *  stays here is the application state it reads and the gated edit path it
   *  writes through. */
  const objectInspector = createObjectInspector({
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    openShapeFill: () => ed.shapeFillBtn.click(),
    openShapeOutline: () => ed.shapeOutlineBtn.click(),
    t,
  });
  const toggleObjectInspector = (open) => objectInspector.toggle(open);

  /** The floating object chip. Its rendering is `object_bar.mjs`; what stays here
   *  is the application state it renders and the verbs it calls. */
  const objectBar = createObjectBar({
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    pages: () => ed.pages,
    scaleOf: (page) => ed.scaleOf(page),
    viewportRect: () => ed.viewportEl.getBoundingClientRect(),
    t,
    croppingNode: () => ed.objectCropSession?.node ?? null,
    arrangeState: () => arrangeState(),
    ensureInspector: () => objectInspector.ensure(),
    toggleInspector: (open) => objectInspector.toggle(open),
    inspectorOpen: () => objectInspector.isOpen(),
    reflectInspector: () => objectInspector.reflect(),
    setWrap: (mode) => ed.setObjectWrap(mode),
    openAltText: () => ed.openAltTextDialog(),
    openChartData: () => chartSurface.openData(),
    enterCrop: () => enterCropMode(),
    canChangePicture,
    changePicture: () => ed.pictureReplace.choose(),
    deleteObject: () => ed.deleteSelectedObject(),
    reflectShapeSwatches: () => ed.reflectShapeSwatches(),
    fillButton: () => ed.shapeFillBtn,
    outlineButton: () => ed.shapeOutlineBtn,
    positionButton: () => ed.labelledObjectMenuButton(ed.objectPositionBtn),
    arrangeButton: () => ed.labelledObjectMenuButton(ed.objectArrangeBtn),
    rotateButton: () => ed.labelledObjectMenuButton(ed.objectRotateBtn),
  });
  const updateObjectContextBar = () => (objectBar.update(), chartSurface.sync());
  /** The chart's Chart tab, settings panel, data dialog and command tree (`chart_surface.mjs`). */
  const chartSurface = createChartSurface({
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    blocked: () => ed.objectEditBlocked(),
    blockedReason: () => ed.readOnlyReason || (ed.reviewMode === "viewing" ? t("chart.reason.viewing") : ed.reviewMode === "suggesting" ? t("chart.reason.suggesting") : ""),
    apply: (result) => ed.applyEditResult(result, { keepView: true }),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    returnFocus: () => ed.focusEditorSurface(),
    registerModal,
    showMenu: (button, commands) => { const box = button.getBoundingClientRect(); ed.showContextMenu(box.left, box.bottom, { surface: "ribbon", commands }); },
    reflectTab: (tab, state) => reflectEnablement(tab, state, ed.EDITOR_KEYBOARD_PLATFORM),
    selectTab: (name) => ed.selectRibbonTab(name),
    t,
  });
  /** Insert ▸ Chart, as ONLYOFFICE and Word do it: the chart lands SELECTED with its data open. */
  const insertChartWithData = () => chartSurface.insertAndOpen(() => insertChartAtCaret({ doc: ed.doc, caret: ed.selection?.focus, blocked: ed.blockMutationInViewing, suggesting: () => ed.reviewMode === "suggesting", status: ed.setStatus, apply: ed.applyEditResult }), (chart) => selectObject(chart.node, chart.kind, null, chart.anchored, chart));
  const positionObjectContextBar = () => objectBar.reposition();

  /** Every arrange fact about the selected object, gathered in ONE pass per
   *  repaint: its anchor kind, its wrap, its stacking and its transform.
   *
   *  `objectPosition` and `objectTransform` are each O(document) with one walk.
   *  Asking them here — once, when the selection or the layout changed — is what
   *  keeps the wrap row, the Position gallery and the Rotate menu from each
   *  asking again, which is the per-object-query-in-a-loop shape that has already
   *  cost this repository a quadratic. Nothing on a pointermove path calls this. */
  function arrangeState() {
    const selection = ed.objectSelection;
    if (!ed.doc || !selection || selection.mode !== "selected") return null;
    const root = selection.ref.root;
    const read = readPosition(ed.doc.objectPosition?.(root) ?? "");
    if (!read.object) return null;
    return {
      root,
      node: selection.node,
      read,
      wrap: read.floating ? ed.doc.objectWrap(root) : "",
      transform: readTransform(ed.doc.objectTransform?.(selection.node) ?? ""),
    };
  }

  /** The Arrange commands — position, stacking, grouping, rotation, text in a
   *  shape. `object_arrange_commands.mjs`; what stays here is the state they read
   *  and the gated edit path they write through. */
  const objectArrange = createObjectArrangeCommands({
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    state: () => arrangeState(),
    runEdit: (thunk, options) => ed.runEdit(thunk, options),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    select: (entry) => selectObject(entry.node, entry.kind, null, entry.anchored, entry),
    t,
    // Ctrl on Windows/Linux, ⌘ on a Mac — derived, never spelled, because a spec
    // that asserts a Mac glyph fails on the Linux runner (`105` UX-009).
    modifier: formatShortcut("\u2318"),
  });

  /** Insert ▸ Shapes arms the pointer; the next gesture on the page draws the
   *  shape. Word's sequence, and every drawing tool's. `shape_draw_mode.mjs`
   *  holds the state machine and the preview; the arithmetic is `shape_draw.mjs`.
   *
   *  O(1) per pointermove: the gesture never touches the document while it runs. */
  const shapeDrawMode = createShapeDrawMode({
    pointToTwip: (page, event) => ed.pointToTwip(page, event),
    scaleOf: (page) => ed.scaleOf(page),
    outlineFor: (token) => SHAPE_PRESETS.find((shape) => shape.token === token)?.outline ?? null,
    insert: (token, rect) => void ed.insertShapeObject(token, rect),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    t,
  });

  // The five viewport scroll listeners never touched the object bar, and the page
  // IntersectionObserver never calls drawSelection, so scrolling left it behind.
  // Window scroll and resize matter too: the whole shell can move under a bar that
  // is positioned in viewport coordinates.
  ed.viewportEl.addEventListener("scroll", positionObjectContextBar, { passive: true });
  window.addEventListener("scroll", positionObjectContextBar, { passive: true });
  window.addEventListener("resize", positionObjectContextBar);

  /** Selects an object as a unit (docs/85 §4.1). Keeps `selection` as a caret at
   *  the object's surrounding-text anchor so the two-step Escape can return to it.
   *  `anchored` records placement only; the engine-owned capability bits below
   *  are the authority for every exposed operation. */
  const OBJECT_CAPABILITY_KEYS = [
    "canResize",
    "canRotate",
    "canMove",
    "canWrap",
    "canDelete",
    "canAltText",
    "canCrop",
    "canFill",
    "canStroke",
    "canEditText",
    "canMoveInText",
  ];

  /** Copies the engine-declared structural capabilities before a wasm hit payload
   *  is freed. JSON object-order entries use the same camelCase field names. */
  function objectCapabilities(source) {
    if (source && OBJECT_CAPABILITY_KEYS.some((key) => key in source)) {
      // The engine's reason for every `false` travels with the bits (HF-259), and so does the file's aspect lock (FID-AT-09).
      const reasons = { capabilityReasons: readCapabilityReasons(source), locksAspectRatio: source.locksAspectRatio === true };
      return Object.assign(Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, source[key] === true])), reasons);
    }
    // A missing or stale engine payload must never make an unsupported mutation
    // appear safe. A correctly built production bridge always supplies the bits.
    return Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, false]));
  }

  /** Copies the engine-owned object identity before a wasm payload is freed.
   *  `node` remains the subject compatibility alias; root owns structural and
   *  anchor commands while subject owns kind-specific properties/text. */
  function objectReference(source, node) {
    const value = source?.ref ?? source;
    const subject = value?.subject || value?.node || node;
    return {
      surface: value?.surface || "body",
      root: value?.root || subject,
      subject,
      path: Array.from(value?.path ?? []),
    };
  }

  function selectObject(node, kind, anchor, anchored = false, descriptor = null) {
    // Handles nobody can see are still handles Delete is aimed at.
    if (!ed.chromeShows("objects")) return;
    if (anchor) ed.selection = { anchor, focus: anchor };
    const ref = objectReference(descriptor, node);
    ed.objectSelection = {
      node: ref.subject,
      ref,
      kind,
      mode: "selected",
      anchored,
      ...objectCapabilities(descriptor),
    };
    ed.pendingFormat = null;
    ed.tableRange.clear();
    ed.drawSelection();
  }

  /** The object context for whatever is selected right now.
   *
   *  `objectContextAtEvent` builds the same shape from a pointer hit, which is why
   *  every object command used to need a mouse to exist. `objectSelection` already
   *  carries the identity and the engine-declared capabilities, so this is a
   *  reshape rather than a second source of truth — the capabilities are never
   *  re-derived here, or they could disagree with what the bar offers. */
  function selectedObjectContext() {
    if (!ed.objectSelection) return null;
    const { node, ref, kind, anchored, ...rest } = ed.objectSelection;
    return {
      surface: "object",
      node,
      ref,
      kind,
      anchored,
      ...Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, rest[key] === true])),
    };
  }

  /** Whether the document holds any floating object at all, which is what decides
   *  if the "select next object" commands are offered rather than greyed. Asked of
   *  the engine's own paint order so the answer cannot drift from what traversal
   *  would actually land on — and memoized, because the question is asked on a
   *  per-keystroke path while `objectOrder()` is O(objects) (`109` HF-183; the
   *  whole argument, and the invalidation contract, is in `object_presence.mjs`).
   *
   *  Cost: O(1) per call, O(objects) once per edit. */
  const objectPresence = createObjectPresence(() => ed.doc.objectOrder());
  function documentHasObjects() {
    if (!ed.doc) return false;
    return objectPresence.has();
  }

  /** Moves the object selection `step` places through the document's objects, in
   *  the engine's own paint order, wrapping at both ends so the gesture never
   *  dead-ends. Reads the order fresh each time because an edit may have added or
   *  removed an object since the last keystroke. */
  function traverseObjects(step) {
    // Deliberately NOT gated on an existing selection. It used to be, which made
    // the whole object surface mouse-only: Tab cycles objects once one is
    // selected, but nothing else selected one, so a keyboard user could not reach
    // an image or a text box at all — while the comment beside the Tab handler
    // claimed this was "the only way to reach an object without a pointer". The
    // wrap-around below already handles "no current object" by starting at either
    // end, so seeding a first selection needs no extra machinery.
    if (!ed.doc) return;
    // Inside a group Tab walks that group's children; at the top level, the
    // top-level objects. `object_traversal.mjs` carries why the one flat list
    // that used to serve both made every shape but a group's first unreachable.
    const inside = traversalRoot(ed.objectSelection?.ref);
    let objects;
    try {
      objects = JSON.parse(inside ? ed.doc.objectDescendants(inside) : ed.doc.objectOrder());
    } catch {
      return;
    }
    if (!Array.isArray(objects) || objects.length === 0) return;
    const current = ed.objectSelection
      ? objects.findIndex((entry) => entry.node === ed.objectSelection.node)
      : -1;
    const next = nextObjectIndex(objects.length, current, step);
    const target = objects[next];
    selectObject(target.node, target.kind, null, target.anchored, target);
    // `selectObject` repaints synchronously, so the outline it just drew is the
    // marker to reveal — a traversal that lands off-screen would otherwise look
    // like nothing happened.
    ed.scrollOverlayIntoView(ed.pagesEl.querySelector(".overlay .object-outline"));
    // Screen readers get no handles to look at, so the position is announced.
    ed.setStatus(traversalAnnouncement(target.kind, next, objects.length, Boolean(inside)));
  }

  /** Escape from a shape inside a group selects the GROUP. `escapeClimbsToGroup`
   *  is the rule; this is the DOM half. */
  function climbOutOfGroup() {
    const ref = ed.objectSelection?.ref;
    if (!escapeClimbsToGroup(ref)) return false;
    // The parent group is a TOP-LEVEL object, so it is in the ordinary order;
    // there is no second lookup to add to the engine for this.
    let group = null;
    try {
      group = JSON.parse(ed.doc.objectOrder()).find((entry) => entry.node === ref.root) ?? null;
    } catch {
      return false;
    }
    // The engine can no longer describe the parent — a group deleted out from
    // under the selection. Collapsing to the caret is the honest fallback: a
    // selection naming a node that is gone is worse than none.
    if (!group) return false;
    selectObject(
      group.node,
      group.kind,
      ed.selection?.focus || null,
      group.anchored,
      group,
    );
    return true;
  }

  /** Selects the shape under the pointer when the click lands in a group that is
   *  already held. `clickDescendsIntoGroup` is the rule; this is the DOM half. */
  function descendIntoSelectedGroup(object, page, x, y, event) {
    const held = ed.objectSelection?.ref;
    const onHeldChild =
      !!held && held.subject !== held.root && ed.pointInsideObject(ed.objectSelection.node, page, x, y);
    const action = groupClickAction(held, object, onHeldChild);
    // Pressing down on the shape already held begins its DRAG. Falling through
    // to the ordinary path re-selected the group and dragged that instead, so a
    // grouped shape could be picked up and never moved (`docs/109` HF-173).
    if (action === "keep") {
      if (ed.objectSelection.canMove) ed.startObjectMove(event, page, ed.objectSelection.node);
      return true;
    }
    if (action !== "descend") return false;
    const child = ed.doc.objectDescendantAt(object.root, page.pageNumber, x, y);
    if (!child) return false;
    const node = child.node;
    const kind = child.kind;
    const anchored = child.anchored;
    const descriptor = { ...objectCapabilities(child), ...objectReference(child, node) };
    child.free?.();
    selectObject(node, kind, ed.selection?.focus || null, anchored, descriptor);
    // The SAME pointerdown that reached the shape also begins its drag, exactly
    // as a click on a top-level object does.
    if (descriptor.canMove) ed.startObjectMove(event, page, node);
    ed.setStatus(`${OBJECT_LABELS[kind] ?? "Object"} inside group selected`);
    return true;
  }

  /** Descends a selected multi-child group to its first paint-ordered leaf. The
   *  engine returns the complete root/subject/path reference and capabilities;
   *  the host never reconstructs group structure from canvas geometry. */
  function descendSelectedGroup() {
    if (!ed.doc || ed.objectSelection?.kind !== "group") return false;
    let descendants;
    try {
      descendants = JSON.parse(ed.doc.objectDescendants(ed.objectSelection.ref.root));
    } catch {
      return false;
    }
    const target = Array.isArray(descendants) ? descendants[0] : null;
    if (!target) return false;
    selectObject(target.node, target.kind, null, target.anchored, target);
    ed.setStatus(`${OBJECT_LABELS[target.kind] ?? "Object"} inside group selected`);
    return true;
  }

  return {
    OBJECT_CAPABILITY_KEYS, arrangeState, cancelCrop, chartSurface, climbOutOfGroup, commitCrop,
    cropKeyIo, descendIntoSelectedGroup, descendSelectedGroup, documentHasObjects, enterCropMode,
    insertChartWithData, keyboardObjectMove, objectArrange, objectCapabilities, objectInspector,
    objectPresence, objectReference, objectResize, objectRotate, objectTextDrag, objectTextIo,
    paintObjectCrop, reflectShapeFormatState, refusedObjectDrag, selectObject,
    selectedObjectContext, shapeDrawMode, toggleObjectInspector, traverseObjects,
    updateObjectContextBar, updateObjectSelectionState,
  };
}
