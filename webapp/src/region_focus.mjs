// F6 / Shift+F6: move the keyboard between the window's major regions.
//
// There was no keyboard route from the document OUT to the chrome short of
// Tab, and Tab inside the editing surface inserts a tab character — so a
// keyboard user in the document could reach the ribbon, an open pane or the
// status bar only by reaching for the mouse. Word, Office for the web and every
// Windows application answer this with F6 (next region) and Shift+F6 (previous),
// which is the "landmark" navigation pattern; this is that.
//
// THE REGIONS, in the order a reader sees them, left to right and top to bottom:
//
//   1. navigation — the ribbon tab strip, or the menu bar in compact mode; focus
//      lands on the tab (or menu) that holds the roving tab stop, so the arrows
//      work at once;
//   2. any open side pane LEFT of the document (Outline, Pages, Compare);
//   3. the document — through the editor's own `focusEditor`, so the focus-owner
//      contract (`expectEditorFocused`, `focus_contract.test.mjs`) is honoured
//      rather than worked around: the keyboard lands where typing goes;
//   4. any open pane RIGHT of it (comments, version history, the property and
//      glyph panes, the Find and replace panel);
//   5. the status bar.
//
// A region with nothing focusable on screen is skipped rather than landed on,
// because focus parked on an inert container reads as the key having done
// nothing. Nothing happens while a modal is open: the modal owns the keyboard
// (`modal.mjs`), and F6 walking out of it would be the HF-063 defect again.
//
// The list is rebuilt per press — O(regions), about a dozen — rather than
// cached, because panes open and close and the chrome changes mode; a cache is
// one more thing to invalidate for a key nobody presses in a loop.
import { t } from "./i18n.mjs";

/** The two command ids, for `editorCommands()` and for the keymap guard. */
export const REGION_COMMAND_IDS = Object.freeze(["view.region.next", "view.region.previous"]);

const FOCUSABLE = [
  "a[href]",
  "button:not(:disabled)",
  "input:not(:disabled):not([type='hidden'])",
  "select:not(:disabled)",
  "textarea:not(:disabled)",
  "[tabindex]:not([tabindex='-1'])",
].join(", ");

const shown = (element) => !!element && !element.hidden && element.getClientRects().length > 0;

/** The first control in `root` a keyboard can land on, or null. */
function firstFocusable(root) {
  for (const candidate of root?.querySelectorAll(FOCUSABLE) ?? []) {
    if (shown(candidate)) return candidate;
  }
  return null;
}

/**
 * @param {object} io
 * @param {Document} io.root the page.
 * @param {() => void} io.focusEditor puts the keyboard on the editing surface.
 * @param {() => boolean} io.documentOpen whether there is a document to type into.
 * @param {() => boolean} io.modalOpen whether a modal owns the keyboard.
 */
export function createRegionCycle(io) {
  const doc = io.root;
  const one = (selector) => doc.querySelector(selector);

  /** The roving stop of whichever navigation axis is on screen. */
  function navigationTarget() {
    const tab = one('.ribbon-tabs [role="tab"][tabindex="0"]') ?? one('.ribbon-tabs [role="tab"][aria-selected="true"]');
    if (shown(tab)) return tab;
    const menu = [...doc.querySelectorAll("#appMenuBar .app-menu-button")].find(shown);
    if (menu) return menu;
    return firstFocusable(one("header.bar"));
  }

  /** The regions on screen right now, in reading order. Each is
   *  `{ id, contains(el), enter() → boolean }`. */
  function regions() {
    const viewport = one("#viewport");
    const editing = [one("#pages"), one("#editorTextInput")].filter(Boolean);
    const panes = [...doc.querySelectorAll("aside.side-panel, #reviewSidebar, #findPanel")].filter(shown);
    // Left of the document is whatever precedes `#viewport` in the markup — except
    // the Find panel, which is authored early but floats over the right edge.
    const before = (el) =>
      el.id !== "findPanel" &&
      !!viewport &&
      !viewport.contains(el) &&
      !!(el.compareDocumentPosition(viewport) & 4 /* DOCUMENT_POSITION_FOLLOWING */);
    const pane = (el) => ({ id: el.id, contains: (node) => el.contains(node), enter: () => focus(firstFocusable(el)) });
    const list = [
      {
        id: "navigation",
        contains: (node) => !!(one("header.bar")?.contains(node) || one(".ribbon")?.contains(node) || one("#compactToolbar")?.contains(node)),
        enter: () => focus(navigationTarget()),
      },
      ...panes.filter(before).map(pane),
      {
        id: "document",
        contains: (node) => editing.some((el) => el === node || el.contains(node)),
        // Skipped until a document is open: an empty editing surface takes no
        // typing, so landing there would be the "key did nothing" this avoids.
        enter: () => {
          if (!io.documentOpen()) return false;
          io.focusEditor();
          return true;
        },
      },
      ...panes.filter((el) => !before(el)).map(pane),
      { id: "status", contains: (node) => !!one(".footer")?.contains(node), enter: () => focus(firstFocusable(one(".footer"))) },
    ];
    return list;
  }

  function focus(element) {
    if (!element) return false;
    element.focus();
    return true;
  }

  /** Moves to the next (`step` 1) or previous (`step` -1) region that can take
   *  the keyboard. Returns the id of the region entered, or null. */
  function move(step) {
    if (io.modalOpen()) return null;
    const list = regions();
    const active = doc.activeElement;
    let at = list.findIndex((region) => active && region.contains(active));
    // Focus outside every region — on <body>, the rail, the ruler — counts as the
    // document, the region a reader is in when nothing else has the keyboard.
    if (at < 0) at = list.findIndex((region) => region.id === "document");
    for (let hop = 1; hop <= list.length; hop += 1) {
      const region = list[(at + step * hop + list.length * hop) % list.length];
      if (region.enter()) return region.id;
    }
    return null;
  }

  return {
    next: () => move(1),
    previous: () => move(-1),
    /** The palette rows; the chords are `keymap.mjs`'s F6 / ⇧F6. `noDoc`, because
     *  the chrome has regions before any document does. */
    commands: () => {
      // One row in File ▸ Shortcuts, as Word lists F6 / Shift+F6 as one "pane" row.
      const referenceRow = { id: "regions", label: t("region.move") };
      return [
        { id: REGION_COMMAND_IDS[0], label: t("region.next"), group: "View", kw: "f6 region pane landmark focus ribbon status bar move next", noDoc: true, referenceRow, run: () => void move(1) },
        { id: REGION_COMMAND_IDS[1], label: t("region.previous"), group: "View", kw: "shift f6 region pane landmark focus ribbon status bar move previous", noDoc: true, referenceRow, run: () => void move(-1) },
      ];
    },
  };
}
