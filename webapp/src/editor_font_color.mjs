// Text colour and highlight: the swatch popovers, recent colours, and the
// underline style menu. Moved out of `main.js` unchanged (docs/177); it reaches
// the rest of the editor only through the context `ed`.
import { bindRadioGroup } from "./radio_group.mjs";
import { closePopover, registerPopover } from "./popover_manager.mjs";
import { HIGHLIGHT_COLORS, HIGHLIGHT_LABEL, TEXT_STANDARD_COLORS, highlightHex } from "./palettes.mjs";
import { makeMenuHeading, makeSwatchCell, makeSwatchGrid, makeUnderlineStyleOption, renderColorMenu as renderColorMenuChrome } from "./picker_chrome.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createFontColor(ed) {
  // -- Font family menu, color/highlight pickers, grow/shrink, change case -------
  // (Q1/Q2/Q5) Real dropdown swatch pickers replace the raw OS color input and
  // the native highlight <select>; a searchable font menu replaces the native
  // font <select>; A⁺/A⁻ step the standard sizes; a Change case menu transforms
  // the selection through the existing rich-run copy/paste ops (no new engine op).

  // The swatch vocabularies themselves live in `palettes.mjs`: literal data plus
  // one lookup, with the engine-parity guard that keeps the highlight list from
  // falling behind `HighlightColor`.

  // Session-remembered recently-used swatches (most-recent first, deduped, capped).
  const recentTextColors = [];
  const recentHighlights = [];
  const recentUnderlineColors = [];
  function recordRecent(list, value) {
    const i = list.indexOf(value);
    if (i !== -1) list.splice(i, 1);
    list.unshift(value);
    if (list.length > 10) list.length = 10;
  }


  /** Reflect the current text color onto the "A" underline bar (and remember it as
   *  the color the split-button's apply half reapplies). */
  function reflectTextColorSwatch(hex) {
    if (hex) ed.lastTextColor = hex;
    ed.textColorBar.style.background = ed.lastTextColor;
    if (ed.selTextColorBar) ed.selTextColorBar.style.background = ed.lastTextColor;
  }
  /** Reflect the current highlight onto the highlighter bar (transparent for none). */
  function reflectHighlightSwatch(name) {
    if (name && name !== "none") ed.lastHighlight = name;
    const hex = name === "none" ? null : highlightHex(name || ed.lastHighlight);
    ed.highlightBar.style.background = hex || "transparent";
    ed.highlightBar.classList.toggle("is-none", !hex);
    if (ed.selHighlightBar) {
      ed.selHighlightBar.style.background = hex || "transparent";
      ed.selHighlightBar.classList.toggle("is-none", !hex);
    }
  }

  /** (Re)renders a color picker menu, marking the active swatch and refreshing the
   *  recently-used row. Called on each open via the popover's reflect hook. The
   *  menu's SHAPE is `picker_chrome.mjs`; what is here is the two vocabularies and
   *  the session state that decide which swatch is lit. */
  function renderColorMenu(kind, menu = kind === "text" ? ed.textColorMenu : ed.highlightMenu) {
    const isText = kind === "text";
    const active = isText ? ed.lastTextColor.toLowerCase() : ed.lastHighlight;
    renderColorMenuChrome(menu, {
      kind,
      swatches: isText ? TEXT_STANDARD_COLORS : HIGHLIGHT_COLORS.map((c) => c.name),
      recents: isText ? recentTextColors : recentHighlights,
      colorOf: (v) => (isText ? v : highlightHex(v) ?? "#000000"),
      labelOf: (v) => (isText ? v.toUpperCase() : HIGHLIGHT_LABEL.get(v) ?? v),
      matches: (v) => (isText ? v.toLowerCase() === active : v === active),
    });
  }

  /** Builds the combined underline style/color menu from canonical engine tokens.
   *  The explicit Automatic row maps to `Some(None)` in the edit delta. */
  function renderUnderlineMenu() {
    ed.underlineMenu.replaceChildren();
    ed.underlineMenu.appendChild(makeMenuHeading("Underline style"));
    const styleGroup = document.createElement("div");
    styleGroup.setAttribute("role", "radiogroup");
    styleGroup.setAttribute("aria-label", "Underline style");
    for (const [style, label] of ed.UNDERLINE_STYLE_LABELS) {
      styleGroup.appendChild(makeUnderlineStyleOption(style, label));
    }
    ed.underlineMenu.appendChild(styleGroup);
    // The menu is rebuilt on every open, so the group is bound per render. It is a
    // real radio group — one of the eleven underline styles is always the current
    // one — and it now gets the same single Tab stop and arrow behaviour as every
    // other segmented control instead of eleven Tab stops and dead arrows.
    bindRadioGroup(styleGroup, {
      attr: "data-underline-style",
      onSelect: (style) => applyUnderlineStyle(style),
    }).reflect(ed.currentUnderlineMixed ? null : ed.currentUnderlineStyle);

    ed.underlineMenu.appendChild(makeMenuHeading("Underline color"));
    const automatic = document.createElement("button");
    automatic.type = "button";
    automatic.className = "color-row-action";
    automatic.dataset.underlineAuto = "1";
    automatic.innerHTML = '<span class="color-chip" style="--sw:#000000"></span><span>Automatic (text color)</span>';
    automatic.classList.toggle("is-active", !ed.currentUnderlineMixed && !ed.currentUnderlineColor);
    ed.underlineMenu.appendChild(automatic);
    ed.underlineMenu.appendChild(makeSwatchGrid(
      TEXT_STANDARD_COLORS.map((hex) =>
        makeSwatchCell(
          "text",
          hex,
          hex,
          `Underline ${hex.toUpperCase()}`,
          !ed.currentUnderlineMixed && hex.toLowerCase() === ed.currentUnderlineColor.toLowerCase(),
        )),
    ));
    if (recentUnderlineColors.length) {
      ed.underlineMenu.appendChild(makeMenuHeading("Recent"));
      ed.underlineMenu.appendChild(makeSwatchGrid(
        recentUnderlineColors.map((hex) =>
          makeSwatchCell(
            "text",
            hex,
            hex,
            `Underline ${hex.toUpperCase()}`,
            !ed.currentUnderlineMixed && hex.toLowerCase() === ed.currentUnderlineColor.toLowerCase(),
          )),
      ));
    }
    const more = document.createElement("button");
    more.type = "button";
    more.className = "color-row-action color-more";
    more.dataset.underlineMore = "1";
    more.innerHTML = '<span class="ms" aria-hidden="true">colorize</span><span>More colors…</span>';
    ed.underlineMenu.appendChild(more);
  }

  const textColorPopover = registerPopover(ed.textColorCaret, ed.textColorMenu, () => renderColorMenu("text"));
  const highlightPopover = registerPopover(ed.highlightCaret, ed.highlightMenu, () => renderColorMenu("highlight"));
  const underlinePopover = registerPopover(ed.underlineMenuBtn, ed.underlineMenu, renderUnderlineMenu);

  function underlineEditBlockedByReview() {
    if (ed.reviewMode !== "suggesting") return false;
    ed.setStatus(
      "Underline style and color are not tracked yet; switch to Editing to apply them",
      "error",
    );
    return true;
  }

  function applyUnderlineStyle(style) {
    if (underlineEditBlockedByReview()) return;
    const patch = style === "none"
      ? { underline: false, underlineStyle: "single", underlineColor: "" }
      : { underline: true, underlineStyle: style };
    ed.armOrApplyRun(patch, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setUnderlineStyle(a, b, c, d, style)),
    );
  }

  function applyUnderlineColor(color) {
    if (underlineEditBlockedByReview()) return;
    const patch = color ? { underline: true, underlineColor: color } : { underlineColor: "" };
    ed.armOrApplyRun(patch, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setUnderlineColor(a, b, c, d, color)),
    );
  }

  ed.underlineMenu.addEventListener("click", (e) => {
    // The style radio group applies the pick itself (so the arrow keys work, which
    // is the whole of `109` UX-021); what a POINTER pick adds is dismissal.
    if (e.target.closest("[data-underline-style]")) {
      closePopover(underlinePopover);
      ed.focusEditorSurface();
      return;
    }
    if (e.target.closest("[data-underline-more]")) {
      ed.underlineColorInput.value = ed.currentUnderlineColor || "#000000";
      ed.underlineColorInput.click();
      return;
    }
    const colorCell = e.target.closest("[data-color], [data-underline-auto]");
    if (!colorCell) return;
    const color = colorCell.dataset.underlineAuto ? "" : colorCell.dataset.color;
    applyUnderlineColor(color);
    if (color) recordRecent(recentUnderlineColors, color);
    closePopover(underlinePopover);
    ed.focusEditorSurface();
  });
  ed.underlineColorInput.addEventListener("change", () => {
    const color = ed.underlineColorInput.value;
    applyUnderlineColor(color);
    recordRecent(recentUnderlineColors, color);
    closePopover(underlinePopover);
    ed.focusEditorSurface();
  });

  // One text-color menu-click handler, shared by the ribbon menu and the floating
  // selection-toolbar menu. `popover` is the popover to close after applying; the
  // apply goes through the same `applyTextColor` path (one undoable action, gated
  // in Viewing/Suggesting). (mousedown is preventDefault'd by the popover manager,
  // so the document selection survives the pointer press.)
  function handleTextColorMenuClick(e, popover) {
    if (e.target.closest("[data-more]")) {
      ed.textColorInput.value = ed.lastTextColor;
      ed.textColorInput.click(); // opens the OS color input as the custom fallback
      return;
    }
    const cell = e.target.closest("[data-color], [data-auto]");
    if (!cell) return;
    const hex = cell.dataset.auto ? "#000000" : cell.dataset.color;
    ed.applyTextColor(hex);
    ed.lastTextColor = hex;
    if (!cell.dataset.auto) recordRecent(recentTextColors, hex);
    reflectTextColorSwatch(hex);
    closePopover(popover);
    ed.focusEditorSurface();
  }
  function handleHighlightMenuClick(e, popover) {
    const cell = e.target.closest("[data-highlight]");
    if (!cell) return;
    const name = cell.dataset.highlight;
    ed.applyHighlight(name);
    if (name !== "none") {
      ed.lastHighlight = name;
      recordRecent(recentHighlights, name);
    }
    reflectHighlightSwatch(name);
    closePopover(popover);
    ed.focusEditorSurface();
  }

  ed.textColorMenu.addEventListener("click", (e) => handleTextColorMenuClick(e, textColorPopover));
  // "More colors…" custom fallback commits when the OS picker closes.
  ed.textColorInput.addEventListener("change", () => {
    const hex = ed.textColorInput.value;
    ed.applyTextColor(hex);
    ed.lastTextColor = hex;
    recordRecent(recentTextColors, hex);
    reflectTextColorSwatch(hex);
  });

  ed.highlightMenu.addEventListener("click", (e) => handleHighlightMenuClick(e, highlightPopover));

  // Floating selection-toolbar pickers: same renderer, same apply path, popovers
  // anchored to the floating buttons so they open next to the selection. Registered
  // after the ribbon popovers so the ribbon behavior is untouched.
  const selTextColorPopover = registerPopover(ed.selTextColorBtn, ed.selTextColorMenu, () =>
    renderColorMenu("text", ed.selTextColorMenu),
  );
  const selHighlightPopover = registerPopover(ed.selHighlightBtn, ed.selHighlightMenu, () =>
    renderColorMenu("highlight", ed.selHighlightMenu),
  );
  ed.selTextColorMenu.addEventListener("click", (e) => handleTextColorMenuClick(e, selTextColorPopover));
  ed.selHighlightMenu.addEventListener("click", (e) => handleHighlightMenuClick(e, selHighlightPopover));

  return {
    applyUnderlineStyle, highlightPopover, recentHighlights, recentTextColors, recordRecent,
    reflectHighlightSwatch, reflectTextColorSwatch, textColorPopover,
  };
}
