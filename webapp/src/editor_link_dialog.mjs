// Links: the popover at the selection that every route opens first (⌘K, Insert
// ▸ Link, the link card's Edit, the context menu; `docs/184` D-13, audit
// DIALOGS-11), and the Insert / edit link dialog behind its "More options…"
// (a web address or a place in this document, its text and ScreenTip, Apply and
// Remove). Moved out of `main.js` (docs/177); it reaches the rest of the editor
// only through the context `ed`.
import { registerModal } from "./modal.mjs";
import { t } from "./i18n.mjs";
import { popoverPosition } from "./popover_position.mjs";
import { matchPlaces, resolveLinkInput, stepActiveOption } from "./link_popover_model.mjs";

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
  const LINK_DIALOG_HINT = "";
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
  function openLinkModal({ node, start, end, link = null, text = "", editing = !!link }) {
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
    linkDialogTitle.textContent = editing ? "Edit link" : "Insert link";
    linkRemoveBtn.hidden = !editing;
    linkUrlInput.removeAttribute("aria-invalid");

    linkModal.open();
  }

  function closeLinkDialog() {
    linkModal?.close();
  }

  /** The target string the active mode resolves to: a trimmed URL, or the picked
   *  bookmark's `#anchor` value. */
  function linkDialogTarget() {
    if (linkDialogMode === "place") return linkPlaceSelect.value.trim();
    // The same rules as the popover's field (audit DIALOGS-14): a bare domain
    // gains https://, an email gains mailto:, and what is no address at all is
    // refused here, with the field marked, rather than written as a dead link.
    const typed = linkUrlInput.value.trim();
    if (!typed) return "";
    const resolved = resolveLinkInput(typed, { bookmarks: bookmarkNames() });
    // The full dialog is the power surface, as Word's is: it writes any scheme
    // verbatim (a document may need a custom app link), and the follow paths
    // refuse what may not be opened (`link_targets.mjs`). It still refuses text
    // that is no address at all.
    if (resolved.why === "scheme") return typed;
    linkUrlInput.setAttribute("aria-invalid", String(!resolved.ok));
    if (!resolved.ok) {
      setLinkDialogNote(t(`linkPopover.${resolved.why}`), true);
      return null;
    }
    return resolved.target;
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
    if (target === null) {
      linkUrlInput.focus();
      return;
    }
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
    await writeLink({ node, start, end, originalText }, target, linkTooltipInput.value.trim(), linkTextInput.value);
    closeLinkDialog();
  }

  /** Writes a link over `[node, start)..[node, end)` as ONE undoable, gated
   *  action, for both surfaces (the popover and the dialog). Unchanged text sets
   *  the link in place with its ScreenTip; changed text, or a link inserted at a
   *  caret, re-applies text and link together through the rich-run paste op. */
  async function writeLink({ node, start, end, originalText }, target, tooltip, typedText) {
    // At a caret there is no text to link: the address is the text, as in Docs.
    const displayText = start === end && !typedText ? target.replace(/^mailto:/, "") : typedText;
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
        linkUrlInput.removeAttribute("aria-invalid");
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

  // ---- Link popover -----------------------------------------------------------
  // The default link surface (`docs/184` D-13, audit DIALOGS-11), Google Docs'
  // shape: anchored under the selection, no scrim, Text and one Link field, the
  // document's bookmarks listed under the field as you type, Apply. Esc, a click
  // outside and Apply all close it and give the caret back to the document.
  // The dialog above is its "More options…", opened in its place with what was
  // typed carried over, for the ScreenTip and the explicit bookmark picker.
  const pop = document.getElementById("linkPopover");
  const popForm = document.getElementById("linkPopoverForm");
  const popText = document.getElementById("linkPopoverText");
  const popUrl = document.getElementById("linkPopoverUrl");
  const popPlaces = document.getElementById("linkPopoverPlaces");
  const popError = document.getElementById("linkPopoverError");
  const popApply = document.getElementById("linkPopoverApply");
  const popRemove = document.getElementById("linkPopoverRemove");
  const popMore = document.getElementById("linkPopoverMore");
  /** What the open popover targets; `null` while it is closed. */
  let popCtx = null;
  /** The bookmark row the keyboard is on, or -1. The field keeps DOM focus and
   *  names this row through `aria-activedescendant` (the APG combobox with a
   *  listbox popup), so typing and arrowing never fight over focus. */
  let activePlace = -1;

  function bookmarkNames() {
    return ed.bookmarkManager?.entries?.().map((entry) => entry.name) ?? [];
  }

  /** The rectangle to hang the popover from: the selection's painted
   *  highlight, else the caret, else the top of the page area. */
  function anchorRect() {
    const boxes = [...ed.pagesEl.querySelectorAll(".overlay .highlight, .overlay .caret")]
      .map((el) => el.getBoundingClientRect())
      .filter((r) => r.width > 0 || r.height > 0);
    if (!boxes.length) return ed.pagesEl.getBoundingClientRect();
    const top = Math.min(...boxes.map((r) => r.top));
    const bottom = Math.max(...boxes.map((r) => r.bottom));
    const left = Math.min(...boxes.map((r) => r.left));
    const right = Math.max(...boxes.map((r) => r.right));
    return { top, bottom, left, right, width: right - left, height: bottom - top };
  }

  function placePopover() {
    const at = popoverPosition(
      anchorRect(),
      { width: pop.offsetWidth, height: pop.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    pop.style.left = `${at.left}px`;
    pop.style.top = `${at.top}px`;
  }

  /** Validates the Link field. Apply is disabled while it holds nothing that
   *  can be linked; the reason is shown, and the field marked invalid, only once
   *  the reader has typed something (an empty field is not yet an error). */
  function validatePopover() {
    const resolved = resolveLinkInput(popUrl.value, { bookmarks: bookmarkNames() });
    const typed = popUrl.value.trim().length > 0;
    popApply.disabled = !resolved.ok;
    popUrl.setAttribute("aria-invalid", String(typed && !resolved.ok));
    popError.textContent = typed && !resolved.ok && resolved.why !== "empty" ? t(`linkPopover.${resolved.why}`) : "";
    return resolved;
  }

  /** The bookmarks matching what is typed, shown while the field reads as a
   *  search (a leading #, or a word with no dot or colon in it). */
  function renderPlaces() {
    const query = popUrl.value.trim();
    const searching = query.startsWith("#") || (query.length > 0 && !/[.:@/]/.test(query));
    const names = searching ? matchPlaces(bookmarkNames(), query) : [];
    popPlaces.replaceChildren(
      ...names.map((name, index) => {
        const row = document.createElement("li");
        row.id = `linkPopoverPlace${index}`;
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", "false");
        row.className = "link-popover-place";
        row.dataset.anchor = name;
        row.textContent = name;
        return row;
      }),
    );
    popPlaces.hidden = names.length === 0;
    popUrl.setAttribute("aria-expanded", String(names.length > 0));
    setActivePlace(-1);
  }

  /** Puts the keyboard on bookmark row `index` (-1: on none). */
  function setActivePlace(index) {
    const rows = [...popPlaces.querySelectorAll("[role=option]")];
    activePlace = index >= 0 && index < rows.length ? index : -1;
    rows.forEach((row, i) => row.setAttribute("aria-selected", String(i === activePlace)));
    const active = rows[activePlace];
    if (active) {
      popUrl.setAttribute("aria-activedescendant", active.id);
      active.scrollIntoView?.({ block: "nearest" });
    } else {
      popUrl.removeAttribute("aria-activedescendant");
    }
  }

  function hidePlaces() {
    popPlaces.hidden = true;
    popUrl.setAttribute("aria-expanded", "false");
    setActivePlace(-1);
  }

  function choosePlace(name) {
    popUrl.value = `#${name}`;
    hidePlaces();
    validatePopover();
    popUrl.focus();
  }

  function openLinkPopover({ node, start, end, link = null, text = "" }) {
    if (!ed.doc) return;
    if (ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting()) return;
    popCtx = { node, start, end, link, originalText: text };
    popText.value = text;
    popUrl.value = link ? (link.kind === "internal" ? `#${link.anchor}` : link.url || "") : "";
    popRemove.hidden = !link;
    const selToolbar = document.getElementById("selToolbar");
    if (selToolbar) selToolbar.hidden = true; // one floating surface at a time
    pop.hidden = false;
    renderPlaces();
    validatePopover();
    placePopover();
    // With text already chosen the address is what is missing; at a caret the
    // text is, as in Docs.
    const first = start === end && !text ? popText : popUrl;
    first.focus({ preventScroll: true });
    first.select();
  }

  function closeLinkPopover({ restoreFocus = true } = {}) {
    if (pop.hidden) return;
    pop.hidden = true;
    hidePlaces();
    popCtx = null;
    if (restoreFocus) ed.focusEditorSurface?.();
  }

  async function applyLinkPopover() {
    if (!ed.doc || !popCtx) return;
    const resolved = validatePopover();
    if (!resolved.ok) {
      if (resolved.why === "empty") popError.textContent = t("linkPopover.empty");
      popUrl.focus();
      return;
    }
    const ctx = popCtx;
    closeLinkPopover({ restoreFocus: false });
    await writeLink(ctx, resolved.target, ctx.link?.tooltip ?? "", popText.value);
    ed.focusEditorSurface?.();
  }

  if (pop) {
    popForm.addEventListener("submit", (event) => {
      event.preventDefault();
      void applyLinkPopover();
    });
    popUrl.addEventListener("input", () => {
      renderPlaces();
      validatePopover();
    });
    // The bookmark list from the keyboard (SKILL §10; WAI-ARIA APG combobox):
    // Up and Down move through the rows while focus stays in the field, Enter
    // or Tab takes the row the keyboard is on, and Esc closes the list before
    // it closes the popover.
    popUrl.addEventListener("keydown", (event) => {
      if (popPlaces.hidden) return;
      const rows = popPlaces.querySelectorAll("[role=option]");
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setActivePlace(stepActiveOption(activePlace, rows.length, event.key));
      } else if ((event.key === "Enter" || event.key === "Tab") && activePlace >= 0) {
        if (event.key === "Enter") event.preventDefault();
        choosePlace(rows[activePlace].dataset.anchor);
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        hidePlaces();
      }
    });
    popPlaces.addEventListener("mousedown", (event) => event.preventDefault());
    popPlaces.addEventListener("click", (event) => {
      const row = event.target.closest?.("[data-anchor]");
      if (row) choosePlace(row.dataset.anchor);
    });
    popRemove.addEventListener("click", async () => {
      if (!ed.doc || !popCtx) return;
      const { node, start, end } = popCtx;
      closeLinkPopover({ restoreFocus: false });
      ed.selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
      ed.drawSelection();
      await ed.runToolbarEdit(() => ed.doc.removeHyperlink(node, start, end));
      ed.focusEditorSurface?.();
    });
    // More options: the dialog takes the popover's place, with what was typed.
    popMore.addEventListener("click", () => {
      if (!popCtx) return;
      const { node, start, end, link } = popCtx;
      const typed = popUrl.value.trim();
      const resolved = resolveLinkInput(typed, { bookmarks: bookmarkNames() });
      const carried = resolved.ok
        ? resolved.kind === "place"
          ? { kind: "internal", anchor: resolved.target.slice(1), tooltip: link?.tooltip }
          : { kind: "external", url: resolved.target, tooltip: link?.tooltip }
        : typed
          ? { kind: "external", url: typed, tooltip: link?.tooltip }
          : link;
      const text = popText.value;
      closeLinkPopover({ restoreFocus: false });
      // A fresh insert carried over is still an insert: no Remove, "Insert link".
      openLinkModal({ node, start, end, link: carried, text, editing: !!link });
    });
    pop.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeLinkPopover();
      }
    });
    document.addEventListener("pointerdown", (event) => {
      if (!pop.hidden && !pop.contains(event.target)) closeLinkPopover({ restoreFocus: false });
    });
    ed.viewportEl?.addEventListener("scroll", () => !pop.hidden && placePopover(), { passive: true });
    window.addEventListener("resize", () => !pop.hidden && placePopover());
  }

  /** Every route to a link opens the popover first; without its markup (an
   *  embed that strips it) the dialog still works on its own. */
  function openLinkDialog(args) {
    if (pop) openLinkPopover(args);
    else openLinkModal(args);
  }

  return { openLinkDialog, openLinkModal };
}
