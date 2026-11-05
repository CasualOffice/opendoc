// The floating selection toolbar (the mini-toolbar) that sits above a text
// selection, and the floating-chrome arbiter that decides which ONE floating
// surface shows (docs/184 D-12). It reaches the rest of the editor only
// through the context `ed`.
import { onButton } from "./popover_manager.mjs";
import { createFloatingArbiter, installFloatingArbiter, paintedSelectionRect } from "./floating_arbiter.mjs";
import { miniToolbarOpacity } from "./floating_arbiter.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSelectionToolbar(ed) {
  // ---- Floating selection toolbar (appears above a text selection) ------------
  const selToolbar = document.getElementById("selToolbar");

  // ---- Which selections earn the mini-toolbar (audit CANVAS-06) -------------
  //
  // Word's mini toolbar fades in after a MOUSE selection and never for a
  // keyboard one: Shift+arrows, Ctrl+A and Tab into a table cell all select
  // text, and a bar over the line above the caret is in the way of the next
  // keystroke. So the bar is earned by a pointer gesture on the pages that ends
  // in a range, and lost by the next key, the next press anywhere else, or a
  // pointer that wanders away from it.
  let pointerMade = false;
  let pressOnPages = false;
  window.addEventListener(
    "pointerdown",
    (event) => {
      if (selToolbar.contains(event.target)) return;
      pressOnPages = ed.pagesEl.contains(event.target);
      pointerMade = false;
    },
    true,
  );
  window.addEventListener(
    "pointerup",
    () => {
      if (!pressOnPages) return;
      pressOnPages = false;
      pointerMade = true;
      // The editor finalises the selection in its own pointer-up handler, which
      // runs after this capture-phase listener (and after the microtask
      // checkpoint between listeners); ask once the event is done.
      setTimeout(() => positionSelToolbar(), 0);
    },
    true,
  );
  window.addEventListener(
    "keydown",
    (event) => {
      if (selToolbar.contains(event.target)) return;
      // Keys inside the bar's own popovers (✦ actions, the two colour menus)
      // are the bar being used, not the reader moving on.
      if (event.target?.closest?.("#selAiMenu, #selTextColorMenu, #selHighlightMenu")) return;
      if (["Shift", "Control", "Meta", "Alt"].includes(event.key)) return;
      pointerMade = false;
      if (!selToolbar.hidden) selToolbar.hidden = true;
    },
    true,
  );
  // Fade with distance, as Word's does: the bar is fully there while the
  // pointer is near it and fades out as the pointer leaves for the document,
  // so it never sits opaque over text the reader has moved on to.
  //
  // The fade is written to a CSSOM rule, never to an element's attributes: the
  // floating arbiter watches the bar's `style` (an owner re-placing it) and the
  // plugin platform watches the root's (a theme change), and a write per
  // pointermove queued a settle, with a layout read of every highlight, and a
  // theme broadcast to every plugin, per move. Faded all the way out, the bar is
  // gone, as Word's is: `hidden`, so it no longer outranks the paste chip and
  // its buttons leave the tab order and the accessibility tree; the next
  // pointer selection earns it back.
  const fadeRule = selToolbarFadeRule(document);
  const setFade = (opacity) => {
    if (!fadeRule) return;
    fadeRule.style.opacity = opacity >= 1 ? "" : String(opacity);
    fadeRule.style.pointerEvents = opacity < 0.1 ? "none" : "";
  };
  window.addEventListener(
    "pointermove",
    (event) => {
      if (selToolbar.hidden || event.pointerType === "touch") return;
      const opacity = miniToolbarOpacity(selToolbar.getBoundingClientRect(), event.clientX, event.clientY);
      if (opacity <= 0) {
        pointerMade = false;
        selToolbar.hidden = true;
        setFade(1);
        return;
      }
      setFade(opacity);
    },
    { passive: true },
  );

  /** Shows the floating toolbar centred just above the current range selection (or
   *  below it when there's no room), or hides it when the selection is collapsed
   *  or was not made with the pointer. `earn` is for a command that acts
   *  through the bar on the current selection however it was made (⌘/ opens
   *  the bar's ✦ actions, which hang off the bar): it shows the bar for a
   *  keyboard selection too, until the next key retires it. */
  function positionSelToolbar({ earn = false } = {}) {
    if (earn) pointerMade = true;
    if (ed.activeLink || !ed.selection || !ed.hasRange() || !pointerMade) {
      selToolbar.hidden = true;
      return;
    }
    const rects = ed.pagesEl.querySelectorAll(".overlay .highlight");
    if (!rects.length) {
      selToolbar.hidden = true;
      return;
    }
    let top = Infinity;
    let bottom = -Infinity;
    let left = Infinity;
    let right = -Infinity;
    for (const el of rects) {
      const b = el.getBoundingClientRect();
      top = Math.min(top, b.top);
      bottom = Math.max(bottom, b.bottom);
      left = Math.min(left, b.left);
      right = Math.max(right, b.right);
    }
    const wasHidden = selToolbar.hidden;
    selToolbar.hidden = false; // must be visible to measure
    if (wasHidden) setFade(1);
    const tw = selToolbar.offsetWidth;
    const th = selToolbar.offsetHeight;
    const viewport = ed.viewportEl.getBoundingClientRect();
    const topBound = Math.max(8, viewport.top + 8);
    const bottomBound = Math.min(window.innerHeight - 8, viewport.bottom - 8);
    let x = (left + right) / 2 - tw / 2;
    // …plus whatever the touch handles need. They hang above the selection's top
    // and below its bottom, and this bar is a FIXED element over the page: eight
    // pixels of gap put it through the middle of the start handle's dot and took
    // every touch aimed at it (`touch_selection.mjs`'s `clearance`).
    const gap = 8 + ed.touchSelection.clearance();
    let y = top - th - gap;
    if (y < topBound) y = bottom + gap; // no room above → drop below the selection
    if (y + th > bottomBound) {
      // A selection at the viewport bottom may have no full-height slot below it;
      // keep the bar visible and out of the browser chrome rather than allowing a
      // fixed-position toolbar to disappear below the window.
      y = Math.min(y, bottomBound - th);
    }
    y = Math.max(topBound, y);
    x = Math.max(8, Math.min(x, window.innerWidth - tw - 8));
    selToolbar.style.left = `${Math.round(x)}px`;
    selToolbar.style.top = `${Math.round(y)}px`;
  }

  for (const b of selToolbar.querySelectorAll("[data-fmt]")) {
    onButton(b, () => ed.toggleFormat(b.dataset.fmt));
  }
  // Text-color and highlight now use the ribbon's swatch-picker popovers, wired up
  // above (registerPopover on #selTextColorBtn / #selHighlightBtn). Keep clicks
  // inside the bar from collapsing the selection; hide on viewport scroll.
  selToolbar.addEventListener("mousedown", (e) => {
    if (e.target.tagName !== "INPUT" && e.target.tagName !== "SELECT") e.preventDefault();
  });
  ed.viewportEl.addEventListener("scroll", () => (selToolbar.hidden = true), { passive: true });

  // ---- The arbiter (docs/184 D-12) --------------------------------------------
  // Installed here because this module is the floating chrome's, and it runs
  // once at boot like every other domain; the surfaces are looked up when the
  // arbiter settles, because two of them are created lazily.
  installFloatingArbiter(
    createFloatingArbiter({
      surfaces: {
        linkChip: () => ed.linkChip,
        objectBar: () => document.querySelector(".object-context-bar"),
        tableInsert: () => ed.pagesEl.querySelector(".table-insert-target"),
        selectionToolbar: () => selToolbar,
        pasteOptions: () => document.getElementById("pasteOptions"),
      },
      selectionRect: () => paintedSelectionRect(ed.pagesEl),
      viewportRect: () => ed.viewportEl.getBoundingClientRect(),
    }),
  );

  return { positionSelToolbar };
}

/** One CSSOM rule, `.sel-toolbar { }`, that the bar's fade writes into: a
 *  constructed sheet where the browser has them, else a `<style>` of its own.
 *  Neither write is a DOM mutation, so no MutationObserver hears a fade. */
function selToolbarFadeRule(doc) {
  try {
    let sheet;
    if (typeof CSSStyleSheet === "function" && "adoptedStyleSheets" in doc) {
      sheet = new CSSStyleSheet();
      doc.adoptedStyleSheets = [...doc.adoptedStyleSheets, sheet];
    } else {
      const style = doc.createElement("style");
      style.dataset.owner = "sel-toolbar-fade";
      doc.head.append(style);
      sheet = style.sheet;
    }
    sheet.insertRule("#selToolbar.sel-toolbar {}", 0);
    return sheet.cssRules[0];
  } catch {
    return null;
  }
}
