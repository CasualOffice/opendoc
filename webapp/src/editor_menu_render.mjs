// The menu rendering engine: the root context menu and its nested submenu
// flyouts, and their keyboard. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { OBJECT_LABELS } from "./object_traversal.mjs";
import { formatShortcut } from "./keyboard.mjs";
import { clampContextMenuPosition, moveMenuIndex, normalizeMenuEntries } from "./context_menu.mjs";
import { focusMenuIndex, menuItemAt, renderMenuLevel as renderMenuLevelRows } from "./menu_render.mjs";
import { createReflowChrome } from "./reflow_chrome.mjs";
import { createFoldChrome } from "./fold_chrome.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createMenuRender(ed) {
  // ---- Menu rendering engine (root context menu + nested submenu flyouts) -----
  function activeMenuLevel() {
    return ed.menuLevels[ed.keyboardLevelIndex] ?? null;
  }

  // Removes every open level deeper than `depth`, releasing submenu DOM nodes and
  // resetting the parent's expanded state.
  function closeMenuLevelsAbove(depth) {
    while (ed.menuLevels.length > depth + 1) {
      const level = ed.menuLevels.pop();
      level.parentButton?.setAttribute("aria-expanded", "false");
      if (level.el !== ed.editorContextMenu) level.el.remove();
    }
    if (ed.keyboardLevelIndex > ed.menuLevels.length - 1) {
      ed.keyboardLevelIndex = Math.max(0, ed.menuLevels.length - 1);
    }
  }

  // The row renderer lives in menu_render.mjs and owns no state; these hooks are
  // the only way it reaches the open level stack.
  const MENU_ROW_HOOKS = {
    shortcutText: formatShortcut,
    onHover(depth, index, button, entry, hasSub) {
      const level = ed.menuLevels[depth];
      if (!level) return;
      ed.keyboardLevelIndex = depth;
      focusMenuIndex(level, index, false);
      if (hasSub) openSubmenu(depth, button, entry);
      else closeMenuLevelsAbove(depth);
    },
    onActivate(depth, button, entry, hasSub) {
      if (hasSub) openSubmenu(depth, button, entry, true);
      else runMenuEntry(entry);
    },
  };

  function renderMenuLevel(el, entries, depth) {
    renderMenuLevelRows(el, entries, depth, MENU_ROW_HOOKS);
  }

  // Opens (or re-focuses) the flyout for a submenu-parent button. Prefers opening
  // to the right of the parent and flips left near the viewport edge.
  function openSubmenu(parentDepth, button, entry, viaKeyboard = false) {
    const depth = parentDepth + 1;
    const existing = ed.menuLevels[depth];
    if (existing && existing.parentButton === button) {
      if (viaKeyboard) {
        ed.keyboardLevelIndex = depth;
        focusMenuIndex(existing, moveMenuIndex(existing.entries, -1, 1));
      }
      return;
    }
    closeMenuLevelsAbove(parentDepth);
    button.setAttribute("aria-expanded", "true");
    const el = document.createElement("div");
    el.className = "context-menu editor-submenu";
    el.setAttribute("role", "menu");
    el.setAttribute("aria-label", entry.label);
    el.hidden = true;
    document.body.appendChild(el);
    const entries = normalizeMenuEntries(entry.submenu);
    const level = { el, entries, index: -1, parentButton: button };
    renderMenuLevel(el, entries, depth);
    el.hidden = false;
    const rect = button.getBoundingClientRect();
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    let left = rect.right - 4;
    if (left + width > window.innerWidth - 8) left = rect.left - width + 4;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    let top = Math.max(8, Math.min(rect.top - 5, window.innerHeight - height - 8));
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    ed.menuLevels[depth] = level;
    if (viaKeyboard) {
      ed.keyboardLevelIndex = depth;
      focusMenuIndex(level, moveMenuIndex(entries, -1, 1));
    }
  }

  function stepToParentLevel() {
    const parentDepth = ed.keyboardLevelIndex - 1;
    const parent = ed.menuLevels[parentDepth];
    closeMenuLevelsAbove(parentDepth);
    ed.keyboardLevelIndex = parentDepth;
    if (parent) {
      focusMenuIndex(
        parent,
        parent.index >= 0 ? parent.index : moveMenuIndex(parent.entries, -1, 1),
      );
    }
  }

  function hideContextMenu({ restoreFocus = false } = {}) {
    if (ed.editorContextMenu.hidden && ed.menuLevels.length === 0) return;
    for (const level of ed.menuLevels) {
      if (level.el === ed.editorContextMenu) level.el.replaceChildren();
      else level.el.remove();
    }
    ed.menuLevels = [];
    ed.keyboardLevelIndex = 0;
    ed.editorContextMenu.hidden = true;
    if (restoreFocus) {
      const target = ed.contextMenuReturnFocus?.isConnected ? ed.contextMenuReturnFocus : ed.pagesEl;
      target.focus({ preventScroll: true });
    }
    ed.contextMenuReturnFocus = null;
  }

  function runMenuEntry(entry) {
    if (!entry || entry.separator || entry.enabled === false || entry.submenu) return;
    hideContextMenu({ restoreFocus: true });
    entry.run();
  }

  function showContextMenu(clientX, clientY, context) {
    hideContextMenu();
    ed.contextMenuReturnFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : ed.pagesEl;
    const entries = normalizeMenuEntries(context.commands ?? ed.buildContextCommands(context));
    if (entries.length === 0) {
      ed.contextMenuReturnFocus = null;
      if (context.surface === "object") {
        const label = (OBJECT_LABELS[context.kind] ?? "Object").toLowerCase();
        ed.setStatus(`No editable properties are available for this nested ${label} yet`);
      }
      return false;
    }
    ed.editorContextMenu.hidden = false;
    renderMenuLevel(ed.editorContextMenu, entries, 0);
    const position = clampContextMenuPosition(
      clientX,
      clientY,
      ed.editorContextMenu.offsetWidth,
      ed.editorContextMenu.offsetHeight,
      window.innerWidth,
      window.innerHeight,
    );
    ed.editorContextMenu.style.left = `${position.left}px`;
    ed.editorContextMenu.style.top = `${position.top}px`;
    ed.menuLevels = [{ el: ed.editorContextMenu, entries, index: -1, parentButton: null }];
    ed.keyboardLevelIndex = 0;
    focusMenuIndex(ed.menuLevels[0], moveMenuIndex(entries, -1, 1));
    return true;
  }

  function keyboardContextMenuPoint() {
    if (!ed.selection || !ed.doc) return null;
    const flat = ed.doc.caretRect(ed.selection.focus.node, ed.selection.focus.offset);
    if (flat.length < 5) return null;
    const [pageNumber, x, y, width, height] = flat;
    const page = ed.pages[pageNumber - 1];
    if (!page) return null;
    const { rect, sx, sy } = ed.scaleOf(page);
    return {
      x: rect.left + (x + width) * sx,
      y: rect.top + (y + height) * sy,
      page,
    };
  }

  ed.pagesEl.addEventListener("contextmenu", (event) => {
    // No menu region, no menu — and no `preventDefault` either, so a container that
    // is a picture gives the visitor the browser's own menu instead of nothing.
    if (!ed.chromeShows("context")) return;
    const page = ed.pageFromEvent(event);
    if (!page || !ed.doc) return;
    // Object hit-test takes precedence (docs/85 §3.1): right-clicking a drawing /
    // image / text box selects it as a unit and shows its OBJECT menu, not the
    // paragraph-text menu.
    // …and only where this container selects objects: without the `objects` region
    // a right-click on an image is a right-click on the TEXT under it, not a
    // gateway to alt text, crop and properties (what the owner saw in read-only).
    const objectContext = ed.chromeShows("objects") ? ed.objectContextAtEvent(page, event) : null;
    if (objectContext) {
      event.preventDefault();
      ed.selectObject(
        objectContext.node,
        objectContext.kind,
        ed.anchorAt(page, event) || ed.selection?.focus || null,
        objectContext.anchored,
        objectContext,
      );
      showContextMenu(event.clientX, event.clientY, objectContext);
      return;
    }
    const anchor = ed.anchorAt(page, event);
    if (!anchor) return;
    event.preventDefault();
    const preserveSelection = ed.selectionContainsClientPoint(
      event.clientX,
      event.clientY,
    ) || ed.tableRange.containsClientPoint(event.clientX, event.clientY);
    if (!preserveSelection) {
      ed.selection = { anchor, focus: anchor };
      ed.tableRange.clear();
      ed.drawSelection();
    }
    showContextMenu(
      event.clientX,
      event.clientY,
      ed.contextAt(anchor, ed.linkAt(page, event)),
    );
  });

  document.addEventListener("pointerdown", (event) => {
    if (ed.editorContextMenu.hidden) return;
    if (ed.menuLevels.some((level) => level.el.contains(event.target))) return;
    hideContextMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (!ed.editorContextMenu.hidden) {
      const level = activeMenuLevel();
      if (!level) return;
      const entry = level.entries[level.index];
      if (event.key === "Escape") {
        event.preventDefault();
        if (ed.keyboardLevelIndex > 0) stepToParentLevel();
        else hideContextMenu({ restoreFocus: true });
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        closeMenuLevelsAbove(ed.keyboardLevelIndex);
        focusMenuIndex(
          level,
          moveMenuIndex(level.entries, level.index, event.key === "ArrowDown" ? 1 : -1),
        );
      } else if (event.key === "Home" || event.key === "End") {
        event.preventDefault();
        closeMenuLevelsAbove(ed.keyboardLevelIndex);
        focusMenuIndex(
          level,
          moveMenuIndex(level.entries, level.index, event.key === "Home" ? "first" : "last"),
        );
      } else if (event.key === "ArrowRight") {
        if (entry && entry.submenu && entry.enabled !== false) {
          event.preventDefault();
          openSubmenu(ed.keyboardLevelIndex, menuItemAt(level, level.index), entry, true);
        }
      } else if (event.key === "ArrowLeft") {
        if (ed.keyboardLevelIndex > 0) {
          event.preventDefault();
          stepToParentLevel();
        }
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        if (entry && entry.submenu) {
          openSubmenu(ed.keyboardLevelIndex, menuItemAt(level, level.index), entry, true);
        } else {
          runMenuEntry(entry);
        }
      }
      return;
    }
    if (
      ed.doc &&
      ed.eventTargetsEditor(event) &&
      ((event.shiftKey && event.key === "F10") || event.key === "ContextMenu")
    ) {
      // A selected object opens its OBJECT menu, anchored to the object's top-left
      // (matching how the object context bar is positioned).
      if (ed.objectSelection && ed.objectSelection.mode === "selected") {
        const rect = ed.doc.objectRect(ed.objectSelection.node); // [page, x, y, w, h]
        const page = rect.length >= 5 ? ed.pages[rect[0] - 1] : null;
        if (!page) return;
        event.preventDefault();
        const { rect: pageRect, sx, sy } = ed.scaleOf(page);
        showContextMenu(pageRect.left + rect[1] * sx, pageRect.top + rect[2] * sy, {
          surface: "object",
          node: ed.objectSelection.node,
          ref: ed.objectSelection.ref,
          kind: ed.objectSelection.kind,
          anchored: ed.objectSelection.anchored,
          ...Object.fromEntries(
            ed.OBJECT_CAPABILITY_KEYS.map((key) => [key, ed.objectSelection[key] === true]),
          ),
        });
        return;
      }
      if (!ed.selection) return;
      const point = keyboardContextMenuPoint();
      if (!point) return;
      event.preventDefault();
      const anchor = ed.selection.focus;
      const link = ed.linkAt(
        point.page,
        ed.clientPointEvent(point.x, Math.max(point.y - 1, 0)),
      );
      showContextMenu(point.x, point.y, ed.contextAt(anchor, link));
    }
  });
  ed.viewportEl.addEventListener("scroll", () => hideContextMenu(), { passive: true });
  window.addEventListener("resize", () => hideContextMenu());

  // The engine seam for reflow is one setter; everything the shell owes it, and
  // why each number is that number, is `reflow_chrome.mjs` (`docs/151` §6).
  // `openOutline` is a request the view makes and this file arbitrates.
  const reflowView = createReflowChrome({
    button: document.getElementById("viewReflowBtn"),
    viewport: ed.viewportEl,
    getDoc: () => ed.doc,
    unavailableReason: () => ed.readOnlyReason,
    onChanged: () => ed.renderAll(), redraw: () => ed.drawSelection(),
    openOutline: () => void (ed.outlinePanel.hidden && ed.reviewSidebar.hidden && ed.toggleOutline()),
    setStatus: ed.setStatus,
  });

  // Folding (ADR-049): the engine calls, the command rows and the margin chevron
  // are `fold_chrome.mjs`; the decisions are `fold_view.mjs`. `getPages`/`scaleOf`
  // are the pair the ruler and touch selection already take, for the same reason.
  const foldView = createFoldChrome({
    getDoc: () => ed.doc,
    caretNode: () => ed.selection?.focus?.node ?? "",
    getPages: () => ed.pages,
    outlineOpen: () => !ed.outlinePanel.hidden,
    surfaceSlackPx: reflowView.surfaceSlackPx,
    scaleOf: ed.scaleOf,
    onChanged: () => ed.renderAll().then(() => ed.scheduleChromeRefresh({ stats: true, outline: true })),
    setStatus: ed.setStatus,
  });

  return { foldView, reflowView, showContextMenu };
}
