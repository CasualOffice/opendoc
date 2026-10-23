// The formatting toolbar's state: the selection's run format, the font family
// and underline faces, format toggles, table styles and the selection's link.
// selEndpoints, runToolbarEdit, updateToolbar and populateStyles stay in
// main.js, which hands them to the command surfaces at boot. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { escapeHtml } from "./text_rules.mjs";
import { onButton, closePopover } from "./popover_manager.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFormatting(ed) {
  // ---- Formatting toolbar (run + paragraph properties) -------------------------


  /** Runs a toolbar edit thunk `(sNode, sOff, eNode, eOff) => EditResult`,
   *  preserving the selection (formatting does not collapse it) and repainting
   *  only the dirty pages. */
  /** A cross-paragraph edit while Suggesting, tracked (docs/108 phase 2).
   *
   *  `text` replaces the range; omit it for a plain deletion. The engine writes the
   *  shape Word writes: the covered text struck, every paragraph mark in the range
   *  except the last one suggested deleted, and the later paragraphs given the first
   *  one's shape with a `w:pPrChange` holding their own — so accepting leaves one
   *  paragraph and rejecting restores them all. One undo step.
   *
   *  Returns false, having said why, when the engine refuses (a range that would
   *  cross a table, or another reviewer's paragraph break in the way). */
  async function suggestAcrossParagraphs(start, end, text) {
    try {
      await ed.runEdit(() => text == null
        ? ed.doc.suggestDeleteRange(start.node, start.offset, end.node, end.offset, undefined, new Date().toISOString())
        : ed.doc.suggestReplaceRange(start.node, start.offset, end.node, end.offset, text, undefined, new Date().toISOString()));
      return true;
    } catch (error) {
      ed.setStatus(String(error?.message ?? error), "error");
      return false;
    }
  }


  /** The uniform run-format state over the selection, or null if not a range. */
  function selectionFormat() {
    if (!ed.doc || !ed.hasRange()) return null;
    const [sn, so, en, eo] = ed.selEndpoints();
    const f = ed.doc.selectionFormat(sn, so, en, eo);
    const state = {
      bold: f.bold,
      italic: f.italic,
      underline: f.underline,
      strike: f.strike,
      boldState: f.boldState,
      italicState: f.italicState,
      underlineState: f.underlineState,
      strikeState: f.strikeState,
    };
    f.free();
    return state;
  }

  /** The run format the collapsed caret inherits (what new typing would carry). */
  function caretFormatState() {
    if (!ed.doc || !ed.selection) return { bold: false, italic: false, underline: false, strike: false };
    const f = ed.doc.caretFormat(ed.selection.focus.node, ed.selection.focus.offset);
    const state = {
      bold: f.bold,
      italic: f.italic,
      underline: f.underline,
      strike: f.strike,
      boldState: f.boldState,
      italicState: f.italicState,
      underlineState: f.underlineState,
      strikeState: f.strikeState,
    };
    f.free();
    return state;
  }

  function reflectFontFamily(family) {
    ed.currentFontFamily = family || "";
    ed.fontFamilyLabel.textContent = family || "Font";
    ed.fontFamilyLabel.style.fontFamily = family ? `"${family}", system-ui, sans-serif` : "";
    ed.fontFamilyBtn.classList.toggle("is-placeholder", !family);
    ed.fontFamilyBtn.title = family ? `Font: ${family}` : "Font";
  }

  const UNDERLINE_STYLE_LABELS = new Map([
    ["none", "None"],
    ["single", "Single"],
    ["double", "Double"],
    ["thick", "Thick"],
    ["dotted", "Dotted"],
    ["dashed", "Dashed"],
    ["dotDash", "Dot-dash"],
    ["wavy", "Wavy"],
    ["words", "Words only"],
  ]);


  /** Reflect the effective typed underline without writing renderer fallback data
   *  back into the document. Empty color is the model's automatic/text-color state. */
  function reflectUnderlineControl(style, color, mixed = false) {
    const canonical = style || "single";
    ed.currentUnderlineStyle = canonical;
    ed.currentUnderlineColor = color || "";
    ed.currentUnderlineMixed = mixed;
    ed.fmtButtons.underline.dataset.underlineStyle = canonical;
    ed.fmtButtons.underline.style.setProperty("--underline-color", color || "currentColor");
    ed.underlineMenuBtn.closest(".underline-split")?.classList.toggle("is-mixed", mixed);
    ed.underlineMenuBtn.title = mixed
      ? "Underline style and color: Mixed"
      : `Underline: ${UNDERLINE_STYLE_LABELS.get(canonical) || canonical}${color ? ` · ${color.toUpperCase()}` : " · Automatic color"}`;
  }

  /** Toggles a run toggle (`bold`/`italic`/`underline`/`strike`). With a range it
   *  formats the selection; at a collapsed caret it arms the format for typing
   *  (premium editors: press Bold, then type — the text comes out bold). */
  function toggleFormat(prop) {
    if (!ed.hasRange()) {
      // Arm/disarm at the caret: flip the effective current value (pending overrides
      // the caret's inherited format), and reflect it in the toolbar.
      const current = ed.pendingFormat?.[prop] ?? caretFormatState()[prop];
      ed.pendingFormat = { ...(ed.pendingFormat || {}), [prop]: !current };
      ed.updateToolbar();
      return;
    }
    const state = selectionFormat();
    if (!state) return;
    const patch = {
      [prop]: !state[prop],
    };
    ed.armOrApplyRun(patch, () =>
      ed.runToolbarEdit((sn, so, en, eo) => ed.doc.formatSelection(
        sn,
        so,
        en,
        eo,
        prop === "bold" ? !state.bold : undefined,
        prop === "italic" ? !state.italic : undefined,
        prop === "underline" ? !state.underline : undefined,
        prop === "strike" ? !state.strike : undefined,
      )),
    );
  }

  /** "#rrggbb" → [r, g, b]. */
  function hexToRgb(hex) {
    return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  }

  /** Every control that toggles run format `key`, on any surface. The ribbon
   *  button is the one held by id; anything else declares itself with `data-fmt`,
   *  so a new surface inherits the state sync by existing rather than by being
   *  added to a list here. Cached because `updateToolbar` runs on every repaint
   *  and the set is fixed markup. */
  const formatToggleCache = new Map();

  function formatToggleButtons(key) {
    let buttons = formatToggleCache.get(key);
    if (!buttons) {
      buttons = [...new Set([ed.fmtButtons[key], ...document.querySelectorAll(`[data-fmt="${key}"]`)])]
        .filter(Boolean);
      formatToggleCache.set(key, buttons);
    }
    return buttons;
  }



  /** One literal for one meaning: the chooser's clear row, and the palette's. */
  const NO_TABLE_STYLE = "No table style";

  function populateTableStyles() {
    ed.tableStyleMenu.replaceChildren();
    const clear = document.createElement("button");
    clear.type = "button";
    clear.className = "table-style-choice table-style-clear";
    clear.dataset.tableStyle = "";
    clear.textContent = NO_TABLE_STYLE;
    ed.tableStyleMenu.appendChild(clear);
    for (const style of ed.doc?.listTableStyles?.() || []) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "table-style-choice";
      button.dataset.tableStyle = style;
      button.innerHTML = `<span class="table-style-swatch" aria-hidden="true"><i></i><i></i><i></i></span><span>${escapeHtml(style)}</span>`;
      ed.tableStyleMenu.appendChild(button);
    }
  }

  ed.tableStyleMenu.addEventListener("click", (event) => {
    const choice = event.target.closest("[data-table-style]");
    if (!choice || !ed.selection || !ed.doc) return;
    closePopover(ed.tableStylePopover);
    ed.runEdit(() => ed.doc.applyTableStyle(ed.selection.focus.node, choice.dataset.tableStyle), { gate: true });
  });

  /** Wires a chrome control to `handler` — the seam 44 command call sites and
   *  every popover trigger go through.
   *
   *  The mousedown listener exists ONLY for its preventDefault: that is what stops
   *  the button taking focus and collapsing the document selection the command is
   *  about to act on. The command itself runs on `click`, i.e. on mouse-UP over
   *  the control, which is what makes a mis-press abortable — press "Clear direct
   *  formatting" or "Delete row", drag off the button, release, and nothing
   *  happens, exactly as in Word and Docs. Running it from mousedown, as this used
   *  to, gave the user no way out of a slip (WCAG 2.5.2 Pointer Cancellation,
   *  Level A — docs/104 HF-069).
   *
   *  A keyboard activation arrives as a `click` with `detail === 0` and no
   *  preceding mouse event, so both routes are the same one listener. */
  /** Opens the Insert link dialog for the selected same-paragraph text (⌘K, the
   * ribbon Link button, and the command palette). The dialog owns the external
   * URL / bookmark / ScreenTip entry and applies through the same gated
   * setHyperlink path the raw prompt used. */
  function editSelectionLink() {
    if (!ed.doc || !ed.selection || !ed.hasRange()) return;
    const { anchor, focus } = ed.selection;
    if (anchor.node !== focus.node) {
      ed.setStatus("Links must stay within one paragraph", "error");
      return;
    }
    const start = Math.min(anchor.offset, focus.offset);
    const end = Math.max(anchor.offset, focus.offset);
    const text = ed.doc.copyText(anchor.node, start, anchor.node, end);
    ed.openLinkDialog({ node: anchor.node, start, end, text });
  }

  // Every Insert ribbon button runs the very command its menu row and palette
  // entry run — one implementation behind three surfaces, wired through the shared
  // `onButton` seam so pointer activation never steals the selection. Insert table
  // is the exception: it has no direct action, it opens the row × column grid
  // popover, and is wired by `registerPopover` where that popover is built.
  // Every Review ribbon button runs the very command its menu row and palette
  // entry run, through the shared `onButton` seam, and carries its command id so
  // the ribbon's membership is readable from the DOM for the parity test.
  for (const entry of ed.REVIEW_SURFACE) {
    for (const button of entry.buttons()) {
      if (!button) continue;
      // `ownsClick` as on the Layout table: the module that renders the surface binds
      // the click, and a second handler would run a toggle twice — i.e. not at all.
      if (!entry.ownsClick) onButton(button, entry.run);
      button.dataset.command = entry.command;
    }
  }

  for (const entry of ed.INSERT_SURFACE) {
    for (const button of entry.buttons) {
      if (entry.activate) onButton(button, entry.activate);
      // Stamp the command id onto the button so the ribbon's membership is readable
      // from the DOM. `insert-surface.spec.mjs` compares this set against the Insert
      // menu's own `data-command` ids, which is what actually catches the drift this
      // table is meant to prevent: a command added to the menu and the palette but
      // never given a ribbon button — exactly how Picture shipped unreachable.
      button.dataset.command = entry.command;
    }
  }

  // Layout and References, wired the same way. The `missing` rows get NO handler:
  // the button stays disabled for the life of the build, so a click can never be
  // dispatched, and wiring a run that cannot work would be the dead control the
  // house rule forbids.
  for (const entry of [...ed.LAYOUT_SURFACE, ...ed.REFERENCE_SURFACE]) {
    for (const button of entry.buttons()) {
      if (!button) continue;
      button.dataset.command = entry.command;
      // Fallback for a control with no i18n key. Captured at BOOT, before a
      // catalogue exists, so always English — hence `authoredTitle`'s preference.
      button.dataset.enabledTitle = button.title;
      if (entry.run && !entry.ownsClick) onButton(button, entry.run);
    }
  }
  for (const key of ["bold", "italic", "underline", "strike"]) {
    onButton(ed.fmtButtons[key], () => toggleFormat(key));
  }
  onButton(ed.clearFormattingBtn, () => {
    if (!ed.hasRange()) {
      // A caret is not a range, so there is nothing to clear — but the control is
      // enabled (the caret IS a selection), so returning in silence made Clear
      // formatting a dead control on every surface that runs it: the ribbon
      // button, the Format menu row, the palette and the compact bar all end up
      // here. It refuses out loud instead, the same way the suggesting-mode
      // branch below already does.
      ed.setStatus("Select the text whose formatting you want to clear", "error");
      return;
    }
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Clear formatting is not tracked; switch to Editing to apply it", "error");
      return;
    }
    ed.runToolbarEdit((a, b, c, d) => ed.doc.clearFormatting(a, b, c, d));
  });

  return {
    NO_TABLE_STYLE, UNDERLINE_STYLE_LABELS, caretFormatState, editSelectionLink,
    formatToggleButtons, formatToggleCache, hexToRgb, populateTableStyles, reflectFontFamily,
    reflectUnderlineControl, selectionFormat, suggestAcrossParagraphs, toggleFormat,
  };
}
