// The ribbon's own chrome: the compact and ribbon view toggle, the Home tab's
// clipboard and Styles gallery, named-style edits, the confirmation dialog, and
// collapsing groups that do not fit into the overflow menu. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { RECOMMENDED_STYLES, caretContexts, offeredStyleNames, styleMenuGroups, styleSlug } from "./style_picker.mjs";
import { styleDisplayName } from "./style_names.mjs";
import { applyStylePreview } from "./style_preview.mjs";
import { createConfirmDialog } from "./confirm_dialog.mjs";
import { createNamePrompt } from "./name_dialog.mjs";
import { registerModal } from "./modal.mjs";
import { closePopover } from "./popover_manager.mjs";
import { groupsToOverflow } from "./ribbon_overflow.mjs";
import { makeMenuHeading } from "./picker_chrome.mjs";
import { readPref, writePref } from "./prefs.mjs";
import { onChromeScaleChange } from "./chrome_scale.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createRibbon(ed) {
  // --- Compact ↔ ribbon view toggle (collapse/expand the band) -----------------
  const ribbonViewToggle = document.getElementById("ribbonViewToggle");


  /** Collapses the ribbon to just its tab strip (compact view) or expands it back
   *  to the full band, persisting the choice. */
  function setRibbonCollapsed(collapsed) {
    ed.ribbonViewCollapsed = collapsed;
    const ribbon = document.querySelector(".ribbon");
    ribbon?.classList.toggle("is-collapsed", collapsed);
    if (ribbonViewToggle) {
      ribbonViewToggle.setAttribute("aria-expanded", String(!collapsed));
      ribbonViewToggle.setAttribute(
        "aria-label",
        collapsed ? "Expand the ribbon" : "Collapse the ribbon",
      );
      ribbonViewToggle.title = collapsed
        ? "Expand the ribbon"
        : "Collapse the ribbon (compact view)";
      const icon = ribbonViewToggle.querySelector(".ms");
      if (icon) icon.textContent = collapsed ? "keyboard_arrow_down" : "keyboard_arrow_up";
    }
    writePref("opendoc.ribbonCollapsed", collapsed ? "1" : "0");
    if (!collapsed && typeof scheduleRibbonOverflow === "function") scheduleRibbonOverflow();
  }

  if (ribbonViewToggle) {
    ribbonViewToggle.addEventListener("click", () => setRibbonCollapsed(!ed.ribbonViewCollapsed));
    if (readPref("opendoc.ribbonCollapsed") === "1") setRibbonCollapsed(true);
  }

  // --- Home ribbon: Clipboard, Styles gallery, overflow, tooltips (docs/64) ----
  // The Home band mirrors template.png. Every control below maps to a real,
  // working opendoc action; nothing is a placeholder (docs/64 "no dead controls").

  const pasteBtn = document.getElementById("pasteBtn");
  const cutBtn = document.getElementById("cutBtn");
  const copyBtn = document.getElementById("copyBtn");
  const replaceBtn = document.getElementById("replaceBtn");
  // Clipboard buttons reuse the exact clipboard actions the command palette and
  // keyboard already invoke (`paste`/`cut`/`copySelection`), so they are never a
  // second code path. Replace opens the same Find & Replace panel on its replace field.
  pasteBtn.addEventListener("click", () => { ed.paste(); });
  cutBtn.addEventListener("click", () => { ed.cut(); });
  copyBtn.addEventListener("click", () => { ed.copySelection(); });
  replaceBtn.addEventListener("click", () => { if (!replaceBtn.disabled) ed.openFind({ replace: true }); });

  // The Styles control — the ONE control the band offers for paragraph styles (docs/115).
  // It replaces three: a `#paragraphStyle` select listing every style in the document
  // flat, this strip, and a "▾" popover listing every style again. Word puts a curated
  // Quick Styles gallery in the ribbon and the full list in a pane; Docs offers six named
  // styles and nothing else. So the band offers a SHORT list and the full stylesheet stays
  // reachable off it — the palette's `Style: <name>` rows and Paragraph properties ▸ Style.
  // Import, cascade, layout, render and round-trip are untouched: this is what is offered
  // for AUTHORING, not what is supported.
  const stylesTrigger = document.getElementById("stylesTrigger");
  const stylesTriggerLabel = document.getElementById("stylesTriggerLabel");
  const stylesMenu = document.getElementById("stylesMenu");
  const stylesMenuInput = document.getElementById("stylesMenuInput");
  const stylesMenuList = document.getElementById("stylesMenuList");
  const stylesMenuEmpty = document.getElementById("stylesMenuEmpty");

  // Styles the document is USING, offered alongside the recommended ones — otherwise
  // opening a file whose paragraphs use `Quotation` leaves the user unable to see or
  // re-apply it. Word adds in-use styles to its gallery too. This is what the CHROME
  // observed (styles the caret visited, plus ones applied this session), not a document
  // scan: there is no `stylesInUse()` on the engine and walking every paragraph from JS
  // is O(n) in document size, which docs/107 §4 forbids. Pruned on document load.
  const stylesInUse = new Set();

  /** The paragraph style at the caret. The deleted select held this; the active card
   *  and `#stylesTrigger`'s label and `data-active-style` hold it now. */
  let activeParagraphStyle = "";

  // Up/Down move between the options while the menu is open, matching every other
  // product popover in the band and the WAI-ARIA listbox pattern. Enter and Space
  // activate a focused option natively (they are <button>s). Bound once to the
  // container, so rebuilding the options never stacks duplicate listeners.
  const STYLE_ROVE_STEP = { ArrowDown: 1, ArrowUp: -1 };
  stylesMenu.addEventListener("keydown", (event) => {
    const options = [...stylesMenuList.querySelectorAll(".style-option")];
    if (!options.length) return;
    const at = options.indexOf(document.activeElement);
    const step = STYLE_ROVE_STEP[event.key];
    let next;
    if (step) next = at < 0 ? (step > 0 ? 0 : options.length - 1) : (at + step + options.length) % options.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = options.length - 1;
    else return;
    event.preventDefault();
    options[next].focus();
  });

  /** Builds one option row, drawn IN the style it applies — with a tick on the one
   *  the caret is already in, which is how Docs and every menu-shaped picker says
   *  "you are here". */
  function makeStyleOption(name) {
    const slug = styleSlug(name);
    const option = document.createElement("button");
    option.type = "button";
    option.className = `menu-item style-option style-option-${slug}`;
    option.dataset.style = name;
    option.setAttribute("role", "option");
    option.setAttribute("aria-selected", "false");
    const check = document.createElement("span");
    check.className = "menu-check";
    check.setAttribute("aria-hidden", "true");
    option.appendChild(check);
    const label = document.createElement("span");
    label.className = "style-option-name";
    label.textContent = styleDisplayName(name);
    applyStylePreview(label, name, (style) => ed.doc?.stylePreview?.(style));
    option.appendChild(label);
    option.addEventListener("click", () => {
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setParagraphStyle(a, b, c, d, name), { paragraphLevel: true });
      closePopover(ed.stylesPopover);
      // Back to the document, as Word and Docs do after a style pick and as
      // `chooseFont` already does.
      ed.focusEditorSurface();
    });
    return option;
  }

  /** What `listStyles()` last reported, and `RECOMMENDED_STYLES` ∩ that — both resolved
   *  once per registry change, not per caret move: `offeredStyles` is on the
   *  per-keystroke path, and matching 15 names case-insensitively against every defined
   *  style there would be O(15n) a keystroke. */
  let definedStyles = new Set();
  let recommendedHere = [];

  /** The suggested group, narrowed to WHERE THE CARET IS. The rule itself is
   *  `offeredStyleNames`; this is only the question "where are we". */
  function offeredStyles() {
    return offeredStyleNames({
      recommended: recommendedHere,
      inUse: stylesInUse,
      defined: definedStyles,
      active: activeParagraphStyle,
      contexts: caretContexts(caretStyleContext()),
    });
  }

  /** The caret's surroundings, as the style list cares about them. Every signal
   *  is one the toolbar already reads on the same refresh, so this adds no
   *  per-keystroke engine work. */
  function caretStyleContext() {
    const node = ed.selection?.focus?.node;
    if (!ed.doc || !node) return {};
    let story = "body";
    if (ed.runningEditBand === "header" || ed.runningEditBand === "footer") story = ed.runningEditBand;
    let inTable = false;
    let listKind = "";
    try {
      inTable = ed.doc.inTable(node);
      listKind = ed.doc.listStyleAt(node) || "";
    } catch {
      // A node the engine no longer has (mid-edit) is not a context; the
      // general list is still correct, just not narrowed.
    }
    return { story, inTable, listKind };
  }

  /** Rebuilds the option rows, but only when the offered set actually changed. This is
   *  on the per-keystroke path (`updateToolbar` runs on every caret move and the offered
   *  set depends on the caret's style), so rebuilding unconditionally would re-resolve a
   *  preview per option per keystroke — and would tear the menu out from under a pointer
   *  while it is open. */
  function renderStylesGallery() {
    const { suggested, rest } = styleMenuGroups({
      suggested: offeredStyles(),
      defined: definedStyles,
      query: stylesMenuInput.value,
    });
    stylesMenuList.replaceChildren();
    // The split is the point: "what you probably want here" and "what this
    // document has" mean different things, and Word's Styles pane makes the
    // same one with its Recommended / All filter.
    if (suggested.length && rest.length) {
      stylesMenuList.appendChild(makeMenuHeading("Suggested"));
    }
    for (const name of suggested) stylesMenuList.appendChild(makeStyleOption(name));
    if (rest.length) {
      if (suggested.length) stylesMenuList.appendChild(makeMenuHeading("All styles"));
      for (const name of rest) stylesMenuList.appendChild(makeStyleOption(name));
    }
    stylesMenuEmpty.hidden = suggested.length + rest.length > 0;
  }

  /** Rebuilds the menu for a (re)loaded document or a changed style registry. The
   *  full stylesheet is NOT put on the band (docs/115 §5). */
  function buildStylesGallery(styles) {
    definedStyles = new Set(styles);
    // Matched case-insensitively, so `body text` from an ODT package still counts.
    const byName = new Map(styles.map((s) => [s.toLowerCase(), s]));
    recommendedHere = RECOMMENDED_STYLES.map((s) => byName.get(s.toLowerCase())).filter(Boolean);
    // A style remembered from the previous document is not in use in this one, and a
    // name that no longer exists cannot be applied.
    for (const name of [...stylesInUse]) if (!definedStyles.has(name)) stylesInUse.delete(name);
    // Force the rebuild `renderStylesGallery` skips when the offered NAMES are
    // unchanged: "Update <style> to match selection" changes a DEFINITION, not a name,
    // and the rows would keep previewing the old one. `ribbon-home` caught that.
    stylesMenuInput.value = "";
    stylesMenuList.replaceChildren();
    renderStylesGallery();
    syncStylesGalleryActive();
  }

  /** Puts the caret's style on the trigger, ticks it in the menu, and publishes it
   *  on the control so the chrome has one answer to "what style is the caret in".
   *
   *  The trigger's LABEL is the whole of what the deleted 14-entry select
   *  contributed that the card gallery did not: a control that says, without being
   *  opened, which style you are in. Docs does exactly this. */
  function syncStylesGalleryActive() {
    const active = activeParagraphStyle;
    stylesTrigger.dataset.activeStyle = active;
    // A style outside the offered six is still shown on the trigger — it is what
    // the caret is in, and a control reporting something else is worse than one
    // reporting a style it cannot re-offer. `offeredStyles` gives it a slot anyway.
    stylesTriggerLabel.textContent = styleDisplayName(active || "Normal");
    stylesTrigger.classList.toggle("is-placeholder", !active);
    stylesTrigger.setAttribute(
      "aria-label",
      active ? `Paragraph style: ${styleDisplayName(active)}` : "Paragraph style",
    );
    for (const option of stylesMenuList.querySelectorAll(".style-option")) {
      const selected = option.dataset.style === active;
      option.setAttribute("aria-selected", String(selected));
      option.classList.toggle("is-selected", selected);
    }
    // The band owns exactly one Tab stop across everything it holds. Re-assert that
    // after the control publishes its own, or the band grows a second one on every
    // caret move — which is precisely how it came to have three.
    ed.syncRibbonTabStops();
  }

  /** Records the style at the caret and keeps the offered set in step. A style the caret
   *  visits is one the document is using, so it joins `stylesInUse` and stays offered
   *  after the caret leaves. */
  function reflectParagraphStyle(name) {
    activeParagraphStyle = name || "";
    if (activeParagraphStyle) stylesInUse.add(activeParagraphStyle);
    renderStylesGallery();
    syncStylesGalleryActive();
  }

  // --- Named-style edits: update-from-selection and create-from-selection ------
  // Word's two core Styles verbs, both routed through the same engine op
  // (`SetStyleDefinition`): "Update <Style> to match selection" mutates the style
  // definition so every paragraph using it reflows; "Create a style" adds a new
  // paragraph style (based on the current one) and applies it. Both rebuild the
  // gallery previews and dropdowns from the (now changed) style registry.
  // Both name cards go through ONE contract (`name_dialog.mjs`): the staged-result
  // dance that keeps Escape from leaving the promise pending is subtle enough that
  // a second copy of it is a second place for that bug to live.
  const styleNamePrompt = createNamePrompt({
    registerModal,
    fallbackFocus: () => ed.pagesEl,
    ids: {
      dialog: "styleNameDialog",
      input: "styleNameInput",
      confirm: "styleNameConfirm",
      cancel: "styleNameCancel",
      close: "styleNameClose",
    },
  });

  /** Opens the create-style dialog and resolves to the entered name, or null if
   *  cancelled. */
  function promptStyleName() {
    return styleNamePrompt.prompt("");
  }

  // ---- Confirmation ----------------------------------------------------------
  // The application's single yes/no question, in `confirm_dialog.mjs`. Why it is
  // never `window.confirm`, and why the answer is staged rather than resolved
  // directly, are both recorded there.
  const confirmDialogUi = createConfirmDialog({ registerModal, fallbackFocus: () => ed.pagesEl });
  const confirmModal = (options) => confirmDialogUi.ask(options);

  /** Gate for anything that replaces the open document. Returns true when it is
   *  safe to proceed. Reads `documentIsDirty()` — the engine revision watermark,
   *  which fails dirty — so this and the beforeunload guard can never disagree
   *  about whether work would be lost (docs/104 T-14: one dirty signal, one
   *  reader). */
  async function confirmDiscardIfEdited() {
    if (!ed.documentIsDirty()) return true;
    return confirmModal({
      title: "Discard unsaved changes?",
      message: `Opening another file replaces “${ed.currentName || "this document"}”. Your edits have not been saved and there is no copy of them anywhere else.`,
      confirmLabel: "Discard and open",
      cancelLabel: "Keep editing",
      note: "Save the .docx first to keep your work.",
      icon: "warning",
    });
  }

  /** The paragraph style applied at the caret, or "" when none — the target of
   *  "Update to match selection" and the base for a new style. */
  function currentParagraphStyleName() {
    if (!ed.doc || !ed.selection) return "";
    try {
      return ed.doc.paragraphStyleAt(ed.selection.focus.node) || "";
    } catch {
      return "";
    }
  }

  /** Redefines paragraph style `name` to match the selection (Word's "Update
   *  <Style> to Match Selection"). Every paragraph using it reflows. */
  async function updateStyleFromSelection(name) {
    if (!ed.doc || !name) return;
    await ed.runToolbarEdit((a, b, c, d) => ed.doc.updateStyleFromSelection(a, b, c, d, name));
    ed.populateStyles();
    ed.updateToolbar();
    ed.setStatus(`Updated “${styleDisplayName(name)}” to match the selection`);
  }

  /** Creates a new paragraph style from the selection and applies it (Word's
   *  "Create a Style"). Prompts for the name; refuses duplicates via the engine. */
  async function createStyleFromSelection() {
    if (!ed.doc || !ed.selection) return;
    const name = await promptStyleName();
    if (!name) return;
    const exists = ed.doc.listStyles().some((s) => s.toLowerCase() === name.toLowerCase());
    if (exists) {
      ed.setStatus(`A style named “${name}” already exists`, "error");
      return;
    }
    await ed.runToolbarEdit((a, b, c, d) => ed.doc.createStyleFromSelection(a, b, c, d, name));
    ed.populateStyles();
    ed.updateToolbar();
    ed.setStatus(`Created style “${name}”`);
  }

  // --- Ribbon overflow: collapse groups that don't fit into a "⋯" menu ---------
  const ribbonBodyEl = document.querySelector(".ribbon-body");
  const ribbonOverflowBtn = document.getElementById("ribbonOverflowBtn");
  const ribbonOverflowMenu = document.getElementById("ribbonOverflowMenu");
  // Canonical group order per panel, captured before any group is relocated.
  const ribbonPanelGroups = new Map(
    ed.ribbonPanels.map((p) => [p, [...p.querySelectorAll(":scope > .rgroup")]]),
  );

  function closeRibbonOverflow({ restoreFocus = false } = {}) {
    if (!ribbonOverflowMenu) return;
    ribbonOverflowMenu.hidden = true;
    ribbonOverflowBtn?.setAttribute("aria-expanded", "false");
    if (restoreFocus) ribbonOverflowBtn?.focus();
  }

  /** Reflows the active ribbon panel: groups that don't fit move into the "⋯"
   *  overflow menu so the ribbon never shows a horizontal scrollbar. */
  function updateRibbonOverflow() {
    if (!ribbonBodyEl || !ribbonOverflowBtn || !ribbonOverflowMenu) return;
    closeRibbonOverflow();
    // Restore every group to its home panel in canonical order before measuring.
    for (const [panel, groups] of ribbonPanelGroups) {
      for (const group of groups) panel.appendChild(group);
    }
    ribbonOverflowMenu.replaceChildren();
    ribbonOverflowBtn.hidden = true;
    const active = ed.ribbonPanels.find((p) => !p.hidden);
    if (!active) return;
    const groups = ribbonPanelGroups.get(active) || [];
    const style = getComputedStyle(active);
    const avail =
      active.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    // An unmeasurable band is not a narrow band. If the panel is display:none or
    // zero-width (mode switch mid-flight, ribbon collapsed, first paint), every
    // unpinned group would "not fit" and be exiled to the overflow menu on no
    // evidence at all. Leave the inline composition alone and wait to be called
    // again once there is a width to measure against.
    if (!(avail > 0)) return;
    const widths = new Map(groups.map((group) => [group, group.offsetWidth]));
    const total = groups.reduce((sum, group) => sum + widths.get(group), 0);
    if (total <= avail + 0.5) return; // everything fits — no overflow control
    // Reserve room for the ⋯ button. Clipboard, Editing, and Mode are persistent
    // anchors; relocate the other groups from right to left until the inline set
    // fits. Mode can fall back to the footer at very small widths.
    const moved = groupsToOverflow({
      groups,
      widthOf: (group) => widths.get(group),
      avail,
      reserve: 44,
      isPinned: (group) => group.hasAttribute("data-ribbon-pinned"),
      isLastResort: (group) => group.dataset.group === "clipboard",
    });
    for (const group of groups) if (moved.includes(group)) ribbonOverflowMenu.appendChild(group);
    ribbonOverflowBtn.hidden = false;
  }

  let ribbonOverflowFrame = 0;
  function scheduleRibbonOverflow() {
    cancelAnimationFrame(ribbonOverflowFrame);
    ribbonOverflowFrame = requestAnimationFrame(() => {
      updateRibbonOverflow();
      ed.syncRibbonTabStops();
    });
  }

  if (ribbonOverflowBtn && ribbonOverflowMenu) {
    // The menu is fixed-positioned and lives on <body> so the ribbon's
    // `overflow:hidden` never clips the dropdown.
    document.body.appendChild(ribbonOverflowMenu);
    const positionOverflowMenu = () => {
      const rect = ribbonOverflowBtn.getBoundingClientRect();
      const mw = ribbonOverflowMenu.offsetWidth;
      const left = Math.max(6, Math.min(rect.right - mw, window.innerWidth - mw - 6));
      const top = Math.round(rect.bottom + 4);
      ribbonOverflowMenu.style.left = `${Math.round(left)}px`;
      ribbonOverflowMenu.style.top = `${top}px`;
      ribbonOverflowMenu.style.maxHeight = `${Math.max(120, window.innerHeight - top - 6)}px`;
    };
    ribbonOverflowBtn.addEventListener("click", () => {
      const open = ribbonOverflowMenu.hidden;
      ribbonOverflowMenu.hidden = !open;
      ribbonOverflowBtn.setAttribute("aria-expanded", String(open));
      if (open) {
        positionOverflowMenu();
        requestAnimationFrame(() => {
          ribbonOverflowMenu.querySelector(
            'button:not(:disabled), select:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])',
          )?.focus();
        });
      }
    });
    document.addEventListener("pointerdown", (e) => {
      if (ribbonOverflowMenu.hidden) return;
      if (e.target.closest("#ribbonOverflowMenu, #ribbonOverflowBtn")) return;
      closeRibbonOverflow();
    });
    ribbonOverflowMenu.addEventListener("keydown", (event) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      closeRibbonOverflow({ restoreFocus: true });
    });
    if (typeof ResizeObserver === "function" && ribbonBodyEl) {
      new ResizeObserver(scheduleRibbonOverflow).observe(ribbonBodyEl);
    } else {
      window.addEventListener("resize", scheduleRibbonOverflow);
    }
    // The band is measured in whatever face is available at first paint. Until
    // Inter arrives that is a fallback with wider metrics, so the groups measure
    // wider than they will ever actually be drawn — enough, on a 1280px Home
    // band with about 55px of slack, to exile Styles to the overflow menu. The
    // ResizeObserver above cannot save us: `.ribbon-body` stays exactly as wide
    // as the viewport, so swapping the face resizes the GROUPS without ever
    // resizing the observed element, and the wrong decision stood permanently.
    // Re-decide when the real metrics are in.
    if (document.fonts?.ready) document.fonts.ready.then(scheduleRibbonOverflow).catch(() => {});
    // The same blind spot for the chrome's own size: the groups are rem, so a
    // `--chrome-scale` or browser-font change resizes them without resizing the
    // band (HF-413). Re-decide on the root, not on the band.
    onChromeScaleChange(scheduleRibbonOverflow);
  }

  return {
    buildStylesGallery, confirmDiscardIfEdited, confirmModal, copyBtn, createStyleFromSelection,
    currentParagraphStyleName, cutBtn, pasteBtn, reflectParagraphStyle, renderStylesGallery,
    replaceBtn, ribbonBodyEl, ribbonOverflowBtn, ribbonOverflowMenu, scheduleRibbonOverflow,
    setRibbonCollapsed, stylesMenu, stylesMenuInput, stylesTrigger, syncStylesGalleryActive,
    updateRibbonOverflow, updateStyleFromSelection,
  };
}
