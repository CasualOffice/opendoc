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

/// The elements the page is built from, resolved once.
///
/// A missing element is a build error rather than a silent no-op: the page and
/// this module ship together, so a null here means the markup and the script
/// disagree and every later handler would fail in a less obvious place.
function resolveElements() {
  const ids = [
    "slidesFile",
    "slidesSave",
    "slidesPosition",
    "slidesSorter",
    "slideStage",
    "slideCanvas",
    "slidesEmpty",
    "slidesFidelity",
    "slidesFidelitySummary",
    "slidesFidelityDetails",
    "slidesFidelityList",
    "slidesError",
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

  function showError(message) {
    elements.slidesError.hidden = false;
    elements.slidesError.textContent = t("slides.openFailed", { reason: message });
  }

  function clearError() {
    elements.slidesError.hidden = true;
    elements.slidesError.textContent = "";
  }

  /// The width a slide may occupy, from the stage's own box.
  ///
  /// Read from layout rather than from the window: the stage sits beside the
  /// sorter, so the window's width is not the slide's and a deck rendered to it
  /// would overflow by the strip's width at every zoom.
  function availableWidth() {
    const box = elements.slideStage.getBoundingClientRect?.();
    const width = box?.width ?? 0;
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
    for (const slide of slides) {
      const item = document.createElement("li");
      item.className = "slides-strip-item";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "slides-thumb";
      // `aria-current` rather than a class alone: the current slide is a state a
      // screen reader must hear, and styling it only visually leaves that reader
      // with no idea which of forty thumbnails is showing.
      if (slide.index === viewer.currentIndex()) button.setAttribute("aria-current", "true");
      const thumb = document.createElement("canvas");
      thumb.className = "slides-thumb-canvas";
      button.append(thumb);
      const label = document.createElement("span");
      label.className = "slides-thumb-label";
      label.textContent = slide.name;
      button.append(label);
      if (slide.hidden) {
        // A hidden slide is still in the deck — retained, saved and sortable —
        // so it is marked rather than omitted. Omitting it would leave an author
        // unable to see what their own file contains.
        const badge = document.createElement("span");
        badge.className = "slides-thumb-hidden";
        badge.textContent = t("slides.hiddenBadge");
        button.append(badge);
      }
      button.addEventListener("click", () => {
        viewer.goTo(slide.index);
        paint();
      });
      item.append(button);
      elements.slidesSorter.append(item);
      viewer.paintThumbnail(thumb, slide.index);
    }
  }

  function renderFidelity() {
    const findings = viewer.findings();
    elements.slidesFidelity.hidden = false;
    const total = findings.reduce((sum, finding) => sum + (finding.occurrences ?? 1), 0);
    if (findings.length === 0) {
      elements.slidesFidelitySummary.textContent = t("slides.fidelityClean");
      elements.slidesFidelityDetails.hidden = true;
      return;
    }
    elements.slidesFidelityDetails.hidden = false;
    elements.slidesFidelitySummary.textContent = t("slides.fidelityLossy", { count: total });
    elements.slidesFidelityList.replaceChildren();
    for (const finding of findings) {
      const item = document.createElement("li");
      // The feature name and the part it was charged to, which is what makes a
      // deck's report actionable: a gradient lost on slide 7 is a different fact
      // from one lost in the master. Not translated — these are OOXML element
      // names, and translating `a:gradFill` would make the report unsearchable.
      item.textContent = finding.part
        ? `${finding.feature} — ${finding.part}`
        : finding.feature;
      elements.slidesFidelityList.append(item);
    }
  }

  function paint() {
    const painted = viewer.paint(elements.slideCanvas, availableWidth());
    elements.slidesEmpty.hidden = painted;
    elements.slideCanvas.hidden = !painted;
    renderPosition();
    for (const button of elements.slidesSorter.querySelectorAll(".slides-thumb")) {
      button.removeAttribute("aria-current");
    }
    const current = elements.slidesSorter.querySelectorAll(".slides-thumb")[
      viewer.currentIndex()
    ];
    current?.setAttribute("aria-current", "true");
  }

  function openBytes(bytes) {
    clearError();
    const result = viewer.open(bytes);
    if (!result.ok) {
      elements.slidesSave.disabled = true;
      elements.slidesEmpty.hidden = false;
      elements.slideCanvas.hidden = true;
      showError(result.message);
      return false;
    }
    elements.slidesSave.disabled = false;
    renderSorter();
    paint();
    renderFidelity();
    return true;
  }

  elements.slidesFile.addEventListener("change", async (event) => {
    const file = event.target?.files?.[0];
    if (!file) return;
    openBytes(new Uint8Array(await file.arrayBuffer()));
  });

  // Arrow keys on the stage. The mapping lives in `slides.mjs` so it is testable
  // without a browser; this only decides whether to consume the event.
  elements.slideStage.addEventListener("keydown", (event) => {
    const next = viewer.indexForKey(event.key);
    if (next === null) return;
    event.preventDefault();
    viewer.goTo(next);
    paint();
  });

  return { openBytes, paint, renderSorter, renderFidelity, viewer };
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
