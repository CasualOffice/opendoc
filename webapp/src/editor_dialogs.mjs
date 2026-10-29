// The editor's small dialogs and insert pickers: the keyboard shortcuts
// reference, About, the bookmark manager, Insert field, Insert picture, the
// caption and cross-reference commands, and the Symbol / Emoji pickers.
// Moved out of `main.js` unchanged (docs/177); it reaches the rest of the
// editor only through the context `ed`.
import { engineVersion } from "../pkg/casual_doc_wasm.js";
import { editRefusalMessage, readRevision } from "./edit_errors.mjs";
import { createAboutDialog } from "./about_dialog.mjs";
import { createBookmarkManager } from "./bookmark_manager.mjs";
import { createDropCapDialog } from "./drop_cap.mjs";
import { createReferenceCommands } from "./reference_commands.mjs";
import { createTocCommands } from "./toc_commands.mjs";
import { EMU_PER_PX, INSERTABLE_IMAGE_TYPES, createPictureReplace, decodeImageBlob } from "./picture_replace.mjs";
import { createGlyphPicker } from "./glyph_picker.mjs";
import { EMOJI_GROUPS, SYMBOL_GROUPS } from "./glyph_sets.mjs";
import { renderShortcutsReference, shortcutGroups } from "./shortcuts_reference.mjs";
import { formatShortcut, navigationShortcuts } from "./keyboard.mjs";
import { registerModal } from "./modal.mjs";
import { shapeNameKey } from "./shape_catalogue.mjs";
import { t } from "./i18n.mjs";
import { newestObject, placedObjectIds } from "./placed_objects.mjs";
import { EMU_PER_TWIP } from "./units.mjs";
import { fieldLabel, fieldResultText } from "./field_kinds.mjs";
import { authorNameFor } from "./host_identity.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createDialogs(ed) {
  // ---- Keyboard shortcuts reference (HF-151) ---------------------------------
  // Help offered "Keyboard shortcuts and commands", which opened the palette — a
  // launcher, not a reference. Nothing in the product ever told a user what the
  // chords WERE. Generated from the command registry plus the caret-movement
  // table in `keyboard.mjs`, so it can neither list a shortcut that does not
  // exist nor miss one that does, and every chord is rendered for the keyboard in
  // front of the user rather than in Apple glyphs on every platform.
  const shortcutsDialog = document.getElementById("shortcutsDialog");
  const shortcutsBody = document.getElementById("shortcutsBody");
  const shortcutsClose = document.getElementById("shortcutsClose");

  function buildShortcutsReference() {
    if (!shortcutsBody) return;
    renderShortcutsReference(
      shortcutsBody,
      shortcutGroups(ed.editorCommands({ surface: "palette" }), navigationShortcuts(), formatShortcut),
    );
  }

  const shortcutsModal = shortcutsDialog
    ? registerModal(shortcutsDialog, {
        initialFocus: () => shortcutsClose,
        fallbackFocus: () => ed.pagesEl,
      })
    : null;

  function toggleShortcutsReference(open) {
    if (!shortcutsModal) return;
    if (open) {
      // Built on open, not once at boot: the registry's labels move with the
      // document (undo names its action) and the roster grows with the styles.
      buildShortcutsReference();
      shortcutsModal.open();
    } else {
      shortcutsModal.close();
    }
  }
  for (const b of shortcutsDialog?.querySelectorAll("[data-shortcuts-dismiss]") ?? []) b.addEventListener("click", () => toggleShortcutsReference(false));

  // ---- About -----------------------------------------------------------------
  const toggleAbout = createAboutDialog(engineVersion, () => ed.pagesEl);

  // ---- Bookmark manager ------------------------------------------------------
  // The dialog is `bookmark_manager.mjs`; what is left here is the two things
  // that are not about bookmarks — the review-mode gate, which is about review
  // mode, and the repaint, which is about pages.

  /** True (after surfacing the standard read-only/untracked message) if a bookmark
   *  mutation must not apply in the current review mode. Bookmark markers are a
   *  structural model change with no tracked-revision representation, so like table
   *  insertion they are blocked in Viewing (read-only) and Suggesting (untracked). */
  function bookmarkMutationBlocked() {
    return ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting();
  }

  /** Repaints after a bookmark edit and marks the document dirty, WITHOUT moving
   *  the caret: the engine's create/rename/delete rest the caret at a marker or the
   *  document root, but the user's selection should stay put (the manager is a
   *  side panel, not a navigation). */
  async function applyBookmarkEdit(res) {
    const dirty = res.dirtyPages;
    const newCount = res.pageCount;
    const revision = readRevision(res);
    res.free();
    ed.noteDocumentEdited(revision);
    if (newCount !== ed.pages.length) {
      await ed.renderAll();
    } else {
      for (const i of dirty) ed.repaintPage(i);
      ed.drawSelection();
    }
    ed.scheduleChromeRefresh({ stats: true, outline: true });
  }

  const bookmarkManager = createBookmarkManager({
    getDoc: () => ed.doc,
    applyEdit: applyBookmarkEdit,
    mutationBlocked: bookmarkMutationBlocked,
    hasRange: ed.hasRange,
    selectionEndpoints: ed.selEndpoints,
    navigateTo: (position) => ed.navToPosition(position, false),
    status: (text, kind) => ed.setStatus(text, kind),
    registerModal,
    fallbackFocus: () => ed.pagesEl,
  });

  function openBookmarkManager() {
    bookmarkManager.open();
  }

  // ---- Insert field ----------------------------------------------------------
  // A Word/Docs-style field inserter over the engine's insertField op (one
  // undoable "Field change"). The kind table and the host-side result formatter
  // are `field_kinds.mjs`; the insert mirrors the bookmark/table structural
  // inserts: blocked in Viewing (read-only) and Suggesting (no tracked-revision
  // representation yet).

  /** Inserts a common field at the caret as a single undoable "Field change".
   *  Fails closed exactly like the bookmark/table inserts (`blockMutationInViewing`
   *  + `blockUntrackedInSuggesting`), then applies the EditResult through the shared
   *  caret-position path so the field renders inline, the caret lands after it, and
   *  Undo/Redo treats it as one action. */
  async function insertFieldAtCaret(kind) {
    // A null `selection` here means no document is open, not "click first" — an
    // open document always carries an insertion point. Defensive guard only; the
    // command is not reachable without a document.
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    ed.breakTypingSession();
    if (ed.blockUntrackedInSuggesting()) return;
    const { node, offset } = ed.selection.focus;
    let res;
    try {
      res = ed.doc.insertField(node, offset, kind, fieldResultText(kind, { fileName: ed.currentName, authorName: authorNameFor(ed.settings) }));
    } catch (err) {
      console.warn("insertField ignored:", err?.message ?? err);
      ed.setStatus("A field can’t be inserted at this position", "error");
      ed.focusEditorSurface();
      return;
    }
    await ed.applyEditResult(res);
    ed.setStatus(`Inserted ${fieldLabel(kind)}`);
    ed.focusEditorSurface();
  }

  // ---- Insert picture ----------------------------------------------------------
  // The engine owns no image codec (docs/85 §Q8), so the host decodes the image to
  // bytes + natural pixel size (`picture_replace.mjs`, shared with Change Picture)
  // and hands them to the `insertImage` op. A wide image is scaled down to fit the
  // text column, preserving aspect.
  const MAX_IMAGE_WIDTH_EMU = 6 * 914_400; // ~6in, a sane default display width

  /** Inserts an already-decoded image at the caret as one undoable action, gated
   *  like the other object edits (read-only in Viewing, blocked in Suggesting). */
  async function insertImageAtCaret(bytes, widthPx, heightPx, mime) {
    // Defensive only — see `insertFieldAtCaret`. The picture lands at the caret,
    // which on a document nobody has clicked into is the start of the body.
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    ed.breakTypingSession();
    if (ed.blockUntrackedInSuggesting()) return;
    let widthEmu = Math.max(1, Math.round(widthPx * EMU_PER_PX));
    let heightEmu = Math.max(1, Math.round(heightPx * EMU_PER_PX));
    if (widthEmu > MAX_IMAGE_WIDTH_EMU) {
      heightEmu = Math.round((heightEmu * MAX_IMAGE_WIDTH_EMU) / widthEmu);
      widthEmu = MAX_IMAGE_WIDTH_EMU;
    }
    const { node, offset } = ed.selection.focus;
    const before = placedObjectIds(ed.doc);
    let res;
    try {
      res = ed.doc.insertImage(node, offset, bytes, widthEmu, heightEmu, mime);
    } catch (err) {
      console.warn("insertImage ignored:", err?.message ?? err);
      ed.setStatus("This picture can’t be inserted here", "error");
      ed.focusEditorSurface();
      return;
    }
    await ed.applyEditResult(res);
    // Word, Docs and ONLYOFFICE all leave an inserted picture SELECTED, with its
    // handles and its toolbar, the same rule a shape and a chart already follow
    // (`docs/109` OL-03).
    const picture = newestObject(ed.doc, before, "image");
    if (picture) ed.selectObject(picture.node, picture.kind, null, picture.anchored, picture);
    ed.setStatus("Picture inserted");
    ed.focusEditorSurface();
  }

  /** Decodes a File/Blob (a picked file or a pasted image) and inserts it. */
  async function insertImageFromBlob(blob) {
    if (!ed.doc) return;
    if (!INSERTABLE_IMAGE_TYPES.has((blob.type || "").toLowerCase())) {
      ed.setStatus("That image format isn’t supported", "error");
      return;
    }
    try {
      const decoded = await decodeImageBlob(blob);
      await insertImageAtCaret(decoded.bytes, decoded.widthPx, decoded.heightPx, decoded.mime);
    } catch (err) {
      console.warn("image decode failed:", err);
      ed.setStatus("Could not read that image", "error");
    }
  }

  /** Insert ▸ Picture: opens a file picker and inserts the chosen image at the
   *  insertion point — the caret if the user placed one, otherwise the start of
   *  the body, exactly as Word and Google Docs do. */
  /** Word's Insert ▸ Text Box. Inserts a floating box at the caret and enters it,
   *  so the user types where they just clicked instead of hunting for the box. */
  async function insertTextBoxObject() {
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Inserting a text box cannot be tracked yet; switch to Editing", "error");
      return;
    }
    const before = placedObjectIds(ed.doc);
    let result;
    try {
      result = ed.doc.insertTextBox(ed.selection.focus.node, ed.selection.focus.offset);
    } catch (err) {
      ed.setStatus(`Could not insert the text box: ${err.message ?? err}`, "error");
      return;
    }
    await ed.applyEditResult(result);
    // Land INSIDE the box through the SAME path a double-click uses, so entry can
    // never diverge between the two ways of getting there.
    const box = newestObject(ed.doc, before, "textbox");
    if (box) {
      ed.selectObject(box.node, box.kind, null, box.anchored, box);
      ed.enterObjectEditMode();
    }
    ed.setStatus("Text box added — type its text");
    ed.focusEditorSurface();
  }

  /** Word's Insert ▸ Shapes. Inserts a floating preset shape and leaves it
   *  SELECTED (not entered) — a shape has no text until you ask for some.
   *
   *  `at` is the rectangle the user DREW, in page-local twips, or null for the
   *  plain command. `insertShape` authors a fixed 2"x1" at the caret's paragraph,
   *  so the drawn geometry is applied by the resize that follows — one more
   *  undoable action, which is why the two are reported as one gesture in the
   *  status line rather than as two. */
  async function insertShapeObject(geometry, at = null) {
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Inserting a shape cannot be tracked yet; switch to Editing", "error");
      return;
    }
    const before = placedObjectIds(ed.doc);
    let result;
    try {
      result = ed.doc.insertShape(ed.selection.focus.node, ed.selection.focus.offset, geometry);
    } catch (err) {
      ed.setStatus(`Could not insert the shape: ${err.message ?? err}`, "error");
      return;
    }
    await ed.applyEditResult(result);
    // Word leaves a new shape SELECTED, not entered — which also puts Fill and
    // Outline within reach straight away.
    const shape = newestObject(ed.doc, before, "shape");
    if (shape) ed.selectObject(shape.node, shape.kind, null, shape.anchored, shape);
    if (at && shape) {
      try {
        await ed.applyEditResult(
          ed.doc.resizeObject(
            // The ROOT, not the leaf: `insertShape` wraps the shape in a
            // group-of-one (the only shape a shape takes in this model), and
            // geometry belongs to the group. Resizing the leaf is refused.
            shape.ref?.root ?? shape.root ?? shape.node,
            at.left * EMU_PER_TWIP,
            at.top * EMU_PER_TWIP,
            at.width * EMU_PER_TWIP,
            at.height * EMU_PER_TWIP,
          ),
        );
      } catch (err) {
        // The shape exists at its default size; saying so beats silence.
        ed.setStatus(editRefusalMessage(err, { routeRefusal: ed.SESSION.sentenceFor }), "error");
      }
    }
    const key = shapeNameKey(geometry);
    ed.setStatus(t("shape.added", { shape: key ? t(key) : "Shape" }));
    ed.focusEditorSurface();
  }

  function insertImageFromFile() {
    if (!ed.doc || !ed.selection) return;
    if (ed.blockMutationInViewing()) return;
    const input = document.createElement("input");
    input.type = "file";
    input.accept = [...INSERTABLE_IMAGE_TYPES].join(",");
    input.addEventListener("change", () => {
      const file = input.files?.[0];
      if (file) void insertImageFromBlob(file);
    });
    input.click();
  }

  /** Word's Change Picture (`picture_replace.mjs`, HF-252). */
  const pictureReplace = createPictureReplace({
    doc: () => ed.doc,
    selection: () => ed.objectSelection,
    runEdit: (thunk, options) => ed.runEdit(thunk, { ...options, keepView: true }),
    blocked: () => ed.objectEditBlocked(),
    setStatus: (text, kind) => ed.setStatus(text, kind),
    t,
    createInput: () => document.createElement("input"),
  });

  const fieldDialog = document.getElementById("fieldDialog");
  const fieldList = document.getElementById("fieldList");
  const fieldClose = document.getElementById("fieldClose");
  const fieldCancel = document.getElementById("fieldCancel");

  function fieldChoiceButtons() {
    return fieldList ? [...fieldList.querySelectorAll(".field-choice")] : [];
  }

  const fieldModal = fieldDialog
    ? registerModal(fieldDialog, {
        initialFocus: () => fieldChoiceButtons()[0],
        fallbackFocus: () => ed.pagesEl,
      })
    : null;

  /** Opens the field picker against the current insertion point; the first choice
   *  is focused for keyboard use. Needs only an open document — Word's Insert ▸
   *  Quick Parts ▸ Field is never gated on having clicked into the page first. */
  function openFieldDialog() {
    if (!ed.doc || !fieldDialog) return;
    // Viewing is read-only: refuse before opening, as the symbol and emoji pickers
    // do, so the dialog never opens onto an insert that cannot happen. Promoting
    // Field to the ribbon is what makes this reachable without a deliberate detour
    // through the palette, and picking a field only to be told no is a worse
    // answer than not opening.
    if (ed.blockMutationInViewing()) return;
    fieldModal.open();
  }

  function closeFieldDialog() {
    fieldModal?.close();
  }

  /** Closes the picker, then inserts — insertFieldAtCaret ends by focusing the
   *  editor surface so the caret (now after the field) is ready for typing. */
  function chooseFieldFromDialog(kind) {
    closeFieldDialog();
    insertFieldAtCaret(kind);
  }

  if (fieldDialog) {
    for (const button of fieldChoiceButtons()) {
      button.addEventListener("click", () => chooseFieldFromDialog(button.dataset.fieldKind));
    }
    fieldClose.addEventListener("click", () => closeFieldDialog());
    fieldCancel.addEventListener("click", () => closeFieldDialog());
    fieldDialog.addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const items = fieldChoiceButtons();
        const index = items.indexOf(document.activeElement);
        const dir = event.key === "ArrowDown" ? 1 : -1;
        items[(index + dir + items.length) % items.length]?.focus();
      }
    });
  }

  const dropCapDialog = createDropCapDialog({
    registerModal, getDoc: () => ed.doc, selectionNode: () => ed.selection?.focus.node ?? "",
    mutationBlocked: () => ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting(),
    apply: (mode, lines) => ed.runEdit(() => ed.doc.setDropCap(ed.selection.focus.node, mode, lines), { gate: true }),
    status: ed.setStatus, fallbackFocus: () => ed.pagesEl,
  });

  // ---- References ▸ captions and cross-references (OO-005) --------------------
  // Both dialogs and the caption-numbering cache are `reference_commands.mjs`; what
  // is here is the application state they read. Every insert goes through the SAME
  // gated, tracked, undoable path as every other edit. The caption's target is the
  // caret's block — selecting a picture or table puts the caret at its anchor.
  const referenceCommands = createReferenceCommands({
    registerModal, getDoc: () => ed.doc, status: ed.setStatus, fallbackFocus: () => ed.pagesEl,
    mutationBlocked: () => ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting(),
    targetNode: () => ed.selection?.focus.node ?? "",
    caret: () => (ed.selection ? { node: ed.selection.focus.node, offset: ed.selection.focus.offset } : null),
    applyEdit: (run) => ed.runEdit(run, { gate: true }),
  });

  // References ▸ Table of contents. Same shape and the same cache discipline as
  // the caption numbering beside it: the count the ribbon reads is refreshed on
  // the deliberate interactions that change it, never on the keystroke path.
  const tocCommands = createTocCommands({
    registerModal, getDoc: () => ed.doc, status: ed.setStatus, fallbackFocus: () => ed.pagesEl,
    mutationBlocked: () => ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting(),
    caretNode: () => ed.selection?.focus.node ?? "",
    selection: () => ed.selection ?? null,
    applyEdit: (run) => ed.runEdit(run, { gate: true }),
    changed: () => ed.updateToolbar(),
  });

  // ---- Insert ▸ Symbol / Emoji pickers ---------------------------------------
  // Word's Insert ▸ Symbol and Docs' Insert ▸ Special characters / emoji, built to
  // that standard: a categorized grid of curated glyphs. Clicking a glyph inserts
  // it at the caret and KEEPS the dialog open (Word/Docs behavior) so the user can
  // add several; Done / Esc / the close button dismiss. Insertion reuses the same
  // gated, tracked, undoable text path as paste (`pasteText` → `insertPlainTextAs`
  // with the "paste" HistoryKind, which never coalesces): each glyph is ONE undo,
  // fails closed read-only in Viewing, and routes through the suggestion path in
  // Suggesting — no engine change, an ordinary text edit. The curated sets are in
  // `glyph_sets.mjs` and the dialog itself is in `glyph_picker.mjs`; what is left
  // here is the insertion and the two one-line openers the commands call.

  /** Inserts a glyph (symbol or emoji) at the caret through the shared gated,
   *  tracked, undoable text path. `pasteText(glyph, "paste")` fails closed in
   *  Viewing, routes through the suggestion path in Suggesting, and records ONE
   *  non-coalescing history entry per call (the "paste" HistoryKind never merges),
   *  so every glyph is exactly one Undo. The picker stays open (Word/Docs). */
  async function insertGlyphAtCaret(glyph) {
    // Defensive only: an open document always has an insertion point, so the
    // glyph lands at the caret, or at the start of the body if none was placed.
    if (!ed.doc || !glyph || !ed.selection) return;
    // A whitespace glyph (the no-break space and its kin in Special characters)
    // goes in as typed: the paste path normalises whitespace and drops a lone
    // space character, which is exactly what this glyph exists not to be.
    await ed.pasteText(glyph, /^\s$/u.test(glyph) ? "typing" : "paste");
  }

  /** Builds a categorized glyph picker (symbol or emoji) over an existing dialog
   *  skeleton in the markup. Returns `{ open }`; the controller owns tab switching,
   *  keyword search, roving arrow-key grid navigation, insertion (keep-open), and
   *  Esc / backdrop / Done dismissal — mirroring the field dialog's focus-trap and
   *  return-focus contract. */

  const glyphPickerHost = {
    insertGlyph: insertGlyphAtCaret,
    blockedInViewing: ed.blockMutationInViewing,
    returnFocusToEditor: ed.focusEditorSurface,
    isDocumentOpen: () => ed.doc !== null,
  };
  const symbolPicker = createGlyphPicker({
    dialogId: "symbolDialog", gridId: "symbolGrid", tabsId: "symbolTabs", searchId: "symbolSearch",
    emptyId: "symbolEmpty", closeId: "symbolClose", doneId: "symbolDone", groups: SYMBOL_GROUPS,
    tabsAreEmoji: false, triggerId: "insertSymbolBtn", ...glyphPickerHost,
  });
  const emojiPicker = createGlyphPicker({
    dialogId: "emojiDialog", gridId: "emojiGrid", tabsId: "emojiTabs", searchId: "emojiSearch",
    emptyId: "emojiEmpty", closeId: "emojiClose", doneId: "emojiDone", groups: EMOJI_GROUPS,
    tabsAreEmoji: true, triggerId: "insertEmojiBtn", ...glyphPickerHost,
  });

  function openSymbolPicker() {
    symbolPicker.open();
  }

  function openEmojiPicker() {
    emojiPicker.open();
  }

  return {
    bookmarkManager, buildShortcutsReference, dropCapDialog, insertFieldAtCaret,
    insertImageFromBlob, insertImageFromFile, insertShapeObject, insertTextBoxObject,
    openBookmarkManager, openEmojiPicker, openFieldDialog, openSymbolPicker, pictureReplace,
    referenceCommands, tocCommands, toggleAbout, toggleShortcutsReference,
  };
}
