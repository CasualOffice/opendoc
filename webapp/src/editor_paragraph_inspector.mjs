// The Paragraph properties inspector, the indent and shading controls, and
// applying a text colour or highlight. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { closeAllPopovers, onButton } from "./popover_manager.mjs";
import { inchesToTwips, twipsToInchText } from "./units.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createParagraphInspector(ed) {
  // -- Paragraph properties inspector ------------------------------------------
  /** An inches field's value → twips (≥ 0); "" or non-numeric → 0. */
  function inchTwips(input) {
    return inchesToTwips(input.value);
  }

  /** An inches field's value → signed twips; blank/non-numeric → 0. */
  function setMixedCheckbox(input, state) {
    input.indeterminate = state === 2;
    input.checked = state === 1;
  }

  function reflectParagraphProperties() {
    if (!ed.doc || !ed.selection) return;
    const [startNode, startOffset, endNode, endOffset] = ed.selEndpoints();
    ed.paragraphAdvanced.reflect();
    ed.listAdvanced.reflect();
    ed.styleAdvanced.reflect();
    const state = ed.doc.selectionParagraphState(startNode, startOffset, endNode, endOffset);
    ed.paragraphPropertiesContext.textContent =
      state.count === 1 ? "1 paragraph" : `${state.count} paragraphs`;

    ed.paraPanelStyle.options[0].textContent = state.styleMixed ? "Mixed" : "Style";
    if (document.activeElement !== ed.paraPanelStyle) {
      ed.paraPanelStyle.value = state.styleMixed ? "" : state.style;
    }
    for (const button of ed.paraPanelAlign.querySelectorAll("button[data-palign]")) {
      button.setAttribute(
        "aria-pressed",
        state.alignmentMixed ? "mixed" : String(button.dataset.palign === state.alignment),
      );
    }

    if (document.activeElement !== ed.indentLeftInput) {
      ed.indentLeftInput.placeholder = state.startMixed ? "Mixed" : "";
      ed.indentLeftInput.value = state.startMixed ? "" : twipsToInchText(state.startTwip);
    }
    if (document.activeElement !== ed.indentRightInput) {
      ed.indentRightInput.placeholder = state.endMixed ? "Mixed" : "";
      ed.indentRightInput.value = state.endMixed ? "" : twipsToInchText(state.endTwip);
    }
    if (![ed.indentSpecialByInput, ed.indentSpecialSel].includes(document.activeElement)) {
      if (state.firstLineMixed || state.hangingMixed) {
        ed.indentSpecialSel.value = "";
        ed.indentSpecialByInput.value = "";
        ed.indentSpecialByInput.placeholder = "Mixed";
      } else if (state.firstLineTwip > 0) {
        ed.indentSpecialSel.value = "first";
        ed.indentSpecialByInput.value = twipsToInchText(state.firstLineTwip);
        ed.indentSpecialByInput.placeholder = "";
      } else if (state.hangingTwip > 0) {
        ed.indentSpecialSel.value = "hanging";
        ed.indentSpecialByInput.value = twipsToInchText(state.hangingTwip);
        ed.indentSpecialByInput.placeholder = "";
      } else {
        ed.indentSpecialSel.value = "none";
        ed.indentSpecialByInput.value = "";
        ed.indentSpecialByInput.placeholder = "";
      }
    }

    if (document.activeElement !== ed.paraLineSpacing) {
      ed.paraLineSpacing.value = state.lineMixed ? "" : String(state.linePercent || "");
    }
    if (document.activeElement !== ed.paraSpaceBefore) {
      ed.paraSpaceBefore.placeholder = state.beforeMixed ? "Mixed" : "";
      ed.paraSpaceBefore.value =
        state.beforeMixed || state.beforeTwip < 0
          ? ""
          : String(Math.round(state.beforeTwip / ed.TWIPS_PER_POINT));
    }
    if (document.activeElement !== ed.paraSpaceAfter) {
      ed.paraSpaceAfter.placeholder = state.afterMixed ? "Mixed" : "";
      ed.paraSpaceAfter.value =
        state.afterMixed || state.afterTwip < 0
          ? ""
          : String(Math.round(state.afterTwip / ed.TWIPS_PER_POINT));
    }

    setMixedCheckbox(ed.pgKeepNext, state.keepNextState);
    setMixedCheckbox(ed.pgKeepLines, state.keepLinesState);
    setMixedCheckbox(ed.pgBreakBefore, state.pageBreakBeforeState);
    ed.paraShadeMixed.hidden = !state.shadingMixed;
    ed.paraShadeNone.setAttribute(
      "aria-pressed",
      state.shadingMixed ? "mixed" : String(state.shading < 0),
    );
    if (!state.shadingMixed && state.shading >= 0 && document.activeElement !== ed.paraShade) {
      ed.paraShade.value = `#${state.shading.toString(16).padStart(6, "0")}`;
    }
    ed.paraBordersMixed.hidden = !state.bordersMixed;
    const bit = { top: 1, bottom: 2, left: 4, right: 8 };
    for (const b of ed.paragraphPropertiesPanel.querySelectorAll(".border-btn")) {
      const k = b.dataset.border;
      const on =
        k === "box"
          ? state.borderEdges === 0b1111
          : k === "none"
            ? state.borderEdges === 0
            : (state.borderEdges & bit[k]) !== 0;
      b.setAttribute("aria-pressed", state.bordersMixed ? "mixed" : String(on));
    }
    state.free();
  }

  // `focusTarget` lets Layout ▸ Indent and Layout ▸ Spacing land on the fieldset
  // the user asked for. The panel owns the real indent/spacing values — the ribbon
  // deliberately holds no second copy of them — so the deep link is what makes the
  // ribbon button honest about what it does.
  function toggleParagraphProperties(open, focusTarget = null) {
    const show = open ?? ed.paragraphPropertiesPanel.hidden;
    if (show && (!ed.doc || !ed.selection)) return;
    const returnFocus =
      !show && ed.paragraphPropertiesPanel.contains(document.activeElement);
    ed.paragraphPropertiesPanel.hidden = !show;
    ed.paraOptsBtn.setAttribute("aria-expanded", String(show));
    if (show) {
      ed.toggleTableProperties(false);
      closeAllPopovers();
      reflectParagraphProperties();
      // A plain open starts at the top of the panel, as every panel does
      // (approved design P1): focusing the style field without preventScroll
      // used to scroll the body ~2000px down on open. A deep link (Layout ▸
      // Indent / Spacing) still scrolls to the field it asked for.
      const body = ed.paragraphPropertiesPanel.querySelector(".panel-body");
      if (body) body.scrollTop = 0;
      queueMicrotask(() => {
        const target = focusTarget?.();
        if (target) target.focus();
        else ed.paraPanelStyle.focus({ preventScroll: true });
      });
    } else if (returnFocus) {
      ed.paraOptsBtn.focus({ preventScroll: true });
    }
  }

  ed.paraOptsBtn.addEventListener("click", (event) => {
    event.stopPropagation();
    toggleParagraphProperties();
  });
  document.getElementById("paragraphPropertiesClose").addEventListener("click", () =>
    toggleParagraphProperties(false),
  );
  document.addEventListener("keydown", (event) => {
    if (
      event.key === "Escape" &&
      !ed.paragraphPropertiesPanel.hidden &&
      (document.activeElement === ed.paraOptsBtn ||
        ed.paragraphPropertiesPanel.contains(document.activeElement))
    ) {
      event.preventDefault();
      toggleParagraphProperties(false);
    }
  });

  ed.paraPanelStyle.addEventListener("change", () =>
    ed.runToolbarEdit((a, b, c, d) =>
      ed.doc.setParagraphStyle(a, b, c, d, ed.paraPanelStyle.value), { paragraphLevel: true }),
  );
  for (const button of ed.paraPanelAlign.querySelectorAll("button[data-palign]")) {
    onButton(button, () =>
      ed.runToolbarEdit((a, b, c, d) =>
        ed.doc.setAlignment(a, b, c, d, button.dataset.palign), { paragraphLevel: true }),
    );
  }
  ed.paraLineSpacing.addEventListener("change", () => {
    if (!ed.paraLineSpacing.value) return;
    ed.runToolbarEdit((a, b, c, d) =>
      ed.doc.setLineSpacing(a, b, c, d, Number(ed.paraLineSpacing.value)), { paragraphLevel: true });
  });
  /** A points field's value → twips (≥ 0); "" clears the paragraph's own value so
   *  the style decides again, which is what `-1` means to the engine. */
  function applySpace(input, setter) {
    const raw = input.value.trim();
    if (raw !== "" && !Number.isFinite(Number(raw))) return;
    const twips = raw === "" ? -1 : Math.max(0, Math.round(Number(raw) * ed.TWIPS_PER_POINT));
    ed.runToolbarEdit((a, b, c, d) => setter(a, b, c, d, twips));
  }
  ed.paraSpaceBefore.addEventListener("change", () =>
    applySpace(ed.paraSpaceBefore, (a, b, c, d, twips) => ed.doc.setSpaceBefore(a, b, c, d, twips)),
  );
  ed.paraSpaceAfter.addEventListener("change", () =>
    applySpace(ed.paraSpaceAfter, (a, b, c, d, twips) => ed.doc.setSpaceAfter(a, b, c, d, twips)),
  );

  ed.indentLeftInput.addEventListener("change", () =>
    ed.runToolbarEdit((a, b, c, d) => ed.doc.setLeftIndent(a, b, c, d, inchTwips(ed.indentLeftInput)), { paragraphLevel: true }),
  );
  ed.indentRightInput.addEventListener("change", () =>
    ed.runToolbarEdit((a, b, c, d) => ed.doc.setRightIndent(a, b, c, d, inchTwips(ed.indentRightInput)), { paragraphLevel: true }),
  );
  function applyIndentSpecial() {
    const by = inchTwips(ed.indentSpecialByInput);
    const kind = ed.indentSpecialSel.value;
    const twips = kind === "first" ? by : kind === "hanging" ? -by : 0;
    ed.runToolbarEdit((a, b, c, d) => ed.doc.setFirstLineIndent(a, b, c, d, twips), { paragraphLevel: true });
  }
  ed.indentSpecialSel.addEventListener("change", applyIndentSpecial);
  ed.indentSpecialByInput.addEventListener("change", applyIndentSpecial);

  ed.paraShade.addEventListener("change", () => {
    const [r, g, b] = ed.hexToRgb(ed.paraShade.value);
    ed.runToolbarEdit((a, x, c, d) => ed.doc.setParagraphShading(a, x, c, d, r, g, b, false), { paragraphLevel: true });
  });
  onButton(ed.paraShadeNone, () =>
    ed.runToolbarEdit((a, x, c, d) => ed.doc.setParagraphShading(a, x, c, d, 0, 0, 0, true), { paragraphLevel: true }),
  );
  for (const [box, setter] of [
    [ed.pgKeepNext, (a, b, c, d, on) => ed.doc.setKeepWithNext(a, b, c, d, on)],
    [ed.pgKeepLines, (a, b, c, d, on) => ed.doc.setKeepLinesTogether(a, b, c, d, on)],
    [ed.pgBreakBefore, (a, b, c, d, on) => ed.doc.setPageBreakBefore(a, b, c, d, on)],
  ]) {
    box.addEventListener("change", () =>
      ed.runToolbarEdit((a, b, c, d) => setter(a, b, c, d, box.checked)),
    );
  }
  /** Apply a text color (hex `#rrggbb`) to the range or arm it at the caret. */
  function applyTextColor(hex) {
    const [r, g, b] = ed.hexToRgb(hex);
    ed.armOrApplyRun({ color: hex }, () =>
      ed.runToolbarEdit((a, bo, c, d) => ed.doc.setTextColor(a, bo, c, d, r, g, b)),
    );
  }
  /** Apply a named OOXML highlight (or "none") to the range or arm it at the caret. */
  function applyHighlight(name) {
    ed.armOrApplyRun({ highlight: name }, () =>
      ed.runToolbarEdit((a, b, c, d) => ed.doc.setHighlight(a, b, c, d, name)),
    );
  }

  return { applyHighlight, applyTextColor, reflectParagraphProperties, toggleParagraphProperties };
}
