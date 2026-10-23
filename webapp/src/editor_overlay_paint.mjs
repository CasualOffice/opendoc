// Overlay painting: checklist markers, the selection highlight, the IME
// pre-edit, and navigating to an anchor. place stays in main.js. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createOverlayPaint(ed) {
  function paintChecklistMarkers() {
    if (!ed.doc) return;
    let markers = [];
    try { markers = JSON.parse(ed.doc.checklistMarkers()) ?? []; } catch { markers = []; }
    for (const marker of markers) {
      const el = ed.place([marker.page, marker.x, marker.y, marker.w, marker.h], "checklist-marker");
      if (!el) continue;
      el.classList.toggle("is-checked", !!marker.checked);
      el.dataset.checklistNode = marker.node;
      el.setAttribute("role", "checkbox");
      el.setAttribute("aria-checked", String(!!marker.checked));
      el.setAttribute("aria-label", marker.checked ? "Checked item" : "Unchecked item");
      el.title = marker.checked ? "Checked — click to uncheck" : "Unchecked — click to check";
      el.addEventListener("pointerdown", (event) => {
        // Own the gesture: toggle the item instead of placing a caret in the gutter.
        event.preventDefault();
        event.stopPropagation();
        // Put the caret at the item so the mutation path has a selection and typing
        // continues in that item afterward; then toggle (gated for Viewing/Suggesting).
        ed.selection = {
          anchor: { node: marker.node, offset: 0 },
          focus: { node: marker.node, offset: 0 },
        };
        // Checklist creation already provisions the symbol fallback that covers
        // both the checked and unchecked glyphs. Starting another asynchronous
        // coverage/render pass here races the edit repaint (and, when the edit is
        // blocked, can erase its read-only feedback), so the toggle uses only the
        // synchronous dirty-page repaint owned by `runNodeEdit`.
        ed.runNodeEdit(() => ed.doc.toggleChecklistItem(marker.node));
        ed.focusEditorSurface();
      });
    }
  }

  /** Paints the caret or highlight for `sel` from engine geometry. */
  function paintSelection({ anchor, focus }) {
    // A container with no `caret` region paints neither. The insertion point still
    // EXISTS — the engine has to have one — it is just not shown or movable here.
    if (!ed.chromeShows("caret")) return;
    // Keep the editable proxy on the caret so an IME candidate window and the iOS
    // autocorrect bar anchor where the user is actually typing (docs/105 UX-001).
    ed.positionEditorTextInput(focus);
    const collapsed = anchor.node === focus.node && anchor.offset === focus.offset;
    if (!collapsed) {
      const rects = ed.doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
      if (rects.length >= 5) {
        for (let i = 0; i < rects.length; i += 5) ed.place(rects.slice(i, i + 5), "highlight");
        return;
      }
      // No visible rects (e.g. a tiny drag within one caret slot) → fall to caret.
    }
    // The load-time insertion point is real — every Insert command acts on it —
    // but it is not yet the user's caret, and the editor surface it belongs to
    // does not have focus, so keystrokes would go nowhere. Painting a blinking
    // cursor there would be a lie; it appears the moment `#pages` takes focus.
    if (collapsed && ed.caretIsImplicit()) return;
    ed.place(ed.doc.caretRect(focus.node, focus.offset), "caret");
  }

  /** The live IME composition overlay element, or `null` when no composition
   *  is in progress. Kept across `compositionupdate` calls so its text can be
   *  updated in place instead of recreated every keystroke. */
  let imePreeditEl = null;

  /** Shows the IME live-preedit overlay at a caret anchor: the in-progress
   * composition text the browser has not yet committed. Never touches the
   * document — `compositionend` still owns the actual insertion
   * (`commitComposedText`) — this only makes the intermediate state visible
   * (docs/67-EDITOR-UX-GAP-ANALYSIS.md, "IME live preedit"). */
  function showImePreedit(node, offset, text) {
    hideImePreedit();
    const flat = ed.doc?.caretRect(node, offset) ?? [];
    if (flat.length < 5) return;
    const [pageNumber, x, y, , h] = flat;
    const page = ed.pages[pageNumber - 1];
    if (!page?.overlay) return;
    const { sx, sy } = ed.scaleOf(page);
    const el = document.createElement("div");
    el.className = "ime-preedit";
    el.style.left = `${x * sx}px`;
    el.style.top = `${y * sy}px`;
    el.style.height = `${h * sy}px`;
    el.textContent = text;
    page.overlay.appendChild(el);
    imePreeditEl = el;
  }

  /** Updates the live-preedit overlay's text (a `compositionupdate` tick). */
  function updateImePreedit(text) {
    if (imePreeditEl) imePreeditEl.textContent = text;
  }

  /** Removes the live-preedit overlay, if shown. */
  function hideImePreedit() {
    imePreeditEl?.remove();
    imePreeditEl = null;
  }


  /** Navigates to an internal-link target and makes the target page/caret visible. */
  function navigateToAnchor(node, offset, pageNumber) {
    if (!node) return;
    ed.pendingFormat = null;
    ed.selection = {
      anchor: { node, offset },
      focus: { node, offset },
    };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollCaretIntoView("center");
  }

  return {
    hideImePreedit, navigateToAnchor, paintChecklistMarkers, paintSelection, showImePreedit,
    updateImePreedit,
  };
}
