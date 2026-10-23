// Insert > Shapes gallery, and the line and paragraph spacing menu. Moved out
// of `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { createSpacingMenu } from "./spacing_menu.mjs";
import { wireListGallery } from "./list_gallery_wiring.mjs";
import { galleryRowStarts, nextGalleryIndex, renderShapeGallery } from "./shape_gallery.mjs";
import { t } from "./i18n.mjs";
import { onButton, openPopover, closePopover, registerPopover } from "./popover_manager.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createShapeGallery(ed) {
  // --- Insert ▸ Shapes gallery -------------------------------------------------
  const shapeGalleryMenu = document.getElementById("shapeGalleryMenu");

  /** The gallery cells, in keyboard order, and where each visual row starts —
   *  rebuilt on every open because the catalogue is fixed but the DOM is not. */
  let shapeGalleryCells = [];
  const SHAPE_GALLERY_COLUMNS = 6;

  /** Renders Word's grouped shape gallery: every preset the engine models, under
   *  the headings Word files them under. `shape_gallery.mjs` owns the DOM. */
  function paintShapeGallery() {
    shapeGalleryCells = renderShapeGallery(shapeGalleryMenu, {
      t,
      onPick: (token) => {
        closePopover(shapeGalleryPopover);
        // Word ARMS the pointer here rather than inserting: the next gesture on
        // the page decides where the shape goes and how big it is. A click
        // without a drag still places the default size, so the gallery works for
        // someone who has never been told about the drag.
        ed.shapeDrawMode.arm(token);
        ed.focusEditorSurface();
      },
    });
    shapeGalleryMenu.style.setProperty("--shape-gallery-columns", String(SHAPE_GALLERY_COLUMNS));
  }

  const shapeGalleryPopover = registerPopover(ed.insertShapeBtn, shapeGalleryMenu, paintShapeGallery);

  /** Opens the gallery, wherever the command came from (ribbon button or palette). */
  function openShapeGallery() {
    if (!ed.doc) return;
    if (shapeGalleryMenu.hidden) openPopover(shapeGalleryPopover);
    queueMicrotask(() => shapeGalleryCells[0]?.focus());
  }

  // Arrow keys walk the grid, which is what makes 22 icon cells operable without a
  // pointer. The rule is `nextGalleryIndex`; this is the focus half.
  shapeGalleryMenu.addEventListener("keydown", (event) => {
    const index = shapeGalleryCells.indexOf(document.activeElement);
    const next = nextGalleryIndex(
      event.key,
      index,
      galleryRowStarts(SHAPE_GALLERY_COLUMNS),
      shapeGalleryCells.length,
    );
    if (next < 0) return;
    event.preventDefault();
    shapeGalleryCells[next]?.focus();
  });

  const bulletGalleryPopover = registerPopover(ed.bulletListMenuBtn, ed.bulletGalleryMenu, () =>
    ed.reflectListGallery(ed.bulletGalleryMenu),
  );
  const numberGalleryPopover = registerPopover(ed.numberedListMenuBtn, ed.numberGalleryMenu, () =>
    ed.reflectListGallery(ed.numberGalleryMenu),
  );
  /** Applies a list-marker spec ("bullet:•", "numbered:decimal", …) to the caret's
   *  list. Shared by the gallery cells and by the `paragraph.listFormat.*`
   *  commands, which are generated from the very same cells. */
  function applyListFormatCommand(spec) {
    if (!ed.selection || !ed.doc) return;
    const applied = ed.runNodeEdit(() => ed.doc.setListFormat(ed.selection.focus.node, spec));
    // A newly chosen bullet glyph may need its covering symbol font fetched
    // (once) so it renders instead of a .notdef box, then re-render.
    if (applied && spec.startsWith("bullet:")) void ed.ensureGlyphCoverage("list marker");
  }
  for (const [menu, popover] of [[ed.bulletGalleryMenu, bulletGalleryPopover], [ed.numberGalleryMenu, numberGalleryPopover]]) {
    wireListGallery(menu, popover, { canApply: () => !!ed.selection && !!ed.doc, closePopover, apply: applyListFormatCommand });
  }

  // -- Line & paragraph spacing --------------------------------------------------
  // Wiring lives in `spacing_menu.mjs`; the competitive reference and the reason
  // an inherited line spacing reads back as "from the style" are recorded there.
  const spacingMenuControl = createSpacingMenu({
    doc: () => ed.doc,
    focusNode: () => ed.selection?.focus?.node ?? null,
    runToolbarEdit: ed.runToolbarEdit,
    registerPopover,
    onButton,
    t,
  });

  return { applyListFormatCommand, openShapeGallery, spacingMenuControl };
}
