// The deck viewer's DOM wiring: the thin layer between `slides.mjs` and the page.
//
// Kept separate from `slides.mjs` for the reason that module's own header gives
// about `indexForKey`: the decisions are testable without a browser, and only
// the wiring needs one. Everything here is event handlers and element writes;
// no slide arithmetic, no unit conversion, no rendering policy.

import { setCatalogue, t } from "./i18n.mjs";
import { EN_STRINGS } from "./en_strings.mjs";
// The same locale seam the public site pages use. The editor's
// `startLocalisation` wants a picker, a settings object and a popover registry —
// editor chrome this page does not have — while this one takes an optional
// `select` and nothing else, which is exactly the shape of a viewer.
import { startSiteLocalisation } from "./site_locale.mjs";
import { createViewer } from "./slides.mjs";
import { renderSlideMirror } from "./slides_mirror.mjs";
// The EDITOR's own toolbar and menu renderers, reused rather than rebuilt. Both
// take their registry, their click binder and their popover manager as
// dependencies — `module_seams.test.mjs` forbids either from importing
// `main.js` — so supplying those is the whole of what a second page owes them.
import { createCompactToolbar } from "./compact_toolbar.mjs";
import { createMenuBar } from "./command_menu.mjs";
import { SLIDE_MENU_SECTIONS, SLIDE_TOOLBAR, createSlideCommands } from "./slide_commands.mjs";
import { configurePopovers, onButton, registerPopover } from "./popover_manager.mjs";
// The slide sorter is the editor's PAGE navigator: the same card, the same
// windowing arithmetic, the same "which one is current" reflection. Only the
// bitmap behind each card differs, which is the one thing a deck owns.
import { pagesPanelRange, reflectPagesPanelSelection } from "./pages_panel.mjs";
// THE FONTS. `casual-pres-wasm` is built with `web-host-fonts`, which drops the
// embedded face blobs from the bundle — so without this the shaper has an empty
// registry and slide text is shaped against nothing. The editor has provisioned
// its faces from this module since the font work; this page shipped with the
// feature and without the provisioning, which is not a fidelity gap in the
// engine so much as the engine being handed nothing to draw with.
import { NAMED_WEB_FONT_FACES, fetchFontBytes, packFontBytes } from "./web_fonts.mjs";

/// The media type a `.pptx` is served and saved as.
const PPTX_MEDIA_TYPE =
  "application/vnd.openxmlformats-officedocument.presentationml.presentation";

/// The elements the page is built from, resolved once.
///
/// A missing element is a build error rather than a silent no-op: the page and
/// this module ship together, so a null here means the markup and the script
/// disagree and every later handler would fail in a less obvious place.
function resolveElements() {
  const ids = [
    "deckTitle",
    "slidesFile",
    "slidesSave",
    "slidesPosition",
    "slidesSorter",
    "slideStage",
    "slideSheet",
    "slideCanvas",
    "slidesEmpty",
    "slidesFidelity",
    "slidesSorterPanel",
    "slidesFidelityPanel",
    "slidesFidelityClose",
    "slidesFidelitySummary",
    "slidesFidelityList",
    "slidesError",
    "slideTextOwn",
    "slideTextInherited",
    "railFidelity",
    "railSlides",
    "status",
    "appMenuBar",
    "appMenuPopover",
    "compactToolbar",
    "slidesZoom",
  ];
  const found = {};
  const missing = [];
  for (const id of ids) {
    const element = document.getElementById(id);
    if (element) found[id] = element;
    else missing.push(id);
  }
  if (missing.length) throw new Error(`slides.html is missing: ${missing.join(", ")}`);
  return found;
}

/// Boots the page against an initialized facade.
///
/// Exported and taking its dependencies so a test can drive the whole wiring
/// with a stub facade and a document fragment, which is the only way to assert
/// the handlers without a browser engine.
export function bootViewer({ facade, elements, devicePixelRatio = 1 }) {
  const viewer = createViewer({ facade, elements, devicePixelRatio });

  /// Shows a refusal in BOTH halves the editor uses: the status bar a reader
  /// sees, and the body-level live region assistive technology hears.
  ///
  /// Two elements and not one, for the reason `109` UX-017 records on the
  /// document side: the visible status strip sheds indicators as the window
  /// narrows, and a `display: none` subtree is not in the accessibility tree — so
  /// a live region inside it goes silent at exactly the width where it matters.
  function showError(message) {
    const text = t("slides.openFailed", { reason: message });
    elements.status.textContent = text;
    elements.slidesError.textContent = text;
  }

  function clearError() {
    elements.slidesError.textContent = "";
  }

  /// The width a slide may occupy, from the stage's own box.
  ///
  /// Read from layout rather than from the window: the stage sits beside the
  /// sorter, so the window's width is not the slide's and a deck rendered to it
  /// would overflow by the strip's width at every zoom.
  /// The desk's breathing room either side of the sheet, in CSS pixels.
  ///
  /// The viewport declares no horizontal padding — it scrolls, and the editor's
  /// own sheets are sized by the document rather than by the window — so a slide
  /// asked for the viewport's full width would touch both edges and lose the
  /// shadow that makes it read as a sheet at all.
  const DESK_MARGIN_PX = 48;

  function availableWidth() {
    const box = elements.slideStage.getBoundingClientRect?.();
    const width = (box?.width ?? 0) - DESK_MARGIN_PX;
    return width > 0 ? width : 960;
  }

  function renderPosition() {
    const total = viewer.slideCount();
    elements.slidesPosition.textContent =
      total === 0
        ? ""
        : t("slides.position", { current: viewer.currentIndex() + 1, total });
  }

  function renderSorter() {
    const slides = viewer.slides();
    elements.slidesSorter.replaceChildren();
    if (slides.length === 0) return;
    // WINDOWED, through the editor's own arithmetic. A 300-slide deck rendered
    // whole is 300 canvases rasterised on open, which is the stall the document
    // navigator already solved — `pagesPanelRange` centres a fixed window on the
    // current item and `renderPagesPanel` says so on screen rather than showing
    // a slice as if it were the deck.
    const range = pagesPanelRange(slides.length, viewer.currentIndex() + 1);
    if (range.start > 1 || range.end < slides.length) {
      const note = document.createElement("p");
      note.className = "panel-window-note";
      note.textContent = t("slides.window", {
        start: range.start,
        end: range.end,
        total: slides.length,
      });
      elements.slidesSorter.append(note);
    }
    for (let n = range.start; n <= range.end; n += 1) {
      const slide = slides[n - 1];
      // The editor's card, element for element, so `reflectPagesPanelSelection`
      // can mark it and `.page-thumb`'s rules style it.
      const card = document.createElement("button");
      card.type = "button";
      card.className = "page-thumb";
      card.dataset.page = String(n);
      card.title = slide.name;
      card.setAttribute("aria-label", slide.name);
      const box = document.createElement("span");
      box.className = "page-thumb-box";
      box.style.aspectRatio = `${viewer.surfaceWidthEmu()} / ${viewer.surfaceHeightEmu()}`;
      const thumb = document.createElement("canvas");
      thumb.className = "page-thumb-canvas";
      box.append(thumb);
      const num = document.createElement("span");
      num.className = "page-thumb-num";
      num.textContent = slide.name;
      card.append(box, num);
      if (slide.hidden) {
        // A hidden slide is still in the deck — retained, saved and sortable —
        // so it is marked rather than omitted. Omitting it would leave an author
        // unable to see what their own file contains.
        const badge = document.createElement("span");
        badge.className = "slide-thumb-hidden";
        badge.textContent = t("slides.hiddenBadge");
        card.append(badge);
      }
      onButton(card, () => {
        viewer.goTo(slide.index);
        paint();
      });
      elements.slidesSorter.append(card);
      viewer.paintThumbnail(thumb, slide.index);
    }
    reflectPagesPanelSelection(elements.slidesSorter, viewer.currentIndex() + 1);
  }

  /// Repaints the thumbnails already on screen, without rebuilding them.
  ///
  /// `renderSorter` calls `replaceChildren`, so using it here would empty the
  /// sorter and refill it — a visible flash on every open, and a window in which
  /// the panel genuinely holds nothing. Measured: a browser spec reading the
  /// sorter's labels during that window got an empty list. The cards are already
  /// correct; only their pixels are stale, so only their pixels are redrawn.
  function repaintThumbnails() {
    const cards = elements.slidesSorter.querySelectorAll(".page-thumb");
    for (const card of cards) {
      const index = Number(card.dataset.page) - 1;
      const canvas = card.querySelector(".page-thumb-canvas");
      if (canvas && Number.isInteger(index)) viewer.paintThumbnail(canvas, index);
    }
  }

  function renderFidelity() {
    const findings = viewer.findings();
    const total = findings.reduce((sum, finding) => sum + (finding.occurrences ?? 1), 0);
    // The count goes in the header chip the editor already uses for its own
    // import/export findings — a neutral status chip, never an alert, because
    // this is the one claim a converter cannot make and dressing it as an error
    // would read as a failure rather than as the advantage it is.
    elements.slidesFidelity.hidden = false;
    elements.slidesFidelity.textContent =
      findings.length === 0 ? t("slides.fidelityClean") : t("slides.fidelityLossy", { count: total });
    elements.railFidelity.disabled = findings.length === 0;
    elements.slidesFidelitySummary.textContent = elements.slidesFidelity.textContent;
    elements.slidesFidelityList.replaceChildren();
    for (const finding of findings) {
      const item = document.createElement("li");
      // The feature name and the part it was charged to, which is what makes a
      // deck's report actionable: a gradient lost on slide 7 is a different fact
      // from one lost in the master. Not translated — these are OOXML element
      // names, and translating `a:gradFill` would make the report unsearchable.
      item.textContent = finding.part ? `${finding.feature} — ${finding.part}` : finding.feature;
      elements.slidesFidelityList.append(item);
    }
  }

  /// Shows or hides one panel, through the rail, as the editor's panels are: the
  /// rail button owns the pressed state and the panel owns `hidden`.
  ///
  /// ONE function for both panels and all three surfaces that drive them — the
  /// rail tile, the View menu row and the toolbar toggle — because three
  /// listeners setting `hidden` independently is the drift `105` UX-004 records,
  /// in miniature.
  function togglePanel(name, show) {
    const panel = name === "slides" ? elements.slidesSorterPanel : elements.slidesFidelityPanel;
    const tile = name === "slides" ? elements.railSlides : elements.railFidelity;
    panel.hidden = !show;
    tile.setAttribute("aria-pressed", show ? "true" : "false");
    // The stage's width changed, so the slide's fit did too.
    paint();
  }

  const toggleFidelity = (show) => togglePanel("fidelity", show);

  function paint() {
    const painted = viewer.paint(elements.slideCanvas, availableWidth());
    elements.slidesEmpty.hidden = painted;
    // The SHEET is what is shown or hidden, not the canvas inside it: hiding only
    // the canvas would leave an empty paper rectangle with a shadow on the desk.
    elements.slideSheet.hidden = !painted;
    // The off-screen structural mirror, rebuilt with the pixels and not after
    // them. A canvas exposes no text, so this is the ONLY thing a screen reader
    // can read on this page besides the sorter — and it has to move with the
    // slide, or a reader paging through the deck hears slide 1 forever.
    renderSlideMirror({
      own: elements.slideTextOwn,
      inherited: elements.slideTextInherited,
      outline: viewer.slideText(),
    });
    renderPosition();
    elements.slidesZoom.textContent = viewer.slideCount()
      ? t("slides.zoomReadout", { percent: viewer.zoomPercent() })
      : "";
    refreshCommandSurfaces();
    reflectPagesPanelSelection(elements.slidesSorter, viewer.currentIndex() + 1);
  }

  function openBytes(bytes) {
    clearError();
    const result = viewer.open(bytes);
    if (!result.ok) {
      elements.slidesSave.disabled = true;
      elements.slidesEmpty.hidden = false;
      elements.slideSheet.hidden = true;
      elements.deckTitle.value = "";
      showError(result.message);
      return false;
    }
    elements.slidesSave.disabled = false;
    elements.status.textContent = "";
    renderSorter();
    paint();
    renderFidelity();
    // The faces, AFTER the first paint rather than before it. A deck on screen
    // in whatever the engine can still reach within milliseconds, then re-drawn
    // in the faces it actually names, beats a blank stage while nine megabytes
    // download — the same order the editor provisions in, for the same reason.
    void provisionFonts(viewer).then((count) => {
      if (count > 0) {
        repaintThumbnails();
        paint();
      }
    });
    return true;
  }

  // ---- The command registry, and the two surfaces that draw from it --------
  //
  // One registry, two renderers, both the editor's own. A command's label, its
  // enablement and its pressed state come from here whichever surface shows it,
  // which is the whole reason `105` UX-004/UX-005 are not repeated on this page.
  const editorCommands = createSlideCommands({
    viewer,
    t,
    actions: {
      openPicker: () => elements.slidesFile.click(),
      save: () => saveDeck(),
      repaint: () => paint(),
      panelShown: (name) =>
        name === "slides"
          ? !elements.slidesSorterPanel.hidden
          : !elements.slidesFidelityPanel.hidden,
      togglePanel: (name) => {
        if (name === "slides") togglePanel("slides", elements.slidesSorterPanel.hidden);
        else togglePanel("fidelity", elements.slidesFidelityPanel.hidden);
      },
    },
  });

  // The popover manager and the click binder are `popover_manager.mjs`'s — the
  // product's ONE implementation of "a small menu hangs off a button until you
  // press elsewhere", with the pointerdown dismissal, the Escape handling, the
  // `aria-expanded` bookkeeping and the mousedown-preventDefault that stops a
  // toolbar press from stealing the selection. An earlier revision of this file
  // wrote a second, worse copy of all of that.
  //
  // `documentReady` gates DOCUMENT-scoped popovers on there being something to
  // act on. This page's chrome is live with or without a deck — every command
  // already says why it is disabled — so the gate is open and the honest
  // statement of that is here rather than a `needsDocument: false` at each site.
  configurePopovers({ documentReady: () => true });

  // THE SHELL IS VISIBLE FROM THE START, which is a deliberate departure from
  // the editor. `style.css` gates the rail, the toolbar and the status bar on
  // `body.doc-loaded`, because a document editor with nothing open genuinely has
  // no chrome to show. A presentation VIEWER is not in that position: both
  // reference products show their toolbar, their thumbnail rail and their status
  // bar with an empty presentation, and every command here already carries a
  // `disabledReason` saying why it cannot run yet. A chrome that appears only
  // after you succeed at the one thing you came to do tells you nothing about
  // what the page is for — which is what a reader reported on this page.
  document.body.classList.add("doc-loaded");

  const toolbar = createCompactToolbar({
    host: elements.compactToolbar,
    editorCommands,
    onButton,
    registerPopover,
    // Empty, and that is correct rather than a stub: these are the editor's
    // enablement lists for controls gated on a RUN or a PARAGRAPH selection, and
    // this page has neither — nothing it offers depends on a caret.
    runControls: [],
    paraControls: [],
    formatToggleCache: new Map(),
    table: SLIDE_TOOLBAR,
    // One roster. The editor passes a second for phones because its desktop bar
    // is thirteen groups over a fold; this one is four, which a phone fits.
    phoneTable: SLIDE_TOOLBAR,
  });

  /// Labels the markup elements whose text the REGISTRY owns.
  ///
  /// Four strings, each with exactly one home. They used to be markup-declared,
  /// which put them out of `t()`'s reach on a page that loads no English
  /// catalogue — the toolbar rendered buttons reading "slides.chooseFile". Now
  /// the registry owns them and the markup asks for them, so the open button in
  /// the header and the File ▸ Open row can never say different things.
  function labelFromRegistry() {
    const by = new Map(editorCommands().map((command) => [command.id, command.label]));
    elements.slidesFile.previousElementSibling?.replaceChildren(
      document.createTextNode(by.get("file.open") ?? ""),
    );
    elements.slidesSave.textContent = by.get("file.save") ?? "";
    for (const [id, label] of [
      ["railSlides", t("slides.sorter")],
      ["railFidelity", t("slides.fidelityPanel")],
    ]) {
      const tile = elements[id];
      tile.setAttribute("aria-label", label);
      tile.title = label;
      tile.querySelector("span:not(.ms)")?.replaceChildren(document.createTextNode(label));
    }
    elements.slidesSorterPanel.querySelector(".panel-title").textContent = t("slides.sorter");
    elements.slidesSorterPanel.setAttribute("aria-label", t("slides.sorter"));
    elements.slidesFidelityPanel.querySelector(".panel-title").textContent =
      t("slides.fidelityPanel");
    elements.slidesFidelityPanel.setAttribute("aria-label", t("slides.fidelityPanel"));
  }

  const menuBar = createMenuBar({
    bar: elements.appMenuBar,
    popover: elements.appMenuPopover,
    sectionsFor: (name) => SLIDE_MENU_SECTIONS[name] ?? [],
    registry: () => new Map(editorCommands().map((command) => [command.id, command])),
    formatShortcut: () => "",
  });

  /// Re-renders every surface that reflects command state.
  ///
  /// Called after anything that can change an enablement — opening a deck,
  /// moving, zooming, toggling a panel — because the registry is resolved live
  /// and a bar rendered once would hold a stale `enabled` forever. The editor
  /// rebuilds its compact bar for exactly this reason.
  function refreshCommandSurfaces() {
    toolbar.render?.();
    menuBar.close?.({ focus: false });
  }

  elements.slidesFile.addEventListener("change", async (event) => {
    const file = event.target?.files?.[0];
    if (!file) return;
    // The deck's name in the title row, from the file: a presentation part
    // carries no title this build reads, so the file name is the only honest
    // answer and inventing one would be worse than showing what was opened.
    elements.deckTitle.value = file.name;
    openBytes(new Uint8Array(await file.arrayBuffer()));
  });

  // The rail owns the panel, as it does in the editor: one button, one panel,
  // one pressed state — and it routes through the same `togglePanel` the menu
  // row and the toolbar toggle do, so three surfaces cannot disagree.
  elements.railSlides.addEventListener("click", () => {
    togglePanel("slides", elements.slidesSorterPanel.hidden);
  });
  elements.railFidelity.addEventListener("click", () => {
    togglePanel("fidelity", elements.slidesFidelityPanel.hidden);
  });
  elements.slidesFidelityClose.addEventListener("click", () => toggleFidelity(false));

  /// Saves the deck, carrying every part the engine does not model through.
  ///
  /// Here rather than inline on the button, because `file.save` in the registry
  /// runs it too — one action, two entry points, and the button is just the one
  /// with a position on screen.
  function saveDeck() {
    const bytes = viewer.save();
    if (!bytes) return;
    const url = URL.createObjectURL(new Blob([bytes], { type: PPTX_MEDIA_TYPE }));
    const link = document.createElement("a");
    link.href = url;
    link.download = elements.deckTitle.value || "presentation.pptx";
    link.click();
    // Revoked on the next frame rather than immediately: a synchronous revoke
    // races the browser's own fetch of the object URL and the download arrives
    // empty on some builds.
    requestAnimationFrame(() => URL.revokeObjectURL(url));
  }

  elements.slidesSave.addEventListener("click", () => saveDeck());

  labelFromRegistry();
  // Rendered once at boot so the bar and the status strip exist before a deck
  // does, with every command disabled and saying why.
  paint();

  // Arrow keys on the stage. The mapping lives in `slides.mjs` so it is testable
  // without a browser; this only decides whether to consume the event.
  elements.slideStage.addEventListener("keydown", (event) => {
    const next = viewer.indexForKey(event.key);
    if (next === null) return;
    event.preventDefault();
    viewer.goTo(next);
    paint();
  });

  return { openBytes, paint, renderSorter, renderFidelity, toggleFidelity, viewer };
}

/// Fetches the named web faces and hands them to the engine.
///
/// Failures do NOT block opening a deck, and that is deliberate: a face that
/// will not download leaves the deck rendered in whatever the engine can still
/// reach, which is strictly better than refusing to show it. Each failure is
/// named rather than swallowed, because "the text looks wrong" and "Lato did
/// not load" are the same event and only one of them is actionable.
///
/// The JS byte cache is released afterwards — the engine holds the authoritative
/// copy now, and double-holding roughly nine megabytes for the tab's life buys
/// nothing.
async function provisionFonts(viewer) {
  const results = await Promise.allSettled(
    NAMED_WEB_FONT_FACES.map((face) => fetchFontBytes(face.url, new Map())),
  );
  const bytes = results
    .filter((result) => result.status === "fulfilled")
    .map((result) => result.value);
  if (bytes.length > 0) {
    const packed = packFontBytes(bytes);
    viewer.registerFonts(packed.bytes, packed.lengths);
  }
  for (const [index, result] of results.entries()) {
    if (result.status === "rejected") {
      const face = NAMED_WEB_FONT_FACES[index];
      console.warn(`font ${face.family} (${face.url}) failed:`, result.reason);
    }
  }
  return bytes.length;
}

/// Starts the page: loads the facade, applies translations, wires the DOM.
async function start() {
  const elements = resolveElements();
  // THE ENGLISH SCRIPT STRINGS GO IN FIRST, and synchronously.
  //
  // `startSiteLocalisation` deliberately loads NO catalogue for English — a site
  // page's English is the markup it was authored with, so fetching one would be a
  // request for strings the page already has. This page is the first outside the
  // editor with SCRIPT-side strings too, and a script has no markup to be
  // authored in, so without this a lookup returns the key itself. Measured, not
  // reasoned: the position readout rendered the literal "slides.position" until
  // this line existed.
  //
  // (That sentence is phrased around the call rather than showing it, because
  // `i18n_params.test.mjs`'s scanner reads comments as well as code and read a
  // `t(…)` written here as a real call site with no parameters. Worth knowing
  // before writing the next one.)
  //
  // `locale_boot.startLocalisation` seeds the same way for the same reason; this
  // is that one line rather than the editor's whole picker-and-popover contract.
  setCatalogue("en", EN_STRINGS);
  // Localise BEFORE the engine request, not after: the catalogue is a small JSON
  // fetch and the engine is megabytes of WebAssembly, so awaiting the engine
  // first would leave the chrome in English for the whole download on every
  // non-English locale.
  await startSiteLocalisation({ select: document.getElementById("slidesLanguage") });
  const module = await import("../pkg/casual_pres_wasm.js");
  await module.default();
  bootViewer({
    facade: module,
    elements,
    devicePixelRatio: globalThis.devicePixelRatio || 1,
  });
}

// Guarded so importing this module in a test does not try to fetch the engine.
if (typeof document !== "undefined" && document.getElementById("slideCanvas")) {
  start().catch((error) => {
    const target = document.getElementById("slidesError");
    if (target) {
      target.hidden = false;
      target.textContent = String(error?.message ?? error);
    }
  });
}
