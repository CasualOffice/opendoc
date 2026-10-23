// Shape Fill and Shape Outline (Word's Shape Format tab) and the object chip's
// Arrange, Position and Rotate menus. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { renderShapeMenu } from "./shape_format_menu.mjs";
import { POSITION_PRESETS, ROTATE_CHOICES, Z_ORDER_CHOICES, positionAvailability } from "./object_arrange.mjs";
import { renderArrangeMenu, renderPositionGallery, renderRotateMenu } from "./object_arrange_chrome.mjs";
import { t } from "./i18n.mjs";
import { onButton, closePopover, registerPopover } from "./popover_manager.mjs";
import { TEXT_STANDARD_COLORS } from "./palettes.mjs";
import { makeMenuHeading, makeSwatchCell, makeSwatchGrid } from "./picker_chrome.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createShapeChrome(ed) {
  // --- Shape Fill / Shape Outline (Word's Shape Format tab) --------------------
  // A drawing was renderable, selectable, movable and resizable but had no way to
  // change what it LOOKS like: the model carried `fill` and `stroke` and nothing
  // could write either. These are the two controls Word's Shape Format tab is
  // built around, on the same palette as the text pickers.

  /** What the context bar calls each kind of object. A shape used to read
   *  "Image", which also handed it a Crop button it has no source rectangle for. */

  const EMU_PER_POINT = 12700;
  /** The short palette the right-click submenu offers; the bar's picker has the
   *  full grid. A menu is a list, so it gets the colors people actually reach for. */
  const SHAPE_MENU_COLORS = ["#000000", "#ffffff", "#ff0000", "#ff9900", "#ffff00", "#00ff00", "#4a86e8", "#9900ff"];
  /** Word's Shape Outline > Weight list, in points. */
  const OUTLINE_WEIGHTS = [0.25, 0.5, 1, 1.5, 2.25, 3, 4.5, 6];

  const shapeFillMenu = document.getElementById("shapeFillMenu");
  const shapeOutlineMenu = document.getElementById("shapeOutlineMenu");

  /** The selected shape's current fill/outline, or an empty record when there is
   *  no shape selected. Read from the model, never remembered from the last
   *  apply — the swatch must describe THIS shape. */
  function selectedShapeFormat() {
    // A picture has the outline half — its border (HF-254) — read the same way.
    if (!ed.doc || !ed.objectSelection || (!ed.objectSelection.canFill && !ed.objectSelection.canStroke)) return {};
    try {
      return ed.doc.shapeFormat(ed.objectSelection.node) ?? {};
    } catch {
      return {};
    }
  }

  /** Paints each bar button's underline bar with the shape's own color (or the
   *  none-hatch when it has none), so the control reads before it is opened. */
  function reflectShapeSwatches() {
    ed.reflectShapeFormatState();
    const format = selectedShapeFormat();
    for (const [btn, color] of [
      [shapeFillBtn, format.fill],
      [shapeOutlineBtn, format.outline],
    ]) {
      const bar = btn.querySelector(".color-apply-bar");
      if (!bar) continue;
      bar.style.background = color || "transparent";
      bar.classList.toggle("is-none", !color);
    }
  }

  /** A bar button carrying a color bar, matching the ribbon's split controls. */
  function makeShapeBarButton(icon, label, title) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "object-bar-btn object-bar-color";
    btn.title = title;
    btn.setAttribute("aria-label", title);
    btn.setAttribute("aria-haspopup", "dialog");
    btn.setAttribute("aria-expanded", "false");
    btn.innerHTML =
      `<span class="ms" aria-hidden="true">${icon}</span><span>${label}</span>` +
      `<span class="color-apply-bar" aria-hidden="true"></span>`;
    // The other bar buttons cancel `pointerdown` to keep the selection, but these
    // open popovers, and cancelling pointerdown suppresses the compatibility
    // `mousedown` the popover manager opens on — the button would do nothing at
    // all. `onButton` cancels that mousedown itself, preserving the selection.
    return btn;
  }

  const shapeFillBtn = makeShapeBarButton("format_color_fill", "Fill", "Shape fill");
  const shapeOutlineBtn = makeShapeBarButton("border_color", "Outline", "Shape outline");

  /** Word's Shape Fill / Shape Outline menus. `shape_format_menu.mjs`. */
  const shapeMenuHost = {
    menu: (kind) => (kind === "fill" ? shapeFillMenu : shapeOutlineMenu),
    format: () => selectedShapeFormat(),
    heading: (text) => makeMenuHeading(text),
    swatchGrid: (cells) => makeSwatchGrid(cells),
    swatchCell: (...args) => makeSwatchCell(...args),
    colors: () => TEXT_STANDARD_COLORS,
    weights: () => OUTLINE_WEIGHTS,
    emuPerPoint: EMU_PER_POINT,
  };

  const shapeFillPopover = registerPopover(shapeFillBtn, shapeFillMenu, () => renderShapeMenu("fill", shapeMenuHost));
  const shapeOutlinePopover = registerPopover(shapeOutlineBtn, shapeOutlineMenu, () =>
    renderShapeMenu("outline", shapeMenuHost),
  );

  // ---- The object chip's Arrange controls ------------------------------------
  // Three popovers on the chip a selected object carries: Word's Position gallery,
  // its Arrange menu (stacking + Group/Ungroup) and its Rotate menu. Registered
  // here rather than beside the commands because `registerPopover` and the
  // `popovers` list it appends to are declared further down the file.

  /** A chip button that OPENS a popover. Unlike the chip's action buttons it must
   *  NOT cancel `pointerdown`: cancelling it suppresses the compatibility
   *  `mousedown` the popover manager opens on, and the button would do nothing at
   *  all — the same trap `makeShapeBarButton` documents. */
  function makeObjectMenuButton(icon, labelKey, menuId) {
    const btn = document.createElement("button");
    btn.type = "button";
    // Icon-only, like Word's own Arrange group and Docs' image chip. The chip is
    // already the densest control in the product and these three would have added
    // ~150px of text to a bar that sits OVER the object when there is no room
    // above it. The name is on the tooltip AND on the accessible name.
    btn.className = "object-bar-btn object-bar-icon";
    btn.setAttribute("aria-haspopup", "menu");
    btn.setAttribute("aria-expanded", "false");
    btn.setAttribute("aria-controls", menuId);
    const glyph = document.createElement("span");
    glyph.className = "ms";
    glyph.setAttribute("aria-hidden", "true");
    glyph.textContent = icon;
    btn.appendChild(glyph);
    btn.dataset.objectMenu = menuId;
    btn.dataset.labelKey = labelKey;
    return labelledObjectMenuButton(btn);
  }

  /** Re-reads a chip menu button's name from the catalogue.
   *
   *  These buttons are built ONCE, at import, when no catalogue is loaded — so a
   *  name baked in there is the KEY, in every language including English. Reading
   *  it on every bar render is also what makes a locale switch reach them. */
  function labelledObjectMenuButton(btn) {
    const label = t(btn.dataset.labelKey);
    btn.title = label;
    btn.setAttribute("aria-label", label);
    return btn;
  }

  const objectPositionMenu = document.getElementById("objectPositionMenu");
  const objectArrangeMenu = document.getElementById("objectArrangeMenu");
  const objectRotateMenu = document.getElementById("objectRotateMenu");
  const objectPositionBtn = makeObjectMenuButton("open_with", "object.position", "objectPositionMenu");
  const objectArrangeBtn = makeObjectMenuButton("layers", "object.arrange", "objectArrangeMenu");
  const objectRotateBtn = makeObjectMenuButton("rotate_right", "object.rotate", "objectRotateMenu");

  const objectPositionPopover = registerPopover(objectPositionBtn, objectPositionMenu, () => {
    const state = ed.arrangeState();
    const availability = positionAvailability(state?.read ?? { object: false });
    renderPositionGallery(objectPositionMenu, {
      presets: POSITION_PRESETS,
      t,
      activeId: null,
      reason: availability.available ? null : t(availability.reasonKey),
      onPick: (preset) => {
        closePopover(objectPositionPopover);
        ed.objectArrange.applyPosition(preset);
      },
    });
  });
  const objectArrangePopover = registerPopover(objectArrangeBtn, objectArrangeMenu, () => {
    const verdict = ed.objectArrange.groupability();
    renderArrangeMenu(objectArrangeMenu, {
      t,
      zChoices: Z_ORDER_CHOICES,
      zOrder: ed.objectArrange.zOrderAvailability(ed.arrangeState()),
      group: { enabled: verdict.can, reason: verdict.reason },
      ungroup: {
        enabled: ed.objectSelection?.kind === "group",
        reason: t("object.ungroup.notAGroup"),
      },
      onZOrder: (order) => {
        closePopover(objectArrangePopover);
        ed.objectArrange.setZOrder(order);
      },
      onGroup: () => {
        closePopover(objectArrangePopover);
        void ed.objectArrange.group();
      },
      onUngroup: () => {
        closePopover(objectArrangePopover);
        void ed.objectArrange.ungroup();
      },
    });
  });
  const objectRotatePopover = registerPopover(objectRotateBtn, objectRotateMenu, () => {
    const state = ed.arrangeState();
    renderRotateMenu(objectRotateMenu, {
      t,
      choices: ROTATE_CHOICES,
      enabled: !!state?.transform,
      reason: t("object.rotate.unsupported"),
      onPick: (value) => {
        closePopover(objectRotatePopover);
        ed.objectArrange.rotate(value);
      },
    });
  });



  function handleShapeMenuClick(event, kind, popover) {
    const none = event.target.closest("[data-shape-none]");
    const cell = event.target.closest("[data-shape-color]");
    const weight = event.target.closest("[data-shape-weight]");
    if (!none && !cell && !weight) return;
    if (kind === "fill") {
      ed.applyShapeFill(none ? null : cell.dataset.shapeColor);
    } else if (none) {
      ed.applyShapeOutline({ color: null });
    } else if (weight) {
      ed.applyShapeOutline({ widthEmu: Number(weight.dataset.shapeWeight) });
    } else {
      ed.applyShapeOutline({ color: cell.dataset.shapeColor });
    }
    closePopover(popover);
  }

  shapeFillMenu.addEventListener("click", (e) => handleShapeMenuClick(e, "fill", shapeFillPopover));
  shapeOutlineMenu.addEventListener("click", (e) =>
    handleShapeMenuClick(e, "outline", shapeOutlinePopover),
  );

  // Split-button apply halves reapply the last-used swatch (Word/Docs behavior).
  onButton(ed.textColorApplyBtn, () => {
    ed.applyTextColor(ed.lastTextColor);
    ed.recordRecent(ed.recentTextColors, ed.lastTextColor);
  });
  onButton(ed.highlightApplyBtn, () => {
    ed.applyHighlight(ed.lastHighlight);
    ed.recordRecent(ed.recentHighlights, ed.lastHighlight);
  });
  ed.reflectTextColorSwatch(ed.lastTextColor);
  ed.reflectHighlightSwatch(ed.lastHighlight);

  return {
    SHAPE_MENU_COLORS, labelledObjectMenuButton, objectArrangeBtn, objectPositionBtn,
    objectRotateBtn, reflectShapeSwatches, selectedShapeFormat, shapeFillBtn, shapeOutlineBtn,
  };
}
