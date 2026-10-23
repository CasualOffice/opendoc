// The format painter (copy formatting, paint it onto a target), font size,
// indents, line breaks and the checklist toggle. Moved out of `main.js`
// unchanged (docs/177); it reaches the rest of the editor only through the
// context `ed`.
import { configurePopovers, onButton, registerPopover } from "./popover_manager.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFormatPainter(ed) {
  // ---- Format painter (Word/Docs "copy formatting → paint onto target") -------


  /** Snapshots the current selection/caret's run + paragraph formatting into a
   *  plain patch. Mixed run properties (and automatic/theme colors) are left out
   *  so painting never forces a single value onto a genuinely mixed source. */
  function captureFormatForPainter() {
    if (!ed.doc || !ed.selection) return null;
    const range = ed.hasRange();
    const [sn, so, en, eo] = ed.selEndpoints();
    const f = range
      ? ed.doc.selectionFormat(sn, so, en, eo)
      : ed.doc.caretFormat(ed.selection.focus.node, ed.selection.focus.offset);
    const fmt = { bold: f.bold, italic: f.italic, underline: f.underline, strike: f.strike };
    f.free();
    const rs = range
      ? ed.doc.selectionRunStyle(sn, so, en, eo)
      : ed.doc.caretRunStyle(ed.selection.focus.node, ed.selection.focus.offset);
    if (!rs.sizeMixed && rs.sizePoints) fmt.sizePoints = rs.sizePoints;
    if (!rs.fontMixed && rs.font) fmt.font = rs.font;
    if (!rs.colorMixed && rs.color) fmt.color = rs.color; // skip automatic/theme (empty)
    if (!rs.highlightMixed && rs.highlight) fmt.highlight = rs.highlight; // includes "none"
    if (!rs.verticalAlignMixed && rs.verticalAlign) fmt.vertAlign = rs.verticalAlign;
    if (fmt.underline && !rs.underlineStyleMixed && rs.underlineStyle) {
      fmt.underlineStyle = rs.underlineStyle;
    }
    if (fmt.underline && !rs.underlineColorMixed) fmt.underlineColor = rs.underlineColor;
    rs.free();
    // Paragraph formatting reachable through absolute getter/setter pairs. Indent
    // and paragraph spacing lack absolute copy ops today (relative-only), so they
    // are intentionally not painted (tracked as a follow-up).
    const node = ed.selection.focus.node;
    fmt.align = ed.doc.alignmentAt(node, ed.selection.focus.offset);
    const style = ed.doc.paragraphStyleAt(node);
    if (style) fmt.paraStyle = style;
    const line = ed.doc.lineSpacingAt(node);
    if (line) fmt.lineSpacing = line;
    return fmt;
  }

  /** Applies the captured format to the current range via the same edit ops the
   *  toolbar uses. Returns whether anything was applied. */
  async function applyPaintedFormat() {
    const fmt = ed.formatPainter?.fmt;
    if (!fmt || !ed.doc || !ed.hasRange()) return false;
    if (ed.blockMutationInViewing()) return false;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Format painter isn't tracked yet; switch to Editing to paint formatting", "error");
      return false;
    }
    // Paragraph style first, so painted direct formatting overrides the style.
    if (fmt.paraStyle) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setParagraphStyle(a, b, c, d, fmt.paraStyle), { paragraphLevel: true });
    await ed.runToolbarEdit((a, b, c, d) =>
      ed.doc.formatSelection(a, b, c, d, fmt.bold, fmt.italic, fmt.underline, fmt.strike),
    );
    if (fmt.underlineStyle) {
      await ed.runToolbarEdit((a, b, c, d) => ed.doc.setUnderlineStyle(a, b, c, d, fmt.underlineStyle));
    }
    if (fmt.underlineColor != null) {
      await ed.runToolbarEdit((a, b, c, d) => ed.doc.setUnderlineColor(a, b, c, d, fmt.underlineColor));
    }
    if (fmt.sizePoints != null) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setFontSize(a, b, c, d, fmt.sizePoints));
    if (fmt.font) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setFont(a, b, c, d, fmt.font));
    if (fmt.color) {
      const [r, g, b] = ed.hexToRgb(fmt.color);
      await ed.runToolbarEdit((a, x, c, d) => ed.doc.setTextColor(a, x, c, d, r, g, b));
    }
    if (fmt.highlight) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setHighlight(a, b, c, d, fmt.highlight));
    if (fmt.vertAlign) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setVertAlign(a, b, c, d, fmt.vertAlign));
    if (fmt.align) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setAlignment(a, b, c, d, fmt.align), { paragraphLevel: true });
    if (fmt.lineSpacing) await ed.runToolbarEdit((a, b, c, d) => ed.doc.setLineSpacing(a, b, c, d, fmt.lineSpacing), { paragraphLevel: true });
    ed.updateToolbar();
    return true;
  }

  /** Reflects the painter's armed / sticky state on the toolbar button and body
   *  (the body flag drives the paintbrush cursor affordance over the pages). */
  function reflectFormatPainter() {
    const armed = !!ed.formatPainter;
    ed.formatPainterBtn.setAttribute("aria-pressed", String(armed));
    ed.formatPainterBtn.classList.toggle("is-sticky", !!ed.formatPainter?.sticky);
    document.body.classList.toggle("is-format-painting", armed);
  }

  function armFormatPainter(sticky) {
    if (!ed.doc || !ed.selection) {
      ed.setStatus("Place the caret in text to copy its formatting", "error");
      return;
    }
    const fmt = captureFormatForPainter();
    if (!fmt) {
      ed.setStatus("Place the caret in text to copy its formatting", "error");
      return;
    }
    ed.formatPainter = { fmt, sticky: !!sticky };
    reflectFormatPainter();
    ed.setStatus(
      sticky
        ? "Format painter locked — paint successive selections; Esc or click the brush to stop"
        : "Format painter — click a word or drag over text to paint the copied formatting",
    );
  }

  function disarmFormatPainter(reason) {
    if (!ed.formatPainter) return;
    ed.formatPainter = null;
    reflectFormatPainter();
    if (reason) ed.setStatus(reason);
  }

  /** Consumes a document pointer gesture as a paint target: a drag's range, or the
   *  word under a bare click. Disarms afterward unless the painter is locked. */
  async function paintFormatFromGesture(gesture, event) {
    if (!ed.hasRange()) {
      const page = gesture?.page || ed.pageFromClientPoint(event.clientX, event.clientY);
      if (page) ed.touchSelection.selectWord(page, event);
    }
    const painted = ed.hasRange() ? await applyPaintedFormat() : false;
    if (!ed.formatPainter?.sticky) disarmFormatPainter();
    else if (painted) ed.setStatus("Painted — keep painting or press Esc to stop");
  }

  // Preserve the model selection on mousedown (like every other toolbar control),
  // then arm/lock/cancel on click. `detail` distinguishes single vs double click:
  // double-click locks sticky mode, a single click on an armed brush cancels it.
  ed.formatPainterBtn.addEventListener("mousedown", (e) => e.preventDefault());
  ed.formatPainterBtn.addEventListener("click", (e) => {
    e.preventDefault();
    if (ed.formatPainterBtn.disabled) return;
    if (e.detail >= 2) {
      armFormatPainter(true);
      return;
    }
    if (ed.formatPainter) {
      disarmFormatPainter("Format painter off");
      return;
    }
    armFormatPainter(false);
  });

  // Escape cancels the painter before any other Escape handler (menu/selection),
  // so a stray Escape always makes the brush the first thing it puts down.
  document.addEventListener(
    "keydown",
    (e) => {
      if (e.key !== "Escape") return;
      // An armed shape is a MODE, and a mode you cannot leave is the worst kind.
      // Ahead of the painter and of every selection handler, for the same reason
      // the painter is ahead of them: a stray Escape should put the tool down.
      if (ed.shapeDrawMode.armedToken()) {
        e.preventDefault();
        e.stopPropagation();
        ed.shapeDrawMode.disarm(true);
        return;
      }
      if (ed.formatPainter) {
        e.preventDefault();
        e.stopPropagation();
        disarmFormatPainter("Format painter off");
      }
    },
    true,
  );

  function suggestRunFormat(patch) {
    return ed.runToolbarEdit((sn, so, en, eo) => {
      if (sn !== en) throw new Error("Tracked formatting requires one paragraph");
      return ed.doc.suggestFormat(
        sn,
        Math.min(so, eo),
        Math.max(so, eo),
        patch.bold,
        patch.italic,
        patch.underline,
        patch.strike,
        patch.sizeHalfPoints,
        patch.color,
        patch.highlight,
        patch.vertAlign,
        patch.font,
        undefined,
        new Date().toISOString(),
      );
    }, { allowInSuggesting: true });
  }
  /** A run-format control: apply to a range, or arm into `pendingFormat` at a caret
   *  (so the next typed text carries it — same model as the B/I/U/S toggles). */
  function armOrApplyRun(patch, applyFn) {
    if (ed.hasRange()) {
      if (ed.reviewMode === "suggesting") suggestRunFormat(patch);
      else applyFn();
    } else if (ed.selection) {
      ed.pendingFormat = { ...(ed.pendingFormat || {}), ...patch };
      ed.updateToolbar();
    }
  }
  onButton(ed.superBtn, () => {
    const value = ed.superBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "super";
    armOrApplyRun({ vertAlign: value }, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setVertAlign(a, b, c, d, value)),
    );
  });
  onButton(ed.subBtn, () => {
    const value = ed.subBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "sub";
    armOrApplyRun({ vertAlign: value }, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setVertAlign(a, b, c, d, value)),
    );
  });
  for (const [key, btn] of Object.entries(ed.alignBtns)) {
    onButton(btn, () => ed.runToolbarEdit((a, b, c, d) => ed.doc.setAlignment(a, b, c, d, key), { paragraphLevel: true }));
  }
  /** Word-style indent commands: list items change numbering level, while ordinary
   * paragraphs retain the existing 0.25in paragraph-indent behavior. */
  function adjustIndentCommand(delta) {
    if (!ed.selection || !ed.doc) return;
    const listKind = ed.doc.listStyleAt(ed.selection.focus.node);
    ed.runToolbarEdit((a, b, c, d) =>
      listKind
        ? ed.doc.adjustListLevel(a, b, c, d, delta > 0 ? 1 : -1)
        : ed.doc.adjustIndent(a, b, c, d, delta), { paragraphLevel: true });
  }
  onButton(ed.indentDecBtn, () => adjustIndentCommand(-360));
  onButton(ed.indentIncBtn, () => adjustIndentCommand(360));
  onButton(ed.bulletListBtn, () => ed.runToolbarEdit((a, b, c, d) => ed.doc.toggleList(a, b, c, d, "bullet"), { paragraphLevel: true }));
  onButton(ed.numberedListBtn, () => ed.runToolbarEdit((a, b, c, d) => ed.doc.toggleList(a, b, c, d, "numbered"), { paragraphLevel: true }));
  /** Toggles the caret's paragraphs into (or out of) a checklist. Shared by the
   *  ribbon button and the `paragraph.list.checklist` command, so the two cannot
   *  diverge — the command used not to exist at all, which left the checklist
   *  reachable only by finding one button on the Home tab. */
  function toggleChecklistCommand() {
    ed.runToolbarEdit((a, b, c, d) => ed.doc.toggleList(a, b, c, d, "checklist"), { paragraphLevel: true });
    // A brand-new checklist introduces the `☐` marker glyph; fetch its covering
    // symbol font (once) so it renders instead of a .notdef box, then re-render.
    void ed.ensureGlyphCoverage("checklist");
  }
  onButton(ed.checkListBtn, toggleChecklistCommand);
  onButton(ed.restartListBtn, () => {
    if (ed.selection && ed.doc) ed.runNodeEdit(() => ed.doc.restartList(ed.selection.focus.node));
  });
  onButton(ed.continueListBtn, () => {
    if (ed.selection && ed.doc) ed.runNodeEdit(() => ed.doc.continueList(ed.selection.focus.node));
  });

  /** Inserts a SOFT line break at the caret — the Shift+Enter gesture.
   *
   * The text stays in one paragraph and so keeps its list membership, style,
   * numbering and spacing. Extracted from the Enter handler so `insert.lineBreak`
   * runs exactly this and not a second, quietly different version of it: the
   * gesture was keyboard-only, which is the same one-surface defect docs/104 keeps
   * recording. */
  async function insertLineBreakAtSelection() {
    if (!ed.doc || !ed.selection) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus("Line breaks cannot be tracked yet; switch to Editing to insert one", "error");
      return;
    }
    // A non-collapsed selection is replaced first, exactly as typing a character
    // would — otherwise the break lands beside text the user meant to overwrite.
    if (ed.hasRange()) {
      const { anchor, focus } = ed.selection;
      const ok = await ed.runEdit(() =>
        ed.doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset),
      );
      if (!ok || !ed.selection) return;
    }
    const at = ed.selection.focus;
    await ed.runEdit(() => ed.doc.insertLineBreak(at.node, at.offset));
  }

  /** Applies an exact point size, or reports why it cannot.
   *
   * Shared by the ribbon's size field and by the `format.size` command, so a
   * caller asking for 14pt goes through the same validation the typed value does
   * instead of a second, quietly different rule. `report` is off for callers with
   * no field to attach a validation bubble to. */
  function applyFontSize(points, { report = true } = {}) {
    const pt = Number(points);
    const valid = Number.isFinite(pt) && pt >= 1 && pt <= 1638 && Number.isInteger(pt * 2);
    if (report) {
      ed.fontSizeSel.setCustomValidity(
        valid ? "" : "Enter a font size from 1 to 1638 pt in 0.5 pt steps.",
      );
    }
    if (!valid) {
      if (report) {
        ed.fontSizeSel.reportValidity();
        ed.updateToolbar();
      }
      return false;
    }
    armOrApplyRun({ sizeHalfPoints: Math.round(pt * 2) }, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setFontSize(a, b, c, d, pt)),
    );
    return true;
  }

  ed.fontSizeSel.addEventListener("change", () => {
    if (ed.fontSizeSel.value.trim() === "") {
      ed.fontSizeSel.setCustomValidity("Enter a font size from 1 to 1638 pt in 0.5 pt steps.");
      ed.fontSizeSel.reportValidity();
      ed.updateToolbar();
      return;
    }
    applyFontSize(ed.fontSizeSel.value);
  });
  // Toolbar popovers are `popover_manager.mjs`: anchoring, one-at-a-time, light
  // dismiss, Escape and focus return, for the ribbon's menus and the footer's
  // alike. The manager does not know what a selection is; this is the one thing
  // it asks the editor, so a menu that would act on the caret does not open when
  // there is no caret to act on.
  configurePopovers({ documentReady: () => !!ed.selection || !!ed.objectSelection });
  const TWIPS_PER_POINT = 20;
  ed.tableStylePopover = registerPopover(ed.tableStyleBtn, ed.tableStyleMenu, () => {});

  // The Styles control: ONE trigger showing the caret's style, opening a SHORT list —
  // Google Docs' shape exactly (docs/115). Not a native `<select>`, because the reason
  // the list is worth opening is that each row is drawn in the style it applies, and an
  // OS popup renders every row in the system font. Not a card gallery either: a gallery
  // spends band width on options nobody is choosing right now and never says, unopened,
  // which style you are in.
  //
  // Still no "More styles" ▾: that listed every style in the document, which is the
  // deleted 14-entry select's long list reached through a side door. The full
  // stylesheet stays off the band — command palette, Paragraph properties (docs/115 §5).
  ed.stylesMenuInput.addEventListener("input", () => ed.renderStylesGallery());
  const stylesPopover = registerPopover(ed.stylesTrigger, ed.stylesMenu, () => {
    // Rebuild on open rather than on every caret move: the suggested set depends
    // on the caret's style, and the menu is not on screen while the caret moves.
    ed.stylesMenuInput.value = "";
    ed.renderStylesGallery();
    ed.syncStylesGalleryActive();
  });

  return {
    TWIPS_PER_POINT, adjustIndentCommand, applyFontSize, armFormatPainter, armOrApplyRun,
    disarmFormatPainter, insertLineBreakAtSelection, paintFormatFromGesture, stylesPopover,
    toggleChecklistCommand,
  };
}
