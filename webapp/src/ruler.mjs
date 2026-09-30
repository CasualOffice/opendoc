// The horizontal ruler: the page's margin zones, the caret paragraph's indent
// markers, and its tab stops.
//
// Extracted from `main.js` whole, rather than a piece at a time, because it is
// one cohesive interaction: every part of it is keyed to the same scale, and the
// scale is the thing that was wrong. `109` HF-085 asks for main.js to stop being
// the webapp; this is 244 lines of it, and extracting it is also how the fix
// below is paid for, since main.js was exactly at its line ratchet.
//
// THE DEFECT THIS EXTRACTION CARRIES A FIX FOR — the owner's report, "rules
// doesnt work for landscape pages":
//
// The ruler took its geometry from `pageGeometry()`, which answered with the
// DOCUMENT'S OPENING SECTION for every page, and its pixel width from the band
// model's `widths[0]` — page 1's. On a document whose later section turns
// landscape, pages in that section were drawn with the first section's portrait
// width and the first section's margins. The scale is `pageWidthPx / widthTwip`,
// so a wrong width moves every tick and every number, and the shaded margin
// zones stop lining up with the paper underneath them.
//
// Both halves are now keyed to ONE page index — the caret's — so the strip always
// describes the page the user is typing on. That matches Word and Docs, where the
// ruler tracks the insertion point rather than whatever is scrolled into view;
// it is also the only choice consistent with the markers already drawn here,
// which have always been the CARET paragraph's indents and tab stops. A ruler
// whose zones described one page while its markers described another would be
// worse than either.
//
// Pure DOM plus an injected engine handle: no module-level state, so a second
// instance (a second editor on a page) does not share a scale with the first.

/** The tab-stop types, in Word's cycle order, as the engine codes them:
 *  0 left, 1 centre, 2 right, 3 decimal, 4 bar.
 *
 *  **Bar was added here when tab stops gained a second surface.** Until then no
 *  control in the product could produce a code-4 stop, so a bar could only
 *  arrive from an imported document, and this array's `?? "L"` fallback drew it
 *  as a LEFT stop — a ruler saying something false about the paragraph, quietly.
 *  `layout.tabStops`' dialog can place one (`flow.rs`/`tabs.rs` have drawn them
 *  all along), so the ruler has to be able to say so, and the corner's cycle
 *  has to be able to reach it: `% TAB_LETTER.length` is what makes both true
 *  from one edit. Word's own bar marker is a vertical rule, which is what "|"
 *  is standing in for. */
const TAB_LETTER = ["L", "C", "R", ".", "|"];

/**
 * Builds a ruler bound to one document view.
 *
 * `getDoc`, `getSelection`, `getPages` and `getBandModel` are read on every call
 * rather than captured, because all four are replaced wholesale when a document
 * is opened or re-paginated.
 *
 * Complexity: `build()` is O(page width in ticks) — a few hundred nodes, bounded
 * by the paper and not by the document. `syncToCaret()` is O(1) unless the caret
 * changed page, which is the only case that rebuilds.
 */
export function createRuler({
  getDoc,
  getSelection,
  getPages,
  getBandModel,
  // "" when the ruler may be drawn, the reason when it may not. Reflow is the
  // one caller today (`docs/151` §6.4): a tile has no page margins to drag, so a
  // ruler over one would be a scale for a page that is not there. It is a
  // REASON rather than a boolean because the withholding has to be sayable —
  // the toggle that caused it carries this sentence, so the reader is told what
  // reflow costs rather than watching a strip vanish.
  withheldReason = () => "",
  runToolbarEdit,
  twipsPerInch,
  labels,
}) {
  const element = document.createElement("div");
  element.className = "ruler";
  element.hidden = true;
  const track = document.createElement("div");
  track.className = "ruler-track";
  element.appendChild(track);

  let geom = null; // { width, marginStart, marginEnd } in twips, for `builtFor`
  let scale = 0; // px per twip at the current zoom
  let builtFor = -1; // the page index `geom` and `scale` describe
  const markers = {}; // key -> element
  let tabInsertCode = 0; // the type a new tab stop gets

  const px = (t) => t * scale;

  /** The page the ruler should describe: the caret's, or the first page when
   *  there is no caret yet. 1-based from the engine, 0-based here. */
  function caretPageIndex() {
    const doc = getDoc();
    const selection = getSelection();
    if (!doc || !selection) return 0;
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (!flat?.length) return 0;
    return Math.max(0, flat[0] - 1);
  }

  /** This page's own section geometry. `null` when the engine will not answer —
   *  a page index it does not have — which hides the ruler rather than drawing a
   *  scale from the wrong page, the failure this whole change is about. */
  function geometryOf(index) {
    const doc = getDoc();
    let g = null;
    try {
      g = doc.pageRulerGeometry(index);
    } catch {
      return null;
    }
    const out = {
      width: g.widthTwip,
      marginStart: g.marginStartTwip,
      marginEnd: g.marginEndTwip,
    };
    // The old `pageGeometry()` call site never freed its handle. A ruler rebuilds
    // on every zoom step, so that leaked one wasm object per step for the life of
    // the tab.
    g.free?.();
    return out.width > 0 ? out : null;
  }

  /** Rebuilds the scale, margin zones and ticks for one page. */
  function build(pageIndex = caretPageIndex()) {
    const doc = getDoc();
    const pages = getPages();
    const band = getBandModel();
    const withheld = withheldReason();
    if (!doc || !pages.length || !band || withheld) {
      element.hidden = true;
      // Recorded on the element so the withholding is assertable rather than
      // indistinguishable from "no document yet", which is the same strip in
      // the same state for a completely different reason.
      if (withheld) element.dataset.withheld = withheld;
      else delete element.dataset.withheld;
      builtFor = -1;
      return;
    }
    delete element.dataset.withheld;
    const index = Math.min(Math.max(pageIndex, 0), pages.length - 1);
    const next = geometryOf(index);
    // The band model is the source for the RENDERED width: page 1 has no sheet at
    // all once the reader has scrolled away from it, and the ruler still has to be
    // the width of the paper. Per page, not `widths[0]` — that was half the defect.
    const pageWidthPx = band.widths[index];
    if (!next || !pageWidthPx) {
      element.hidden = true;
      builtFor = -1;
      return;
    }
    geom = next;
    builtFor = index;
    scale = pageWidthPx / geom.width;
    element.style.width = `${pageWidthPx}px`;

    track.replaceChildren();
    const contentStart = geom.marginStart;

    // The unshaded content span between the page margins. Clicking it adds a tab
    // stop at that position, in the current tab type, on the caret paragraph.
    const content = document.createElement("div");
    content.className = "ruler-content";
    content.style.left = `${px(geom.marginStart)}px`;
    content.style.width = `${px(geom.width - geom.marginStart - geom.marginEnd)}px`;
    content.addEventListener("pointerdown", (e) => {
      if (!getDoc() || !getSelection() || e.button !== 0) return;
      const pos = Math.max(0, Math.round(e.offsetX / scale));
      e.preventDefault();
      e.stopPropagation();
      runToolbarEdit((a, b, c, d) => getDoc().setTabStop(a, b, c, d, pos, tabInsertCode), {
        paragraphLevel: true,
      });
      updateMarkers();
    });
    track.appendChild(content);

    // Word-style tab-type selector at the left edge; click to cycle L/C/R/dot.
    const corner = document.createElement("button");
    corner.type = "button";
    corner.className = "tab-corner";
    corner.title = labels.tabCorner;
    corner.textContent = TAB_LETTER[tabInsertCode];
    corner.addEventListener("click", () => {
      tabInsertCode = (tabInsertCode + 1) % TAB_LETTER.length;
      corner.textContent = TAB_LETTER[tabInsertCode];
    });
    track.appendChild(corner);

    // Minor ticks every 1/8", plus a numbered major tick at each inch measured
    // from the left margin (0 at the content edge).
    for (let t = 0; t <= geom.width; t += twipsPerInch / 8) {
      const tick = document.createElement("div");
      tick.className = "ruler-tick minor";
      tick.style.left = `${px(t)}px`;
      track.appendChild(tick);
    }
    for (let i = 0, t = contentStart; t <= geom.width + 1; i++, t = contentStart + i * twipsPerInch) {
      const tick = document.createElement("div");
      tick.className = "ruler-tick major";
      tick.style.left = `${px(t)}px`;
      track.appendChild(tick);
      if (i > 0) {
        const num = document.createElement("div");
        num.className = "ruler-num";
        num.textContent = String(i);
        num.style.left = `${px(t)}px`;
        track.appendChild(num);
      }
    }

    // Indent markers (recreated each build; positioned by the selection). Only
    // the markers are pointer-interactive; the rest of the strip is click-through,
    // so a marker drag can never steal a page click.
    for (const [key, cls] of [
      ["firstLine", "down"],
      ["left", "up"],
      ["right", "up"],
    ]) {
      const m = document.createElement("div");
      m.className = `ruler-marker ${cls}`;
      m.dataset.marker = key;
      m.title = labels[key];
      m.addEventListener("pointerdown", (e) => startMarkerDrag(key, e));
      track.appendChild(m);
      markers[key] = m;
    }

    element.hidden = false;
    updateMarkers();
  }

  /** Follows the caret. Rebuilds only when it has moved to a page the current
   *  scale does not describe — crossing a section break is the case that matters,
   *  and it is the case that used to go unnoticed. */
  function syncToCaret() {
    if (!getDoc() || !getPages().length) return;
    const index = Math.min(caretPageIndex(), getPages().length - 1);
    if (index !== builtFor) build(index);
    else updateMarkers();
  }

  /** Positions the three indent markers from the caret paragraph's indentation. */
  function updateMarkers() {
    if (!geom || !markers.left) return;
    const doc = getDoc();
    const selection = getSelection();
    let start = 0;
    let end = 0;
    let firstLine = 0;
    if (doc && selection) {
      const ind = doc.paragraphIndent(selection.focus.node);
      start = ind.startTwip;
      end = ind.endTwip;
      firstLine = ind.firstLineTwip - ind.hangingTwip;
      ind.free();
    }
    const contentStart = geom.marginStart;
    const contentEnd = geom.width - geom.marginEnd;
    markers.left.style.left = `${px(contentStart + start)}px`;
    markers.firstLine.style.left = `${px(contentStart + start + firstLine)}px`;
    markers.right.style.left = `${px(contentEnd - end)}px`;
    renderTabStops();
  }

  /** Draws the caret paragraph's tab stops as glyphs (recreated each update).
   *  Each glyph: click cycles its type, drag moves it, drag off removes it. */
  function renderTabStops() {
    for (const g of track.querySelectorAll(".tab-glyph")) g.remove();
    const doc = getDoc();
    const selection = getSelection();
    if (!doc || !selection || !geom) return;
    const tabs = doc.paragraphTabs(selection.focus.node); // flat [pos, code, …]
    for (let k = 0; k < tabs.length; k += 2) {
      const pos = tabs[k];
      const code = tabs[k + 1];
      const g = document.createElement("div");
      g.className = `tab-glyph tab-${code}`;
      g.textContent = TAB_LETTER[code] ?? "L";
      g.style.left = `${px(geom.marginStart + pos)}px`;
      g.title = labels.tabGlyph;
      g.addEventListener("pointerdown", (e) => startTabDrag(pos, code, g, e));
      track.appendChild(g);
    }
  }

  /** A tab-glyph pointer interaction: no move → cycle type; horizontal move →
   *  reposition; released off the strip → delete (Word's drag-off-to-remove). */
  function startTabDrag(pos, code, glyph, ev) {
    const doc = getDoc();
    if (!doc || !getSelection() || ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    const trackRect = track.getBoundingClientRect();
    let moved = false;
    let curPos = pos;
    const onMove = (e) => {
      if (Math.abs(e.clientX - ev.clientX) > 3 || Math.abs(e.clientY - ev.clientY) > 3) moved = true;
      curPos = Math.max(0, Math.round((e.clientX - trackRect.left) / scale - geom.marginStart));
      glyph.style.left = `${px(geom.marginStart + curPos)}px`; // live
    };
    const onUp = (e) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      const offRuler = e.clientY > trackRect.bottom + 14 || e.clientY < trackRect.top - 14;
      if (offRuler) {
        runToolbarEdit((a, b, c, d) => getDoc().removeTabStop(a, b, c, d, pos), {
          paragraphLevel: true,
        });
      } else if (moved && curPos !== pos) {
        runToolbarEdit((a, b, c, d) => getDoc().moveTabStop(a, b, c, d, pos, curPos), {
          paragraphLevel: true,
        });
      } else {
        runToolbarEdit(
          (a, b, c, d) =>
            getDoc().setTabStop(a, b, c, d, pos, (code + 1) % TAB_LETTER.length),
          { paragraphLevel: true },
        );
      }
      updateMarkers();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  /** Drag an indent marker. Uses window-level move/up listeners (never
   *  `setPointerCapture`, which once froze the page when a click did not release
   *  the capture) so the pointer is always released. The ruler acts on the caret
   *  paragraph, so a selection is required. */
  function startMarkerDrag(key, ev) {
    const doc = getDoc();
    const selection = getSelection();
    if (!doc || !selection || !geom) return;
    ev.preventDefault();
    ev.stopPropagation(); // don't let the pointerdown fall through to the page

    const trackRect = track.getBoundingClientRect();
    const contentStart = geom.marginStart;
    const contentEnd = geom.width - geom.marginEnd;

    // The left marker carries the first-line marker with it (Word/Docs
    // behaviour); capture the current first-line offset to preserve it.
    const ind = doc.paragraphIndent(selection.focus.node);
    const startTwip = ind.startTwip;
    const firstLineOff = ind.firstLineTwip - ind.hangingTwip;
    ind.free();

    const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);
    const xTwipAt = (clientX) => clamp((clientX - trackRect.left) / scale, 0, geom.width);

    // Live visual feedback while dragging (the model is committed on pointerup).
    const preview = (x) => {
      if (key === "left") {
        markers.left.style.left = `${px(x)}px`;
        markers.firstLine.style.left = `${px(x + firstLineOff)}px`;
      } else {
        markers[key].style.left = `${px(x)}px`;
      }
    };

    // Resolve the marker's x to an absolute indent for the engine setter.
    const commit = async (x) => {
      let call;
      if (key === "left") {
        const twips = Math.round(x - contentStart);
        call = (sn, so, en, eo) => getDoc().setLeftIndent(sn, so, en, eo, twips);
      } else if (key === "firstLine") {
        const twips = Math.round(x - contentStart - startTwip);
        call = (sn, so, en, eo) => getDoc().setFirstLineIndent(sn, so, en, eo, twips);
      } else {
        const twips = Math.round(contentEnd - x);
        call = (sn, so, en, eo) => getDoc().setRightIndent(sn, so, en, eo, twips);
      }
      await runToolbarEdit(call);
      updateMarkers(); // snap to the model's clamped truth
    };

    markers[key].classList.add("dragging");
    const onMove = (e) => preview(xTwipAt(e.clientX));
    const onUp = (e) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      markers[key].classList.remove("dragging");
      commit(xTwipAt(e.clientX));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  return {
    element,
    build,
    syncToCaret,
    updateMarkers,
    /** The page index the current scale describes; -1 when hidden. For tests and
     *  for callers that need to know whether a rebuild is pending. */
    builtForPage: () => builtFor,
  };
}
