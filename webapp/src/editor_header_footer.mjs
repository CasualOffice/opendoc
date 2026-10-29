// Header and footer editing: entering and leaving a running band, its markers
// and context, notes, and the page-setup gate. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { runWrapPlan, wrapAvailability, wrapPlan } from "./object_arrange.mjs";
import { list, t } from "./i18n.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";
import {
  BAND_COMMANDS, PAGE_NUMBER_POSITIONS, bandCommandLabel, bandCommandReason, bandLabel, parseLinkState,
} from "./header_footer_commands.mjs";
import { PAGE_OF_TOTAL, splitPageOfTotal } from "./header_footer_commands.mjs";
import { EN_STRINGS } from "./en_strings.mjs";

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
    runningMarkerEl.textContent = band === "header" ? t("runningBand.addHeader") : t("runningBand.addFooter");
    const name = band === "header" ? t("runningBand.editHeader") : t("runningBand.editFooter");
    runningMarkerEl.title = name;
    runningMarkerEl.setAttribute("aria-label", name);
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
    showBandTab();
    drawRunningBands(band);
    ed.setStatus(band === "header" ? t("runningBand.editingHeader") : t("runningBand.editingFooter"));
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
      label.textContent = bandLabel(band, bands);
      el.appendChild(label);
      page.wrap.appendChild(el);
      if (page.pageNumber === ed.runningEditPage) placeBandBar(page, band, bands, sy);
      // The INACTIVE layer, dimmed (docs/174 OL-08): Word and ONLYOFFICE both
      // grey the body while a header or footer is open, so which layer the
      // keystrokes reach is visible at a glance. A veil between the two bands,
      // over the raster, never in it — dimming the whole sheet is what washed
      // out the band being edited before (see `.running-band`'s comment).
      const veil = document.createElement("div");
      veil.className = "running-veil";
      veil.style.top = `${Math.round(bands.top * sy)}px`;
      veil.style.bottom = `${Math.round(bands.bottom * sy)}px`;
      page.wrap.appendChild(veil);
    }
  }

  /* The band's label NAMES THE SECTION once a document has more than one, the
     way Word's does ("Header -Section 2-"), and says when the band is inherited
     ("Same as Previous"). Without it two pages showing different headers looked
     identical, so a user editing one section's header and seeing another
     section's pages not change had nothing on screen to explain why. The words
     are `bandLabel`'s, in `header_footer_commands.mjs`. */

  function clearRunningBands() {
    // The band BAR is not removed here: every repaint redraws the bands, and
    // removing the bar would blur the control the keyboard is on. It moves, and
    // it hides when the band closes (`exitRunningEdit`).
    for (const el of ed.pagesEl.querySelectorAll(".running-band, .running-veil")) el.remove();
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
    hideBandBar();
    hideBandTab();
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


  // ---- The open band's own chrome, and the band commands (HF-192, RM-23) ------
  // Google Docs' shape: while a header or footer is open, the band carries a
  // small bar of its own — here Link to previous, Different first page, an
  // Options menu (odd/even pages, the page-number positions, the settings
  // dialog, Remove) and Close. ONLYOFFICE puts the same controls on a contextual
  // Header & Footer tab (`view/HeaderFooterTab.js` L63-87); this chrome has no
  // contextual tab to spend and the ribbon must fit 1280px, so the controls go
  // where the band is, which is also where Word's and Docs' users look.
  //
  // ONE element, moved to the open page's sheet on every redraw rather than
  // rebuilt: rebuilding it would blur a control the keyboard is on every time an
  // edit repaints the page. Every command it runs is also an Insert-menu row and
  // a palette row (`surfaceRows`), so none is reachable from the bar only.
  const byId = (id) => document.getElementById(id);
  const bandBar = byId("runningBandBar");
  const bandLink = byId("runningBandLink");
  const bandLinkWrap = byId("runningBandLinkWrap");
  const bandFirst = byId("runningBandFirstPage");
  const bandOptions = byId("runningBandOptionsBtn");
  const bandMenu = byId("runningBandMenu");
  const bandOddEven = byId("runningBandOddEven");
  const bandRemove = { header: byId("runningBandRemoveHeader"), footer: byId("runningBandRemoveFooter") };
  const bandClose = byId("runningBandCloseBtn");
  /** The band the bar was last drawn for. */
  let barBand = null;

  const linkRow = (band) => BAND_COMMANDS.find((row) => row.band === band && row.action === "link");
  const removeRow = (band) => BAND_COMMANDS.find((row) => row.band === band && row.action === "remove");

  /** One band's Link to Previous state on `page`, from the engine. O(sections). */
  function linkState(band, page = runningEditPageOrView()) {
    if (!ed.doc || !page || typeof ed.doc.runningLinkState !== "function") return null;
    try {
      return parseLinkState(ed.doc.runningLinkState(band, page.pageNumber));
    } catch {
      return null;
    }
  }

  /** The families a copy could not carry, as one sentence, or null. */
  const LOSS_KEYS = {
    bookmark: "paste.loss.bookmark",
    comment: "paste.loss.comment",
    note: "paste.loss.note",
    fieldRange: "paste.loss.fieldRange",
    trackedMove: "paste.loss.trackedMove",
  };
  function copyLossMessage(kinds) {
    const known = (Array.isArray(kinds) ? kinds : []).filter((key) => LOSS_KEYS[key]);
    if (!known.length) return null;
    return t("headerFooter.copyLoss", { items: list(known.map((key) => t(LOSS_KEYS[key]))) });
  }

  /** Links one band of the page in view to the previous section's, or gives the
   *  section its own copy (`setRunningLinked`). The copy keeps the page number
   *  and the pictures (`docs/129` §2), and the caret, if it was in the band,
   *  follows into the copy. Says what it did, and refuses out loud on the first
   *  section, which has nothing to link to. */
  async function setLinked(band, linked, page = runningEditPageOrView()) {
    if (!ed.doc || !page) return false;
    const state = linkState(band, page);
    const reason = bandCommandReason(linkRow(band), state);
    if (reason) {
      ed.setStatus(reason, "error");
      return false;
    }
    if (state.linked === linked) return true;
    const focus = ed.selection?.focus;
    let loss = null;
    const applied = await ed.runEdit(
      () => {
        const result = ed.doc.setRunningLinked(band, page.pageNumber, linked, focus?.node ?? "", focus?.offset ?? 0);
        loss = copyLossMessage(result.pasteLoss);
        return result;
      },
      { gate: true, keepView: true },
    );
    if (!applied) return false;
    if (ed.runningEditBand) drawRunningBands(ed.runningEditBand);
    const done =
      band === "header"
        ? linked ? t("headerFooter.headerLinked") : t("headerFooter.headerUnlinked")
        : linked ? t("headerFooter.footerLinked") : t("headerFooter.footerUnlinked");
    ed.setStatus(loss ? `${done} ${loss}` : done);
    return true;
  }

  /** Flips one band's Link to Previous. */
  function toggleLinked(band, page = runningEditPageOrView()) {
    const state = linkState(band, page);
    return setLinked(band, state?.linked !== true, page);
  }

  /** Removes the band the page in view shows (`removeRunningContent`): from this
   *  section's pages and the sections linked to it, never from an earlier one.
   *  Leaves the band first, because the caret is about to point at nothing. */
  async function removeBand(band, page = runningEditPageOrView()) {
    if (!ed.doc || !page) return false;
    const reason = bandCommandReason(removeRow(band), linkState(band, page));
    if (reason) {
      ed.setStatus(reason, "error");
      return false;
    }
    if (ed.blockMutationInViewing()) return false;
    if (ed.runningEditBand) exitRunningEdit();
    const applied = await ed.runEdit(() => ed.doc.removeRunningContent(band, page.pageNumber, "", 0), {
      gate: true,
      keepView: true,
    });
    if (applied) ed.setStatus(band === "header" ? t("headerFooter.headerRemoved") : t("headerFooter.footerRemoved"));
    return applied;
  }

  /** Puts a page number left, centre or right in one band of the page in view
   *  (`insertRunningPageNumber`), creating the band when the page has none and
   *  keeping what it holds, then opens the band with the caret after the number
   *  — Word opens the header too, so the person sees what was added and can
   *  type beside it. The context is set BEFORE the edit for the reason
   *  `createRunningContent` gives. */
  async function insertPageNumber(band, align, page = runningEditPageOrView(), ofTotal = false) {
    if (!ed.doc || !page) return false;
    if (ed.blockMutationInViewing()) return false;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus(t("headerFooter.notTrackable"), "error");
      return false;
    }
    const previous = { band: ed.runningEditBand, page: ed.runningEditPage };
    setRunningContext(band, page);
    let result;
    try {
      // "Page X of Y": the raw pattern, split around its two placeholders, so a
      // language that puts words after the count ("第 1 页，共 3 页") gets them.
      const words = ofTotal ? splitPageOfTotal(t("pageNumber.ofTotalPattern")) : null;
      result = words
        ? ed.doc.insertRunningPageNumber(band, page.pageNumber, align, words.before, words.between, words.after)
        : ed.doc.insertRunningPageNumber(band, page.pageNumber, align);
    } catch (err) {
      if (previous.band) setRunningContext(previous.band, { pageNumber: previous.page });
      else exitRunningEdit();
      const message = String(err?.message ?? err);
      ed.setStatus(
        /table/.test(message) ? t("headerFooter.pageNumberEdgeTable") : t("headerFooter.bandChangeFailed", { message }),
        "error",
      );
      return false;
    }
    const node = result.node;
    const offset = result.offset;
    await ed.applyEditResult(result);
    enterRunningEdit(band, node, offset, page);
    ed.setStatus(band === "header" ? t("headerFooter.pageNumberInHeader") : t("headerFooter.pageNumberInFooter"));
    return true;
  }

  /** Paints the bar's controls from the engine for `band` on `page`. */
  function reflectBandBar(band, page) {
    if (!bandBar) return;
    const state = linkState(band, page);
    const reason = bandCommandReason(linkRow(band), state);
    if (bandLink) {
      bandLink.checked = state?.linked === true;
      bandLink.disabled = !!reason;
    }
    if (bandLinkWrap) {
      // The reason is the title of the WHOLE control, so a pointer on the words
      // gets it as well as one on the box; `aria-describedby` would need a
      // second, visible element for the same sentence.
      bandLinkWrap.title = reason || (band === "header" ? t("runningBand.linkHeaderTitle") : t("runningBand.linkFooterTitle"));
    }
    const variants = ed.headerFooterSettings?.variantState?.(page) ?? {};
    if (bandFirst) bandFirst.checked = variants.firstPage === true;
    bandOddEven?.setAttribute("aria-checked", String(variants.evenOdd === true));
    const removeReason = bandCommandReason(removeRow(band), state);
    for (const [which, item] of Object.entries(bandRemove)) {
      if (!item) continue;
      item.hidden = which !== band;
      // `aria-disabled`, not `disabled`: a menu row the keyboard cannot land on
      // cannot tell anyone why it is unavailable (SKILL §10).
      if (removeReason) item.setAttribute("aria-disabled", "true");
      else item.removeAttribute("aria-disabled");
      item.title = removeReason;
    }
    reflectBandTab(band, state, reason, removeReason, variants);
  }

  /** Puts the bar on the open page's sheet, just inside the band boundary. */
  function placeBandBar(page, band, bands, sy) {
    if (!bandBar || !page?.wrap) return;
    barBand = band;
    if (bandBar.parentElement !== page.wrap) page.wrap.appendChild(bandBar);
    bandBar.classList.toggle("is-footer", band === "footer");
    const edge = Math.round((band === "header" ? bands.top : bands.bottom) * sy);
    bandBar.style.top = band === "header" ? `${edge + 4}px` : "auto";
    bandBar.style.bottom = band === "footer" ? `${edge + 4}px` : "auto";
    reflectBandBar(band, page);
    bandBar.hidden = false;
  }

  function hideBandBar() {
    if (!bandBar) return;
    bandBar.hidden = true;
    barBand = null;
  }

  /** Runs `action` and gives the keyboard back to the band, which is where the
   *  person was typing. */
  async function fromBar(action) {
    await action();
    if (ed.runningEditBand) ed.focusEditorSurface();
  }

  if (bandBar) {
    // The bar sits inside the sheet, whose pointer handlers would otherwise read
    // a press on a control as a click in the page and move the caret or leave
    // the band.
    for (const type of ["pointerdown", "mousedown", "dblclick"]) {
      bandBar.addEventListener(type, (event) => event.stopPropagation());
    }
    // A toolbar is one Tab stop; the arrows move inside it (WAI-ARIA toolbar).
    bandBar.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      const items = [...bandBar.querySelectorAll("input, button")].filter((item) => !item.disabled);
      const at = items.indexOf(document.activeElement);
      if (at < 0) return;
      event.preventDefault();
      const step = event.key === "ArrowRight" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
    });
    bandLink?.addEventListener("change", () => {
      const band = barBand ?? ed.runningEditBand;
      if (band) void fromBar(() => setLinked(band, bandLink.checked));
    });
    bandFirst?.addEventListener("change", () => {
      void fromBar(async () => {
        await ed.headerFooterSettings?.toggleVariant?.("firstPage");
        if (ed.runningEditBand) drawRunningBands(ed.runningEditBand);
      });
    });
    if (bandClose) onButton(bandClose, () => exitRunningEdit());
  }
  if (bandOptions && bandMenu) {
    const popover = registerPopover(bandOptions, bandMenu, () => {
      if (barBand) reflectBandBar(barBand, runningEditPageOrView());
    });
    const choose = (item, action) =>
      onButton(item, () => {
        if (item.getAttribute("aria-disabled") === "true") {
          if (item.title) ed.setStatus(item.title, "error");
          return;
        }
        popover.close();
        void fromBar(action);
      });
    if (bandOddEven) {
      choose(bandOddEven, async () => {
        await ed.headerFooterSettings?.toggleVariant?.("evenOdd");
        if (ed.runningEditBand) drawRunningBands(ed.runningEditBand);
      });
    }
    for (const item of bandMenu.querySelectorAll("[data-page-number-align]")) {
      choose(item, () => insertPageNumber(barBand ?? ed.runningEditBand ?? "footer", item.dataset.pageNumberAlign));
    }
    const ofTotal = byId("runningBandPageOfTotal");
    if (ofTotal) {
      choose(ofTotal, () =>
        insertPageNumber(barBand ?? ed.runningEditBand ?? "footer", "center", undefined, true),
      );
    }
    const here = byId("runningBandPageNumberHere");
    if (here) choose(here, () => ed.insertFieldAtCaret("page"));
    const settings = byId("runningBandSettings");
    if (settings) choose(settings, () => ed.headerFooterSettings?.open?.(true));
    for (const [band, item] of Object.entries(bandRemove)) {
      if (item) choose(item, () => removeBand(band));
    }
  }

  // ---- The contextual Header & Footer tab (docs/174 OL-08) --------------------
  // ONLYOFFICE's HeaderFooterTab and Word's Header & Footer tab: enabled, and
  // selected, only while a band is open; the tab that was showing comes back
  // when the band closes. Every control runs the SAME function as the band bar,
  // the Insert menu and the palette — this is a face, not a second path.
  const hfTab = byId("tabHeaderFooter");
  const hf = {
    header: byId("hfTabHeaderBtn"),
    footer: byId("hfTabFooterBtn"),
    remove: byId("hfTabRemoveBtn"),
    pageNumber: byId("hfTabPageNumberBtn"),
    pageNumberMenu: byId("hfPageNumberMenu"),
    date: byId("hfTabDateBtn"),
    field: byId("hfTabFieldBtn"),
    picture: byId("hfTabPictureBtn"),
    position: byId("hfTabPositionBtn"),
    oddEven: byId("hfTabOddEvenBtn"),
    firstPage: byId("hfTabFirstPageBtn"),
    link: byId("hfTabLinkBtn"),
    close: byId("hfTabCloseBtn"),
  };
  /** The tab showing when the band opened, restored when it closes. */
  let tabBeforeBand = null;

  function selectedRibbonTab() {
    return document.querySelector(".ribbon-tab[aria-selected='true']")?.dataset.tab ?? "home";
  }

  function showBandTab() {
    if (!hfTab) return;
    if (hfTab.disabled) {
      tabBeforeBand = selectedRibbonTab();
      hfTab.disabled = false;
      hfTab.removeAttribute("title");
    }
    if (selectedRibbonTab() !== "headerFooter") ed.selectRibbonTab?.("headerFooter");
  }

  function hideBandTab() {
    if (!hfTab || hfTab.disabled) return;
    hfTab.disabled = true;
    hfTab.title = t("documentChrome.headerFooter.title");
    if (hfTab.getAttribute("aria-selected") === "true") ed.selectRibbonTab?.(tabBeforeBand ?? "home");
    tabBeforeBand = null;
  }

  /** Paints the tab's switches and its two band-specific controls. */
  function reflectBandTab(band, state, linkReason, removeReason, variants) {
    if (!hf.link) return;
    hf.link.dataset.command = `layout.${band}.linkToPrevious`;
    hf.link.setAttribute("aria-pressed", String(state?.linked === true));
    hf.link.disabled = !!linkReason;
    hf.link.title = linkReason || (band === "header" ? t("runningBand.linkHeaderTitle") : t("runningBand.linkFooterTitle"));
    if (hf.remove) {
      hf.remove.dataset.command = `layout.${band}.remove`;
      hf.remove.disabled = !!removeReason;
      hf.remove.title = removeReason || (band === "header" ? t("headerFooter.removeHeader") : t("headerFooter.removeFooter"));
    }
    hf.firstPage?.setAttribute("aria-pressed", String(variants.firstPage === true));
    hf.oddEven?.setAttribute("aria-pressed", String(variants.evenOdd === true));
    hf.header?.setAttribute("aria-pressed", String(band === "header"));
    hf.footer?.setAttribute("aria-pressed", String(band === "footer"));
  }

  /** Runs `action` from the tab and gives the keyboard back to the band. */
  const fromTab = (action) => () => void fromBar(action);
  const redraw = () => {
    if (ed.runningEditBand) drawRunningBands(ed.runningEditBand);
  };
  if (hf.header) onButton(hf.header, fromTab(() => editRunningContent("header", runningEditPageOrView())));
  if (hf.footer) onButton(hf.footer, fromTab(() => editRunningContent("footer", runningEditPageOrView())));
  if (hf.remove) onButton(hf.remove, fromTab(() => removeBand(ed.runningEditBand ?? "header")));
  if (hf.date) onButton(hf.date, fromTab(() => ed.insertFieldAtCaret("date")));
  if (hf.field) onButton(hf.field, () => ed.openFieldDialog());
  if (hf.picture) onButton(hf.picture, () => ed.insertImageFromFile());
  if (hf.position) onButton(hf.position, () => ed.headerFooterSettings?.open?.(true));
  if (hf.oddEven) {
    onButton(hf.oddEven, fromTab(async () => {
      await ed.headerFooterSettings?.toggleVariant?.("evenOdd");
      redraw();
    }));
  }
  if (hf.firstPage) {
    onButton(hf.firstPage, fromTab(async () => {
      await ed.headerFooterSettings?.toggleVariant?.("firstPage");
      redraw();
    }));
  }
  if (hf.link) onButton(hf.link, fromTab(() => toggleLinked(ed.runningEditBand ?? "header")));
  if (hf.close) onButton(hf.close, () => exitRunningEdit());
  if (hf.pageNumber && hf.pageNumberMenu) {
    // Rows built at boot WITHOUT text (a catalogue is not installed yet — SKILL
    // §11) and labelled each time the menu opens.
    const rows = [
      ...PAGE_NUMBER_POSITIONS.map((row) => ({ ...row, run: () => insertPageNumber(row.band, row.align) })),
      {
        ...PAGE_OF_TOTAL,
        run: () => insertPageNumber(PAGE_OF_TOTAL.band, PAGE_OF_TOTAL.align, undefined, true),
      },
      { command: "insert.field.page", labelKey: "pageNumber.atCursor", run: () => ed.insertFieldAtCaret("page") },
    ];
    const items = rows.map((row) => {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "menu-item";
      item.setAttribute("role", "menuitem");
      item.dataset.command = row.command;
      // English until the menu first opens, so the row is never nameless.
      item.textContent = EN_STRINGS[row.labelKey] ?? "";
      hf.pageNumberMenu.appendChild(item);
      return { item, row };
    });
    const menu = registerPopover(hf.pageNumber, hf.pageNumberMenu, () => {
      for (const { item, row } of items) item.textContent = t(row.labelKey);
    });
    for (const { item, row } of items) {
      onButton(item, () => {
        menu.close();
        void fromBar(row.run);
      });
    }
  }

  /** The palette and Insert-menu rows of every band command, shaped for
   *  `LAYOUT_SURFACE` (no ribbon face: the bar above is the in-place one).
   *
   *  `label` is a GETTER, read only when the palette or a menu builds its rows
   *  (after the catalogue is installed): a link row is a switch with its state
   *  ("Header: linked to previous section"). A row that cannot run on this page
   *  — Link to Previous on the first section, Remove where there is no band —
   *  refuses with the reason; the in-place faces show it disabled. */
  function surfaceRows() {
    return [
      ...BAND_COMMANDS.map((row) => ({
        command: row.command,
        get label() {
          return bandCommandLabel(row, linkState(row.band));
        },
        kw:
          row.action === "link"
            ? `link to previous same as previous section ${row.band} unlink inherit`
            : `remove delete ${row.band} running section`,
        buttons: () => [],
        // "doc", not a getter over the page's state: the toolbar sweep reads
        // `requires` on every keystroke, and asking the engine and measuring the
        // pages there would make typing O(sections + sheets). The band bar and
        // the dialog show the disabled state with its reason; from the palette
        // and the menu the command refuses OUT LOUD with the same sentence
        // (`setLinked`, `removeBand`).
        requires: "doc",
        run: () => void (row.action === "link" ? toggleLinked(row.band) : removeBand(row.band)),
      })),
      ...PAGE_NUMBER_POSITIONS.map((row) => ({
        command: row.command,
        get label() {
          return t(row.labelKey);
        },
        kw: `page number numbering ${row.band} ${row.align} position insert field`,
        buttons: () => [],
        requires: "doc",
        run: () => void insertPageNumber(row.band, row.align),
      })),
      {
        command: PAGE_OF_TOTAL.command,
        get label() {
          return t(PAGE_OF_TOTAL.labelKey);
        },
        kw: "page x of y number count total numpages footer insert field",
        buttons: () => [],
        requires: "doc",
        run: () => void insertPageNumber(PAGE_OF_TOTAL.band, PAGE_OF_TOTAL.align, undefined, true),
      },
    ];
  }

  // On the editor context, so the dialog and the surface tables reach the same
  // functions without a new line in `main.js`, which sits at its ratchet.
  ed.headerFooterBand = { insertPageNumber, linkState, removeBand, setLinked, surfaceRows, toggleLinked };

  return {
    blockedFromPageSetup, deleteSelectedObject, drawRunningBands, editRunningContent,
    exitRunningEdit, hideRunningMarker, insertNote, pageInView, runningBandAt,
    runningEditPageOrView, setObjectWrap, setRunningContext, updateRunningMarker,
  };
}
