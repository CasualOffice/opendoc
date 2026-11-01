// Editing the text in a picture (`docs/182` §20b): what the editor does
// around the engine's image-text line boxes (`casual-doc-wasm`'s
// `image_text.rs`).
//
// A scanned PDF page opens as its picture with a line box over each line OCR
// read, its text laid out but not painted: the page shows the scan's own
// pixels, and a misread shows only once that line is edited. This module is
// the one place that knows that:
//
//   * `prepare()` runs at the edit choke point (`runEdit` for text edits,
//     `runToolbarEdit` for formatting) BEFORE the edit. When the caret's text
//     is unpainted (`caretTextUnpainted`, the cost of `caretFormat`, no
//     document walk) and the box is over a picture, the box is made editable
//     first (`editImageText`): the ink under it is erased in a patch picture
//     and its text is painted in the ink's measured colour, height and
//     weight. One undo step of its own, so undo takes the typing away first
//     and then the patch: the original pixels, exactly.
//   * The commands (palette, and the right-click menu over a line box): the
//     three ways the original text is taken away under an edited line (Fill,
//     the default, Blur, Pixelate: what Acrobat's and Photos' editors offer)
//     and Restore original. A choice made for one line becomes the default
//     for the next (a per-viewer preference). Each is DISABLED WITH THE
//     REASON outside a line box (SKILL §10).
//
// Nothing here is shown until a document has image text: the commands sit in
// the palette and appear in the context menu only over a line box (owner
// rule: new features must not degrade the UI).
import { t } from "./i18n.mjs";
import { readPref, writePref } from "./prefs.mjs";

/** The per-viewer preference naming the fill an edit erases with. */
export const IMAGE_TEXT_MODE_PREF = "opendoc.imageText.mode";
/** The fills, in menu order. */
export const IMAGE_TEXT_MODES = Object.freeze(["fill", "blur", "pixelate"]);
/** The command ids. */
export const IMAGE_TEXT_COMMANDS = Object.freeze({
  fill: "imageText.fill",
  blur: "imageText.blur",
  pixelate: "imageText.pixelate",
  restore: "imageText.restore",
});

/** The stored mode, or `fill`. Pure over `view`. */
export function imageTextMode(view = globalThis) {
  const stored = readPref(IMAGE_TEXT_MODE_PREF, "fill", view);
  return IMAGE_TEXT_MODES.includes(stored) ? stored : "fill";
}

/** Why a picture-text command cannot run in `state` (`""`, `"unedited"`,
 *  `"edited"`), as a catalogue key, or `""` when it can. Pure. */
export function imageTextRefusal(command, state) {
  if (!state) return "imageText.reason.noBox";
  if (command === IMAGE_TEXT_COMMANDS.restore && state !== "edited") return "imageText.reason.notEdited";
  return "";
}

/**
 * @param {object} ed the editor context.
 * @param {{ applyEditResult: (res: object, options?: object) => Promise<void> }} io
 */
export function createImageTextEdit(ed, { applyEditResult }) {
  const caret = () => ed.selection?.focus ?? null;

  /** The state of the line box at the caret (O(document), so never per
   *  keystroke: only when a menu or the palette is built). */
  function atCaret() {
    const at = caret();
    if (!ed.doc || !at || typeof ed.doc.imageTextState !== "function") return "";
    try {
      return ed.doc.imageTextState(at.node) || "";
    } catch {
      return "";
    }
  }

  /** Erases the ink under the caret's line box by `mode`; the caret stays. */
  async function edit(mode) {
    const at = caret();
    if (!ed.doc || !at) return false;
    let res;
    try {
      res = ed.doc.editImageText(at.node, at.offset, mode);
    } catch (error) {
      ed.setStatus(t(String(error?.message ?? error)), "error");
      return false;
    }
    await applyEditResult(res, { keepSelection: true, keepView: true });
    return true;
  }

  return {
    atCaret,
    /** Before a text edit at the caret: makes an unedited line box editable.
     *  Always lets the edit go on (a refusal is said, not silent). */
    async prepare() {
      const at = caret();
      const doc = ed.doc;
      if (!doc || !at || typeof doc.caretTextUnpainted !== "function") return true;
      let unpainted = false;
      try {
        unpainted = doc.caretTextUnpainted(at.node, at.offset);
      } catch {
        return true;
      }
      if (!unpainted || atCaret() !== "unedited") return true;
      await edit(imageTextMode());
      return true;
    },
    /** The registry rows: the palette and the right-click menu. */
    commands(state = atCaret()) {
      const fill = (mode) => {
        const refusal = imageTextRefusal(IMAGE_TEXT_COMMANDS[mode], state);
        return {
          id: IMAGE_TEXT_COMMANDS[mode],
          label: { fill: t("imageText.fill.command"), blur: t("imageText.blur.command"), pixelate: t("imageText.pixelate.command") }[mode],
          group: "Picture text",
          kw: `picture image scan text edit erase background ${mode} ocr`,
          enabled: !refusal,
          disabledReason: refusal ? t(refusal) : "",
          run: () => {
            writePref(IMAGE_TEXT_MODE_PREF, mode);
            void edit(mode);
          },
        };
      };
      const restoreRefusal = imageTextRefusal(IMAGE_TEXT_COMMANDS.restore, state);
      return [
        ...IMAGE_TEXT_MODES.map(fill),
        {
          id: IMAGE_TEXT_COMMANDS.restore,
          label: t("imageText.restore.command"),
          group: "Picture text",
          kw: "picture image scan text restore original revert undo erase",
          enabled: !restoreRefusal,
          disabledReason: restoreRefusal ? t(restoreRefusal) : "",
          run: async () => {
            const at = caret();
            if (!ed.doc || !at) return;
            let res;
            try {
              res = ed.doc.restoreImageText(at.node);
            } catch (error) {
              ed.setStatus(t(String(error?.message ?? error)), "error");
              return;
            }
            await applyEditResult(res, { keepView: true });
          },
        },
      ];
    },
  };
}
