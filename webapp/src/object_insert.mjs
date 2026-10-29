// SPDX-License-Identifier: Apache-2.0
// The object-insert commands main.js reaches through ONE context getter
// (`ed.objectInsert`): Insert ▸ Picture from URL and Paste as picture.
//
// One seam rather than one getter per command because `main.js` is ratcheted
// (`module_seams.test.mjs`) and owned by another lane: the editor pays one
// import, one construction and one getter, and every surface — the ribbon, the
// menus, the palette, the context menu and the Paste Options chip — calls in
// through here.
import { createPasteAsPicture } from "./paste_picture.mjs";
import { createPictureUrlDialog } from "./picture_url_dialog.mjs";

/** Installs the object-insert commands on the editor context `ed`. Runs once,
 *  from `main.js`, after the clipboard and the picture-insert path exist. */
export function createObjectInsert(ed) {
  const pictureUrl = createPictureUrlDialog(ed);
  const { pasteAsPicture, pasteRunsAsPicture } = createPasteAsPicture(ed);
  return {
    /** Insert ▸ Picture from URL… */
    openPictureFromUrl: () => pictureUrl.open(),
    /** Edit ▸ Paste as picture. */
    pasteAsPicture,
    /** Paste Options ▸ Picture, and any caller that already holds runs. */
    pasteRunsAsPicture,
  };
}
