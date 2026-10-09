// Word's Change Picture, Docs' Replace image.
//
// Swapping a picture used to mean deleting it and inserting another, which
// threw away everything the reader had set on the object — its size, its
// position, its wrap, its border, its alt text — and made them set it all again
// (`docs/109` HF-252). The engine's `replacePicture` keeps the object and swaps
// the image, fitted inside the old frame at the new image's own proportions;
// this is the host half: the file picker, the decode, the one gated edit, and a
// status line that says what was kept.
//
// Reached from three surfaces, as `SKILL` §10 asks: the picture's own chip, its
// right-click menu, and — through the menu's flattening — the command palette.
//
// `io`:
//   `doc()`, `selection()`        the engine handle and the object selection
//   `runEdit(thunk, options)`      the one gated, undoable edit path
//   `blocked()`                    true (having said why) when the review mode or
//                                  the document refuses object edits
//   `setStatus(text, kind)`, `t(key, params)`
//   `createInput()`                a fresh `<input type=file>` (DOM seam)
//
// The decode and the type list live here too, and Insert ▸ Picture imports
// them: one list of what the engine can place, so Insert and Change can never
// accept different files (the engine's own `image_part_type` is the other half).

/** One EMU is 1/914400in; at 96dpi a CSS pixel is 9525 EMU. */
export const EMU_PER_PX = 9525;

/** The image types `insertImage` and `replacePicture` accept. */
export const INSERTABLE_IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/bmp",
  "image/tiff",
  "image/webp",
]);

/** Decodes a File/Blob to `{ bytes, widthPx, heightPx, mime }` via the browser
 *  (the engine owns no image codec, `docs/85` §Q8). */
export async function decodeImageBlob(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const bitmap = await createImageBitmap(blob);
  const widthPx = bitmap.width;
  const heightPx = bitmap.height;
  bitmap.close?.();
  return { bytes, widthPx, heightPx, mime: blob.type };
}

/**
 * Whether the selected object is a picture whose image can be changed.
 *
 * A picture is the kind the engine calls `"image"`; a chart or an embedded
 * object painted as its cached preview reports its own kind, and has no image of
 * its own to swap.
 *
 * O(1).
 */
export function canChangePicture(selection) {
  return !!selection && selection.mode === "selected" && selection.kind === "image";
}

export function createPictureReplace(io) {
  /** Replaces the selected picture's image with `file`. */
  async function replaceWith(file, node) {
    if (!file) return;
    if (!INSERTABLE_IMAGE_TYPES.has(String(file.type || "").toLowerCase())) {
      io.setStatus(io.t("object.changePicture.unsupported"), "error");
      return;
    }
    let decoded;
    try {
      decoded = await (io.decode ?? decodeImageBlob)(file);
    } catch {
      io.setStatus(io.t("object.changePicture.unreadable"), "error");
      return;
    }
    const widthEmu = Math.max(1, Math.round(decoded.widthPx * EMU_PER_PX));
    const heightEmu = Math.max(1, Math.round(decoded.heightPx * EMU_PER_PX));
    const hadAltText = !!io.doc()?.objectDescr?.(node);
    const applied = await io.runEdit(
      () => io.doc().replacePicture(node, decoded.bytes, widthEmu, heightEmu, decoded.mime),
      { gate: true },
    );
    if (applied === false) return;
    // Say what was KEPT, and ask for the one thing a person must check: alt
    // text written for the old picture may not describe the new one.
    io.setStatus(io.t(hadAltText ? "object.changePicture.doneAltText" : "object.changePicture.done"));
  }

  return {
    /** Opens the file picker for the selected picture. Refuses — with the
     *  reason — before the picker opens, so nobody chooses a file only to be
     *  told afterwards that nothing could have happened. */
    choose() {
      const selection = io.selection();
      if (!io.doc() || !canChangePicture(selection)) return;
      if (io.blocked()) return;
      const node = selection.node;
      const input = io.createInput();
      input.type = "file";
      input.accept = [...INSERTABLE_IMAGE_TYPES].join(",");
      input.addEventListener("change", () => {
        void replaceWith(input.files?.[0], node);
      });
      input.click();
    },
    replaceWith,
  };
}
