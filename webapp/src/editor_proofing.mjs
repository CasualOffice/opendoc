// Proofing (docs/114, docs/146, HF-035): the spell and grammar checker's
// chrome, the overlay layer it paints into, and drawSelection, which repaints
// that layer. Moved out of `main.js` unchanged (docs/177); it reaches the rest
// of the editor only through the context `ed`.
import { createProofingChrome } from "./proofing_chrome.mjs";
import { registerModal } from "./modal.mjs";
import { paintMovePad, paintResizeHandles } from "./object_guides.mjs";
import { activeLocale, t } from "./i18n.mjs";
import { openWordStore } from "./drafts.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createProofing(ed) {
  // ---- Proofing (docs/114, docs/146, `109` HF-035) -----------------------------
  // The coordinator, the two switches, the one replacement path and the language
  // packs are all `proofing_chrome.mjs`; what is here is this application's answers
  // to the questions it asks. The three hooks are the whole of proofing's contact
  // with the editor: `paint()` from the overlay repaint, `noteEdited()` from the
  // edit choke point (O(1) — it re-arms a timer and nothing else), and
  // `noteWindowChanged()` from the scroll that moves the page window.
  const proofing = createProofingChrome({
    getDoc: () => ed.doc,
    pages: () => ed.pages,
    pageWindow: () => ed.pageWindow,
    place: ed.place,
    caret: () => (ed.selection ? ed.selection.focus : null),
    preference: (key) => ed.settings[key],
    setPreference: (key, value) => {
      ed.settings[key] = value;
      ed.saveSettings();
    },
    toggles: () => ({ spell: ed.spellCheckToggle, grammar: ed.grammarCheckToggle }),
    status: (text, kind) => ed.setStatus(text, kind),
    repaint: () => paintOverlayLayer(),
    redraw: () => drawSelection(),
    reflect: () => ed.updateToolbar(),
    openWords: () => openWordStore({}),
    runEdit: ed.runEdit,
    reviewMode: () => ed.reviewMode,
    registerModal,
    fallbackFocus: () => ed.pagesEl,
    activeLocale: () => activeLocale(),
    indexedDB: globalThis.indexedDB,
    stamp: new URL(import.meta.url).search,
    origin: () => location.origin,
  });
  const spellChecker = proofing.checker;
  const { replaceMisspelling, setSpellCheckEnabled, setGrammarCheckEnabled } = proofing;

  /** Draws the current selection from engine geometry: a highlight for a real
   *  range, else a caret at the focus (so a click — or a range with no visible
   *  rects — always shows a cursor). */
  /** Repaints everything the page overlays carry, and nothing else.
   *
   *  Split out of `drawSelection` because scrolling now creates and destroys
   *  overlays: a sheet materialized by `updatePageWindow` arrives empty, and the
   *  caret, selection, comment markers and checklist boxes that belong on it have
   *  to be drawn again. The chrome around them (toolbar state, page number, the
   *  comment column) does not change when the window moves, so it stays in
   *  `drawSelection` and is not paid for on every scroll. */
  function paintOverlayLayer() {
    if (!ed.doc) return;
    ed.clearOverlays();
    // A repaint is the only thing that can move a table boundary, so it is the only
    // place the chrome layer's geometry cache must be dropped — which is what makes a
    // pointer-move a lookup rather than an engine query. AFTER `clearOverlays`,
    // because the same call puts an in-flight drag's guide back (`docs/141` D-1).
    ed.tableChrome.invalidate();
    // The cell range's memo has the same lifetime and the same reason: a repaint
    // is the only thing that can change which cells a pair of endpoints covers.
    ed.tableRange.invalidate();
    ed.paintReviewMarkers();
    ed.paintChecklistMarkers();
    spellChecker.paint();
    ed.findSession.paint(); // every Find match, the current one distinct (TA-04)
    // A selected object owns the visible chrome (outline + handles) in place of the
    // text caret; in "editing" mode the ordinary text caret is shown instead.
    if (ed.objectSelection && ed.objectSelection.mode === "selected") {
      paintObjectSelection();
    } else if (ed.selection) {
      paintTableSelection();
      // ONE `cellRect` for the whole table chrome. It used to be asked twice — once
      // for the active-cell outline and once inside the handle query's own
      // `inTable` — and it is a page scan each time.
      const cellRect = ed.doc.cellRect(ed.selection.focus.node); // [page, x, y, w, h] or []
      paintActiveCell(cellRect); // under the caret/highlight
      ed.paintSelection(ed.selection);
      ed.tableChrome.paintCaretColumnHandles(ed.pages, cellRect);
      ed.tableChrome.paintTouchPills(ed.pages);
      ed.touchSelection.paint(ed.pages); // the finger's handles, same layer, same reason
    }
    // Outside the `selection` branch: a strip is armed by the POINTER, and being
    // over a table is not a reason to require a caret.
    ed.tableGutter.paint(ed.pages);
    ed.remotePresence.paint(ed.place);
  }

  function drawSelection() {
    if (!ed.doc) return;
    paintOverlayLayer();
    ed.foldView.syncBodyChevron(); // after the overlay repaint, which clears it
    ed.updateObjectSelectionState();
    ed.updateObjectContextBar();
    ed.updateToolbar();
    ed.updateReviewControls();
    ed.scheduleReviewMarginRender();
    ed.updatePageNumber();
    if (ed.tableRange.mirrorChanged()) ed.scheduleChromeRefresh({ a11y: true });
    ed.rulerView.syncToCaret();
    ed.positionSelToolbar();
    ed.hostSession?.noteSelection();
    ed.remotePresence.changed();
  }

  function paintTableSelection() {
    const rects = ed.tableRange.rects();
    for (let i = 0; i + 4 < rects.length; i += 5) ed.place(rects.slice(i, i + 5), "table-cell-selection");
  }

  /** Outlines the table cell the caret is in (nothing when not in a table), so the
   *  user always sees which cell they are editing. */
  function paintActiveCell(flat) {
    if (flat.length >= 5) ed.place(flat, "cell-outline");
  }

  /** Paints the selected object's outline + exact supported resize handles from engine
   *  geometry (docs/85 §3.3), the same overlay mechanism as the caret/highlight so
   *  the chrome matches the raster exactly. Unsupported carrier/edge combinations
   *  are omitted by the engine. */
  function paintObjectSelection() {
    const { node } = ed.objectSelection;
    // In crop mode the image shows crop chrome (dimmed cut region + crop handles)
    // in place of the resize handles — the Word/Docs crop experience.
    if (ed.objectCropSession && ed.objectCropSession.node === node) {
      ed.paintObjectCrop();
      return;
    }
    const rect = ed.doc.objectRect(node); // [page, x, y, w, h] twips
    const outlineEl = ed.place(rect, "object-outline");
    // The objects held alongside it (Ctrl/⌘+click), so a multi-object selection
    // is visible before Group is pressed rather than being a state only the menu
    // knows about. One rect per HELD member — a user-picked handful, on a repaint,
    // never on a pointermove — which is why this is a loop and the object chip's
    // arrange gather is not.
    for (const member of ed.objectArrange.multiSelect()) {
      // By SUBJECT: `objectRect` is answered for the subject, and a root id
      // answers nothing at all — which is why holding only the root drew no
      // outline for the second object.
      ed.place(ed.doc.objectRect(member.node), "object-outline is-co-selected");
    }
    // A movable object's BODY needs `touch-action: none` the way its grips
    // already have it, or the browser starts scrolling at touch-start and the
    // move gesture is gone before any handler runs. Selected + movable only.
    const padPage = rect.length >= 5 ? ed.pages[rect[0] - 1] : null;
    if ((ed.objectSelection.canMove || ed.objectSelection.canMoveInText) && padPage?.overlay) {
      paintMovePad(padPage.overlay, rect.slice(1, 5), ed.scaleOf(padPage), (event) =>
        ed.objectSelection.canMove ? ed.startObjectMove(event, padPage, node) : ed.objectTextDrag.arm(event, padPage, ed.objectSelection));
    }
    // The angle comes with the frame, in ONE engine read: the grips are drawn at
    // the object's rotated corners, the cursors are turned with them, and the
    // rotation grip is pushed out along the object's own axis.
    const turned = (ed.doc.objectFrame?.(node) ?? [])[5] ?? 0;
    const rotationDegrees = turned / 1000;
    // The outline is TURNED with the object rather than redrawn as its
    // axis-aligned bounding box: a box that is bigger than the object on every
    // side is not an outline of it, and Word, Docs and ONLYOFFICE all draw the
    // turned rectangle.
    if (rotationDegrees && outlineEl) outlineEl.style.transform = `rotate(${rotationDegrees}deg)`;
    paintResizeHandles(
      (pageNumber) => {
        const page = ed.pages[pageNumber - 1];
        return page?.overlay ? { overlay: page.overlay, scale: ed.scaleOf(page) } : null;
      },
      ed.doc.objectHandles(node), // [page, cx, cy, kind] * supported handle count
      (event, pageNumber, kind) => ed.objectResize.start(event, ed.pages[pageNumber - 1], node, kind),
      {
        rotationDegrees,
        rotateLabel: t("object.rotate.handle"),
        rotateValueText: t("object.rotate.degrees", { degrees: Math.round(rotationDegrees) }),
        onRotateDown: ed.objectSelection.canRotate
          ? (event, pageNumber) => ed.objectRotate.start(event, ed.pages[pageNumber - 1], node)
          : null,
        onRotateKey: (event) => ed.objectRotate.onKey(event),
      },
    );
  }

  return {
    drawSelection, paintOverlayLayer, proofing, replaceMisspelling, setGrammarCheckEnabled,
    setSpellCheckEnabled, spellChecker,
  };
}
