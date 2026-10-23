// The Insert / edit link dialog: a web address or a place in this document,
// its text and tooltip, Apply and Remove. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { registerModal } from "./modal.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createLinkDialog(ed) {
  // ---- Insert / edit link dialog ---------------------------------------------
  // A Word/Docs-style hyperlink dialog: a "Text to display" field, a Web-address /
  // Place-in-this-document target picker (the picker is populated from the
  // document's bookmarks so a user never hand-types "#name"), and an optional
  // ScreenTip. It replaces the old window.prompt UI and applies through the very
  // same gated engine ops (`setHyperlink`/`removeHyperlink`, one undoable action,
  // fail-closed in Viewing/Suggesting). Changing the display text re-applies the
  // link via the rich-run paste op so text + link stay one undoable action.
  const linkDialog = document.getElementById("linkDialog");
  const linkDialogForm = document.getElementById("linkDialogForm");
  const linkDialogTitle = document.getElementById("linkDialogTitle");
  const linkTextInput = document.getElementById("linkTextInput");
  const linkUrlInput = document.getElementById("linkUrlInput");
  const linkPlaceSelect = document.getElementById("linkPlaceSelect");
  const linkPlaceEmpty = document.getElementById("linkPlaceEmpty");
  const linkTooltipInput = document.getElementById("linkTooltipInput");
  const linkModeUrl = document.getElementById("linkModeUrl");
  const linkModePlace = document.getElementById("linkModePlace");
  const linkModeUrlPanel = document.getElementById("linkModeUrlPanel");
  const linkModePlacePanel = document.getElementById("linkModePlacePanel");
  const linkDialogNote = document.getElementById("linkDialogNote");
  const linkDialogClose = document.getElementById("linkDialogClose");
  const linkCancelBtn = document.getElementById("linkCancelBtn");
  const linkRemoveBtn = document.getElementById("linkRemoveBtn");
  const LINK_DIALOG_HINT = "Enter applies; Esc cancels.";
  /** The range a currently-open link dialog targets. `null` when it is closed. */
  let linkDialogCtx = null;
  let linkDialogMode = "url";

  function setLinkDialogNote(message, isError) {
    linkDialogNote.textContent = message || LINK_DIALOG_HINT;
    linkDialogNote.classList.toggle("error", !!isError && !!message);
  }

  /** Switches the target picker between an external "Web address" field and the
   *  document's "Place in this document" bookmark dropdown. */
  function setLinkDialogMode(mode) {
    linkDialogMode = mode === "place" ? "place" : "url";
    const place = linkDialogMode === "place";
    linkModeUrl.setAttribute("aria-selected", String(!place));
    linkModePlace.setAttribute("aria-selected", String(place));
    linkModeUrl.classList.toggle("is-active", !place);
    linkModePlace.classList.toggle("is-active", place);
    linkModeUrlPanel.hidden = place;
    linkModePlacePanel.hidden = !place;
  }

  /** Fills the place dropdown from the document's bookmarks (headings are not
   *  offered: the engine's link target is a bookmark anchor or URL, and
   *  auto-bookmarking a heading would be a second, separate undoable op). Returns
   *  the bookmark count; `selectedAnchor` pre-selects an existing internal link. */
  function populateLinkPlaces(selectedAnchor) {
    const entries = ed.bookmarkManager.entries();
    linkPlaceSelect.replaceChildren();
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = entries.length ? "Choose a bookmark…" : "No bookmarks in this document";
    linkPlaceSelect.append(placeholder);
    for (const { name } of entries) {
      const option = document.createElement("option");
      option.value = `#${name}`;
      option.textContent = name;
      if (selectedAnchor && name === selectedAnchor) option.selected = true;
      linkPlaceSelect.append(option);
    }
    linkPlaceSelect.disabled = entries.length === 0;
    linkPlaceEmpty.hidden = entries.length !== 0;
    return entries.length;
  }

  const linkModal = linkDialog
    ? registerModal(linkDialog, {
        initialFocus: () => (linkDialogMode === "place" ? linkPlaceSelect : linkUrlInput),
        fallbackFocus: () => ed.pagesEl,
        onOpen: () => {
          if (linkDialogMode !== "place") linkUrlInput.select();
        },
        onClose: () => {
          linkDialogCtx = null;
        },
      })
    : null;

  /** Opens the dialog over `[node, start)..[node, end)`. `link` (optional) is an
   *  existing hyperlink to edit — it prefills the URL / bookmark / ScreenTip and
   *  shows Remove; a fresh insert leaves them empty. `text` is the current display
   *  text. Fails closed in Viewing/Suggesting (the same gate the apply path
   *  enforces) so the dialog never opens onto a dead Apply. */
  function openLinkDialog({ node, start, end, link = null, text = "" }) {
    if (!ed.doc || !linkDialog) return;
    if (ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting()) return;
    linkDialogCtx = { node, start, end, editing: !!link, originalText: text };

    const internal = link?.kind === "internal";
    const hasPlaces = populateLinkPlaces(internal ? link.anchor : null);
    linkTextInput.value = text;
    linkTooltipInput.value = link?.tooltip || "";
    linkUrlInput.value = internal ? "" : (link?.url || "");
    setLinkDialogMode(internal && hasPlaces ? "place" : "url");
    setLinkDialogNote("", false);
    linkDialogTitle.textContent = link ? "Edit link" : "Insert link";
    linkRemoveBtn.hidden = !link;

    linkModal.open();
  }

  function closeLinkDialog() {
    linkModal?.close();
  }

  /** The target string the active mode resolves to: a trimmed URL, or the picked
   *  bookmark's `#anchor` value. */
  function linkDialogTarget() {
    return linkDialogMode === "place"
      ? linkPlaceSelect.value.trim()
      : linkUrlInput.value.trim();
  }

  /** Applies the dialog as one undoable, gated action. An empty target is
   *  rejected inline (never creates an empty link). When the display text is
   *  unchanged the link is set via `setHyperlink` (carrying the ScreenTip); when
   *  the user edits the text, the text + link are re-applied together via the
   *  rich-run paste op (one undoable action — the ScreenTip is only persisted
   *  when the text is left unchanged, since that op carries no tooltip). */
  async function applyLinkDialog() {
    if (!ed.doc || !linkDialogCtx) return;
    const { node, start, end, originalText } = linkDialogCtx;
    const target = linkDialogTarget();
    if (!target) {
      setLinkDialogNote(
        linkDialogMode === "place"
          ? "Choose a bookmark to link to."
          : "Enter a web address, or link to a place in this document.",
        true,
      );
      (linkDialogMode === "place" ? linkPlaceSelect : linkUrlInput).focus();
      return;
    }
    const tooltip = linkTooltipInput.value.trim();
    const displayText = linkTextInput.value;
    const textChanged = displayText.length > 0 && displayText !== originalText;
    // Point the model selection at the target range so the shared, gated apply
    // paths (which read the live selection) act on exactly this link.
    ed.selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
    ed.drawSelection();

    if (!textChanged) {
      await ed.runToolbarEdit(() => ed.doc.setHyperlink(node, start, end, target, tooltip || null));
    } else {
      // Rebuild the range as a single run carrying the first run's formatting plus
      // the new text and href; pasteRichRuns batches InsertText+FormatText+
      // SetHyperlink into one undoable action.
      let base = {};
      try {
        base = (JSON.parse(ed.doc.copyRichRuns(node, start, node, end)) || [])
          .find((run) => !run.paragraphBreak) || {};
      } catch { /* fall back to an unformatted run */ }
      const run = { ...base, text: displayText, href: target, paragraphBreak: false };
      await ed.pasteRichRunsJson(JSON.stringify([run]));
      if (tooltip) ed.setStatus("Link added. Its ScreenTip needs the display text left unchanged.");
    }
    closeLinkDialog();
  }

  /** Removes the link over the dialog's range (`removeHyperlink`, one gated
   *  undoable action) — the Remove button, shown only when editing. */
  async function removeLinkDialog() {
    if (!ed.doc || !linkDialogCtx) return;
    const { node, start, end } = linkDialogCtx;
    ed.selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
    ed.drawSelection();
    await ed.runToolbarEdit(() => ed.doc.removeHyperlink(node, start, end));
    closeLinkDialog();
    ed.setStatus("Link removed");
  }

  if (linkDialog) {
    linkDialogForm.addEventListener("submit", (event) => {
      event.preventDefault();
      void applyLinkDialog();
    });
    linkModeUrl.addEventListener("click", () => {
      setLinkDialogMode("url");
      linkUrlInput.focus();
    });
    linkModePlace.addEventListener("click", () => {
      setLinkDialogMode("place");
      linkPlaceSelect.focus();
    });
    // Picking a place is a target choice; clear any stale "enter a URL" error.
    linkPlaceSelect.addEventListener("change", () => setLinkDialogNote("", false));
    for (const field of [linkUrlInput, linkTextInput, linkTooltipInput]) {
      field.addEventListener("input", () => {
        if (linkDialogNote.classList.contains("error")) setLinkDialogNote("", false);
      });
    }
    linkRemoveBtn.addEventListener("click", () => void removeLinkDialog());
    linkCancelBtn.addEventListener("click", () => closeLinkDialog());
    linkDialogClose.addEventListener("click", () => closeLinkDialog());
    linkDialog.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && event.target === linkPlaceSelect) {
        // Enter on the native select would not submit the form; apply explicitly.
        event.preventDefault();
        void applyLinkDialog();
      }
    });
  }

  return { openLinkDialog };
}
