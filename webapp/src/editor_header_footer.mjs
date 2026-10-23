// Header and footer editing: entering and leaving a running band, its markers
// and context, notes, and the page-setup gate. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { runWrapPlan, wrapAvailability, wrapPlan } from "./object_arrange.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createHeaderFooter(ed) {
  // ---- Header / footer editing -----------------------------------------------

  /** One viewport-driven projection update per animation frame. */
  let runningContextScrollFrame = 0;

  // ---- Header / footer markers -------------------------------------------------
  // LibreOffice Writer raises a marker with a `+` button when the pointer is in the
  // top or bottom margin, and that is the affordance adopted in docs/85 §8d. Word
  // and Docs rely on a bare double-click, which is invisible to anyone who has not
  // been told about it — and an ABSENT header has nothing to double-click on, so
  // the capability is unreachable without knowing the command exists. This is the
  // same one-surface-only reachability failure the command-surface audit kept
  // finding, so it is worth the small piece of chrome.
  let runningMarkerEl = null;
  let runningMarkerBand = null;

  /** ONE page's running-content context: its top/bottom margin bands in page-local
   *  twips, which section (1-based, of how many) the page belongs to, and whether
   *  each band is inherited from the previous section ("Same as Previous").
   *
   *  Asked of the engine PER PAGE. This used to read `pageSetup()` — the FIRST
   *  section's geometry — so on a document whose later section changes orientation
   *  or margins (the reported case: page 5 turns landscape) every page of that
   *  section drew its header boundary at the wrong height, and nothing on screen
   *  could say which section's header was open. */
  function runningBandsOf(page) {
    if (!ed.doc || !page) return null;
    let bands;
    try {
      bands = JSON.parse(ed.doc.runningBands(page.pageNumber));
    } catch {
      return null;
    }
    if (!bands) return null;
    return {
      top: bands.headerTwips,
      bottom: bands.footerTwips,
      section: bands.section,
      sectionCount: bands.sectionCount,
      headerLinked: bands.headerLinked === true,
      footerLinked: bands.footerLinked === true,
    };
  }

  function hideRunningMarker() {
    if (!runningMarkerEl) return;
    runningMarkerEl.hidden = true;
    runningMarkerBand = null;
  }

  /** Shows the marker over `band` on `page`, creating the element on first use. */
  function showRunningMarker(page, band) {
    if (runningMarkerBand === band && runningMarkerEl && !runningMarkerEl.hidden) return;
    if (!runningMarkerEl) {
      runningMarkerEl = document.createElement("button");
      runningMarkerEl.type = "button";
      runningMarkerEl.className = "running-marker";
      runningMarkerEl.addEventListener("click", (event) => {
        event.preventDefault();
        const target = runningMarkerBand;
        hideRunningMarker();
        if (target) editRunningContent(target);
      });
    }
    if (page.wrap && runningMarkerEl.parentElement !== page.wrap) {
      page.wrap.appendChild(runningMarkerEl);
    }
    runningMarkerBand = band;
    runningMarkerEl.textContent = band === "header" ? "+ Header" : "+ Footer";
    runningMarkerEl.title = `Edit the page ${band}`;
    runningMarkerEl.setAttribute("aria-label", `Edit the page ${band}`);
    // Page-local, inside the sheet: positioned against the page's own wrap (which
    // the overlays already use as their containing block) rather than against the
    // scroller, where the offsets put it above the paper entirely.
    runningMarkerEl.style.left = "10px";
    runningMarkerEl.style.top = band === "header" ? "10px" : "auto";
    runningMarkerEl.style.bottom = band === "footer" ? "10px" : "auto";
    runningMarkerEl.hidden = false;
  }

  /** Which margin band a page-local y falls in, or `null` for the body. The one
   *  place the band geometry is interpreted, so the marker, the double-click and
   *  the context all agree on where the header is. */
  function runningBandAt(page, yTwip) {
    if (!ed.doc || !page) return null;
    // Asked of the engine, which owns page geometry. Deriving it here from the
    // DOCUMENT's page setup was wrong the moment a document had two sections with
    // different margins: the host and the engine then disagreed about where a
    // page's header ends, and the same click meant two different things.
    return ed.doc.bandAt(page.pageNumber, 0, yTwip) || null;
  }

  /** Decides whether the pointer is in a margin band and shows/hides the marker.
   *  Suppressed while already editing running content — the context is open, so
   *  the marker would only be an invitation to do what is already happening. */
  function updateRunningMarker(page, event) {
    // Moving onto the marker itself must not dismiss it: the pointer is then over
    // the button rather than a page, so `pageFromEvent` finds nothing and the
    // naive read of that is "left the band". Hovering an affordance cannot be the
    // thing that destroys it.
    if (runningMarkerEl && event?.target && runningMarkerEl.contains(event.target)) return;
    if (!ed.doc || ed.runningEditBand || !page) {
      hideRunningMarker();
      return;
    }
    // `bandAt` alone answers this. Asking `runningBands` first — a second engine
    // call and a JSON parse, per pointer move — only proved the page had margins.
    const { y } = ed.pointToTwip(page, event);
    const band = runningBandAt(page, y);
    if (band) showRunningMarker(page, band);
    else hideRunningMarker();
  }

  /** The page every running-content command acts on: the page whose band is open
   *  while editing one, otherwise the page in view. Stays here rather than moving
   *  with the variant toggles, because it reads `runningEditPage` and `pages` — the
   *  application's own state — and is handed to the module as `io.runningPage`. */
  function runningEditPageOrView() {
    if (ed.runningEditPage) {
      return ed.pages.find((page) => page.pageNumber === ed.runningEditPage) ?? pageInView();
    }
    return pageInView();
  }

  /** Whether a section-property change is forbidden right now, having already said
   *  why. The two review gates every page-setup edit shares, in one place: the
   *  modules that own those surfaces take this as `io.blockedFromMutating` rather
   *  than each carrying its own copy of the review policy. */
  function blockedFromPageSetup() {
    if (ed.blockMutationInViewing()) return true;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Page-setup changes cannot be tracked yet; switch to Editing", "error");
      return true;
    }
    return false;
  }

  /** Inserts a footnote or endnote at the caret and puts the caret IN the new
   *  note's body, which is where Word and Docs both leave it.
   *
   *  The engine ops existed but nothing called them, precisely because a note body
   *  could not be typed into: inserting one would have created a note the user
   *  could not fill in. That is fixed — a note body is an ordinary block surface
   *  now — so the command can finally ship. */
  async function insertNote(kind) {
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus(`Inserting a ${kind} cannot be tracked yet; switch to Editing`, "error");
      return;
    }
    let result;
    try {
      result =
        kind === "footnote"
          ? ed.doc.insertFootnote(ed.selection.focus.node, ed.selection.focus.offset)
          : ed.doc.insertEndnote(ed.selection.focus.node, ed.selection.focus.offset);
    } catch (err) {
      ed.setStatus(`Could not insert the ${kind}: ${err.message ?? err}`, "error");
      return;
    }
    const node = result.node;
    const offset = result.offset;
    await ed.applyEditResult(result);
    // `applyEditResult` already left the caret on the note's first paragraph; make
    // that explicit and tell the user where they now are.
    ed.selection = { anchor: { node, offset }, focus: { node, offset } };
    ed.drawSelection();
    ed.updateToolbar();
    ed.setStatus(`${kind === "footnote" ? "Footnote" : "Endnote"} added — type the note text`);
    ed.focusEditorSurface();
  }

  /** Creates an empty header or footer **for the section `page` belongs to** and
   *  enters it. Gated like any other mutation: refused read-only in Viewing, and
   *  blocked in Suggesting because adding running content has no tracked-change
   *  representation. */
  async function createRunningContent(band, page = pageInView()) {
    if (!ed.doc || !page) return;
    if (ed.blockMutationInViewing()) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus(`Adding a ${band} cannot be tracked yet; switch to Editing`, "error");
      return;
    }
    // The context is known BEFORE the edit — the user asked for this page's band —
    // and the edit needs it: applying an edit places and scrolls to its caret, and
    // a caret in running content has a placement on every page. Entering
    // afterwards meant that scroll resolved without a context and landed on the
    // last page's copy, throwing the user to the end of the document.
    const previous = { band: ed.runningEditBand, page: ed.runningEditPage };
    setRunningContext(band, page);
    let result;
    try {
      result = ed.doc.createRunningContent(band, page.pageNumber);
    } catch (err) {
      if (previous.band) setRunningContext(previous.band, { pageNumber: previous.page });
      else exitRunningEdit();
      ed.setStatus(`Could not add a ${band}: ${err.message ?? err}`, "error");
      return;
    }
    const node = result.node;
    const offset = result.offset;
    await ed.applyEditResult(result);
    // The new body is now laid out, so its paragraph has geometry to put a caret
    // on; entering after the render is what makes the caret visible.
    enterRunningEdit(band, node, offset, page);
  }

  /** Finds a caret position inside a page's header or footer by walking the band
   *  inward from the page edge until the engine answers.
   *
   *  `runningHitTest` resolves only points actually inside placed running content,
   *  and the band's height depends on the document's own margins, so a fixed probe
   *  point would miss. Walking in from the edge finds the first line whatever the
   *  geometry, and stops well before the body's content area so a document with no
   *  running content answers nothing rather than capturing a body line. */
  function probeRunningBand(page, band) {
    if (!ed.doc) return null;
    const x = Math.round(page.wTwip / 2);
    // A generous fraction of the page, which covers ordinary and large margins
    // without reaching the middle of the sheet.
    const span = Math.round(page.hTwip * 0.2);
    const STEP = 40; // ~0.7mm; fine enough to land inside a single line box
    for (let d = 0; d <= span; d += STEP) {
      const y = band === "header" ? d : page.hTwip - d;
      const hit = ed.doc.runningHitTest(page.pageNumber, x, y);
      if (!hit) continue;
      const found = { band: hit.band, node: hit.node, offset: hit.offset };
      hit.free?.();
      if (found.band === band) return found;
    }
    return null;
  }

  /** Enters header/footer editing at `position`, an ordinary caret position that
   *  happens to live in running content. */
  function enterRunningEdit(band, node, offset, page) {
    setRunningContext(band, page);
    ed.objectSelection = null;
    ed.tableRange.clear();
    ed.selection = { anchor: { node, offset }, focus: { node, offset } };
    ed.pagesEl.dataset.runningEdit = band;
    document.body.classList.add("running-edit");
    hideRunningMarker();
    drawRunningBands(band);
    ed.setStatus(`Editing the ${band} — press Esc to return to the document`);
    ed.focusEditorSurface();
    ed.drawSelection();
    ed.updateToolbar();
  }

  /** Records which page's band is open, on the host AND in the engine.
   *
   *  Running content is ONE body drawn on every page, so a caret in it has a
   *  placement per page and "where is this caret" is unanswerable without naming
   *  one. The engine holds the context so every geometry answer uses it; keeping
   *  it only here is what put the caret on page 1's header while the user was
   *  editing page 3's and scrolled the view there. */
  function setRunningContext(band, page) {
    ed.runningEditBand = band;
    ed.runningEditPage = page?.pageNumber ?? ed.runningEditPage ?? 1;
    ed.pagesEl.dataset.runningEdit = band;
    ed.doc?.setEditContext(band, ed.runningEditPage);
  }

  /** Marks the band being edited on every page: a dashed boundary at the edge of
   *  the running area plus a small label, the way Word and LibreOffice show it.
   *
   *  This replaced dimming the canvas. The header is painted INTO the page raster,
   *  so dimming the page washed out the content being edited and greyed the whole
   *  sheet — the cue has to sit beside the band, not over it. */
  function drawRunningBands(band) {
    clearRunningBands();
    for (const page of ed.materializedPages()) {
      // Per PAGE, because the band belongs to the page's own section: a document
      // that turns landscape at page 5 has different margins there, and one
      // document-level band height drew the boundary in the wrong place on every
      // page of that section.
      const bands = runningBandsOf(page);
      if (!bands) continue;
      const { sy } = ed.scaleOf(page);
      const el = document.createElement("div");
      el.className = `running-band${band === "footer" ? " is-footer" : ""}`;
      if (band === "header") {
        el.style.top = "0px";
        el.style.height = `${Math.round(bands.top * sy)}px`;
      } else {
        el.style.bottom = "0px";
        el.style.height = `${Math.round(bands.bottom * sy)}px`;
      }
      const label = document.createElement("span");
      label.className = "running-band-label";
      label.textContent = runningBandLabel(band, bands);
      el.appendChild(label);
      page.wrap.appendChild(el);
    }
  }

  /** The band's label, which in Word NAMES THE SECTION once a document has more
   *  than one ("Header -Section 2-") and says when the band is inherited ("Same as
   *  Previous").
   *
   *  Without it two pages showing different headers looked identical, so a user
   *  editing one section's header and seeing another section's pages not change had
   *  nothing on screen to explain why — the reported confusion. */
  function runningBandLabel(band, bands) {
    const name = band === "header" ? "Header" : "Footer";
    if (!bands || (bands.sectionCount ?? 1) <= 1) return name;
    const linked = band === "header" ? bands.headerLinked : bands.footerLinked;
    return `${name} -Section ${bands.section}-${linked ? " · Same as Previous" : ""}`;
  }

  function clearRunningBands() {
    for (const el of ed.pagesEl.querySelectorAll(".running-band")) el.remove();
  }

  /** Leaves header/footer editing. The caret stays where it is rather than
   *  jumping: Word and Docs both leave the insertion point alone and simply end
   *  the context, and a caret that teleports on Esc loses the user's place. */
  function exitRunningEdit() {
    if (!ed.runningEditBand) return false;
    ed.runningEditBand = null;
    ed.runningEditPage = null;
    ed.doc?.setEditContext(undefined, undefined);
    delete ed.pagesEl.dataset.runningEdit;
    document.body.classList.remove("running-edit");
    clearRunningBands();
    ed.setStatus("");
    ed.drawSelection();
    ed.updateToolbar();
    return true;
  }

  /** The page the user is looking at: the one covering the middle of the viewport,
   *  or the nearest page when the midpoint is in a sheet gap. Entering a header
   *  from the ribbon or the palette must act on THAT page — `pages[0]` sent the
   *  caret to page one and scrolled the view off whatever the user was working on. */
  function pageInView() {
    if (!ed.pages.length) return null;
    const viewport = document.getElementById("viewport");
    if (!viewport) return ed.pages[0];
    const middle = viewport.getBoundingClientRect().top + viewport.clientHeight / 2;
    let nearest = ed.pages[0];
    let nearestDistance = Number.POSITIVE_INFINITY;
    for (const page of ed.materializedPages()) {
      const rect = page.wrap.getBoundingClientRect();
      if (rect.top <= middle && rect.bottom >= middle) return page;
      const distance = rect.bottom < middle ? middle - rect.bottom : rect.top - middle;
      if (distance < nearestDistance) {
        nearest = page;
        nearestDistance = distance;
      }
    }
    return nearest;
  }

  /** Re-project a repeated running-content caret onto the page now in view.
   *
   * The selection's model node/offset and owning story do not change. Only the
   * page copy used for geometry changes, through the engine's edit-context API.
   * Throttling to one animation frame keeps wheel/touch scrolling bounded and
   * prevents overlay churn for every raw scroll event. */
  function syncRunningContextToViewport() {
    runningContextScrollFrame = 0;
    if (!ed.runningEditBand || !ed.selection) return;
    const visiblePage = pageInView();
    if (!visiblePage || visiblePage.pageNumber === ed.runningEditPage) return;
    // Different-first/even/odd sections can put a different running story on the
    // visible page. Ask the engine whether the current model focus is actually
    // placed there before changing projection; scrolling must never retarget the
    // selection into an unrelated header/footer body.
    let projected = [];
    try {
      ed.doc.setEditContext(ed.runningEditBand, visiblePage.pageNumber);
      projected = ed.doc.caretRect(ed.selection.focus.node, ed.selection.focus.offset);
    } catch {
      return;
    } finally {
      ed.doc.setEditContext(ed.runningEditBand, ed.runningEditPage);
    }
    if (projected.length < 5 || projected[0] !== visiblePage.pageNumber) return;
    ed.breakTypingSession();
    setRunningContext(ed.runningEditBand, visiblePage);
    ed.drawSelection();
  }

  ed.viewportEl.addEventListener(
    "scroll",
    () => {
      if (!ed.runningEditBand || runningContextScrollFrame) return;
      runningContextScrollFrame = requestAnimationFrame(syncRunningContextToViewport);
    },
    { passive: true },
  );

  /** Puts the caret in a page's header or footer, entering the context. Used by
   *  the double-click in the band and by the Insert menu / palette commands —
   *  a double-click alone is invisible to anyone who has not been told about it,
   *  and an ABSENT header has nothing to double-click on (docs/85 §8d). */
  function editRunningContent(band, page = pageInView()) {
    if (!ed.doc || !page) return;
    const hit = probeRunningBand(page, band);
    if (hit) {
      enterRunningEdit(hit.band, hit.node, hit.offset, page);
      return;
    }
    // Geometry can miss an existing story when its content is outside the
    // narrow probe band. Resolve the model's existing body before considering
    // creation; creating here would relink the section and orphan the real
    // header/footer. Asked about THIS page, so the body is the one this page
    // shows — resolving it document-wide sent the edit into the first section's
    // header while the user was looking at a later, differently-oriented one.
    const existing = ed.doc.runningContentCaret?.(band, page.pageNumber);
    if (existing) {
      const position = { node: existing.node, offset: existing.offset };
      existing.free?.();
      enterRunningEdit(band, position.node, position.offset, page);
      return;
    }
    // Nothing placed in that band: make one. Word and Docs both create the
    // header the moment you ask to edit a document that has none, rather than
    // refusing — the ask IS the intent. One undoable action creates the body and
    // links the section to it, and the caret it returns is the new body's first
    // (empty) paragraph.
    void createRunningContent(band, page);
  }

  /** Changes a floating object's text-wrap mode (docs/85 §5.3), as one undoable op.
   *  Blocked fail-closed in Suggesting/Viewing mode by `runEdit`'s gate. */
  function setObjectWrap(mode) {
    const state = ed.arrangeState();
    if (!state || !wrapAvailability(state.read).available) return;
    const plan = wrapPlan(mode, state.read, state.wrap);
    // An empty plan means the object is already in that mode: a chip that
    // re-asserts the current choice must not fill the undo stack.
    if (plan.length === 0) return;
    // ONE engine action (`docs/109` OL-04): as two calls only the second
    // result's pages were repainted and the text did not reflow.
    ed.runEdit(() => runWrapPlan(ed.doc, state.root, plan), { gate: true });
  }

  /** Deletes the selected object as one undoable action (docs/85 §4). Mirrors the
   *  wrap op's apply path: `runEdit(..., { gate:true })` is the single fail-closed
   *  gate (read-only in Viewing, untracked-blocked in Suggesting). The object
   *  selection is dropped inside the thunk — which `runEdit` only runs once the gate
   *  passes — so `applyEditResult` repaints with the plain text caret the
   *  `EditResult` points at (the object's former surrounding-text anchor). */
  function deleteSelectedObject() {
    if (!ed.objectSelection || ed.objectSelection.mode !== "selected" || !ed.objectSelection.canDelete) return;
    const root = ed.objectSelection.ref.root;
    // A member of a group is deleted ALONE, as in Word; sending it to the root
    // deleted everything grouped with it (HF-214).
    const member = ed.insideGroupSelection() ? ed.objectSelection.node : null;
    ed.runEdit(
      () => {
        const res = member ? ed.doc.deleteGroupMember(member) : ed.doc.deleteObject(root);
        ed.objectSelection = null;
        ed.clearObjectStatus();
        return res;
      },
      { gate: true },
    );
  }

  return {
    blockedFromPageSetup, deleteSelectedObject, drawRunningBands, editRunningContent,
    exitRunningEdit, hideRunningMarker, insertNote, pageInView, runningBandAt,
    runningEditPageOrView, setObjectWrap, setRunningContext, updateRunningMarker,
  };
}
