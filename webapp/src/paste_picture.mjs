// SPDX-License-Identifier: Apache-2.0
// Paste as picture (`docs/153` `text.paste-as-picture`).
//
// Word's Paste Special ▸ Picture and its Paste Options ▸ Picture: whatever is
// on the clipboard goes in as ONE inline picture of how it looks, so it can be
// placed, sized and cropped like any picture and can no longer be reflowed or
// restyled by the document around it.
//
// THE ENGINE DRAWS IT (`casual-doc-wasm`'s `picture_insert`): the clipboard's
// runs are laid out at the width of the column the caret is in, with this
// document's styles and fonts, painted by the same CPU renderer that paints
// pages, and trimmed to their ink. The browser alternative — an HTML fragment
// drawn through an SVG `foreignObject` into a canvas — paints with the
// browser's fonts and CSS rather than the document's, taints the canvas on any
// cross-origin image, and would show something the text paste would not have.
//
// If the clipboard already holds a picture (a screenshot), that picture is
// pasted as it is: there is nothing to rasterise.
//
// One undoable action: the rasterise call does not touch the document; the
// insert is `insertImage`, through `insertImageFromBlob`, the path every other
// picture takes (gated in Viewing and Suggesting, left selected afterwards).
import { extractMarker, htmlToRuns } from "./clipboard.mjs";
import { t } from "./i18n.mjs";

/** The resolution a paste-as-picture is painted at: at least twice CSS
 *  resolution, so it prints cleanly, and the screen's own when that is higher,
 *  capped where the engine caps it. */
export function pictureDpi(devicePixelRatio = 1) {
  const ratio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return Math.min(288, 96 * Math.max(2, ratio));
}

/** Plain text as clipboard runs: one paragraph per line. O(text). */
export function textToRuns(text) {
  const runs = [];
  String(text ?? "")
    .split(/\r\n|\r|\n/)
    .forEach((line, index) => {
      if (index > 0) runs.push({ paragraphBreak: true });
      if (line) runs.push({ text: line });
    });
  return runs;
}

/** The runs an internal round-trip marker carries (an OpenDoc-to-OpenDoc copy),
 *  or null. The marker is either the runs array or `{ blocks, runs }`. */
export function markerRuns(markerJson) {
  let parsed = null;
  try {
    parsed = JSON.parse(markerJson);
  } catch {
    return null;
  }
  if (Array.isArray(parsed)) return parsed;
  return Array.isArray(parsed?.runs) ? parsed.runs : null;
}

/** Catalogue keys for the engine's refusal codes. */
const REFUSAL_KEYS = Object.freeze({
  "paste.picture-empty": "pastePicture.empty",
  "paste.picture-too-large": "pastePicture.tooLarge",
});

/**
 * @param {object} ed the editor context: `doc`, `selection`, `reviewMode`,
 *        `blockMutationInViewing()`, `blockUntrackedInSuggesting()`,
 *        `insertImageFromBlob(blob, size)`, `setStatus(text, kind)`.
 * @param {object} [io] seams: `clipboard` (defaults to `navigator.clipboard`),
 *        `devicePixelRatio()`.
 */
export function createPasteAsPicture(ed, io = {}) {
  const clipboard = () => io.clipboard ?? globalThis.navigator?.clipboard;
  const ratio = () => io.devicePixelRatio?.() ?? globalThis.devicePixelRatio ?? 1;

  /** Refuses (saying why) where a picture cannot be inserted. */
  function blocked() {
    if (!ed.doc || !ed.selection) return true;
    if (ed.blockMutationInViewing()) return true;
    return ed.blockUntrackedInSuggesting();
  }

  /** Lays `runs` out, paints them and inserts the picture at the caret.
   *  `beforeInsert` runs after the picture is painted and before it is inserted. */
  async function pasteRunsAsPicture(runs, { beforeInsert = null } = {}) {
    if (blocked()) return false;
    if (!Array.isArray(runs) || !runs.some((run) => run.text && run.text.trim())) {
      ed.setStatus(t("pastePicture.empty"), "error");
      return false;
    }
    let raster;
    try {
      raster = ed.doc.rasterizeClipboard(ed.selection.focus.node, JSON.stringify(runs), pictureDpi(ratio()));
    } catch (err) {
      ed.setStatus(t(REFUSAL_KEYS[err?.code] ?? "pastePicture.failed"), "error");
      return false;
    }
    const png = raster.png;
    const scale = 96 / raster.dpi;
    const size = { widthPx: raster.widthPx * scale, heightPx: raster.heightPx * scale };
    raster.free?.();
    // Paste Options ▸ Picture undoes the text paste HERE, once the picture exists:
    // a refusal above leaves the paste the person already has.
    if (beforeInsert) await beforeInsert();
    await ed.insertImageFromBlob(new Blob([png], { type: "image/png" }), size);
    return true;
  }

  /** Edit ▸ Paste as picture: reads the clipboard and pastes it as a picture. */
  async function pasteAsPicture() {
    if (blocked()) return;
    const board = clipboard();
    let items = [];
    let refused = 0;
    try {
      items = board?.read ? await board.read() : [];
    } catch {
      items = [];
      refused += 1;
    }
    // A picture on the clipboard is pasted as itself.
    for (const item of items) {
      const imageType = item.types.find((type) => type.startsWith("image/"));
      if (!imageType) continue;
      await ed.insertImageFromBlob(await item.getType(imageType));
      return;
    }
    for (const item of items) {
      if (!item.types.includes("text/html")) continue;
      const html = await (await item.getType("text/html")).text();
      const marker = extractMarker(html);
      const runs = marker ? markerRuns(marker) : htmlToRuns(new DOMParser().parseFromString(html, "text/html").body);
      if (runs?.length) {
        await pasteRunsAsPicture(runs);
        return;
      }
    }
    let text = "";
    try {
      text = (await board?.readText?.()) ?? "";
    } catch {
      text = "";
      refused += 1;
    }
    if (text) {
      await pasteRunsAsPicture(textToRuns(text));
      return;
    }
    // Both reads refused is the browser's permission, not an empty clipboard.
    ed.setStatus(t(refused === 2 ? "pastePicture.blocked" : "pastePicture.nothing"), "error");
  }

  return { pasteAsPicture, pasteRunsAsPicture };
}
