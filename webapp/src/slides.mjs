// The deck viewer's own module (docs/156 Tier 3).
//
// It is NOT part of `main.js`. That file is 16,177 lines and the one-writer rule
// on it exists because two editors in it conflict badly; a second surface added
// into it would also make the DOCX editor's bundle carry the deck engine for
// every visitor who never opens one. So this is a separate page with a separate
// entry point, and the only thing the two share is the chrome vocabulary.
//
// # What it does, and what it deliberately does not
//
// Opens a `.pptx`, renders a slide to a canvas at the device's own pixel ratio,
// pages through the deck, shows a sorter, and surfaces the fidelity report. It
// does **not** edit: `casual-pres-wasm` exposes no operation set, because a
// presentation editing surface has to route through `casual-doc-transaction`
// (ADR-005, ADR-043) and bypassing it is precisely the defect `105` CQ-002
// records on the document path. A viewer that could type would be that defect
// repeated, so the facade does not offer it and this page does not pretend to.
//
// # Why the canvas is sized from the FILE and scaled by the device
//
// `slideWidthEmu`/`slideHeightEmu` are the deck's own surface. The CSS box comes
// from that aspect ratio and the available width; the backing store is that box
// times `devicePixelRatio`, and `renderSlide` is asked for the matching dpi. A
// page that rendered at a fixed 96 and let the browser upscale would be blurry on
// every retina display, and no guard that only checked "a bitmap appeared" would
// catch it.

import { t } from "./i18n.mjs";

/// A slide surface is stated in EMU; 914400 EMU is one inch.
const EMU_PER_INCH = 914400;

/// The dpi a slide thumbnail renders at.
///
/// Thumbnails are decoration for navigation, not reading, so they are rendered
/// once at a low fixed density rather than at the device ratio: a 40-slide deck
/// re-rendered at retina density on every open is a visible stall for a strip
/// nobody reads text in.
const THUMBNAIL_DPI = 12;

/// Everything the page needs to answer a keystroke or a click.
///
/// One object rather than module-level mutable state, so a second deck opened in
/// the same tab replaces it wholly — a half-replaced viewer showing one deck's
/// slide count and another's pixels is the failure this shape prevents.
export function createViewer({ facade, elements, devicePixelRatio = 1 }) {
  const state = { deck: null, index: 0 };

  /// Opens bytes as a deck, or reports why not.
  ///
  /// A refusal is shown rather than swallowed: the engine refuses a package it
  /// cannot read correctly instead of opening it wrong, and a viewer that hid
  /// that would turn a loud refusal into a blank page.
  function open(bytes) {
    try {
      state.deck = facade.open(bytes);
      state.index = 0;
    } catch (error) {
      state.deck = null;
      return { ok: false, message: String(error?.message ?? error) };
    }
    return { ok: true };
  }

  /// The slide count, or zero when nothing is open.
  function slideCount() {
    return state.deck ? state.deck.slideCount : 0;
  }

  /// Moves to `index`, clamped into the deck.
  ///
  /// Clamped rather than refused: End on the last slide and Home on the first are
  /// ordinary keystrokes, and a viewer that errored on them would make the
  /// keyboard feel broken at both ends of every deck.
  function goTo(index) {
    const count = slideCount();
    if (count === 0) return 0;
    state.index = Math.max(0, Math.min(count - 1, index));
    return state.index;
  }

  /// The current slide's index.
  function currentIndex() {
    return state.index;
  }

  /// The slide descriptors a sorter needs, in presentation order.
  ///
  /// `p:sldIdLst`'s order, which is what the facade exposes — never the part
  /// names, because `slide10.xml` sorts before `slide2.xml` and a sorter built on
  /// that would show a different, plausible deck.
  function slides() {
    const count = slideCount();
    const out = [];
    for (let index = 0; index < count; index += 1) {
      out.push({
        index,
        name: state.deck.slideName(index) || t("slides.untitled", { number: index + 1 }),
        hidden: state.deck.slideHidden(index),
      });
    }
    return out;
  }

  /// The CSS box a slide should occupy inside `availableWidth`, preserving the
  /// deck's own aspect ratio.
  function cssSize(availableWidth) {
    if (!state.deck) return { width: 0, height: 0 };
    const widthEmu = state.deck.slideWidthEmu;
    const heightEmu = state.deck.slideHeightEmu;
    if (widthEmu <= 0 || heightEmu <= 0) return { width: 0, height: 0 };
    const width = Math.max(1, Math.floor(availableWidth));
    return { width, height: Math.max(1, Math.round((width * heightEmu) / widthEmu)) };
  }

  /// The dpi that fills `cssWidth` CSS pixels at `devicePixelRatio`.
  ///
  /// Derived from the slide's own width rather than assumed: a 4:3 deck and a
  /// 16:9 deck at the same on-screen width are different scales, and a fixed dpi
  /// would render one of them at the wrong size.
  function dpiFor(cssWidth) {
    if (!state.deck) return 96;
    const inches = state.deck.slideWidthEmu / EMU_PER_INCH;
    if (inches <= 0) return 96;
    return (cssWidth * devicePixelRatio) / inches;
  }

  /// Renders the current slide into `canvas`.
  function paint(canvas, availableWidth) {
    if (!state.deck) return false;
    const css = cssSize(availableWidth);
    if (css.width === 0) return false;
    const bitmap = state.deck.renderSlide(state.index, dpiFor(css.width));
    // Dimensions before pixels, for the reason `paintThumbnail` records: `rgba`
    // moves the bitmap out and frees the handle.
    const width = bitmap.widthPx;
    const height = bitmap.heightPx;
    // The backing store is device pixels; the CSS box is the layout size. Setting
    // only the attributes would leave the element at its intrinsic size and the
    // deck would be the wrong size on the page.
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = `${css.width}px`;
    canvas.style.height = `${css.height}px`;
    const context = canvas.getContext("2d");
    if (!context) return false;
    // `rgba` is consumed by this call: the facade moves the pixels out rather
    // than copying them, which is what keeps paging through a deck from
    // duplicating a full-slide bitmap per frame.
    context.putImageData(new ImageData(new Uint8ClampedArray(bitmap.rgba), width, height), 0, 0);
    return true;
  }

  /// Renders one thumbnail, at the fixed low density.
  function paintThumbnail(canvas, index) {
    if (!state.deck) return false;
    const bitmap = state.deck.renderSlide(index, THUMBNAIL_DPI);
    // THE DIMENSIONS COME FIRST, ALWAYS. `rgba` is a MOVE across the boundary —
    // it takes the bitmap by value so paging a deck does not duplicate a
    // full-slide buffer per frame — so reading it frees the handle and every
    // later getter hits a dropped pointer. An earlier version of this function
    // read it as the first argument of `new ImageData(...)`, where JS evaluates
    // left to right, and the next `bitmap.widthPx` threw "null pointer passed to
    // rust". Nothing on the page caught it: the first thumbnail threw, the loop
    // stopped, and the deck simply never painted.
    //
    // `pixelsAfterDimensions` in the guards drives this ordering directly, so the
    // rule is asserted rather than left to this comment.
    const width = bitmap.widthPx;
    const height = bitmap.heightPx;
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) return false;
    context.putImageData(new ImageData(new Uint8ClampedArray(bitmap.rgba), width, height), 0, 0);
    return true;
  }

  /// The current slide's text, as structure, or `null` when nothing is open.
  ///
  /// Parsed here rather than in the mirror, for the reason `findings` is: the
  /// facade's boundary is JSON and exactly one place should know that. A
  /// malformed projection yields `null` rather than losing the deck — the slides
  /// still render, and a mirror with nothing in it is a worse outcome than a
  /// mirror that is briefly empty.
  function slideText() {
    if (!state.deck) return null;
    try {
      const parsed = JSON.parse(state.deck.slideText(state.index));
      return Array.isArray(parsed?.shapes) ? parsed : null;
    } catch {
      return null;
    }
  }

  /// The fidelity report's findings, parsed.
  ///
  /// Surfaced rather than hidden: it is what makes direct OOXML an advantage over
  /// a converter rather than a claim, and a viewer that opened silently would
  /// throw away the one thing this engine can say that a converter cannot.
  function findings() {
    if (!state.deck) return [];
    try {
      const parsed = JSON.parse(state.deck.fidelityReport);
      return Array.isArray(parsed?.findings) ? parsed.findings : [];
    } catch {
      // A malformed report is a bug one layer down, and losing the deck over it
      // would be the wrong trade: the slides still render.
      return [];
    }
  }

  /// Saves the deck, carrying every part the engine does not model through.
  function save() {
    if (!state.deck) return null;
    return state.deck.save();
  }

  /// Maps a key to a slide index, or `null` when the key is not navigation.
  ///
  /// Separated from the DOM so the mapping is testable without a browser — the
  /// `keyboard.type`-style trap this repository has hit twice is a spec that
  /// never actually reached the element under test.
  function indexForKey(key) {
    const count = slideCount();
    if (count === 0) return null;
    switch (key) {
      case "ArrowRight":
      case "ArrowDown":
      case "PageDown":
      case " ":
        return Math.min(count - 1, state.index + 1);
      case "ArrowLeft":
      case "ArrowUp":
      case "PageUp":
        return Math.max(0, state.index - 1);
      case "Home":
        return 0;
      case "End":
        return count - 1;
      default:
        return null;
    }
  }

  const _unusedElements = elements;
  return {
    open,
    slideCount,
    slides,
    goTo,
    currentIndex,
    cssSize,
    dpiFor,
    paint,
    paintThumbnail,
    slideText,
    findings,
    save,
    indexForKey,
  };
}
