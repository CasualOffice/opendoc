// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads the `casual-doc-wasm` module, opens a user-selected `.docx` fully
// client-side, and blits each rendered page onto a canvas. This is the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import init, { open } from "../pkg/casual_doc_wasm.js";
import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { embedMarker, extractMarker, htmlToRuns, htmlToStructured, runsToHtml } from "./clipboard.mjs";
import {
  compatibilityOccurrenceCount,
  downloadNameForFormat,
  formatInfo,
} from "./format_io.mjs";
import {
  keyboardPlatform,
  lineDeletionDirection,
  navigationDirection,
  wordDeletionDirection,
} from "./keyboard.mjs";
import {
  clampContextMenuPosition,
  moveMenuIndex,
  normalizeMenuEntries,
} from "./context_menu.mjs";

function escapeHtml(text) {
  return String(text)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** url → Uint8Array of already-fetched font bytes (persists across documents). */
const fontCache = new Map();

const statusEl = document.getElementById("status");
const reviewLiveRegion = document.getElementById("reviewLiveRegion");
const fileEl = document.getElementById("file");
const zoomEl = document.getElementById("zoom");
// Zoom (Q4): an editable % plus Fit width / Fit page. `zoomFactor` is the live
// scale the renderer/ruler read; `zoomMode` is "custom" for a fixed % or a fit
// mode that recomputes from the viewport on every render/resize.
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 5;
let zoomFactor = 1;
let zoomMode = "custom";
const pagesEl = document.getElementById("pages");
const dropEl = document.getElementById("drop");
const viewportEl = document.getElementById("viewport");
const fmtButtons = {
  bold: document.getElementById("bold"),
  italic: document.getElementById("italic"),
  underline: document.getElementById("underline"),
  strike: document.getElementById("strike"),
};
const alignBtns = {
  start: document.getElementById("alignStart"),
  center: document.getElementById("alignCenter"),
  end: document.getElementById("alignEnd"),
  justify: document.getElementById("alignJustify"),
};
const superBtn = document.getElementById("superscript");
const subBtn = document.getElementById("subscript");
const fontSizeSel = document.getElementById("fontSize");
// Text-color and highlight swatch pickers (Q1): a split control — an "apply"
// half that reapplies the last-used swatch, and a caret half that opens the
// swatch menu. `textColorInput` is the hidden OS <input type=color> used only as
// the "More colors…" custom fallback, never the primary control.
const textColorCaret = document.getElementById("textColor");
const textColorApplyBtn = document.getElementById("textColorApply");
const textColorBar = document.getElementById("textColorBar");
const textColorInput = document.getElementById("textColorCustom");
const textColorMenu = document.getElementById("textColorMenu");
const highlightCaret = document.getElementById("highlight");
const highlightApplyBtn = document.getElementById("highlightApply");
const highlightBar = document.getElementById("highlightBar");
const highlightMenu = document.getElementById("highlightMenu");
// Floating selection-toolbar color/highlight pickers: mirror the ribbon swatch
// menus but anchored to buttons that sit near the selection. Declared up here so
// the shared reflect helpers can update their swatch bars during early init.
const selTextColorBtn = document.getElementById("selTextColorBtn");
const selHighlightBtn = document.getElementById("selHighlightBtn");
const selTextColorMenu = document.getElementById("selTextColorMenu");
const selHighlightMenu = document.getElementById("selHighlightMenu");
const selTextColorBar = document.getElementById("selTextColorBar");
const selHighlightBar = document.getElementById("selHighlightBar");
const clearFormattingBtn = document.getElementById("clearFormatting");
const formatPainterBtn = document.getElementById("formatPainter");
const spacingBtn = document.getElementById("spacingBtn");
const spacingMenu = document.getElementById("spacingMenu");
const spaceBeforeInput = document.getElementById("spaceBefore");
const spaceAfterInput = document.getElementById("spaceAfter");
const lineSpacingMode = document.getElementById("lineSpacingMode");
const lineSpacingValue = document.getElementById("lineSpacingValue");
const lineSpacingUnit = document.getElementById("lineSpacingUnit");
const paraOptsBtn = document.getElementById("paraOptsBtn");
const paragraphPropertiesPanel = document.getElementById("paragraphPropertiesPanel");
const paragraphPropertiesContext = document.getElementById("paragraphPropertiesContext");
const paragraphPropertiesCloseBtn = document.getElementById("paragraphPropertiesClose");
const paraPanelStyle = document.getElementById("paraPanelStyle");
const paraPanelAlign = document.getElementById("paraPanelAlign");
const paraLineSpacing = document.getElementById("paraLineSpacing");
const paraSpaceBefore = document.getElementById("paraSpaceBefore");
const paraSpaceAfter = document.getElementById("paraSpaceAfter");
const paraShade = document.getElementById("paraShade");
const paraShadeNone = document.getElementById("paraShadeNone");
const paraShadeMixed = document.getElementById("paraShadeMixed");
const paraBordersMixed = document.getElementById("paraBordersMixed");
const pgKeepNext = document.getElementById("pgKeepNext");
const pgKeepLines = document.getElementById("pgKeepLines");
const pgBreakBefore = document.getElementById("pgBreakBefore");
const indentLeftInput = document.getElementById("indentLeft");
const indentRightInput = document.getElementById("indentRight");
const indentSpecialSel = document.getElementById("indentSpecial");
const indentSpecialByInput = document.getElementById("indentSpecialBy");
const borderColorInput = document.getElementById("borderColor");
const tableBtn = document.getElementById("tableBtn");
const tableFmtMenu = document.getElementById("tableMenu");
const cellShade = document.getElementById("cellShade");
const cellShadeNone = document.getElementById("cellShadeNone");
const cellVAlign = document.getElementById("cellVAlign");
const cellBorderColor = document.getElementById("cellBorderColor");
const tableBorderColor = document.getElementById("tableBorderColor");
const tableAlign = document.getElementById("tableAlign");
const tableContext = document.getElementById("tableContext");
const tableRibbon = document.querySelector(".table-ribbon");
const tableRibbonControls = [...tableRibbon.querySelectorAll("button")];
const tablePropertiesBtn = document.getElementById("tablePropertiesBtn");
const tableStyleBtn = document.getElementById("tableStyleBtn");
const tableStyleMenu = document.getElementById("tableStyleMenu");
const tablePropertiesPanel = document.getElementById("tablePropertiesPanel");
const tablePropertiesContext = document.getElementById("tablePropertiesContext");
const tablePropertiesCloseBtn = document.getElementById("tablePropertiesClose");
const tableColumnWidthNote = document.getElementById("tableColumnWidthNote");
const mergeCellsBtn = document.getElementById("mergeCellsBtn");
const splitCellBtn = document.getElementById("splitCellBtn");
const splitCellDialog = document.getElementById("splitCellDialog");
const splitCellClose = document.getElementById("splitCellClose");
const splitCellCancel = document.getElementById("splitCellCancel");
const splitCellConfirm = document.getElementById("splitCellConfirm");
const splitCellRows = document.getElementById("splitCellRows");
const splitCellColumns = document.getElementById("splitCellColumns");
const tableHeaderRow = document.getElementById("tableHeaderRow");
const tableFixedLayout = document.getElementById("tableFixedLayout");
const tableColumnWidth = document.getElementById("tableColumnWidth");
const tableWidth = document.getElementById("tableWidth");
const tableIndent = document.getElementById("tableIndent");
const tableRowHeight = document.getElementById("tableRowHeight");
const tableRowHeightRule = document.getElementById("tableRowHeightRule");
const tableCellMargin = document.getElementById("tableCellMargin");
const tableCellSpacing = document.getElementById("tableCellSpacing");
const tableCaption = document.getElementById("tableCaption");
const tableDescription = document.getElementById("tableDescription");
const tableFormula = document.getElementById("tableFormula");
const tableFormulaApply = document.getElementById("tableFormulaApply");
const insertTableBtn = document.getElementById("insertTableBtn");
const insertLinkBtn = document.getElementById("insertLinkBtn");
const insertTableMenu = document.getElementById("insertTableMenu");
const gridPicker = document.getElementById("gridPicker");
const gridLabel = document.getElementById("gridLabel");
const ribbonTabs = [...document.querySelectorAll(".ribbon-tab")];
const ribbonPanels = [...document.querySelectorAll(".ribbon-panel")];
const tabTable = document.getElementById("tabTable");
const undoBtn = document.getElementById("undoBtn");
const redoBtn = document.getElementById("redoBtn");
const viewOutlineBtn = document.getElementById("viewOutlineBtn");
const viewZoomOut = document.getElementById("viewZoomOut");
const viewZoomIn = document.getElementById("viewZoomIn");
const findBtn = document.getElementById("findBtn");
const findPanel = document.getElementById("findPanel");
const findInput = document.getElementById("findInput");
const replaceInput = document.getElementById("replaceInput");
const findPrevBtn = document.getElementById("findPrev");
const findNextBtn = document.getElementById("findNext");
const findStatus = document.getElementById("findStatus");
const findCase = document.getElementById("findCase");
const findWholeWord = document.getElementById("findWholeWord");
const findSelection = document.getElementById("findSelection");
let findScope = null;
const replaceOneBtn = document.getElementById("replaceOne");
const replaceAllBtn = document.getElementById("replaceAll");
const findCloseBtn = document.getElementById("findClose");

/** Shows the named ribbon tab's panel and marks its tab selected. */
function selectRibbonTab(name) {
  for (const t of ribbonTabs) {
    const selected = t.dataset.tab === name;
    t.setAttribute("aria-selected", String(selected));
    t.tabIndex = selected ? 0 : -1;
  }
  for (const p of ribbonPanels) p.hidden = p.dataset.panel !== name;
  // Recompute overflow synchronously (not on a later frame) so a control is
  // already in its final inline-or-overflow location the moment the panel shows —
  // the newly shown panel reflows and the previous panel's groups are restored.
  if (typeof updateRibbonOverflow === "function") updateRibbonOverflow();
}
for (const t of ribbonTabs) {
  t.addEventListener("click", () => {
    if (t.disabled) return;
    selectRibbonTab(t.dataset.tab);
    // Clicking a tab while collapsed brings the ribbon back (Word behavior).
    if (ribbonViewCollapsed) setRibbonCollapsed(false);
  });
}

document.querySelector(".ribbon-tabs")?.addEventListener("keydown", (event) => {
  if (!event.target.matches(".ribbon-tab")) return;
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  const enabled = ribbonTabs.filter((tab) => !tab.disabled);
  const current = enabled.indexOf(event.target);
  if (current < 0 || enabled.length === 0) return;
  let next = current;
  if (event.key === "Home") next = 0;
  else if (event.key === "End") next = enabled.length - 1;
  else next = (current + (event.key === "ArrowRight" ? 1 : -1) + enabled.length) % enabled.length;
  event.preventDefault();
  enabled[next].click();
  enabled[next].focus();
});

// --- Compact ↔ ribbon view toggle (collapse/expand the band) -----------------
const ribbonViewToggle = document.getElementById("ribbonViewToggle");
let ribbonViewCollapsed = false;

/** Collapses the ribbon to just its tab strip (compact view) or expands it back
 *  to the full band, persisting the choice. */
function setRibbonCollapsed(collapsed) {
  ribbonViewCollapsed = collapsed;
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
  try {
    localStorage.setItem("opendoc.ribbonCollapsed", collapsed ? "1" : "0");
  } catch {
    /* private mode / storage disabled — the toggle still works in-session */
  }
  if (!collapsed && typeof scheduleRibbonOverflow === "function") scheduleRibbonOverflow();
}

if (ribbonViewToggle) {
  ribbonViewToggle.addEventListener("click", () => setRibbonCollapsed(!ribbonViewCollapsed));
  try {
    if (localStorage.getItem("opendoc.ribbonCollapsed") === "1") setRibbonCollapsed(true);
  } catch {
    /* ignore */
  }
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
// second code path. Replace opens the same Find & Replace panel as Find.
pasteBtn.addEventListener("click", () => { paste(); });
cutBtn.addEventListener("click", () => { cut(); });
copyBtn.addEventListener("click", () => { copySelection(); });
replaceBtn.addEventListener("click", () => { if (!findBtn.disabled) findBtn.click(); });

// Quick Styles gallery — populated from the document's real styles alongside
// the visible all-styles `#paragraphStyle` selector. Both controls apply through
// the same `setParagraphStyle` path.
const stylesGallery = document.getElementById("stylesGallery");
// The ▾ affordance and the scrollable popover it reveals. The inline strip shows
// only a few quick styles; the popover lists EVERY document style so the whole
// set is reachable from the gallery, not just via the separate `#paragraphStyle`
// dropdown. `stylesMorePopover` is the toolbar-popover manager entry, wired up
// once in the popover section further below.
const stylesMoreBtn = document.getElementById("stylesMoreBtn");
const stylesMorePanel = document.getElementById("stylesMorePanel");
let stylesMorePopover = null;

// How many quick-access cards the width-constrained inline ribbon strip shows.
// The remaining styles live in the "More styles" ▾ popover (Word's collapsed
// gallery row + its More expander), so raising this never hides any style.
const QUICK_STYLE_COUNT = 4;

// Roving-tabindex keyboard navigation for a Styles listbox: the group is a
// single Tab stop and Left/Right/Up/Down (plus Home/End) move focus between the
// option cards, matching the WAI-ARIA listbox pattern. Enter/Space already
// activate a focused card natively (they are <button>s), which applies the
// style. Attached once per container so repeated `buildStylesGallery` rebuilds
// never stack duplicate listeners. Shared by the inline strip and the popover.
function attachGalleryRoving(container) {
  if (!container) return;
  container.addEventListener("keydown", (event) => {
    const cards = [...container.querySelectorAll(".style-card")];
    if (!cards.length) return;
    const current = document.activeElement;
    const index = cards.indexOf(current);
    let next = -1;
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = index < 0 ? 0 : (index + 1) % cards.length;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = index < 0 ? cards.length - 1 : (index - 1 + cards.length) % cards.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = cards.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    for (const card of cards) card.tabIndex = card === cards[next] ? 0 : -1;
    cards[next].focus();
  });
}
attachGalleryRoving(stylesGallery);
attachGalleryRoving(stylesMorePanel);

// Esc inside the popover closes it and returns focus to the ▾ button (the global
// popover Escape handler dismisses it too, but without restoring focus).
if (stylesMorePanel) {
  stylesMorePanel.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeStylesMorePanel({ restoreFocus: true });
    }
  });
}

/** Draws a gallery card's label IN the style it represents, from the engine's
 *  resolved preview (font family/size/weight/slant/underline/color) — Word's
 *  Styles gallery. Degrades to the plain slug look if the preview is
 *  unavailable. The preview font size is clamped so a 28pt Title still fits the
 *  dense 30px card while keeping the visible hierarchy. */
function applyStyleCardPreview(label, name) {
  if (!doc || typeof doc.stylePreview !== "function") return;
  let preview;
  try {
    preview = doc.stylePreview(name);
  } catch {
    return; // unknown style — keep the plain label (graceful degrade)
  }
  if (!preview) return;
  const family = preview.fontFamily;
  if (family) label.style.fontFamily = `"${family.replace(/"/g, "")}", sans-serif`;
  const size = preview.sizePoints;
  if (size > 0) {
    // Map the point size into the card's bounded px range, preserving hierarchy.
    label.style.fontSize = `${Math.max(11, Math.min(17, Math.round(size * 0.9)))}px`;
  }
  label.style.fontWeight = preview.bold ? "700" : "450";
  label.style.fontStyle = preview.italic ? "italic" : "normal";
  label.style.textDecoration = preview.underline ? "underline" : "none";
  // Explicit RGB colors preview as authored; automatic/theme colors resolve to
  // an empty string and inherit the card's theme-aware ink (light/dark safe).
  label.style.color = preview.color || "";
  const align = preview.alignment;
  label.style.textAlign = align === "start" ? "left" : align === "end" ? "right" : align;
}

/** Builds one gallery option card drawn IN its own style. `index === 0` makes it
 *  the container's initial roving Tab stop. `fromPanel` cards additionally close
 *  the "More styles" popover after applying (Word closes the gallery on pick). */
function makeStyleCard(name, index, { fromPanel = false } = {}) {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "");
  const card = document.createElement("button");
  card.type = "button";
  card.className = `style-card style-card-${slug}`;
  card.dataset.style = name;
  card.setAttribute("role", "option");
  card.setAttribute("aria-selected", "false");
  // Roving tabindex: only the first card is a Tab stop; arrow keys move focus
  // among the rest (see attachGalleryRoving).
  card.tabIndex = index === 0 ? 0 : -1;
  card.title = name;
  const label = document.createElement("span");
  label.className = "style-card-name";
  label.textContent = name;
  applyStyleCardPreview(label, name);
  card.appendChild(label);
  card.addEventListener("click", () => {
    if (card.disabled) return;
    runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, name));
    if (fromPanel) closeStylesMorePanel({ restoreFocus: true });
  });
  return card;
}

/** Rebuilds the Styles gallery from the document's style names: a short inline
 *  quick-access strip on the ribbon plus the full-set "More styles" popover, so
 *  every paragraph style is reachable from the gallery. */
function buildStylesGallery(styles) {
  if (!stylesGallery) return;
  // Drop the previous cards but keep the static ▾ "More styles" button, which the
  // popover manager holds a reference to.
  for (const card of stylesGallery.querySelectorAll(".style-card")) card.remove();
  // Order styles with the common built-ins first; the rest follow document order.
  const preferred = ["Normal", "Body Text", "Title", "Heading 1", "Heading 2", "Subtitle"];
  const ordered = [];
  for (const preferredName of preferred) {
    const match = styles.find((name) => name.toLowerCase() === preferredName.toLowerCase());
    if (match && !ordered.includes(match)) ordered.push(match);
  }
  for (const name of styles) if (!ordered.includes(name)) ordered.push(name);

  // Inline quick strip: a handful of cards, inserted before the ▾ button.
  const shown = ordered.slice(0, QUICK_STYLE_COUNT);
  for (const [i, name] of shown.entries()) {
    const card = makeStyleCard(name, i);
    if (stylesMoreBtn) stylesGallery.insertBefore(card, stylesMoreBtn);
    else stylesGallery.appendChild(card);
  }
  if (stylesMoreBtn) stylesMoreBtn.hidden = ordered.length <= QUICK_STYLE_COUNT;

  // Full popover: a card for EVERY style, so nothing is gallery-unreachable.
  if (stylesMorePanel) {
    stylesMorePanel.replaceChildren();
    for (const [i, name] of ordered.entries()) {
      stylesMorePanel.appendChild(makeStyleCard(name, i, { fromPanel: true }));
    }
  }
}

/** Marks the card(s) matching the reflected paragraph style active and keeps
 *  each container's roving Tab stop on that style — for both the inline strip and
 *  the "More styles" popover. */
function reflectActiveCards(container) {
  if (!container) return;
  const active = paragraphStyleSel.value;
  const cards = [...container.querySelectorAll(".style-card")];
  let tabStop = cards.findIndex((card) => card.dataset.style === active);
  if (tabStop < 0) tabStop = 0; // no active style visible → first card is the Tab stop
  for (const [i, card] of cards.entries()) {
    const isActive = card.dataset.style === active;
    card.setAttribute("aria-selected", String(isActive));
    // Keep the roving Tab stop on the applied style so Tab lands where the
    // caret already is (arrow keys still reach every card).
    card.tabIndex = i === tabStop ? 0 : -1;
  }
}

/** Highlights the gallery card(s) matching the reflected paragraph style. */
function syncStylesGalleryActive() {
  reflectActiveCards(stylesGallery);
  reflectActiveCards(stylesMorePanel);
}

/** Closes the "More styles" popover, optionally restoring focus to its button. */
function closeStylesMorePanel({ restoreFocus = false } = {}) {
  if (!stylesMorePopover || !stylesMorePanel || stylesMorePanel.hidden) return;
  closePopover(stylesMorePopover);
  if (restoreFocus && stylesMoreBtn) stylesMoreBtn.focus();
}

// --- Named-style edits: update-from-selection and create-from-selection ------
// Word's two core Styles verbs, both routed through the same engine op
// (`SetStyleDefinition`): "Update <Style> to match selection" mutates the style
// definition so every paragraph using it reflows; "Create a style" adds a new
// paragraph style (based on the current one) and applies it. Both rebuild the
// gallery previews and dropdowns from the (now changed) style registry.
const styleNameDialog = document.getElementById("styleNameDialog");
const styleNameInput = document.getElementById("styleNameInput");
const styleNameConfirm = document.getElementById("styleNameConfirm");
const styleNameCancel = document.getElementById("styleNameCancel");
const styleNameClose = document.getElementById("styleNameClose");
let styleNameResolve = null;

/** Opens the create-style dialog and resolves to the entered name, or null if
 *  cancelled. A single in-flight prompt at a time. */
function promptStyleName() {
  if (!styleNameDialog) return Promise.resolve(null);
  return new Promise((resolve) => {
    styleNameResolve = resolve;
    styleNameInput.value = "";
    styleNameDialog.hidden = false;
    styleNameInput.focus();
  });
}

function closeStyleNameDialog(result) {
  if (!styleNameDialog || styleNameDialog.hidden) return;
  styleNameDialog.hidden = true;
  const resolve = styleNameResolve;
  styleNameResolve = null;
  if (resolve) resolve(result);
}

if (styleNameDialog) {
  styleNameConfirm.addEventListener("click", () => closeStyleNameDialog(styleNameInput.value.trim()));
  styleNameCancel.addEventListener("click", () => closeStyleNameDialog(null));
  styleNameClose.addEventListener("click", () => closeStyleNameDialog(null));
  styleNameDialog.addEventListener("click", (event) => {
    if (event.target === styleNameDialog) closeStyleNameDialog(null);
  });
  styleNameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      closeStyleNameDialog(styleNameInput.value.trim());
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeStyleNameDialog(null);
    }
  });
}

/** The paragraph style applied at the caret, or "" when none — the target of
 *  "Update to match selection" and the base for a new style. */
function currentParagraphStyleName() {
  if (!doc || !selection) return "";
  try {
    return doc.paragraphStyleAt(selection.focus.node) || "";
  } catch {
    return "";
  }
}

/** Redefines paragraph style `name` to match the selection (Word's "Update
 *  <Style> to Match Selection"). Every paragraph using it reflows. */
async function updateStyleFromSelection(name) {
  if (!doc || !name) return;
  await runToolbarEdit((a, b, c, d) => doc.updateStyleFromSelection(a, b, c, d, name));
  populateStyles();
  updateToolbar();
  setStatus(`Updated “${name}” to match the selection`);
}

/** Creates a new paragraph style from the selection and applies it (Word's
 *  "Create a Style"). Prompts for the name; refuses duplicates via the engine. */
async function createStyleFromSelection() {
  if (!doc || !selection) return;
  const name = await promptStyleName();
  if (!name) return;
  const exists = doc.listStyles().some((s) => s.toLowerCase() === name.toLowerCase());
  if (exists) {
    setStatus(`A style named “${name}” already exists`, "error");
    return;
  }
  await runToolbarEdit((a, b, c, d) => doc.createStyleFromSelection(a, b, c, d, name));
  populateStyles();
  updateToolbar();
  setStatus(`Created style “${name}”`);
}

// --- Ribbon overflow: collapse groups that don't fit into a "⋯" menu ---------
const ribbonBodyEl = document.querySelector(".ribbon-body");
const ribbonEl = document.querySelector(".ribbon");
const ribbonOverflowBtn = document.getElementById("ribbonOverflowBtn");
const ribbonOverflowMenu = document.getElementById("ribbonOverflowMenu");
// Canonical group order per panel, captured before any group is relocated.
const ribbonPanelGroups = new Map(
  ribbonPanels.map((p) => [p, [...p.querySelectorAll(":scope > .rgroup")]]),
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
  const active = ribbonPanels.find((p) => !p.hidden);
  if (!active) return;
  const groups = ribbonPanelGroups.get(active) || [];
  const style = getComputedStyle(active);
  const avail =
    active.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const widths = new Map(groups.map((group) => [group, group.offsetWidth]));
  const total = groups.reduce((sum, group) => sum + widths.get(group), 0);
  if (total <= avail + 0.5) return; // everything fits — no overflow control
  // Reserve room for the ⋯ button. Clipboard, Editing, and Mode are persistent
  // anchors; relocate the other groups from right to left until the inline set
  // fits. Mode can fall back to the footer at very small widths.
  const reserve = 44;
  let inlineWidth = total;
  const moved = [];
  for (let i = groups.length - 1; i >= 0 && inlineWidth > avail - reserve; i--) {
    const group = groups[i];
    if (group.hasAttribute("data-ribbon-pinned")) continue;
    moved.push(group);
    inlineWidth -= widths.get(group);
  }
  // If a future pinned composition cannot fit at an extremely small width,
  // preserve Clipboard and move the remaining pinned group as the last resort.
  if (inlineWidth > avail - reserve) {
    for (let i = groups.length - 1; i >= 0 && inlineWidth > avail - reserve; i--) {
      const group = groups[i];
      if (moved.includes(group) || group.dataset.group === "clipboard") continue;
      moved.push(group);
      inlineWidth -= widths.get(group);
    }
  }
  for (const group of groups) if (moved.includes(group)) ribbonOverflowMenu.appendChild(group);
  ribbonOverflowBtn.hidden = false;
}

let ribbonOverflowFrame = 0;
function scheduleRibbonOverflow() {
  cancelAnimationFrame(ribbonOverflowFrame);
  ribbonOverflowFrame = requestAnimationFrame(updateRibbonOverflow);
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
}

// --- Delayed tooltips for icon-only ribbon controls (docs/64 §3) -------------
// A single custom tooltip (~350ms hover/focus delay) shows the control's name +
// shortcut. Reuses the existing `title`/`aria-label` content; the native title
// is suppressed only while the control is actively hovered so it never appears
// alongside the custom one, and is restored on leave (keeping dynamic titles and
// accessibility intact).
const TIP_SELECTOR = ".fmt, .ribbon-tab, .review-mode-seg, .style-card";
const ribbonTooltip = document.createElement("div");
ribbonTooltip.className = "ribbon-tooltip";
ribbonTooltip.setAttribute("role", "tooltip");
ribbonTooltip.hidden = true;
document.body.appendChild(ribbonTooltip);
let tipTimer = 0;
let tipTarget = null;

function tipContentFor(el) {
  const raw = (el.dataset.tipTitle ?? el.getAttribute("title") ?? "").trim();
  const label = (el.getAttribute("aria-label") ?? "").trim();
  const match = raw.match(/^(.*?)\s*\(([^)]+)\)\s*$/);
  const name = (label || (match ? match[1] : raw)).trim();
  const shortcut = match ? match[2].trim() : "";
  return { name, shortcut };
}

function positionTip(el) {
  const rect = el.getBoundingClientRect();
  const tw = ribbonTooltip.offsetWidth;
  const th = ribbonTooltip.offsetHeight;
  let left = rect.left + rect.width / 2 - tw / 2;
  left = Math.max(6, Math.min(left, window.innerWidth - tw - 6));
  let top = rect.bottom + 6;
  if (top + th > window.innerHeight - 6) top = rect.top - th - 6;
  ribbonTooltip.style.left = `${Math.round(left)}px`;
  ribbonTooltip.style.top = `${Math.round(top)}px`;
}

function showTip(el) {
  const { name, shortcut } = tipContentFor(el);
  if (!name) return;
  ribbonTooltip.textContent = name;
  if (shortcut) {
    const kbd = document.createElement("kbd");
    kbd.textContent = shortcut;
    ribbonTooltip.appendChild(kbd);
  }
  ribbonTooltip.hidden = false;
  positionTip(el);
  ribbonTooltip.classList.add("is-visible");
}

function armTip(el) {
  if (el.getAttribute("title")) {
    el.dataset.tipTitle = el.getAttribute("title");
    el.removeAttribute("title");
  }
  tipTarget = el;
  clearTimeout(tipTimer);
  tipTimer = window.setTimeout(() => {
    if (tipTarget === el) showTip(el);
  }, 350);
}

function disarmTip(el) {
  if (el && el.dataset.tipTitle != null) {
    el.setAttribute("title", el.dataset.tipTitle);
    delete el.dataset.tipTitle;
  }
  if (tipTarget === el || !el) {
    clearTimeout(tipTimer);
    tipTimer = 0;
    tipTarget = null;
    ribbonTooltip.classList.remove("is-visible");
    ribbonTooltip.hidden = true;
  }
}

function bindRibbonTooltipSurface(surface) {
  if (!surface) return;
  surface.addEventListener("pointerover", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (!el || !surface.contains(el) || el === tipTarget) return;
    if (tipTarget) disarmTip(tipTarget);
    armTip(el);
  });
  surface.addEventListener("pointerout", (e) => {
    if (!tipTarget) return;
    if (e.relatedTarget && tipTarget.contains(e.relatedTarget)) return;
    disarmTip(tipTarget);
  });
  surface.addEventListener("focusin", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (!el) return;
    if (tipTarget && tipTarget !== el) disarmTip(tipTarget);
    armTip(el);
  });
  surface.addEventListener("focusout", (e) => {
    const el = e.target.closest(TIP_SELECTOR);
    if (el) disarmTip(el);
  });
  surface.addEventListener("click", () => {
    if (tipTarget) disarmTip(tipTarget);
  });
}

bindRibbonTooltipSurface(ribbonEl);
bindRibbonTooltipSurface(ribbonOverflowMenu);
window.addEventListener("scroll", () => { if (tipTarget) disarmTip(tipTarget); }, true);

undoBtn.addEventListener("click", () => runEdit(() => doc.undo()));
redoBtn.addEventListener("click", () => runEdit(() => doc.redo()));
viewOutlineBtn.addEventListener("click", () => toggleOutline());
viewZoomOut.addEventListener("click", () => stepZoom(-1));
viewZoomIn.addEventListener("click", () => stepZoom(1));
const railOutline = document.getElementById("railOutline");
const railPages = document.getElementById("railPages");
const railReview = document.getElementById("railReview");
const outlinePanel = document.getElementById("outlinePanel");
const outlineClose = document.getElementById("outlineClose");
const outlineBody = document.getElementById("outlineBody");
const pagesPanel = document.getElementById("pagesPanel");
const pagesClose = document.getElementById("pagesClose");
const pagesBody = document.getElementById("pagesBody");
const a11yDocument = document.getElementById("a11yDocument");
const reviewBtn = document.getElementById("reviewBtn");
const reviewClose = document.getElementById("reviewClose");
const reviewFilters = [...document.querySelectorAll("[data-review-filter]")];
const selComment = document.getElementById("selComment");
const reviewModeButtons = [...document.querySelectorAll("[data-review-mode]")];
const reviewPrevious = document.getElementById("reviewPrevious");
const reviewNext = document.getElementById("reviewNext");
const reviewAcceptAll = document.getElementById("reviewAcceptAll");
const reviewRejectAll = document.getElementById("reviewRejectAll");
const reviewBulkActions = document.getElementById("reviewBulkActions");
const suggestingBanner = document.getElementById("suggestingBanner");
const suggestingBannerEdit = document.getElementById("suggestingBannerEdit");
const viewingBanner = document.getElementById("viewingBanner");
const viewingBannerEdit = document.getElementById("viewingBannerEdit");
const reviewSidebar = document.getElementById("reviewSidebar");
const reviewSidebarBody = document.getElementById("reviewSidebarBody");
const reviewSidebarHeader = document.getElementById("reviewSidebarHeader");
let reviewMode = "editing";
let activeReviewCommentId = null;

// The "Show changes" markup preview (docs/93): renders struck deletions +
// author-colored insertions + highlighted comments from the engine's markup
// layout. A view toggle — it never changes the model or the caret. Reachable
// from the View menu / palette (`view.showChanges`), and bound to the review
// mode: Suggesting turns it on automatically, and a document that arrives with
// tracked changes shows markup by default (review UX v2 Q1). The manual toggle
// stays available in every mode.
let showingChanges = false;

/** Mirrors `showingChanges` onto the document element (a `.showing-changes`
 *  body class + the View menu / palette pressed state) so the redline's on/off
 *  status is visible and discoverable, not a silent internal flag. */
function reflectShowingChangesState() {
  document.body.classList.toggle("showing-changes", showingChanges);
}

/** The single entry point for turning the markup preview on or off. Re-renders
 *  through the engine's markup vs. editing layout (`renderAll` → `setShowChanges`)
 *  only when the state actually changes, so callers can request a state
 *  idempotently (e.g. entering Suggesting when it is already on). */
async function setShowingChanges(on) {
  const next = !!on;
  if (next === showingChanges) {
    reflectShowingChangesState();
    return;
  }
  showingChanges = next;
  reflectShowingChangesState();
  if (doc) await renderAll();
}

async function toggleShowChanges() {
  if (!doc) return;
  await setShowingChanges(!showingChanges);
}

/** Whether the open document currently carries any tracked revision, used to
 *  decide whether markup should be shown by default on open (Q1). */
function documentHasTrackedChanges() {
  if (!doc) return false;
  try {
    return (JSON.parse(doc.listRevisions()) ?? []).length > 0;
  } catch {
    return false;
  }
}
let activeReviewItemId = null;
let reviewPopover = null;
let reviewSidebarPreference = null;
let reviewComposerState = null;
let reviewDeleteConfirmId = null;
let reviewMarginFrame = 0;
// Sidebar virtualization (REVIEW-GAP-020). The full render (on content edit /
// resize) computes every item's stacked document-scroll position once and keeps
// it in `reviewLayout`; scrolling only re-windows that precomputed layout, never
// re-parsing the review payload or rebuilding cards. `reviewCardCache` retains
// each item's built DOM + measured height keyed by a content signature, so an
// unchanged card is never rebuilt or re-measured, and only cards inside (or near)
// the viewport are ever mounted into the DOM.
let reviewLayout = [];
// A caret→card index rebuilt each render: for every shown item, its card id and
// the model anchor range (node + byte offsets) its text occupies, so a caret
// landing in a commented or tracked-changed range can expand that exact card
// (REVIEW-GAP-019 caret-driven expansion, for comments AND revisions).
let reviewAnchorIndex = [];
const reviewCardCache = new Map();
let reviewWindowFrame = 0;
// Pixels above and below the viewport to keep mounted, so a scroll reveals an
// already-present card instead of a blank gap before the next frame mounts it.
const REVIEW_WINDOW_OVERSCAN = 800;

/** Reads the typed comment/revision review data (docs/81 REVIEW-GAP-022's
 *  `listComments`/`listRevisions`), shaped like the legacy combined
 *  `reviewSummary()` payload so `summary.comments`/`summary.revisions` call
 *  sites are unchanged. Prefer `doc.listComments()`/`doc.listRevisions()`
 *  directly when only one of the two is needed. */
function readReviewData(doc) {
  let comments = [];
  let revisions = [];
  try { comments = JSON.parse(doc.listComments()) ?? []; } catch { comments = []; }
  try { revisions = JSON.parse(doc.listRevisions()) ?? []; } catch { revisions = []; }
  return { comments, revisions };
}

// --- Per-author review color / attribution (docs/81 REVIEW-GAP-015) -----------
//
// Word and Google Docs give each distinct reviewer a stable, auto-assigned color
// so overlapping authors are distinguishable at a glance, plus a hover tooltip
// with name/date/change type (docs/68 §"Reference reading", §50). We mirror that:
// a fixed cycling palette, keyed deterministically by the author's stable
// identity, is assigned in the webapp only — presentation, never persisted into
// the model (the engine keeps just the opaque `author` string). The projection
// (`listComments`/`listRevisions`) exposes the author *name* (and comment
// initials), which is the stable key docs/68 §50 specifies hashing.

// Ten hues chosen to stay legible over the white document canvas and, as a solid
// avatar fill with white text, in both light and dark themes. Deliberately not
// the theme accent, so author colors never collide with selection/UI chrome.
const REVIEW_AUTHOR_PALETTE = [
  "#1a73e8", // blue
  "#188038", // green
  "#d93025", // red
  "#9334e6", // purple
  "#e37400", // orange
  "#0b8043", // deep green
  "#a50e0e", // dark red
  "#8430ce", // violet
  "#b06000", // amber-brown
  "#12805c", // teal
];

// The neutral fallback for an item with no author at all ("You"/"Unknown"): a
// grey that is not part of the palette, so an unattributed change never masquer-
// ades as a specific reviewer's color.
const REVIEW_AUTHOR_FALLBACK_COLOR = "#5f6368";

/** The stable per-author key: the author name, else the initials, else empty
 *  (the unattributed "You"/"Unknown" bucket). Case-folded so "Ada"/"ada" share
 *  one color. */
function reviewAuthorKey(item) {
  const name = String(item?.author ?? "").trim();
  if (name) return name.toLowerCase();
  const initials = String(item?.initials ?? "").trim();
  if (initials) return initials.toLowerCase();
  return "";
}

/** A deterministic palette color for an author key. Empty key → neutral
 *  fallback. Same key always yields the same color within and across sessions
 *  (pure function of the key), so an author's insertions, deletions, and
 *  comments all render in one color. */
function reviewAuthorColor(key) {
  if (!key) return REVIEW_AUTHOR_FALLBACK_COLOR;
  // FNV-1a-style rolling hash — stable, order-sensitive, no dependencies.
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return REVIEW_AUTHOR_PALETTE[hash % REVIEW_AUTHOR_PALETTE.length];
}

/** The human display name for an author, matching the sidebar's existing
 *  convention: name, else initials, else "You" (an unattributed local edit). */
function reviewAuthorDisplay(item) {
  return String(item?.author ?? "").trim()
    || String(item?.initials ?? "").trim()
    || "You";
}

/** The change-type label used in an attribution tooltip. */
function reviewChangeTypeLabel(kind) {
  switch (kind) {
    case "insertion": return "Insertion";
    case "deletion": return "Deletion";
    case "replacement": return "Replacement";
    case "formatting": return "Formatting change";
    case "move": return "Move";
    case "move_from": return "Move (source)";
    case "move_to": return "Move (destination)";
    default: return "Change";
  }
}

/** The `author · type · date` attribution string shown on hover of a tracked
 *  change (its inline marker and its sidebar card). Omits empty segments. */
function reviewRevisionTooltip(revision) {
  return [
    reviewAuthorDisplay(revision),
    reviewChangeTypeLabel(revision.kind),
    formatReviewDate(revision.date),
  ].filter(Boolean).join(" · ");
}

/** The `author · date` attribution string for a comment marker/card. */
function reviewCommentTooltip(comment) {
  return [
    reviewAuthorDisplay(comment),
    comment.resolved ? "Resolved" : "Comment",
    formatReviewDate(comment.date),
  ].filter(Boolean).join(" · ");
}

/** A descriptive accessible name for a review card — who, what kind, and a short
 *  text snippet — so a screen reader announces the card's content and role
 *  rather than a nameless generic article (REVIEW-GAP-023). */
function reviewCardAriaLabel(item) {
  const author = reviewAuthorDisplay(item.data) || "You";
  const snippet = String(item.data.text || "").replace(/\s+/g, " ").trim().slice(0, 80);
  const suffix = snippet ? `: ${snippet}` : "";
  if (item.type === "comment") {
    const kind = item.data.parentParaId ? "Reply" : item.data.resolved ? "Resolved comment" : "Comment";
    return `${kind} by ${author}${suffix}`;
  }
  const kind = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
  return `Suggested ${kind} by ${author}${suffix}`;
}

// Enable the three-state mode control (Editing / Suggesting / Viewing) once a
// document is loaded (its per-button pressed state is owned by setReviewMode),
// and reflect the review-sidebar workflow controls' availability: Next/Previous
// and Accept-all/Reject-all need at least one tracked change, and bulk decisions
// are hidden in read-only Viewing mode (REVIEW-GAP-018).
function updateReviewControls() {
  if (!doc) return;
  let count = 0;
  try { count = (JSON.parse(doc.listRevisions()) ?? []).length; } catch { count = 0; }
  for (const button of reviewModeButtons) button.disabled = false;
  if (reviewPrevious) reviewPrevious.disabled = count === 0;
  if (reviewNext) reviewNext.disabled = count === 0;
  const canDecide = count > 0 && reviewMode !== "viewing";
  if (reviewAcceptAll) reviewAcceptAll.disabled = !canDecide;
  if (reviewRejectAll) reviewRejectAll.disabled = !canDecide;
  if (reviewBulkActions) reviewBulkActions.hidden = reviewMode === "viewing";
}

/** The three review modes (docs/68 §"Suggesting mode"): `editing` applies
 *  edits directly, `suggesting` records them as tracked revisions, and
 *  `viewing` is fully read-only — no Operation reaches apply. Any unrecognized
 *  value falls back to `editing`. */
function setReviewMode(mode) {
  const previous = reviewMode;
  reviewMode =
    mode === "suggesting" ? "suggesting" : mode === "viewing" ? "viewing" : "editing";
  suggestingBanner.hidden = reviewMode !== "suggesting";
  if (viewingBanner) viewingBanner.hidden = reviewMode !== "viewing";
  for (const button of reviewModeButtons) {
    button.setAttribute("aria-pressed", String(button.dataset.reviewMode === reviewMode));
  }
  // Announce a genuine user mode change (not the load-time reset to Editing).
  if (reviewMode !== previous) {
    announceReview(`${reviewMode[0].toUpperCase()}${reviewMode.slice(1)} mode`);
    disarmFormatPainter(); // a mode change disarms the painter (its apply path may differ)
  }
  // Entering Suggesting auto-enables the markup view so a reviewer immediately
  // sees struck deletions + author-colored insertions (Word/Docs behavior, Q1).
  // Leaving Suggesting keeps whatever the reader last chose — the toggle stays
  // manual in Editing/Viewing. `setShowingChanges` is idempotent and re-renders
  // only on a real change, so this is a no-op when markup is already shown.
  if (reviewMode === "suggesting" && !showingChanges) {
    void setShowingChanges(true);
  }
  updateReviewControls();
  drawSelection();
  // Toolbar controls must not retain focus after changing mode: clipboard,
  // typing, and deletion events are deliberately accepted only while the
  // canvas editor owns focus.
  focusEditorSurface();
}

function reviewRangeClientRect(startNode, startOffset, endNode, endOffset) {
  let rects = doc?.selectionRects(startNode, startOffset, endNode, endOffset) ?? [];
  if (rects.length < 5 && startNode === endNode && startOffset === endOffset) {
    rects = doc?.caretRect(startNode, startOffset) ?? [];
  }
  if (rects.length < 5) return null;
  const [pageNumber, x, y, width, height] = rects;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const { rect: canvasRect, sx, sy } = scaleOf(page);
  return {
    pageNumber,
    left: canvasRect.left + x * sx,
    right: canvasRect.left + (x + width) * sx,
    top: canvasRect.top + y * sy,
    bottom: canvasRect.top + (y + height) * sy,
    pageRight: canvasRect.right,
  };
}

function reviewFormattingValue(property, value) {
  if (value == null) return "inherited";
  if (typeof value === "boolean") return value ? "on" : "off";
  if (property === "sizeHalfPoints" && Number.isFinite(Number(value))) {
    return `${Number(value) / 2} pt`;
  }
  if (typeof value === "object") {
    const scalar = Object.values(value).find((candidate) =>
      ["string", "number", "boolean"].includes(typeof candidate));
    return scalar == null ? "custom" : String(scalar);
  }
  return String(value);
}

function reviewFormattingDescription(changes) {
  const labels = {
    bold: "Bold",
    italic: "Italic",
    underline: "Underline",
    strike: "Strikethrough",
    font: "Font",
    sizeHalfPoints: "Font size",
    color: "Text color",
    highlight: "Highlight",
    verticalAlignment: "Vertical alignment",
  };
  return (changes ?? []).map((change) => {
    const property = String(change?.property || "");
    const label = labels[property] || property || "Formatting";
    return `${label}: ${reviewFormattingValue(property, change?.before)} → ${reviewFormattingValue(property, change?.after)}`;
  }).join("\n");
}

function revisionRange(revision) {
  if (revision?.anchor?.node) {
    return {
      startNode: revision.anchor.node,
      startOffset: Number(revision.anchor.start) || 0,
      endNode: revision.anchor.node,
      endOffset: Number(revision.anchor.end) || Number(revision.anchor.start) || 0,
    };
  }
  const text = String(revision?.text || "");
  if (!doc || !text) return null;
  const first = doc.firstPosition();
  const match = doc.findText(text, first.node, first.offset, true, false);
  first.free();
  if (!match.found) { match.free(); return null; }
  const range = {
    startNode: match.startNode,
    startOffset: match.startOffset,
    endNode: match.endNode,
    endOffset: match.endOffset,
  };
  match.free();
  return range;
}

function reviewCardButton(label, action, danger = false, ariaLabel = "") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `review-margin-action${danger ? " danger" : ""}`;
  button.textContent = label;
  // A descriptive accessible name where the visible verb alone ("Accept",
  // "Reply") lacks context for a screen reader (REVIEW-GAP-023).
  if (ariaLabel) {
    button.setAttribute("aria-label", ariaLabel);
    button.title = ariaLabel;
  }
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

function reviewIconButton(icon, label, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "review-margin-icon-action";
  button.setAttribute("aria-label", label);
  button.title = label;
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  button.appendChild(glyph);
  button.addEventListener("click", async (event) => {
    event.stopPropagation();
    await action();
  });
  return button;
}

/** Makes a textarea grow with its content (modern Docs/Word composer): height
 *  tracks the scroll height from a min of one row up to a cap, after which it
 *  scrolls. Returns a `resize()` the caller can invoke after programmatic value
 *  changes. */
function autoGrowTextarea(textarea, { min = 34, max = 180 } = {}) {
  const resize = () => {
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(max, Math.max(min, textarea.scrollHeight))}px`;
  };
  textarea.addEventListener("input", resize);
  requestAnimationFrame(resize);
  return resize;
}

/** Shared "modern comment" composer key handling: Enter submits, Shift+Enter
 *  inserts a newline, Escape cancels. Kept identical across the top-level comment
 *  box and every reply composer so the interaction never differs by surface (Q5).
 *  Always stops propagation so a keystroke in the composer never reaches the
 *  canvas editor or the card's expand/collapse handler. */
function attachComposerKeys(textarea, { onSubmit, onCancel }) {
  textarea.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      onSubmit?.();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onCancel?.();
    }
  });
}

function scheduleReviewMarginRender() {
  if (reviewMarginFrame) return;
  reviewMarginFrame = requestAnimationFrame(() => {
    reviewMarginFrame = 0;
    renderReviewMarginItems();
  });
}

/** Whether `comment` is a threaded reply to `parent`. A reply carries a
 *  non-null `parentParaId` that joins to the parent's `paraId` (the DOCX
 *  `w15:paraIdParent` → `w14:paraId` link) or, as a fallback, to the parent's
 *  comment id. A thread root has a null/absent `parentParaId` and is a reply to
 *  nothing. The `parentParaId != null` guard is load-bearing: `listComments`
 *  projects a comment with no join key as `paraId: null` / `parentParaId: null`
 *  (e.g. imported comments with no `commentsExtended`/`w14:paraId`), and without
 *  the guard `null === null` would count every top-level comment as a reply to
 *  every other — an O(n²) reply-DOM (and signature-string) blowup that makes a
 *  comment-heavy document consume gigabytes of memory. */
function reviewCommentIsReplyTo(comment, parent) {
  const parentKey = comment?.parentParaId;
  if (parentKey == null) return false;
  return parentKey === parent.paraId || parentKey === parent.id;
}

function renderReviewMarginItems() {
  reviewSidebarBody.replaceChildren();
  if (!doc || !pages.length) {
    reviewSidebar.hidden = true;
    viewportEl.classList.remove("has-review-sidebar");
    reviewLayout = [];
    reviewCardCache.clear();
    return;
  }
  const summary = readReviewData(doc);
  const items = [];
  const comments = summary.comments ?? [];
  updateReviewControls();
  for (const comment of comments) {
    if (!comment.anchor?.node) continue;
    // Open / Resolved / All comment filter (REVIEW-GAP-018/019). A reply always
    // follows its root's visibility so a thread is never half-shown: match on the
    // root comment's resolved state when this is a reply.
    if (reviewFilter !== "all") {
      const root = comment.parentParaId
        ? comments.find((c) => c.paraId === comment.parentParaId) ?? comment
        : comment;
      const resolved = !!root.resolved;
      if (reviewFilter === "resolved" ? !resolved : resolved) continue;
    }
    const rect = reviewRangeClientRect(comment.anchor.node, Number(comment.anchor.start) || 0, comment.anchor.node, Number(comment.anchor.end) || 0);
    if (rect) items.push({ type: "comment", data: comment, rect });
  }
  const revisionItems = [];
  const groupedRevisions = new Map();
  const groupedMoves = new Map();
  for (const revision of summary.revisions ?? []) {
    if (revision.movePair?.fromStart && revision.movePair?.toStart) {
      const moveKey = `${revision.movePair.fromStart}:${revision.movePair.toStart}`;
      const group = groupedMoves.get(moveKey) ?? [];
      group.push(revision);
      groupedMoves.set(moveKey, group);
      continue;
    }
    const groupId = String(revision.groupId || "");
    const groupKind = String(revision.groupKind || "");
    if (groupId && ["typing", "replacement", "formatting"].includes(groupKind)) {
      const group = groupedRevisions.get(groupId) ?? [];
      group.push(revision);
      groupedRevisions.set(groupId, group);
    } else {
      revisionItems.push(revision);
    }
  }
  for (const [groupId, revisions] of groupedRevisions) {
    const ranges = revisions.map(revisionRange).filter(Boolean);
    const node = ranges[0]?.startNode;
    if (!node || ranges.some((range) => range.startNode !== node || range.endNode !== node)) {
      revisionItems.push(...revisions);
      continue;
    }
    const deletions = revisions.filter((revision) => revision.kind === "deletion");
    const insertions = revisions.filter((revision) => revision.kind === "insertion");
    const formatting = revisions.filter((revision) => revision.kind === "formatting");
    const groupKind = String(revisions[0]?.groupKind || "");
    const kind = groupKind === "formatting"
      ? "formatting"
      : groupKind === "replacement" || deletions.length > 0
        ? "replacement"
        : "insertion";
    revisionItems.push({
      id: groupId,
      groupId,
      kind,
      author: revisions.find((revision) => revision.author)?.author,
      date: revisions.find((revision) => revision.date)?.date,
      text: (kind === "formatting" ? formatting : kind === "insertion" ? insertions : deletions)
        .map((revision) => String(revision.text || "")).join(""),
      oldText: deletions.map((revision) => String(revision.text || "")).join(""),
      newText: insertions.map((revision) => String(revision.text || "")).join(""),
      formattingDelta: formatting.flatMap((revision) =>
        Array.isArray(revision.formattingDelta) ? revision.formattingDelta : []),
      anchor: {
        node,
        start: Math.min(...ranges.map((range) => range.startOffset)),
        end: Math.max(...ranges.map((range) => range.endOffset)),
      },
      revisions,
    });
  }
  for (const [moveKey, revisions] of groupedMoves) {
    const source = revisions.filter((revision) => revision.kind === "move_from");
    const destination = revisions.filter((revision) => revision.kind === "move_to");
    const ranges = revisions.map(revisionRange).filter(Boolean);
    if (!source.length || !destination.length || ranges.length !== revisions.length) {
      revisionItems.push(...revisions);
      continue;
    }
    const movePair = revisions[0].movePair;
    revisionItems.push({
      id: `move:${moveKey}`,
      kind: "move",
      author: revisions.find((revision) => revision.author)?.author,
      date: revisions.find((revision) => revision.date)?.date,
      text: destination.map((revision) => String(revision.text || "")).join(""),
      oldText: source.map((revision) => String(revision.text || "")).join(""),
      newText: destination.map((revision) => String(revision.text || "")).join(""),
      anchor: source[0].anchor,
      destinationAnchor: destination[0].anchor,
      movePair,
      ranges,
      revisions,
    });
  }
  // Tracked changes are not "resolved" — the Resolved filter is comment-only, so
  // it hides revisions; Open and All show them (REVIEW-GAP-018/019).
  for (const revision of reviewFilter === "resolved" ? [] : revisionItems) {
    const ranges = revision.ranges ?? [revisionRange(revision)].filter(Boolean);
    const positioned = ranges
      .map((range) => ({
        range,
        rect: reviewRangeClientRect(
          range.startNode,
          range.startOffset,
          range.endNode,
          range.endOffset,
        ),
      }))
      .filter((item) => item.rect)
      .sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);
    if (positioned.length) {
      items.push({
        type: "revision",
        data: revision,
        range: positioned[0].range,
        rect: positioned[0].rect,
        ranges: positioned.map((item) => item.range),
      });
    }
  }
  if (reviewComposerState?.range) {
    const { start, end } = reviewComposerState.range;
    const rect = reviewRangeClientRect(start.node, start.offset, end.node, end.offset);
    if (rect) items.push({ type: "composer", data: reviewComposerState, rect });
  }
  items.sort((a, b) => a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top);

  const show = reviewSidebarPreference ?? items.length > 0;
  reviewSidebar.hidden = !show;
  // Mutually exclusive with the outline panel (see toggleOutline): whenever the
  // review sidebar is shown the outline closes, so the canvas is only ever
  // inset from one side at a time.
  if (show && outlinePanel && !outlinePanel.hidden) {
    outlinePanel.hidden = true;
    railOutline.setAttribute("aria-pressed", "false");
  }
  if (show && pagesPanel && !pagesPanel.hidden) {
    pagesPanel.hidden = true;
    railPages.setAttribute("aria-pressed", "false");
  }
  // Reserve the comment column's width in the page stack only while the column
  // is shown, so pages stay centered-ish and the single `.viewport` scrollbar
  // sits past the comments (never between the canvas and the comments).
  viewportEl.classList.toggle("has-review-sidebar", show);
  reviewBtn.setAttribute("aria-pressed", String(show));
  railReview.setAttribute("aria-pressed", String(show));
  if (!show) {
    reviewLayout = [];
    return;
  }

  // The comment layer rides inside `.viewport`'s single scroll context; its body
  // spans the page-stack height so the transparent margin remains click-to-
  // deselect and cards are never clipped. No scrollTop sync: one scroll owner.
  const viewportRect = viewportEl.getBoundingClientRect();
  reviewSidebarBody.style.height = `${Math.max(pagesEl.scrollHeight, viewportEl.clientHeight)}px`;
  // Cancel the sticky header's flow height so cards stay pixel-aligned to their
  // canvas anchors (the header floats above via `position: sticky`).
  if (reviewSidebarHeader) {
    reviewSidebarBody.style.marginTop = `-${reviewSidebarHeader.offsetHeight}px`;
  }

  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "review-sidebar-empty";
    empty.innerHTML = '<span class="ms" aria-hidden="true">chat_bubble_outline</span><br>No comments or suggestions yet.<br>Select text and choose Add comment.';
    reviewSidebarBody.appendChild(empty);
    reviewLayout = [];
    reviewCardCache.clear();
    return;
  }

  // Build (or reuse) each item's card, but do not mount them all: compute every
  // card's stacked document-scroll position once here, then mount only the
  // viewport window (`mountReviewWindow`). A collapsed card whose content
  // signature is unchanged is reused from `reviewCardCache` without rebuilding
  // or re-measuring; the active/expanded card and the composer always rebuild so
  // their live controls stay fresh (REVIEW-GAP-020).
  const built = [];
  reviewAnchorIndex = [];
  for (const item of items) {
    const itemId = item.type === "composer" ? "composer" : `${item.type}:${item.data.id}`;
    const anchor = item.data?.anchor;
    if (item.type !== "composer" && anchor?.node) {
      reviewAnchorIndex.push({
        itemId,
        type: item.type,
        dataId: item.data.id,
        node: anchor.node,
        start: Number(anchor.start) || 0,
        end: Number(anchor.end) || Number(anchor.start) || 0,
      });
    }
    const expanded = activeReviewItemId === itemId;
    const sig = reviewCardSignature(item, comments);
    let entry = reviewCardCache.get(itemId);
    if (entry && entry.sig === sig && item.type !== "composer" && !expanded) {
      built.push({ itemId, item, entry });
      continue;
    }
    if (item.type === "composer") {
      const composer = document.createElement("article");
      composer.className = "review-margin-card review-margin-composer expanded";
      composer.setAttribute("role", "group");
      composer.setAttribute("aria-label", "Add comment");
      const textarea = document.createElement("textarea");
      textarea.rows = 3;
      textarea.maxLength = 4096;
      textarea.placeholder = "Add a comment…";
      textarea.dataset.testid = "review-comment-composer";
      const actions = document.createElement("div");
      actions.className = "review-composer-actions";
      const cancel = reviewCardButton("Cancel", () => closeReviewPopover());
      cancel.dataset.testid = "review-comment-cancel";
      const submit = reviewCardButton("Comment", async () => {
        const text = textarea.value.trim();
        if (!text || !reviewComposerState?.range) return;
        const { start, end } = reviewComposerState.range;
        const metadata = currentReviewTimestamp();
        await runEdit(() => doc.addComment(start.node, start.offset, end.node, end.offset, text, undefined, undefined, metadata.date));
        reviewComposerState = null;
        reviewSidebarPreference = true;
        announceReview("Comment added");
        drawSelection();
      }, false, "Add comment");
      submit.dataset.testid = "review-comment-submit";
      // Enter submits, Shift+Enter is a newline, Esc cancels — the modern
      // Docs/Word comment interaction, identical to the reply composer (Q5).
      attachComposerKeys(textarea, {
        onSubmit: () => submit.click(),
        onCancel: () => closeReviewPopover(),
      });
      autoGrowTextarea(textarea);
      actions.append(cancel, submit);
      composer.append(textarea, actions);
      entry = { el: composer, sig, height: 0, measured: false, focusTextarea: textarea, needsFocus: true };
      reviewCardCache.set(itemId, entry);
      built.push({ itemId, item, entry });
      continue;
    }

    const card = document.createElement("article");
    const revisionKind = item.type === "revision" ? ` review-margin-${String(item.data.kind || "change").replaceAll("_", "-")}` : "";
    card.className = `review-margin-card review-margin-${item.type}${revisionKind}${item.data.resolved ? " resolved" : ""}${expanded ? " expanded" : ""}`;
    card.tabIndex = 0;
    card.dataset.reviewItemId = itemId;
    card.setAttribute("aria-expanded", String(expanded));
    // A labelled, expandable group so a screen reader announces what the card is
    // (who, what kind, a text snippet) and its expanded/collapsed state, instead
    // of a nameless generic article (REVIEW-GAP-023). `group` (not `button`)
    // because an expanded card contains real action buttons.
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", reviewCardAriaLabel(item));
    const header = document.createElement("div");
    header.className = "review-margin-card-head";
    const avatar = document.createElement("span");
    avatar.className = "review-margin-avatar";
    const authorName = item.data.author || item.data.initials || "You";
    avatar.textContent = authorName.trim().slice(0, 1).toUpperCase() || "U";
    // Per-author color (docs/81 REVIEW-GAP-015): the avatar chip is filled with
    // the author's stable palette color so the same reviewer is recognizable
    // across their comments and tracked changes, matching the inline markers.
    const authorColor = reviewAuthorColor(reviewAuthorKey(item.data));
    avatar.style.setProperty("--review-author-color", authorColor);
    // Attribution tooltip on the whole card: author · type · date for a tracked
    // change, author · state · date for a comment (reuses the native `title`
    // tooltip pattern used across the editor's chrome).
    card.title = item.type === "revision"
      ? reviewRevisionTooltip(item.data)
      : reviewCommentTooltip(item.data);
    const title = document.createElement("div");
    title.className = "review-margin-title";
    const author = document.createElement("strong");
    author.textContent = authorName;
    const meta = document.createElement("small");
    meta.textContent = item.type === "revision"
      ? formatReviewDate(item.data.date)
      : `${item.data.resolved ? "Resolved · " : ""}${formatReviewDate(item.data.date)}`;
    title.append(author, meta);
    header.append(avatar, title);
    if (expanded && item.type === "comment") {
      header.append(
        reviewIconButton(item.data.resolved ? "undo" : "check", item.data.resolved ? "Reopen comment" : "Resolve comment", async () => {
          const resolving = !item.data.resolved;
          await runEdit(() => doc.setCommentResolved(item.data.id, resolving));
          announceReview(resolving ? "Comment resolved" : "Comment reopened");
          if (resolving) {
            activeReviewItemId = null;
            activeReviewCommentId = null;
            reviewDeleteConfirmId = null;
            if (item.data.anchor?.node) {
              const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
              selection = {
                anchor: { node: item.data.anchor.node, offset: end },
                focus: { node: item.data.anchor.node, offset: end },
              };
            }
          }
          drawSelection();
        }),
        reviewIconButton("more_vert", "More options", () => {
          reviewDeleteConfirmId = reviewDeleteConfirmId === item.data.id ? null : item.data.id;
          scheduleReviewMarginRender();
        }),
      );
    }
    const body = document.createElement("p");
    body.className = "review-margin-body";
    if (item.type === "revision") {
      if (item.data.kind === "replacement") {
        body.textContent = `Replaced “${item.data.oldText}” with “${item.data.newText}”`;
      } else if (item.data.kind === "formatting") {
        const target = item.data.text || item.data.newText || item.data.oldText;
        const details = reviewFormattingDescription(item.data.formattingDelta);
        body.textContent = `Changed formatting for “${target}”${details ? `\n${details}` : ""}`;
      } else if (item.data.kind === "move") {
        body.textContent = `Moved “${item.data.newText || item.data.oldText}”`;
      } else {
        const verb = item.data.kind === "deletion"
          ? "Deleted"
          : item.data.kind === "insertion"
            ? "Added"
            : item.data.kind === "move_from"
              ? "Unpaired move source"
              : item.data.kind === "move_to"
                ? "Unpaired move destination"
                : "Changed";
        body.textContent = `${verb} “${String(item.data.text || "")}”`;
      }
    } else {
      body.textContent = String(item.data.text || "");
    }
    const actions = document.createElement("div");
    actions.className = "review-margin-card-actions";
    if (item.type === "comment") {
      if (reviewDeleteConfirmId === item.data.id) {
        const prompt = document.createElement("span");
        prompt.textContent = "Delete this thread?";
        actions.append(
          prompt,
          reviewCardButton("Delete", async () => {
            reviewDeleteConfirmId = null;
            activeReviewItemId = null;
            await runEdit(() => doc.deleteComment(item.data.id));
            drawSelection();
          }, true),
          reviewCardButton("Cancel", () => {
            reviewDeleteConfirmId = null;
            scheduleReviewMarginRender();
          }),
        );
      }
    } else if (!["move_from", "move_to"].includes(item.data.kind) || item.data.movePair) {
      const changeLabel = (reviewChangeTypeLabel(item.data.kind) || "change").toLowerCase();
      actions.append(
        reviewCardButton("Accept", async () => {
          await runEdit(() => item.data.movePair
            ? doc.decideMovePair(
              item.data.movePair.fromStart,
              item.data.movePair.toStart,
              true,
            )
            : item.data.groupId
              ? doc.decideRevisionGroup(item.data.groupId, true)
              : doc.decideRevision(item.data.id, true));
          announceReview(`Accepted ${changeLabel}`);
          drawSelection();
          focusEditorSurface();
        }, false, `Accept this ${changeLabel}`),
        reviewCardButton("Reject", async () => {
          await runEdit(() => item.data.movePair
            ? doc.decideMovePair(
              item.data.movePair.fromStart,
              item.data.movePair.toStart,
              false,
            )
            : item.data.groupId
              ? doc.decideRevisionGroup(item.data.groupId, false)
              : doc.decideRevision(item.data.id, false));
          announceReview(`Rejected ${changeLabel}`);
          drawSelection();
          focusEditorSurface();
        }, true, `Reject this ${changeLabel}`),
      );
    }
    const focus = () => {
      const expanding = activeReviewItemId !== itemId;
      activeReviewItemId = expanding ? itemId : null;
      reviewDeleteConfirmId = null;
      if (item.type === "comment") {
        if (expanding) {
          focusReviewComment(item.data, false);
        } else if (item.data.resolved && item.data.anchor?.node) {
          activeReviewCommentId = null;
          const end = Number(item.data.anchor.end) || Number(item.data.anchor.start) || 0;
          selection = {
            anchor: { node: item.data.anchor.node, offset: end },
            focus: { node: item.data.anchor.node, offset: end },
          };
          drawSelection();
        }
      } else {
        focusReviewRevision(item.data, false);
      }
      scheduleReviewMarginRender();
    };
    card.addEventListener("click", (event) => {
      if (event.target.closest("button, textarea, input, select")) return;
      focus();
    });
    card.addEventListener("keydown", (event) => {
      // Let inner controls (Accept/Reject, the move source/destination
      // navigation buttons, composers) handle their own Enter/Space instead
      // of also toggling the card's expansion — mirrors the click guard above.
      if (event.target.closest("button, textarea, input, select")) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        focus();
      }
    });
    card.append(header, body);
    if (item.type === "revision" && item.data.kind === "move") {
      const path = document.createElement("div");
      path.className = "review-move-path";
      const source = reviewMoveEndButton(
        "From",
        item.data.anchor,
        "Original location",
        "Go to the moved text's original location",
      );
      const arrow = document.createElement("span");
      arrow.className = "ms";
      arrow.setAttribute("aria-hidden", "true");
      arrow.textContent = "arrow_forward";
      const destination = reviewMoveEndButton(
        "To",
        item.data.destinationAnchor,
        "New location",
        "Go to the moved text's new location",
      );
      path.append(source, arrow, destination);
      card.appendChild(path);
    }
    // REVIEW-GAP-012: thread comments that overlap this tracked change beneath
    // it, read-only, on an expanded revision card. `revisionThread` reports the
    // overlap; authoring a reply to a change is `addComment` over the change's
    // own range (unchanged DOCX comment ownership). Only rendered when the
    // overlap is non-empty, so a change with no related comment adds no chrome.
    if (item.type === "revision" && expanded && item.data.id) {
      let related = [];
      try {
        related = JSON.parse(doc.revisionThread(item.data.id) || "[]");
      } catch {
        related = [];
      }
      if (related.length) {
        const relatedList = document.createElement("div");
        relatedList.className = "review-margin-revision-comments";
        const relatedLabel = document.createElement("small");
        relatedLabel.textContent = "Related comments";
        relatedList.appendChild(relatedLabel);
        for (const related_comment of related) {
          const relatedItem = document.createElement("div");
          relatedItem.className = "review-margin-revision-comment";
          const relatedAuthor = document.createElement("strong");
          relatedAuthor.textContent =
            related_comment.author || related_comment.initials || "You";
          const relatedText = document.createElement("p");
          relatedText.textContent = String(related_comment.text || "");
          relatedItem.append(relatedAuthor, relatedText);
          relatedList.appendChild(relatedItem);
        }
        card.appendChild(relatedList);
      }
    }
    if (item.type === "comment") {
      const replies = comments.filter((comment) => reviewCommentIsReplyTo(comment, item.data));
      if (replies.length) {
        const thread = document.createElement("div");
        thread.className = "review-margin-replies";
        for (const reply of replies) {
          const replyItem = document.createElement("div");
          replyItem.className = `review-margin-reply${reply.resolved ? " resolved" : ""}`;
          const replyHead = document.createElement("div");
          replyHead.className = "review-margin-reply-head";
          const replyAuthor = document.createElement("strong");
          replyAuthor.textContent = reply.author || reply.initials || "You";
          const replyDate = document.createElement("small");
          replyDate.textContent = `${reply.resolved ? "Resolved · " : ""}${formatReviewDate(reply.date)}`;
          replyHead.append(replyAuthor, replyDate);
          const replyBody = document.createElement("p");
          replyBody.textContent = String(reply.text || "");
          replyItem.append(replyHead, replyBody);
          // REVIEW-GAP-011: edit or delete this specific reply (not just the
          // whole thread). Only on an expanded, unresolved parent card, so the
          // controls stay out of the collapsed/resolved presentation.
          if (expanded && !item.data.resolved) {
            const replyActionRow = document.createElement("div");
            replyActionRow.className = "review-margin-reply-actions";
            const editReply = reviewCardButton("Edit", () => {
              const input = document.createElement("input");
              input.type = "text";
              input.className = "review-margin-reply-edit";
              input.setAttribute("aria-label", "Edit reply text");
              input.maxLength = 4096;
              input.value = String(reply.text || "");
              const save = reviewCardButton("Save", async () => {
                const text = input.value.trim();
                if (!text) return;
                await runEdit(() => doc.updateComment(reply.id, text));
                announceReview("Reply updated");
                drawSelection();
              }, false, "Save reply");
              const cancelEdit = reviewCardButton("Cancel", () => {
                scheduleReviewMarginRender();
              });
              const editActions = document.createElement("div");
              editActions.className = "review-composer-actions";
              editActions.append(cancelEdit, save);
              input.addEventListener("click", (event) => event.stopPropagation());
              input.addEventListener("keydown", (event) => {
                event.stopPropagation();
                if (event.key === "Enter") {
                  event.preventDefault();
                  save.click();
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  cancelEdit.click();
                }
              });
              replyBody.replaceWith(input);
              replyActionRow.replaceWith(editActions);
              input.focus({ preventScroll: true });
            });
            const deleteReply = reviewCardButton(
              "Delete",
              async () => {
                await runEdit(() => doc.deleteReply(reply.id));
                announceReview("Reply deleted");
                drawSelection();
              },
              true,
              "Delete reply",
            );
            replyActionRow.append(editReply, deleteReply);
            replyItem.appendChild(replyActionRow);
          }
          thread.appendChild(replyItem);
        }
        card.appendChild(thread);
      }
      if (expanded && !item.data.resolved) {
        // Always-ready multi-line reply composer (Q5): a textarea the user can
        // type into immediately — no click-to-arm step — matching modern Docs/
        // Word comment threads. It auto-grows; Enter submits, Shift+Enter is a
        // newline, Esc clears. The action row surfaces only once the composer
        // is focused or has text, so a collapsed thread stays dense.
        const replyComposer = document.createElement("div");
        replyComposer.className = "review-reply-composer";
        const textarea = document.createElement("textarea");
        textarea.rows = 1;
        textarea.maxLength = 4096;
        textarea.placeholder = "Reply…";
        textarea.dataset.testid = "review-reply-composer";
        textarea.setAttribute("aria-label", "Reply to this comment");
        const replyActions = document.createElement("div");
        replyActions.className = "review-composer-actions";
        const resize = autoGrowTextarea(textarea);
        const clear = () => {
          textarea.value = "";
          resize();
          syncActions();
        };
        const submit = reviewCardButton("Reply", async () => {
          const text = textarea.value.trim();
          if (!text) return;
          const metadata = currentReviewTimestamp();
          await runEdit(() => doc.replyToComment(item.data.id, text, undefined, undefined, metadata.date));
          announceReview("Reply added");
          drawSelection();
        }, false, "Send reply");
        const cancel = reviewCardButton("Cancel", () => {
          clear();
          textarea.blur();
        });
        replyActions.append(cancel, submit);
        const syncActions = () => {
          const active = document.activeElement === textarea || textarea.value.trim().length > 0;
          replyActions.hidden = !active;
        };
        syncActions();
        textarea.addEventListener("pointerdown", (event) => event.stopPropagation());
        textarea.addEventListener("input", syncActions);
        textarea.addEventListener("focus", syncActions);
        textarea.addEventListener("blur", () => requestAnimationFrame(syncActions));
        attachComposerKeys(textarea, {
          onSubmit: () => submit.click(),
          onCancel: () => cancel.click(),
        });
        replyComposer.append(textarea, replyActions);
        card.appendChild(replyComposer);
      }
    }
    if (actions.childElementCount) card.appendChild(actions);
    entry = { el: card, sig, height: 0, measured: false };
    reviewCardCache.set(itemId, entry);
    built.push({ itemId, item, entry });
  }

  // Measure only the freshly built cards, batched: mount them all off-view, read
  // every height (one layout for all reads, since no DOM mutation interleaves),
  // then detach them. Reused cards keep their cached measured height, so a large
  // review set is not re-measured on every edit.
  const toMeasure = built.filter(({ entry }) => !entry.measured);
  for (const { entry } of toMeasure) {
    entry.el.style.top = "-99999px";
    reviewSidebarBody.appendChild(entry.el);
  }
  for (const { entry } of toMeasure) {
    entry.height = entry.el.offsetHeight;
    entry.measured = true;
  }
  for (const { entry } of toMeasure) reviewSidebarBody.removeChild(entry.el);

  // Position pass. Each card's natural anchor is its item's on-canvas marker Y in
  // document-scroll coordinates; cards are then destacked so they never overlap.
  const GAP = 8;
  const seen = new Set();
  const layout = [];
  const anchorY = built.map(
    ({ item }) => item.rect.top - viewportRect.top + viewportEl.scrollTop,
  );
  const activeIdx = activeReviewItemId
    ? built.findIndex((b) => b.itemId === activeReviewItemId)
    : -1;

  if (activeIdx >= 0) {
    // Stacked-chip surfacing (REVIEW-GAP-019): when several review items cluster
    // on the same/near anchor Y, the selected card claims true anchor alignment —
    // it stays locked to ITS OWN marker — and the rest stack around it (the
    // Google Docs pattern). A plain top-down pass could only ever push the
    // selected card DOWN past its marker (a later item in a dense cluster ended
    // up far below the change it points at, and manual scrolling never closed the
    // gap because both move together). So we anchor the layout on the selected
    // card and destack outward: the cards after it flow downward, the cards
    // before it flow upward — each still pinned to its own anchor except where a
    // neighbour would overlap. Document order (and thus mount order) is preserved.
    const tops = new Array(built.length);
    tops[activeIdx] = Math.max(GAP, anchorY[activeIdx]);
    for (let i = activeIdx + 1; i < built.length; i++) {
      tops[i] = Math.max(anchorY[i], tops[i - 1] + built[i - 1].entry.height + GAP);
    }
    for (let i = activeIdx - 1; i >= 0; i--) {
      tops[i] = Math.min(anchorY[i], tops[i + 1] - built[i].entry.height - GAP);
    }
    // Only if the cards above are collectively too tall to fit above the selected
    // card's anchor (a cluster crowding the document top) does the whole stack
    // shift down — the selected card yields its exact anchor solely when the
    // geometry leaves no alternative.
    const overflow = GAP - tops[0];
    if (overflow > 0) for (let i = 0; i < tops.length; i++) tops[i] += overflow;
    for (let i = 0; i < built.length; i++) {
      seen.add(built[i].itemId);
      layout.push({ itemId: built[i].itemId, top: tops[i], entry: built[i].entry });
    }
  } else {
    // No selection: stack every card top-down (anchor top, pushed down to clear
    // the card above), exactly as the non-virtualized layout did.
    let nextY = GAP;
    for (let i = 0; i < built.length; i++) {
      const { itemId, entry } = built[i];
      seen.add(itemId);
      const y = Math.max(GAP, anchorY[i], nextY);
      nextY = y + entry.height + GAP;
      layout.push({ itemId, top: y, entry });
    }
  }
  // Drop cache entries (and their retained DOM) for items no longer present.
  for (const key of [...reviewCardCache.keys()]) {
    if (!seen.has(key)) reviewCardCache.delete(key);
  }
  reviewLayout = layout;
  mountReviewWindow();
}

/** A stable string that changes whenever anything affecting a card's rendered
 *  DOM or measured height changes, so `reviewCardCache` reuses a card only when
 *  it would render identically. The composer is never cached (always rebuilt so
 *  its live textarea/focus stays correct). */
function reviewCardSignature(item, comments) {
  if (item.type === "composer") return "composer";
  const d = item.data;
  const itemId = `${item.type}:${d.id}`;
  const expanded = activeReviewItemId === itemId;
  const confirm = reviewDeleteConfirmId === d.id;
  const replies = item.type === "comment"
    ? comments
      .filter((c) => reviewCommentIsReplyTo(c, d))
      .map((r) => `${r.id}${r.resolved ? 1 : 0}${r.text}${r.author}${r.date}`)
      .join("")
    : "";
  return JSON.stringify([
    item.type, d.id, d.kind || "", expanded ? 1 : 0, d.resolved ? 1 : 0, confirm ? 1 : 0,
    d.text || "", d.oldText || "", d.newText || "", d.author || "", d.initials || "", d.date || "",
    d.groupId || "", d.movePair ? 1 : 0,
    Array.isArray(d.formattingDelta) ? d.formattingDelta : 0,
    d.anchor ? `${d.anchor.node}:${d.anchor.start}:${d.anchor.end}` : "",
    replies,
  ]);
}

/** Mounts only the cards whose precomputed position falls inside (or within
 *  `REVIEW_WINDOW_OVERSCAN` of) the viewport, and detaches the rest. Runs after
 *  a full render and on every scroll frame; it never re-parses the review
 *  payload, recomputes geometry, or rebuilds a card — it only attaches/detaches
 *  retained DOM by comparing cached positions to the current scroll band. This
 *  is what keeps the mounted card count bounded regardless of review size
 *  (REVIEW-GAP-020). */
function mountReviewWindow() {
  if (reviewSidebar.hidden) return;
  const scrollTop = viewportEl.scrollTop;
  const bandTop = scrollTop - REVIEW_WINDOW_OVERSCAN;
  const bandBottom = scrollTop + viewportEl.clientHeight + REVIEW_WINDOW_OVERSCAN;
  for (const { itemId, top, entry } of reviewLayout) {
    // The composer and the active/expanded card are always kept mounted: they
    // own live focus/controls the user is interacting with.
    const force = itemId === "composer" || itemId === activeReviewItemId;
    const visible = force || (top + entry.height >= bandTop && top <= bandBottom);
    const mounted = entry.el.parentNode === reviewSidebarBody;
    if (visible) {
      entry.el.style.top = `${Math.round(top)}px`;
      if (!mounted) reviewSidebarBody.appendChild(entry.el);
      if (entry.needsFocus && entry.focusTextarea) {
        entry.needsFocus = false;
        const textarea = entry.focusTextarea;
        requestAnimationFrame(() => textarea.focus({ preventScroll: true }));
      }
    } else if (mounted) {
      reviewSidebarBody.removeChild(entry.el);
    }
  }
}

/** rAF-debounced `mountReviewWindow` for the scroll path: re-windowing is cheap
 *  (no parse/geometry/rebuild), but coalescing to one call per frame keeps a
 *  fast scroll from doing redundant DOM work. */
function scheduleReviewWindow() {
  if (reviewWindowFrame) return;
  reviewWindowFrame = requestAnimationFrame(() => {
    reviewWindowFrame = 0;
    mountReviewWindow();
  });
}

// One scroll owner: the comment layer lives inside `.viewport` and rides its
// scroll context natively, so cards stay pinned to their anchored text without
// any scroll-sync or per-frame re-render (that eliminates the momentum drift and
// the between-canvas scrollbar). Cards are positioned in document-scroll
// coordinates; only content edits and resizes recompute those positions. On a
// plain scroll we only re-window the already-computed layout (mount the cards
// entering the viewport, detach those leaving it) — no re-parse, no geometry,
// no rebuild — so a document with hundreds of comments scrolls with a bounded,
// viewport-sized number of mounted cards (REVIEW-GAP-020).
viewportEl.addEventListener("scroll", scheduleReviewWindow, { passive: true });
reviewSidebarBody.addEventListener("click", (event) => {
  if (event.target !== reviewSidebarBody || !activeReviewItemId) return;
  activeReviewItemId = null;
  activeReviewCommentId = null;
  reviewDeleteConfirmId = null;
  drawSelection();
});
window.addEventListener("resize", scheduleReviewMarginRender);
const linkChip = document.getElementById("linkChip");
const linkChipKind = document.getElementById("linkChipKind");
const linkChipTarget = document.getElementById("linkChipTarget");
const linkChipAction = document.getElementById("linkChipAction");
const linkChipEdit = document.getElementById("linkChipEdit");
const linkChipRemove = document.getElementById("linkChipRemove");
const indentDecBtn = document.getElementById("indentDec");
const indentIncBtn = document.getElementById("indentInc");
const bulletListBtn = document.getElementById("bulletList");
const numberedListBtn = document.getElementById("numberedList");
const bulletListMenuBtn = document.getElementById("bulletListMenuBtn");
const numberedListMenuBtn = document.getElementById("numberedListMenuBtn");
const bulletGalleryMenu = document.getElementById("bulletGalleryMenu");
const numberGalleryMenu = document.getElementById("numberGalleryMenu");
const checkListBtn = document.getElementById("checkList");
const restartListBtn = document.getElementById("restartList");
const continueListBtn = document.getElementById("continueList");
const fontFamilyBtn = document.getElementById("fontFamily");
const fontFamilyLabel = document.getElementById("fontFamilyLabel");
const fontMenu = document.getElementById("fontMenu");
const fontMenuInput = document.getElementById("fontMenuInput");
const fontMenuList = document.getElementById("fontMenuList");
const fontMenuEmpty = document.getElementById("fontMenuEmpty");
const growFontBtn = document.getElementById("growFont");
const shrinkFontBtn = document.getElementById("shrinkFont");
const changeCaseBtn = document.getElementById("changeCaseBtn");
const changeCaseMenu = document.getElementById("changeCaseMenu");
const paragraphStyleSel = document.getElementById("paragraphStyle");
const runControls = [
  superBtn,
  subBtn,
  fontSizeSel,
  textColorCaret,
  textColorApplyBtn,
  highlightCaret,
  highlightApplyBtn,
  fontFamilyBtn,
  growFontBtn,
  shrinkFontBtn,
  changeCaseBtn,
  clearFormattingBtn,
  formatPainterBtn,
];
const paraControls = [
  ...Object.values(alignBtns),
  spacingBtn,
  paraOptsBtn,
  indentDecBtn,
  indentIncBtn,
  bulletListBtn,
  numberedListBtn,
  restartListBtn,
  continueListBtn,
  paragraphStyleSel,
];
const saveBtn = document.getElementById("save");
const saveFormatEl = document.getElementById("saveFormat");
const compatibilityStatusEl = document.getElementById("compatibilityStatus");
const zoomInBtn = document.getElementById("zoomIn");
const zoomOutBtn = document.getElementById("zoomOut");
const documentChrome = document.getElementById("documentChrome");
const docTitleEl = document.getElementById("docTitle");
const documentStateEl = document.getElementById("documentState");
const documentStateText = document.getElementById("documentStateText");
const statsEl = document.getElementById("stats");
const statWords = document.getElementById("statWords");
const statChars = document.getElementById("statChars");
const statParas = document.getElementById("statParas");
const statPages = document.getElementById("statPages");

// The engine `render_page(i, dpi)` rasterizes at `dpi` device px per inch
// (device_px = twip / 1440 * dpi). We render at 96·zoom·backingDpr() for a crisp
// result on HiDPI screens (the DPR factor is capped — see MAX_BACKING_DPR), then
// present at the logical page size (the wrap's CSS box) via the canvas element.
const BASE_DPI = 96;

/** Cap on the devicePixelRatio factor used for the raster *backing store*.
 * Text stays crisp at 1.5× while the RGBA pixel buffers shrink ~×0.56 relative
 * to a Retina dpr of 2 (memory reduction: pixel count scales with dpr²). The
 * CSS/logical page size is always the true logical size — independent of dpr —
 * so scroll height and hit-test geometry never depend on the display density. */
const MAX_BACKING_DPR = 1.5;

/** The clamped devicePixelRatio used only for the raster backing store. */
function backingDpr() {
  return Math.min(window.devicePixelRatio || 1, MAX_BACKING_DPR);
}

/** The currently open document handle (or null). Kept so a zoom change re-renders. */
let doc = null;
/** Monotonic token so a slow render from a previous file/zoom is discarded. */
let renderToken = 0;
/** Per-page DOM records: { pageNumber (1-based), wrap, overlay, canvas, wTwip,
 * hTwip, visible }. `wrap` (the sheet box) and `overlay` (caret/selection layer)
 * always exist and are sized from the page geometry; `canvas` is the live raster
 * that is mounted only for pages in/near the viewport (virtualized — see
 * `observePages`) and is `null` for off-screen pages. `visible` mirrors the
 * IntersectionObserver so repaints know whether a live canvas exists. */
let pages = [];
/** Current selection as model anchors, or null. `focus` trails the pointer. */
let selection = null; // { anchor: {node, offset}, focus: {node, offset} }
/** Current object selection (docs/85 §3.2 `Selection::Object`): a drawing/image/
 * text box selected as a unit, distinct from the text caret/range in `selection`.
 * `mode` is the interaction-grammar state (§4): "selected" shows the outline +
 * handles; "editing" is inside a container object's content. `null` = no object
 * selected. `selection` still holds a caret at the object's surrounding-text
 * anchor so a two-step Escape can collapse back to it. */
let objectSelection = null; // { node, kind, mode: "selected" | "editing" } | null
/** Active object resize drag (docs/85 §5.3): a handle drag that previews as host
 * chrome and commits ONE `SetExtent` op on release. `null` when not resizing. */
let objectResizeDrag = null;
/** Active image-crop session — the Word/Docs-style direct-manipulation crop: the
 * selected image shows crop handles + a dimmed overlay of the region being cut,
 * dragged live and committed as ONE `SetImageCrop` op on Enter / click-away.
 * `{ node, box:[x,y,w,h twips], crop:{l,t,r,b fractions}, handleDrag }` or null. */
let objectCropSession = null;
/** Active floating-object move drag (docs/85 §5.3): a body drag that previews an
 * outline and commits ONE `SetAnchor` (position) on release. `null` when idle. */
let objectMoveDrag = null;
/** Current table-cell selection overlay, separate from text ranges. */
let tableSelection = null; // { node, mode: "row" | "column" | "table" }
let tableResizeDrag = null; // { node, col, page, startClientX, startWidthTwips, preview }
let dragging = false;
/** Primary-pointer gesture retained until pointerup so link activation is
 * suppressed after a drag/Shift extension. */
let pointerGesture = null;
let selectionAutoScrollFrame = 0;
let chromeRefreshFrame = 0;
let chromeRefreshStats = false;
let chromeRefreshOutline = false;
let chromeRefreshA11y = false;
/** The model-derived link currently represented by the host-owned link chip. */
let activeLink = null;
/** One-frame throttle for pointer feedback over canvas-painted link geometry. */
let linkHoverFrame = 0;
let pendingLinkHover = null;
/** Armed run formatting for typing at a collapsed caret (e.g. click Bold with no
 *  selection → next typed characters are bold). `null` when nothing is armed; else
 *  a subset of { bold, italic, underline, strike } → boolean. Cleared whenever the
 *  caret moves for any reason other than the typing that consumes it. */
let pendingFormat = null;
/** The open document's filename, for the Save download. */
let currentName = "document.docx";
// The registered format the current document was opened as; the default target
// for Save (so an opened .odt saves back as .odt), and the basis for offering the
// other registered exporters.
let currentSourceFormat = "org.openxmlformats.wordprocessingml.document";
/** Honest local-file lifecycle shown beside the title. OpenDoc does not claim
 * cloud persistence: a mutation is Edited until the user downloads a copy. */
let documentState = "opened";
/** True while an IME composition is active on the canvas editor surface. */
let composingText = false;
/** Host gesture identity for history coalescing. The engine also validates exact
 * caret continuity, so this id is permission to merge, never the sole criterion. */
let typingSession = 0;
let typingSessionActive = false;
let lastTypingAt = 0;
const TYPING_PAUSE_MS = 1000;
const EDITOR_KEYBOARD_PLATFORM = keyboardPlatform(navigator);

function focusEditorSurface() {
  pagesEl.focus({ preventScroll: true });
}

function resetPointerGesture() {
  pointerGesture = null;
  dragging = false;
  if (selectionAutoScrollFrame) cancelAnimationFrame(selectionAutoScrollFrame);
  selectionAutoScrollFrame = 0;
}

function isInteractiveChromeTarget(target) {
  if (!(target instanceof Element)) return false;
  return !!target.closest(
    "input, select, textarea, button, [contenteditable='true'], .context-menu, .settings-panel, .cmd-overlay, .find-panel, .link-chip",
  );
}

function eventTargetsEditor(event) {
  return event.target === pagesEl || pagesEl.contains(event.target) || document.activeElement === pagesEl;
}

function clientPointEvent(clientX, clientY) {
  return { clientX, clientY };
}

let statusClearTimer = 0;

function setStatus(text, kind = "", { timeout = 0 } = {}) {
  clearTimeout(statusClearTimer);
  statusClearTimer = 0;
  statusEl.textContent = text;
  statusEl.className = `status ${kind}`;
  if (text && timeout > 0) {
    statusClearTimer = window.setTimeout(() => {
      statusEl.textContent = "";
      statusEl.className = "status";
      statusClearTimer = 0;
    }, timeout);
  }
}

function setDocumentState(state) {
  const states = {
    opened: { icon: "check_circle", text: "Opened" },
    edited: { icon: "edit", text: "Edited" },
    downloaded: { icon: "download_done", text: "Downloaded" },
  };
  const next = states[state] ?? states.opened;
  documentState = state in states ? state : "opened";
  documentStateEl.dataset.state = documentState;
  documentStateEl.querySelector(".ms").textContent = next.icon;
  documentStateText.textContent = next.text;
  documentStateEl.title = next.text;
}

function clearObjectStatus() {
  if (/^Image selected/.test(statusEl.textContent)) setStatus("");
}

// Concise polite announcements for review events (comment added, change
// accepted/rejected, bulk decisions, filter/mode changes) to the review live
// region (REVIEW-GAP-023). Re-announcing the same string still fires by briefly
// clearing the node first, so repeated identical actions (e.g. two accepts) are
// each spoken. Never moves focus.
function announceReview(text) {
  if (!reviewLiveRegion || !text) return;
  reviewLiveRegion.textContent = "";
  // A microtask gap makes assistive tech treat the new text as a fresh change.
  requestAnimationFrame(() => {
    reviewLiveRegion.textContent = text;
  });
}

function scheduleChromeRefresh({ stats = false, outline = false, a11y = false } = {}) {
  chromeRefreshStats ||= stats;
  chromeRefreshOutline ||= outline;
  // The off-screen accessibility tree mirrors document structure, so it is
  // rebuilt on the same content-changed triggers as the outline.
  chromeRefreshA11y ||= a11y || outline;
  if (chromeRefreshFrame) return;
  chromeRefreshFrame = requestAnimationFrame(() => {
    chromeRefreshFrame = 0;
    const refreshStats = chromeRefreshStats;
    const refreshOutline = chromeRefreshOutline;
    const refreshA11y = chromeRefreshA11y;
    chromeRefreshStats = false;
    chromeRefreshOutline = false;
    chromeRefreshA11y = false;
    if (refreshStats) updateStats();
    if (refreshOutline) buildOutline();
    if (refreshA11y) buildAccessibilityTree();
  });
}

/** Refreshes the footer word / paragraph / page counts from the engine. */
function updateStats() {
  if (!doc) {
    statsEl.hidden = true;
    return;
  }
  const s = doc.documentStats();
  const words = s.words;
  const chars = s.charactersWithSpaces;
  const charsNoSpaces = s.characters;
  const paras = s.paragraphs;
  s.free();
  statWords.textContent = `${words.toLocaleString()} word${words === 1 ? "" : "s"}`;
  statChars.textContent = `${chars.toLocaleString()} character${chars === 1 ? "" : "s"}`;
  // Word distinguishes with- vs without-spaces; surface both on hover.
  statChars.title = `${chars.toLocaleString()} characters (with spaces)\n${charsNoSpaces.toLocaleString()} characters (no spaces)`;
  statParas.textContent = `${paras.toLocaleString()} paragraph${paras === 1 ? "" : "s"}`;
  // Narrow windows shed the lower-priority counts from the bar (see the
  // status-bar disclosure ladder in style.css). Word keeps the full set one
  // gesture away in its Word Count dialog; until we have that dialog, the
  // whole region carries every figure so nothing shed becomes unobtainable.
  statsEl.title = `${words.toLocaleString()} words\n${chars.toLocaleString()} characters (with spaces)\n${charsNoSpaces.toLocaleString()} characters (no spaces)\n${paras.toLocaleString()} paragraphs`;
  statsEl.hidden = false;
  statPages.hidden = false;
  updatePageNumber();
}

/** Cheap current-page update (caret's page / total), for caret moves. */
function updatePageNumber() {
  if (!doc || !pages.length) return;
  let cur = 1;
  if (selection) {
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (flat.length) cur = flat[0];
  }
  statPages.textContent = `Page ${cur} of ${pages.length}`;
  reflectPagesSelection(cur);
}

async function boot() {
  try {
    await init();
    setStatus("Ready — open a .docx, .odt, .json, or .txt");
    fileEl.disabled = false;
  } catch (err) {
    console.error(err);
    setStatus("Failed to load the WASM engine", "error");
    return;
  }

  const params = new URLSearchParams(window.location.search);
  // The public demo and plain editor both open sample.docx. The smaller rich
  // corpus remains available only through the explicit e2e fixture route,
  // whose specs assert on its exact content. ?blank=1 opts out for the bare
  // local-upload state.
  const demo = params.get("demo");
  if (params.get("fixture") === "rich") {
    await loadStartupDocument("./demo.docx", "opendoc-demo.docx");
  } else if (params.get("fixture") === "float") {
    // A document with a top-level floating image, for the object move/wrap e2e
    // (no shipped sample doc contains a float). Generated by the
    // `generate_float_fixture_docx` engine test.
    await loadStartupDocument("./float.docx", "float.docx");
  } else if (demo && DEMO_PRESETS[demo]) {
    await applyDemoPreset(DEMO_PRESETS[demo]);
  } else if (params.get("blank") !== "1") {
    await loadStartupDocument("./sample.docx", "sample.docx");
  }
}

// Curated `?demo=<kind>` presets for the editor. `?demo=1` — used by the Home
// hero live embed — is the plain sample with no panel; the named kinds boot the
// REAL editor on a shipped sample document and open the surface most relevant to
// the capability, so a deep link lands directly in a meaningful state.
const DEMO_PRESETS = {
  "1": { src: "./sample.docx", name: "sample.docx" },
  tables: { src: "./sample.docx", name: "sample.docx", tab: "insert" },
  changes: { src: "./demo.docx", name: "opendoc-demo.docx", review: "suggesting" },
  comments: { src: "./demo.docx", name: "opendoc-demo.docx", sidebar: true },
  find: { src: "./sample.docx", name: "sample.docx", find: true },
  formatting: { src: "./sample.docx", name: "sample.docx", tab: "home", selectAll: true },
  export: { src: "./sample.docx", name: "sample.docx", tab: "view" },
};

// Loads a preset's document, then applies its optional UI action. Every action
// reuses an existing editor function, so a preset can only reach real, working
// surfaces — never fabricate one. Failures are non-fatal: the plain editor is
// already usable, so a preset action that can't run just leaves it as-is.
async function applyDemoPreset(preset) {
  // Apply the preset's UI state through the opened-document hook, so it lands the
  // moment the document opens — before the (network) font fetch — instead of
  // leaving the demo on the plain editor until fonts arrive. Every action reuses
  // an existing editor function, so a preset can only reach real, working
  // surfaces. Failures are non-fatal: the plain editor is already usable.
  await loadStartupDocument(
    preset.src,
    preset.name,
    () => {
      try {
        if (preset.tab) selectRibbonTab(preset.tab);
        if (preset.review) setReviewMode(preset.review);
        if (preset.sidebar) {
          reviewSidebarPreference = true;
          scheduleReviewMarginRender();
        }
        if (preset.find) openFind();
      } catch (err) {
        console.error("demo preset action failed", err);
      }
    },
    () => {
      // Post-render actions that need laid-out geometry.
      try {
        if (preset.selectAll) selectAll();
      } catch (err) {
        console.error("demo preset render action failed", err);
      }
    },
  );
}

async function loadStartupDocument(url, name, onOpened, onRendered) {
  try {
    setStatus("Loading the sample document…");
    const response = await fetch(url);
    if (!response.ok) throw new Error(`sample request returned ${response.status}`);
    await openBytes(new Uint8Array(await response.arrayBuffer()), name, onOpened, onRendered);
  } catch (err) {
    console.error(err);
    setStatus("The sample could not be loaded — you can still open a local DOCX", "error");
  }
}

async function openBytes(bytes, name, onOpened, onRendered) {
  try {
    setStatus(`Opening ${name}…`);
    hideLinkChip();
    clearLinkHover();
    // A previous document's memory is freed when it is dropped; replace it.
    if (doc) doc.free();
    doc = open(bytes);
    currentSourceFormat = doc.sourceFormat;
    applyActiveAuthorToDocument();
    selection = null;
    tableSelection = null;
    objectCropSession = null; // a new document invalidates any in-progress crop
    reviewMode = "editing";
    suggestingBanner.hidden = true;
    if (viewingBanner) viewingBanner.hidden = true;
    reviewSidebarPreference = null;
    activeReviewCommentId = null;
    activeReviewItemId = null;
    reviewComposerState = null;
    reviewDeleteConfirmId = null;
    // A new document invalidates every retained card and its cached geometry.
    reviewLayout = [];
    reviewCardCache.clear();
    for (const button of reviewModeButtons) {
      button.setAttribute("aria-pressed", String(button.dataset.reviewMode === reviewMode));
    }
    breakTypingSession();
    currentName = name;
    docTitleEl.value = name;
    documentChrome.hidden = false;
    setDocumentState("opened");
    saveBtn.disabled = false;
    populateSaveFormats();
    showCompatibilityFindings(0, "export");
    railOutline.disabled = false;
    railPages.disabled = false;
    populateStyles();
    populateTableStyles();
    dropEl.hidden = true;
    document.body.classList.add("doc-loaded");
    // Redline visible by default (Q1): a document that arrives already carrying
    // tracked changes shows markup on open, so struck deletions are never
    // invisible behind a buried toggle. A clean document opens with markup off.
    // Set before the first `renderAll` so it renders the correct layout once.
    showingChanges = documentHasTrackedChanges();
    reflectShowingChangesState();
    // Apply any caller-supplied opened-document state (e.g. a gallery demo
    // preset) now — after the review/mode reset above and before the first
    // render, so it renders once in the requested state — rather than making the
    // caller await the network font fetch below (which would leave a demo sitting
    // on the plain editor for seconds).
    if (typeof onOpened === "function") onOpened();
    const fontWarnings = await provisionFonts(name);
    await renderAll();
    if (fontWarnings.length > 0) {
      setStatus(
        `Opened ${name}; unavailable web fonts: ${[...new Set(fontWarnings)].join(", ")}`,
        "error",
      );
    }
    buildOutline();
    buildAccessibilityTree();
    drawSelection();
    // Post-render demo hook (e.g. a gallery preset that needs laid-out geometry,
    // like selecting text). Runs once the page is composed.
    if (typeof onRendered === "function") onRendered();
  } catch (err) {
    console.error(err);
    setStatus(`Could not open ${name}: ${err.message ?? err}`, "error");
  }
}

// ---- Document rename ---------------------------------------------------------
// The header title is the input itself (styled as plain text until focused),
// matching the Word/Docs "click the title to rename" convention. Renaming
// only changes `currentName` (what Save downloads as); it never touches the
// open document's own model or its docProps/core.xml `dc:title`.
function commitRename() {
  const trimmed = docTitleEl.value.trim();
  if (!trimmed) {
    docTitleEl.value = currentName;
    return;
  }
  const named = /\.docx$/i.test(trimmed) ? trimmed : `${trimmed}.docx`;
  const changed = named !== currentName;
  currentName = named;
  docTitleEl.value = named;
  if (changed) setDocumentState("edited");
}

docTitleEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    docTitleEl.blur();
  } else if (e.key === "Escape") {
    e.preventDefault();
    docTitleEl.value = currentName;
    docTitleEl.blur();
  }
});
docTitleEl.addEventListener("focus", () => docTitleEl.select());
docTitleEl.addEventListener("blur", commitRename);

// Provision the host-owned named families in one bounded batch/repagination,
// then fetch only the script fallbacks this document's uncovered code points
// require. Network failures do not block opening: the target-bundled
// metric-compatible faces remain available.
async function provisionFonts(name) {
  if (!doc) return [];
  const warnings = [];
  setStatus(`Fetching web fonts for ${name}…`);

  const named = await Promise.allSettled(
    NAMED_WEB_FONT_FACES.map((face) => fetchFontBytes(face.url, fontCache)),
  );
  const namedBytes = named
    .filter((result) => result.status === "fulfilled")
    .map((result) => result.value);
  if (namedBytes.length > 0) {
    const packed = packFontBytes(namedBytes);
    doc.registerFonts(packed.bytes, packed.lengths);
  }
  // The WASM shaper now holds the authoritative copy of these faces, so release
  // the JS byte cache (~9 MB) rather than double-holding it for the tab's life.
  // A subsequent document re-fetches + re-registers from the CDN as usual.
  for (const face of NAMED_WEB_FONT_FACES) fontCache.delete(face.url);
  for (const [index, result] of named.entries()) {
    if (result.status === "rejected") {
      const face = NAMED_WEB_FONT_FACES[index];
      console.warn(`font ${face.family} (${face.url}) failed:`, result.reason);
      warnings.push(face.family);
    }
  }

  warnings.push(...(await provisionMissingFallbacks(name)));
  return warnings;
}

/** The script fallback buckets already fetched + registered this session, so a
 *  later coverage check never re-fetches a font it already has. */
const provisionedFallbackKeys = new Set();

/** Fetches and registers any script fallback fonts the document now needs but
 *  hasn't got yet (`doc.missingCoverage()` → buckets), skipping ones already
 *  provisioned. Used both on open and after an edit that introduces new glyphs
 *  (e.g. a checklist's `☐`/`☒` markers), so newly-added symbols render instead of
 *  tofu. Returns the keys that failed to load. */
async function provisionMissingFallbacks(label) {
  const warnings = [];
  if (!doc) return warnings;
  const missing = doc.missingCoverage();
  const keys = fallbackKeysFor(missing).filter((key) => !provisionedFallbackKeys.has(key));
  if (keys.length === 0) return warnings;
  setStatus(`Fetching fonts for ${label} (${keys.join(", ")})…`);
  for (const key of keys) {
    const { url, scripts } = SCRIPT_FALLBACK_FONTS[key];
    try {
      const bytes = await fetchFontBytes(url, fontCache);
      doc.registerFallbackFont(bytes, scripts); // registers + re-paginates
      provisionedFallbackKeys.add(key);
      // WASM holds the authoritative copy now; drop the JS cache entry so the
      // fallback bytes are not double-held for the session (see registerFonts).
      fontCache.delete(url);
    } catch (err) {
      console.warn(`font ${key} (${url}) failed:`, err);
      setStatus(`Could not load the ${key} font — some text may show as ▯`, "error");
      warnings.push(key);
    }
  }
  return warnings;
}

/** Ensures any glyphs a just-applied edit introduced (e.g. checklist checkbox
 *  markers) have a covering font, then re-renders if one was fetched. */
async function ensureGlyphCoverage(label) {
  const before = provisionedFallbackKeys.size;
  await provisionMissingFallbacks(label);
  if (provisionedFallbackKeys.size !== before) {
    await renderAll();
  }
}

// ---- Page virtualization -----------------------------------------------------
// A document keeps ONE lightweight `.page-wrap` (sized from the page box) per
// page, but a live raster `<canvas>` only for pages in or near the viewport.
// Off-screen pages are blank sheet placeholders, so tab memory is bounded by the
// viewport, not the page count. An IntersectionObserver mounts a page's canvas
// as it scrolls in and releases it (freeing the RGBA buffer) as it scrolls out.

/** Keep a live canvas for pages within ~one viewport-height of the visible band
 *  above and below, so a normal scroll never reveals an unpainted page. */
const PAGE_VIRTUALIZATION_ROOT_MARGIN = "100% 0px";
let pageObserver = null;

function ensurePageObserver() {
  if (pageObserver) return pageObserver;
  pageObserver = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const idx = pageIndexOfWrap(entry.target);
        if (idx < 0) continue;
        const page = pages[idx];
        page.visible = entry.isIntersecting;
        if (entry.isIntersecting) paintPageCanvas(page, idx);
        else releasePageCanvas(page);
      }
    },
    { root: viewportEl, rootMargin: PAGE_VIRTUALIZATION_ROOT_MARGIN },
  );
  return pageObserver;
}

/** Rewire the observer to the current page set (called after every rebuild). */
function observePages() {
  const obs = ensurePageObserver();
  obs.disconnect();
  for (const page of pages) {
    page.wrap.__pageIndex = page.pageNumber - 1;
    obs.observe(page.wrap);
  }
}

/** Resolve an observed wrap back to its (still-current) page index, or -1. */
function pageIndexOfWrap(wrap) {
  const idx = wrap.__pageIndex;
  return Number.isInteger(idx) && pages[idx]?.wrap === wrap ? idx : -1;
}

/** Mount and paint a page's raster canvas if it has none. The RGBA buffer is
 *  freed back to WASM immediately after the blit so it never accumulates. */
function paintPageCanvas(page, index) {
  if (!doc || page.canvas) return;
  let bmp;
  try {
    bmp = doc.renderPage(index, currentDpi());
  } catch (err) {
    console.error(`render page ${index}`, err);
    return;
  }
  const canvas = document.createElement("canvas");
  canvas.className = "page";
  canvas.width = bmp.widthPx;
  canvas.height = bmp.heightPx;
  // The surface is fully opaque, so tiny-skia's premultiplied RGBA equals the
  // straight-alpha RGBA `ImageData` expects — a direct blit is correct.
  canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
  bmp.free(); // return the ~13 MB RGBA Vec to WASM deterministically, not at GC.
  // The canvas sits under the transparent caret/selection overlay.
  page.wrap.insertBefore(canvas, page.overlay);
  page.canvas = canvas;
}

/** Drop a page's raster canvas (releases its GPU/CPU pixels). The sheet-sized
 *  wrap remains, so layout and scroll height are unchanged. */
function releasePageCanvas(page) {
  if (!page.canvas) return;
  page.canvas.remove();
  page.canvas = null;
}

/** Synchronously paint the pages currently on screen (plus one screenful of
 *  margin), so the viewport is never briefly blank before the async observer
 *  fires. Matches the observer's rootMargin. */
function paintPagesInView() {
  if (!pages.length) return;
  const vr = viewportEl.getBoundingClientRect();
  const margin = vr.height;
  for (let i = 0; i < pages.length; i++) {
    const page = pages[i];
    const r = page.wrap.getBoundingClientRect();
    const onscreen = r.bottom >= vr.top - margin && r.top <= vr.bottom + margin;
    page.visible = onscreen;
    if (onscreen) paintPageCanvas(page, i);
  }
}

// ---- Print (⌘/Ctrl+P) --------------------------------------------------------
// Printing must reproduce EVERY page, but the viewport keeps a live raster only
// for on-screen pages (virtualization), so `window.print()` alone would emit
// mostly-blank sheets. A dedicated print path renders each page independently
// with `doc.renderPage` into an off-DOM `#printContainer` (one canvas per page
// at the page's real physical size), calls the browser print dialog, then tears
// the container down. It never touches the live `.page-wrap`/`.overlay`/canvas
// set, so the normal virtualized state is preserved automatically — nothing to
// restore. Each transient bitmap is `free()`d right after the blit (as
// `paintPageCanvas` does) so a long document's print build never balloons tab
// memory beyond the sheet canvases it must hold to print.

// Print raster resolution. High enough for crisp printed text, low enough that
// the transient per-page RGBA buffer (freed immediately) and the retained sheet
// canvases stay modest even for a long document.
const PRINT_DPI = 150;
let printStyleEl = null;

/** Remove the off-DOM print container and its injected stylesheet, if present.
 *  Idempotent, so it is safe to call defensively before a build and in the
 *  `finally` after `window.print()`. */
function teardownPrint() {
  document.getElementById("printContainer")?.remove();
  printStyleEl?.remove();
  printStyleEl = null;
}

/** Build the print-only stylesheet. On screen `#printContainer` is hidden; in
 *  print it is the ONLY visible element (all editor chrome is hidden) and each
 *  page sheet breaks to its own physical page. `@page` is sized to the document
 *  page with zero margin — the rendered raster already includes the document's
 *  own margins, so a sheet margin here would double them. */
function buildPrintStyle(wIn, hIn) {
  const style = document.createElement("style");
  style.id = "printStyle";
  style.textContent = `
#printContainer { display: none; }
@media print {
  html, body { margin: 0 !important; padding: 0 !important; background: #fff !important; }
  body > *:not(#printContainer) { display: none !important; }
  #printContainer { display: block !important; }
  #printContainer .print-page { display: block; break-after: page; page-break-after: always; }
  #printContainer .print-page:last-child { break-after: auto; page-break-after: auto; }
  @page { size: ${wIn}in ${hIn}in; margin: 0; }
}`;
  return style;
}

/** Print the rendered document pages. Read-only, always allowed (no mutation
 *  gate, no unsaved-changes requirement). */
function printDocument() {
  if (!doc) return;
  teardownPrint(); // clear any stale build from an interrupted prior print
  const count = doc.pageCount;
  if (!count) return;

  // The sheet size (for `@page`) comes from the first page in inches. Each page
  // canvas is additionally sized to its own physical dimensions, so a document
  // with mixed page sizes still prints each page at its true proportion.
  const first = doc.pageSize(0);
  const sheetWIn = first.widthTwip / TWIPS_PER_INCH;
  const sheetHIn = first.heightTwip / TWIPS_PER_INCH;
  first.free();

  const container = document.createElement("div");
  container.id = "printContainer";
  container.setAttribute("aria-hidden", "true");

  for (let i = 0; i < count; i++) {
    let bmp;
    try {
      bmp = doc.renderPage(i, PRINT_DPI);
    } catch (err) {
      console.error(`print render page ${i}`, err);
      continue;
    }
    const canvas = document.createElement("canvas");
    canvas.className = "print-page";
    canvas.width = bmp.widthPx;
    canvas.height = bmp.heightPx;
    canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
    bmp.free(); // return the RGBA buffer to WASM now, not at GC.
    // Present the high-res raster at the page's true physical size so it fills
    // the (margin-0) sheet exactly and prints at full resolution.
    const size = doc.pageSize(i);
    canvas.style.width = `${size.widthTwip / TWIPS_PER_INCH}in`;
    canvas.style.height = `${size.heightTwip / TWIPS_PER_INCH}in`;
    size.free();
    container.appendChild(canvas);
  }

  printStyleEl = buildPrintStyle(sheetWIn, sheetHIn);
  document.head.appendChild(printStyleEl); // hides the container on screen first
  document.body.appendChild(container);
  try {
    window.print();
  } finally {
    // `window.print()` blocks until the dialog is dismissed in Chromium/Firefox,
    // so the sheets are gone as soon as printing ends — the viewport's live
    // virtualized canvases were never disturbed.
    teardownPrint();
  }
}

async function renderAll() {
  if (!doc) return;
  // Keep the engine's render layout in sync with the "Show changes" toggle: a
  // fresh markup layout when on (so a preview reflects the latest document), the
  // live editing layout when off. Caret/selection always use the editing layout.
  doc.setShowChanges(showingChanges);
  clearFindParagraphCache();
  const token = ++renderToken;
  if (zoomMode !== "custom") zoomFactor = computeFitZoom(zoomMode);
  const zoom = zoomFactor;
  updateZoomDisplay();
  const count = doc.pageCount;
  // Logical CSS px per twip at this zoom — independent of devicePixelRatio, so
  // the page box geometry (hence scroll height and hit-test scale) is stable.
  const cssPerTwip = (BASE_DPI * zoom) / TWIPS_PER_INCH;

  // Build the replacement page set off-DOM and publish it atomically. Only the
  // sheet-sized wraps + overlays are created here; the raster canvases are
  // mounted lazily by the viewport observer, so a large document never rasters
  // every page up front.
  const nextPages = [];
  const fragment = document.createDocumentFragment();
  const renderingStatus =
    `Rendering ${count} page${count === 1 ? "" : "s"} at ${Math.round(zoom * 100)}%…`;
  setStatus(renderingStatus);

  for (let i = 0; i < count; i++) {
    if (token !== renderToken) return;

    // The page box in twips — the domain of hit-testing and selection geometry,
    // and the source of the wrap's fixed CSS size so it holds space whether or
    // not a live canvas is currently mounted.
    const size = doc.pageSize(i);
    const wTwip = size.widthTwip;
    const hTwip = size.heightTwip;
    size.free();

    const wrap = document.createElement("div");
    wrap.className = "page-wrap";
    wrap.style.width = `${wTwip * cssPerTwip}px`;
    wrap.style.height = `${hTwip * cssPerTwip}px`;

    // A transparent overlay above the canvas holds the caret/selection we draw
    // ourselves from engine geometry — so the highlight matches the raster
    // exactly (doc 58: custom engine-driven selection, no overlay-vs-glyph drift).
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    wrap.appendChild(overlay);

    fragment.appendChild(wrap);
    nextPages.push({ pageNumber: i + 1, wrap, overlay, canvas: null, wTwip, hTwip, visible: false });
  }

  if (token !== renderToken) return;
  pages = nextPages;
  pagesEl.replaceChildren(ruler, fragment); // ruler sits above the pages, same width
  buildRuler();
  observePages(); // wire the viewport observer to the new wraps
  paintPagesInView(); // paint what is on screen now; the observer handles scroll
  drawSelection(); // re-place any existing selection at the new zoom
  if (token === renderToken) {
    // A command may have reported a more important status while this async
    // render was running. Clear only the progress message this render owns.
    if (statusEl.textContent === renderingStatus) setStatus("");
    updateStats();
    if (!pagesPanel.hidden) buildPages();
  }
}

// ---- Selection & copy (doc 58 pipeline: hit-test → selection → draw → copy) ---

/** twip → CSS px scale for a page, from the wrap's live on-screen size, so it
 *  tracks the render under any zoom / DPR / CSS scaling. */
function scaleOf(page) {
  // Measured from the sheet-sized wrap (always present), so hit-test/selection
  // geometry holds even when the page's raster canvas is virtualized away.
  const rect = page.wrap.getBoundingClientRect();
  return { rect, sx: rect.width / page.wTwip, sy: rect.height / page.hTwip };
}

/** A pointer event on a page's overlay/canvas → that page's local twip point. */
function pointToTwip(page, event) {
  const { rect, sx, sy } = scaleOf(page);
  return {
    // Clamp edge/gap clicks into the page box. The layout hit tester then
    // resolves the nearest caret on that page instead of returning no hit for
    // a point just outside the rasterized sheet.
    x: Math.max(0, Math.min(page.wTwip, Math.round((event.clientX - rect.left) / sx))),
    y: Math.max(0, Math.min(page.hTwip, Math.round((event.clientY - rect.top) / sy))),
  };
}

/** Resolve a pointer event to a model anchor, or null if it misses content. */
function anchorAt(page, event) {
  const { x, y } = pointToTwip(page, event);
  const hit = doc.hitTest(page.pageNumber, x, y);
  if (!hit) return null;
  const anchor = { node: hit.node, offset: hit.offset };
  hit.free(); // release the WASM-owned payload; we copied out its fields
  return anchor;
}

/** Clears every page's caret/selection layer. */
function clearOverlays() {
  for (const p of pages) p.overlay.replaceChildren();
}

/** Reads the current comment list once (JSON round-trip), or `[]` on
 *  failure. Shared by marker rendering and by the caret-driven activation
 *  below so both agree on the same anchors. */
function reviewComments() {
  if (!doc) return [];
  try { return JSON.parse(doc.listComments()) ?? []; } catch { return []; }
}

/** The comment (if any) whose anchor range contains `anchor` (a `{node,
 *  offset}` model position), matching the same open/resolved visibility rule
 *  `paintReviewMarkers` uses. Resolved comments only match while their card
 *  is already explicitly expanded. */
function reviewCommentAtAnchor(anchor) {
  if (!anchor?.node) return null;
  for (const comment of reviewComments()) {
    const explicitlyOpen = activeReviewItemId === `comment:${comment.id}`;
    if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
    if (comment.anchor.node !== anchor.node) continue;
    const start = Number(comment.anchor.start) || 0;
    const end = Number(comment.anchor.end) || 0;
    if (anchor.offset >= start && anchor.offset <= end) return comment;
  }
  return null;
}

/** Non-blocking side effect of an authoritative caret placement
 *  (REVIEW-GAP-005, docs/81): if the resulting caret lands inside a
 *  commented range, expand/surface that comment's card. This never touches
 *  `selection` — document hit-testing (docs/80) is always the sole source
 *  of truth for where the caret lands; card expansion is derived from the
 *  resulting caret, never the other way around, and never blocks or delays
 *  caret placement. */
/** The review item (comment or revision) whose anchor range EXACTLY equals the
 *  current non-collapsed selection, or null. A review target is focused by
 *  selecting its exact range, so an exact match uniquely identifies the item the
 *  reviewer picked — even where a suggestion shares a boundary with (or nests
 *  inside) a comment, which the point-based `reviewCommentAtAnchor` /
 *  smallest-containing scans would otherwise resolve to the wrong item. */
function reviewItemForExactSelection() {
  const a = selection?.anchor;
  const f = selection?.focus;
  if (!a || !f || a.node !== f.node) return null;
  const lo = Math.min(a.offset, f.offset);
  const hi = Math.max(a.offset, f.offset);
  if (lo === hi) return null; // a collapsed caret has no range to match exactly
  for (const entry of reviewAnchorIndex) {
    if (entry.node === a.node && entry.start === lo && entry.end === hi) return entry;
  }
  return null;
}

function syncActiveReviewCommentToCaret(anchor) {
  // A freshly focused review target selects its own exact range: activate THAT
  // item so the clustered sidebar layout (REVIEW-GAP-019) anchors the selected
  // card to its own marker. Without this, selecting a suggestion that shares a
  // boundary with a comment activated the comment instead, and the suggestion's
  // card drifted from its marker (they moved together on scroll, never closing
  // the gap).
  const exact = reviewItemForExactSelection();
  if (exact) {
    activeReviewItemId = exact.itemId;
    activeReviewCommentId = exact.type === "comment" ? exact.dataId : null;
    reviewSidebarPreference = true;
    scheduleReviewMarginRender();
    return;
  }
  const comment = reviewCommentAtAnchor(anchor);
  if (comment) {
    activeReviewCommentId = comment.id;
    activeReviewItemId = `comment:${comment.id}`;
    reviewSidebarPreference = true;
    scheduleReviewMarginRender();
    return;
  }
  // No comment under the caret — surface a tracked-change card whose anchor range
  // contains the caret, so caret-driven expansion works for suggestions too
  // (REVIEW-GAP-019). The smallest containing range wins when several stack.
  if (!anchor?.node) return;
  const offset = Number(anchor.offset) || 0;
  let best = null;
  for (const entry of reviewAnchorIndex) {
    if (entry.node !== anchor.node) continue;
    if (offset < entry.start || offset > entry.end) continue;
    if (!best || entry.end - entry.start < best.end - best.start) best = entry;
  }
  if (!best || best.itemId === activeReviewItemId) return;
  activeReviewItemId = best.itemId;
  reviewSidebarPreference = true;
  scheduleReviewMarginRender();
}

/** Paint comment ranges in the existing overlay layer. This never touches the
 * canvas or document layout; it is the same interaction layer used by the
 * selection highlight and caret. These markers are a pure visual affordance:
 * they intentionally carry no click handler of their own, so a pointer event
 * always falls through to the page's normal pointerdown hit-testing (see
 * `onPointerDown` / `syncActiveReviewCommentToCaret`) instead of hijacking
 * caret placement (REVIEW-GAP-005). */
function paintReviewMarkers() {
  if (!doc) return;
  const summary = readReviewData(doc);
  for (const comment of summary.comments ?? []) {
    const explicitlyOpen = activeReviewItemId === `comment:${comment.id}`;
    if ((comment.resolved && !explicitlyOpen) || !comment.anchor?.node) continue;
    const { node, start, end } = comment.anchor;
    const rects = doc.selectionRects(node, Number(start) || 0, node, Number(end) || 0);
    const color = reviewAuthorColor(reviewAuthorKey(comment));
    const tooltip = reviewCommentTooltip(comment);
    for (let i = 0; i < rects.length; i += 5) {
      const el = place(rects.slice(i, i + 5), activeReviewCommentId === comment.id ? "review-comment-marker review-comment-marker-active" : "review-comment-marker");
      if (el) {
        el.dataset.reviewCommentId = comment.id;
        el.style.setProperty("--review-author-color", color);
        el.title = tooltip;
      }
    }
  }
  for (const revision of summary.revisions ?? []) {
    const range = revisionRange(revision);
    if (!range) continue;
    const deletionLike = revision.kind === "deletion" || revision.kind === "move_from";
    let rects = doc.selectionRects(range.startNode, range.startOffset, range.endNode, range.endOffset);
    if (rects.length < 5 && deletionLike && range.startNode === range.endNode) {
      rects = doc.caretRect(range.startNode, range.startOffset);
    }
    const moveItemId = revision.movePair?.fromStart && revision.movePair?.toStart
      ? `revision:move:${revision.movePair.fromStart}:${revision.movePair.toStart}`
      : null;
    // Highlight the marker whenever ITS sidebar item is active. A revision can be
    // surfaced under any of three ids depending on how the sidebar groups it — a
    // move pair, a typing/replacement/formatting group (keyed by groupId), or an
    // ungrouped revision (keyed by its own id) — so match against all three. This
    // makes a selected inline suggestion show the active state (and lets
    // `scrollReviewSelectionIntoView` target its marker), not only moves.
    const activeIds = [
      moveItemId,
      revision.groupId ? `revision:${revision.groupId}` : null,
      `revision:${revision.id}`,
    ];
    const active = activeIds.includes(activeReviewItemId)
      ? " review-revision-marker-active"
      : "";
    const kind = `${deletionLike
      ? "review-revision-marker review-deletion-marker"
      : "review-revision-marker review-insertion-marker"}${active}`;
    const color = reviewAuthorColor(reviewAuthorKey(revision));
    const tooltip = reviewRevisionTooltip(revision);
    for (let i = 0; i < rects.length; i += 5) {
      const el = place(rects.slice(i, i + 5), kind);
      if (el) {
        el.style.setProperty("--review-author-color", color);
        el.title = tooltip;
        // Inline accept/reject affordance (Q2): hovering a tracked-change marker
        // previews a compact card; pressing on it pins the card open. The
        // pointerdown is NOT swallowed, so it still bubbles to the page's
        // hit-testing and caret placement is unaffected (REVIEW-GAP-005). Opening
        // on pointerdown (before the caret repaint detaches this marker) keeps the
        // pinned card reliable; the card itself lives on document.body and
        // survives the repaint.
        el.dataset.reviewRevisionId = String(revision.id ?? "");
        el.addEventListener("mouseenter", () => showReviewInlineCard(revision, el, false));
        el.addEventListener("mouseleave", () => scheduleReviewInlineCardHide());
        el.addEventListener("pointerdown", () => showReviewInlineCard(revision, el, true));
      }
    }
  }
}

/** Draws the current selection from engine geometry: a highlight for a real
 *  range, else a caret at the focus (so a click — or a range with no visible
 *  rects — always shows a cursor). */
function drawSelection() {
  if (!doc) return;
  clearOverlays();
  paintReviewMarkers();
  paintChecklistMarkers();
  // A selected object owns the visible chrome (outline + handles) in place of the
  // text caret; in "editing" mode the ordinary text caret is shown instead.
  if (objectSelection && objectSelection.mode === "selected") {
    paintObjectSelection();
  } else if (selection) {
    paintTableSelection();
    paintActiveCell(selection.focus); // under the caret/highlight
    paintSelection(selection);
    paintTableResizeHandles(selection.focus);
  }
  updateObjectSelectionState();
  updateObjectContextBar();
  updateToolbar();
  updateReviewControls();
  scheduleReviewMarginRender();
  updatePageNumber();
  updateRulerMarkers();
  positionSelToolbar();
}

function paintTableSelection() {
  if (!tableSelection) return;
  const rects = doc.tableSelectionRects(tableSelection.node, tableSelection.mode);
  for (let i = 0; i + 4 < rects.length; i += 5) place(rects.slice(i, i + 5), "table-cell-selection");
}

/** Outlines the table cell the caret is in (nothing when not in a table), so the
 *  user always sees which cell they are editing. */
function paintActiveCell(focus) {
  const flat = doc.cellRect(focus.node); // [page, x, y, w, h] twips, or []
  if (flat.length >= 5) place(flat, "cell-outline");
}

/** Draws Word-style internal column resize handles for the active regular table. */
function paintTableResizeHandles(focus) {
  if (!doc?.inTable(focus.node)) return;
  const handles = doc.tableColumnResizeHandles(focus.node);
  for (let i = 0; i + 4 < handles.length; i += 5) {
    const [pageNumber, x, y, h, col] = handles.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { sx, sy } = scaleOf(page);
    const el = document.createElement("div");
    el.className = "table-col-resize-handle";
    el.dataset.col = String(col);
    el.style.left = `${x * sx}px`;
    el.style.top = `${y * sy}px`;
    el.style.height = `${h * sy}px`;
    el.addEventListener("pointerdown", (event) => startTableColumnResize(event, page, focus.node, col));
    page.overlay.appendChild(el);
  }
}

/** Paints the selected object's outline + eight resize/move handles from engine
 *  geometry (docs/85 §3.3), the same overlay mechanism as the caret/highlight so
 *  the chrome matches the raster exactly. Handles are display-only this slice. */
function paintObjectSelection() {
  const { node } = objectSelection;
  // In crop mode the image shows crop chrome (dimmed cut region + crop handles)
  // in place of the resize handles — the Word/Docs crop experience.
  if (objectCropSession && objectCropSession.node === node) {
    paintObjectCrop();
    return;
  }
  place(doc.objectRect(node), "object-outline");
  const handles = doc.objectHandles(node); // [page, cx, cy, kind] * 8
  for (let i = 0; i + 3 < handles.length; i += 4) {
    const [pageNumber, cx, cy, kind] = handles.slice(i, i + 4);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { sx, sy } = scaleOf(page);
    const el = document.createElement("div");
    el.className = "object-handle";
    el.dataset.handle = String(kind);
    el.style.left = `${cx * sx}px`;
    el.style.top = `${cy * sy}px`;
    el.addEventListener("pointerdown", (event) => startObjectResize(event, page, node, kind));
    page.overlay.appendChild(el);
  }
}

// ---- Image crop (direct-manipulation, Word/Docs standard) -------------------
// The Crop button enters a live crop MODE on the selected image: the image keeps
// its outline, the region that will be removed is dimmed, and eight crop handles
// on the kept rectangle are dragged to adjust it. Enter (or clicking Crop again,
// or clicking away) commits one SetImageCrop op; Esc cancels with no change.
// Crop is picture-only and, like resize, not representable as a tracked revision,
// so it is blocked in Suggesting/Viewing.
//
// Model note: `setImageCrop` insets are fractions of the SOURCE image and there
// is no engine getter for the current crop, so — like the previous numeric
// dialog — entering crop treats the displayed box as the full frame and a commit
// REPLACES any existing crop. (Outward re-crop of an already-cropped image needs
// an engine crop-getter + source-bytes binding; tracked as a follow-up.)

/** Enters crop mode on the selected image (or, if already cropping, commits). */
function enterCropMode() {
  if (!doc || !objectSelection || objectSelection.kind === "textbox") return;
  if (objectCropSession) {
    commitCrop();
    return;
  }
  if (reviewMode === "viewing") {
    blockMutationInViewing();
    return;
  }
  if (reviewMode === "suggesting") {
    setStatus("Cropping an image is not tracked; switch to Editing to crop it", "error");
    return;
  }
  const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
  const [, x, y, w, h] = rect;
  objectCropSession = {
    node: objectSelection.node,
    box: [x, y, w, h],
    crop: { l: 0, t: 0, r: 0, b: 0 },
    handleDrag: null,
  };
  focusEditorSurface();
  drawSelection();
  updateObjectContextBar();
  setStatus("Drag the handles to crop · Enter to apply · Esc to cancel");
}

// The smallest keep-fraction of the image per axis, so a crop can't collapse it.
const MIN_CROP_KEEP = 0.08;

/** Paints the crop chrome: the image outline, four dim strips over the removed
 *  margins (leaving the kept rectangle bright over the canvas image beneath),
 *  the kept-rectangle border, and eight draggable crop handles. Re-derives pixels
 *  from the stable box + live crop fractions, so it is correct after scroll/zoom. */
function paintObjectCrop() {
  const s = objectCropSession;
  const [bx, by, bw, bh] = s.box;
  const rectFlat = doc.objectRect(s.node);
  if (rectFlat.length < 5) return;
  const pageNumber = rectFlat[0];
  const page = pages[pageNumber - 1];
  if (!page) return;
  const { sx, sy } = scaleOf(page);
  place([pageNumber, bx, by, bw, bh], "object-outline");
  // Kept rectangle in twips (source box minus cropped edges).
  const kx = bx + s.crop.l * bw;
  const ky = by + s.crop.t * bh;
  const kw = bw * (1 - s.crop.l - s.crop.r);
  const kh = bh * (1 - s.crop.t - s.crop.b);
  // Four dim strips covering the removed margins around the kept rectangle.
  const strip = (x, y, w, h) => {
    if (w <= 0 || h <= 0) return;
    const el = document.createElement("div");
    el.className = "object-crop-dim";
    el.style.left = `${x * sx}px`;
    el.style.top = `${y * sy}px`;
    el.style.width = `${w * sx}px`;
    el.style.height = `${h * sy}px`;
    page.overlay.appendChild(el);
  };
  strip(bx, by, bw, ky - by); // top
  strip(bx, ky + kh, bw, by + bh - (ky + kh)); // bottom
  strip(bx, ky, kx - bx, kh); // left
  strip(kx + kw, ky, bx + bw - (kx + kw), kh); // right
  // Kept-rectangle border.
  const rect = document.createElement("div");
  rect.className = "object-crop-rect";
  rect.style.left = `${kx * sx}px`;
  rect.style.top = `${ky * sy}px`;
  rect.style.width = `${kw * sx}px`;
  rect.style.height = `${kh * sy}px`;
  page.overlay.appendChild(rect);
  // Eight crop handles on the kept rectangle (NW,N,NE,E,SE,S,SW,W).
  const points = [
    [kx, ky], [kx + kw / 2, ky], [kx + kw, ky], [kx + kw, ky + kh / 2],
    [kx + kw, ky + kh], [kx + kw / 2, ky + kh], [kx, ky + kh], [kx, ky + kh / 2],
  ];
  points.forEach(([cx, cy], kind) => {
    const el = document.createElement("div");
    el.className = "object-crop-handle";
    el.dataset.handle = String(kind);
    el.style.left = `${cx * sx}px`;
    el.style.top = `${cy * sy}px`;
    el.addEventListener("pointerdown", (event) => startCropHandleDrag(event, page, kind));
    page.overlay.appendChild(el);
  });
}

/** Begins dragging a crop handle. The kept rectangle updates live; the model is
 *  untouched until commit. */
function startCropHandleDrag(event, page, handleKind) {
  if (!objectCropSession) return;
  event.preventDefault();
  event.stopPropagation();
  const s = objectCropSession;
  s.handleDrag = {
    handleKind,
    page,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startCrop: { ...s.crop },
  };
  const move = (e) => updateCropHandleDrag(e);
  const up = (e) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (objectCropSession) objectCropSession.handleDrag = null;
    e.preventDefault();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/** Updates the kept rectangle from a crop-handle drag. Per-handle signs decide
 *  which edges move; each edge is clamped so opposite edges keep MIN_CROP_KEEP of
 *  the image between them and neither passes the image bounds. */
function updateCropHandleDrag(event) {
  const s = objectCropSession;
  if (!s || !s.handleDrag) return;
  const drag = s.handleDrag;
  const [, , bw, bh] = s.box;
  const { sx, sy } = scaleOf(drag.page);
  const dxFrac = bw > 0 ? (event.clientX - drag.startClientX) / sx / bw : 0;
  const dyFrac = bh > 0 ? (event.clientY - drag.startClientY) / sy / bh : 0;
  // Handle index → which edges it moves. NW,N,NE,E,SE,S,SW,W.
  const movesLeft = [true, false, false, false, false, false, true, true][drag.handleKind];
  const movesRight = [false, false, true, true, true, false, false, false][drag.handleKind];
  const movesTop = [true, true, true, false, false, false, false, false][drag.handleKind];
  const movesBottom = [false, false, false, false, true, true, true, false][drag.handleKind];
  const c = { ...drag.startCrop };
  if (movesLeft) c.l = Math.min(Math.max(0, drag.startCrop.l + dxFrac), 1 - drag.startCrop.r - MIN_CROP_KEEP);
  if (movesRight) c.r = Math.min(Math.max(0, drag.startCrop.r - dxFrac), 1 - drag.startCrop.l - MIN_CROP_KEEP);
  if (movesTop) c.t = Math.min(Math.max(0, drag.startCrop.t + dyFrac), 1 - drag.startCrop.b - MIN_CROP_KEEP);
  if (movesBottom) c.b = Math.min(Math.max(0, drag.startCrop.b - dyFrac), 1 - drag.startCrop.t - MIN_CROP_KEEP);
  s.crop = c;
  drawSelection();
  event.preventDefault();
}

/** Commits the crop as one SetImageCrop op (or clears it when nothing is cropped),
 *  then exits crop mode. */
function commitCrop() {
  const s = objectCropSession;
  if (!s) return;
  const { l, t, r, b } = s.crop;
  const node = s.node;
  const cropped = l > 0.0005 || t > 0.0005 || r > 0.0005 || b > 0.0005;
  objectCropSession = null;
  runEdit(() => doc.setImageCrop(node, cropped ? [l, t, r, b] : null), { gate: true }).then(() => {
    updateObjectContextBar();
    setStatus(cropped ? "Image cropped" : "");
  });
}

/** Exits crop mode with no change. */
function cancelCrop() {
  if (!objectCropSession) return;
  objectCropSession = null;
  drawSelection();
  updateObjectContextBar();
  setStatus("");
}

/** Begins a handle drag-resize (docs/85 §5.3). Records the object's current
 *  placed size and shows a live preview outline; the model is untouched until
 *  release. Object geometry is not trackable, so a resize is blocked in
 *  Suggesting/Viewing mode. */
function startObjectResize(event, page, node, handleKind) {
  if (!doc || !objectSelection || objectSelection.node !== node) return;
  event.preventDefault();
  event.stopPropagation(); // do not let the page pointerdown re-hit-test
  if (reviewMode === "viewing") {
    blockMutationInViewing();
    return;
  }
  if (reviewMode === "suggesting") {
    setStatus("Resizing an object is not tracked; switch to Editing to resize it", "error");
    return;
  }
  focusEditorSurface();
  hideLinkChip();
  resetPointerGesture();
  const rect = doc.objectRect(node); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
  const [, x, y, w, h] = rect;
  const preview = document.createElement("div");
  preview.className = "object-resize-preview";
  const { sx, sy } = scaleOf(page);
  preview.style.left = `${x * sx}px`;
  preview.style.top = `${y * sy}px`;
  preview.style.width = `${w * sx}px`;
  preview.style.height = `${h * sy}px`;
  page.overlay.appendChild(preview);
  objectResizeDrag = {
    node,
    handleKind,
    page,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startX: x,
    startY: y,
    startW: w,
    startH: h,
    lastW: w,
    lastH: h,
    aspect: h > 0 ? w / h : 1,
    preview,
  };
  event.currentTarget.setPointerCapture?.(event.pointerId);
}

// Minimum object edge in twips (~0.1in) so a drag can't collapse an object.
const MIN_OBJECT_TWIP = 144;

/** Updates the resize preview from the pointer delta. Per-handle signs decide
 *  which edges grow (corners = both axes, N/S = height, E/W = width). Shift on a
 *  corner constrains to the original aspect ratio (docs/85 §10.3). */
function updateObjectResize(event) {
  if (!objectResizeDrag) return;
  const drag = objectResizeDrag;
  const { sx, sy } = scaleOf(drag.page);
  const dxTwip = Math.round((event.clientX - drag.startClientX) / sx);
  const dyTwip = Math.round((event.clientY - drag.startClientY) / sy);
  // Handle index → (dw factor, dh factor). NW,N,NE,E,SE,S,SW,W.
  const [fw, fh] = [
    [-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0],
  ][drag.handleKind];
  let newW = Math.max(MIN_OBJECT_TWIP, drag.startW + fw * dxTwip);
  let newH = Math.max(MIN_OBJECT_TWIP, drag.startH + fh * dyTwip);
  // Corner aspect-lock, kind-aware to match the platform norm: a PICTURE keeps
  // its proportions by DEFAULT on a corner drag (Word/Docs lock aspect for
  // images) and Shift frees it; a TEXT BOX resizes freely by default and Shift
  // locks. Either way the constraint drives both edges from the axis that moved
  // more, so the object keeps its proportions.
  const isCorner = fw !== 0 && fh !== 0;
  const isImage = objectSelection?.kind !== "textbox";
  const lockAspect = isCorner && (isImage ? !event.shiftKey : event.shiftKey);
  if (lockAspect) {
    if (Math.abs(newW - drag.startW) >= Math.abs(newH - drag.startH)) {
      newH = Math.max(MIN_OBJECT_TWIP, Math.round(newW / drag.aspect));
    } else {
      newW = Math.max(MIN_OBJECT_TWIP, Math.round(newH * drag.aspect));
    }
  }
  drag.lastW = newW;
  drag.lastH = newH;
  drag.preview.style.width = `${newW * sx}px`;
  drag.preview.style.height = `${newH * sy}px`;
  event.preventDefault();
}

/** Commits (or cancels) the resize on release: one `SetExtent` op, converting the
 *  final placed size (twips) to authored EMU. Returns whether a drag was active. */
function finishObjectResize(event) {
  if (!objectResizeDrag) return false;
  const drag = objectResizeDrag;
  objectResizeDrag = null;
  drag.preview.remove();
  event.preventDefault();
  const changed = Math.abs(drag.lastW - drag.startW) >= 8 || Math.abs(drag.lastH - drag.startH) >= 8;
  if (changed) {
    const EMU_PER_TWIP = 635;
    runEdit(() => doc.setObjectExtent(drag.node, drag.lastW * EMU_PER_TWIP, drag.lastH * EMU_PER_TWIP), {
      gate: true,
    });
  } else {
    drawSelection();
  }
  return true;
}

/** Reflects the object-selection state onto `#pages` as data attributes so the
 *  host (and tests) can observe the grammar state machine without reading into
 *  the overlay. */
function updateObjectSelectionState() {
  if (objectSelection) {
    pagesEl.dataset.objectSelected = objectSelection.node;
    pagesEl.dataset.objectKind = objectSelection.kind;
    pagesEl.dataset.objectMode = objectSelection.mode;
  } else {
    delete pagesEl.dataset.objectSelected;
    delete pagesEl.dataset.objectKind;
    delete pagesEl.dataset.objectMode;
  }
}

/** The lazily-created placeholder object context bar (docs/85 §4.1). */
let objectContextBarEl = null;

/** Shows/positions a context bar above a selected object. It describes only
 *  interactions that work in the current build; deferred actions never appear
 *  as product placeholders. */
function updateObjectContextBar() {
  if (!objectContextBarEl) {
    objectContextBarEl = document.createElement("div");
    objectContextBarEl.className = "object-context-bar";
    objectContextBarEl.hidden = true;
    document.body.appendChild(objectContextBarEl);
  }
  if (!objectSelection || objectSelection.mode !== "selected") {
    objectContextBarEl.hidden = true;
    return;
  }
  const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h]
  const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
  if (!page) {
    objectContextBarEl.hidden = true;
    return;
  }
  const { rect: pageRect, sx, sy } = scaleOf(page);
  const label = objectSelection.kind === "textbox" ? "Text box" : "Image";
  objectContextBarEl.replaceChildren();
  const strong = document.createElement("strong");
  strong.textContent = label;
  objectContextBarEl.appendChild(strong);
  if (objectSelection.anchored) {
    // A floating object exposes a live Wrap control; move + resize are drags.
    const active = doc.objectWrap(objectSelection.node);
    const wrap = document.createElement("div");
    wrap.className = "object-wrap-menu";
    for (const [value, text] of WRAP_MODES) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "object-wrap-btn";
      btn.dataset.wrap = value;
      btn.textContent = text;
      btn.setAttribute("aria-pressed", String(value === active));
      btn.addEventListener("pointerdown", (event) => event.preventDefault()); // keep selection
      btn.addEventListener("click", () => setObjectWrap(value));
      wrap.appendChild(btn);
    }
    objectContextBarEl.appendChild(wrap);
    const hint = document.createElement("small");
    hint.textContent = "Drag to move · handles to resize";
    objectContextBarEl.appendChild(hint);
  } else {
    const hint = document.createElement("small");
    hint.textContent = "Drag handles to resize";
    objectContextBarEl.appendChild(hint);
  }
  // Object-editing actions (alt text / crop / delete). Each keeps the selection
  // on pointerdown (like the wrap buttons) and opens its dialog / runs its op.
  const divider = document.createElement("span");
  divider.className = "object-bar-divider";
  objectContextBarEl.appendChild(divider);
  const actions = document.createElement("div");
  actions.className = "object-bar-actions";
  actions.appendChild(objectBarButton("description", "Alt text", "Edit alt text", openAltTextDialog));
  if (objectSelection.kind !== "textbox") {
    // Crop is a picture-only operation; a text box has no source rectangle.
    // Direct-manipulation crop (drag handles) is the primary gesture; while a
    // crop session is live the button becomes "Apply" and reads as active.
    const cropping = !!objectCropSession && objectCropSession.node === objectSelection.node;
    const cropBtn = objectBarButton(
      "crop",
      cropping ? "Apply" : "Crop",
      cropping ? "Apply crop (Enter)" : "Crop image",
      enterCropMode,
    );
    if (cropping) cropBtn.classList.add("is-active");
    actions.appendChild(cropBtn);
  }
  actions.appendChild(objectBarButton("delete", "Delete", "Delete object", deleteSelectedObject, true));
  objectContextBarEl.appendChild(actions);
  objectContextBarEl.hidden = false;
  // Position just above the object's top-left, clamped into the viewport.
  const left = pageRect.left + rect[1] * sx;
  const top = pageRect.top + rect[2] * sy - objectContextBarEl.offsetHeight - 8;
  objectContextBarEl.style.left = `${Math.max(8, left)}px`;
  objectContextBarEl.style.top = `${Math.max(8, top)}px`;
}

/** Selects an object as a unit (docs/85 §4.1). Keeps `selection` as a caret at
 *  the object's surrounding-text anchor so the two-step Escape can return to it.
 *  `anchored` marks a floating object (movable + wrappable); inline objects are
 *  resize-only. */
function selectObject(node, kind, anchor, anchored = false) {
  if (anchor) selection = { anchor, focus: anchor };
  objectSelection = { node, kind, mode: "selected", anchored };
  pendingFormat = null;
  tableSelection = null;
  drawSelection();
}

/** The wrap-mode choices offered for a floating object (docs/85 §5.3 / §10). */
const WRAP_MODES = [
  ["square", "Square"],
  ["tight", "Tight"],
  ["through", "Through"],
  ["topAndBottom", "Top & bottom"],
  ["behind", "Behind text"],
  ["front", "In front"],
];

/** Changes a floating object's text-wrap mode (docs/85 §5.3), as one undoable op.
 *  Blocked fail-closed in Suggesting/Viewing mode by `runEdit`'s gate. */
function setObjectWrap(mode) {
  if (!objectSelection || !objectSelection.anchored) return;
  runEdit(() => doc.setObjectWrap(objectSelection.node, mode), { gate: true });
}

/** Builds one object-context-bar action button (icon + label). Mirrors the wrap
 *  buttons: `pointerdown` is prevented so clicking it never deselects the object
 *  (the canvas pointerdown deselect is what the wrap buttons dodge too). */
function objectBarButton(icon, label, title, onClick, danger = false) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = `object-bar-btn${danger ? " danger" : ""}`;
  btn.title = title;
  btn.setAttribute("aria-label", title);
  btn.innerHTML = `<span class="ms" aria-hidden="true">${icon}</span><span>${label}</span>`;
  btn.addEventListener("pointerdown", (event) => event.preventDefault()); // keep selection
  btn.addEventListener("click", onClick);
  return btn;
}

/** Deletes the selected object as one undoable action (docs/85 §4). Mirrors the
 *  wrap op's apply path: `runEdit(..., { gate:true })` is the single fail-closed
 *  gate (read-only in Viewing, untracked-blocked in Suggesting). The object
 *  selection is dropped inside the thunk — which `runEdit` only runs once the gate
 *  passes — so `applyEditResult` repaints with the plain text caret the
 *  `EditResult` points at (the object's former surrounding-text anchor). */
function deleteSelectedObject() {
  if (!objectSelection || objectSelection.mode !== "selected") return;
  const node = objectSelection.node;
  runEdit(
    () => {
      const res = doc.deleteObject(node);
      objectSelection = null;
      clearObjectStatus();
      return res;
    },
    { gate: true },
  );
}

// ---- Object alt text + crop dialogs -----------------------------------------
// Reuse the shared `.dialog-overlay`/`.dialog-card` system (as the bookmark
// manager does). The node is captured on open so the dialog stays bound to one
// object even though `objectSelection` remains live behind the modal overlay.
const altTextDialog = document.getElementById("altTextDialog");
const altTextInput = document.getElementById("altTextInput");
const altTextForm = document.getElementById("altTextForm");
const altTextNote = document.getElementById("altTextNote");
const altTextClose = document.getElementById("altTextClose");
const altTextCancel = document.getElementById("altTextCancel");
const ALT_TEXT_HINT = "Leave empty to remove the alt text. Enter applies; Shift+Enter adds a line.";
/** The object a currently-open object dialog is editing, and the chrome to
 *  restore focus to when it closes. */
let objectDialogNode = null;
let objectDialogReturnFocus = null;

/** True (after surfacing the standard read-only/untracked message) if an object
 *  edit must not apply in the current review mode. Object edits are fail-closed
 *  in Suggesting (untracked) and read-only in Viewing — the same gate `runEdit`
 *  applies for delete/wrap; the dialogs pre-check it so they can close cleanly. */
function objectEditBlocked() {
  return blockMutationInViewing() || blockUntrackedInSuggesting();
}

function setAltTextNote(message, isError) {
  altTextNote.textContent = message || ALT_TEXT_HINT;
  altTextNote.classList.toggle("error", !!isError && !!message);
}

function openAltTextDialog() {
  if (!doc || !objectSelection || !altTextDialog) return;
  objectDialogNode = objectSelection.node;
  objectDialogReturnFocus = document.activeElement;
  // Prefill the current alt text so the user refines it rather than blind-
  // overwriting (Word/Docs both show the existing description).
  altTextInput.value = doc.objectDescr(objectSelection.node) ?? "";
  setAltTextNote("", false);
  altTextDialog.hidden = false;
  queueMicrotask(() => {
    altTextInput.focus();
    altTextInput.select();
  });
}

function closeAltTextDialog() {
  if (!altTextDialog || altTextDialog.hidden) return;
  altTextDialog.hidden = true;
  objectDialogNode = null;
  restoreObjectDialogFocus();
}

/** Applies the alt text as one undoable action. The engine rejects an
 *  over-length description; that is shown inline (its raw error name is internal
 *  vocabulary) rather than as the toolbar's generic status. Empty input sends
 *  `null`, which clears the alt text. */
function applyAltText() {
  if (!doc || !objectDialogNode) return;
  const text = altTextInput.value.trim();
  if (objectEditBlocked()) {
    closeAltTextDialog();
    return;
  }
  let res;
  try {
    res = doc.setObjectDescr(objectDialogNode, text || null);
  } catch (err) {
    console.warn("setObjectDescr ignored:", err?.message ?? err);
    setAltTextNote("That description is too long. Try a shorter one.", true);
    altTextInput.focus();
    return;
  }
  applyEditResult(res).then(() => {
    setDocumentState("edited");
    closeAltTextDialog();
    setStatus(text ? "Alt text updated" : "Alt text removed");
  });
}

/** Returns focus to the chrome that opened an object dialog, or the canvas if it
 *  is gone (the context bar is rebuilt on every repaint, so its buttons detach). */
function restoreObjectDialogFocus() {
  const returnTo = objectDialogReturnFocus;
  objectDialogReturnFocus = null;
  if (returnTo && typeof returnTo.focus === "function" && document.contains(returnTo)) {
    returnTo.focus({ preventScroll: true });
  } else {
    focusEditorSurface();
  }
}

if (altTextDialog) {
  altTextForm.addEventListener("submit", (event) => {
    event.preventDefault();
    applyAltText();
  });
  altTextInput.addEventListener("input", () => {
    if (altTextNote.classList.contains("error")) setAltTextNote("", false);
  });
  altTextInput.addEventListener("keydown", (event) => {
    // Enter applies; Shift+Enter inserts a newline (multi-line descriptions).
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      applyAltText();
    }
  });
  altTextClose.addEventListener("click", () => closeAltTextDialog());
  altTextCancel.addEventListener("click", () => closeAltTextDialog());
  altTextDialog.addEventListener("click", (event) => {
    if (event.target === altTextDialog) closeAltTextDialog();
  });
  altTextDialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeAltTextDialog();
    } else if (event.key === "Tab") {
      trapModalFocus(event, altTextDialog);
    }
  });
}

// Threshold (twips) beyond which a float body drag counts as a move, not a click.
const MOVE_THRESHOLD_TWIP = 40;

/** Begins a floating-object move drag (docs/85 §5.3): previews an outline that
 *  follows the pointer; the model is untouched until release. */
function startObjectMove(event, page, node) {
  if (!doc) return;
  if (reviewMode === "viewing" || reviewMode === "suggesting") {
    // Selection still happened; a move is simply not offered in these modes.
    return;
  }
  const rect = doc.objectRect(node); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
  const [, x, y, w, h] = rect;
  const preview = document.createElement("div");
  preview.className = "object-resize-preview";
  const { sx, sy } = scaleOf(page);
  preview.style.left = `${x * sx}px`;
  preview.style.top = `${y * sy}px`;
  preview.style.width = `${w * sx}px`;
  preview.style.height = `${h * sy}px`;
  page.overlay.appendChild(preview);
  objectMoveDrag = {
    node,
    page,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startX: x,
    startY: y,
    lastX: x,
    lastY: y,
    moved: false,
    preview,
  };
}

/** Updates the move preview from the pointer delta (page-local twips). */
function updateObjectMove(event) {
  if (!objectMoveDrag) return;
  const drag = objectMoveDrag;
  const { sx, sy } = scaleOf(drag.page);
  const dxTwip = Math.round((event.clientX - drag.startClientX) / sx);
  const dyTwip = Math.round((event.clientY - drag.startClientY) / sy);
  drag.lastX = Math.max(0, drag.startX + dxTwip);
  drag.lastY = Math.max(0, drag.startY + dyTwip);
  if (Math.abs(dxTwip) + Math.abs(dyTwip) > MOVE_THRESHOLD_TWIP) drag.moved = true;
  drag.preview.style.left = `${drag.lastX * sx}px`;
  drag.preview.style.top = `${drag.lastY * sy}px`;
  event.preventDefault();
}

/** Commits (or cancels) a float move on release: one `SetAnchor` to the new
 *  absolute page position (page-local twips → EMU). Returns whether a drag was
 *  active. A bare click (no movement) commits nothing — it was a select. */
function finishObjectMove(event) {
  if (!objectMoveDrag) return false;
  const drag = objectMoveDrag;
  objectMoveDrag = null;
  drag.preview.remove();
  event.preventDefault();
  if (drag.moved) {
    const EMU_PER_TWIP = 635;
    runEdit(
      () => doc.setObjectAnchorPosition(drag.node, drag.lastX * EMU_PER_TWIP, drag.lastY * EMU_PER_TWIP),
      { gate: true },
    );
  } else {
    drawSelection();
  }
  return true;
}

// Arrow-nudge step in twips: a fine ~1/32in step, and a coarse ~1/8in step with
// Shift (matching Word/Docs arrow-vs-Shift+arrow nudging).
const NUDGE_TWIP = 45;
const NUDGE_TWIP_LARGE = 180;

/** Nudges the selected floating object by one step in the given direction, as a
 *  single `SetAnchor` op (gated in Viewing/Suggesting like a drag-move). */
function nudgeSelectedObject(dx, dy, large) {
  if (!doc || !objectSelection || !objectSelection.anchored) return;
  const node = objectSelection.node;
  const rect = doc.objectRect(node); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
  const step = large ? NUDGE_TWIP_LARGE : NUDGE_TWIP;
  const nx = Math.max(0, rect[1] + dx * step);
  const ny = Math.max(0, rect[2] + dy * step);
  const EMU_PER_TWIP = 635;
  runEdit(() => doc.setObjectAnchorPosition(node, nx * EMU_PER_TWIP, ny * EMU_PER_TWIP), {
    gate: true,
  });
}

/** Aborts an in-progress float move, discarding the preview (Escape / cancel). */
function cancelObjectMove() {
  if (!objectMoveDrag) return;
  objectMoveDrag.preview.remove();
  objectMoveDrag = null;
  drawSelection();
}

/** Enters a container object's edit mode (docs/85 §4.3). A leaf object (image)
 *  has no edit mode — its primary context action is a later image slice. Placing
 *  a caret *inside* a text box's flowed body is the P1G-OBJ-TEXTBOX slice; here
 *  the grammar transitions state and the surrounding-text caret is shown. */
function enterObjectEditMode() {
  if (!objectSelection) return;
  if (objectSelection.kind !== "textbox") {
    setStatus("Image selected — drag its handles to resize", "", { timeout: 3000 });
    return;
  }
  objectSelection = { ...objectSelection, mode: "editing" };
  drawSelection();
}

/** Paints a clickable target over each checklist item's checkbox marker (docs/67
 *  — checklist authoring). The checkbox glyph itself is baked into the page raster
 *  by the layout engine; this overlay is the *model-as-truth* click target
 *  (`doc.checklistMarkers()` gives each marker's engine rect + node + state) that
 *  toggles the item's checked state via one edit op, then re-renders. Like the
 *  review markers it never mutates the canvas; unlike them it carries its own
 *  handler (the checkbox is a control, not passive text). Gated through
 *  `runNodeEdit` so it is blocked in Viewing and Suggesting, consistent with the
 *  other list edits. */
function paintChecklistMarkers() {
  if (!doc) return;
  let markers = [];
  try { markers = JSON.parse(doc.checklistMarkers()) ?? []; } catch { markers = []; }
  for (const marker of markers) {
    const el = place([marker.page, marker.x, marker.y, marker.w, marker.h], "checklist-marker");
    if (!el) continue;
    el.classList.toggle("is-checked", !!marker.checked);
    el.dataset.checklistNode = marker.node;
    el.setAttribute("role", "checkbox");
    el.setAttribute("aria-checked", String(!!marker.checked));
    el.setAttribute("aria-label", marker.checked ? "Checked item" : "Unchecked item");
    el.title = marker.checked ? "Checked — click to uncheck" : "Unchecked — click to check";
    el.addEventListener("pointerdown", (event) => {
      // Own the gesture: toggle the item instead of placing a caret in the gutter.
      event.preventDefault();
      event.stopPropagation();
      // Put the caret at the item so the mutation path has a selection and typing
      // continues in that item afterward; then toggle (gated for Viewing/Suggesting).
      selection = {
        anchor: { node: marker.node, offset: 0 },
        focus: { node: marker.node, offset: 0 },
      };
      // Checklist creation already provisions the symbol fallback that covers
      // both the checked and unchecked glyphs. Starting another asynchronous
      // coverage/render pass here races the edit repaint (and, when the edit is
      // blocked, can erase its read-only feedback), so the toggle uses only the
      // synchronous dirty-page repaint owned by `runNodeEdit`.
      runNodeEdit(() => doc.toggleChecklistItem(marker.node));
      focusEditorSurface();
    });
  }
}

/** Paints the caret or highlight for `sel` from engine geometry. */
function paintSelection({ anchor, focus }) {
  const collapsed = anchor.node === focus.node && anchor.offset === focus.offset;
  if (!collapsed) {
    const rects = doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
    if (rects.length >= 5) {
      for (let i = 0; i < rects.length; i += 5) place(rects.slice(i, i + 5), "highlight");
      return;
    }
    // No visible rects (e.g. a tiny drag within one caret slot) → fall to caret.
  }
  place(doc.caretRect(focus.node, focus.offset), "caret");
}

/** The live IME composition overlay element, or `null` when no composition
 *  is in progress. Kept across `compositionupdate` calls so its text can be
 *  updated in place instead of recreated every keystroke. */
let imePreeditEl = null;

/** Shows the IME live-preedit overlay at a caret anchor: the in-progress
 * composition text the browser has not yet committed. Never touches the
 * document — `compositionend` still owns the actual insertion
 * (`commitComposedText`) — this only makes the intermediate state visible
 * (docs/67-EDITOR-UX-GAP-ANALYSIS.md, "IME live preedit"). */
function showImePreedit(node, offset, text) {
  hideImePreedit();
  const flat = doc?.caretRect(node, offset) ?? [];
  if (flat.length < 5) return;
  const [pageNumber, x, y, , h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return;
  const { sx, sy } = scaleOf(page);
  const el = document.createElement("div");
  el.className = "ime-preedit";
  el.style.left = `${x * sx}px`;
  el.style.top = `${y * sy}px`;
  el.style.height = `${h * sy}px`;
  el.textContent = text;
  page.overlay.appendChild(el);
  imePreeditEl = el;
}

/** Updates the live-preedit overlay's text (a `compositionupdate` tick). */
function updateImePreedit(text) {
  if (imePreeditEl) imePreeditEl.textContent = text;
}

/** Removes the live-preedit overlay, if shown. */
function hideImePreedit() {
  imePreeditEl?.remove();
  imePreeditEl = null;
}

/** Places one flat `[page, x, y, w, h]` twip rect as a `kind` box on its page,
 *  converting twips → CSS px with that page's live scale. */
function place(flat, kind) {
  if (flat.length < 5) return null;
  const [pageNumber, x, y, w, h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const { sx, sy } = scaleOf(page);
  const el = document.createElement("div");
  el.className = kind;
  el.style.left = `${x * sx}px`;
  el.style.top = `${y * sy}px`;
  el.style.width = `${Math.max(w * sx, kind === "caret" ? 2 : 0)}px`;
  el.style.height = `${h * sy}px`;
  page.overlay.appendChild(el);
  return el;
}

/** Navigates to an internal-link target and makes the target page/caret visible. */
function navigateToAnchor(node, offset, pageNumber) {
  if (!node) return;
  pendingFormat = null;
  selection = {
    anchor: { node, offset },
    focus: { node, offset },
  };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView("center");
}

/** Copies a WASM-owned link hit into an ordinary JS value, then frees it. */
function linkAt(page, event) {
  if (!doc) return false;
  const { x, y } = pointToTwip(page, event);
  const hit = doc.linkAt(page.pageNumber, x, y);
  if (!hit) return null;
  const link = {
    kind: hit.kind,
    url: hit.url,
    anchor: hit.anchor,
    tooltip: hit.tooltip,
    startNode: hit.startNode,
    startOffset: hit.startOffset,
    endNode: hit.endNode,
    endOffset: hit.endOffset,
    targetNode: hit.targetNode,
    targetOffset: hit.targetOffset,
    targetPage: hit.targetPage,
  };
  hit.free();
  return link;
}

/** Clears the visible chip without changing the document selection. */
function hideLinkChip() {
  activeLink = null;
  linkChip.hidden = true;
}

/** Clears pointer feedback from every rendered page. */
function clearLinkHover() {
  pendingLinkHover = null;
  if (linkHoverFrame) cancelAnimationFrame(linkHoverFrame);
  linkHoverFrame = 0;
  for (const page of pages) page.canvas?.classList.remove("link-hover");
}

/** Throttles the model query used to make canvas-painted links visibly hoverable. */
function scheduleLinkHover(page, event) {
  pendingLinkHover = { page, clientX: event.clientX, clientY: event.clientY };
  if (linkHoverFrame) return;
  linkHoverFrame = requestAnimationFrame(() => {
    linkHoverFrame = 0;
    const pending = pendingLinkHover;
    pendingLinkHover = null;
    if (!pending || dragging || !pages.includes(pending.page)) return;
    const hit = linkAt(pending.page, pending);
    for (const candidate of pages) {
      candidate.canvas?.classList.toggle("link-hover", candidate === pending.page && !!hit);
    }
  });
}

/** Shows a bounded link chip and selects the exact authored model range, making
 * the target discoverable without hijacking drag or Shift-selection behavior. */
function showLinkChipAt(page, event) {
  const link = linkAt(page, event);
  if (!link) {
    hideLinkChip();
    return false;
  }
  activeLink = link;
  pendingFormat = null;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();

  const internal = link.kind === "internal";
  const resolved = !internal || (!!link.targetNode && !!link.targetPage);
  const target = internal ? `#${link.anchor}` : link.url;
  linkChipKind.textContent =
    link.tooltip || (internal ? "Document bookmark" : "External link");
  linkChipTarget.textContent = target;
  linkChipTarget.title = target;
linkChipAction.textContent = internal ? (resolved ? "Jump" : "Missing") : "Open";
linkChipAction.disabled = !resolved;
linkChipEdit.hidden = internal;
linkChipRemove.hidden = internal;
  linkChip.hidden = false;

  const width = linkChip.offsetWidth;
  const height = linkChip.offsetHeight;
  const left = Math.max(12, Math.min(event.clientX - 18, window.innerWidth - width - 12));
  let top = event.clientY + 14;
  if (top + height > window.innerHeight - 12) top = event.clientY - height - 14;
  linkChip.style.left = `${Math.round(left)}px`;
  linkChip.style.top = `${Math.max(12, Math.round(top))}px`;
  return true;
}

/** Activates a previously queried authored link. The runtime resolves
 * geometry/bookmarks; this host owns the external-scheme allowlist and browser
 * navigation. */
function activateLink(link) {
  if (!link) return false;
  hideLinkChip();

  if (link.kind === "internal") {
    if (!link.targetNode || !link.targetPage) {
      setStatus(`Bookmark “${link.anchor}” was not found`, "error");
      return true;
    }
    navigateToAnchor(link.targetNode, link.targetOffset, link.targetPage);
    setStatus(`Jumped to ${link.anchor}`);
    return true;
  }

  let target;
  try {
    target = new URL(link.url, window.location.href);
  } catch {
    setStatus("Blocked an invalid link target", "error");
    return true;
  }
  if (target.protocol === "http:" || target.protocol === "https:") {
    window.open(target.href, "_blank", "noopener,noreferrer");
  } else if (target.protocol === "mailto:") {
    window.location.assign(target.href);
  } else {
    setStatus(`Blocked ${target.protocol || "unknown"} link scheme`, "error");
  }
  return true;
}

function onPointerDown(page, event) {
  if (event.button !== 0) return;
  // A pointerdown that reaches the page during a crop (a crop handle's own
  // pointerdown stops propagation) is a click-away → commit the crop, exactly as
  // Word/Docs do. This click is consumed by the commit; the next click interacts.
  if (objectCropSession) {
    event.preventDefault();
    commitCrop();
    return;
  }
  focusEditorSurface();
  hideLinkChip();
  clearLinkHover();
  pointerGesture = null;
  // Object selection (docs/85 §3.1) takes precedence over a text caret: a click
  // on a drawing/image/text box selects it as a unit and shows its handles.
  const { x, y } = pointToTwip(page, event);
  const object = doc.objectAt(page.pageNumber, x, y);
  if (object) {
    const node = object.node;
    const kind = object.kind;
    const anchored = object.anchored;
    object.free?.();
    // A caret at the nearest text slot is the object's surrounding-text anchor
    // (for the two-step Escape); fall back to the current caret.
    const anchor = anchorAt(page, event) || selection?.focus || null;
    selectObject(node, kind, anchor, anchored);
    // A floating object is movable: the same gesture that selects it can drag it
    // (a bare click commits nothing). Inline objects flow with the text.
    if (anchored) startObjectMove(event, page, node);
    else startSelectionAutoScroll();
    event.preventDefault();
    return;
  }
  // A click that is not on an object deselects any selected object and proceeds
  // with ordinary text hit-testing.
  objectSelection = null;
  clearObjectStatus();
  const anchor = anchorAt(page, event);
  if (!anchor) {
    updateObjectSelectionState();
    updateObjectContextBar();
    return;
  }
  pendingFormat = null; // a click moves the caret → disarm typing format
  tableSelection = null;
  dragging = true;
  pointerGesture = {
    page,
    clientX: event.clientX,
    clientY: event.clientY,
    lastClientX: event.clientX,
    lastClientY: event.clientY,
    moved: false,
    shift: event.shiftKey,
  };
  // Shift+Click extends the current selection to the click (keeps the anchor).
  selection =
    event.shiftKey && selection
      ? { anchor: selection.anchor, focus: anchor }
      : { anchor, focus: anchor };
  // Non-blocking secondary effect (REVIEW-GAP-005): surface the sidebar card
  // for a comment the click landed inside, without altering the caret/range
  // hit-testing just computed above.
  syncActiveReviewCommentToCaret(anchor);
  drawSelection();
  startSelectionAutoScroll();
  event.preventDefault();
}

function startTableColumnResize(event, page, node, col) {
  if (!doc || !selection) return;
  event.preventDefault();
  event.stopPropagation();
  focusEditorSurface();
  hideLinkChip();
  hideContextMenu();
  clearLinkHover();
  resetPointerGesture();
  const startWidthTwips = doc.tableColumnWidthAt(node, col);
  if (startWidthTwips <= 0) return;
  const preview = document.createElement("div");
  preview.className = "table-col-resize-preview";
  preview.style.left = event.currentTarget.style.left;
  preview.style.top = "0";
  preview.style.height = `${page.overlay.clientHeight}px`;
  page.overlay.appendChild(preview);
  tableResizeDrag = {
    node,
    col,
    page,
    startClientX: event.clientX,
    startWidthTwips,
    preview,
    lastWidthTwips: startWidthTwips,
  };
  event.currentTarget.setPointerCapture?.(event.pointerId);
}

function cancelTableColumnResize() {
  if (!tableResizeDrag) return;
  tableResizeDrag.preview.remove();
  tableResizeDrag = null;
}

/** Aborts an in-progress object resize (pointer cancel / window blur), discarding
 *  the preview and committing nothing. */
function cancelObjectResize() {
  if (!objectResizeDrag) return;
  objectResizeDrag.preview.remove();
  objectResizeDrag = null;
  drawSelection();
}

function updateTableColumnResize(event) {
  if (!tableResizeDrag) return;
  const { sx } = scaleOf(tableResizeDrag.page);
  const deltaTwips = Math.round((event.clientX - tableResizeDrag.startClientX) / sx);
  const widthTwips = Math.max(72, tableResizeDrag.startWidthTwips + deltaTwips);
  tableResizeDrag.lastWidthTwips = widthTwips;
  const deltaPx = deltaTwips * sx;
  tableResizeDrag.preview.style.transform = `translateX(${deltaPx}px)`;
  event.preventDefault();
}

function finishTableColumnResize(event) {
  if (!tableResizeDrag) return false;
  const drag = tableResizeDrag;
  tableResizeDrag = null;
  drag.preview.remove();
  event.preventDefault();
  if (Math.abs(drag.lastWidthTwips - drag.startWidthTwips) >= 8) {
    runEdit(() => doc.setTableColumnWidthAt(drag.node, drag.col, drag.lastWidthTwips), { gate: true });
  } else {
    drawSelection();
  }
  return true;
}

function onPointerMove(page, event) {
  if (objectMoveDrag) {
    updateObjectMove(event);
    return;
  }
  if (objectResizeDrag) {
    updateObjectResize(event);
    return;
  }
  if (tableResizeDrag) {
    updateTableColumnResize(event);
    return;
  }
  if (dragging && event.buttons === 0) {
    resetPointerGesture();
    return;
  }
  if (!dragging) {
    scheduleLinkHover(page, event);
    return;
  }
  updateDragSelection(event);
}

function updateDragSelection(event) {
  if (!dragging || !pointerGesture || !selection) return;
  pointerGesture.lastClientX = event.clientX;
  pointerGesture.lastClientY = event.clientY;
  if (
    pointerGesture &&
    Math.hypot(
      event.clientX - pointerGesture.clientX,
      event.clientY - pointerGesture.clientY,
    ) > 4
  ) {
    pointerGesture.moved = true;
  }
  const page = pageFromClientPoint(event.clientX, event.clientY);
  if (!page) return;
  const focus = anchorAt(page, event);
  if (!focus) return;
  selection = { anchor: selection.anchor, focus };
  drawSelection();
}

const AUTO_SCROLL_EDGE_PX = 56;
const AUTO_SCROLL_MAX_PX = 24;

function startSelectionAutoScroll() {
  if (selectionAutoScrollFrame) return;
  const tick = () => {
    selectionAutoScrollFrame = 0;
    if (!dragging || !pointerGesture) return;

    const rect = viewportEl.getBoundingClientRect();
    const y = pointerGesture.lastClientY;
    let dy = 0;
    if (y < rect.top + AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (rect.top + AUTO_SCROLL_EDGE_PX - y) / AUTO_SCROLL_EDGE_PX);
      dy = -Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    } else if (y > rect.bottom - AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (y - (rect.bottom - AUTO_SCROLL_EDGE_PX)) / AUTO_SCROLL_EDGE_PX);
      dy = Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    }

    if (dy !== 0) {
      const before = viewportEl.scrollTop;
      viewportEl.scrollTop = Math.max(0, before + dy);
      if (viewportEl.scrollTop !== before) {
        updateDragSelection(clientPointEvent(pointerGesture.lastClientX, pointerGesture.lastClientY));
      }
    }

    selectionAutoScrollFrame = requestAnimationFrame(tick);
  };
  selectionAutoScrollFrame = requestAnimationFrame(tick);
}

function onPointerUp(event) {
  if (finishObjectMove(event)) return;
  if (finishObjectResize(event)) return;
  if (finishTableColumnResize(event)) return;
  const gesture = pointerGesture;
  resetPointerGesture();
  // Format painter: this pointer gesture landed on the document, so consume it as
  // the paint target (a drag's range, or the word under a bare click) instead of
  // the normal caret/link-chip behavior.
  if (formatPainter && gesture) {
    void paintFormatFromGesture(gesture, event);
    return;
  }
  if (
    gesture &&
    !gesture.shift &&
    !gesture.moved &&
    Math.hypot(event.clientX - gesture.clientX, event.clientY - gesture.clientY) <= 4
  ) {
    const link = linkAt(gesture.page, event);
    if (link?.kind === "internal" && link.targetNode && link.targetPage) {
      activateLink(link);
    } else {
      showLinkChipAt(gesture.page, event);
    }
  }
}

/** Double-click selects the word under the pointer. */
function selectWord(page, event) {
  const a = anchorAt(page, event);
  if (!a) return;
  focusEditorSurface();
  const bounds = doc.wordAt(a.node, a.offset); // [start, end] or []
  if (bounds.length === 2) {
    selection = {
      anchor: { node: a.node, offset: bounds[0] },
      focus: { node: a.node, offset: bounds[1] },
    };
    drawSelection();
  }
}

function selectionText() {
  if (!selection) return;
  const { anchor, focus } = selection;
  return doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
}

/** The selection as clipboard HTML: the exact `copyRichRuns` JSON embedded as
 * a leading comment (a lossless internal round-trip marker) plus a visible
 * rendering built from the same runs (what an external app sees). `null` if
 * there's nothing to copy. */
function selectionRichHtml() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  const runsJson = doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset);
  const runs = JSON.parse(runsJson);
  // When the selection spans block structure the flat runs flatten — a table or
  // a list — carry a structured payload for internal OpenDoc-to-OpenDoc paste
  // (`{ blocks, runs }`: the flat runs ride along so a Suggesting-mode paste, or
  // a structured paste the engine declines, still has the rich-run fallback).
  const structured = doc.copyStructured(anchor.node, anchor.offset, focus.node, focus.offset);
  if (structured) {
    const blocks = JSON.parse(structured).blocks;
    return embedMarker(JSON.stringify({ blocks, runs })) + runsToHtml(runs);
  }
  if (!runs.length) return null;
  return embedMarker(runsJson) + runsToHtml(runs);
}

async function copySelection(event = null) {
  const text = selectionText();
  if (!text) return;
  const html = selectionRichHtml();
  if (event?.clipboardData) {
    event.preventDefault();
    event.clipboardData.setData("text/plain", text);
    if (html) event.clipboardData.setData("text/html", html);
    const n = text.length;
    setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
    return true;
  }
  try {
    if (html && window.ClipboardItem) {
      await navigator.clipboard.write([
        new ClipboardItem({
          "text/plain": new Blob([text], { type: "text/plain" }),
          "text/html": new Blob([html], { type: "text/html" }),
        }),
      ]);
    } else {
      await navigator.clipboard.writeText(text);
    }
    const n = text.length;
    setStatus(`Copied ${n} character${n === 1 ? "" : "s"}`);
    return true;
  } catch (err) {
    console.warn("clipboard write failed:", err);
    setStatus("Clipboard write was blocked by the browser", "err");
    return false;
  }
}

// Delegated pointer handling: resolve which page the event is over.
function pageFromEvent(event) {
  const wrap = event.target.closest?.(".page-wrap");
  if (!wrap) return null;
  // Index among page wraps only — `pagesEl` also holds the ruler as a child, so
  // indexing over all children would be off by the ruler's slot (dead clicks).
  const idx = [...pagesEl.querySelectorAll(".page-wrap")].indexOf(wrap);
  return pages[idx] ?? null;
}

function pageFromClientPoint(clientX, clientY) {
  const target = document.elementFromPoint(clientX, clientY);
  const direct = target ? pageFromEvent({ target }) : null;
  if (direct) return direct;
  if (!pages.length) return null;

  let best = null;
  let bestDistance = Infinity;
  for (const page of pages) {
    const rect = page.wrap.getBoundingClientRect();
    const dx = clientX < rect.left ? rect.left - clientX : clientX > rect.right ? clientX - rect.right : 0;
    const dy = clientY < rect.top ? rect.top - clientY : clientY > rect.bottom ? clientY - rect.bottom : 0;
    const dist = dx * dx + dy * dy;
    if (dist < bestDistance) {
      bestDistance = dist;
      best = page;
    }
  }
  return best;
}
pagesEl.addEventListener("pointerdown", (e) => {
  // Ambiguous page-gap clicks must not jump into the nearest table or other
  // fragment. Only a hit inside a concrete page may place the caret; drag
  // continuation still uses nearest-page resolution once a gesture exists.
  const page = pageFromEvent(e);
  if (page) onPointerDown(page, e);
});
pagesEl.addEventListener("pointermove", (e) => {
  const page = pageFromEvent(e);
  if (page && !dragging) onPointerMove(page, e);
});
window.addEventListener("pointermove", (e) => {
  if (objectMoveDrag) {
    updateObjectMove(e);
    return;
  }
  if (objectResizeDrag) {
    updateObjectResize(e);
    return;
  }
  if (tableResizeDrag) {
    updateTableColumnResize(e);
    return;
  }
  if (dragging) {
    if (e.buttons === 0) resetPointerGesture();
    else updateDragSelection(e);
  }
});
pagesEl.addEventListener("pointerleave", clearLinkHover);
pagesEl.addEventListener("dblclick", (e) => {
  const page = pageFromEvent(e);
  if (!page) return;
  // Double-click on an object enters its edit mode (container) or selects it
  // (leaf) — docs/85 §4.3; otherwise it selects the word under the caret.
  const { x, y } = pointToTwip(page, e);
  const object = doc?.objectAt(page.pageNumber, x, y);
  if (object) {
    const node = object.node;
    const kind = object.kind;
    object.free?.();
    focusEditorSurface();
    if (!objectSelection || objectSelection.node !== node) {
      selectObject(node, kind, anchorAt(page, e) || selection?.focus || null);
    }
    enterObjectEditMode();
    e.preventDefault();
    return;
  }
  selectWord(page, e);
});
// Triple-click selects the paragraph (the click's `detail` is the click count).
pagesEl.addEventListener("click", (e) => {
  if (e.detail !== 3) return;
  const page = pageFromEvent(e);
  if (!page) return;
  const a = anchorAt(page, e);
  if (!a) return;
  focusEditorSurface();
  selection = {
    anchor: { node: a.node, offset: 0 },
    focus: { node: a.node, offset: doc.paragraphLength(a.node) },
  };
  drawSelection();
});
window.addEventListener("pointerup", onPointerUp);
window.addEventListener("pointercancel", () => {
  cancelObjectMove();
  cancelObjectResize();
  cancelTableColumnResize();
  resetPointerGesture();
});
window.addEventListener("lostpointercapture", () => {
  cancelObjectMove();
  cancelObjectResize();
  cancelTableColumnResize();
  resetPointerGesture();
});
window.addEventListener("blur", () => {
  cancelObjectMove();
  cancelObjectResize();
  cancelTableColumnResize();
  resetPointerGesture();
});
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    cancelObjectMove();
    cancelObjectResize();
    cancelTableColumnResize();
    resetPointerGesture();
  }
});

linkChip.addEventListener("mousedown", (event) => {
  // Keep the model selection visible while the host control receives the click.
  event.preventDefault();
});
linkChipAction.addEventListener("click", () => activateLink(activeLink));
linkChipEdit.addEventListener("click", () => {
  if (!activeLink || !selection || activeLink.startNode !== activeLink.endNode) return;
  const link = activeLink;
  const text = doc.copyText(link.startNode, link.startOffset, link.endNode, link.endOffset);
  hideLinkChip(); // the model range stays selected; the dialog owns it now
  openLinkDialog({ node: link.startNode, start: link.startOffset, end: link.endOffset, link, text });
});
linkChipRemove.addEventListener("click", () => {
  if (!activeLink || !selection || activeLink.startNode !== activeLink.endNode) return;
  runToolbarEdit(() => doc.removeHyperlink(activeLink.startNode, activeLink.startOffset, activeLink.endOffset));
  hideLinkChip();
});
document.addEventListener("pointerdown", (event) => {
  if (!linkChip.hidden && !linkChip.contains(event.target)) hideLinkChip();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    hideLinkChip();
    closeReviewPopover();
    closeReviewInlineCard();
  }
});
// Dismiss the inline accept/reject card on any pointerdown outside it and its
// originating marker (the card's own buttons stopPropagation, so they are not
// treated as "outside").
document.addEventListener("pointerdown", (event) => {
  if (!reviewInlineCard) return;
  if (reviewInlineCard.contains(event.target)) return;
  if (event.target.closest?.("[data-review-revision-id]")) return;
  closeReviewInlineCard();
});
document.addEventListener("keydown", (event) => {
  if (!doc || event.defaultPrevented) return;
  const mod = event.metaKey || event.ctrlKey;
  if (mod && event.altKey && event.key.toLowerCase() === "m") {
    event.preventDefault();
    openReviewComposer();
  } else if (mod && event.shiftKey && event.key.toLowerCase() === "e") {
    event.preventDefault();
    // Cycle Editing → Suggesting → Viewing → Editing for keyboard access to
    // all three modes (REVIEW-GAP-014).
    const next =
      reviewMode === "editing" ? "suggesting" : reviewMode === "suggesting" ? "viewing" : "editing";
    setReviewMode(next);
  } else if (mod && event.altKey && event.key === "Enter" && !isInteractiveChromeTarget(event.target)) {
    // Word's Accept ▸ Next (⌘/Ctrl+Alt+Enter): decide the change at the caret and
    // advance to the next one (Q3). `stopImmediatePropagation` keeps the canvas
    // editor's own Enter handling from also firing.
    event.preventDefault();
    event.stopImmediatePropagation();
    void decideReviewAndAdvance(true);
  } else if (
    mod && event.altKey && (event.key === "Backspace" || event.key === "Delete")
    && !isInteractiveChromeTarget(event.target)
  ) {
    // Reject ▸ Next (⌘/Ctrl+Alt+Backspace).
    event.preventDefault();
    event.stopImmediatePropagation();
    void decideReviewAndAdvance(false);
  }
});
viewportEl.addEventListener("scroll", hideLinkChip, { passive: true });
window.addEventListener("resize", hideLinkChip);

// ---- Context-aware editor menu ---------------------------------------------
// One surface serves prose, links, review ranges, lists, and tables. Commands
// call the same transaction-backed actions as the ribbon and palette.
const editorContextMenu = document.createElement("div");
editorContextMenu.className = "context-menu editor-context-menu";
editorContextMenu.hidden = true;
editorContextMenu.setAttribute("role", "menu");
editorContextMenu.setAttribute("aria-label", "Editor commands");
document.body.appendChild(editorContextMenu);
// Open menus form a stack: index 0 is the root context menu, deeper indexes are
// nested submenu flyouts. `keyboardLevelIndex` marks which level the arrow keys
// currently drive; hovering a submenu opens a deeper level visually without
// stealing keyboard control until the user presses ArrowRight/Enter.
let menuLevels = [];
let keyboardLevelIndex = 0;
let contextMenuReturnFocus = null;

function selectionContainsClientPoint(clientX, clientY) {
  if (!doc || !hasRange()) return false;
  const rects = doc.selectionRects(
    selection.anchor.node,
    selection.anchor.offset,
    selection.focus.node,
    selection.focus.offset,
  );
  for (let i = 0; i + 4 < rects.length; i += 5) {
    const [pageNumber, x, y, width, height] = rects.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { rect, sx, sy } = scaleOf(page);
    if (
      clientX >= rect.left + x * sx &&
      clientX <= rect.left + (x + width) * sx &&
      clientY >= rect.top + y * sy &&
      clientY <= rect.top + (y + height) * sy
    ) {
      return true;
    }
  }
  return false;
}

function tableSelectionContainsClientPoint(clientX, clientY) {
  if (!doc || !tableSelection) return false;
  const rects = doc.tableSelectionRects(
    tableSelection.node,
    tableSelection.mode,
  );
  for (let i = 0; i + 4 < rects.length; i += 5) {
    const [pageNumber, x, y, width, height] = rects.slice(i, i + 5);
    const page = pages[pageNumber - 1];
    if (!page) continue;
    const { rect, sx, sy } = scaleOf(page);
    if (
      clientX >= rect.left + x * sx &&
      clientX <= rect.left + (x + width) * sx &&
      clientY >= rect.top + y * sy &&
      clientY <= rect.top + (y + height) * sy
    ) {
      return true;
    }
  }
  return false;
}

function anchorInsideRange(anchor, range) {
  return (
    anchor &&
    range &&
    range.startNode === anchor.node &&
    range.endNode === anchor.node &&
    anchor.offset >= range.startOffset &&
    anchor.offset <= range.endOffset
  );
}

function reviewContextAt(anchor) {
  if (!doc || !anchor) return { comment: null, revision: null };
  const summary = readReviewData(doc);
  const comment = (summary.comments ?? []).find((item) =>
    item.anchor?.node === anchor.node &&
    anchor.offset >= (Number(item.anchor.start) || 0) &&
    anchor.offset <= (Number(item.anchor.end) || Number(item.anchor.start) || 0),
  ) ?? null;
  const revision = (summary.revisions ?? []).find((item) =>
    anchorInsideRange(anchor, revisionRange(item)),
  ) ?? null;
  return { comment, revision };
}

function plainTableInfo(node) {
  if (!doc?.inTable(node)) return null;
  const info = doc.tableInfo(node);
  const value = info?.found ? {
    found: true,
    regular: info.regular,
    rowHeightRule: info.rowHeightRule,
    table: info.table,
    column: info.column,
  } : null;
  info?.free();
  return value;
}

// 0-based index of the column containing the caret, or -1 when the node is not
// inside a table. Used so table sort keys off the caret's own column (Docs/Word
// standard) rather than always the first column.
function caretTableColumn(node) {
  if (!doc?.inTable(node)) return -1;
  const info = doc.tableInfo(node);
  const column = info?.found ? info.column : -1;
  info?.free();
  return column;
}

function contextAt(anchor, link = null) {
  const review = reviewContextAt(anchor);
  return {
    surface: "context",
    anchor,
    link,
    comment: review.comment,
    revision: review.revision,
    table: plainTableInfo(anchor.node),
    listKind: doc.listStyleAt(anchor.node),
    hasRange: hasRange(),
    sameParagraphRange:
      hasRange() && selection.anchor.node === selection.focus.node,
    suggesting: reviewMode === "suggesting",
  };
}

function decideContextRevision(revision, accept) {
  if (!revision) return;
  return runEdit(() =>
    revision.movePair?.fromStart && revision.movePair?.toStart
      ? doc.decideMovePair(
        revision.movePair.fromStart,
        revision.movePair.toStart,
        accept,
      )
      : revision.groupId
        ? doc.decideRevisionGroup(revision.groupId, accept)
        : doc.decideRevision(revision.id, accept),
  );
}

function selectTableContext(node, mode) {
  tableSelection = { node, mode };
  drawSelection();
  setStatus(`Selected table ${mode}`);
  focusEditorSurface();
}

function editContextLink(link) {
  if (!link || link.startNode !== link.endNode) return;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();
  const text = doc.copyText(link.startNode, link.startOffset, link.endNode, link.endOffset);
  openLinkDialog({ node: link.startNode, start: link.startOffset, end: link.endOffset, link, text });
}

function removeContextLink(link) {
  if (!link || link.startNode !== link.endNode) return;
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();
  runToolbarEdit(() =>
    doc.removeHyperlink(link.startNode, link.startOffset, link.endOffset),
  );
}

function openContextLink(link) {
  if (!link) return;
  if (link.url) {
    window.open(link.url, "_blank", "noopener");
    return;
  }
  if (link.targetNode != null) {
    selection = {
      anchor: { node: link.targetNode, offset: link.targetOffset ?? 0 },
      focus: { node: link.targetNode, offset: link.targetOffset ?? 0 },
    };
    drawSelection();
    scrollCaretIntoView("center");
    focusEditorSurface();
  }
}

// Builds the right-click menu as a short, grouped, contextual set. Primary
// actions (clipboard, link, comment, review decisions) stay at the top level;
// the long tail (text styling, list/indent, table row/column operations) is
// tucked into submenus so no single target dumps a 30-row list. Every entry
// still routes through the same transaction-backed actions as the ribbon and
// command palette, so availability and mutation gates never drift.
function buildContextCommands(context) {
  // Right-clicking a selected drawing/image/text box shows OBJECT commands, not
  // the paragraph-text menu (docs/85 §4.1; Word/Google Docs image menu). The
  // object was already selected by the handler that resolved this context.
  if (context.surface === "object") return buildObjectContextCommands(context);
  const registry = new Map(editorCommands(context).map((command) => [command.id, command]));
  const pick = (id, extra = {}) => {
    const base = registry.get(id);
    return base ? { ...base, ...extra } : null;
  };
  const structuralEnabled = !context.suggesting;
  const structuralReason = structuralEnabled
    ? ""
    : "This structural change cannot be tracked in Suggesting mode";
  const inTable = !!context.table;
  const commands = [];

  // 1 — Clipboard: the universal primary actions, with leading icons.
  commands.push(
    pick("edit.cut", { icon: "cut", group: "clipboard" }),
    pick("edit.copy", { icon: "copy", group: "clipboard" }),
    pick("edit.paste", { icon: "paste", group: "clipboard" }),
  );

  // 2 — Review decisions: only over a tracked change, kept near the top since
  // they are the most specific thing a right-click can land on.
  if (context.revision) {
    commands.push(
      {
        id: "review.accept",
        label: "Accept suggestion",
        group: "review",
        icon: "accept",
        run: () => decideContextRevision(context.revision, true),
      },
      {
        id: "review.reject",
        label: "Reject suggestion",
        group: "review",
        icon: "reject",
        danger: true,
        run: () => decideContextRevision(context.revision, false),
      },
    );
  }

  // Annotations (link + comment) — contextual to the text/selection under the
  // pointer. Assembled here, placed after the primary group for prose and after
  // the table tools inside a cell.
  const annotate = [];
  if (context.link) {
    const linkReason = context.suggesting
      ? "Link changes cannot be tracked in Suggesting mode"
      : "";
    if (context.link.url || context.link.targetNode != null) {
      annotate.push({
        id: "link.open",
        label: "Open link",
        group: "annotate",
        icon: "linkOpen",
        run: () => openContextLink(context.link),
      });
    }
    annotate.push(
      {
        id: "link.edit",
        label: "Edit link…",
        group: "annotate",
        icon: "link",
        enabled: !context.suggesting,
        disabledReason: linkReason,
        run: () => editContextLink(context.link),
      },
      {
        id: "link.remove",
        label: "Remove link",
        group: "annotate",
        enabled: !context.suggesting,
        disabledReason: linkReason,
        run: () => removeContextLink(context.link),
      },
    );
  } else {
    annotate.push({
      id: "link.add",
      label: "Add link…",
      group: "annotate",
      icon: "link",
      shortcut: "⌘K",
      enabled: context.sameParagraphRange && !context.suggesting,
      disabledReason: context.suggesting
        ? "Link changes cannot be tracked in Suggesting mode"
        : context.hasRange
          ? "Links must stay within one paragraph"
          : "Select text to add a link",
      run: () => editSelectionLink(),
    });
  }
  if (context.comment) {
    annotate.push({
      id: "comment.open",
      label: "Open comment",
      group: "annotate",
      icon: "comment",
      run: () => focusReviewComment(context.comment),
    });
  } else {
    annotate.push({
      id: "comment.add",
      label: "Add comment",
      group: "annotate",
      icon: "comment",
      shortcut: "⌘⌥M",
      enabled: context.hasRange,
      disabledReason: "Select text to add a comment",
      run: () => openReviewComposer(),
    });
  }

  // Text styling, list/indentation, and the paragraph dialog. Shared building
  // blocks: on prose they are three top-level rows; inside a table cell they
  // collapse into a single trailing "Format ▸" submenu so the table actions
  // lead and the cell menu stays compact.
  const formatSubmenu = [
    pick("format.bold", { group: "style" }),
    pick("format.italic", { group: "style" }),
    pick("format.underline", { group: "style" }),
    pick("format.strike", { group: "style" }),
    pick("format.superscript", { group: "script" }),
    pick("format.subscript", { group: "script" }),
    pick("format.clear", { group: "clear" }),
  ].filter(Boolean);
  const listSubmenu = [
    {
      id: "paragraph.bullets",
      label: context.listKind === "bullet" ? "Remove bullets" : "Bulleted list",
      group: "list",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet")),
    },
    {
      id: "paragraph.numbering",
      label: context.listKind === "numbered" ? "Remove numbering" : "Numbered list",
      group: "list",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered")),
    },
    {
      id: "paragraph.restart",
      label: "Restart numbering",
      group: "list",
      visible: context.listKind === "numbered",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runNodeEdit(() => doc.restartList(context.anchor.node)),
    },
    {
      id: "paragraph.continue",
      label: "Continue numbering",
      group: "list",
      visible: context.listKind === "numbered" && doc.canContinueList(context.anchor.node),
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runNodeEdit(() => doc.continueList(context.anchor.node)),
    },
    {
      id: "paragraph.indent.increase",
      label: "Increase indent",
      group: "indent",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => adjustIndentCommand(360),
    },
    {
      id: "paragraph.indent.decrease",
      label: "Decrease indent",
      group: "indent",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => adjustIndentCommand(-360),
    },
  ];
  const paragraphProperties = {
    id: "paragraph.properties",
    label: "Paragraph properties…",
    enabled: structuralEnabled,
    disabledReason: structuralReason,
    run: () => toggleParagraphProperties(true),
  };

  if (!inTable) {
    // Prose menu: annotations, then the text-arrangement rows.
    commands.push(...annotate);
    commands.push({
      id: "format.menu",
      label: "Format text",
      group: "arrange",
      icon: "format",
      submenu: formatSubmenu,
    });
    commands.push({
      id: "paragraph.list",
      label: "List & indentation",
      group: "arrange",
      icon: "list",
      submenu: listSubmenu,
    });
    commands.push({ ...paragraphProperties, group: "arrange", icon: "paragraph" });
    return commands.filter(Boolean);
  }

  // 3 — Table cell: lead with the table tools (Insert / Delete / Merge / Split,
  // then Select / Autofit & sort, then the property dialogs), matching Word's
  // and Google Docs' table menus. The generic text-format rows are demoted to a
  // single trailing "Format ▸" submenu.
  const regular = context.table.regular;
  const selectedTable = tableSelection
    ? plainTableInfo(tableSelection.node)?.table
    : "";
  const hasTableSelection =
    !!selectedTable && selectedTable === context.table.table;
  const columnsReason = regular
    ? structuralReason
    : "Unavailable for merged or spanned tables";
  const tableMutation = (id, label, run, options = {}) => ({
    id,
    label,
    group: options.group ?? "op",
    enabled:
      structuralEnabled &&
      (options.regular !== true || regular) &&
      (options.enabled ?? true),
    disabledReason:
      !structuralEnabled
        ? structuralReason
        : options.regular === true && !regular
          ? columnsReason
          : options.enabled === false
            ? options.disabledReason
            : "",
    danger: options.danger,
    run,
  });

  const insertSubmenu = [
    tableMutation("table.insert.rowAbove", "Row above",
      () => runEdit(() => doc.insertRow(context.anchor.node, false), { gate: true }),
      { group: "row" }),
    tableMutation("table.insert.rowBelow", "Row below",
      () => runEdit(() => doc.insertRow(context.anchor.node, true), { gate: true }),
      { group: "row" }),
    tableMutation("table.insert.columnLeft", "Column left",
      () => runEdit(() => doc.insertColumn(context.anchor.node, false), { gate: true }),
      { regular: true, group: "col" }),
    tableMutation("table.insert.columnRight", "Column right",
      () => runEdit(() => doc.insertColumn(context.anchor.node, true), { gate: true }),
      { regular: true, group: "col" }),
  ];
  const deleteSubmenu = [
    tableMutation("table.delete.row", "Delete row",
      () => runEdit(() => doc.deleteRow(context.anchor.node), { gate: true }),
      { danger: true, group: "cell" }),
    tableMutation("table.delete.column", "Delete column",
      () => runEdit(() => doc.deleteColumn(context.anchor.node), { gate: true }),
      { danger: true, regular: true, group: "cell" }),
    tableMutation("table.delete.table", "Delete table",
      () => runEdit(() => doc.deleteTable(context.anchor.node), { gate: true }),
      { danger: true, group: "table" }),
  ];
  const selectSubmenu = [
    {
      id: "table.select.row",
      label: "Select row",
      group: "sel",
      run: () => selectTableContext(context.anchor.node, "row"),
    },
    {
      id: "table.select.column",
      label: "Select column",
      group: "sel",
      enabled: regular,
      disabledReason: regular ? "" : columnsReason,
      run: () => selectTableContext(context.anchor.node, "column"),
    },
    {
      id: "table.select.table",
      label: "Select table",
      group: "sel",
      run: () => selectTableContext(context.anchor.node, "table"),
    },
  ];
  const layoutSubmenu = [
    tableMutation("table.distribute.rows", "Distribute rows",
      () => runEdit(() => doc.distributeTableRows(context.anchor.node), { gate: true }),
      {
        regular: true,
        group: "distribute",
        enabled: ["exact", "atLeast"].includes(context.table.rowHeightRule),
        disabledReason: "Rows need a fixed or minimum height before distribution",
      }),
    tableMutation("table.distribute.columns", "Distribute columns",
      () => runEdit(() => doc.distributeTableColumns(context.anchor.node), { gate: true }),
      { regular: true, group: "distribute" }),
    tableMutation("table.sort.ascending", "Sort ascending",
      () => runEdit(() => doc.sortTable(context.anchor.node, "ascending", context.table?.column ?? -1), { gate: true }),
      { regular: true, group: "sort" }),
    tableMutation("table.sort.descending", "Sort descending",
      () => runEdit(() => doc.sortTable(context.anchor.node, "descending", context.table?.column ?? -1), { gate: true }),
      { regular: true, group: "sort" }),
  ];

  commands.push(
    {
      id: "table.insert",
      label: "Insert",
      group: "table",
      icon: "tableInsert",
      submenu: insertSubmenu,
    },
    {
      id: "table.delete",
      label: "Delete",
      group: "table",
      icon: "tableDelete",
      submenu: deleteSubmenu,
    },
    tableMutation("table.merge", "Merge cells",
      async () => {
        await runEdit(() =>
          doc.mergeTableSelection(tableSelection.node, tableSelection.mode), { gate: true });
        tableSelection = null;
      },
      {
        group: "table",
        enabled: hasTableSelection,
        disabledReason: "Select a row, column, or table before merging",
      }),
    tableMutation("table.split", "Split cell…",
      () => toggleSplitCellDialog(true),
      { group: "table" }),
    {
      id: "table.select",
      label: "Select",
      group: "table-select",
      icon: "tableSelect",
      submenu: selectSubmenu,
    },
    {
      id: "table.layout",
      label: "Autofit & sort",
      group: "table-select",
      icon: "tableLayout",
      submenu: layoutSubmenu,
    },
    {
      id: "table.cellFormat",
      label: "Cell formatting…",
      group: "table-properties",
      icon: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => {
        selectRibbonTab("table");
        tableBtn.click();
      },
    },
    {
      id: "table.properties",
      label: "Table properties…",
      group: "table-properties",
      icon: "settings",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => toggleTableProperties(true),
    },
  );

  // Annotations sit below the table tools, then the demoted text-format submenu.
  commands.push(...annotate);
  commands.push({
    id: "format.menu",
    label: "Format",
    group: "format",
    icon: "format",
    submenu: [
      ...formatSubmenu.map((entry) => ({ ...entry, group: "type" })),
      ...listSubmenu.map((entry) => ({ ...entry, group: "list" })),
      { ...paragraphProperties, group: "para" },
    ],
  });
  return commands.filter(Boolean);
}

// Builds the right-click menu for a selected object (image / text box). This is
// the object counterpart to `buildContextCommands`: it emits object commands
// (Wrap / Alt text / Crop / Delete) instead of paragraph-text ones, reusing the
// exact same functions the floating object context bar wires up. Mutations are
// disabled — with the object review-mode reason — in Viewing and Suggesting,
// mirroring how the text menu greys its structural rows; the underlying
// functions still gate fail-closed, so the menu can never bypass a review mode.
function buildObjectContextCommands(context) {
  const isPicture = context.kind !== "textbox";
  // Object edits are untrackable, so they are read-only in Viewing and blocked
  // (untracked) in Suggesting — the same gate `runEdit({ gate:true })` applies.
  const mutationEnabled = reviewMode === "editing";
  const mutationReason =
    reviewMode === "viewing"
      ? "Turn on Editing to change this object"
      : "Object changes cannot be tracked in Suggesting mode";
  const commands = [];

  // Wrap text — a submenu of wrap modes, only for a floating (anchored) object,
  // exactly like the context bar. The active mode is checked on the right.
  if (context.anchored) {
    const active = doc.objectWrap(context.node);
    commands.push({
      id: "object.wrap",
      label: "Wrap text",
      group: "arrange",
      icon: "wrap",
      submenu: WRAP_MODES.map(([value, text]) => ({
        id: `object.wrap.${value}`,
        label: text,
        group: "wrap",
        shortcut: value === active ? "✓" : "",
        enabled: mutationEnabled,
        disabledReason: mutationReason,
        run: () => setObjectWrap(value),
      })),
    });
  }

  // Alt text — opens the shared alt-text dialog (its Apply pre-checks the gate).
  commands.push({
    id: "object.altText",
    label: "Alt text…",
    group: "arrange",
    icon: "altText",
    enabled: mutationEnabled,
    disabledReason: mutationReason,
    run: () => openAltTextDialog(),
  });

  // Crop — picture-only; a text box has no source rectangle to crop.
  if (isPicture) {
    commands.push({
      id: "object.crop",
      label: "Crop image",
      group: "arrange",
      icon: "crop",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => enterCropMode(),
    });
  }

  // Delete — the destructive action, kept in its own trailing group.
  commands.push({
    id: "object.delete",
    label: "Delete",
    group: "delete",
    icon: "delete",
    danger: true,
    enabled: mutationEnabled,
    disabledReason: mutationReason,
    run: () => deleteSelectedObject(),
  });
  return commands;
}

// Resolves a pointer event to an OBJECT context (or null). Prefers a fresh
// object hit-test at the point; falls back to the already-selected object when
// the click lands on it (e.g. on a resize handle the hit-test skips). The
// caller selects the object before showing the menu.
function objectContextAtEvent(page, event) {
  const { x, y } = pointToTwip(page, event);
  const object = doc.objectAt(page.pageNumber, x, y);
  if (object) {
    const ctx = {
      surface: "object",
      node: object.node,
      kind: object.kind,
      anchored: object.anchored,
    };
    object.free?.();
    return ctx;
  }
  // No fresh hit, but an object is selected and the point is inside its box.
  if (objectSelection && objectSelection.mode === "selected") {
    const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h]
    if (
      rect.length >= 5 &&
      rect[0] === page.pageNumber &&
      x >= rect[1] &&
      x <= rect[1] + rect[3] &&
      y >= rect[2] &&
      y <= rect[2] + rect[4]
    ) {
      return {
        surface: "object",
        node: objectSelection.node,
        kind: objectSelection.kind,
        anchored: objectSelection.anchored,
      };
    }
  }
  return null;
}

// ---- Menu rendering engine (root context menu + nested submenu flyouts) -----
// Compact, currentColor stroke icons for primary rows and submenu parents. The
// icon gutter is always reserved so labels align whether or not a row has one —
// the same alignment Word and Google Docs use.
const menuIcon = (inner) =>
  `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${inner}</svg>`;
const MENU_ICONS = {
  cut: menuIcon('<circle cx="4" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><path d="M5.3 10.7 13 3M10.7 10.7 3 3"/>'),
  copy: menuIcon('<rect x="5.5" y="5.5" width="8" height="8" rx="1.4"/><path d="M10.5 5.5V3.4A1.4 1.4 0 0 0 9.1 2H3.4A1.4 1.4 0 0 0 2 3.4v5.7A1.4 1.4 0 0 0 3.4 10.5h2.1"/>'),
  paste: menuIcon('<rect x="3.5" y="3" width="9" height="11" rx="1.4"/><path d="M6 3.2V2.4A1.1 1.1 0 0 1 7.1 1.3h1.8A1.1 1.1 0 0 1 10 2.4v.8z"/>'),
  link: menuIcon('<path d="M6.7 9.3 9.3 6.7"/><path d="M7.2 4.6 8.4 3.4a2.4 2.4 0 0 1 3.4 3.4L10.6 8"/><path d="M8.8 11.4 7.6 12.6a2.4 2.4 0 0 1-3.4-3.4L5.4 8"/>'),
  linkOpen: menuIcon('<path d="M9 3h4v4"/><path d="M13 3 7.5 8.5"/><path d="M11 9.5V12a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 12V6.5A1.5 1.5 0 0 1 4 5h2.5"/>'),
  comment: menuIcon('<path d="M2.5 3.5h11a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H7l-3 2.3V10.5H2.5a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z"/>'),
  accept: menuIcon('<path d="M3 8.5 6.3 12 13 4"/>'),
  reject: menuIcon('<path d="M4 4 12 12M12 4 4 12"/>'),
  format: menuIcon('<path d="M4 12.5 7.5 3.5h1L12 12.5M5.4 9.5h5.2"/>'),
  list: menuIcon('<path d="M6 4h8M6 8h8M6 12h8"/><path d="M2.7 4h.01M2.7 8h.01M2.7 12h.01"/>'),
  paragraph: menuIcon('<path d="M8.5 2.5H12M8.5 6H12M4 9.5H12M4 13H12M5 6.5A2.2 2.2 0 0 1 5 2.5h1.5v4"/>'),
  settings: menuIcon('<circle cx="8" cy="8" r="1.9"/><path d="M8 1.7v1.8M8 12.5v1.8M2.4 5.5l1.6.9M12 9.6l1.6.9M2.4 10.5l1.6-.9M12 6.4l1.6-.9"/>'),
  tableInsert: menuIcon('<path d="M2.5 6h7M2.5 10h4M6 2.5v8"/><path d="M11.5 8.5v5M9 11h5"/>'),
  tableDelete: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M6.2 6.2 9.8 9.8M9.8 6.2 6.2 9.8"/>'),
  tableSelect: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M2.5 6.5h11M6.5 2.5v11"/>'),
  tableLayout: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1"/><path d="M2.5 8h11M8 2.5v11"/>'),
  wrap: menuIcon('<rect x="2.5" y="3" width="6" height="6" rx="1"/><path d="M10.5 4h3M10.5 7h3M2.5 11.5h11M2.5 13.5h11"/>'),
  altText: menuIcon('<rect x="2.5" y="2.5" width="11" height="11" rx="1.4"/><path d="M5 10.5 7 5l2 5.5M5.6 9h2.8"/><path d="M10.5 5v5.5"/>'),
  crop: menuIcon('<path d="M4.5 1.5v10a1 1 0 0 0 1 1h9M1.5 4.5h10a1 1 0 0 1 1 1v9"/>'),
  delete: menuIcon('<path d="M3 4.5h10M6.5 4.5V3a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v1.5M5 4.5l.6 8a1 1 0 0 0 1 .95h2.8a1 1 0 0 0 1-.95l.6-8"/>'),
};

function menuLevelItems(level) {
  return [...level.el.querySelectorAll(":scope > .menu-item")];
}

function menuItemAt(level, index) {
  return menuLevelItems(level).find(
    (item) => Number(item.dataset.menuIndex) === index,
  ) ?? null;
}

function activeMenuLevel() {
  return menuLevels[keyboardLevelIndex] ?? null;
}

function focusMenuIndex(level, index, focus = true) {
  level.index = index;
  for (const item of menuLevelItems(level)) {
    const active = Number(item.dataset.menuIndex) === index;
    item.tabIndex = active ? 0 : -1;
    item.classList.toggle("active", active);
    if (active && focus) {
      item.focus({ preventScroll: true });
      item.scrollIntoView({ block: "nearest" });
    }
  }
}

// Removes every open level deeper than `depth`, releasing submenu DOM nodes and
// resetting the parent's expanded state.
function closeMenuLevelsAbove(depth) {
  while (menuLevels.length > depth + 1) {
    const level = menuLevels.pop();
    level.parentButton?.setAttribute("aria-expanded", "false");
    if (level.el !== editorContextMenu) level.el.remove();
  }
  if (keyboardLevelIndex > menuLevels.length - 1) {
    keyboardLevelIndex = Math.max(0, menuLevels.length - 1);
  }
}

function renderMenuLevel(el, entries, depth) {
  el.replaceChildren();
  entries.forEach((entry, index) => {
    if (entry.separator) {
      const sep = document.createElement("div");
      sep.className = "menu-divider";
      sep.setAttribute("role", "separator");
      el.appendChild(sep);
      return;
    }
    const hasSub = Array.isArray(entry.submenu);
    const button = document.createElement("button");
    button.type = "button";
    button.className =
      `menu-item${entry.danger ? " danger" : ""}${hasSub ? " has-submenu" : ""}`;
    button.dataset.menuIndex = String(index);
    button.dataset.commandId = entry.id;
    button.setAttribute("role", "menuitem");
    button.disabled = entry.enabled === false;
    button.tabIndex = -1;
    if (hasSub) {
      button.setAttribute("aria-haspopup", "menu");
      button.setAttribute("aria-expanded", "false");
    }
    if (entry.disabledReason) button.title = entry.disabledReason;
    const icon = document.createElement("span");
    icon.className = "menu-item-icon";
    if (entry.icon && MENU_ICONS[entry.icon]) icon.innerHTML = MENU_ICONS[entry.icon];
    button.appendChild(icon);
    const label = document.createElement("span");
    label.className = "menu-item-label";
    label.textContent = entry.label;
    button.appendChild(label);
    if (hasSub) {
      const caret = document.createElement("span");
      caret.className = "menu-item-caret";
      caret.textContent = "›";
      button.appendChild(caret);
    } else if (entry.shortcut) {
      // Only the keyboard shortcut is ever shown on the right. Disabled rows are
      // greyed in place with no reason text (Google Docs convention) so the menu
      // never widens to fit an explanation; the reason stays as a hover title.
      const hint = document.createElement("span");
      hint.className = "menu-item-hint";
      hint.textContent = entry.shortcut;
      button.appendChild(hint);
    }
    button.addEventListener("mousemove", () => {
      if (button.disabled) return;
      const level = menuLevels[depth];
      if (!level) return;
      keyboardLevelIndex = depth;
      focusMenuIndex(level, index, false);
      if (hasSub) openSubmenu(depth, button, entry);
      else closeMenuLevelsAbove(depth);
    });
    button.addEventListener("click", (event) => {
      if (hasSub) {
        event.stopPropagation();
        openSubmenu(depth, button, entry, true);
        return;
      }
      runMenuEntry(entry);
    });
    el.appendChild(button);
  });
}

// Opens (or re-focuses) the flyout for a submenu-parent button. Prefers opening
// to the right of the parent and flips left near the viewport edge.
function openSubmenu(parentDepth, button, entry, viaKeyboard = false) {
  const depth = parentDepth + 1;
  const existing = menuLevels[depth];
  if (existing && existing.parentButton === button) {
    if (viaKeyboard) {
      keyboardLevelIndex = depth;
      focusMenuIndex(existing, moveMenuIndex(existing.entries, -1, 1));
    }
    return;
  }
  closeMenuLevelsAbove(parentDepth);
  button.setAttribute("aria-expanded", "true");
  const el = document.createElement("div");
  el.className = "context-menu editor-submenu";
  el.setAttribute("role", "menu");
  el.setAttribute("aria-label", entry.label);
  el.hidden = true;
  document.body.appendChild(el);
  const entries = normalizeMenuEntries(entry.submenu);
  const level = { el, entries, index: -1, parentButton: button };
  renderMenuLevel(el, entries, depth);
  el.hidden = false;
  const rect = button.getBoundingClientRect();
  const width = el.offsetWidth;
  const height = el.offsetHeight;
  let left = rect.right - 4;
  if (left + width > window.innerWidth - 8) left = rect.left - width + 4;
  left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
  let top = Math.max(8, Math.min(rect.top - 5, window.innerHeight - height - 8));
  el.style.left = `${left}px`;
  el.style.top = `${top}px`;
  menuLevels[depth] = level;
  if (viaKeyboard) {
    keyboardLevelIndex = depth;
    focusMenuIndex(level, moveMenuIndex(entries, -1, 1));
  }
}

function stepToParentLevel() {
  const parentDepth = keyboardLevelIndex - 1;
  const parent = menuLevels[parentDepth];
  closeMenuLevelsAbove(parentDepth);
  keyboardLevelIndex = parentDepth;
  if (parent) {
    focusMenuIndex(
      parent,
      parent.index >= 0 ? parent.index : moveMenuIndex(parent.entries, -1, 1),
    );
  }
}

function hideContextMenu({ restoreFocus = false } = {}) {
  if (editorContextMenu.hidden && menuLevels.length === 0) return;
  for (const level of menuLevels) {
    if (level.el === editorContextMenu) level.el.replaceChildren();
    else level.el.remove();
  }
  menuLevels = [];
  keyboardLevelIndex = 0;
  editorContextMenu.hidden = true;
  if (restoreFocus) {
    const target = contextMenuReturnFocus?.isConnected ? contextMenuReturnFocus : pagesEl;
    target.focus({ preventScroll: true });
  }
  contextMenuReturnFocus = null;
}

function runMenuEntry(entry) {
  if (!entry || entry.separator || entry.enabled === false || entry.submenu) return;
  hideContextMenu({ restoreFocus: true });
  entry.run();
}

function showContextMenu(clientX, clientY, context) {
  hideContextMenu();
  contextMenuReturnFocus =
    document.activeElement instanceof HTMLElement ? document.activeElement : pagesEl;
  const entries = normalizeMenuEntries(buildContextCommands(context));
  editorContextMenu.hidden = false;
  renderMenuLevel(editorContextMenu, entries, 0);
  const position = clampContextMenuPosition(
    clientX,
    clientY,
    editorContextMenu.offsetWidth,
    editorContextMenu.offsetHeight,
    window.innerWidth,
    window.innerHeight,
  );
  editorContextMenu.style.left = `${position.left}px`;
  editorContextMenu.style.top = `${position.top}px`;
  menuLevels = [{ el: editorContextMenu, entries, index: -1, parentButton: null }];
  keyboardLevelIndex = 0;
  focusMenuIndex(menuLevels[0], moveMenuIndex(entries, -1, 1));
}

function keyboardContextMenuPoint() {
  if (!selection || !doc) return null;
  const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
  if (flat.length < 5) return null;
  const [pageNumber, x, y, width, height] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return null;
  const { rect, sx, sy } = scaleOf(page);
  return {
    x: rect.left + (x + width) * sx,
    y: rect.top + (y + height) * sy,
    page,
  };
}

pagesEl.addEventListener("contextmenu", (event) => {
  const page = pageFromEvent(event);
  if (!page || !doc) return;
  // Object hit-test takes precedence (docs/85 §3.1): right-clicking a drawing /
  // image / text box selects it as a unit and shows its OBJECT menu, not the
  // paragraph-text menu.
  const objectContext = objectContextAtEvent(page, event);
  if (objectContext) {
    event.preventDefault();
    selectObject(
      objectContext.node,
      objectContext.kind,
      anchorAt(page, event) || selection?.focus || null,
      objectContext.anchored,
    );
    showContextMenu(event.clientX, event.clientY, objectContext);
    return;
  }
  const anchor = anchorAt(page, event);
  if (!anchor) return;
  event.preventDefault();
  const preserveSelection = selectionContainsClientPoint(
    event.clientX,
    event.clientY,
  ) || tableSelectionContainsClientPoint(event.clientX, event.clientY);
  if (!preserveSelection) {
    selection = { anchor, focus: anchor };
    tableSelection = null;
    drawSelection();
  }
  showContextMenu(
    event.clientX,
    event.clientY,
    contextAt(anchor, linkAt(page, event)),
  );
});

document.addEventListener("pointerdown", (event) => {
  if (editorContextMenu.hidden) return;
  if (menuLevels.some((level) => level.el.contains(event.target))) return;
  hideContextMenu();
});
document.addEventListener("keydown", (event) => {
  if (!editorContextMenu.hidden) {
    const level = activeMenuLevel();
    if (!level) return;
    const entry = level.entries[level.index];
    if (event.key === "Escape") {
      event.preventDefault();
      if (keyboardLevelIndex > 0) stepToParentLevel();
      else hideContextMenu({ restoreFocus: true });
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      closeMenuLevelsAbove(keyboardLevelIndex);
      focusMenuIndex(
        level,
        moveMenuIndex(level.entries, level.index, event.key === "ArrowDown" ? 1 : -1),
      );
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      closeMenuLevelsAbove(keyboardLevelIndex);
      focusMenuIndex(
        level,
        moveMenuIndex(level.entries, level.index, event.key === "Home" ? "first" : "last"),
      );
    } else if (event.key === "ArrowRight") {
      if (entry && entry.submenu && entry.enabled !== false) {
        event.preventDefault();
        openSubmenu(keyboardLevelIndex, menuItemAt(level, level.index), entry, true);
      }
    } else if (event.key === "ArrowLeft") {
      if (keyboardLevelIndex > 0) {
        event.preventDefault();
        stepToParentLevel();
      }
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (entry && entry.submenu) {
        openSubmenu(keyboardLevelIndex, menuItemAt(level, level.index), entry, true);
      } else {
        runMenuEntry(entry);
      }
    }
    return;
  }
  if (
    doc &&
    eventTargetsEditor(event) &&
    ((event.shiftKey && event.key === "F10") || event.key === "ContextMenu")
  ) {
    // A selected object opens its OBJECT menu, anchored to the object's top-left
    // (matching how the object context bar is positioned).
    if (objectSelection && objectSelection.mode === "selected") {
      const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h]
      const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
      if (!page) return;
      event.preventDefault();
      const { rect: pageRect, sx, sy } = scaleOf(page);
      showContextMenu(pageRect.left + rect[1] * sx, pageRect.top + rect[2] * sy, {
        surface: "object",
        node: objectSelection.node,
        kind: objectSelection.kind,
        anchored: objectSelection.anchored,
      });
      return;
    }
    if (!selection) return;
    const point = keyboardContextMenuPoint();
    if (!point) return;
    event.preventDefault();
    const anchor = selection.focus;
    const link = linkAt(
      point.page,
      clientPointEvent(point.x, Math.max(point.y - 1, 0)),
    );
    showContextMenu(point.x, point.y, contextAt(anchor, link));
  }
});
viewportEl.addEventListener("scroll", () => hideContextMenu(), { passive: true });
window.addEventListener("resize", () => hideContextMenu());

// ---- Horizontal ruler (margins + the caret paragraph's indent markers) -------
const ruler = document.createElement("div");
ruler.className = "ruler";
ruler.hidden = true;
const rulerTrack = document.createElement("div");
rulerTrack.className = "ruler-track";
ruler.appendChild(rulerTrack);

let rulerGeom = null; // { widthTwip, marginStartTwip, marginEndTwip }
let rulerScale = 0; // px per twip at the current zoom
const markers = {}; // key -> element
const TWIPS_PER_INCH = 1440;
let tabInsertCode = 0; // the type new ruler tabs get: 0 L, 1 C, 2 R, 3 decimal
const TAB_LETTER = ["L", "C", "R", "."];

/** Rebuilds the ruler scale, margin zones, and ticks for the current page/zoom. */
function buildRuler() {
  if (!doc || !pages.length) {
    ruler.hidden = true;
    return;
  }
  const g = doc.pageGeometry();
  rulerGeom = {
    width: g.widthTwip,
    marginStart: g.marginStartTwip,
    marginEnd: g.marginEndTwip,
  };
  const pageWidthPx = pages[0].wrap.getBoundingClientRect().width;
  rulerScale = pageWidthPx / rulerGeom.width;
  ruler.style.width = `${pageWidthPx}px`;
  const px = (t) => t * rulerScale;

  rulerTrack.replaceChildren();
  const contentStart = rulerGeom.marginStart;

  // The white content span between the (shaded) page margins. Clicking it adds a
  // tab stop at that position (in the current tab type) on the caret paragraph.
  const content = document.createElement("div");
  content.className = "ruler-content";
  content.style.left = `${px(rulerGeom.marginStart)}px`;
  content.style.width = `${px(rulerGeom.width - rulerGeom.marginStart - rulerGeom.marginEnd)}px`;
  content.addEventListener("pointerdown", (e) => {
    if (!doc || !selection || e.button !== 0) return;
    const pos = Math.max(0, Math.round(e.offsetX / rulerScale));
    e.preventDefault();
    e.stopPropagation();
    runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, tabInsertCode));
    updateRulerMarkers();
  });
  rulerTrack.appendChild(content);

  // Word-style tab-type selector at the ruler's left edge; click to cycle L/C/R/dot.
  const corner = document.createElement("button");
  corner.type = "button";
  corner.className = "tab-corner";
  corner.title = "Tab stop type — click to change";
  corner.textContent = TAB_LETTER[tabInsertCode];
  corner.addEventListener("click", () => {
    tabInsertCode = (tabInsertCode + 1) % TAB_LETTER.length;
    corner.textContent = TAB_LETTER[tabInsertCode];
  });
  rulerTrack.appendChild(corner);

  // Minor ticks every 1/8", plus a numbered major tick at each inch measured from
  // the left margin (0 at the content edge).
  for (let t = 0; t <= rulerGeom.width; t += TWIPS_PER_INCH / 8) {
    const tick = document.createElement("div");
    tick.className = "ruler-tick minor";
    tick.style.left = `${px(t)}px`;
    rulerTrack.appendChild(tick);
  }
  for (let i = 0, t = contentStart; t <= rulerGeom.width + 1; i++, t = contentStart + i * TWIPS_PER_INCH) {
    const tick = document.createElement("div");
    tick.className = "ruler-tick major";
    tick.style.left = `${px(t)}px`;
    rulerTrack.appendChild(tick);
    if (i > 0) {
      const num = document.createElement("div");
      num.className = "ruler-num";
      num.textContent = String(i);
      num.style.left = `${px(t)}px`;
      rulerTrack.appendChild(num);
    }
  }

  // Indent markers (recreated each build; positioned by the selection). Only the
  // markers are pointer-interactive; the rest of the ruler is click-through, so a
  // marker drag can never steal a page click.
  for (const [key, cls] of [
    ["firstLine", "down"],
    ["left", "up"],
    ["right", "up"],
  ]) {
    const m = document.createElement("div");
    m.className = `ruler-marker ${cls}`;
    m.dataset.marker = key;
    m.title =
      key === "firstLine" ? "First-line indent" : key === "left" ? "Left indent" : "Right indent";
    m.addEventListener("pointerdown", (e) => startMarkerDrag(key, e));
    rulerTrack.appendChild(m);
    markers[key] = m;
  }

  ruler.hidden = false;
  updateRulerMarkers();
}

/** Positions the three indent markers from the caret paragraph's indentation. */
function updateRulerMarkers() {
  if (!rulerGeom || !markers.left) return;
  const px = (t) => t * rulerScale;
  let start = 0;
  let end = 0;
  let firstLine = 0;
  if (doc && selection) {
    const ind = doc.paragraphIndent(selection.focus.node);
    start = ind.startTwip;
    end = ind.endTwip;
    firstLine = ind.firstLineTwip - ind.hangingTwip;
    ind.free();
  }
  const contentStart = rulerGeom.marginStart;
  const contentEnd = rulerGeom.width - rulerGeom.marginEnd;
  markers.left.style.left = `${px(contentStart + start)}px`;
  markers.firstLine.style.left = `${px(contentStart + start + firstLine)}px`;
  markers.right.style.left = `${px(contentEnd - end)}px`;
  renderTabStops();
}

/** Draws the caret paragraph's tab stops as glyphs on the ruler (recreated each
 *  update). Each glyph: click cycles its type, drag moves it, drag off removes it. */
function renderTabStops() {
  for (const g of rulerTrack.querySelectorAll(".tab-glyph")) g.remove();
  if (!doc || !selection || !rulerGeom) return;
  const px = (t) => t * rulerScale;
  const tabs = doc.paragraphTabs(selection.focus.node); // flat [pos, code, …]
  for (let k = 0; k < tabs.length; k += 2) {
    const pos = tabs[k];
    const code = tabs[k + 1];
    const g = document.createElement("div");
    g.className = `tab-glyph tab-${code}`;
    g.textContent = TAB_LETTER[code] ?? "L";
    g.style.left = `${px(rulerGeom.marginStart + pos)}px`;
    g.title = "Tab stop — click to change type, drag to move, drag off to remove";
    g.addEventListener("pointerdown", (e) => startTabDrag(pos, code, g, e));
    rulerTrack.appendChild(g);
  }
}

/** A tab-glyph pointer interaction: no move → cycle type; horizontal move →
 *  reposition; released off the ruler → delete (Word's drag-off-to-remove). */
function startTabDrag(pos, code, glyph, ev) {
  if (!doc || !selection || ev.button !== 0) return;
  ev.preventDefault();
  ev.stopPropagation();
  const trackRect = rulerTrack.getBoundingClientRect();
  const px = (t) => t * rulerScale;
  let moved = false;
  let curPos = pos;
  const onMove = (e) => {
    if (Math.abs(e.clientX - ev.clientX) > 3 || Math.abs(e.clientY - ev.clientY) > 3) moved = true;
    curPos = Math.max(0, Math.round((e.clientX - trackRect.left) / rulerScale - rulerGeom.marginStart));
    glyph.style.left = `${px(rulerGeom.marginStart + curPos)}px`; // live
  };
  const onUp = (e) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    const offRuler = e.clientY > trackRect.bottom + 14 || e.clientY < trackRect.top - 14;
    if (offRuler) {
      runToolbarEdit((a, b, c, d) => doc.removeTabStop(a, b, c, d, pos));
    } else if (moved && curPos !== pos) {
      runToolbarEdit((a, b, c, d) => doc.moveTabStop(a, b, c, d, pos, curPos));
    } else {
      runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, (code + 1) % TAB_LETTER.length));
    }
    updateRulerMarkers();
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

/** Drag an indent marker. Uses window-level move/up listeners (never
 *  setPointerCapture) so the pointer is always released — the ruler acts on the
 *  caret paragraph, so a selection is required. */
function startMarkerDrag(key, ev) {
  if (!doc || !selection || !rulerGeom) return;
  ev.preventDefault();
  ev.stopPropagation(); // don't let the pointerdown fall through to the page

  const trackRect = rulerTrack.getBoundingClientRect();
  const px = (t) => t * rulerScale;
  const contentStart = rulerGeom.marginStart;
  const contentEnd = rulerGeom.width - rulerGeom.marginEnd;

  // The left marker carries the first-line marker with it (Word/Docs behaviour);
  // capture the current first-line offset so it is preserved during the drag.
  const ind = doc.paragraphIndent(selection.focus.node);
  const startTwip = ind.startTwip;
  const firstLineOff = ind.firstLineTwip - ind.hangingTwip;
  ind.free();

  const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);
  const xTwipAt = (clientX) => clamp((clientX - trackRect.left) / rulerScale, 0, rulerGeom.width);

  // Live visual feedback while dragging (model is committed on pointerup).
  const preview = (x) => {
    if (key === "left") {
      markers.left.style.left = `${px(x)}px`;
      markers.firstLine.style.left = `${px(x + firstLineOff)}px`;
    } else {
      markers[key].style.left = `${px(x)}px`;
    }
  };

  // Resolve the marker's ruler x to an absolute indent for the WASM setter.
  const commit = async (x) => {
    let call;
    if (key === "left") {
      const twips = Math.round(x - contentStart);
      call = (sn, so, en, eo) => doc.setLeftIndent(sn, so, en, eo, twips);
    } else if (key === "firstLine") {
      const twips = Math.round(x - contentStart - startTwip);
      call = (sn, so, en, eo) => doc.setFirstLineIndent(sn, so, en, eo, twips);
    } else {
      const twips = Math.round(contentEnd - x);
      call = (sn, so, en, eo) => doc.setRightIndent(sn, so, en, eo, twips);
    }
    await runToolbarEdit(call);
    updateRulerMarkers(); // snap to the model's clamped truth
  };

  markers[key].classList.add("dragging");
  const onMove = (e) => preview(xTwipAt(e.clientX));
  const onUp = (e) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    markers[key].classList.remove("dragging");
    commit(xTwipAt(e.clientX));
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

// ---- Editing (keys → semantic edits through the WASM choke point) ------------

/** Device DPI the pages are rastered at (HiDPI-crisp, DPR-capped for memory). */
function currentDpi() {
  return BASE_DPI * zoomFactor * backingDpr();
}

/** Whether the selection currently spans any text (a real range vs a caret). */
function hasRange() {
  return (
    selection &&
    (selection.anchor.node !== selection.focus.node || selection.anchor.offset !== selection.focus.offset)
  );
}

/** Re-raster a single page after an edit — the incremental repaint that keeps
 *  editing latency to one page, not the whole document. If the page is on screen
 *  it is re-rendered in place; if it is virtualized off-screen, its stale canvas
 *  is dropped so it re-renders fresh (from current model state) when scrolled in. */
function repaintPage(i) {
  const page = pages[i];
  if (!page) return;
  releasePageCanvas(page);
  if (page.visible) paintPageCanvas(page, i);
}

/** Scroll one engine-derived overlay marker in the editor viewport (not an
 * arbitrary page ancestor). The selection is painted before this runs, so its
 * DOM rectangle is only a projection of model geometry, never a source of
 * document state. */
function scrollOverlayIntoView(marker, block = "nearest") {
  if (!marker) return;
  const markerRect = marker.getBoundingClientRect();
  const viewportRect = viewportEl.getBoundingClientRect();
  const current = viewportEl.scrollTop;
  const max = Math.max(0, viewportEl.scrollHeight - viewportEl.clientHeight);
  let target = current;
  if (block === "center") {
    target = current + markerRect.top + markerRect.height / 2 - (viewportRect.top + viewportRect.height / 2);
  } else if (markerRect.top < viewportRect.top) {
    target = current + markerRect.top - viewportRect.top;
  } else if (markerRect.bottom > viewportRect.bottom) {
    target = current + markerRect.bottom - viewportRect.bottom;
  } else {
    return;
  }
  viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
}

/** Scroll the caret in the editor viewport. Navigation callers can request a
 * centered target so headings/anchors retain useful reading room. */
function scrollCaretIntoView(block = "nearest") {
  scrollOverlayIntoView(pagesEl.querySelector(".overlay .caret"), block);
}

/** Bring the current review selection's OWN on-canvas marker just into view when
 *  focusing a comment/change from the sidebar or Next/Previous. A review target
 *  is a range, so it paints a highlight (not a caret) — scrolling only `.caret`
 *  did nothing, leaving an off-screen item unreachable. And a "center" scroll
 *  overshot: for a clustered paragraph it recentred on the whole selection and
 *  pushed the very item the reviewer picked out of the viewport. "nearest" fixes
 *  both — an already-visible marker never moves (no overshoot), and an off-screen
 *  one is revealed by the minimum scroll to its own rect, not the paragraph's. */
function scrollReviewSelectionIntoView() {
  // Prefer the ACTIVE review item's own marker: in a clustered paragraph several
  // items paint highlights, and `.highlight` in DOM order can belong to an
  // earlier item — scrolling to it lands on the paragraph top, not the item the
  // reviewer selected. The active marker (painted from the item resolved in
  // `syncActiveReviewCommentToCaret`) is unambiguous; fall back to the selection
  // highlight, then the caret.
  const marker =
    pagesEl.querySelector(".overlay .review-comment-marker-active, .overlay .review-revision-marker-active")
    || pagesEl.querySelector(".overlay .highlight")
    || pagesEl.querySelector(".overlay .caret");
  scrollOverlayIntoView(marker, "nearest");
}

/** Find selects a real range, so paintSelection deliberately emits highlights
 * and no caret. Scroll its first rectangle into view; querying only `.caret`
 * made Previous/Next update the selection on an off-screen page without moving
 * the canvas. */
function scrollFindMatchIntoView() {
  scrollOverlayIntoView(pagesEl.querySelector(".overlay .highlight"), "center");
}

/** Apply an EditResult: place the caret, repaint only the dirty pages (or rebuild
 *  on a page-count change), redraw the caret, and keep it in view. */
async function applyEditResult(res) {
  const node = res.node;
  const offset = res.offset;
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  clearFindParagraphCache();
  selection = { anchor: { node, offset }, focus: { node, offset } };
  if (newCount !== pages.length) {
    await renderAll(); // structural change (page added/removed): rebuild the list
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ stats: true, outline: true });
  scrollCaretIntoView();
}

/** Ends the current typing gesture. The next printable key receives a fresh
 * session id and therefore cannot merge with earlier history. */
function breakTypingSession() {
  typingSessionActive = false;
  lastTypingAt = 0;
}

/** Returns the gesture id for an adjacent typing tick. A pause is a semantic
 * boundary even if the caret has not moved. */
function typingSessionForKey() {
  const now = performance.now();
  if (!typingSessionActive || now - lastTypingAt > TYPING_PAUSE_MS) {
    typingSession = (typingSession + 1) >>> 0;
    if (typingSession === 0) typingSession = 1;
  }
  typingSessionActive = true;
  lastTypingAt = now;
  return typingSession;
}

// Pointer interaction always establishes a new caret/selection/command gesture.
document.addEventListener("pointerdown", breakTypingSession, { capture: true });

/** True (after showing the standard status message and returning focus to the
 *  canvas) if Suggesting mode should block a command whose mutation cannot -
 *  yet or ever - be represented as a tracked revision. This is the single
 *  fail-closed gate shared by every mutation path (`runEdit`, `runNodeEdit`,
 *  `runToolbarEdit`): a command that bypasses tracking must never silently
 *  apply while the mode still reads Suggesting (REVIEW-GAP-004). */
function blockUntrackedInSuggesting() {
  if (reviewMode !== "suggesting") return false;
  setStatus("This command cannot be tracked yet; switch to Editing to apply it", "error");
  focusEditorSurface();
  return true;
}

/** True (after showing the read-only status message and returning focus to the
 *  canvas) if Viewing mode should block a document mutation. Viewing is fully
 *  read-only — no Operation reaches apply (docs/68 §"Suggesting mode") — so
 *  every mutation path (typing, deletion, paste, toolbar formatting, table
 *  ops, and comment/revision decisions) fails closed here rather than
 *  depending on any individual command or menu item being disabled
 *  (REVIEW-GAP-014). Navigation, selection, scroll, and copy are not
 *  mutations and are unaffected. */
function blockMutationInViewing() {
  if (reviewMode !== "viewing") return false;
  setStatus("Viewing mode is read-only; switch to Editing to change the document", "error");
  focusEditorSurface();
  return true;
}

/** Runs an edit thunk and applies its result; unsupported edits are ignored.
 *  `gate: true` marks a mutation that has no tracked-revision representation
 *  (yet, or ever, per REVIEW-GAP-009's structural backlog): it is blocked
 *  outright in Suggesting mode rather than silently applying untracked. */
async function runEdit(thunk, { typing = false, gate = false } = {}) {
  if (blockMutationInViewing()) return;
  if (!typing) breakTypingSession();
  if (gate && blockUntrackedInSuggesting()) return;
  let res;
  try {
    res = thunk();
  } catch (err) {
    if (typing) breakTypingSession();
    console.warn("edit ignored:", err?.message ?? err);
    // The engine's error names (Unsupported, CrossParagraph, …) are internal
    // vocabulary, not user-facing text — a bounded, generic message is enough
    // to stop this from reading as "nothing happened" (docs/67, "Error/
    // reporting UX": never silently do nothing).
    setStatus("That edit isn't supported for this selection yet", "error");
    return;
  }
  await applyEditResult(res);
  setDocumentState("edited");
}

/** Move the caret by arrow key. Shift extends (moves the focus); plain collapses. */
function navCaret(dir, extend) {
  if (!selection) return;
  if (objectCropSession) cancelCrop(); // arrow-navigating away discards the crop preview
  objectSelection = null; // moving the caret leaves any object selection
  clearObjectStatus();
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const collapseToStart = dir === "left" || dir === "wordLeft";
  const collapseToEnd = dir === "right" || dir === "wordRight";
  const c =
    !extend && hasRange() && (collapseToStart || collapseToEnd)
      ? doc.selectionEdge(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
          collapseToEnd,
        )
      : doc.moveCaret(selection.focus.node, selection.focus.offset, dir);
  const to = { node: c.node, offset: c.offset };
  c.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  scrollCaretIntoView();
}

// ---- Formatting toolbar (run + paragraph properties) -------------------------

/** The current selection endpoints as `[sNode, sOff, eNode, eOff]`, or null. */
function selEndpoints() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  return [anchor.node, anchor.offset, focus.node, focus.offset];
}

/** Runs a toolbar edit thunk `(sNode, sOff, eNode, eOff) => EditResult`,
 *  preserving the selection (formatting does not collapse it) and repainting
 *  only the dirty pages. */
async function runToolbarEdit(thunk, { allowInSuggesting = false } = {}) {
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (!allowInSuggesting && blockUntrackedInSuggesting()) return;
  const ends = selEndpoints();
  if (!ends) return;
  let res;
  try {
    res = thunk(...ends);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    setStatus("That tracked format is not supported for this selection yet", "error");
    return;
  }
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  if (newCount !== pages.length) await renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ outline: true });
  setDocumentState("edited");
}

/** The uniform run-format state over the selection, or null if not a range. */
function selectionFormat() {
  if (!doc || !hasRange()) return null;
  const [sn, so, en, eo] = selEndpoints();
  const f = doc.selectionFormat(sn, so, en, eo);
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
  if (!doc || !selection) return { bold: false, italic: false, underline: false, strike: false };
  const f = doc.caretFormat(selection.focus.node, selection.focus.offset);
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

/** Reflects an authored font family without restricting imported documents to
 * the toolbar's starter list. The temporary option is presentation-only: the
 * renderer's physical substitution/fallback family is never written back as the
 * document's requested font. */
let currentFontFamily = "";
function reflectFontFamily(family) {
  currentFontFamily = family || "";
  fontFamilyLabel.textContent = family || "Font";
  fontFamilyLabel.style.fontFamily = family ? `"${family}", system-ui, sans-serif` : "";
  fontFamilyBtn.classList.toggle("is-placeholder", !family);
  fontFamilyBtn.title = family ? `Font: ${family}` : "Font";
}

/** Toggles a run toggle (`bold`/`italic`/`underline`/`strike`). With a range it
 *  formats the selection; at a collapsed caret it arms the format for typing
 *  (premium editors: press Bold, then type — the text comes out bold). */
function toggleFormat(prop) {
  if (!hasRange()) {
    // Arm/disarm at the caret: flip the effective current value (pending overrides
    // the caret's inherited format), and reflect it in the toolbar.
    const current = pendingFormat?.[prop] ?? caretFormatState()[prop];
    pendingFormat = { ...(pendingFormat || {}), [prop]: !current };
    updateToolbar();
    return;
  }
  const state = selectionFormat();
  if (!state) return;
  const patch = {
    [prop]: !state[prop],
  };
  armOrApplyRun(patch, () =>
    runToolbarEdit((sn, so, en, eo) => doc.formatSelection(
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

/** Reflects the selection in the toolbar: active states + which controls are
 *  enabled (run controls need a text range; paragraph controls need a caret). */
function updateToolbar() {
  const hasSel = !!selection;
  const range = hasRange();
  const runState = selectionFormat();

  // B/I/U/S work with a range (format it) or a collapsed caret (arm it for typing),
  // so they are enabled whenever there is any selection. Pressed state: a range
  // reflects its uniform run format; a caret reflects the armed format (if any) over
  // the format new typing would inherit.
  const caretFmt = !range && hasSel ? caretFormatState() : null;
  for (const key of ["bold", "italic", "underline", "strike"]) {
    fmtButtons[key].disabled = !hasSel;
    const mixed = range && runState?.[`${key}State`] === 2;
    const pressed = range
      ? runState && runState[key]
      : (pendingFormat?.[key] ?? (caretFmt ? caretFmt[key] : false));
    fmtButtons[key].setAttribute("aria-pressed", mixed ? "mixed" : String(!!pressed));
  }
  // Run controls work with a range (apply) or a caret (arm for typing), so they are
  // enabled whenever there is a selection.
  for (const el of runControls) el.disabled = !hasSel;
  for (const el of paraControls) el.disabled = !hasSel;

  const align = hasSel && doc ? doc.alignmentAt(selection.focus.node, selection.focus.offset) : "start";
  for (const [key, btn] of Object.entries(alignBtns)) {
    btn.setAttribute("aria-pressed", String(key === align));
  }

  // Reflect the current run styling (size / font / color / super-sub) — over a
  // selection, or what a collapsed caret inherits (so it's "picked up" on click,
  // not only when text is selected).
  let size = "";
  let font = "";
  let sup = false;
  let sub = false;
  let sizeMixed = false;
  let fontMixed = false;
  let colorMixed = false;
  let highlight = "none";
  let highlightMixed = false;
  let verticalAlignMixed = false;
  if (doc && hasSel) {
    const rs = range
      ? doc.selectionRunStyle(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
        )
      : doc.caretRunStyle(selection.focus.node, selection.focus.offset);
    if (rs.sizePoints) size = String(rs.sizePoints);
    sizeMixed = rs.sizeMixed;
    font = rs.font;
    fontMixed = rs.fontMixed;
    if (rs.color) textColorInput.value = rs.color;
    colorMixed = rs.colorMixed;
    highlight = rs.highlight || "none";
    highlightMixed = rs.highlightMixed;
    sup = rs.superscript;
    sub = rs.subscript;
    verticalAlignMixed = rs.verticalAlignMixed;
    rs.free();
  }
  // An armed (pending) run format overrides the inherited value in the display.
  if (pendingFormat) {
    if (pendingFormat.sizeHalfPoints != null) size = String(pendingFormat.sizeHalfPoints / 2);
    if (pendingFormat.font != null) font = pendingFormat.font;
    if (pendingFormat.color) textColorInput.value = pendingFormat.color;
    if (pendingFormat.highlight != null) highlight = pendingFormat.highlight;
    if (pendingFormat.vertAlign != null) {
      sup = pendingFormat.vertAlign === "super";
      sub = pendingFormat.vertAlign === "sub";
    }
    sizeMixed = false;
    fontMixed = false;
    colorMixed = false;
    highlightMixed = false;
    verticalAlignMixed = false;
  }
  fontSizeSel.value = size;
  fontSizeSel.placeholder = sizeMixed ? "Mixed" : "Size";
  fontSizeSel.closest(".ctl")?.classList.toggle("is-mixed", sizeMixed);
  reflectFontFamily(fontMixed ? "" : font);
  fontFamilyBtn.closest(".ctl")?.classList.toggle("is-mixed", fontMixed);
  textColorCaret.closest(".ctl")?.classList.toggle("is-mixed", colorMixed);
  reflectTextColorSwatch(colorMixed ? null : (textColorInput.value || null));
  highlightCaret.closest(".ctl")?.classList.toggle("is-mixed", highlightMixed);
  reflectHighlightSwatch(highlightMixed ? null : highlight);
  superBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sup));
  subBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sub));

  // Reflect the current paragraph style + spacing + list kind.
  paragraphStyleSel.value = hasSel && doc ? doc.paragraphStyleAt(selection.focus.node) : "";
  if (hasSel && doc) for (const p of popovers) if (!p.menu.hidden) p.reflect();
  const listKind = hasSel && doc ? doc.listStyleAt(selection.focus.node) : "";
  bulletListBtn.setAttribute("aria-pressed", String(listKind === "bullet"));
  numberedListBtn.setAttribute("aria-pressed", String(listKind === "numbered"));
  checkListBtn.setAttribute("aria-pressed", String(listKind === "checklist"));
  restartListBtn.disabled = !hasSel || listKind !== "numbered";
  // Continue numbering is available only when the caret's numbered item has an
  // earlier numbered list at the same level to resume (the engine's own guard).
  continueListBtn.disabled =
    !hasSel || listKind !== "numbered" || !doc.canContinueList(selection.focus.node);
  // The contextual Table ribbon is enabled only inside a table; regular-grid
  // column commands stay unavailable on merged/spanned tables rather than
  // failing after the user clicks them.
  const inTable = hasSel && doc && doc.inTable(selection.focus.node);
  const tableInfo = inTable ? doc.tableInfo(selection.focus.node) : null;
  for (const control of tableRibbonControls) control.disabled = !inTable;
  tableStyleBtn.disabled = !inTable;
  const activeTableStyle = inTable && tableInfo?.found ? (doc.tableStyleAt?.(selection.focus.node) || "") : "";
  tableStyleBtn.title = activeTableStyle ? `Table style: ${activeTableStyle}` : "Choose table style";
  for (const control of tableRibbon.querySelectorAll(
    '[data-table-action*="column"]',
  )) {
    control.disabled = !inTable || !tableInfo?.regular;
  }
  for (const control of tableRibbon.querySelectorAll("[data-table-distribute]")) {
    control.disabled =
      !inTable ||
      !tableInfo?.regular ||
      (control.dataset.tableDistribute === "rows" &&
        !["exact", "atLeast"].includes(tableInfo.rowHeightRule));
  }
  for (const control of tableRibbon.querySelectorAll("[data-table-sort]")) {
    control.disabled = !inTable || !tableInfo?.regular;
  }
  mergeCellsBtn.disabled = !inTable || !tableSelection;
  tableContext.textContent = tableInfo?.found ? tableContextLabel(tableInfo) : "";
  if (!tablePropertiesPanel.hidden) {
    if (!tableInfo?.found) toggleTableProperties(false);
    else reflectTableProperties(selection.focus.node);
  }
  if (!paragraphPropertiesPanel.hidden) {
    if (!hasSel) toggleParagraphProperties(false);
    else reflectParagraphProperties();
  }
  tableInfo?.free();

  // Insert-table needs just a caret to drop the new table after.
  insertTableBtn.disabled = !(hasSel && doc);
  insertLinkBtn.disabled =
    !range || selection.anchor.node !== selection.focus.node;
  // Ribbon: undo/redo/view controls need a document; the Table tab is contextual.
  undoBtn.disabled = !doc || !doc.canUndo;
  redoBtn.disabled = !doc || !doc.canRedo;
  const undoLabel = doc?.undoLabel || "";
  const redoLabel = doc?.redoLabel || "";
  const undoName = undoLabel ? `Undo ${undoLabel}` : "Undo";
  const redoName = redoLabel ? `Redo ${redoLabel}` : "Redo";
  undoBtn.setAttribute("aria-label", undoName);
  redoBtn.setAttribute("aria-label", redoName);
  undoBtn.title = `${undoName} (⌘Z)`;
  redoBtn.title = `${redoName} (⌘⇧Z)`;
  findBtn.disabled = !doc;
  replaceBtn.disabled = !doc;
  // Clipboard buttons mirror the clipboard actions' own preconditions: copy/cut
  // need a range; paste needs a caret. The actions still fail closed in Viewing
  // mode, but the buttons also disable there so the affordance matches.
  copyBtn.disabled = !range;
  cutBtn.disabled = !range || reviewMode === "viewing";
  pasteBtn.disabled = !hasSel || !doc || reviewMode === "viewing";
  syncStylesGalleryActive();
  propertiesBtn.disabled = !doc;
  pageSetupBtn.disabled = !doc;
  viewOutlineBtn.disabled = !doc;
  viewOutlineBtn.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  reviewBtn.disabled = !doc;
  reviewBtn.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  railReview.disabled = !doc;
  railReview.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  viewZoomOut.disabled = !doc;
  viewZoomIn.disabled = !doc;
  tabTable.disabled = !inTable;
  if (tabTable.disabled && tabTable.getAttribute("aria-selected") === "true") {
    selectRibbonTab("home");
  }
  if (!outlinePanel.hidden) reflectOutlineSelection();
}

/** Fills the paragraph-style dropdown from the open document's styles. */
function populateStyles() {
  const styles = doc ? doc.listStyles() : [];
  for (const select of [paragraphStyleSel, paraPanelStyle]) {
    select.replaceChildren();
    for (const [value, label] of [["", "Style"], ...styles.map((s) => [s, s])]) {
      const opt = document.createElement("option");
      opt.value = value;
      opt.textContent = label;
      select.appendChild(opt);
    }
  }
  // The visible Styles gallery mirrors the same style list.
  buildStylesGallery(styles);
  scheduleRibbonOverflow();
}

function populateTableStyles() {
  tableStyleMenu.replaceChildren();
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "table-style-choice table-style-clear";
  clear.dataset.tableStyle = "";
  clear.textContent = "No table style";
  tableStyleMenu.appendChild(clear);
  for (const style of doc?.listTableStyles?.() || []) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "table-style-choice";
    button.dataset.tableStyle = style;
    button.innerHTML = `<span class="table-style-swatch" aria-hidden="true"><i></i><i></i><i></i></span><span>${escapeHtml(style)}</span>`;
    tableStyleMenu.appendChild(button);
  }
}
let tableStylePopover;
tableStyleMenu.addEventListener("click", (event) => {
  const choice = event.target.closest("[data-table-style]");
  if (!choice || !selection || !doc) return;
  closePopover(tableStylePopover);
  runEdit(() => doc.applyTableStyle(selection.focus.node, choice.dataset.tableStyle), { gate: true });
});

// mousedown (not click) so a button never steals the selection focus mid-edit.
function onButton(el, handler) {
  el.addEventListener("mousedown", (e) => {
    e.preventDefault();
    handler();
  });
  // Native buttons activated from the keyboard emit `click` without a preceding
  // mouse event (`detail === 0`). Preserve the selection on pointer activation
  // while keeping every command reachable with Enter/Space.
  el.addEventListener("click", (e) => {
    if (e.detail !== 0) return;
    e.preventDefault();
    handler();
  });
}

/** Opens the Insert link dialog for the selected same-paragraph text (⌘K, the
 * ribbon Link button, and the command palette). The dialog owns the external
 * URL / bookmark / ScreenTip entry and applies through the same gated
 * setHyperlink path the raw prompt used. */
function editSelectionLink() {
  if (!doc || !selection || !hasRange()) return;
  const { anchor, focus } = selection;
  if (anchor.node !== focus.node) {
    setStatus("Links must stay within one paragraph", "error");
    return;
  }
  const start = Math.min(anchor.offset, focus.offset);
  const end = Math.max(anchor.offset, focus.offset);
  const text = doc.copyText(anchor.node, start, anchor.node, end);
  openLinkDialog({ node: anchor.node, start, end, text });
}

onButton(insertLinkBtn, editSelectionLink);
for (const key of ["bold", "italic", "underline", "strike"]) {
  onButton(fmtButtons[key], () => toggleFormat(key));
}
onButton(clearFormattingBtn, () => {
  if (!hasRange()) return;
  if (reviewMode === "suggesting") {
    setStatus("Clear formatting is not tracked; switch to Editing to apply it", "error");
    return;
  }
  runToolbarEdit((a, b, c, d) => doc.clearFormatting(a, b, c, d));
});

// ---- Format painter (Word/Docs "copy formatting → paint onto target") -------
// Single click captures the caret/selection's formatting and arms a one-shot
// paint; the next document click (expanded to the clicked word) or drag receives
// it, then it disarms. Double-click the brush locks it (sticky) so successive
// targets keep receiving the format until Esc or another brush click. Formatting
// is applied through the very same engine ops the toolbar's own Bold/color/…
// controls use, so painted formatting is indistinguishable from hand-applied.
let formatPainter = null; // { fmt, sticky } while armed, else null

/** Snapshots the current selection/caret's run + paragraph formatting into a
 *  plain patch. Mixed run properties (and automatic/theme colors) are left out
 *  so painting never forces a single value onto a genuinely mixed source. */
function captureFormatForPainter() {
  if (!doc || !selection) return null;
  const range = hasRange();
  const [sn, so, en, eo] = selEndpoints();
  const f = range
    ? doc.selectionFormat(sn, so, en, eo)
    : doc.caretFormat(selection.focus.node, selection.focus.offset);
  const fmt = { bold: f.bold, italic: f.italic, underline: f.underline, strike: f.strike };
  f.free();
  const rs = range
    ? doc.selectionRunStyle(sn, so, en, eo)
    : doc.caretRunStyle(selection.focus.node, selection.focus.offset);
  if (!rs.sizeMixed && rs.sizePoints) fmt.sizePoints = rs.sizePoints;
  if (!rs.fontMixed && rs.font) fmt.font = rs.font;
  if (!rs.colorMixed && rs.color) fmt.color = rs.color; // skip automatic/theme (empty)
  if (!rs.highlightMixed && rs.highlight) fmt.highlight = rs.highlight; // includes "none"
  if (!rs.verticalAlignMixed && rs.verticalAlign) fmt.vertAlign = rs.verticalAlign;
  rs.free();
  // Paragraph formatting reachable through absolute getter/setter pairs. Indent
  // and paragraph spacing lack absolute copy ops today (relative-only), so they
  // are intentionally not painted (tracked as a follow-up).
  const node = selection.focus.node;
  fmt.align = doc.alignmentAt(node, selection.focus.offset);
  const style = doc.paragraphStyleAt(node);
  if (style) fmt.paraStyle = style;
  const line = doc.lineSpacingAt(node);
  if (line) fmt.lineSpacing = line;
  return fmt;
}

/** Applies the captured format to the current range via the same edit ops the
 *  toolbar uses. Returns whether anything was applied. */
async function applyPaintedFormat() {
  const fmt = formatPainter?.fmt;
  if (!fmt || !doc || !hasRange()) return false;
  if (blockMutationInViewing()) return false;
  if (reviewMode === "suggesting") {
    setStatus("Format painter isn't tracked yet; switch to Editing to paint formatting", "error");
    return false;
  }
  // Paragraph style first, so painted direct formatting overrides the style.
  if (fmt.paraStyle) await runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, fmt.paraStyle));
  await runToolbarEdit((a, b, c, d) =>
    doc.formatSelection(a, b, c, d, fmt.bold, fmt.italic, fmt.underline, fmt.strike),
  );
  if (fmt.sizePoints != null) await runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, fmt.sizePoints));
  if (fmt.font) await runToolbarEdit((a, b, c, d) => doc.setFont(a, b, c, d, fmt.font));
  if (fmt.color) {
    const [r, g, b] = hexToRgb(fmt.color);
    await runToolbarEdit((a, x, c, d) => doc.setTextColor(a, x, c, d, r, g, b));
  }
  if (fmt.highlight) await runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, fmt.highlight));
  if (fmt.vertAlign) await runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, fmt.vertAlign));
  if (fmt.align) await runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, fmt.align));
  if (fmt.lineSpacing) await runToolbarEdit((a, b, c, d) => doc.setLineSpacing(a, b, c, d, fmt.lineSpacing));
  updateToolbar();
  return true;
}

/** Reflects the painter's armed / sticky state on the toolbar button and body
 *  (the body flag drives the paintbrush cursor affordance over the pages). */
function reflectFormatPainter() {
  const armed = !!formatPainter;
  formatPainterBtn.setAttribute("aria-pressed", String(armed));
  formatPainterBtn.classList.toggle("is-sticky", !!formatPainter?.sticky);
  document.body.classList.toggle("is-format-painting", armed);
}

function armFormatPainter(sticky) {
  if (!doc || !selection) {
    setStatus("Place the caret in text to copy its formatting", "error");
    return;
  }
  const fmt = captureFormatForPainter();
  if (!fmt) {
    setStatus("Place the caret in text to copy its formatting", "error");
    return;
  }
  formatPainter = { fmt, sticky: !!sticky };
  reflectFormatPainter();
  setStatus(
    sticky
      ? "Format painter locked — paint successive selections; Esc or click the brush to stop"
      : "Format painter — click a word or drag over text to paint the copied formatting",
  );
}

function disarmFormatPainter(reason) {
  if (!formatPainter) return;
  formatPainter = null;
  reflectFormatPainter();
  if (reason) setStatus(reason);
}

/** Consumes a document pointer gesture as a paint target: a drag's range, or the
 *  word under a bare click. Disarms afterward unless the painter is locked. */
async function paintFormatFromGesture(gesture, event) {
  if (!hasRange()) {
    const page = gesture?.page || pageFromClientPoint(event.clientX, event.clientY);
    if (page) selectWord(page, event);
  }
  const painted = hasRange() ? await applyPaintedFormat() : false;
  if (!formatPainter?.sticky) disarmFormatPainter();
  else if (painted) setStatus("Painted — keep painting or press Esc to stop");
}

// Preserve the model selection on mousedown (like every other toolbar control),
// then arm/lock/cancel on click. `detail` distinguishes single vs double click:
// double-click locks sticky mode, a single click on an armed brush cancels it.
formatPainterBtn.addEventListener("mousedown", (e) => e.preventDefault());
formatPainterBtn.addEventListener("click", (e) => {
  e.preventDefault();
  if (formatPainterBtn.disabled) return;
  if (e.detail >= 2) {
    armFormatPainter(true);
    return;
  }
  if (formatPainter) {
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
    if (formatPainter && e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      disarmFormatPainter("Format painter off");
    }
  },
  true,
);

function suggestRunFormat(patch) {
  return runToolbarEdit((sn, so, en, eo) => {
    if (sn !== en) throw new Error("Tracked formatting requires one paragraph");
    return doc.suggestFormat(
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
  if (hasRange()) {
    if (reviewMode === "suggesting") suggestRunFormat(patch);
    else applyFn();
  } else if (selection) {
    pendingFormat = { ...(pendingFormat || {}), ...patch };
    updateToolbar();
  }
}
onButton(superBtn, () => {
  const value = superBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "super";
  armOrApplyRun({ vertAlign: value }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, value)),
  );
});
onButton(subBtn, () => {
  const value = subBtn.getAttribute("aria-pressed") === "true" ? "baseline" : "sub";
  armOrApplyRun({ vertAlign: value }, () =>
    runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, value)),
  );
});
for (const [key, btn] of Object.entries(alignBtns)) {
  onButton(btn, () => runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, key)));
}
/** Word-style indent commands: list items change numbering level, while ordinary
 * paragraphs retain the existing 0.25in paragraph-indent behavior. */
function adjustIndentCommand(delta) {
  if (!selection || !doc) return;
  const listKind = doc.listStyleAt(selection.focus.node);
  runToolbarEdit((a, b, c, d) =>
    listKind
      ? doc.adjustListLevel(a, b, c, d, delta > 0 ? 1 : -1)
      : doc.adjustIndent(a, b, c, d, delta),
  );
}
onButton(indentDecBtn, () => adjustIndentCommand(-360));
onButton(indentIncBtn, () => adjustIndentCommand(360));
onButton(bulletListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet")));
onButton(numberedListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered")));
onButton(checkListBtn, () => {
  runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "checklist"));
  // A brand-new checklist introduces the `☐` marker glyph; fetch its covering
  // symbol font (once) so it renders instead of a .notdef box, then re-render.
  void ensureGlyphCoverage("checklist");
});
onButton(restartListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.restartList(selection.focus.node));
});
onButton(continueListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.continueList(selection.focus.node));
});

fontSizeSel.addEventListener("change", () => {
  const pt = Number(fontSizeSel.value);
  const valid =
    fontSizeSel.value.trim() !== "" &&
    Number.isFinite(pt) &&
    pt >= 1 &&
    pt <= 1638 &&
    Number.isInteger(pt * 2);
  fontSizeSel.setCustomValidity(valid ? "" : "Enter a font size from 1 to 1638 pt in 0.5 pt steps.");
  if (!valid) {
    fontSizeSel.reportValidity();
    updateToolbar();
    return;
  }
  armOrApplyRun({ sizeHalfPoints: Math.round(pt * 2) }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, pt)),
  );
});
// ---- Toolbar popovers (compact anchored menus such as spacing) --------------
// One lightweight manager: anchor a menu under its button, only one open at a
// time, dismiss on outside-pointerdown / Escape. Each popover registers a
// `reflect()` that syncs its controls to the caret paragraph.
const TWIPS_PER_POINT = 20;
const popovers = [];

function openPopover(p) {
  if (!selection) return;
  for (const q of popovers) if (q !== p) closePopover(q);
  const r = p.btn.getBoundingClientRect();
  p.menu.hidden = false;
  p.btn.setAttribute("aria-expanded", "true");
  p.reflect();
  const gutter = 8;
  const width = p.menu.offsetWidth;
  const height = p.menu.offsetHeight;
  const left = Math.min(
    Math.max(gutter, r.left),
    Math.max(gutter, window.innerWidth - width - gutter),
  );
  const below = r.bottom + 4;
  const above = r.top - height - 4;
  const top =
    below + height <= window.innerHeight - gutter
      ? below
      : Math.max(gutter, above);
  p.menu.style.left = `${Math.round(left)}px`;
  p.menu.style.top = `${Math.round(top)}px`;
}

function closePopover(p) {
  p.menu.hidden = true;
  p.btn.setAttribute("aria-expanded", "false");
}

function registerPopover(btn, menu, reflect) {
  const p = { btn, menu, reflect };
  popovers.push(p);
  onButton(btn, () => (menu.hidden ? openPopover(p) : closePopover(p)));
  // Keep clicks inside the menu from stealing the selection focus, but let form
  // controls (inputs, selects) focus, toggle, and open normally.
  menu.addEventListener("mousedown", (e) => {
    if (!["INPUT", "SELECT", "OPTION"].includes(e.target.tagName)) e.preventDefault();
  });
  return p;
}

tableStylePopover = registerPopover(tableStyleBtn, tableStyleMenu, () => {});

// The "More styles" ▾ popover reuses the same anchored-menu manager (outside-
// click + Escape dismissal, single-open, anchored positioning). Its reflect()
// re-marks the active style each time it opens.
if (stylesMoreBtn && stylesMorePanel) {
  stylesMorePopover = registerPopover(stylesMoreBtn, stylesMorePanel, syncStylesGalleryActive);
  // When the ▾ opens the popover, move focus into it (onto the active/first card)
  // so keyboard users land in the list; a close toggle leaves focus on the button.
  const focusIntoMorePanel = () => {
    if (stylesMorePanel.hidden) return;
    const card =
      stylesMorePanel.querySelector('.style-card[tabindex="0"]') ||
      stylesMorePanel.querySelector(".style-card");
    card?.focus();
  };
  stylesMoreBtn.addEventListener("mousedown", focusIntoMorePanel);
  stylesMoreBtn.addEventListener("click", (e) => {
    if (e.detail === 0) focusIntoMorePanel();
  });
}

document.addEventListener("mousedown", (e) => {
  for (const p of popovers) {
    if (
      !p.menu.hidden &&
      !p.menu.contains(e.target) &&
      e.target !== p.btn &&
      !p.btn.contains(e.target)
    ) {
      closePopover(p);
    }
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") for (const p of popovers) if (!p.menu.hidden) closePopover(p);
});

// -- Font family menu, color/highlight pickers, grow/shrink, change case -------
// (Q1/Q2/Q5) Real dropdown swatch pickers replace the raw OS color input and
// the native highlight <select>; a searchable font menu replaces the native
// font <select>; A⁺/A⁻ step the standard sizes; a Change case menu transforms
// the selection through the existing rich-run copy/paste ops (no new engine op).

// Standard-colors palette (Google-Docs-style: a grayscale row + a hue row). The
// document theme palette is not exposed to the webapp, so the theme-colors row
// is omitted gracefully rather than faked.
const TEXT_STANDARD_COLORS = [
  "#000000", "#434343", "#666666", "#999999", "#b7b7b7", "#cccccc", "#d9d9d9", "#efefef", "#f3f3f3", "#ffffff",
  "#980000", "#ff0000", "#ff9900", "#ffff00", "#00ff00", "#00ffff", "#4a86e8", "#0000ff", "#9900ff", "#ff00ff",
];
// The complete set of OOXML `w:highlight` named colors the engine accepts, with
// their display swatch and a human label. `setHighlight` takes the name, not a hex.
const HIGHLIGHT_COLORS = [
  { name: "yellow", hex: "#ffff00", label: "Yellow" },
  { name: "green", hex: "#00ff00", label: "Bright green" },
  { name: "cyan", hex: "#00ffff", label: "Turquoise" },
  { name: "magenta", hex: "#ff00ff", label: "Pink" },
  { name: "blue", hex: "#0000ff", label: "Blue" },
  { name: "red", hex: "#ff0000", label: "Red" },
  { name: "darkYellow", hex: "#808000", label: "Dark yellow" },
  { name: "darkGreen", hex: "#008000", label: "Green" },
  { name: "darkCyan", hex: "#008080", label: "Teal" },
  { name: "darkMagenta", hex: "#800080", label: "Violet" },
  { name: "darkRed", hex: "#800000", label: "Dark red" },
  { name: "darkBlue", hex: "#000080", label: "Dark blue" },
  { name: "darkGray", hex: "#808080", label: "Gray 50%" },
  { name: "lightGray", hex: "#c0c0c0", label: "Gray 25%" },
  { name: "black", hex: "#000000", label: "Black" },
  { name: "white", hex: "#ffffff", label: "White" },
];
const HIGHLIGHT_HEX = new Map(HIGHLIGHT_COLORS.map((c) => [c.name, c.hex]));
const HIGHLIGHT_LABEL = new Map(HIGHLIGHT_COLORS.map((c) => [c.name, c.label]));
function highlightHex(name) {
  return name && name !== "none" ? HIGHLIGHT_HEX.get(name) ?? null : null;
}

// Session-remembered recently-used swatches (most-recent first, deduped, capped).
const recentTextColors = [];
const recentHighlights = [];
function recordRecent(list, value) {
  const i = list.indexOf(value);
  if (i !== -1) list.splice(i, 1);
  list.unshift(value);
  if (list.length > 10) list.length = 10;
}

let lastTextColor = "#000000";
let lastHighlight = "yellow";

/** Reflect the current text color onto the "A" underline bar (and remember it as
 *  the color the split-button's apply half reapplies). */
function reflectTextColorSwatch(hex) {
  if (hex) lastTextColor = hex;
  textColorBar.style.background = lastTextColor;
  if (selTextColorBar) selTextColorBar.style.background = lastTextColor;
}
/** Reflect the current highlight onto the highlighter bar (transparent for none). */
function reflectHighlightSwatch(name) {
  if (name && name !== "none") lastHighlight = name;
  const hex = name === "none" ? null : highlightHex(name || lastHighlight);
  highlightBar.style.background = hex || "transparent";
  highlightBar.classList.toggle("is-none", !hex);
  if (selHighlightBar) {
    selHighlightBar.style.background = hex || "transparent";
    selHighlightBar.classList.toggle("is-none", !hex);
  }
}

/** Builds one swatch cell button. `value` is what gets applied (a hex for text,
 *  a named color for highlight); `color` is the display hex; `active` lights it. */
function makeSwatchCell(kind, value, color, label, active) {
  const cell = document.createElement("button");
  cell.type = "button";
  cell.className = "swatch-cell";
  cell.style.setProperty("--sw", color);
  cell.title = label;
  cell.setAttribute("aria-label", label);
  cell.dataset[kind === "text" ? "color" : "highlight"] = value;
  if (active) cell.classList.add("is-active");
  if (color.toLowerCase() === "#ffffff") cell.classList.add("is-light");
  return cell;
}
function makeSwatchGrid(cells) {
  const grid = document.createElement("div");
  grid.className = "swatch-grid";
  grid.setAttribute("role", "group");
  for (const cell of cells) grid.appendChild(cell);
  return grid;
}
function makeMenuHeading(text) {
  const h = document.createElement("div");
  h.className = "menu-heading";
  h.textContent = text;
  return h;
}

/** (Re)renders a color picker menu, marking the active swatch and refreshing the
 *  recently-used row. Called on each open via the popover's reflect hook. */
function renderColorMenu(kind, menu = kind === "text" ? textColorMenu : highlightMenu) {
  const activeValue = kind === "text" ? lastTextColor.toLowerCase() : lastHighlight;
  menu.replaceChildren();

  // Automatic (text) / No color (highlight) — the reset entry.
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "color-row-action";
  if (kind === "text") {
    reset.dataset.auto = "1";
    reset.innerHTML = '<span class="color-chip" style="--sw:#000000"></span><span>Automatic</span>';
  } else {
    reset.dataset.highlight = "none";
    reset.innerHTML = '<span class="color-chip color-chip-none"></span><span>No color</span>';
  }
  menu.appendChild(reset);

  menu.appendChild(makeMenuHeading(kind === "text" ? "Standard colors" : "Highlight colors"));
  if (kind === "text") {
    menu.appendChild(makeSwatchGrid(
      TEXT_STANDARD_COLORS.map((hex) =>
        makeSwatchCell("text", hex, hex, hex.toUpperCase(), hex.toLowerCase() === activeValue)),
    ));
  } else {
    menu.appendChild(makeSwatchGrid(
      HIGHLIGHT_COLORS.map((c) =>
        makeSwatchCell("highlight", c.name, c.hex, c.label, c.name === activeValue)),
    ));
  }

  const recents = kind === "text" ? recentTextColors : recentHighlights;
  if (recents.length) {
    menu.appendChild(makeMenuHeading("Recent"));
    menu.appendChild(makeSwatchGrid(
      recents.map((value) => {
        const color = kind === "text" ? value : highlightHex(value) ?? "#000000";
        const label = kind === "text" ? value.toUpperCase() : (HIGHLIGHT_LABEL.get(value) ?? value);
        return makeSwatchCell(kind, value, color, label,
          kind === "text" ? value.toLowerCase() === activeValue : value === activeValue);
      }),
    ));
  }

  if (kind === "text") {
    const more = document.createElement("button");
    more.type = "button";
    more.className = "color-row-action color-more";
    more.dataset.more = "1";
    more.innerHTML = '<span class="ms" aria-hidden="true">colorize</span><span>More colors…</span>';
    menu.appendChild(more);
  }
}

const textColorPopover = registerPopover(textColorCaret, textColorMenu, () => renderColorMenu("text"));
const highlightPopover = registerPopover(highlightCaret, highlightMenu, () => renderColorMenu("highlight"));

// One text-color menu-click handler, shared by the ribbon menu and the floating
// selection-toolbar menu. `popover` is the popover to close after applying; the
// apply goes through the same `applyTextColor` path (one undoable action, gated
// in Viewing/Suggesting). (mousedown is preventDefault'd by the popover manager,
// so the document selection survives the pointer press.)
function handleTextColorMenuClick(e, popover) {
  if (e.target.closest("[data-more]")) {
    textColorInput.value = lastTextColor;
    textColorInput.click(); // opens the OS color input as the custom fallback
    return;
  }
  const cell = e.target.closest("[data-color], [data-auto]");
  if (!cell) return;
  const hex = cell.dataset.auto ? "#000000" : cell.dataset.color;
  applyTextColor(hex);
  lastTextColor = hex;
  if (!cell.dataset.auto) recordRecent(recentTextColors, hex);
  reflectTextColorSwatch(hex);
  closePopover(popover);
  focusEditorSurface();
}
function handleHighlightMenuClick(e, popover) {
  const cell = e.target.closest("[data-highlight]");
  if (!cell) return;
  const name = cell.dataset.highlight;
  applyHighlight(name);
  if (name !== "none") {
    lastHighlight = name;
    recordRecent(recentHighlights, name);
  }
  reflectHighlightSwatch(name);
  closePopover(popover);
  focusEditorSurface();
}

textColorMenu.addEventListener("click", (e) => handleTextColorMenuClick(e, textColorPopover));
// "More colors…" custom fallback commits when the OS picker closes.
textColorInput.addEventListener("change", () => {
  const hex = textColorInput.value;
  applyTextColor(hex);
  lastTextColor = hex;
  recordRecent(recentTextColors, hex);
  reflectTextColorSwatch(hex);
});

highlightMenu.addEventListener("click", (e) => handleHighlightMenuClick(e, highlightPopover));

// Floating selection-toolbar pickers: same renderer, same apply path, popovers
// anchored to the floating buttons so they open next to the selection. Registered
// after the ribbon popovers so the ribbon behavior is untouched.
const selTextColorPopover = registerPopover(selTextColorBtn, selTextColorMenu, () =>
  renderColorMenu("text", selTextColorMenu),
);
const selHighlightPopover = registerPopover(selHighlightBtn, selHighlightMenu, () =>
  renderColorMenu("highlight", selHighlightMenu),
);
selTextColorMenu.addEventListener("click", (e) => handleTextColorMenuClick(e, selTextColorPopover));
selHighlightMenu.addEventListener("click", (e) => handleHighlightMenuClick(e, selHighlightPopover));

// Split-button apply halves reapply the last-used swatch (Word/Docs behavior).
onButton(textColorApplyBtn, () => {
  applyTextColor(lastTextColor);
  recordRecent(recentTextColors, lastTextColor);
});
onButton(highlightApplyBtn, () => {
  applyHighlight(lastHighlight);
  recordRecent(recentHighlights, lastHighlight);
});
reflectTextColorSwatch(lastTextColor);
reflectHighlightSwatch(lastHighlight);

// ---- Font family menu (Q2): searchable, own-typeface, recently-used group ----
// No font-enumeration API is exposed to the webapp, so this is a curated common
// list (the registry seam populates faces for rendering; this list is the menu
// inventory). The caret's actual family is always included so an imported font
// stays selectable/visible even when it is not in the list.
const COMMON_FONTS = [
  "Arial", "Calibri", "Cambria", "Comic Sans MS", "Consolas", "Courier New",
  "Georgia", "Helvetica", "Lato", "Montserrat", "Noto Sans", "Noto Serif",
  "Open Sans", "Roboto", "Segoe UI", "Tahoma", "Times New Roman",
  "Trebuchet MS", "Verdana",
];
const recentFonts = [];
let fontMenuActiveIndex = -1;

function fontInventory() {
  const all = new Set(COMMON_FONTS);
  if (currentFontFamily) all.add(currentFontFamily);
  return [...all].sort((a, b) => a.localeCompare(b));
}

/** (Re)renders the font list filtered by the search box; recently-used first,
 *  then the alphabetical inventory, each name shown in its own typeface. */
function renderFontMenu() {
  const query = fontMenuInput.value.trim().toLowerCase();
  const match = (name) => name.toLowerCase().includes(query);
  const recent = recentFonts.filter(match);
  const inventory = fontInventory().filter((name) => match(name) && !recent.includes(name));
  fontMenuList.replaceChildren();

  const addRow = (name, group) => {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "font-menu-item";
    row.setAttribute("role", "option");
    row.dataset.font = name;
    row.style.fontFamily = `"${name}", system-ui, sans-serif`;
    row.setAttribute("aria-selected", String(name === currentFontFamily));
    if (name === currentFontFamily) row.classList.add("is-current");
    row.innerHTML = `<span class="font-menu-check ms" aria-hidden="true">check</span><span class="font-menu-name">${escapeHtml(name)}</span>`;
    fontMenuList.appendChild(row);
    if (group) row.dataset.group = group;
  };

  if (recent.length) {
    const h = makeMenuHeading("Recently used");
    fontMenuList.appendChild(h);
    for (const name of recent) addRow(name, "recent");
    fontMenuList.appendChild(makeMenuHeading("All fonts"));
  }
  for (const name of inventory) addRow(name);

  const rows = fontMenuList.querySelectorAll(".font-menu-item");
  fontMenuEmpty.hidden = rows.length > 0;
  // Default the active row to the current font (or the first row).
  fontMenuActiveIndex = [...rows].findIndex((r) => r.dataset.font === currentFontFamily);
  if (fontMenuActiveIndex < 0 && rows.length) fontMenuActiveIndex = 0;
  paintFontActive();
}
function paintFontActive() {
  const rows = fontMenuList.querySelectorAll(".font-menu-item");
  rows.forEach((row, i) => row.classList.toggle("is-active", i === fontMenuActiveIndex));
  rows[fontMenuActiveIndex]?.scrollIntoView({ block: "nearest" });
}
function chooseFont(name) {
  applyFontFamily(name);
  reflectFontFamily(name);
  recordRecent(recentFonts, name);
  closePopover(fontPopover);
  focusEditorSurface();
}

const fontPopover = registerPopover(fontFamilyBtn, fontMenu, () => {
  fontMenuInput.value = "";
  renderFontMenu();
  requestAnimationFrame(() => fontMenuInput.focus());
});
fontMenuInput.addEventListener("input", renderFontMenu);
fontMenuInput.addEventListener("keydown", (e) => {
  const rows = fontMenuList.querySelectorAll(".font-menu-item");
  if (e.key === "ArrowDown") {
    e.preventDefault();
    fontMenuActiveIndex = Math.min(rows.length - 1, fontMenuActiveIndex + 1);
    paintFontActive();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    fontMenuActiveIndex = Math.max(0, fontMenuActiveIndex - 1);
    paintFontActive();
  } else if (e.key === "Enter") {
    e.preventDefault();
    const row = rows[fontMenuActiveIndex];
    if (row) chooseFont(row.dataset.font);
  }
});
fontMenuList.addEventListener("click", (e) => {
  const row = e.target.closest(".font-menu-item");
  if (row) chooseFont(row.dataset.font);
});

// ---- Grow / shrink font (Q5) -------------------------------------------------
const FONT_STEP_SIZES = [
  8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 40, 44, 48, 54, 60, 66, 72, 80, 88, 96,
];
function currentFontPt() {
  const v = Number(fontSizeSel.value);
  if (Number.isFinite(v) && v >= 1) return v;
  if (pendingFormat?.sizeHalfPoints != null) return pendingFormat.sizeHalfPoints / 2;
  return 11;
}
function stepFontSize(dir) {
  const cur = currentFontPt();
  let next;
  if (dir > 0) {
    next = FONT_STEP_SIZES.find((s) => s > cur + 1e-6) ?? Math.min(1638, Math.round((cur + 2) * 2) / 2);
  } else {
    const smaller = FONT_STEP_SIZES.filter((s) => s < cur - 1e-6);
    next = smaller.length ? smaller[smaller.length - 1] : Math.max(1, Math.round((cur - 1) * 2) / 2);
  }
  armOrApplyRun({ sizeHalfPoints: Math.round(next * 2) }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, next)),
  );
}
onButton(growFontBtn, () => stepFontSize(1));
onButton(shrinkFontBtn, () => stepFontSize(-1));

// ---- Change case (Q5): transform selected text, preserving per-run format ----
function transformCase(text, mode) {
  switch (mode) {
    case "upper":
      return text.toLocaleUpperCase();
    case "lower":
      return text.toLocaleLowerCase();
    case "title":
      return text.replace(/\p{L}[\p{L}'’]*/gu, (w) => w[0].toLocaleUpperCase() + w.slice(1).toLocaleLowerCase());
    case "sentence": {
      const lowered = text.toLocaleLowerCase();
      return lowered.replace(/(^\s*\p{L})|([.!?]["')\]]?\s+\p{L})/gu, (m) => m.toLocaleUpperCase());
    }
    case "toggle":
      return [...text].map((ch) => {
        const up = ch.toLocaleUpperCase();
        const lo = ch.toLocaleLowerCase();
        return ch === lo && ch !== up ? up : lo;
      }).join("");
    default:
      return text;
  }
}
async function applyChangeCase(mode) {
  if (!doc || !hasRange()) return;
  const { anchor, focus } = selection;
  let runs;
  try {
    runs = JSON.parse(doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset));
  } catch {
    return;
  }
  if (!Array.isArray(runs) || !runs.length) return;
  const full = runs.map((r) => (r.paragraphBreak ? "\n" : String(r.text ?? ""))).join("");
  const transformed = transformCase(full, mode);
  const out = runs.map((r) => ({ ...r }));
  if (transformed.length === full.length) {
    // Length-preserving: re-slice so cross-run sentence/title casing is correct.
    let i = 0;
    for (const r of out) {
      const len = r.paragraphBreak ? 1 : String(r.text ?? "").length;
      if (!r.paragraphBreak && r.text != null) r.text = transformed.slice(i, i + len);
      i += len;
    }
  } else {
    // Rare Unicode length change (e.g. ß→SS): fall back to per-run transform.
    for (const r of out) if (!r.paragraphBreak && r.text != null) r.text = transformCase(String(r.text), mode);
  }
  await pasteRichRunsJson(JSON.stringify(out));
}
const changeCasePopover = registerPopover(changeCaseBtn, changeCaseMenu, () => {});
changeCaseMenu.addEventListener("click", (e) => {
  const item = e.target.closest("[data-case]");
  if (!item) return;
  closePopover(changeCasePopover);
  void applyChangeCase(item.dataset.case);
});

// -- List marker-format galleries (bullet glyph / number format) ---------------
// The bullet and numbered buttons carry a ▾ split that opens a small gallery of
// marker choices; picking one retargets the caret's list through the gated
// `setListFormat` path (one undo; blocked in Viewing/Suggesting like the sibling
// restart/continue list ops). The main button keeps its plain on/off toggle.
/** Mark the gallery cell matching the caret's current list marker as checked. */
function reflectListGallery(menu) {
  const current = doc && selection ? doc.listFormatAt(selection.focus.node) : "";
  for (const cell of menu.querySelectorAll(".list-gallery-cell")) {
    cell.setAttribute("aria-checked", String(cell.dataset.spec === current));
  }
}
const bulletGalleryPopover = registerPopover(bulletListMenuBtn, bulletGalleryMenu, () =>
  reflectListGallery(bulletGalleryMenu),
);
const numberGalleryPopover = registerPopover(numberedListMenuBtn, numberGalleryMenu, () =>
  reflectListGallery(numberGalleryMenu),
);
function wireListGallery(menu, popover) {
  menu.addEventListener("click", (e) => {
    const cell = e.target.closest("[data-spec]");
    if (!cell || !selection || !doc) return;
    closePopover(popover);
    const spec = cell.dataset.spec;
    const applied = runNodeEdit(() => doc.setListFormat(selection.focus.node, spec));
    // A newly chosen bullet glyph may need its covering symbol font fetched
    // (once) so it renders instead of a .notdef box, then re-render.
    if (applied && spec.startsWith("bullet:")) void ensureGlyphCoverage("list marker");
  });
}
wireListGallery(bulletGalleryMenu, bulletGalleryPopover);
wireListGallery(numberGalleryMenu, numberGalleryPopover);

// -- Line & paragraph spacing --------------------------------------------------
/** Reflect the caret paragraph's spacing into the menu (line-preset check +
 *  space before/after fields). */
function reflectSpacingMenu() {
  if (!doc || !selection) return;
  const s = doc.paragraphSpacing(selection.focus.node);
  const percent = s.lineRule === 0 ? s.linePercent : 0; // presets are `auto` multiples
  for (const b of spacingMenu.querySelectorAll(".spacing-line")) {
    b.setAttribute("aria-checked", String(Number(b.dataset.percent) === percent));
  }
  // Reflect the mode + value fields (lineRule: 0 auto/multiple, 1 atLeast, 2 exact).
  const editingCustom =
    document.activeElement === lineSpacingMode ||
    document.activeElement === lineSpacingValue;
  if (!editingCustom) {
    if (s.lineRule === 1) {
      lineSpacingMode.value = "atLeast";
      lineSpacingValue.value = s.lineTwip > 0 ? String(round2(s.lineTwip / TWIPS_PER_POINT)) : "";
    } else if (s.lineRule === 2) {
      lineSpacingMode.value = "exact";
      lineSpacingValue.value = s.lineTwip > 0 ? String(round2(s.lineTwip / TWIPS_PER_POINT)) : "";
    } else {
      lineSpacingMode.value = "multiple";
      lineSpacingValue.value = s.linePercent > 0 ? String(round2(s.linePercent / 100)) : "";
    }
    reflectLineSpacingUnit();
  }
  // Don't overwrite a field the user is mid-edit in.
  if (document.activeElement !== spaceBeforeInput) {
    spaceBeforeInput.value = s.beforeTwip >= 0 ? String(Math.round(s.beforeTwip / TWIPS_PER_POINT)) : "";
  }
  if (document.activeElement !== spaceAfterInput) {
    spaceAfterInput.value = s.afterTwip >= 0 ? String(Math.round(s.afterTwip / TWIPS_PER_POINT)) : "";
  }
}
registerPopover(spacingBtn, spacingMenu, reflectSpacingMenu);

for (const b of spacingMenu.querySelectorAll(".spacing-line")) {
  onButton(b, () => {
    runToolbarEdit((a, x, c, d) => doc.setLineSpacing(a, x, c, d, Number(b.dataset.percent)));
    reflectSpacingMenu();
  });
}

/** Round to at most 2 decimals, trimming trailing zeros. */
function round2(n) {
  return Math.round(n * 100) / 100;
}

/** Sync the value field's unit label + step to the current mode (× for a
 *  multiple, pt for atLeast/exact). */
function reflectLineSpacingUnit() {
  const multiple = lineSpacingMode.value === "multiple";
  lineSpacingUnit.textContent = multiple ? "×" : "pt";
  lineSpacingValue.step = multiple ? "0.05" : "1";
}

/** Commit the custom line-spacing mode + value. Multiple rides `setLineSpacing`
 *  (the `auto` percent rule); At least / Exactly ride `setLineSpacingExact`
 *  (twips + `at_least` flag: true → atLeast, false → exact). Blank/non-numeric
 *  is ignored. */
function applyCustomLineSpacing() {
  const raw = lineSpacingValue.value.trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return;
  const v = Number(raw);
  if (v <= 0) return;
  const mode = lineSpacingMode.value;
  if (mode === "multiple") {
    const percent = Math.round(v * 100);
    runToolbarEdit((a, x, c, d) => doc.setLineSpacing(a, x, c, d, percent));
  } else {
    const twips = Math.max(0, Math.round(v * TWIPS_PER_POINT));
    const atLeast = mode === "atLeast";
    runToolbarEdit((a, x, c, d) => doc.setLineSpacingExact(a, x, c, d, twips, atLeast));
  }
  reflectSpacingMenu();
}
// Switching mode only reinterprets the value's unit; it never auto-applies (a
// multiple typed as "1.5" must not be re-read as 1.5 pt). Commit on value change.
lineSpacingMode.addEventListener("change", reflectLineSpacingUnit);
lineSpacingValue.addEventListener("change", applyCustomLineSpacing);

/** Commit a space-before/after field: blank clears (back to style default),
 *  otherwise points → twips (clamped ≥ 0). Ignores non-numeric input. */
function applySpace(input, setter) {
  const raw = input.value.trim();
  if (raw !== "" && !Number.isFinite(Number(raw))) return;
  const twips = raw === "" ? -1 : Math.max(0, Math.round(Number(raw) * TWIPS_PER_POINT));
  runToolbarEdit((a, x, c, d) => setter(a, x, c, d, twips));
}
spaceBeforeInput.addEventListener("change", () =>
  applySpace(spaceBeforeInput, (a, b, c, d, t) => doc.setSpaceBefore(a, b, c, d, t)),
);
spaceAfterInput.addEventListener("change", () =>
  applySpace(spaceAfterInput, (a, b, c, d, t) => doc.setSpaceAfter(a, b, c, d, t)),
);

// -- Paragraph properties inspector ------------------------------------------
/** Twips → inches string, trimming trailing zeros; "" for zero. */
function inchStr(twip) {
  if (!twip) return "";
  return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "");
}
/** An inches field's value → twips (≥ 0); "" or non-numeric → 0. */
function inchTwips(input) {
  const raw = input.value.trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.max(0, Math.round(Number(raw) * TWIPS_PER_INCH));
}

/** An inches field's value → signed twips; blank/non-numeric → 0. */
function signedInchTwips(input) {
  const raw = input.value.trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return 0;
  return Math.round(Number(raw) * TWIPS_PER_INCH);
}

function setMixedCheckbox(input, state) {
  input.indeterminate = state === 2;
  input.checked = state === 1;
}

function reflectParagraphProperties() {
  if (!doc || !selection) return;
  const [startNode, startOffset, endNode, endOffset] = selEndpoints();
  const state = doc.selectionParagraphState(startNode, startOffset, endNode, endOffset);
  paragraphPropertiesContext.textContent =
    state.count === 1 ? "1 paragraph" : `${state.count} paragraphs`;

  paraPanelStyle.options[0].textContent = state.styleMixed ? "Mixed" : "Style";
  if (document.activeElement !== paraPanelStyle) {
    paraPanelStyle.value = state.styleMixed ? "" : state.style;
  }
  for (const button of paraPanelAlign.querySelectorAll("button[data-palign]")) {
    button.setAttribute(
      "aria-pressed",
      state.alignmentMixed ? "mixed" : String(button.dataset.palign === state.alignment),
    );
  }

  if (document.activeElement !== indentLeftInput) {
    indentLeftInput.placeholder = state.startMixed ? "Mixed" : "";
    indentLeftInput.value = state.startMixed ? "" : inchStr(state.startTwip);
  }
  if (document.activeElement !== indentRightInput) {
    indentRightInput.placeholder = state.endMixed ? "Mixed" : "";
    indentRightInput.value = state.endMixed ? "" : inchStr(state.endTwip);
  }
  if (![indentSpecialByInput, indentSpecialSel].includes(document.activeElement)) {
    if (state.firstLineMixed || state.hangingMixed) {
      indentSpecialSel.value = "";
      indentSpecialByInput.value = "";
      indentSpecialByInput.placeholder = "Mixed";
    } else if (state.firstLineTwip > 0) {
      indentSpecialSel.value = "first";
      indentSpecialByInput.value = inchStr(state.firstLineTwip);
      indentSpecialByInput.placeholder = "";
    } else if (state.hangingTwip > 0) {
      indentSpecialSel.value = "hanging";
      indentSpecialByInput.value = inchStr(state.hangingTwip);
      indentSpecialByInput.placeholder = "";
    } else {
      indentSpecialSel.value = "none";
      indentSpecialByInput.value = "";
      indentSpecialByInput.placeholder = "";
    }
  }

  if (document.activeElement !== paraLineSpacing) {
    paraLineSpacing.value = state.lineMixed ? "" : String(state.linePercent || "");
  }
  if (document.activeElement !== paraSpaceBefore) {
    paraSpaceBefore.placeholder = state.beforeMixed ? "Mixed" : "";
    paraSpaceBefore.value =
      state.beforeMixed || state.beforeTwip < 0
        ? ""
        : String(Math.round(state.beforeTwip / TWIPS_PER_POINT));
  }
  if (document.activeElement !== paraSpaceAfter) {
    paraSpaceAfter.placeholder = state.afterMixed ? "Mixed" : "";
    paraSpaceAfter.value =
      state.afterMixed || state.afterTwip < 0
        ? ""
        : String(Math.round(state.afterTwip / TWIPS_PER_POINT));
  }

  setMixedCheckbox(pgKeepNext, state.keepNextState);
  setMixedCheckbox(pgKeepLines, state.keepLinesState);
  setMixedCheckbox(pgBreakBefore, state.pageBreakBeforeState);
  paraShadeMixed.hidden = !state.shadingMixed;
  paraShadeNone.setAttribute(
    "aria-pressed",
    state.shadingMixed ? "mixed" : String(state.shading < 0),
  );
  if (!state.shadingMixed && state.shading >= 0 && document.activeElement !== paraShade) {
    paraShade.value = `#${state.shading.toString(16).padStart(6, "0")}`;
  }
  paraBordersMixed.hidden = !state.bordersMixed;
  const bit = { top: 1, bottom: 2, left: 4, right: 8 };
  for (const b of paragraphPropertiesPanel.querySelectorAll(".border-btn")) {
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

function toggleParagraphProperties(open) {
  const show = open ?? paragraphPropertiesPanel.hidden;
  if (show && (!doc || !selection)) return;
  const returnFocus =
    !show && paragraphPropertiesPanel.contains(document.activeElement);
  paragraphPropertiesPanel.hidden = !show;
  paraOptsBtn.setAttribute("aria-expanded", String(show));
  if (show) {
    toggleTableProperties(false);
    for (const popover of popovers) closePopover(popover);
    reflectParagraphProperties();
    queueMicrotask(() => paraPanelStyle.focus());
  } else if (returnFocus) {
    paraOptsBtn.focus({ preventScroll: true });
  }
}

paraOptsBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleParagraphProperties();
});
paragraphPropertiesCloseBtn.addEventListener("click", () =>
  toggleParagraphProperties(false),
);
document.addEventListener("keydown", (event) => {
  if (
    event.key === "Escape" &&
    !paragraphPropertiesPanel.hidden &&
    (document.activeElement === paraOptsBtn ||
      paragraphPropertiesPanel.contains(document.activeElement))
  ) {
    event.preventDefault();
    toggleParagraphProperties(false);
  }
});

// Borders: presets toggle edges (box = all, none = clear) in the chosen color at a
// 1 pt single line (8 eighth-points).
for (const b of paragraphPropertiesPanel.querySelectorAll(".border-btn")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(borderColorInput.value);
    runToolbarEdit((a, x, c, d) => doc.setParagraphBorder(a, x, c, d, b.dataset.border, r, g, bl, 8));
    reflectParagraphProperties();
  });
}
paraPanelStyle.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) =>
    doc.setParagraphStyle(a, b, c, d, paraPanelStyle.value),
  ),
);
for (const button of paraPanelAlign.querySelectorAll("button[data-palign]")) {
  onButton(button, () =>
    runToolbarEdit((a, b, c, d) =>
      doc.setAlignment(a, b, c, d, button.dataset.palign),
    ),
  );
}
paraLineSpacing.addEventListener("change", () => {
  if (!paraLineSpacing.value) return;
  runToolbarEdit((a, b, c, d) =>
    doc.setLineSpacing(a, b, c, d, Number(paraLineSpacing.value)),
  );
});
paraSpaceBefore.addEventListener("change", () =>
  applySpace(paraSpaceBefore, (a, b, c, d, twips) =>
    doc.setSpaceBefore(a, b, c, d, twips),
  ),
);
paraSpaceAfter.addEventListener("change", () =>
  applySpace(paraSpaceAfter, (a, b, c, d, twips) =>
    doc.setSpaceAfter(a, b, c, d, twips),
  ),
);

// -- Table & cell formatting (a single-node edit: applies to the caret's cell) --
/** Runs a `(node) => EditResult` edit on the caret's node, preserving the selection
 *  and repainting only the dirty pages (rebuild on a page-count change). Every
 *  current caller is a table/list structural mutation with no tracked-revision
 *  representation (REVIEW-GAP-009), so this always fails closed in Suggesting
 *  mode instead of silently applying untracked (REVIEW-GAP-004). */
function runNodeEdit(thunk) {
  if (!selection || !doc) return false;
  if (blockMutationInViewing()) return false;
  if (blockUntrackedInSuggesting()) return false;
  let res;
  try {
    res = thunk(selection.focus.node);
  } catch (err) {
    console.warn("edit ignored:", err?.message ?? err);
    setStatus(err?.message ?? "Table change could not be applied", "error");
    return false;
  }
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  if (newCount !== pages.length) renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ outline: true });
  return true;
}

function tableContextLabel(info) {
  return `${info.rows}×${info.columns} table · row ${info.row + 1}, column ${info.column + 1}${info.regular ? "" : " · merged/spanned"}`;
}

function reflectTableMenu() {
  if (!doc || !selection) return;
  const node = selection.focus.node;
  const rgb = doc.cellShadingAt(node);
  if (rgb >= 0 && document.activeElement !== cellShade) {
    cellShade.value = `#${rgb.toString(16).padStart(6, "0")}`;
  }
  const va = doc.cellVerticalAlignAt(node) || "top";
  for (const b of cellVAlign.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b.dataset.valign === va));
  }
  const edges = doc.cellBorderEdges(node);
  const bit = { top: 1, bottom: 2, left: 4, right: 8 };
  for (const b of tableFmtMenu.querySelectorAll(".border-btn")) {
    const k = b.dataset.cellborder;
    const on = k === "box" ? edges === 0b1111 : k === "none" ? edges === 0 : (edges & bit[k]) !== 0;
    b.setAttribute("aria-pressed", String(on));
  }
}
const tablePopover = registerPopover(tableBtn, tableFmtMenu, reflectTableMenu);

const TABLE_RIBBON_ACTIONS = {
  "insert-row-above": (n) => doc.insertRow(n, false),
  "insert-row-below": (n) => doc.insertRow(n, true),
  "insert-column-left": (n) => doc.insertColumn(n, false),
  "insert-column-right": (n) => doc.insertColumn(n, true),
  "delete-row": (n) => doc.deleteRow(n),
  "delete-column": (n) => doc.deleteColumn(n),
  "delete-table": (n) => doc.deleteTable(n),
};

for (const b of tableRibbon.querySelectorAll("[data-table-action]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const run = TABLE_RIBBON_ACTIONS[b.dataset.tableAction];
    if (!run) return;
    tableSelection = null;
    runEdit(() => run(selection.focus.node), { gate: true });
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-distribute]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const command = b.dataset.tableDistribute;
    tableSelection = null;
    runEdit(() =>
      command === "rows"
        ? doc.distributeTableRows(selection.focus.node)
        : doc.distributeTableColumns(selection.focus.node),
      { gate: true },
    );
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-sort]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const column = caretTableColumn(selection.focus.node);
    runEdit(() => doc.sortTable(selection.focus.node, b.dataset.tableSort, column), { gate: true });
  });
}

for (const b of tableRibbon.querySelectorAll("[data-table-select]")) {
  onButton(b, () => {
    if (!selection || !doc) return;
    const mode = b.dataset.tableSelect;
    tableSelection = { node: selection.focus.node, mode };
    drawSelection();
    setStatus(`Selected table ${mode}`);
    updateToolbar();
    focusEditorSurface();
  });
}

onButton(mergeCellsBtn, async () => {
  if (!selection || !doc) return;
  if (!tableSelection) {
    setStatus("Select a table row, column, or table first", "error");
    return;
  }
  await runEdit(() => doc.mergeTableSelection(tableSelection.node, tableSelection.mode), { gate: true });
  tableSelection = null;
  updateToolbar();
});

function toggleSplitCellDialog(open) {
  splitCellDialog.hidden = !open;
  if (open) {
    splitCellRows.value = "1";
    splitCellColumns.value = "2";
    splitCellColumns.focus();
  } else {
    splitCellBtn.focus({ preventScroll: true });
  }
}

onButton(splitCellBtn, () => {
  if (!selection || !doc) return;
  toggleSplitCellDialog(true);
});

onButton(splitCellClose, () => toggleSplitCellDialog(false));
onButton(splitCellCancel, () => toggleSplitCellDialog(false));
onButton(splitCellConfirm, async () => {
  if (!selection || !doc) return;
  const rows = Number.parseInt(splitCellRows.value, 10);
  const columns = Number.parseInt(splitCellColumns.value, 10);
  if (!Number.isInteger(rows) || !Number.isInteger(columns) || rows < 1 || columns < 1 || rows > 20 || columns > 20) {
    splitCellColumns.setCustomValidity("Enter whole numbers from 1 to 20.");
    splitCellColumns.reportValidity();
    return;
  }
  splitCellColumns.setCustomValidity("");
  await runEdit(() => doc.splitMergedCell(selection.focus.node, rows, columns), { gate: true });
  toggleSplitCellDialog(false);
  tableSelection = null;
  updateToolbar();
});

cellShade.addEventListener("change", () => {
  const [r, g, b] = hexToRgb(cellShade.value);
  runNodeEdit((n) => doc.setCellShading(n, r, g, b, false));
});
onButton(cellShadeNone, () => runNodeEdit((n) => doc.setCellShading(n, 0, 0, 0, true)));
for (const b of cellVAlign.querySelectorAll("button")) {
  onButton(b, () => {
    runNodeEdit((n) => doc.setCellVerticalAlign(n, b.dataset.valign));
    reflectTableMenu();
  });
}
for (const b of tableFmtMenu.querySelectorAll(".border-btn")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(cellBorderColor.value);
    runNodeEdit((n) => doc.setCellBorder(n, b.dataset.cellborder, r, g, bl, 8));
    reflectTableMenu();
  });
}
for (const b of tableFmtMenu.querySelectorAll("[data-tableborder]")) {
  onButton(b, () => {
    const [r, g, bl] = hexToRgb(tableBorderColor.value);
    runNodeEdit((n) => doc.setTableBorder(n, b.dataset.tableborder, r, g, bl, 8));
  });
}

// -- Table properties inspector ----------------------------------------------
let tablePropertiesCurrent = null;
let tablePropertiesNode = null;

function dialogTwipsValue(twips) {
  return twips < 0
    ? ""
    : (twips / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

function optionalDialogTwips(input) {
  const raw = input.value.trim();
  return raw === "" ? -1 : Math.round(Number(raw) * TWIPS_PER_INCH);
}

function updateTableRowHeightField() {
  const automatic = tableRowHeightRule.value === "auto";
  tableRowHeight.disabled = automatic;
  tableRowHeight.required = !automatic;
  if (automatic) tableRowHeight.setCustomValidity("");
}

function reflectTableProperties(node = selection?.focus.node) {
  if (!doc || node == null) return false;
  const info = doc.tableInfo(node);
  if (!info.found) {
    info.free();
    return false;
  }

  tablePropertiesNode = node;
  tablePropertiesContext.textContent = tableContextLabel(info);
  tableCaption.value = info.caption || "";
  tableDescription.value = info.description || "";
  tableHeaderRow.checked = info.headerRow;
  tableFixedLayout.checked = info.fixedLayout;
  tableColumnWidth.disabled = !info.regular;
  tableColumnWidth.value = dialogTwipsValue(info.columnWidthTwips);
  tableColumnWidthNote.textContent = info.regular
    ? "Sets the width of the current column."
    : "Column sizing is unavailable for merged or spanned tables.";
  tableWidth.value = dialogTwipsValue(info.tableWidthTwips);
  tableIndent.value = dialogTwipsValue(info.tableIndentTwips);
  tableRowHeight.value = dialogTwipsValue(info.rowHeightTwips);
  tableRowHeightRule.value = info.rowHeightRule || "auto";
  tableCellMargin.value = dialogTwipsValue(info.cellMarginTwips);
  tableCellSpacing.value = dialogTwipsValue(info.cellSpacingTwips);
  for (const button of tableAlign.querySelectorAll("button")) {
    button.setAttribute("aria-pressed", String(button.dataset.talign === info.alignment));
  }
  updateTableRowHeightField();

  tablePropertiesCurrent = {
    alignment: info.alignment,
    tableWidthTwips: optionalDialogTwips(tableWidth),
    tableIndentTwips: signedInchTwips(tableIndent),
    fixedLayout: info.fixedLayout,
    headerRow: info.headerRow,
    columnWidthTwips: optionalDialogTwips(tableColumnWidth),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalDialogTwips(tableRowHeight),
    rowHeightRule: info.rowHeightRule || "auto",
    cellMarginTwips: optionalDialogTwips(tableCellMargin),
    cellSpacingTwips: optionalDialogTwips(tableCellSpacing),
    caption: tableCaption.value,
    description: tableDescription.value,
  };
  info.free();
  return true;
}

function toggleTableProperties(open) {
  const show = open ?? tablePropertiesPanel.hidden;
  if (show && !reflectTableProperties()) return;
  const returnFocus = !show && tablePropertiesPanel.contains(document.activeElement);
  tablePropertiesPanel.hidden = !show;
  tablePropertiesBtn.setAttribute("aria-expanded", String(show));
  if (show) {
    toggleParagraphProperties(false);
    closePopover(tablePopover);
    queueMicrotask(() =>
      tableAlign.querySelector('button[aria-pressed="true"]')?.focus(),
    );
  } else if (returnFocus) {
    tablePropertiesBtn.focus({ preventScroll: true });
  }
}

tablePropertiesBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  toggleTableProperties();
});
tableFormulaApply.addEventListener("click", () => {
  if (!selection || !doc || !tableFormula.value.trim()) return;
  runNodeEdit((node) => doc.calculateTableFormula(node, tableFormula.value));
});
tablePropertiesCloseBtn.addEventListener("click", () => toggleTableProperties(false));
tableAlign.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-talign]");
  if (!button) return;
  for (const candidate of tableAlign.querySelectorAll("button")) {
    candidate.setAttribute("aria-pressed", String(candidate === button));
  }
  commitTableProperties();
});
tablePropertiesPanel.addEventListener("change", (event) => {
  if (!(event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement)) {
    return;
  }
  if (event.target === tableRowHeightRule) updateTableRowHeightField();
  commitTableProperties();
});

function tablePropertiesPatch() {
  const inputs = [
    tableWidth,
    tableIndent,
    tableColumnWidth,
    tableRowHeight,
    tableCellMargin,
    tableCellSpacing,
  ].filter((input) => !input.disabled);
  for (const input of inputs) {
    input.setCustomValidity("");
    if (!input.checkValidity()) {
      input.reportValidity();
      input.focus();
      return null;
    }
  }
  if (tableRowHeightRule.value !== "auto" && tableRowHeight.value.trim() === "") {
    tableRowHeight.setCustomValidity("Enter a row height or choose Auto.");
    tableRowHeight.reportValidity();
    tableRowHeight.focus();
    return null;
  }

  const next = {
    alignment:
      tableAlign.querySelector('button[aria-pressed="true"]')?.dataset.talign ?? "left",
    tableWidthTwips: optionalDialogTwips(tableWidth),
    tableIndentTwips: signedInchTwips(tableIndent),
    fixedLayout: tableFixedLayout.checked,
    headerRow: tableHeaderRow.checked,
    columnWidthTwips: optionalDialogTwips(tableColumnWidth),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalDialogTwips(tableRowHeight),
    rowHeightRule: tableRowHeightRule.value,
    cellMarginTwips: optionalDialogTwips(tableCellMargin),
    cellSpacingTwips: optionalDialogTwips(tableCellSpacing),
    caption: tableCaption.value,
    description: tableDescription.value,
  };
  const patch = {};
  for (const [key, value] of Object.entries(next)) {
    if (key === "columnWidthTwips" && tableColumnWidth.disabled) continue;
    if (value !== tablePropertiesCurrent[key]) patch[key] = value;
  }
  // The bridge requires the value and rule together whenever row height changes.
  if ("rowHeightTwips" in patch || "rowHeightRule" in patch) {
    patch.rowHeightTwips = next.rowHeightTwips;
    patch.rowHeightRule = next.rowHeightRule;
  }
  return patch;
}

function commitTableProperties() {
  if (!doc || !tablePropertiesCurrent || !tablePropertiesNode) return;
  const patch = tablePropertiesPatch();
  if (!patch) return;
  if (Object.keys(patch).length === 0) {
    return;
  }
  const applied = runNodeEdit(() =>
    doc.applyTableProperties(tablePropertiesNode, JSON.stringify(patch)),
  );
  if (applied) {
    const activeNode =
      selection && doc.inTable(selection.focus.node) ? selection.focus.node : null;
    if (activeNode == null) toggleTableProperties(false);
    else reflectTableProperties(activeNode);
    setStatus("Table properties updated");
  }
}
document.addEventListener("keydown", (event) => {
  if (
    tablePropertiesPanel.hidden ||
    (document.activeElement !== tablePropertiesBtn &&
      !tablePropertiesPanel.contains(document.activeElement))
  ) {
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    toggleTableProperties(false);
  }
});

// -- Insert table: a hover grid picker (Google-Docs style) --------------------
const GRID_ROWS = 8;
const GRID_COLS = 10;
for (let r = 1; r <= GRID_ROWS; r++) {
  for (let c = 1; c <= GRID_COLS; c++) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "gc";
    cell.dataset.r = String(r);
    cell.dataset.c = String(c);
    gridPicker.appendChild(cell);
  }
}
function highlightGrid(rows, cols) {
  for (const cell of gridPicker.children) {
    const on = Number(cell.dataset.r) <= rows && Number(cell.dataset.c) <= cols;
    cell.classList.toggle("on", on);
  }
  gridLabel.textContent = rows ? `${cols} × ${rows}` : "Insert table";
}
gridPicker.addEventListener("pointermove", (e) => {
  const cell = e.target.closest(".gc");
  if (cell) highlightGrid(Number(cell.dataset.r), Number(cell.dataset.c));
});
gridPicker.addEventListener("pointerleave", () => highlightGrid(0, 0));
gridPicker.addEventListener("pointerdown", async (e) => {
  const cell = e.target.closest(".gc");
  if (!cell || !selection || !doc) return;
  e.preventDefault();
  const rows = Number(cell.dataset.r);
  const cols = Number(cell.dataset.c);
  await runEdit(() => doc.insertTable(selection.focus.node, rows, cols), { gate: true });
  closePopover(insertTablePopover);
  focusEditorSurface();
});
const insertTablePopover = registerPopover(insertTableBtn, insertTableMenu, () => highlightGrid(0, 0));

// ---- Off-screen accessibility tree (docs/67 row 9) --------------------------
/**
 * Rebuilds the read-only, off-screen structural mirror of the document from the
 * engine's `accessibilityTree()` projection so a screen reader can read the
 * canvas (which paints pixels only, exposing no structure). Headings become
 * `h1`–`h6` (levels 7–9 clamp to `h6`), list items group into `ul`/`ol`,
 * tables become real `table`/`tr`/`td`, and everything else is a `p`. This is
 * never an editing surface — the model stays the source of truth (docs/67 Open
 * Risks). Rebuilt on the same coalesced content-change frame as the outline.
 */
function buildAccessibilityTree() {
  if (!a11yDocument) return;
  if (!doc) {
    a11yDocument.replaceChildren();
    return;
  }
  let nodes;
  try {
    nodes = JSON.parse(doc.accessibilityTree());
  } catch {
    nodes = [];
  }
  const frag = document.createDocumentFragment();
  let listEl = null;
  let listOrdered = null;
  const flushList = () => {
    if (listEl) {
      frag.appendChild(listEl);
      listEl = null;
      listOrdered = null;
    }
  };
  for (const node of Array.isArray(nodes) ? nodes : []) {
    if (node.kind === "listItem") {
      if (!listEl || listOrdered !== node.ordered) {
        flushList();
        listEl = document.createElement(node.ordered ? "ol" : "ul");
        listOrdered = node.ordered;
      }
      const li = document.createElement("li");
      li.textContent = String(node.text ?? "");
      listEl.appendChild(li);
      continue;
    }
    flushList();
    if (node.kind === "heading") {
      const level = Math.min(6, Math.max(1, Number(node.level) || 1));
      const heading = document.createElement(`h${level}`);
      heading.textContent = String(node.text ?? "");
      frag.appendChild(heading);
    } else if (node.kind === "table") {
      const table = document.createElement("table");
      const tbody = document.createElement("tbody");
      for (const row of Array.isArray(node.rows) ? node.rows : []) {
        const tr = document.createElement("tr");
        for (const cell of Array.isArray(row) ? row : []) {
          const td = document.createElement("td");
          td.textContent = String(cell ?? "");
          tr.appendChild(td);
        }
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
      frag.appendChild(table);
    } else {
      const paragraph = document.createElement("p");
      paragraph.textContent = String(node.text ?? "");
      frag.appendChild(paragraph);
    }
  }
  flushList();
  a11yDocument.replaceChildren(frag);
}

// ---- Outline panel (heading tree → scroll-to) -------------------------------
/** Rebuilds the outline list from the document's headings (no-op when hidden). */
function buildOutline() {
  if (!doc || outlinePanel.hidden) return;
  const rows = doc.documentOutline(); // "level\tnode\ttext"
  outlineBody.replaceChildren();
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = "No headings yet. Apply a Heading style to build an outline.";
    outlineBody.appendChild(empty);
    return;
  }
  for (const row of rows) {
    const tab = row.indexOf("\t");
    const tab2 = row.indexOf("\t", tab + 1);
    const level = Math.min(6, Math.max(1, Number(row.slice(0, tab)) || 1));
    const node = row.slice(tab + 1, tab2);
    const text = row.slice(tab2 + 1);
    const item = document.createElement("button");
    item.type = "button";
    item.className = `outline-item lvl-${level}`;
    item.dataset.node = node;
    item.textContent = text;
    item.title = text;
    item.addEventListener("click", () => navigateToNode(node));
    outlineBody.appendChild(item);
  }
  reflectOutlineSelection();
}

/** Keeps the outline's active row synchronized with the model-backed caret. */
function reflectOutlineSelection() {
  const activeNode = selection?.focus?.node ?? "";
  for (const item of outlineBody.querySelectorAll(".outline-item")) {
    const active = item.dataset.node === activeNode;
    item.classList.toggle("is-active", active);
    if (active) item.setAttribute("aria-current", "location");
    else item.removeAttribute("aria-current");
  }
}

/** Places the caret at the start of `node` and scrolls it into view. */
function navigateToNode(node) {
  if (!doc) return;
  selection = { anchor: { node, offset: 0 }, focus: { node, offset: 0 } };
  drawSelection();
  scrollCaretIntoView("center");
}

function toggleOutline() {
  outlinePanel.hidden = !outlinePanel.hidden;
  // Outline (left) and the review sidebar (right) are mutually exclusive so the
  // canvas is never squeezed from both sides at once. Opening the outline closes
  // the review sidebar; the reverse is enforced in renderReviewMarginItems.
  if (!outlinePanel.hidden && !reviewSidebar.hidden) toggleReview(false);
  if (!outlinePanel.hidden && !pagesPanel.hidden) {
    pagesPanel.hidden = true;
    railPages.setAttribute("aria-pressed", "false");
  }
  railOutline.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  buildOutline();
}
railOutline.addEventListener("click", toggleOutline);
outlineClose.addEventListener("click", toggleOutline);

/** Builds one thumbnail card per rendered page in the Pages navigator. */
function buildPages() {
  if (!doc || pagesPanel.hidden) return;
  pagesBody.replaceChildren();
  if (!pages.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = "No pages yet.";
    pagesBody.appendChild(empty);
    return;
  }
  // A small live render per page, so each card shows the real page layout rather
  // than a blank box. THUMB_DPI is low (a navigator preview, not readable text),
  // and each transient bitmap is freed immediately — a whole-document panel of
  // thumbnails stays a few MB even for long documents.
  const THUMB_DPI = 24;
  pages.forEach((page, index) => {
    const n = page.pageNumber;
    const card = document.createElement("button");
    card.type = "button";
    card.className = "page-thumb";
    card.dataset.page = String(n);
    card.title = `Page ${n}`;
    card.setAttribute("aria-label", `Page ${n}`);
    const box = document.createElement("span");
    box.className = "page-thumb-box";
    box.style.aspectRatio = `${page.wTwip} / ${page.hTwip}`;
    try {
      const bmp = doc.renderPage(index, THUMB_DPI);
      const canvas = document.createElement("canvas");
      canvas.className = "page-thumb-canvas";
      canvas.width = bmp.widthPx;
      canvas.height = bmp.heightPx;
      canvas
        .getContext("2d")
        .putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
      bmp.free(); // return the RGBA buffer to WASM now, not at GC.
      box.appendChild(canvas);
    } catch (err) {
      // A page that fails to render still shows a (correctly proportioned) card.
      console.error(`thumbnail page ${index}`, err);
      box.classList.add("is-empty");
    }
    const num = document.createElement("span");
    num.className = "page-thumb-num";
    num.textContent = String(n);
    card.append(box, num);
    card.addEventListener("click", () => goToPage(n));
    pagesBody.appendChild(card);
  });
  let cur = 1;
  if (selection) {
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (flat.length) cur = flat[0];
  }
  reflectPagesSelection(cur);
}

/** Scrolls page `n` into view using the single scroll owner, then highlights it. */
function goToPage(n) {
  const page = pages[n - 1];
  if (!page) return;
  const wr = page.wrap.getBoundingClientRect();
  const vp = viewportEl.getBoundingClientRect();
  viewportEl.scrollTo({ top: Math.max(0, viewportEl.scrollTop + (wr.top - vp.top) - 16), behavior: "auto" });
  reflectPagesSelection(n);
}

/** Keeps the Pages navigator's active card synchronized with the caret's page. */
function reflectPagesSelection(pageNumber) {
  if (pagesPanel.hidden) return;
  for (const card of pagesBody.querySelectorAll(".page-thumb")) {
    const active = Number(card.dataset.page) === pageNumber;
    card.classList.toggle("is-active", active);
    if (active) card.setAttribute("aria-current", "page");
    else card.removeAttribute("aria-current");
  }
}

function togglePages() {
  pagesPanel.hidden = !pagesPanel.hidden;
  // Pages, Outline (left) and the review sidebar (right) are mutually exclusive
  // so the canvas is never squeezed from both sides at once.
  if (!pagesPanel.hidden) {
    outlinePanel.hidden = true;
    railOutline.setAttribute("aria-pressed", "false");
    if (!reviewSidebar.hidden) toggleReview(false);
  }
  railPages.setAttribute("aria-pressed", String(!pagesPanel.hidden));
  buildPages();
}
railPages.addEventListener("click", togglePages);
pagesClose.addEventListener("click", togglePages);

function reviewText(value) {
  return value == null || value === "" ? "Not provided" : String(value);
}

function formatReviewDate(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

/**
 * Timestamp for a review action. Author/initials are no longer read here:
 * they come from the WASM engine's active host identity (`doc.setActiveAuthor`,
 * kept in sync with the Identity settings below), so callers pass `undefined`
 * for those arguments and let the engine fall back to it.
 */
function currentReviewTimestamp() {
  return { date: new Date().toISOString() };
}

let reviewFilter = "open";
let reviewReplyParent = null;

function focusReviewComment(comment, expand = true) {
  const anchor = comment?.anchor;
  if (!anchor?.node) return;
  reviewSidebarPreference = true;
  activeReviewCommentId = comment.id;
  if (expand) activeReviewItemId = `comment:${comment.id}`;
  selection = {
    anchor: { node: anchor.node, offset: Number(anchor.start) || 0 },
    focus: { node: anchor.node, offset: Number(anchor.end) || Number(anchor.start) || 0 },
  };
  drawSelection();
  focusEditorSurface();
  scrollReviewSelectionIntoView();
  scheduleReviewMarginRender();
}

function closeReviewPopover() {
  reviewPopover?.remove();
  reviewPopover = null;
  reviewComposerState = null;
  reviewReplyParent = null;
  scheduleReviewMarginRender();
}

// --- Inline accept/reject card (Q2) ------------------------------------------
// Hovering (or clicking) a tracked-change marker on the canvas surfaces a
// compact card — author, one-line change summary, ✔ Accept / ✗ Reject — the
// Google-Docs suggestion affordance. A pinned card (opened by click) stays until
// an outside pointerdown or Escape; a hover card follows the pointer and hides
// shortly after it leaves both the marker and the card. Accept/Reject reuse the
// same grouped/move-aware decision path as the sidebar and context menu.
let reviewInlineCard = null;
let reviewInlineCardRevisionId = null;
let reviewInlineCardPinned = false;
let reviewInlineHideTimer = 0;

/** One-line summary of a tracked change for the inline card and screen-reader
 *  announcements. */
function reviewRevisionSummary(revision) {
  const text = String(revision?.text || "");
  const quoted = text ? `“${text}”` : "";
  switch (revision?.kind) {
    case "deletion": return `Deleted ${quoted}`.trim();
    case "insertion": return `Added ${quoted}`.trim();
    case "formatting": return `Formatting change ${quoted}`.trim();
    case "move_from": case "move_to": case "move": return `Moved ${quoted}`.trim();
    case "replacement": return `Replaced ${quoted}`.trim();
    default: return `Changed ${quoted}`.trim();
  }
}

function closeReviewInlineCard() {
  clearTimeout(reviewInlineHideTimer);
  reviewInlineHideTimer = 0;
  reviewInlineCard?.remove();
  reviewInlineCard = null;
  reviewInlineCardRevisionId = null;
  reviewInlineCardPinned = false;
}

/** Hide a hover-opened card after a short grace period, so moving the pointer
 *  from the marker onto the card itself does not dismiss it. A pinned (clicked)
 *  card ignores this. */
function scheduleReviewInlineCardHide() {
  if (reviewInlineCardPinned) return;
  clearTimeout(reviewInlineHideTimer);
  reviewInlineHideTimer = window.setTimeout(() => {
    if (!reviewInlineCardPinned) closeReviewInlineCard();
  }, 220);
}

function showReviewInlineCard(revision, anchorEl, pinned) {
  if (!revision || !anchorEl) return;
  const revisionId = String(revision.id ?? "");
  clearTimeout(reviewInlineHideTimer);
  reviewInlineHideTimer = 0;
  // Re-hovering / re-clicking the marker already showing keeps the one card;
  // a click on an already-open hover card just pins it.
  if (reviewInlineCard && reviewInlineCardRevisionId === revisionId) {
    if (pinned) reviewInlineCardPinned = true;
    return;
  }
  closeReviewInlineCard();
  reviewInlineCardRevisionId = revisionId;
  reviewInlineCardPinned = !!pinned;

  const card = document.createElement("div");
  card.className = "review-inline-card";
  card.setAttribute("role", "dialog");
  card.setAttribute("aria-label", "Tracked change");
  card.tabIndex = -1;

  const head = document.createElement("div");
  head.className = "review-inline-head";
  const dot = document.createElement("span");
  dot.className = "review-inline-dot";
  dot.style.background = reviewAuthorColor(reviewAuthorKey(revision));
  const who = document.createElement("div");
  who.className = "review-inline-who";
  const author = document.createElement("strong");
  author.textContent = reviewAuthorDisplay(revision);
  const meta = document.createElement("small");
  meta.textContent = [reviewChangeTypeLabel(revision.kind), formatReviewDate(revision.date)]
    .filter(Boolean).join(" · ");
  who.append(author, meta);
  head.append(dot, who);

  const body = document.createElement("p");
  body.className = "review-inline-body";
  body.textContent = reviewRevisionSummary(revision);
  card.append(head, body);

  // Accept/Reject are edits; hide them in read-only Viewing mode, leaving the
  // card as a summary the reader can still see.
  if (reviewMode !== "viewing") {
    const bar = document.createElement("div");
    bar.className = "review-inline-bar";
    const makeBtn = (icon, label, danger, handler) => {
      const button = document.createElement("button");
      button.type = "button";
      if (danger) button.className = "danger";
      const glyph = document.createElement("span");
      glyph.className = "ms";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = icon;
      button.append(glyph, document.createTextNode(label));
      button.setAttribute("aria-label", `${label} this ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
      button.title = button.getAttribute("aria-label");
      button.addEventListener("click", async (event) => {
        event.stopPropagation();
        await handler();
      });
      return button;
    };
    const decide = async (accept) => {
      await decideContextRevision(revision, accept);
      announceReview(`${accept ? "Accepted" : "Rejected"} ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
      closeReviewInlineCard();
      focusEditorSurface();
    };
    bar.append(
      makeBtn("check", "Accept", false, () => decide(true)),
      makeBtn("close", "Reject", true, () => decide(false)),
    );
    card.appendChild(bar);
  }

  // The card is interactive: entering it cancels the hover-hide timer, leaving
  // it reschedules the hide (unless pinned).
  card.addEventListener("mouseenter", () => clearTimeout(reviewInlineHideTimer));
  card.addEventListener("mouseleave", () => scheduleReviewInlineCardHide());

  document.body.appendChild(card);
  reviewInlineCard = card;

  // Position just below the marker, clamped to the viewport.
  const markerRect = anchorEl.getBoundingClientRect();
  const left = Math.max(12, Math.min(window.innerWidth - card.offsetWidth - 12, markerRect.left));
  const below = markerRect.bottom + 6;
  const top = below + card.offsetHeight + 12 > window.innerHeight
    ? Math.max(12, markerRect.top - card.offsetHeight - 6)
    : below;
  card.style.left = `${left}px`;
  card.style.top = `${top}px`;

  // A pinned card takes focus so it is keyboard-dismissable and its buttons are
  // immediately reachable by Tab; a hover card must not steal the caret.
  if (pinned) card.focus({ preventScroll: true });
}

// --- Unified Next/Previous over comments AND changes (Q4) --------------------
// Word's Review "Previous / Next" walks every comment and tracked change in one
// document-ordered loop. `reviewNavTargets` builds that merged list (honoring
// the active Open/Resolved/All filter the same way the sidebar does) and orders
// it by on-screen position — the same page/top/left ordering the comment column
// uses — so Next/Previous visit exactly what the reader sees, in reading order.

/** Whether rect `a` sits strictly after `ref` in document (reading) order. */
function reviewRectIsAfter(a, ref) {
  if (!a || !ref) return false;
  if (a.pageNumber !== ref.pageNumber) return a.pageNumber > ref.pageNumber;
  if (Math.abs(a.top - ref.top) > 2) return a.top > ref.top;
  return a.left > ref.left + 2;
}

/** The current selection's leading/trailing on-screen positions, so navigation
 *  can skip the item the caret is already on regardless of selection direction. */
function reviewSelectionRects() {
  if (!selection) return null;
  const a = selection.anchor;
  const f = selection.focus;
  const ra = reviewRangeClientRect(a.node, a.offset, a.node, a.offset);
  const rf = reviewRangeClientRect(f.node, f.offset, f.node, f.offset);
  if (!ra && !rf) return null;
  if (!ra) return { start: rf, end: rf };
  if (!rf) return { start: ra, end: ra };
  return reviewRectIsAfter(rf, ra) ? { start: ra, end: rf } : { start: rf, end: ra };
}

/** The merged, document-ordered list of navigable review items — root comments
 *  (per filter) plus tracked changes — each with its model range and on-screen
 *  rect. Replies are represented by their root; changes are individual (their
 *  grouped sidebar card is still surfaced via the caret→card anchor index). */
function reviewNavTargets() {
  if (!doc) return [];
  const { comments, revisions } = readReviewData(doc);
  const targets = [];
  for (const comment of comments ?? []) {
    if (!comment.anchor?.node || comment.parentParaId) continue;
    if (reviewFilter !== "all") {
      const resolved = !!comment.resolved;
      if (reviewFilter === "resolved" ? !resolved : resolved) continue;
    }
    const startOffset = Number(comment.anchor.start) || 0;
    const endOffset = Number(comment.anchor.end) || startOffset;
    const rect = reviewRangeClientRect(comment.anchor.node, startOffset, comment.anchor.node, endOffset);
    if (rect) {
      targets.push({
        type: "comment",
        data: comment,
        range: { startNode: comment.anchor.node, startOffset, endNode: comment.anchor.node, endOffset },
        rect,
      });
    }
  }
  // Tracked changes are not comment-"resolved": show them under Open and All,
  // hide them only under the comment-only Resolved filter (mirrors the sidebar).
  if (reviewFilter !== "resolved") {
    for (const revision of revisions ?? []) {
      if (!String(revision.text || "").length && revision.kind !== "formatting") continue;
      const range = revisionRange(revision);
      if (!range) continue;
      const rect = reviewRangeClientRect(range.startNode, range.startOffset, range.endNode, range.endOffset);
      if (rect) targets.push({ type: "revision", data: revision, range, rect });
    }
  }
  targets.sort((a, b) =>
    a.rect.pageNumber - b.rect.pageNumber || a.rect.top - b.rect.top || a.rect.left - b.rect.left);
  return targets;
}

/** Moves the caret/selection to a navigation target, scrolls it into view, and
 *  surfaces its sidebar card (via the caret→card anchor index, which handles the
 *  grouped/move cards correctly). Announces its position + kind for AT. */
function focusReviewTarget(target, index, total) {
  reviewSidebarPreference = true;
  const { range } = target;
  selection = {
    anchor: { node: range.startNode, offset: range.startOffset },
    focus: { node: range.endNode, offset: range.endOffset },
  };
  // Resolve the active item FIRST (from the exact selected range), so the markers
  // paint with the correct item active and the scroll targets that item's own
  // marker — not the first highlight or a boundary-sharing neighbour.
  syncActiveReviewCommentToCaret(selection.focus);
  drawSelection();
  focusEditorSurface();
  scrollReviewSelectionIntoView();
  const who = reviewAuthorDisplay(target.data) || "You";
  const label = target.type === "comment"
    ? `Comment by ${who}`
    : `${reviewChangeTypeLabel(target.data.kind)} by ${who}`;
  announceReview(`${index + 1} of ${total}: ${label}`);
}

/** The index of the target the caret is currently on: an exact match to a just-
 *  navigated selection, else the item whose range contains the caret. `-1` when
 *  the caret is not on any item (a fresh navigation from arbitrary text). This
 *  is what lets Next/Previous step by list position, so two items that share a
 *  boundary (a change starting exactly where a comment ends) still advance. */
function reviewCurrentTargetIndex(targets) {
  const focus = selection?.focus;
  const anchor = selection?.anchor;
  if (!focus) return -1;
  for (let i = 0; i < targets.length; i++) {
    const r = targets[i].range;
    if (anchor && r.startNode === anchor.node && r.endNode === focus.node) {
      const forward = r.startOffset === anchor.offset && r.endOffset === focus.offset;
      const backward = r.startOffset === focus.offset && r.endOffset === anchor.offset;
      if (forward || backward) return i;
    }
  }
  const collapsed = !anchor
    || (anchor.node === focus.node && anchor.offset === focus.offset);
  for (let i = 0; i < targets.length; i++) {
    const r = targets[i].range;
    if (r.startNode === focus.node && r.endNode === focus.node
      && focus.offset >= r.startOffset && focus.offset <= r.endOffset) {
      // A collapsed caret resting exactly on a non-empty item's START boundary
      // is at the threshold *before* the item, not inside it — the reviewer has
      // not visited it yet. Report "not on any item" so Next steps onto the item
      // (not past it) and Previous walks to the one before it. A caret at the END
      // boundary, or strictly inside, is genuinely on the item, so Next advances
      // past it — which also preserves stepping between two items that share a
      // boundary (a change starting exactly where a comment ends). Without this,
      // a fresh caret at document start (offset 0, the leading edge of a comment
      // anchored there) was treated as already-on the comment, so the first Next
      // skipped straight to the next item.
      if (collapsed && focus.offset === r.startOffset && r.startOffset !== r.endOffset) {
        continue;
      }
      return i;
    }
  }
  return -1;
}

/** Next (`+1`) / Previous (`-1`) across the unified comment+change list, wrapping
 *  at the ends. When the caret already sits on an item, it steps by list index
 *  (robust to boundary-adjacent items); otherwise it lands on the nearest item
 *  after/before the caret in reading order. */
function navigateReview(direction) {
  if (!doc) return;
  const targets = reviewNavTargets();
  if (!targets.length) return;
  const current = reviewCurrentTargetIndex(targets);
  let index;
  if (current >= 0) {
    index = (current + (direction > 0 ? 1 : -1) + targets.length) % targets.length;
  } else {
    const caret = reviewSelectionRects();
    if (!caret) {
      index = direction > 0 ? 0 : targets.length - 1;
    } else if (direction > 0) {
      // First item at or after the caret, so a caret sitting exactly on an
      // item's start (e.g. document start === a comment's leading edge) lands on
      // that item rather than skipping it. "At or after" == not strictly before.
      const found = targets.findIndex((t) => !reviewRectIsAfter(caret.end, t.rect));
      index = found === -1 ? 0 : found;
    } else {
      let found = -1;
      for (let i = 0; i < targets.length; i++) {
        if (reviewRectIsAfter(caret.start, targets[i].rect)) found = i;
      }
      index = found === -1 ? targets.length - 1 : found;
    }
  }
  focusReviewTarget(targets[index], index, targets.length);
}

// --- Single-change decisions at the caret + Accept/Reject ▸ Next (Q3) --------

/** The tracked change under the caret (its focus, else its anchor), or null. */
function reviewRevisionAtCaret() {
  return reviewContextAt(selection?.focus).revision
    || reviewContextAt(selection?.anchor).revision;
}

/** Accepts (or rejects) the tracked change at the caret, using the same
 *  grouped/move-aware decision path as the sidebar and context menu. Returns
 *  whether a change was found and decided. */
async function decideReviewAtCaret(accept) {
  const revision = reviewRevisionAtCaret();
  if (!revision) {
    setStatus("Place the caret inside a tracked change to accept or reject it", "error");
    return false;
  }
  await decideContextRevision(revision, accept);
  announceReview(`${accept ? "Accepted" : "Rejected"} ${(reviewChangeTypeLabel(revision.kind) || "change").toLowerCase()}`);
  drawSelection();
  focusEditorSurface();
  return true;
}

/** Word's core review loop: accept/reject the change at the caret, then advance
 *  the caret to the next change/comment. Advances even when the caret was not on
 *  a change, so the shortcut always makes progress through the document. */
async function decideReviewAndAdvance(accept) {
  const decided = await decideReviewAtCaret(accept);
  navigateReview(1);
  return decided;
}

/**
 * Moves the caret/selection to one end of a tracked move — its source
 * (`move_from`) or destination (`move_to`) anchor — and scrolls that location
 * to the centre of the viewport. This is the keyboard-accessible "go to the
 * original / new location" navigation a move review card exposes for both ends
 * of the move (REVIEW-GAP-016), so a reviewer can jump to precisely where the
 * text came from and where it went.
 */
function navigateToReviewAnchor(anchor) {
  if (!doc || !anchor?.node) return;
  reviewSidebarPreference = true;
  const start = Number(anchor.start) || 0;
  const rawEnd = Number(anchor.end);
  const end = Number.isFinite(rawEnd) ? rawEnd : start;
  selection = {
    anchor: { node: anchor.node, offset: start },
    focus: { node: anchor.node, offset: end },
  };
  drawSelection();
  focusEditorSurface();
  scrollReviewSelectionIntoView();
}

/**
 * A keyboard-accessible navigation control for one end of a tracked move.
 * The visible secondary line is the precise page the end sits on (from the
 * same range geometry the sidebar uses to place cards), falling back to a
 * generic location label when geometry is unavailable (e.g. an off-screen or
 * not-yet-laid-out anchor). Activating it jumps the caret to that end.
 */
function reviewMoveEndButton(endLabel, anchor, fallbackLocation, action) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "review-move-end";
  const start = Number(anchor?.start) || 0;
  const rawEnd = Number(anchor?.end);
  const end = Number.isFinite(rawEnd) ? rawEnd : start;
  const rect = anchor?.node
    ? reviewRangeClientRect(anchor.node, start, anchor.node, end)
    : null;
  const locationText = rect ? `Page ${rect.pageNumber}` : fallbackLocation;
  const label = document.createElement("b");
  label.textContent = endLabel;
  const location = document.createElement("span");
  location.textContent = locationText;
  button.append(label, location);
  const description = rect ? `${action} (page ${rect.pageNumber})` : action;
  button.title = description;
  button.setAttribute("aria-label", description);
  button.disabled = !anchor?.node;
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    navigateToReviewAnchor(anchor);
  });
  return button;
}

function focusReviewRevision(revision, expand = true) {
  if (!doc || !revision?.text) return;
  reviewSidebarPreference = true;
  if (expand) activeReviewItemId = `revision:${revision.id}`;
  // Clicking a card scrolls the canvas to that change's anchor (card→canvas
  // sync, REVIEW-GAP-019). For a tracked move, the destination is the sensible
  // default landing spot; its per-end buttons still jump to source/destination.
  if (revision.kind === "move" && revision.destinationAnchor?.node) {
    const dest = revision.destinationAnchor;
    selection = {
      anchor: { node: dest.node, offset: Number(dest.start) || 0 },
      focus: { node: dest.node, offset: Number(dest.end) || Number(dest.start) || 0 },
    };
    drawSelection();
    focusEditorSurface();
    scrollReviewSelectionIntoView();
    scheduleReviewMarginRender();
    return;
  }
  const range = revisionRange(revision);
  if (range) {
    selection = {
      anchor: { node: range.startNode, offset: range.startOffset },
      focus: { node: range.endNode, offset: range.endOffset },
    };
    drawSelection();
    focusEditorSurface();
    scrollReviewSelectionIntoView();
    scheduleReviewMarginRender();
    return;
  }
  const first = selection?.focus || doc.firstPosition();
  const match = doc.findText(String(revision.text), first.node, first.offset, true, false);
  if (first.free) first.free();
  if (!match.found) { match.free(); return; }
  selection = {
    anchor: { node: match.startNode, offset: match.startOffset },
    focus: { node: match.endNode, offset: match.endOffset },
  };
  drawSelection();
  focusEditorSurface();
  scrollReviewSelectionIntoView();
  scheduleReviewMarginRender();
  match.free();
}

function toggleReview(open) {
  const show = open ?? reviewSidebar.hidden;
  reviewSidebarPreference = show;
  if (!show) {
    activeReviewItemId = null;
    activeReviewCommentId = null;
    reviewComposerState = null;
  }
  scheduleReviewMarginRender();
  // Focus management (REVIEW-GAP-023): closing the sidebar returns focus to the
  // rail toggle that owns it, so keyboard/AT users are not stranded.
  if (!show) railReview?.focus?.({ preventScroll: true });
}
reviewBtn.addEventListener("click", () => toggleReview());
railReview.addEventListener("click", () => toggleReview());
reviewClose.addEventListener("click", () => toggleReview(false));
reviewAcceptAll.addEventListener("click", async () => { if (doc) { await runEdit(() => doc.decideAllRevisions(true)); announceReview("All changes accepted"); scheduleReviewMarginRender(); } });
reviewRejectAll.addEventListener("click", async () => { if (doc) { await runEdit(() => doc.decideAllRevisions(false)); announceReview("All changes rejected"); scheduleReviewMarginRender(); } });
reviewPrevious.addEventListener("click", () => navigateReview(-1));
reviewNext.addEventListener("click", () => navigateReview(1));
// The visible mode control (`#reviewModeControl`) is a three-button segmented
// group; each button carries `data-review-mode` and is wired below.
suggestingBannerEdit.addEventListener("click", () => setReviewMode("editing"));
if (viewingBannerEdit) viewingBannerEdit.addEventListener("click", () => setReviewMode("editing"));
function openReviewComposer(parent = null) {
  if (!doc || (!parent && (!hasRange() || !selection))) return;
  reviewSidebarPreference = true;
  if (parent) {
    reviewReplyParent = parent;
    activeReviewItemId = `comment:${parent}`;
    reviewComposerState = null;
    scheduleReviewMarginRender();
    return;
  }
  // A comment can span paragraphs (Word/Docs). Order the endpoints in document
  // order: same node compares offsets directly; otherwise `selectionEdge`
  // returns the earlier/later endpoint so the start marker lands in the start
  // paragraph and the end marker in the end paragraph.
  const { anchor, focus } = selection;
  let start;
  let end;
  if (anchor.node === focus.node) {
    const forward = anchor.offset <= focus.offset;
    start = forward ? { ...anchor } : { ...focus };
    end = forward ? { ...focus } : { ...anchor };
  } else {
    const s = doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, false);
    const e = doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, true);
    start = { node: s.node, offset: s.offset };
    end = { node: e.node, offset: e.offset };
    s.free();
    e.free();
  }
  reviewComposerState = { range: { start, end } };
  activeReviewItemId = null;
  scheduleReviewMarginRender();
}
// Comment authoring/reply uses the in-sidebar composer (`openReviewComposer` →
// `reviewComposerState`, rendered by `renderReviewMarginItems`); the legacy
// hidden side-panel composer was removed (docs/81 REVIEW-GAP-018/026).
selComment.addEventListener("mousedown", (event) => event.preventDefault());
selComment.addEventListener("click", () => openReviewComposer());
for (const filter of reviewFilters) {
  filter.addEventListener("click", () => {
    reviewFilter = filter.dataset.reviewFilter;
    for (const button of reviewFilters) {
      button.setAttribute("aria-pressed", String(button === filter));
    }
    const label = { open: "Open comments", resolved: "Resolved comments", all: "All comments" }[reviewFilter] || reviewFilter;
    announceReview(`Filter: ${label}`);
    scheduleReviewMarginRender();
  });
}
for (const mode of reviewModeButtons) {
  mode.addEventListener("click", () => setReviewMode(mode.dataset.reviewMode));
}

// ---- Command palette (⌘⇧P) — fuzzy search over real editor actions ----------
const cmdPalette = document.getElementById("cmdPalette");
const cmdInput = document.getElementById("cmdInput");
const cmdList = document.getElementById("cmdList");
const searchTrigger = document.getElementById("searchTrigger");
let cmdMatches = [];
let cmdSel = 0;
let cmdReturnFocus = null;

/** Shared command descriptors for search and contextual surfaces. Dynamic
 * entries are rebuilt so document styles and availability never go stale. */
function editorCommands(context = { surface: "palette" }) {
  const fmt = (k) => () => toggleFormat(k);
  const align = (a) => () => runToolbarEdit((s, o, e, f) => doc.setAlignment(s, o, e, f, a));
  const cmds = [
    { id: "file.open", label: "Open…", group: "File", kw: "load docx odt json txt", noDoc: true, run: () => fileEl.click() },
    { id: "file.save", label: "Save", group: "File", kw: "export download", shortcut: "⌘S", run: () => saveDocument() },
    { id: "file.export.docx", label: "Export as DOCX…", group: "File", kw: "export save as word", run: () => exportDocumentAs("org.openxmlformats.wordprocessingml.document") },
    { id: "file.export.odt", label: "Export as ODT…", group: "File", kw: "export save as opendocument", run: () => exportDocumentAs("org.oasis.opendocument.text") },
    { id: "file.export.text", label: "Export as Plain text…", group: "File", kw: "export save as txt", run: () => exportDocumentAs("text.plain") },
    { id: "file.export.json", label: "Export as Normalized JSON…", group: "File", kw: "export save as json", run: () => exportDocumentAs("org.casualoffice.normalized-json") },
    { id: "file.print", label: "Print", group: "File", kw: "print pages paper hard copy pdf", shortcut: "⌘P", run: () => printDocument() },
    { id: "file.properties", label: "Document properties", group: "File", kw: "metadata title author", run: () => toggleProperties(true) },
    {
      id: "edit.undo",
      label: doc?.undoLabel ? `Undo ${doc.undoLabel}` : "Undo",
      group: "Edit",
      kw: "revert",
      shortcut: "⌘Z",
      contextMenu: true,
      enabled: !!doc?.canUndo,
      disabledReason: "Nothing to undo",
      run: () => runEdit(() => doc.undo()),
    },
    {
      id: "edit.redo",
      label: doc?.redoLabel ? `Redo ${doc.redoLabel}` : "Redo",
      group: "Edit",
      kw: "",
      shortcut: "⌘⇧Z",
      contextMenu: true,
      enabled: !!doc?.canRedo,
      disabledReason: "Nothing to redo",
      run: () => runEdit(() => doc.redo()),
    },
    {
      id: "edit.cut",
      label: "Cut",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘X",
      contextMenu: true,
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select content to cut",
      run: () => cut(),
    },
    {
      id: "edit.copy",
      label: "Copy",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘C",
      contextMenu: true,
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select content to copy",
      run: () => copySelection(),
    },
    {
      id: "edit.paste",
      label: "Paste",
      group: "Clipboard",
      kw: "",
      shortcut: "⌘V",
      contextMenu: true,
      enabled: !!doc && !!selection,
      disabledReason: "Place the caret before pasting",
      run: () => paste(),
    },
    {
      id: "edit.pasteText",
      label: "Paste without formatting",
      group: "Clipboard",
      kw: "plain text unformatted keep text only",
      shortcut: "⌘⇧V",
      contextMenu: true,
      enabled: !!doc && !!selection,
      disabledReason: "Place the caret before pasting",
      run: () => pasteAsText(),
    },
    {
      id: "edit.selectAll",
      label: "Select all",
      group: "Clipboard",
      kw: "selection document",
      shortcut: "⌘A",
      contextMenu: true,
      enabled: !!doc,
      run: () => selectAll(),
    },
    { id: "edit.find", label: "Find and replace", group: "Edit", kw: "search replace", shortcut: "⌘F", run: () => openFind() },
    { id: "format.bold", label: "Bold", group: "Format", kw: "strong", shortcut: "⌘B", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("bold") },
    { id: "format.italic", label: "Italic", group: "Format", kw: "emphasis", shortcut: "⌘I", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("italic") },
    { id: "format.underline", label: "Underline", group: "Format", kw: "", shortcut: "⌘U", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("underline") },
    { id: "format.strike", label: "Strikethrough", group: "Format", kw: "strike", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("strike") },
    { id: "format.superscript", label: "Superscript", group: "Format", kw: "raise exponent", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => superBtn.click() },
    { id: "format.subscript", label: "Subscript", group: "Format", kw: "lower", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => subBtn.click() },
    { id: "format.clear", label: "Clear direct formatting", group: "Format", kw: "reset defaults", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => clearFormattingBtn.click() },
    { id: "format.painter", label: "Format painter", group: "Format", kw: "copy formatting paint brush clone style match", shortcut: "⌘⇧C", enabled: !!selection, disabledReason: "Place the caret or select text to copy its formatting", run: () => armFormatPainter(false) },
    { id: "format.grow", label: "Increase font size", group: "Format", kw: "grow bigger larger font", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => stepFontSize(1) },
    { id: "format.shrink", label: "Decrease font size", group: "Format", kw: "shrink smaller font", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => stepFontSize(-1) },
    { id: "format.color", label: "Text color…", group: "Format", kw: "font foreground colour", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => textColorCaret.click() },
    { id: "format.highlight", label: "Highlight color…", group: "Format", kw: "marker colour", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => highlightCaret.click() },
    { id: "format.case.upper", label: "Change case: UPPERCASE", group: "Format", kw: "capitals uppercase", enabled: (context.hasRange ?? hasRange()), disabledReason: "Select text to change case", run: () => applyChangeCase("upper") },
    { id: "format.case.lower", label: "Change case: lowercase", group: "Format", kw: "lowercase", enabled: (context.hasRange ?? hasRange()), disabledReason: "Select text to change case", run: () => applyChangeCase("lower") },
    { id: "format.case.title", label: "Change case: Capitalize Each Word", group: "Format", kw: "title case capitalize", enabled: (context.hasRange ?? hasRange()), disabledReason: "Select text to change case", run: () => applyChangeCase("title") },
    { id: "format.case.sentence", label: "Change case: Sentence case", group: "Format", kw: "sentence capitalize", enabled: (context.hasRange ?? hasRange()), disabledReason: "Select text to change case", run: () => applyChangeCase("sentence") },
    { id: "format.case.toggle", label: "Change case: tOGGLE cASE", group: "Format", kw: "toggle invert case", enabled: (context.hasRange ?? hasRange()), disabledReason: "Select text to change case", run: () => applyChangeCase("toggle") },
    { id: "paragraph.align.start", label: "Align left", group: "Paragraph", kw: "", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: align("start") },
    { id: "paragraph.align.center", label: "Align center", group: "Paragraph", kw: "centre", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: align("center") },
    { id: "paragraph.align.end", label: "Align right", group: "Paragraph", kw: "", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: align("end") },
    { id: "paragraph.align.justify", label: "Justify", group: "Paragraph", kw: "align", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: align("justify") },
    { id: "paragraph.list.bullet", label: "Bullet list", group: "Paragraph", kw: "unordered", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "bullet")) },
    { id: "paragraph.list.numbered", label: "Numbered list", group: "Paragraph", kw: "ordered", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "numbered")) },
    { id: "paragraph.list.restart", label: "Restart numbering", group: "Paragraph", kw: "list restart 1", run: () => selection && runNodeEdit(() => doc.restartList(selection.focus.node)) },
    { id: "paragraph.list.continue", label: "Continue numbering", group: "Paragraph", kw: "list continue resume", run: () => selection && runNodeEdit(() => doc.continueList(selection.focus.node)) },
    { id: "paragraph.indent.increase", label: "Increase indent", group: "Paragraph", kw: "", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => adjustIndentCommand(360) },
    { id: "paragraph.indent.decrease", label: "Decrease indent", group: "Paragraph", kw: "outdent", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => adjustIndentCommand(-360) },
    { id: "insert.table", label: "Insert table (3×3)", group: "Insert", kw: "grid", enabled: !!selection, disabledReason: "Place the caret before inserting a table", run: () => selection && runEdit(() => doc.insertTable(selection.focus.node, 3, 3), { gate: true }) },
    { id: "insert.link", label: "Add or edit link", group: "Insert", kw: "hyperlink url bookmark toc", shortcut: "⌘K", enabled: context.hasRange ?? hasRange(), disabledReason: "Select text to add a link", run: () => editSelectionLink() },
    { id: "insert.bookmark", label: "Bookmark…", group: "Insert", kw: "bookmark manager navigate create rename delete go to", run: () => openBookmarkManager() },
    { id: "insert.field", label: "Field…", group: "Insert", kw: "field placeholder page number of pages date time file name author auto update", enabled: !!selection, disabledReason: "Place the caret before inserting a field", run: () => openFieldDialog() },
    { id: "insert.image", label: "Picture…", group: "Insert", kw: "image picture insert photo file png jpeg jpg gif paste", enabled: !!selection, disabledReason: "Place the caret before inserting a picture", run: () => insertImageFromFile() },
    { id: "insert.symbol", label: "Symbol…", group: "Insert", kw: "symbol special character glyph currency math greek arrow fraction diacritic omega degree unicode", enabled: !!selection, disabledReason: "Place the caret before inserting a symbol", run: () => openSymbolPicker() },
    { id: "insert.emoji", label: "Emoji…", group: "Insert", kw: "emoji emoticon smiley face reaction sticker unicode", enabled: !!selection, disabledReason: "Place the caret before inserting an emoji", run: () => openEmojiPicker() },
    ...FIELD_KINDS.map((f) => ({
      id: `insert.field.${f.kind}`,
      label: `Insert field: ${f.label}`,
      group: "Insert",
      kw: `field ${f.kw}`,
      enabled: !!selection,
      disabledReason: "Place the caret before inserting a field",
      run: () => insertFieldAtCaret(f.kind),
    })),
    { id: "view.outline", label: "Toggle outline", group: "View", kw: "headings navigation", run: () => toggleOutline() },
    { id: "view.showChanges", label: "Show changes (read-only)", group: "View", kw: "tracked changes markup deletions insertions review redline", run: () => toggleShowChanges() },
    { id: "view.zoomIn", label: "Zoom in", group: "View", kw: "", run: () => stepZoom(1) },
    { id: "view.zoomOut", label: "Zoom out", group: "View", kw: "", run: () => stepZoom(-1) },
    { id: "view.settings", label: "Settings", group: "View", kw: "theme accent dark", run: () => settingsBtn.click() },
    { id: "layout.pageSetup", label: "Page setup", group: "Layout", kw: "margins orientation paper size", run: () => togglePageSetup(true) },
    { id: "layout.paragraph", label: "Paragraph properties", group: "Layout", kw: "spacing borders shading indent", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => toggleParagraphProperties(true) },
    { id: "help.commands", label: "Keyboard shortcuts and commands", group: "Help", kw: "help shortcuts command palette", shortcut: "⌘⇧P", noDoc: true, run: () => openCmd() },
    {
      id: "review.comment",
      label: "Add comment",
      group: "Review",
      kw: "annotate note",
      shortcut: "⌘⌥M",
      enabled: context.hasRange ?? hasRange(),
      disabledReason: "Select text to comment on",
      run: () => openReviewComposer(),
    },
    { id: "review.toggle", label: "Toggle comments & suggestions", group: "Review", kw: "sidebar review panel", run: () => toggleReview() },
    { id: "review.mode.editing", label: "Editing mode", group: "Review", kw: "review mode edit", run: () => setReviewMode("editing") },
    { id: "review.mode.suggesting", label: "Suggesting mode (track changes)", group: "Review", kw: "review mode track changes suggest", run: () => setReviewMode("suggesting") },
    { id: "review.mode.viewing", label: "Viewing mode (read-only)", group: "Review", kw: "review mode view read only", run: () => setReviewMode("viewing") },
    { id: "review.next", label: "Next comment or change", group: "Review", kw: "revision suggestion comment navigate forward", run: () => navigateReview(1) },
    { id: "review.previous", label: "Previous comment or change", group: "Review", kw: "revision suggestion comment navigate back", run: () => navigateReview(-1) },
    { id: "review.acceptAtCaret", label: "Accept change at cursor", group: "Review", kw: "revision suggestion approve current", run: () => decideReviewAtCaret(true) },
    { id: "review.rejectAtCaret", label: "Reject change at cursor", group: "Review", kw: "revision suggestion discard current", run: () => decideReviewAtCaret(false) },
    { id: "review.acceptNext", label: "Accept change and move to next", group: "Review", kw: "revision suggestion approve next advance", shortcut: "⌘⌥⏎", run: () => decideReviewAndAdvance(true) },
    { id: "review.rejectNext", label: "Reject change and move to next", group: "Review", kw: "revision suggestion discard next advance", shortcut: "⌘⌥⌫", run: () => decideReviewAndAdvance(false) },
    { id: "review.acceptAll", label: "Accept all changes", group: "Review", kw: "revision suggestion approve", run: () => reviewAcceptAll.click() },
    { id: "review.rejectAll", label: "Reject all changes", group: "Review", kw: "revision suggestion discard", run: () => reviewRejectAll.click() },
  ];
  const styleTarget = currentParagraphStyleName();
  cmds.push(
    {
      id: "style.updateFromSelection",
      label: styleTarget ? `Update “${styleTarget}” to match selection` : "Update style to match selection",
      group: "Style",
      kw: "redefine modify match formatting paragraph style",
      enabled: !!styleTarget,
      disabledReason: "Place the caret in a paragraph that uses a named style",
      run: () => updateStyleFromSelection(styleTarget),
    },
    {
      id: "style.createFromSelection",
      label: "Create style from selection…",
      group: "Style",
      kw: "new define save formatting paragraph style",
      enabled: !!selection,
      disabledReason: "Place the caret in a paragraph first",
      run: () => createStyleFromSelection(),
    },
  );
  if (doc) {
    for (const name of doc.listStyles()) {
      cmds.push({
        id: `style.${name}`,
        label: `Style: ${name}`,
        group: "Style",
        kw: "paragraph heading",
        run: () => runToolbarEdit((s, o, e, f) => doc.setParagraphStyle(s, o, e, f, name)),
      });
    }
  }
  return cmds.filter((command) => doc || command.noDoc);
}

// ---- Application menus -----------------------------------------------------
// The Vellum reference supplies the two-row title/menu composition, but its
// labels all route to one prototype palette. OpenDoc renders real categorized
// menus from the same command descriptors used by the palette and context menu,
// so availability, shortcuts, mutation gates, and dynamic Undo/Redo labels stay
// consistent across every command surface.
const appMenuBar = document.getElementById("appMenuBar");
const appMenuButtons = [...appMenuBar.querySelectorAll(".app-menu-button")];
const appMenuPopover = document.getElementById("appMenuPopover");
let activeAppMenu = null;
let activeAppMenuTrigger = null;

const APP_MENU_SECTIONS = {
  file: [
    ["file.open", "file.save"],
    ["file.export.docx", "file.export.odt", "file.export.text", "file.export.json"],
    ["file.print"],
    ["file.properties"],
  ],
  edit: [
    ["edit.undo", "edit.redo"],
    ["edit.cut", "edit.copy", "edit.paste", "edit.pasteText"],
    ["edit.selectAll", "edit.find"],
    ["format.painter"],
  ],
  view: [
    ["view.outline", "review.toggle", "view.showChanges"],
    ["view.zoomIn", "view.zoomOut"],
    ["review.mode.editing", "review.mode.suggesting", "review.mode.viewing"],
  ],
  insert: [["insert.table", "insert.image", "insert.link", "insert.bookmark", "insert.field"], ["insert.symbol", "insert.emoji"], ["review.comment"]],
  format: [
    ["format.bold", "format.italic", "format.underline", "format.strike"],
    ["format.grow", "format.shrink", "format.color", "format.highlight"],
    ["format.case.upper", "format.case.lower", "format.case.title", "format.case.sentence", "format.case.toggle"],
    ["format.superscript", "format.subscript", "format.clear"],
    ["paragraph.align.start", "paragraph.align.center", "paragraph.align.end", "paragraph.align.justify"],
    ["paragraph.list.bullet", "paragraph.list.numbered"],
    ["paragraph.indent.decrease", "paragraph.indent.increase", "layout.paragraph"],
    ["style.updateFromSelection", "style.createFromSelection"],
  ],
  tools: [["layout.pageSetup", "layout.paragraph"], ["file.properties", "view.settings"]],
  help: [["help.commands"]],
};

function appMenuFocusableItems() {
  return [...appMenuPopover.querySelectorAll(".app-menu-item:not(:disabled)")];
}

function positionAppMenu(trigger) {
  const rect = trigger.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth;
  const width = appMenuPopover.offsetWidth;
  appMenuPopover.style.left = `${Math.max(8, Math.min(rect.left, viewportWidth - width - 8))}px`;
  appMenuPopover.style.top = `${rect.bottom + 4}px`;
}

function closeAppMenu({ restoreFocus = false } = {}) {
  if (appMenuPopover.hidden) return;
  appMenuPopover.hidden = true;
  for (const button of appMenuButtons) button.setAttribute("aria-expanded", "false");
  const trigger = activeAppMenuTrigger;
  activeAppMenu = null;
  activeAppMenuTrigger = null;
  if (restoreFocus) trigger?.focus({ preventScroll: true });
}

function renderAppMenu(name) {
  const byId = new Map(
    editorCommands({ surface: "menu", hasRange: hasRange() }).map((command) => [command.id, command]),
  );
  appMenuPopover.replaceChildren();
  let renderedSections = 0;
  for (const ids of APP_MENU_SECTIONS[name] ?? []) {
    const commands = ids.map((id) => byId.get(id)).filter(Boolean);
    if (!commands.length) continue;
    if (renderedSections > 0) {
      const separator = document.createElement("div");
      separator.className = "app-menu-separator";
      separator.setAttribute("role", "separator");
      appMenuPopover.appendChild(separator);
    }
    renderedSections += 1;
    for (const command of commands) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "app-menu-item";
      item.setAttribute("role", "menuitem");
      item.dataset.command = command.id;
      item.disabled = command.enabled === false;
      if (command.disabledReason) item.title = command.disabledReason;

      const label = document.createElement("span");
      label.className = "app-menu-item-label";
      label.textContent = command.label;
      const hint = document.createElement("span");
      hint.className = "app-menu-item-hint";
      hint.textContent = command.shortcut ?? "";
      item.append(label, hint);
      item.addEventListener("click", () => {
        if (command.enabled === false) return;
        const trigger = activeAppMenuTrigger;
        closeAppMenu();
        trigger?.focus({ preventScroll: true });
        command.run();
      });
      appMenuPopover.appendChild(item);
    }
  }
}

function openAppMenu(name, { focusFirst = true } = {}) {
  const trigger = appMenuButtons.find((button) => button.dataset.menu === name);
  if (!trigger) return;
  for (const button of appMenuButtons) {
    button.setAttribute("aria-expanded", String(button === trigger));
  }
  activeAppMenu = name;
  activeAppMenuTrigger = trigger;
  renderAppMenu(name);
  appMenuPopover.setAttribute("aria-label", `${trigger.textContent.trim()} menu`);
  appMenuPopover.hidden = false;
  positionAppMenu(trigger);
  if (focusFirst) appMenuFocusableItems()[0]?.focus({ preventScroll: true });
}

function adjacentAppMenuTrigger(trigger, direction) {
  const index = appMenuButtons.indexOf(trigger);
  return appMenuButtons[(index + direction + appMenuButtons.length) % appMenuButtons.length];
}

for (const button of appMenuButtons) {
  button.addEventListener("click", () => {
    if (!appMenuPopover.hidden && activeAppMenu === button.dataset.menu) {
      closeAppMenu({ restoreFocus: true });
    } else {
      openAppMenu(button.dataset.menu);
    }
  });
  button.addEventListener("pointerenter", () => {
    if (!appMenuPopover.hidden && activeAppMenu !== button.dataset.menu) {
      openAppMenu(button.dataset.menu, { focusFirst: false });
    }
  });
  button.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openAppMenu(button.dataset.menu);
    } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      const next = adjacentAppMenuTrigger(button, event.key === "ArrowRight" ? 1 : -1);
      next.focus({ preventScroll: true });
      next.scrollIntoView({ inline: "nearest", block: "nearest" });
    } else if (event.key === "Escape") {
      closeAppMenu({ restoreFocus: true });
    }
  });
}

appMenuPopover.addEventListener("keydown", (event) => {
  const items = appMenuFocusableItems();
  const index = items.indexOf(document.activeElement);
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const direction = event.key === "ArrowDown" ? 1 : -1;
    items[(index + direction + items.length) % items.length]?.focus();
  } else if (event.key === "Home" || event.key === "End") {
    event.preventDefault();
    items[event.key === "Home" ? 0 : items.length - 1]?.focus();
  } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
    event.preventDefault();
    const next = adjacentAppMenuTrigger(activeAppMenuTrigger, event.key === "ArrowRight" ? 1 : -1);
    next.scrollIntoView({ inline: "nearest", block: "nearest" });
    openAppMenu(next.dataset.menu);
  } else if (event.key === "Escape") {
    event.preventDefault();
    closeAppMenu({ restoreFocus: true });
  } else if (event.key === "Tab") {
    closeAppMenu();
  }
});

document.addEventListener("pointerdown", (event) => {
  if (
    !appMenuPopover.hidden &&
    !appMenuPopover.contains(event.target) &&
    !appMenuBar.contains(event.target)
  ) {
    closeAppMenu();
  }
});
window.addEventListener("resize", () => closeAppMenu());

function buildCommands() {
  return editorCommands({ surface: "palette" });
}

// ---- Bookmark manager ------------------------------------------------------
// A Word/Docs-style bookmark surface over the engine's create/rename/delete ops
// (bookmarkEntries/createBookmark/renameBookmark/deleteBookmark) plus the
// existing bookmarkPosition navigation. Each mutation is one undoable action
// grouped by the engine as a "Bookmark change".
const bookmarkDialog = document.getElementById("bookmarkDialog");
const bookmarkNameInput = document.getElementById("bookmarkNameInput");
const bookmarkAddForm = document.getElementById("bookmarkAddForm");
const bookmarkAddBtn = document.getElementById("bookmarkAddBtn");
const bookmarkAddNote = document.getElementById("bookmarkAddNote");
const bookmarkList = document.getElementById("bookmarkList");
const bookmarkEmpty = document.getElementById("bookmarkEmpty");
const bookmarkSortBtn = document.getElementById("bookmarkSortBtn");
const bookmarkSortLabel = document.getElementById("bookmarkSortLabel");
const bookmarkClose = document.getElementById("bookmarkClose");
const bookmarkDone = document.getElementById("bookmarkDone");
const BOOKMARK_ADD_HINT = "Select text in the document, then add a bookmark for it.";
let bookmarkSortAsc = true;
let bookmarkReturnFocus = null;

/** The bookmark name's byte length under the engine's UTF-8 255-byte bound. */
function bookmarkNameByteLength(name) {
  return new TextEncoder().encode(name).length;
}

/** A clean, user-facing validation message for `name`, or "" when valid. The
 *  engine enforces the same non-empty + 255-byte bound (its raw error name is
 *  internal vocabulary, so it is never shown directly). */
function validateBookmarkName(name) {
  if (!name) return "Enter a name for the bookmark.";
  if (bookmarkNameByteLength(name) > 255) return "That name is too long (max 255 characters).";
  return "";
}

function setBookmarkAddNote(message, isError) {
  bookmarkAddNote.textContent = message || BOOKMARK_ADD_HINT;
  bookmarkAddNote.classList.toggle("error", !!isError && !!message);
}

/** True (after surfacing the standard read-only/untracked message) if a bookmark
 *  mutation must not apply in the current review mode. Bookmark markers are a
 *  structural model change with no tracked-revision representation, so like table
 *  insertion they are blocked in Viewing (read-only) and Suggesting (untracked). */
function bookmarkMutationBlocked() {
  return blockMutationInViewing() || blockUntrackedInSuggesting();
}

/** Repaints after a bookmark edit and marks the document dirty, WITHOUT moving
 *  the caret: the engine's create/rename/delete rest the caret at a marker or the
 *  document root, but the user's selection should stay put (the manager is a
 *  side panel, not a navigation). */
async function applyBookmarkEdit(res) {
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  res.free();
  clearFindParagraphCache();
  if (newCount !== pages.length) {
    await renderAll();
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ stats: true, outline: true });
  setDocumentState("edited");
}

/** The current bookmarks as `[{ id, name }]`, parsed from the engine's
 *  `"{id}\t{name}"` entries and ordered by the active sort direction. */
function bookmarkEntries() {
  if (!doc) return [];
  const entries = (doc.bookmarkEntries?.() || []).map((row) => {
    const tab = row.indexOf("\t");
    return { id: row.slice(0, tab), name: row.slice(tab + 1) };
  });
  entries.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  if (!bookmarkSortAsc) entries.reverse();
  return entries;
}

function refreshBookmarkList() {
  const entries = bookmarkEntries();
  bookmarkList.replaceChildren();
  bookmarkEmpty.hidden = entries.length > 0;
  bookmarkList.hidden = entries.length === 0;
  for (const { id, name } of entries) bookmarkList.append(bookmarkRow(id, name));
}

/** One bookmark row: a Go-to button (name) plus Rename and Delete actions. */
function bookmarkRow(id, name) {
  const li = document.createElement("li");
  li.className = "bookmark-row";
  li.dataset.id = id;

  const goto = document.createElement("button");
  goto.type = "button";
  goto.className = "bookmark-goto";
  goto.title = `Go to “${name}”`;
  goto.innerHTML = `<span class="ms" aria-hidden="true">arrow_forward</span><span class="bookmark-name"></span>`;
  goto.querySelector(".bookmark-name").textContent = name;
  goto.addEventListener("click", () => gotoBookmark(name));

  const actions = document.createElement("div");
  actions.className = "bookmark-row-actions";

  const rename = document.createElement("button");
  rename.type = "button";
  rename.className = "bookmark-action";
  rename.title = "Rename";
  rename.setAttribute("aria-label", `Rename “${name}”`);
  rename.innerHTML = `<span class="ms" aria-hidden="true">edit</span>`;
  rename.addEventListener("click", () => beginBookmarkRename(li, id, name));

  const del = document.createElement("button");
  del.type = "button";
  del.className = "bookmark-action danger";
  del.title = "Delete";
  del.setAttribute("aria-label", `Delete “${name}”`);
  del.innerHTML = `<span class="ms" aria-hidden="true">delete</span>`;
  del.addEventListener("click", () => deleteBookmark(id, name));

  actions.append(rename, del);
  li.append(goto, actions);
  return li;
}

/** Navigates to `name` (reusing the existing bookmarkPosition resolver) and
 *  places the caret there, closing the dialog so focus returns to the canvas. */
function gotoBookmark(name) {
  if (!doc) return;
  const encoded = doc.bookmarkPosition(name);
  const [node, offset] = encoded.split("\t");
  if (!node || !offset) {
    setStatus(`Bookmark “${name}” was not found`, "error");
    return;
  }
  closeBookmarkDialog();
  navToPosition({ node, offset: Number(offset) }, false);
}

/** Swaps a row's name for an inline editor. Enter confirms via renameBookmark;
 *  Esc (or blur) cancels and restores the row. */
function beginBookmarkRename(li, id, currentName) {
  if (li.classList.contains("editing")) return;
  li.classList.add("editing");
  const goto = li.querySelector(".bookmark-goto");
  const input = document.createElement("input");
  input.type = "text";
  input.className = "bookmark-rename-input";
  input.maxLength = 255;
  input.value = currentName;
  input.setAttribute("aria-label", "New bookmark name");
  li.replaceChild(input, goto);
  input.focus();
  input.select();

  let settled = false;
  const cancel = () => {
    if (settled) return;
    settled = true;
    refreshBookmarkList();
  };
  const commit = () => {
    if (settled) return;
    const next = input.value.trim();
    if (next === currentName) return cancel();
    const problem = validateBookmarkName(next);
    if (problem) {
      setStatus(problem, "error");
      input.focus();
      input.select();
      return;
    }
    if (bookmarkMutationBlocked()) {
      settled = true;
      closeBookmarkDialog();
      return;
    }
    settled = true;
    renameBookmark(id, next);
  };

  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  });
  input.addEventListener("blur", cancel);
}

function createBookmarkFromSelection() {
  if (!doc) return;
  const name = bookmarkNameInput.value.trim();
  const problem = validateBookmarkName(name);
  if (problem) {
    setBookmarkAddNote(problem, true);
    bookmarkNameInput.focus();
    return;
  }
  if (!hasRange()) {
    setBookmarkAddNote("Select some text in the document first.", true);
    return;
  }
  const ends = selEndpoints();
  if (!ends) return;
  if (bookmarkMutationBlocked()) {
    closeBookmarkDialog();
    return;
  }
  let res;
  try {
    res = doc.createBookmark(ends[0], ends[1], ends[2], ends[3], name);
  } catch (err) {
    console.warn("createBookmark ignored:", err?.message ?? err);
    setBookmarkAddNote("That bookmark couldn't be created for this selection.", true);
    return;
  }
  applyBookmarkEdit(res).then(() => {
    bookmarkNameInput.value = "";
    setBookmarkAddNote("", false);
    refreshBookmarkList();
    setStatus(`Bookmark “${name}” added`);
    bookmarkNameInput.focus();
  });
}

function renameBookmark(id, name) {
  if (!doc) return;
  let res;
  try {
    res = doc.renameBookmark(id, name);
  } catch (err) {
    console.warn("renameBookmark ignored:", err?.message ?? err);
    setStatus("That bookmark couldn't be renamed", "error");
    refreshBookmarkList();
    return;
  }
  applyBookmarkEdit(res).then(() => {
    refreshBookmarkList();
    setStatus(`Bookmark renamed to “${name}”`);
  });
}

function deleteBookmark(id, name) {
  if (!doc) return;
  if (bookmarkMutationBlocked()) {
    closeBookmarkDialog();
    return;
  }
  let res;
  try {
    res = doc.deleteBookmark(id);
  } catch (err) {
    console.warn("deleteBookmark ignored:", err?.message ?? err);
    setStatus("That bookmark couldn't be deleted", "error");
    refreshBookmarkList();
    return;
  }
  applyBookmarkEdit(res).then(() => {
    refreshBookmarkList();
    setStatus(`Bookmark “${name}” deleted`);
  });
}

function openBookmarkManager() {
  if (!doc || !bookmarkDialog) return;
  bookmarkReturnFocus = document.activeElement;
  bookmarkNameInput.value = "";
  setBookmarkAddNote("", false);
  refreshBookmarkList();
  bookmarkDialog.hidden = false;
  queueMicrotask(() => bookmarkNameInput.focus());
}

function closeBookmarkDialog() {
  if (!bookmarkDialog || bookmarkDialog.hidden) return;
  bookmarkDialog.hidden = true;
  const returnTo = bookmarkReturnFocus;
  bookmarkReturnFocus = null;
  if (returnTo && typeof returnTo.focus === "function" && document.contains(returnTo)) {
    returnTo.focus({ preventScroll: true });
  } else {
    focusEditorSurface();
  }
}

if (bookmarkDialog) {
  bookmarkAddForm.addEventListener("submit", (event) => {
    event.preventDefault();
    createBookmarkFromSelection();
  });
  bookmarkNameInput.addEventListener("input", () => {
    if (bookmarkAddNote.classList.contains("error")) setBookmarkAddNote("", false);
  });
  bookmarkSortBtn.addEventListener("click", () => {
    bookmarkSortAsc = !bookmarkSortAsc;
    bookmarkSortLabel.textContent = bookmarkSortAsc ? "A–Z" : "Z–A";
    refreshBookmarkList();
  });
  bookmarkClose.addEventListener("click", () => closeBookmarkDialog());
  bookmarkDone.addEventListener("click", () => closeBookmarkDialog());
  bookmarkDialog.addEventListener("click", (event) => {
    if (event.target === bookmarkDialog) closeBookmarkDialog();
  });
  bookmarkDialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeBookmarkDialog();
    } else if (event.key === "Tab") {
      trapModalFocus(event, bookmarkDialog);
    }
  });
}

// ---- Insert field ----------------------------------------------------------
// A Word/Docs-style field inserter over the engine's insertField op (one
// undoable "Field change"). PAGE/NUMPAGES recompute at pagination and carry no
// cached text; the clock/context kinds (date/time/filename/author) cache an
// already-formatted string the HOST computes here, because the engine reads no
// clock or filesystem (see the insertField binding, crates/casual-doc-wasm).
// The insert mirrors the bookmark/table structural inserts: blocked in Viewing
// (read-only) and Suggesting (no tracked-revision representation yet).
const FIELD_KINDS = [
  { kind: "page", label: "Page number", icon: "tag", kw: "page number current", note: "Current page number" },
  { kind: "numpages", label: "Number of pages", icon: "tag", kw: "number of pages count total", note: "Total page count" },
  { kind: "date", label: "Date", icon: "calendar_today", kw: "date today", note: "Today’s date" },
  { kind: "time", label: "Time", icon: "schedule", kw: "time clock now", note: "Current time" },
  { kind: "filename", label: "File name", icon: "description", kw: "file name filename document", note: "This document’s file name" },
  { kind: "author", label: "Author", icon: "person", kw: "author name creator", note: "The active author" },
];
const FIELD_LABELS = new Map(FIELD_KINDS.map((f) => [f.kind, f.label]));

/** The already-formatted display text a cached field kind shows. PAGE/NUMPAGES
 *  recompute at pagination and take no cached text (undefined → the binding's
 *  `None`). The engine reads no clock or filesystem, so date/time are formatted
 *  with the locale-default medium `Intl.DateTimeFormat`, filename is the editor's
 *  current document name, and author is the active review author. */
function fieldResultText(kind) {
  switch (kind) {
    case "date":
      return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date());
    case "time":
      return new Intl.DateTimeFormat(undefined, { timeStyle: "medium" }).format(new Date());
    case "filename":
      return currentName;
    case "author":
      return settings.authorName.trim() || "You";
    default:
      return undefined; // page / numpages: engine recomputes, no cached text
  }
}

/** Inserts a common field at the caret as a single undoable "Field change".
 *  Fails closed exactly like the bookmark/table inserts (`blockMutationInViewing`
 *  + `blockUntrackedInSuggesting`), then applies the EditResult through the shared
 *  caret-position path so the field renders inline, the caret lands after it, and
 *  Undo/Redo treats it as one action. */
async function insertFieldAtCaret(kind) {
  if (!doc || !selection) {
    setStatus("Place the caret before inserting a field", "error");
    focusEditorSurface();
    return;
  }
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (blockUntrackedInSuggesting()) return;
  const { node, offset } = selection.focus;
  let res;
  try {
    res = doc.insertField(node, offset, kind, fieldResultText(kind));
  } catch (err) {
    console.warn("insertField ignored:", err?.message ?? err);
    setStatus("A field can’t be inserted at this position", "error");
    focusEditorSurface();
    return;
  }
  await applyEditResult(res);
  setDocumentState("edited");
  setStatus(`Inserted ${FIELD_LABELS.get(kind) ?? "field"}`);
  focusEditorSurface();
}

// ---- Insert picture ----------------------------------------------------------
// The engine owns no image codec (docs/85 §Q8), so the host decodes the image to
// bytes + natural pixel size and hands them to the `insertImage` op. One EMU is
// 1/914400in; at 96dpi a CSS px is 9525 EMU. A wide image is scaled down to fit
// the text column, preserving aspect.
const EMU_PER_PX = 9525;
const MAX_IMAGE_WIDTH_EMU = 6 * 914_400; // ~6in, a sane default display width

const INSERTABLE_IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/bmp",
  "image/tiff",
  "image/webp",
]);

/** Decodes a File/Blob to `{ bytes, widthPx, heightPx, mime }` via the browser. */
async function decodeImageBlob(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const bitmap = await createImageBitmap(blob);
  const widthPx = bitmap.width;
  const heightPx = bitmap.height;
  bitmap.close?.();
  return { bytes, widthPx, heightPx, mime: blob.type };
}

/** Inserts an already-decoded image at the caret as one undoable action, gated
 *  like the other object edits (read-only in Viewing, blocked in Suggesting). */
async function insertImageAtCaret(bytes, widthPx, heightPx, mime) {
  if (!doc || !selection) {
    setStatus("Place the caret to insert a picture", "error");
    focusEditorSurface();
    return;
  }
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (blockUntrackedInSuggesting()) return;
  let widthEmu = Math.max(1, Math.round(widthPx * EMU_PER_PX));
  let heightEmu = Math.max(1, Math.round(heightPx * EMU_PER_PX));
  if (widthEmu > MAX_IMAGE_WIDTH_EMU) {
    heightEmu = Math.round((heightEmu * MAX_IMAGE_WIDTH_EMU) / widthEmu);
    widthEmu = MAX_IMAGE_WIDTH_EMU;
  }
  const { node, offset } = selection.focus;
  let res;
  try {
    res = doc.insertImage(node, offset, bytes, widthEmu, heightEmu, mime);
  } catch (err) {
    console.warn("insertImage ignored:", err?.message ?? err);
    setStatus("This picture can’t be inserted here", "error");
    focusEditorSurface();
    return;
  }
  await applyEditResult(res);
  setDocumentState("edited");
  setStatus("Picture inserted");
  focusEditorSurface();
}

/** Decodes a File/Blob (a picked file or a pasted image) and inserts it. */
async function insertImageFromBlob(blob) {
  if (!doc) return;
  if (!INSERTABLE_IMAGE_TYPES.has((blob.type || "").toLowerCase())) {
    setStatus("That image format isn’t supported", "error");
    return;
  }
  try {
    const decoded = await decodeImageBlob(blob);
    await insertImageAtCaret(decoded.bytes, decoded.widthPx, decoded.heightPx, decoded.mime);
  } catch (err) {
    console.warn("image decode failed:", err);
    setStatus("Could not read that image", "error");
  }
}

/** Insert ▸ Picture: opens a file picker and inserts the chosen image. */
function insertImageFromFile() {
  if (!doc || !selection) {
    setStatus("Place the caret to insert a picture", "error");
    focusEditorSurface();
    return;
  }
  if (blockMutationInViewing()) return;
  const input = document.createElement("input");
  input.type = "file";
  input.accept = [...INSERTABLE_IMAGE_TYPES].join(",");
  input.addEventListener("change", () => {
    const file = input.files?.[0];
    if (file) void insertImageFromBlob(file);
  });
  input.click();
}

const fieldDialog = document.getElementById("fieldDialog");
const fieldList = document.getElementById("fieldList");
const fieldClose = document.getElementById("fieldClose");
const fieldCancel = document.getElementById("fieldCancel");
let fieldReturnFocus = null;

function fieldChoiceButtons() {
  return fieldList ? [...fieldList.querySelectorAll(".field-choice")] : [];
}

/** Opens the field picker with the caret's position captured; the first choice
 *  is focused for keyboard use. Requires a live caret (matches insert.table). */
function openFieldDialog() {
  if (!doc || !fieldDialog) return;
  if (!selection) {
    setStatus("Place the caret before inserting a field", "error");
    focusEditorSurface();
    return;
  }
  fieldReturnFocus = document.activeElement;
  fieldDialog.hidden = false;
  queueMicrotask(() => fieldChoiceButtons()[0]?.focus());
}

function closeFieldDialog() {
  if (!fieldDialog || fieldDialog.hidden) return;
  fieldDialog.hidden = true;
  const returnTo = fieldReturnFocus;
  fieldReturnFocus = null;
  if (returnTo && typeof returnTo.focus === "function" && document.contains(returnTo)) {
    returnTo.focus({ preventScroll: true });
  } else {
    focusEditorSurface();
  }
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
  fieldDialog.addEventListener("click", (event) => {
    if (event.target === fieldDialog) closeFieldDialog();
  });
  fieldDialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeFieldDialog();
    } else if (event.key === "Tab") {
      trapModalFocus(event, fieldDialog);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const items = fieldChoiceButtons();
      const index = items.indexOf(document.activeElement);
      const dir = event.key === "ArrowDown" ? 1 : -1;
      items[(index + dir + items.length) % items.length]?.focus();
    }
  });
}

// ---- Insert ▸ Symbol / Emoji pickers ---------------------------------------
// Word's Insert ▸ Symbol and Docs' Insert ▸ Special characters / emoji, built to
// that standard: a categorized grid of curated glyphs. Clicking a glyph inserts
// it at the caret and KEEPS the dialog open (Word/Docs behavior) so the user can
// add several; Done / Esc / the close button dismiss. Insertion reuses the same
// gated, tracked, undoable text path as paste (`pasteText` → `insertPlainTextAs`
// with the "paste" HistoryKind, which never coalesces): each glyph is ONE undo,
// fails closed read-only in Viewing, and routes through the suggestion path in
// Suggesting — no engine change, an ordinary text edit. Curated arrays (not a
// Unicode DB); each entry carries a name for its tooltip and for keyword search.
const SYMBOL_GROUPS = [
  {
    name: "Currency",
    items: [
      { c: "€", n: "Euro" }, { c: "£", n: "Pound sterling" }, { c: "¥", n: "Yen" },
      { c: "¢", n: "Cent" }, { c: "₹", n: "Indian rupee" }, { c: "₩", n: "Won" },
      { c: "₽", n: "Ruble" }, { c: "₺", n: "Turkish lira" }, { c: "₴", n: "Hryvnia" },
      { c: "₦", n: "Naira" }, { c: "฿", n: "Baht" }, { c: "₫", n: "Dong" },
      { c: "₪", n: "Shekel" }, { c: "₱", n: "Peso" }, { c: "$", n: "Dollar" }, { c: "¤", n: "Currency sign" },
    ],
  },
  {
    name: "Math",
    items: [
      { c: "±", n: "Plus-minus" }, { c: "×", n: "Multiplication" }, { c: "÷", n: "Division" },
      { c: "≠", n: "Not equal to" }, { c: "≤", n: "Less than or equal" }, { c: "≥", n: "Greater than or equal" },
      { c: "≈", n: "Almost equal to" }, { c: "≡", n: "Identical to" }, { c: "∞", n: "Infinity" },
      { c: "∑", n: "Summation" }, { c: "∏", n: "Product" }, { c: "√", n: "Square root" },
      { c: "∫", n: "Integral" }, { c: "∂", n: "Partial differential" }, { c: "∆", n: "Increment (delta)" },
      { c: "∇", n: "Nabla" }, { c: "∈", n: "Element of" }, { c: "∉", n: "Not an element of" },
      { c: "∅", n: "Empty set" }, { c: "∩", n: "Intersection" }, { c: "∪", n: "Union" },
      { c: "⊂", n: "Subset of" }, { c: "⊃", n: "Superset of" }, { c: "°", n: "Degree" },
      { c: "µ", n: "Micro sign" }, { c: "∝", n: "Proportional to" }, { c: "∴", n: "Therefore" },
      { c: "∵", n: "Because" }, { c: "¬", n: "Not sign" }, { c: "∧", n: "Logical and" },
      { c: "∨", n: "Logical or" }, { c: "‰", n: "Per mille" }, { c: "′", n: "Prime" }, { c: "″", n: "Double prime" },
    ],
  },
  {
    name: "Arrows",
    items: [
      { c: "←", n: "Leftwards arrow" }, { c: "→", n: "Rightwards arrow" }, { c: "↑", n: "Upwards arrow" },
      { c: "↓", n: "Downwards arrow" }, { c: "↔", n: "Left-right arrow" }, { c: "↕", n: "Up-down arrow" },
      { c: "↖", n: "Up-left arrow" }, { c: "↗", n: "Up-right arrow" }, { c: "↘", n: "Down-right arrow" },
      { c: "↙", n: "Down-left arrow" }, { c: "⇐", n: "Leftwards double arrow" }, { c: "⇒", n: "Rightwards double arrow" },
      { c: "⇑", n: "Upwards double arrow" }, { c: "⇓", n: "Downwards double arrow" }, { c: "⇔", n: "Left-right double arrow" },
      { c: "↩", n: "Leftwards arrow with hook" }, { c: "↪", n: "Rightwards arrow with hook" }, { c: "⟶", n: "Long rightwards arrow" },
      { c: "➔", n: "Heavy round-tipped rightwards arrow" }, { c: "↺", n: "Anticlockwise open circle arrow" }, { c: "↻", n: "Clockwise open circle arrow" },
    ],
  },
  {
    name: "Punctuation",
    items: [
      { c: "–", n: "En dash" }, { c: "—", n: "Em dash" }, { c: "…", n: "Horizontal ellipsis" },
      { c: "‘", n: "Left single quotation mark" }, { c: "’", n: "Right single quotation mark" },
      { c: "“", n: "Left double quotation mark" }, { c: "”", n: "Right double quotation mark" },
      { c: "‚", n: "Single low-9 quotation mark" }, { c: "„", n: "Double low-9 quotation mark" },
      { c: "«", n: "Left-pointing double angle quotation" }, { c: "»", n: "Right-pointing double angle quotation" },
      { c: "‹", n: "Single left-pointing angle quotation" }, { c: "›", n: "Single right-pointing angle quotation" },
      { c: "†", n: "Dagger" }, { c: "‡", n: "Double dagger" }, { c: "§", n: "Section sign" },
      { c: "¶", n: "Pilcrow (paragraph)" }, { c: "•", n: "Bullet" }, { c: "·", n: "Middle dot" },
      { c: "※", n: "Reference mark" }, { c: "¡", n: "Inverted exclamation mark" }, { c: "¿", n: "Inverted question mark" },
    ],
  },
  {
    name: "Latin",
    items: [
      { c: "á", n: "a acute" }, { c: "à", n: "a grave" }, { c: "â", n: "a circumflex" }, { c: "ä", n: "a diaeresis" },
      { c: "ã", n: "a tilde" }, { c: "å", n: "a ring" }, { c: "é", n: "e acute" }, { c: "è", n: "e grave" },
      { c: "ê", n: "e circumflex" }, { c: "ë", n: "e diaeresis" }, { c: "í", n: "i acute" }, { c: "î", n: "i circumflex" },
      { c: "ï", n: "i diaeresis" }, { c: "ó", n: "o acute" }, { c: "ô", n: "o circumflex" }, { c: "ö", n: "o diaeresis" },
      { c: "õ", n: "o tilde" }, { c: "ø", n: "o stroke" }, { c: "ú", n: "u acute" }, { c: "û", n: "u circumflex" },
      { c: "ü", n: "u diaeresis" }, { c: "ñ", n: "n tilde" }, { c: "ç", n: "c cedilla" }, { c: "ß", n: "Sharp s (eszett)" },
      { c: "æ", n: "ae ligature" }, { c: "œ", n: "oe ligature" }, { c: "ā", n: "a macron" }, { c: "ē", n: "e macron" },
      { c: "ī", n: "i macron" }, { c: "ō", n: "o macron" }, { c: "ū", n: "u macron" }, { c: "ý", n: "y acute" },
    ],
  },
  {
    name: "Greek",
    items: [
      { c: "α", n: "alpha" }, { c: "β", n: "beta" }, { c: "γ", n: "gamma" }, { c: "δ", n: "delta" },
      { c: "ε", n: "epsilon" }, { c: "ζ", n: "zeta" }, { c: "η", n: "eta" }, { c: "θ", n: "theta" },
      { c: "ι", n: "iota" }, { c: "κ", n: "kappa" }, { c: "λ", n: "lambda" }, { c: "μ", n: "mu" },
      { c: "ν", n: "nu" }, { c: "ξ", n: "xi" }, { c: "π", n: "pi" }, { c: "ρ", n: "rho" },
      { c: "σ", n: "sigma" }, { c: "τ", n: "tau" }, { c: "φ", n: "phi" }, { c: "χ", n: "chi" },
      { c: "ψ", n: "psi" }, { c: "ω", n: "omega" }, { c: "Γ", n: "Gamma capital" }, { c: "Δ", n: "Delta capital" },
      { c: "Θ", n: "Theta capital" }, { c: "Λ", n: "Lambda capital" }, { c: "Ξ", n: "Xi capital" }, { c: "Π", n: "Pi capital" },
      { c: "Σ", n: "Sigma capital" }, { c: "Φ", n: "Phi capital" }, { c: "Ψ", n: "Psi capital" }, { c: "Ω", n: "Omega capital" },
    ],
  },
  {
    name: "Fractions",
    items: [
      { c: "½", n: "One half" }, { c: "⅓", n: "One third" }, { c: "⅔", n: "Two thirds" }, { c: "¼", n: "One quarter" },
      { c: "¾", n: "Three quarters" }, { c: "⅕", n: "One fifth" }, { c: "⅖", n: "Two fifths" }, { c: "⅗", n: "Three fifths" },
      { c: "⅘", n: "Four fifths" }, { c: "⅙", n: "One sixth" }, { c: "⅚", n: "Five sixths" }, { c: "⅛", n: "One eighth" },
      { c: "⅜", n: "Three eighths" }, { c: "⅝", n: "Five eighths" }, { c: "⅞", n: "Seven eighths" }, { c: "№", n: "Numero sign" },
      { c: "™", n: "Trade mark" }, { c: "©", n: "Copyright" }, { c: "®", n: "Registered" }, { c: "℅", n: "Care of" }, { c: "ℓ", n: "Script small l (litre)" },
    ],
  },
];

const EMOJI_GROUPS = [
  {
    name: "Smileys",
    icon: "😀",
    items: [
      { c: "😀", n: "grinning face" }, { c: "😃", n: "smiley open mouth" }, { c: "😄", n: "smiling eyes" },
      { c: "😁", n: "beaming grin" }, { c: "😆", n: "laughing squint" }, { c: "😅", n: "sweat smile" },
      { c: "😂", n: "tears of joy laughing" }, { c: "🤣", n: "rolling on floor laughing" }, { c: "😊", n: "blush smiling" },
      { c: "🙂", n: "slight smile" }, { c: "🙃", n: "upside down" }, { c: "😉", n: "wink" },
      { c: "😌", n: "relieved" }, { c: "😍", n: "heart eyes love" }, { c: "🥰", n: "smiling hearts love" },
      { c: "😘", n: "blowing kiss" }, { c: "😗", n: "kissing" }, { c: "😋", n: "yum savoring" },
      { c: "😛", n: "tongue out" }, { c: "😜", n: "wink tongue" }, { c: "🤪", n: "zany goofy" },
      { c: "🤨", n: "raised eyebrow skeptical" }, { c: "😐", n: "neutral face" }, { c: "😑", n: "expressionless" },
      { c: "😶", n: "no mouth" }, { c: "🙄", n: "rolling eyes" }, { c: "😏", n: "smirk" },
      { c: "😴", n: "sleeping" }, { c: "😷", n: "mask sick" }, { c: "🤒", n: "thermometer ill" },
      { c: "🤗", n: "hugging" }, { c: "🤔", n: "thinking" }, { c: "🤯", n: "mind blown exploding head" },
      { c: "🥳", n: "party face celebrate" }, { c: "😎", n: "sunglasses cool" }, { c: "🤓", n: "nerd" },
      { c: "😕", n: "confused" }, { c: "🙁", n: "slight frown" }, { c: "😢", n: "crying" },
      { c: "😭", n: "sobbing loud cry" }, { c: "😱", n: "screaming fear" }, { c: "😳", n: "flushed" },
      { c: "🥺", n: "pleading puppy eyes" }, { c: "😡", n: "angry pouting" }, { c: "😠", n: "angry" },
    ],
  },
  {
    name: "People",
    icon: "👋",
    items: [
      { c: "👋", n: "waving hand hello" }, { c: "🤚", n: "raised back of hand" }, { c: "✋", n: "raised hand stop" },
      { c: "👌", n: "ok hand" }, { c: "🤏", n: "pinching hand small" }, { c: "✌️", n: "victory peace" },
      { c: "🤞", n: "crossed fingers luck" }, { c: "🤟", n: "love you gesture" }, { c: "🤘", n: "rock on horns" },
      { c: "🤙", n: "call me hand" }, { c: "👈", n: "backhand pointing left" }, { c: "👉", n: "backhand pointing right" },
      { c: "👆", n: "backhand pointing up" }, { c: "👇", n: "backhand pointing down" }, { c: "☝️", n: "index pointing up" },
      { c: "👍", n: "thumbs up like" }, { c: "👎", n: "thumbs down dislike" }, { c: "✊", n: "raised fist" },
      { c: "👊", n: "fist bump" }, { c: "👏", n: "clapping applause" }, { c: "🙌", n: "raising hands celebrate" },
      { c: "🙏", n: "folded hands thanks pray" }, { c: "💪", n: "flexed biceps strong" }, { c: "👀", n: "eyes looking" },
      { c: "👶", n: "baby" }, { c: "🧑", n: "person adult" }, { c: "👨", n: "man" },
      { c: "👩", n: "woman" }, { c: "👴", n: "old man" }, { c: "👵", n: "old woman" },
    ],
  },
  {
    name: "Nature",
    icon: "🐻",
    items: [
      { c: "🐶", n: "dog face" }, { c: "🐱", n: "cat face" }, { c: "🐭", n: "mouse" }, { c: "🐹", n: "hamster" },
      { c: "🐰", n: "rabbit" }, { c: "🦊", n: "fox" }, { c: "🐻", n: "bear" }, { c: "🐼", n: "panda" },
      { c: "🐨", n: "koala" }, { c: "🐯", n: "tiger face" }, { c: "🦁", n: "lion" }, { c: "🐮", n: "cow face" },
      { c: "🐷", n: "pig face" }, { c: "🐸", n: "frog" }, { c: "🐵", n: "monkey face" }, { c: "🐔", n: "chicken" },
      { c: "🐧", n: "penguin" }, { c: "🐦", n: "bird" }, { c: "🦆", n: "duck" }, { c: "🦉", n: "owl" },
      { c: "🐴", n: "horse face" }, { c: "🦄", n: "unicorn" }, { c: "🐝", n: "bee honeybee" }, { c: "🦋", n: "butterfly" },
      { c: "🐌", n: "snail" }, { c: "🐞", n: "lady beetle bug" }, { c: "🐢", n: "turtle" }, { c: "🐍", n: "snake" },
      { c: "🐙", n: "octopus" }, { c: "🐠", n: "tropical fish" }, { c: "🐬", n: "dolphin" }, { c: "🐳", n: "whale" },
      { c: "🌵", n: "cactus" }, { c: "🌲", n: "evergreen tree" }, { c: "🌳", n: "deciduous tree" }, { c: "🌴", n: "palm tree" },
      { c: "🍀", n: "four leaf clover luck" }, { c: "🍁", n: "maple leaf" }, { c: "🌸", n: "cherry blossom" }, { c: "🌻", n: "sunflower" },
      { c: "🌹", n: "rose" }, { c: "🌷", n: "tulip" }, { c: "🌼", n: "blossom flower" }, { c: "🍄", n: "mushroom" },
    ],
  },
  {
    name: "Food",
    icon: "🍎",
    items: [
      { c: "🍎", n: "red apple" }, { c: "🍐", n: "pear" }, { c: "🍊", n: "tangerine orange" }, { c: "🍋", n: "lemon" },
      { c: "🍌", n: "banana" }, { c: "🍉", n: "watermelon" }, { c: "🍇", n: "grapes" }, { c: "🍓", n: "strawberry" },
      { c: "🍒", n: "cherries" }, { c: "🍑", n: "peach" }, { c: "🥭", n: "mango" }, { c: "🍍", n: "pineapple" },
      { c: "🥝", n: "kiwi" }, { c: "🍅", n: "tomato" }, { c: "🥑", n: "avocado" }, { c: "🥦", n: "broccoli" },
      { c: "🥕", n: "carrot" }, { c: "🌽", n: "corn" }, { c: "🥔", n: "potato" }, { c: "🍞", n: "bread" },
      { c: "🧀", n: "cheese" }, { c: "🥚", n: "egg" }, { c: "🍳", n: "fried egg cooking" }, { c: "🥞", n: "pancakes" },
      { c: "🍔", n: "hamburger" }, { c: "🍟", n: "french fries" }, { c: "🍕", n: "pizza" }, { c: "🌭", n: "hot dog" },
      { c: "🌮", n: "taco" }, { c: "🌯", n: "burrito" }, { c: "🥗", n: "green salad" }, { c: "🍜", n: "steaming noodle bowl ramen" },
      { c: "🍝", n: "spaghetti pasta" }, { c: "🍣", n: "sushi" }, { c: "🍦", n: "soft ice cream" }, { c: "🍰", n: "shortcake slice" },
      { c: "🎂", n: "birthday cake" }, { c: "🍫", n: "chocolate bar" }, { c: "🍬", n: "candy" }, { c: "🍭", n: "lollipop" },
      { c: "☕", n: "hot coffee tea" }, { c: "🍵", n: "teacup" }, { c: "🍺", n: "beer mug" }, { c: "🍷", n: "wine glass" },
    ],
  },
  {
    name: "Activity",
    icon: "⚽",
    items: [
      { c: "⚽", n: "soccer football" }, { c: "🏀", n: "basketball" }, { c: "🏈", n: "american football" }, { c: "⚾", n: "baseball" },
      { c: "🎾", n: "tennis" }, { c: "🏐", n: "volleyball" }, { c: "🏉", n: "rugby" }, { c: "🎱", n: "pool 8 ball billiards" },
      { c: "🏓", n: "ping pong table tennis" }, { c: "🏸", n: "badminton" }, { c: "🥅", n: "goal net" }, { c: "🏒", n: "ice hockey" },
      { c: "🏑", n: "field hockey" }, { c: "🏏", n: "cricket" }, { c: "⛳", n: "flag in hole golf" }, { c: "🏹", n: "bow and arrow archery" },
      { c: "🎣", n: "fishing pole" }, { c: "🥊", n: "boxing glove" }, { c: "🥋", n: "martial arts uniform" }, { c: "⛸️", n: "ice skate" },
      { c: "🎿", n: "skis" }, { c: "🏂", n: "snowboarder" }, { c: "🏋️", n: "weight lifter" }, { c: "🤸", n: "cartwheel gymnast" },
      { c: "🏄", n: "surfer" }, { c: "🏊", n: "swimmer" }, { c: "🚴", n: "cyclist bicycle" }, { c: "🎮", n: "video game controller" },
      { c: "🎲", n: "game die dice" }, { c: "🎯", n: "direct hit bullseye dart" }, { c: "🎳", n: "bowling" }, { c: "🎸", n: "guitar" },
      { c: "🎹", n: "musical keyboard piano" }, { c: "🥁", n: "drum" }, { c: "🎺", n: "trumpet" }, { c: "🎻", n: "violin" },
      { c: "🎬", n: "clapper board movie" }, { c: "🎨", n: "artist palette paint" }, { c: "🎤", n: "microphone sing" }, { c: "🎧", n: "headphones" },
    ],
  },
  {
    name: "Travel",
    icon: "✈️",
    items: [
      { c: "🚗", n: "car automobile" }, { c: "🚕", n: "taxi" }, { c: "🚙", n: "sport utility vehicle" }, { c: "🚌", n: "bus" },
      { c: "🏎️", n: "racing car" }, { c: "🚓", n: "police car" }, { c: "🚑", n: "ambulance" }, { c: "🚒", n: "fire engine" },
      { c: "🚚", n: "delivery truck" }, { c: "🚜", n: "tractor" }, { c: "🚲", n: "bicycle" }, { c: "🛵", n: "motor scooter" },
      { c: "🏍️", n: "motorcycle" }, { c: "🚨", n: "police light siren" }, { c: "✈️", n: "airplane" }, { c: "🚀", n: "rocket" },
      { c: "🛸", n: "flying saucer ufo" }, { c: "🚁", n: "helicopter" }, { c: "⛵", n: "sailboat" }, { c: "🚤", n: "speedboat" },
      { c: "⚓", n: "anchor" }, { c: "🚉", n: "station train" }, { c: "🚄", n: "high speed train" }, { c: "🚇", n: "metro subway" },
      { c: "🗺️", n: "world map" }, { c: "🗽", n: "statue of liberty" }, { c: "🗼", n: "tokyo tower" }, { c: "🏰", n: "castle" },
      { c: "🎡", n: "ferris wheel" }, { c: "🎢", n: "roller coaster" }, { c: "⛲", n: "fountain" }, { c: "🏖️", n: "beach with umbrella" },
      { c: "🏝️", n: "desert island" }, { c: "🏔️", n: "snow-capped mountain" }, { c: "🌋", n: "volcano" }, { c: "🏕️", n: "camping" },
      { c: "⛺", n: "tent" }, { c: "🏠", n: "house home" }, { c: "🏡", n: "house with garden" }, { c: "🌆", n: "cityscape at dusk" },
    ],
  },
  {
    name: "Objects",
    icon: "💡",
    items: [
      { c: "⌚", n: "watch" }, { c: "📱", n: "mobile phone" }, { c: "💻", n: "laptop computer" }, { c: "⌨️", n: "keyboard" },
      { c: "🖥️", n: "desktop computer" }, { c: "🖨️", n: "printer" }, { c: "🖱️", n: "computer mouse" }, { c: "💾", n: "floppy disk save" },
      { c: "💿", n: "optical disc cd" }, { c: "📷", n: "camera" }, { c: "📹", n: "video camera" }, { c: "🎥", n: "movie camera" },
      { c: "📞", n: "telephone receiver" }, { c: "☎️", n: "telephone" }, { c: "📺", n: "television" }, { c: "📻", n: "radio" },
      { c: "🔋", n: "battery" }, { c: "🔌", n: "electric plug" }, { c: "💡", n: "light bulb idea" }, { c: "🔦", n: "flashlight" },
      { c: "🕯️", n: "candle" }, { c: "💰", n: "money bag" }, { c: "💳", n: "credit card" }, { c: "🔧", n: "wrench" },
      { c: "🔨", n: "hammer" }, { c: "⚙️", n: "gear settings" }, { c: "🔩", n: "nut and bolt" }, { c: "⚖️", n: "balance scale" },
      { c: "🔗", n: "link chain" }, { c: "🔒", n: "locked" }, { c: "🔑", n: "key" }, { c: "🗝️", n: "old key" },
      { c: "🚪", n: "door" }, { c: "🧳", n: "luggage suitcase" }, { c: "⏰", n: "alarm clock" }, { c: "⌛", n: "hourglass done" },
      { c: "📦", n: "package box" }, { c: "✏️", n: "pencil" }, { c: "📌", n: "pushpin" }, { c: "📎", n: "paperclip" },
      { c: "🔍", n: "magnifying glass search" }, { c: "📖", n: "open book" }, { c: "📝", n: "memo note" }, { c: "📅", n: "calendar" },
    ],
  },
  {
    name: "Symbols",
    icon: "❤️",
    items: [
      { c: "❤️", n: "red heart love" }, { c: "🧡", n: "orange heart" }, { c: "💛", n: "yellow heart" }, { c: "💚", n: "green heart" },
      { c: "💙", n: "blue heart" }, { c: "💜", n: "purple heart" }, { c: "🖤", n: "black heart" }, { c: "🤍", n: "white heart" },
      { c: "💔", n: "broken heart" }, { c: "❣️", n: "heart exclamation" }, { c: "💕", n: "two hearts" }, { c: "💖", n: "sparkling heart" },
      { c: "💯", n: "hundred points perfect" }, { c: "⭐", n: "star" }, { c: "🌟", n: "glowing star" }, { c: "✨", n: "sparkles" },
      { c: "⚡", n: "high voltage lightning" }, { c: "🔥", n: "fire flame lit" }, { c: "💧", n: "droplet water" }, { c: "✅", n: "check mark button" },
      { c: "❌", n: "cross mark wrong" }, { c: "❗", n: "exclamation mark" }, { c: "❓", n: "question mark" }, { c: "⚠️", n: "warning" },
      { c: "♻️", n: "recycling" }, { c: "✔️", n: "check mark" }, { c: "➕", n: "plus" }, { c: "➖", n: "minus" },
      { c: "✖️", n: "multiply" }, { c: "➗", n: "divide" }, { c: "🔴", n: "red circle" }, { c: "🟠", n: "orange circle" },
      { c: "🟡", n: "yellow circle" }, { c: "🟢", n: "green circle" }, { c: "🔵", n: "blue circle" }, { c: "🟣", n: "purple circle" },
      { c: "⚫", n: "black circle" }, { c: "⚪", n: "white circle" }, { c: "🔶", n: "large orange diamond" }, { c: "🔷", n: "large blue diamond" },
    ],
  },
  {
    name: "Flags",
    icon: "🏁",
    items: [
      { c: "🏁", n: "chequered flag finish" }, { c: "🚩", n: "triangular flag" }, { c: "🎌", n: "crossed flags" }, { c: "🏴", n: "black flag" },
      { c: "🏳️", n: "white flag" }, { c: "🏳️‍🌈", n: "rainbow pride flag" }, { c: "🏴‍☠️", n: "pirate flag" }, { c: "🇺🇸", n: "United States flag" },
      { c: "🇬🇧", n: "United Kingdom flag" }, { c: "🇨🇦", n: "Canada flag" }, { c: "🇫🇷", n: "France flag" }, { c: "🇩🇪", n: "Germany flag" },
      { c: "🇮🇹", n: "Italy flag" }, { c: "🇪🇸", n: "Spain flag" }, { c: "🇯🇵", n: "Japan flag" }, { c: "🇨🇳", n: "China flag" },
      { c: "🇰🇷", n: "South Korea flag" }, { c: "🇮🇳", n: "India flag" }, { c: "🇧🇷", n: "Brazil flag" }, { c: "🇷🇺", n: "Russia flag" },
      { c: "🇲🇽", n: "Mexico flag" }, { c: "🇦🇺", n: "Australia flag" }, { c: "🇳🇱", n: "Netherlands flag" }, { c: "🇸🇪", n: "Sweden flag" },
      { c: "🇨🇭", n: "Switzerland flag" }, { c: "🇸🇬", n: "Singapore flag" }, { c: "🇦🇪", n: "United Arab Emirates flag" }, { c: "🇿🇦", n: "South Africa flag" },
    ],
  },
];

/** Inserts a glyph (symbol or emoji) at the caret through the shared gated,
 *  tracked, undoable text path. `pasteText(glyph, "paste")` fails closed in
 *  Viewing, routes through the suggestion path in Suggesting, and records ONE
 *  non-coalescing history entry per call (the "paste" HistoryKind never merges),
 *  so every glyph is exactly one Undo. The picker stays open (Word/Docs). */
async function insertGlyphAtCaret(glyph) {
  if (!doc || !glyph) return;
  if (!selection) {
    setStatus("Place the caret before inserting", "error");
    return;
  }
  await pasteText(glyph, "paste");
}

/** Builds a categorized glyph picker (symbol or emoji) over an existing dialog
 *  skeleton in the markup. Returns `{ open }`; the controller owns tab switching,
 *  keyword search, roving arrow-key grid navigation, insertion (keep-open), and
 *  Esc / backdrop / Done dismissal — mirroring the field dialog's focus-trap and
 *  return-focus contract. */
function createGlyphPicker({ dialogId, gridId, tabsId, searchId, emptyId, closeId, doneId, groups, tabsAreEmoji }) {
  const dialog = document.getElementById(dialogId);
  const grid = document.getElementById(gridId);
  const tabs = document.getElementById(tabsId);
  const search = document.getElementById(searchId);
  const empty = document.getElementById(emptyId);
  const closeBtn = document.getElementById(closeId);
  const doneBtn = document.getElementById(doneId);
  if (!dialog || !grid || !tabs || !search) return { open: () => {} };

  let returnFocus = null;
  let activeGroup = 0;

  // One tab button per category. Emoji tabs show the category's glyph; symbol
  // tabs show the category name (which fits the narrower label).
  groups.forEach((group, index) => {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "glyph-tab";
    tab.dataset.groupIndex = String(index);
    tab.setAttribute("role", "tab");
    tab.title = group.name;
    tab.setAttribute("aria-label", group.name);
    tab.textContent = tabsAreEmoji ? group.icon : group.name;
    tab.addEventListener("click", () => {
      search.value = "";
      selectGroup(index);
    });
    tabs.append(tab);
  });

  function currentItems() {
    const query = search.value.trim().toLowerCase();
    if (query) {
      return groups
        .flatMap((group) => group.items)
        .filter((item) => item.n.toLowerCase().includes(query) || item.c === query);
    }
    return groups[activeGroup].items;
  }

  function renderGrid() {
    const items = currentItems();
    grid.replaceChildren();
    for (const item of items) {
      const cell = document.createElement("button");
      cell.type = "button";
      cell.className = "glyph-cell";
      cell.tabIndex = -1;
      cell.textContent = item.c;
      cell.title = tabsAreEmoji ? item.n : `${item.n} (U+${item.c.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")})`;
      cell.setAttribute("aria-label", item.n);
      cell.dataset.glyph = item.c;
      cell.addEventListener("click", () => insertGlyphAtCaret(item.c));
      grid.append(cell);
    }
    const first = grid.querySelector(".glyph-cell");
    if (first) first.tabIndex = 0;
    grid.hidden = items.length === 0;
    if (empty) empty.hidden = items.length !== 0;
  }

  function selectGroup(index) {
    activeGroup = index;
    for (const tab of tabs.querySelectorAll(".glyph-tab")) {
      const on = Number(tab.dataset.groupIndex) === index && !search.value.trim();
      tab.classList.toggle("is-active", on);
      tab.setAttribute("aria-selected", String(on));
    }
    renderGrid();
  }

  // Roving-tabindex arrow navigation across the grid; Enter/Space fire the
  // button's own click, so insertion (keep-open) is shared with pointer use.
  grid.addEventListener("keydown", (event) => {
    const cells = [...grid.querySelectorAll(".glyph-cell")];
    if (!cells.length) return;
    const current = cells.indexOf(document.activeElement);
    if (current < 0) return;
    let next = -1;
    if (event.key === "ArrowRight") next = current + 1;
    else if (event.key === "ArrowLeft") next = current - 1;
    else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      // Columns = how many cells share the first cell's top edge.
      const top0 = cells[0].offsetTop;
      let cols = cells.findIndex((cell) => cell.offsetTop > top0);
      if (cols < 0) cols = cells.length;
      next = current + (event.key === "ArrowDown" ? cols : -cols);
    } else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = cells.length - 1;
    else return;
    if (next < 0 || next >= cells.length) {
      event.preventDefault();
      return;
    }
    event.preventDefault();
    cells[current].tabIndex = -1;
    cells[next].tabIndex = 0;
    cells[next].focus();
  });

  // A query flattens all categories into a filtered result set and clears the
  // active tab highlight; clearing it restores the current category.
  search.addEventListener("input", () => selectGroup(activeGroup));

  function close() {
    if (dialog.hidden) return;
    dialog.hidden = true;
    const to = returnFocus;
    returnFocus = null;
    if (to && typeof to.focus === "function" && document.contains(to)) to.focus({ preventScroll: true });
    else focusEditorSurface();
  }

  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) close();
  });
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Tab") {
      trapModalFocus(event, dialog);
    }
  });
  closeBtn?.addEventListener("click", () => close());
  doneBtn?.addEventListener("click", () => close());

  function open() {
    if (!doc) return;
    if (!selection) {
      setStatus("Place the caret before inserting", "error");
      focusEditorSurface();
      return;
    }
    // Viewing is read-only; fail closed before opening (mirrors the link dialog)
    // so the picker never opens onto a dead insert. Suggesting is allowed — the
    // insert routes through the tracked suggestion path.
    if (blockMutationInViewing()) return;
    returnFocus = document.activeElement;
    search.value = "";
    selectGroup(0);
    dialog.hidden = false;
    queueMicrotask(() => search.focus());
  }

  return { open };
}

const symbolPicker = createGlyphPicker({
  dialogId: "symbolDialog", gridId: "symbolGrid", tabsId: "symbolTabs", searchId: "symbolSearch",
  emptyId: "symbolEmpty", closeId: "symbolClose", doneId: "symbolDone", groups: SYMBOL_GROUPS, tabsAreEmoji: false,
});
const emojiPicker = createGlyphPicker({
  dialogId: "emojiDialog", gridId: "emojiGrid", tabsId: "emojiTabs", searchId: "emojiSearch",
  emptyId: "emojiEmpty", closeId: "emojiClose", doneId: "emojiDone", groups: EMOJI_GROUPS, tabsAreEmoji: true,
});

function openSymbolPicker() {
  symbolPicker.open();
}

function openEmojiPicker() {
  emojiPicker.open();
}

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
/** The range a currently-open link dialog targets, plus what to restore focus
 *  to when it closes. `null` when the dialog is closed. */
let linkDialogCtx = null;
let linkDialogReturnFocus = null;
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
  const entries = bookmarkEntries();
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

/** Opens the dialog over `[node, start)..[node, end)`. `link` (optional) is an
 *  existing hyperlink to edit — it prefills the URL / bookmark / ScreenTip and
 *  shows Remove; a fresh insert leaves them empty. `text` is the current display
 *  text. Fails closed in Viewing/Suggesting (the same gate the apply path
 *  enforces) so the dialog never opens onto a dead Apply. */
function openLinkDialog({ node, start, end, link = null, text = "" }) {
  if (!doc || !linkDialog) return;
  if (blockMutationInViewing() || blockUntrackedInSuggesting()) return;
  linkDialogCtx = { node, start, end, editing: !!link, originalText: text };
  linkDialogReturnFocus = document.activeElement;

  const internal = link?.kind === "internal";
  const hasPlaces = populateLinkPlaces(internal ? link.anchor : null);
  linkTextInput.value = text;
  linkTooltipInput.value = link?.tooltip || "";
  linkUrlInput.value = internal ? "" : (link?.url || "");
  setLinkDialogMode(internal && hasPlaces ? "place" : "url");
  setLinkDialogNote("", false);
  linkDialogTitle.textContent = link ? "Edit link" : "Insert link";
  linkRemoveBtn.hidden = !link;

  linkDialog.hidden = false;
  queueMicrotask(() => {
    const first = linkDialogMode === "place" ? linkPlaceSelect : linkUrlInput;
    first.focus();
    if (first === linkUrlInput) linkUrlInput.select();
  });
}

function closeLinkDialog() {
  if (!linkDialog || linkDialog.hidden) return;
  linkDialog.hidden = true;
  linkDialogCtx = null;
  const returnTo = linkDialogReturnFocus;
  linkDialogReturnFocus = null;
  if (returnTo && typeof returnTo.focus === "function" && document.contains(returnTo)) {
    returnTo.focus({ preventScroll: true });
  } else {
    focusEditorSurface();
  }
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
  if (!doc || !linkDialogCtx) return;
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
  selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
  drawSelection();

  if (!textChanged) {
    await runToolbarEdit(() => doc.setHyperlink(node, start, end, target, tooltip || null));
  } else {
    // Rebuild the range as a single run carrying the first run's formatting plus
    // the new text and href; pasteRichRuns batches InsertText+FormatText+
    // SetHyperlink into one undoable action.
    let base = {};
    try {
      base = (JSON.parse(doc.copyRichRuns(node, start, node, end)) || [])
        .find((run) => !run.paragraphBreak) || {};
    } catch { /* fall back to an unformatted run */ }
    const run = { ...base, text: displayText, href: target, paragraphBreak: false };
    await pasteRichRunsJson(JSON.stringify([run]));
    if (tooltip) setStatus("Link added. Its ScreenTip needs the display text left unchanged.");
  }
  closeLinkDialog();
}

/** Removes the link over the dialog's range (`removeHyperlink`, one gated
 *  undoable action) — the Remove button, shown only when editing. */
async function removeLinkDialog() {
  if (!doc || !linkDialogCtx) return;
  const { node, start, end } = linkDialogCtx;
  selection = { anchor: { node, offset: start }, focus: { node, offset: end } };
  drawSelection();
  await runToolbarEdit(() => doc.removeHyperlink(node, start, end));
  closeLinkDialog();
  setStatus("Link removed");
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
  linkDialog.addEventListener("click", (event) => {
    if (event.target === linkDialog) closeLinkDialog();
  });
  linkDialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeLinkDialog();
    } else if (event.key === "Enter" && event.target === linkPlaceSelect) {
      // Enter on the native select would not submit the form; apply explicitly.
      event.preventDefault();
      void applyLinkDialog();
    } else if (event.key === "Tab") {
      trapModalFocus(event, linkDialog);
    }
  });
}

function renderCommands(query) {
  const q = query.trim().toLowerCase();
  const all = buildCommands();
  cmdMatches = q
    ? all.filter((c) => `${c.label} ${c.group} ${c.kw} ${c.shortcut ?? ""}`.toLowerCase().includes(q))
    : all;
  cmdSel = cmdMatches.findIndex((command) => command.enabled !== false);
  cmdList.replaceChildren();
  if (!cmdMatches.length) {
    const empty = document.createElement("div");
    empty.className = "cmd-empty";
    empty.textContent = "No matching commands";
    cmdList.appendChild(empty);
    return;
  }
  cmdMatches.forEach((c, i) => {
    const item = document.createElement("button");
    item.type = "button";
    item.className = `cmd-item${i === cmdSel ? " sel" : ""}`;
    item.setAttribute("role", "option");
    item.disabled = c.enabled === false;
    if (c.disabledReason) item.title = c.disabledReason;
    // The hint column shows the disabled reason when unavailable, else the
    // command's keyboard shortcut when it has one (so the palette teaches the
    // shortcut), else its group.
    const hint = c.enabled === false ? c.disabledReason : (c.shortcut || c.group);
    item.innerHTML = `<span>${escapeHtml(c.label)}</span><span class="cmd-hint">${escapeHtml(hint)}</span>`;
    item.addEventListener("mousemove", () => setCmdSel(i));
    item.addEventListener("click", () => runCommand(i));
    cmdList.appendChild(item);
  });
}

function setCmdSel(i) {
  if (i < 0 || cmdMatches[i]?.enabled === false) return;
  cmdSel = i;
  const items = cmdList.querySelectorAll(".cmd-item");
  items.forEach((el, k) => el.classList.toggle("sel", k === i));
  items[i]?.scrollIntoView({ block: "nearest" });
}

function moveCmdSelection(direction) {
  if (!cmdMatches.some((command) => command.enabled !== false)) return;
  let index = cmdSel;
  for (let count = 0; count < cmdMatches.length; count++) {
    index = (index + direction + cmdMatches.length) % cmdMatches.length;
    if (cmdMatches[index].enabled !== false) {
      setCmdSel(index);
      return;
    }
  }
}

function runCommand(i) {
  const cmd = cmdMatches[i];
  if (!cmd || cmd.enabled === false) return;
  closeCmd();
  cmd.run();
}

function openCmd() {
  closeAppMenu();
  cmdReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  cmdPalette.hidden = false;
  searchTrigger.setAttribute("aria-expanded", "true");
  cmdInput.value = "";
  renderCommands("");
  cmdInput.focus();
}
function closeCmd() {
  cmdPalette.hidden = true;
  searchTrigger.setAttribute("aria-expanded", "false");
  cmdReturnFocus?.focus();
  cmdReturnFocus = null;
}

cmdInput.addEventListener("input", () => renderCommands(cmdInput.value));
cmdInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    moveCmdSelection(1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    moveCmdSelection(-1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    runCommand(cmdSel);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeCmd();
  }
});
cmdPalette.addEventListener("pointerdown", (e) => {
  if (e.target === cmdPalette) closeCmd(); // click the backdrop
});
document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  const lower = e.key.toLowerCase();
  // Command palette: ⌘⇧P (the VS Code / editor-palette convention). Moved off
  // ⌘K so that ⌘K can carry the Word / Google Docs / Pages standard "insert or
  // edit hyperlink" — the two used to collide (docs/67 audit row 8). The header
  // Search pill is the discoverable on-screen entry point (doc 69 §1.4.1).
  if (e.shiftKey && lower === "p") {
    e.preventDefault();
    cmdPalette.hidden ? openCmd() : closeCmd();
    return;
  }
  // ⌘K inserts/edits a hyperlink on the current text selection. Skipped while a
  // chrome input (find box, dialog field, the palette itself) is focused so it
  // never hijacks typing there.
  if (!e.shiftKey && lower === "k" && doc && !isInteractiveChromeTarget(e.target)) {
    e.preventDefault();
    if (hasRange() && selection) {
      editSelectionLink();
    } else {
      setStatus("Select text to add a link", "error");
    }
    return;
  }
  if (lower === "s" && doc) {
    e.preventDefault();
    saveDocument();
    return;
  }
  // ⌘/Ctrl+P prints the document's rendered pages. Intercept the browser default
  // (which would print the editor chrome and mostly-blank virtualized pages) and
  // run our dedicated print path instead. Read-only, so it works in any mode
  // with no unsaved-changes requirement.
  if (!e.shiftKey && lower === "p" && doc) {
    e.preventDefault();
    printDocument();
  }
});
// Visible entry point for the palette (doc 69 §1.4.1): the shortcut already
// worked, it just had no on-screen affordance to discover it.
searchTrigger.addEventListener("click", () => openCmd());

// ---- Find / replace ---------------------------------------------------------
const FIND_SCAN_CAP = 5000;

function setFindStatus(text, miss = false) {
  findStatus.textContent = text;
  findStatus.classList.toggle("miss", miss);
}

/** Scans every match in document order, starting from the top, up to
 * FIND_SCAN_CAP — bounded like every other pagination/parse loop in this
 * codebase, since findText wraps around and would otherwise loop forever.
 * Once every match has been visited once, the engine's wrap fallback can
 * re-surface *any* earlier match (not necessarily the first one), so
 * termination checks membership in every key seen so far, not just the
 * first — comparing only to the first match's key under-counts (it can
 * oscillate between two later matches forever without ever revisiting the
 * exact first one). */
const findTextEncoder = new TextEncoder();
const findParagraphTextCache = new Map();

function paragraphTextForFind(node) {
  if (!findParagraphTextCache.has(node)) {
    const length = doc.paragraphLength(node);
    findParagraphTextCache.set(node, doc.copyText(node, 0, node, length));
  }
  return findParagraphTextCache.get(node);
}

function byteOffsetToStringIndex(text, byteOffset) {
  if (byteOffset <= 0) return 0;
  let bytes = 0;
  for (let i = 0; i < text.length; ) {
    if (bytes >= byteOffset) return i;
    const cp = text.codePointAt(i);
    const width = findTextEncoder.encode(String.fromCodePoint(cp)).length;
    bytes += width;
    i += cp > 0xffff ? 2 : 1;
  }
  return text.length;
}

function isWholeWordMatch(match, query) {
  const text = paragraphTextForFind(match.startNode);
  const start = byteOffsetToStringIndex(text, match.startOffset);
  const end = byteOffsetToStringIndex(text, match.endOffset);
  const word = /[\p{L}\p{N}_]/u;
  return !word.test(text[start - 1] || "") && !word.test(text[end] || "");
}

function clearFindParagraphCache() {
  findParagraphTextCache.clear();
}

// Capture the current selection as an ordered scope for "find in selection".
// selectionEdge(...false) is the document-order start, (...true) the end. We
// copy the values out and free the WASM handles immediately. Returns null when
// there is no non-empty range to scope to.
function captureFindScope() {
  if (!selection || !hasRange()) return null;
  const { anchor, focus } = selection;
  const s = doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, false);
  const e = doc.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, true);
  const scope = {
    startNode: s.node,
    startOffset: s.offset,
    endNode: e.node,
    endOffset: e.offset,
  };
  s.free();
  e.free();
  return scope;
}

// True iff position (aNode:aOff) <= position (bNode:bOff) in document order.
// Same node compares offsets with no WASM call; otherwise selectionEdge(...false)
// returns whichever endpoint is earlier, and since the two nodes differ the
// returned node uniquely identifies which one that is.
function findPosLE(aNode, aOff, bNode, bOff) {
  if (aNode === bNode) return aOff <= bOff;
  const edge = doc.selectionEdge(aNode, aOff, bNode, bOff, false);
  const aIsEarlier = edge.node === aNode;
  edge.free();
  return aIsEarlier;
}

// A find match spans a single paragraph, so match.startNode === match.endNode.
// Accept iff [match.startOffset .. match.endOffset] on that node lies within the
// ordered scope [scopeStart .. scopeEnd]. Boundary-node matches (the common
// single-paragraph case and the first/last paragraph of a multi-paragraph
// scope) resolve with zero extra WASM calls; only genuinely-interior nodes need
// the two order tests.
function matchInFindSelection(match) {
  if (!findSelection.checked) return true;
  if (!findScope) return false;
  const { startNode, startOffset, endNode, endOffset } = findScope;
  const node = match.startNode;
  if (node === startNode) {
    if (match.startOffset < startOffset) return false;
    // Single-node scope (startNode === endNode) also caps the upper bound.
    return node === endNode ? match.endOffset <= endOffset : true;
  }
  if (node === endNode) {
    return match.endOffset <= endOffset;
  }
  // Interior node: in scope iff scopeStart <= match and match <= scopeEnd.
  return (
    findPosLE(startNode, startOffset, node, match.startOffset) &&
    findPosLE(node, match.endOffset, endNode, endOffset)
  );
}

function scanAllMatches(query, matchCase, wholeWord = false) {
  const matches = [];
  if (!doc || !query) return matches;
  const first = doc.firstPosition();
  let node = first.node;
  let offset = first.offset;
  first.free();
  const seen = new Set();
  for (let i = 0; i < FIND_SCAN_CAP; i++) {
    const match = doc.findText(query, node, offset, true, matchCase);
    if (!match.found) {
      match.free();
      break;
    }
    const key = `${match.startNode}:${match.startOffset}`;
    if (seen.has(key)) {
      match.free();
      break;
    }
    seen.add(key);
    const candidate = {
      startNode: match.startNode,
      startOffset: match.startOffset,
      endNode: match.endNode,
      endOffset: match.endOffset,
    };
    if ((!wholeWord || isWholeWordMatch(candidate, query)) && matchInFindSelection(candidate)) {
      matches.push(candidate);
    }
    node = match.endNode;
    offset = match.endOffset;
    match.free();
  }
  return matches;
}

function updateFindStatus() {
  const query = findInput.value;
  if (!query) {
    setFindStatus("");
    return;
  }
  const matches = scanAllMatches(query, findCase.checked, findWholeWord.checked);
  if (!matches.length) {
    setFindStatus("No match", true);
    return;
  }
  if (matches.length === 1) {
    setFindStatus("1 match");
    return;
  }
  const idx = selection
    ? matches.findIndex(
        (m) => m.startNode === selection.anchor.node && m.startOffset === selection.anchor.offset,
      )
    : -1;
  setFindStatus(idx >= 0 ? `${idx + 1} of ${matches.length}` : `${matches.length} matches`);
}

function selectedPlainText() {
  if (!selection) return "";
  const { anchor, focus } = selection;
  if (anchor.node === focus.node && anchor.offset === focus.offset) return "";
  return doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
}

function queryMatchesSelection() {
  const query = findInput.value;
  if (!query) return false;
  const selected = selectedPlainText();
  if (!selected.includes("\n")) {
    return findCase.checked
      ? selected === query
      : selected.toLocaleLowerCase() === query.toLocaleLowerCase();
  }
  return false;
}

function selectTextMatch(match) {
  if (!match || match.found === false) {
    setFindStatus("No match", true);
    return false;
  }
  selection = {
    anchor: { node: match.startNode, offset: match.startOffset },
    focus: { node: match.endNode, offset: match.endOffset },
  };
  drawSelection();
  // Deliberately does NOT call focusEditorSurface(): this runs on every
  // keystroke while live-searching (findInput's "input" listener), and
  // stealing focus back to the canvas mid-typing sent subsequent keystrokes
  // to the document instead of the find box. Focus returns to the canvas
  // only when the panel actually closes (closeFind).
  scrollFindMatchIntoView();
  updateFindStatus();
  return true;
}

function findFromSelection(forward) {
  if (!doc || !findInput.value) {
    setFindStatus("");
    return false;
  }
  const matches = scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked);
  if (!matches.length) {
    setFindStatus("No match", true);
    return false;
  }
  const current = selection
    ? matches.findIndex(
        (m) => m.startNode === selection.anchor.node && m.startOffset === selection.anchor.offset,
      )
    : -1;
  const index = current >= 0
    ? (current + (forward ? 1 : matches.length - 1)) % matches.length
    : forward ? 0 : matches.length - 1;
  return selectTextMatch(matches[index]);
}

async function replaceCurrentMatch() {
  if (!doc || !findInput.value) return;
  if (blockUntrackedInSuggesting()) return;
  if (!queryMatchesSelection()) {
    findFromSelection(true);
    return;
  }
  const { anchor, focus } = selection;
  await runEdit(() =>
    doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, replaceInput.value),
  );
  findFromSelection(true);
}

/** Replaces every match in document order. Each iteration re-finds from just
 * past the previous replacement (not from the top), so a replacement text
 * that itself contains the query (e.g. "cat" -> "cats") can never re-match
 * what was just inserted and loop forever; FIND_SCAN_CAP is a bounded
 * backstop regardless. */
async function replaceAllMatches() {
  if (!doc || !findInput.value) return;
  if (blockUntrackedInSuggesting()) return;
  const replacement = replaceInput.value;
  // Collect every match (honoring case / whole-word / selection scope) in
  // document order, then replace them ALL as ONE undoable action — Word and
  // Google Docs undo a Replace All in a single step. The ranges are passed in
  // DESCENDING document order (reverse of the scan) so applying each replacement
  // never shifts an earlier, not-yet-applied match's offsets.
  const matches = scanAllMatches(findInput.value, findCase.checked, findWholeWord.checked);
  if (!matches.length) {
    setFindStatus("No match", true);
    return;
  }
  const ordered = matches.slice().reverse();
  await runEdit(() =>
    doc.replaceRanges(
      ordered.map((m) => m.startNode),
      ordered.map((m) => m.startOffset),
      ordered.map((m) => m.endNode),
      ordered.map((m) => m.endOffset),
      replacement,
    ),
  );
  setFindStatus(`Replaced ${matches.length}`);
}

function openFind() {
  if (!doc) return;
  findPanel.hidden = false;
  if (findSelection.checked) findSelection.dispatchEvent(new Event("change"));
  const selected = selectedPlainText();
  if (selected && !selected.includes("\n") && selected.length <= 80) findInput.value = selected;
  updateFindStatus();
  findInput.focus();
  findInput.select();
}

function closeFind() {
  findPanel.hidden = true;
  focusEditorSurface();
}

findInput.addEventListener("input", () => {
  if (findInput.value) findFromSelection(true);
  else setFindStatus("");
});
findCase.addEventListener("change", () => updateFindStatus());
findWholeWord.addEventListener("change", () => updateFindStatus());
findSelection.addEventListener("change", () => {
  if (findSelection.checked) {
    findScope = captureFindScope();
  } else {
    findScope = null;
  }
  updateFindStatus();
});
findInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    findFromSelection(!e.shiftKey);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeFind();
  }
});
replaceInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    replaceCurrentMatch();
  } else if (e.key === "Escape") {
    e.preventDefault();
    closeFind();
  }
});
findCase.addEventListener("change", () => findFromSelection(true));
findPrevBtn.addEventListener("click", () => findFromSelection(false));
findNextBtn.addEventListener("click", () => findFromSelection(true));
replaceOneBtn.addEventListener("click", replaceCurrentMatch);
replaceAllBtn.addEventListener("click", replaceAllMatches);
findCloseBtn.addEventListener("click", closeFind);
findBtn.addEventListener("click", () => openFind());
document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    openFind();
  }
});

// Indentation: left/right absolute, and a first-line/hanging "special" indent
// (setFirstLineIndent encodes hanging as a negative value, 0 clears both).
indentLeftInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setLeftIndent(a, b, c, d, inchTwips(indentLeftInput))),
);
indentRightInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setRightIndent(a, b, c, d, inchTwips(indentRightInput))),
);
function applyIndentSpecial() {
  const by = inchTwips(indentSpecialByInput);
  const kind = indentSpecialSel.value;
  const twips = kind === "first" ? by : kind === "hanging" ? -by : 0;
  runToolbarEdit((a, b, c, d) => doc.setFirstLineIndent(a, b, c, d, twips));
}
indentSpecialSel.addEventListener("change", applyIndentSpecial);
indentSpecialByInput.addEventListener("change", applyIndentSpecial);

paraShade.addEventListener("change", () => {
  const [r, g, b] = hexToRgb(paraShade.value);
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, r, g, b, false));
});
onButton(paraShadeNone, () =>
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, 0, 0, 0, true)),
);
for (const [box, setter] of [
  [pgKeepNext, (a, b, c, d, on) => doc.setKeepWithNext(a, b, c, d, on)],
  [pgKeepLines, (a, b, c, d, on) => doc.setKeepLinesTogether(a, b, c, d, on)],
  [pgBreakBefore, (a, b, c, d, on) => doc.setPageBreakBefore(a, b, c, d, on)],
]) {
  box.addEventListener("change", () =>
    runToolbarEdit((a, b, c, d) => setter(a, b, c, d, box.checked)),
  );
}
/** Apply a text color (hex `#rrggbb`) to the range or arm it at the caret. */
function applyTextColor(hex) {
  const [r, g, b] = hexToRgb(hex);
  armOrApplyRun({ color: hex }, () =>
    runToolbarEdit((a, bo, c, d) => doc.setTextColor(a, bo, c, d, r, g, b)),
  );
}
/** Apply a named OOXML highlight (or "none") to the range or arm it at the caret. */
function applyHighlight(name) {
  armOrApplyRun({ highlight: name }, () =>
    runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, name)),
  );
}

// ---- Floating selection toolbar (appears above a text selection) ------------
const selToolbar = document.getElementById("selToolbar");

/** Shows the floating toolbar centred just above the current range selection (or
 *  below it when there's no room), or hides it when the selection is collapsed. */
function positionSelToolbar() {
  if (activeLink || !selection || !hasRange()) {
    selToolbar.hidden = true;
    return;
  }
  const rects = pagesEl.querySelectorAll(".overlay .highlight");
  if (!rects.length) {
    selToolbar.hidden = true;
    return;
  }
  let top = Infinity;
  let bottom = -Infinity;
  let left = Infinity;
  let right = -Infinity;
  for (const el of rects) {
    const b = el.getBoundingClientRect();
    top = Math.min(top, b.top);
    bottom = Math.max(bottom, b.bottom);
    left = Math.min(left, b.left);
    right = Math.max(right, b.right);
  }
  selToolbar.hidden = false; // must be visible to measure
  const tw = selToolbar.offsetWidth;
  const th = selToolbar.offsetHeight;
  const viewport = viewportEl.getBoundingClientRect();
  const topBound = Math.max(8, viewport.top + 8);
  const bottomBound = Math.min(window.innerHeight - 8, viewport.bottom - 8);
  let x = (left + right) / 2 - tw / 2;
  let y = top - th - 8;
  if (y < topBound) y = bottom + 8; // no room above → drop below the selection
  if (y + th > bottomBound) {
    // A selection at the viewport bottom may have no full-height slot below it;
    // keep the bar visible and out of the browser chrome rather than allowing a
    // fixed-position toolbar to disappear below the window.
    y = Math.min(y, bottomBound - th);
  }
  y = Math.max(topBound, y);
  x = Math.max(8, Math.min(x, window.innerWidth - tw - 8));
  selToolbar.style.left = `${Math.round(x)}px`;
  selToolbar.style.top = `${Math.round(y)}px`;
}

for (const b of selToolbar.querySelectorAll("[data-fmt]")) {
  onButton(b, () => toggleFormat(b.dataset.fmt));
}
// Text-color and highlight now use the ribbon's swatch-picker popovers, wired up
// above (registerPopover on #selTextColorBtn / #selHighlightBtn). Keep clicks
// inside the bar from collapsing the selection; hide on viewport scroll.
selToolbar.addEventListener("mousedown", (e) => {
  if (e.target.tagName !== "INPUT" && e.target.tagName !== "SELECT") e.preventDefault();
});
viewportEl.addEventListener("scroll", () => (selToolbar.hidden = true), { passive: true });
/** Apply a font family to the range or arm it at the caret. */
function applyFontFamily(family) {
  if (!family) return;
  armOrApplyRun({ font: family }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFont(a, b, c, d, family)),
  );
}
paragraphStyleSel.addEventListener("change", () => {
  const name = paragraphStyleSel.value;
  runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, name));
});

/** Surfaces the compatibility-finding count from an import or export in the
 *  status chip; hidden when there is nothing to report. */
function showCompatibilityFindings(count, phase) {
  if (!compatibilityStatusEl) return;
  compatibilityStatusEl.hidden = count === 0;
  compatibilityStatusEl.textContent =
    count === 0 ? "" : `${count.toLocaleString()} ${phase} finding${count === 1 ? "" : "s"}`;
  compatibilityStatusEl.title =
    count === 0
      ? ""
      : `${count.toLocaleString()} compatibility finding${count === 1 ? "" : "s"} reported during ${phase}`;
}

/** Fills the Save-format selector with every registered exporter, defaulting to
 *  the format the document was opened as (so a round-trip save keeps the format). */
function populateSaveFormats() {
  if (!saveFormatEl || !doc) return;
  saveFormatEl.replaceChildren();
  for (const formatId of doc.availableExportFormats()) {
    const option = document.createElement("option");
    option.value = formatId;
    option.textContent = formatInfo(formatId).label;
    saveFormatEl.append(option);
  }
  saveFormatEl.value = currentSourceFormat;
  if (!saveFormatEl.value && saveFormatEl.options.length > 0) {
    saveFormatEl.selectedIndex = 0;
  }
  saveFormatEl.disabled = saveFormatEl.options.length === 0;
}

/** Serializes the edited document through the selected registered exporter and
 *  downloads it. Saving back to the source format preserves unchanged bytes where
 *  safe; a different target uses the semantic writer. */
/** Exports the current document through the given registered exporter and
 *  downloads it. Saving back to the source format preserves unchanged bytes where
 *  safe; a different target uses the semantic writer. Shared by the Save button,
 *  the ⌘S shortcut, and the File ▸ Export-as menu entries. */
function exportDocumentAs(targetFormat) {
  if (!doc || !targetFormat) return;
  try {
    let artifact;
    if (targetFormat === currentSourceFormat) {
      try {
        artifact = doc.exportAs(targetFormat, "exact_if_unchanged");
      } catch {
        artifact = doc.exportAs(targetFormat, "preserve_when_safe");
      }
    } else {
      artifact = doc.exportAs(targetFormat, "semantic");
    }
    const bytes = artifact.bytes;
    const mimeType = artifact.mimeType;
    const extension = artifact.suggestedExtension;
    const findings = compatibilityOccurrenceCount(artifact.reportJson);
    artifact.free();
    const blob = new Blob([bytes], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = downloadNameForFormat(currentName, extension);
    a.click();
    URL.revokeObjectURL(url);
    setDocumentState("downloaded");
    showCompatibilityFindings(findings, "export");
    setStatus(
      findings === 0
        ? `Saved ${a.download}`
        : `Saved ${a.download} with ${findings.toLocaleString()} compatibility finding${findings === 1 ? "" : "s"}`,
    );
  } catch (err) {
    console.error(err);
    setStatus(`Save failed: ${err?.message ?? err}`, "error");
  }
}

/** Saves using the format chosen in the Save-format selector (default: source). */
function saveDocument() {
  exportDocumentAs(saveFormatEl && saveFormatEl.value ? saveFormatEl.value : currentSourceFormat);
}
saveBtn.addEventListener("click", saveDocument);

/** Moves the caret to an engine Caret result (document bounds). Shift extends. */
function navToPosition(caret, extend) {
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const to = { node: caret.node, offset: caret.offset };
  if (typeof caret.free === "function") caret.free();
  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView();
}

/** Page Up/Down moves the model caret by one visible editor viewport. Browser
 * geometry chooses only the page-local probe; `hitTest` returns the model anchor
 * that remains the source of truth. */
function navByViewport(dir, extend) {
  if (!selection) return;
  breakTypingSession();
  pendingFormat = null;
  const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
  if (flat.length < 5) return;

  const [pageNumber, x, y, w] = flat;
  const caretPage = pages[pageNumber - 1];
  if (!caretPage) return;
  const { rect: pageRect, sx, sy } = scaleOf(caretPage);
  const viewportRect = viewportEl.getBoundingClientRect();
  const distance = Math.max(48, viewportRect.height - 48);
  const targetX = pageRect.left + x * sx + Math.max(1, (w * sx) / 2);
  const targetY = pageRect.top + y * sy + (dir === "pageUp" ? -distance : distance);
  const page = pageFromClientPoint(targetX, targetY);
  if (!page) return;
  const to = anchorAt(page, clientPointEvent(targetX, targetY));
  if (!to) return;

  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView();
}

/** Selects the whole document (⌘A). */
function selectAll() {
  if (!doc) return;
  breakTypingSession();
  const a = doc.firstPosition();
  const b = doc.lastPosition();
  selection = {
    anchor: { node: a.node, offset: a.offset },
    focus: { node: b.node, offset: b.offset },
  };
  a.free();
  b.free();
  drawSelection();
  focusEditorSurface();
}

function editorClipboardEvent(event) {
  return doc && selection && !isInteractiveChromeTarget(event.target) && eventTargetsEditor(event);
}

function editorTextInputEvent(event) {
  return doc && selection && !isInteractiveChromeTarget(event.target) && eventTargetsEditor(event);
}

/** Cut (⌘X): copy the selection to the clipboard, then delete it. */
async function cut(event = null) {
  if (!hasRange()) return;
  // Cut is a mutation (copy + delete); in read-only Viewing mode it is blocked
  // before touching the clipboard so it never partially executes as a copy.
  if (blockMutationInViewing()) return;
  const copied = await copySelection(event);
  if (!copied) return;
  const { anchor, focus } = selection;
  if (reviewMode === "suggesting" && anchor.node !== focus.node) {
    setStatus("Cross-paragraph cuts cannot be tracked yet; switch to Editing to cut", "error");
    return;
  }
  await runEdit(() => reviewMode === "suggesting" && anchor.node === focus.node
    ? doc.suggestDelete(anchor.node, Math.min(anchor.offset, focus.offset), Math.max(anchor.offset, focus.offset), undefined, new Date().toISOString())
    : doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset));
}

async function pasteText(text, actionKind = "paste") {
  if (!doc || !selection) return;
  if (!text) return;
  const { anchor, focus } = selection;
  const sameParagraph = anchor.node === focus.node && !text.includes("\n");
  if (reviewMode === "suggesting" && sameParagraph) {
    const start = Math.min(anchor.offset, focus.offset);
    const end = Math.max(anchor.offset, focus.offset);
    await runEdit(() => end > start
      ? doc.suggestReplace(anchor.node, start, end, text, undefined, new Date().toISOString())
      : doc.suggestInsert(anchor.node, start, text, undefined, new Date().toISOString()));
    return;
  }
  if (reviewMode === "suggesting") {
    setStatus("Multi-paragraph paste cannot be tracked yet; switch to Editing to paste it", "error");
    return;
  }
  await runEdit(() => doc.insertPlainTextAs(anchor.node, anchor.offset, focus.node, focus.offset, text, actionKind));
}

async function commitComposedText(text) {
  if (!text) return;
  pendingFormat = null;
  await pasteText(text, "typing");
}

/**
 * Suggesting-mode rich paste at a collapsed caret, single paragraph only
 * (REVIEW-GAP-008): inserts each clipboard run as its own tracked
 * `suggestStyledInsert`, chained under one gesture (`typingSessionForKey`)
 * so the whole paste is one review card and one Undo step — the same
 * paragraph-snapshot coalescing real adjacent keystrokes already use (docs
 * 82 §3; see `suggest_insert`'s `continuing_group` in casual-doc-wasm).
 * This is what lets a rich paste keep its bold/italic/color/etc. per run
 * instead of flattening to one plain-text tracked insertion.
 *
 * Returns whether it handled the paste. It does not: multi-paragraph
 * content (`paragraphBreak` runs — REVIEW-GAP-009's structural-tracking
 * backlog), or a paste that also needs to replace an existing selection (no
 * tracked multi-run *replacement* group exists yet — the model's
 * `RevisionGroupKind::Replacement` requires exactly one deletion plus one
 * insertion). Both remain the existing flattened-plain-text fallback in
 * `pasteRichRunsJson`. A run's `href` (hyperlink) is not carried into the
 * tracked insertion either — the same as the existing flattened fallback,
 * so this is not a regression, just an unchanged, explicit limitation.
 */
async function pasteTrackedRichRuns(runs) {
  if (!doc || !selection || hasRange()) return false;
  if (!Array.isArray(runs) || runs.some((run) => run.paragraphBreak)) return false;
  const insertable = runs.filter((run) => !run.paragraphBreak && run.text);
  if (!insertable.length) return false;

  breakTypingSession();
  const session = typingSessionForKey();
  const node = selection.focus.node;
  let offset = selection.focus.offset;
  for (const run of insertable) {
    await runEdit(
      () => doc.suggestStyledInsert(
        node,
        offset,
        run.text,
        run.bold,
        run.italic,
        run.underline,
        run.strike,
        run.sizeHalfPoints,
        run.color,
        run.highlight,
        run.vertAlign,
        run.font,
        undefined,
        new Date().toISOString(),
        session,
      ),
      { typing: true },
    );
    offset += run.text.length;
  }
  // Close the gesture explicitly so a later, unrelated keystroke cannot
  // merge into this paste's group (mirrors the pause-based boundary real
  // typing uses; nothing else calls `typingSessionForKey` between here and
  // the next real key).
  breakTypingSession();
  return true;
}

/** Replaces the selection with a rich-run clipboard fragment, as one
 * undoable action (`doc.pasteRichRuns` — the paste counterpart of
 * `copyRichRuns`). `runsJson` must be a JSON array in the shape
 * `copyRichRuns` produces. */
async function pasteRichRunsJson(runsJson) {
  if (!doc || !selection) return;
  // Retain the rich fragment so the paste-options chip can re-apply it as
  // "Merge formatting" (emphasis kept, font/size/color dropped). Consumed and
  // cleared by `offerPasteOptions`.
  lastRichPasteJson = runsJson;
  if (reviewMode === "suggesting") {
    let runs = null;
    try {
      runs = JSON.parse(runsJson);
    } catch { /* malformed payload; runs stays null */ }
    if (await pasteTrackedRichRuns(runs)) return;
    // Falls back to a flattened, single-format tracked replace/insert for
    // the cases `pasteTrackedRichRuns` intentionally does not cover yet
    // (see its doc comment): an existing selection to replace, or
    // multi-paragraph content.
    const text = Array.isArray(runs)
      ? runs.map((run) => run.paragraphBreak ? "\n" : String(run.text ?? "")).join("")
      : "";
    if (text) {
      await pasteText(text, "paste");
      return;
    }
    // No plain-text fallback could be derived (malformed clipboard payload, or
    // an entirely non-text rich fragment) — `doc.pasteRichRuns` has no tracked
    // representation, so this must fail closed rather than silently apply
    // untracked (REVIEW-GAP-004).
    blockUntrackedInSuggesting();
    return;
  }
  const { anchor, focus } = selection;
  await runEdit(() =>
    doc.pasteRichRuns(anchor.node, anchor.offset, focus.node, focus.offset, runsJson),
  );
}

/** Tries to paste `html` as a rich fragment: the internal round-trip marker
 * if present (an OpenDoc-to-OpenDoc copy, lossless), else a best-effort
 * sanitized parse of the DOM (an external app's paste — Word, Docs, a
 * browser selection). Returns whether anything was pasted, so the caller can
 * fall back to plain text when `html` carries no usable content. */
async function pasteHtml(html) {
  if (!html) return false;
  const internal = extractMarker(html);
  if (internal) {
    let parsed = null;
    try {
      parsed = JSON.parse(internal);
    } catch { /* not JSON — treat as no usable internal payload below */ }
    // A structured fragment (`{ blocks, runs }`) reconstructs tables/lists in
    // Editing mode; Suggesting mode has no tracked representation for structural
    // paste (GAP-009), so it uses the flat runs. If the engine declines the
    // structured paste (a range selection, or a caret inside a table cell), fall
    // back to the flat runs too.
    if (parsed && !Array.isArray(parsed) && Array.isArray(parsed.blocks)) {
      if (
        reviewMode !== "suggesting" &&
        (await pasteStructured(JSON.stringify({ blocks: parsed.blocks })))
      ) {
        return true;
      }
      await pasteRichRunsJson(JSON.stringify(parsed.runs ?? []));
      return true;
    }
    await pasteRichRunsJson(internal);
    return true;
  }
  const parsed = new DOMParser().parseFromString(html, "text/html");
  // An external `<table>` or `<ul>`/`<ol>` pastes as REAL structure (a table, or
  // bullet/numbered list paragraphs) via `pasteExternalStructured`, as one
  // undoable action — before the flat rich-run fallback that would flatten it to
  // text. Suggesting mode has no tracked structural representation (GAP-009), so it
  // keeps the flat rich path; and a structured insert only lands at a collapsed
  // body caret — otherwise the engine declines and we fall back below.
  if (reviewMode !== "suggesting") {
    const structured = htmlToStructured(parsed.body);
    if (structured && (await pasteExternalStructured(structured))) return true;
  }
  const runs = htmlToRuns(parsed.body);
  if (!runs.length) return false;
  await pasteRichRunsJson(JSON.stringify(runs));
  return true;
}

/** Editing-mode paste of external structure (a foreign `<table>` / `<ul>`/`<ol>`
 * parsed by `htmlToStructured`): reconstructs real tables and list paragraphs at
 * the caret via `doc.pasteExternalStructured`, as one undoable action. Returns
 * true when applied; false when the engine declines (a range selection, or a caret
 * that is not a top-level body paragraph), so the caller falls back to the flat
 * rich-run paste. Calls the engine directly (not through `runEdit`, which swallows
 * the decline) so the fallback can see it. */
async function pasteExternalStructured(fragment) {
  if (!doc || !selection) return false;
  const { anchor, focus } = selection;
  breakTypingSession();
  let res;
  try {
    res = doc.pasteExternalStructured(
      anchor.node,
      anchor.offset,
      focus.node,
      focus.offset,
      JSON.stringify(fragment),
    );
  } catch {
    return false;
  }
  await applyEditResult(res);
  return true;
}

/** Editing-mode structured paste: reconstructs a copied fragment of tables and
 * list paragraphs at the caret via `doc.pasteStructured`, as one undoable
 * action. Returns true when applied; false when the engine declines (a range
 * selection, or a caret that is not a top-level body paragraph), so the caller
 * falls back to the flat rich-run paste. Calls the engine directly (not through
 * `runEdit`, which swallows the decline) so the fallback can see it. */
async function pasteStructured(fragmentJson) {
  if (!doc || !selection) return false;
  const { anchor, focus } = selection;
  breakTypingSession();
  let res;
  try {
    res = doc.pasteStructured(anchor.node, anchor.offset, focus.node, focus.offset, fragmentJson);
  } catch {
    return false;
  }
  await applyEditResult(res);
  return true;
}

/** Paste (⌘V): insert clipboard content at the caret, replacing any
 *  selection. Rich HTML (internal or external) wins when present; plain text
 *  with newline-as-paragraph-split remains the fallback. */
async function paste(event = null) {
  if (!doc || !selection) return;
  // Read-only Viewing mode blocks paste up front (it still calls
  // preventDefault below) so no clipboard read or insertion is attempted.
  if (reviewMode === "viewing") {
    if (event?.clipboardData) event.preventDefault();
    blockMutationInViewing();
    return;
  }
  if (event?.clipboardData) {
    event.preventDefault();
    // A pasted image (a clipboard file item of an image type) is inserted as a
    // picture, matching Word/Docs — before the text/HTML fallback.
    const imageItem = [...(event.clipboardData.items ?? [])].find(
      (it) => it.kind === "file" && it.type.startsWith("image/"),
    );
    if (imageItem) {
      const file = imageItem.getAsFile();
      if (file) await insertImageFromBlob(file);
      return;
    }
    const html = event.clipboardData.getData("text/html");
    const plain = event.clipboardData.getData("text/plain");
    if (await pasteHtml(html)) {
      offerPasteOptions(plain);
      return;
    }
    await pasteText(plain);
    return;
  }
  try {
    let plain = "";
    try { plain = await navigator.clipboard.readText(); } catch { /* html-only clipboard */ }
    if (navigator.clipboard.read) {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        if (!item.types.includes("text/html")) continue;
        const html = await (await item.getType("text/html")).text();
        if (await pasteHtml(html)) {
          offerPasteOptions(plain);
          return;
        }
      }
    }
    await pasteText(plain);
  } catch (err) {
    console.warn("paste failed:", err);
    setStatus("Clipboard paste was blocked by the browser", "err");
  }
}

/** Paste as plain text (⌘/Ctrl+Shift+V): drops all formatting, keeping only the
 *  clipboard's text through the existing `pasteText` path. */
async function pasteAsText() {
  if (!doc || !selection) return;
  if (reviewMode === "viewing") {
    blockMutationInViewing();
    return;
  }
  try {
    const text = await navigator.clipboard.readText();
    if (text) await pasteText(text);
  } catch (err) {
    console.warn("paste text failed:", err);
    setStatus("Clipboard paste was blocked by the browser", "err");
  }
}

// ---- Paste options affordance (Q3) ------------------------------------------
// After a rich paste, a small chip near the caret lets the user switch that
// paste to text-only (undo the rich insertion, re-paste as plain text).
const pasteOptionsEl = document.getElementById("pasteOptions");
const pasteOptionsTextOnlyBtn = document.getElementById("pasteOptionsTextOnly");
const pasteOptionsMergeBtn = document.getElementById("pasteOptionsMerge");
const pasteOptionsCloseBtn = document.getElementById("pasteOptionsClose");
let pasteOptionsPlain = null;
let pasteOptionsRuns = null;
// The rich-run JSON of the most recent paste, captured by `pasteRichRunsJson`
// and drained by `offerPasteOptions` for the "Merge formatting" option.
let lastRichPasteJson = null;

function hidePasteOptions() {
  pasteOptionsEl.hidden = true;
  pasteOptionsPlain = null;
  pasteOptionsRuns = null;
}
/** Shows the chip only when the plain text differs from what a rich paste
 *  produced would matter — i.e. there is text to fall back to. "Merge
 *  formatting" additionally needs the rich-run payload the paste applied, so
 *  its button is only shown when that payload is available. */
function offerPasteOptions(plain) {
  const runsJson = lastRichPasteJson;
  lastRichPasteJson = null;
  if (!plain) return hidePasteOptions();
  pasteOptionsPlain = plain;
  pasteOptionsRuns = runsJson;
  pasteOptionsMergeBtn.hidden = !runsJson;
  pasteOptionsEl.hidden = false;
  requestAnimationFrame(positionPasteOptions);
}
function positionPasteOptions() {
  if (pasteOptionsEl.hidden) return;
  const caret = pagesEl.querySelector(".overlay .caret");
  const w = pasteOptionsEl.offsetWidth;
  const h = pasteOptionsEl.offsetHeight;
  const vp = viewportEl.getBoundingClientRect();
  let x;
  let y;
  if (caret) {
    const r = caret.getBoundingClientRect();
    x = r.left;
    y = r.bottom + 6;
  } else {
    x = vp.left + 16;
    y = vp.bottom - h - 16;
  }
  x = Math.max(vp.left + 8, Math.min(x, vp.right - w - 8));
  y = Math.max(vp.top + 8, Math.min(y, vp.bottom - h - 8));
  pasteOptionsEl.style.left = `${Math.round(x)}px`;
  pasteOptionsEl.style.top = `${Math.round(y)}px`;
}
async function switchPasteToTextOnly() {
  const text = pasteOptionsPlain;
  hidePasteOptions();
  if (!text || !doc) return;
  if (doc.canUndo) await runEdit(() => doc.undo());
  await pasteText(text);
  focusEditorSurface();
}
/** "Merge formatting": undo the rich paste and re-insert it with each run's
 *  properties reduced to the emphasis flags only (bold/italic/underline/
 *  strike/vertAlign). Dropping font, size, color, and highlight lets the text
 *  inherit the destination paragraph's formatting — the Word/Docs behavior. */
async function switchPasteToMergeFormatting() {
  const runsJson = pasteOptionsRuns;
  hidePasteOptions();
  if (!runsJson || !doc) return;
  let runs = null;
  try {
    runs = JSON.parse(runsJson);
  } catch {
    /* malformed retained payload; nothing to merge */
  }
  if (!Array.isArray(runs)) return;
  const merged = runs.map((run) =>
    run.paragraphBreak
      ? { paragraphBreak: true }
      : {
          text: run.text,
          bold: run.bold,
          italic: run.italic,
          underline: run.underline,
          strike: run.strike,
          vertAlign: run.vertAlign,
        },
  );
  if (doc.canUndo) await runEdit(() => doc.undo());
  await pasteRichRunsJson(JSON.stringify(merged));
  focusEditorSurface();
}
onButton(pasteOptionsTextOnlyBtn, () => void switchPasteToTextOnly());
onButton(pasteOptionsMergeBtn, () => void switchPasteToMergeFormatting());
onButton(pasteOptionsCloseBtn, hidePasteOptions);
pasteOptionsEl.addEventListener("mousedown", (e) => {
  if (e.target.tagName !== "BUTTON") e.preventDefault();
});
viewportEl.addEventListener("scroll", hidePasteOptions, { passive: true });
document.addEventListener("pointerdown", (e) => {
  if (!pasteOptionsEl.hidden && !pasteOptionsEl.contains(e.target)) hidePasteOptions();
});
document.addEventListener("keydown", (e) => {
  if (pasteOptionsEl.hidden) return;
  if (e.key === "Escape") return hidePasteOptions();
  const editingKey = e.key.length === 1
    || ["Backspace", "Delete", "Enter", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(e.key);
  if (editingKey && !e.metaKey && !e.ctrlKey && !e.altKey) hidePasteOptions();
}, true);

document.addEventListener("copy", (e) => {
  if (editorClipboardEvent(e) && hasRange()) copySelection(e);
});
document.addEventListener("cut", (e) => {
  if (editorClipboardEvent(e) && hasRange()) cut(e);
});
document.addEventListener("paste", (e) => {
  if (editorClipboardEvent(e)) paste(e);
});
document.addEventListener("compositionstart", (e) => {
  if (!editorTextInputEvent(e)) return;
  breakTypingSession();
  composingText = true;
  pendingFormat = null;
  if (selection) showImePreedit(selection.focus.node, selection.focus.offset, e.data || "");
});
document.addEventListener("compositionupdate", (e) => {
  if (!composingText) return;
  updateImePreedit(e.data || "");
});
document.addEventListener("compositionend", async (e) => {
  hideImePreedit();
  if (!editorTextInputEvent(e)) {
    composingText = false;
    return;
  }
  e.preventDefault();
  composingText = false;
  await commitComposedText(e.data || "");
});

const FORMAT_KEYS = { b: "bold", i: "italic", u: "underline" };

document.addEventListener("keydown", async (e) => {
  if (!doc) return;
  // The canvas editor owns keystrokes only while its focus owner is active.
  // Chrome controls, popovers, and link chips keep normal browser semantics.
  if (isInteractiveChromeTarget(e.target) || !eventTargetsEditor(e)) return;

  const mod = e.metaKey || e.ctrlKey;
  const key = e.key;
  const lower = key.toLowerCase();

  if (composingText || e.isComposing || key === "Process") return;

  // The object interaction grammar (docs/85 §4) owns the keyboard while an
  // object is selected. Escape is the two-step exit (editing → selected → text);
  // Enter/Delete act on the object; a selected object swallows text keys so a
  // stale caret is never edited.
  if ((objectResizeDrag || objectMoveDrag) && key === "Escape") {
    e.preventDefault();
    cancelObjectResize(); // Escape during a drag cancels it (docs/85 §4.2)
    cancelObjectMove();
    return;
  }
  // Crop mode owns Enter (apply) and Escape (cancel) before the object grammar.
  if (objectCropSession) {
    if (key === "Enter") {
      e.preventDefault();
      commitCrop();
      return;
    }
    if (key === "Escape") {
      e.preventDefault();
      cancelCrop();
      return;
    }
  }
  if (objectSelection) {
    if (key === "Escape") {
      e.preventDefault();
      if (objectSelection.mode === "editing") {
        objectSelection = { ...objectSelection, mode: "selected" };
      } else {
        objectSelection = null; // collapse to the surrounding-text caret
      }
      clearObjectStatus();
      drawSelection();
      return;
    }
    if (objectSelection.mode === "selected") {
      if (key === "Enter") {
        e.preventDefault();
        enterObjectEditMode(); // double-click's keyboard twin (§4.3)
        return;
      }
      if (key === "Delete" || key === "Backspace") {
        e.preventDefault();
        deleteSelectedObject(); // one undoable delete; gated in Viewing/Suggesting
        return;
      }
      // Arrow keys nudge a FLOATING object's position (Word/Docs); Shift takes a
      // larger step. Only anchored objects have a position — an inline image has
      // none, so its arrows still fall through to move the caret off it.
      const nudge = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[key];
      if (nudge && objectSelection.anchored && !mod) {
        e.preventDefault();
        nudgeSelectedObject(nudge[0], nudge[1], e.shiftKey);
        return;
      }
      // Swallow text-producing keys; navigation/modifier combos fall through so
      // the user can still move the caret off the object.
      if (key.length === 1 && !mod) {
        e.preventDefault();
        return;
      }
    }
  }

  // Word's Windows/Linux shortcut for clearing direct character formatting.
  // macOS keeps Ctrl+Space available to the host/input source.
  if (e.ctrlKey && !e.metaKey && key === " " && hasRange()) {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      setStatus("Clear formatting is not tracked; switch to Editing to apply it", "error");
      return;
    }
    await runToolbarEdit((a, b, c, d) => doc.clearFormatting(a, b, c, d));
    return;
  }

  // Word's "copy formatting" shortcut — arm the format painter from the caret /
  // selection. Its paste twin (⌘/Ctrl+Shift+V) is taken by paste-plain, so a
  // single armed brush + a click/drag is how the copied format is put down.
  if (mod && e.shiftKey && lower === "c") {
    e.preventDefault();
    breakTypingSession();
    armFormatPainter(false);
    return;
  }
  // Clipboard, select-all, history (⌘/Ctrl based).
  if (mod && lower === "c") {
    e.preventDefault();
    breakTypingSession();
    await copySelection();
    return;
  }
  if (mod && lower === "x") {
    e.preventDefault();
    await cut();
    return;
  }
  if (mod && lower === "v") {
    e.preventDefault();
    if (e.shiftKey) await pasteAsText(); // ⌘/Ctrl+Shift+V — keep text only
    else await paste();
    return;
  }
  if (mod && lower === "a") {
    e.preventDefault();
    selectAll();
    return;
  }
  if (mod && lower === "z") {
    e.preventDefault();
    await runEdit(() => (e.shiftKey ? doc.redo() : doc.undo()));
    return;
  }
  if (mod && lower === "y") {
    e.preventDefault();
    await runEdit(() => doc.redo());
    return;
  }
  if (mod && FORMAT_KEYS[lower]) {
    e.preventDefault();
    breakTypingSession();
    toggleFormat(FORMAT_KEYS[lower]);
    return;
  }

  if (!selection) return;

  // Navigation uses an explicit macOS/Windows keymap. It runs before the
  // generic modifier guard so the supported Ctrl/Command/Option combinations
  // reach semantic engine moves. Shift extends every navigation intent.
  const navDir = navigationDirection(e, EDITOR_KEYBOARD_PLATFORM);
  if (navDir === "docStart" || navDir === "docEnd") {
    e.preventDefault();
    navToPosition(navDir === "docStart" ? doc.firstPosition() : doc.lastPosition(), e.shiftKey);
    return;
  }
  if (navDir === "pageUp" || navDir === "pageDown") {
    e.preventDefault();
    navByViewport(navDir, e.shiftKey);
    return;
  }
  if (navDir) {
    e.preventDefault();
    navCaret(navDir, e.shiftKey);
    return;
  }

  // Tab / Shift+Tab indent / outdent the paragraph(s) the selection touches — the
  // word-processor convention (and how lists are demoted/promoted). Caught before
  // `if (mod) return` is irrelevant (Tab carries no ⌘), but before the browser can
  // move focus off the page.
  if (key === "Tab") {
    e.preventDefault();
    pendingFormat = null;
    if (doc.inTable(selection.focus.node)) {
      try {
        const c = doc.moveTableCell(selection.focus.node, !e.shiftKey);
        navToPosition(c, false);
      } catch {
        // First/last-cell boundaries are expected no-ops for this navigation slice.
      }
      return;
    }
    if (reviewMode === "suggesting") {
      setStatus("Indent and list structure changes cannot be tracked yet; switch to Editing", "error");
      return;
    }
    const listKind = doc.listStyleAt(selection.focus.node);
    if (listKind) {
      await runEdit(() =>
        doc.adjustListLevel(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
          e.shiftKey ? -1 : 1,
        ),
      );
    } else {
      await runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, e.shiftKey ? -360 : 360));
    }
    return;
  }

  const { anchor, focus } = selection;
  const range = hasRange();
  const wordDelete = wordDeletionDirection(e, EDITOR_KEYBOARD_PLATFORM);

  if (wordDelete) {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      const start = range
        ? (anchor.offset <= focus.offset ? anchor : focus)
        : wordDelete === "backward"
          ? (() => { const c = doc.moveCaret(focus.node, focus.offset, "wordLeft"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
          : focus;
      const end = range
        ? (anchor.offset <= focus.offset ? focus : anchor)
        : wordDelete === "forward"
          ? (() => { const c = doc.moveCaret(focus.node, focus.offset, "wordRight"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })()
          : focus;
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
      } else {
        setStatus("This word deletion crosses a paragraph and cannot be tracked yet", "error");
      }
      return;
    }
    await runEdit(() =>
      range
        ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset)
        : wordDelete === "backward"
          ? doc.deleteWordBackward(focus.node, focus.offset)
          : doc.deleteWordForward(focus.node, focus.offset),
    );
    return;
  }

  // macOS ⌘Backspace deletes from the caret to the start of the line (⌘Delete to
  // the line end). The engine's `lineStart`/`lineEnd` caret move gives the same
  // boundary Home/End navigation uses, and the span is removed as one undoable
  // range delete. This must run before the `if (mod)` guard, which otherwise
  // swallows the ⌘ chord.
  const lineDelete = lineDeletionDirection(e, EDITOR_KEYBOARD_PLATFORM);
  if (lineDelete) {
    e.preventDefault();
    const boundary = range
      ? null
      : (() => {
          const c = doc.moveCaret(focus.node, focus.offset, lineDelete === "backward" ? "lineStart" : "lineEnd");
          const p = { node: c.node, offset: c.offset };
          c.free();
          return p;
        })();
    const start = range
      ? (anchor.offset <= focus.offset ? anchor : focus)
      : lineDelete === "backward" ? boundary : focus;
    const end = range
      ? (anchor.offset <= focus.offset ? focus : anchor)
      : lineDelete === "backward" ? focus : boundary;
    if (reviewMode === "suggesting") {
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
      } else if (!range) {
        // A no-op (caret already at the line boundary) is fine to swallow; a
        // cross-paragraph span has no tracked representation yet.
        if (start.node !== end.node) setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      } else {
        setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      }
      return;
    }
    if (!(start.node === end.node && start.offset === end.offset)) {
      await runEdit(() => doc.deleteSelection(start.node, start.offset, end.node, end.offset));
    }
    return;
  }

  if (mod) {
    breakTypingSession();
    return; // leave other ⌘/Ctrl shortcuts to the browser
  }

  if (key === "Backspace") {
    e.preventDefault();
    if (!range && focus.offset === 0) {
      const listKind = doc.listStyleAt(focus.node);
      if (listKind) {
        const level = doc.listLevelAt?.(focus.node) ?? 0;
        await runToolbarEdit((a, b, c, d) =>
          level > 0
            ? doc.adjustListLevel(a, b, c, d, -1)
            : doc.toggleList(a, b, c, d, listKind),
        );
        return;
      }
    }
    if (reviewMode === "suggesting") {
      const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : (() => { const c = doc.moveCaret(focus.node, focus.offset, "left"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
      const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : focus;
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        return;
      }
      setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      return;
    }
    await runEdit(() => range ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : doc.deleteBackward(focus.node, focus.offset));
    return;
  }
  if (key === "Delete") {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      const start = range ? (anchor.offset <= focus.offset ? anchor : focus) : focus;
      const end = range ? (anchor.offset <= focus.offset ? focus : anchor) : (() => { const c = doc.moveCaret(focus.node, focus.offset, "right"); const p = { node: c.node, offset: c.offset }; c.free(); return p; })();
      if (start.node === end.node && start.offset < end.offset) {
        await runEdit(() => doc.suggestDelete(start.node, start.offset, end.offset, undefined, new Date().toISOString()));
        return;
      }
      setStatus("This deletion crosses a paragraph and cannot be tracked yet", "error");
      return;
    }
    await runEdit(() => range ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : doc.deleteForward(focus.node, focus.offset));
    return;
  }
  if (key === "Enter") {
    e.preventDefault();
    if (reviewMode === "suggesting") {
      setStatus("Paragraph breaks cannot be tracked yet; switch to Editing to insert one", "error");
      return;
    }
    // Word/Docs convention: Enter on an empty list item exits the list instead
    // of creating another empty bullet/number. The current paragraph remains in
    // place, so the caret does not jump and Undo restores the list marker.
    if (!range) {
      const listKind = doc.listStyleAt(focus.node);
      if (listKind && doc.paragraphLength(focus.node) === 0) {
        const level = doc.listLevelAt?.(focus.node) ?? 0;
        await runToolbarEdit((a, b, c, d) =>
          level > 0
            ? doc.adjustListLevel(a, b, c, d, -1)
            : doc.toggleList(a, b, c, d, listKind),
        );
        return;
      }
    }
    await runEdit(() =>
      doc.insertPlainTextAs(
        anchor.node,
        anchor.offset,
        focus.node,
        focus.offset,
        "\n",
        "paragraphBreak",
      ),
    );
    return;
  }
  // A printable character (single key, no modifiers).
  if (key.length === 1) {
    e.preventDefault();
    const session = typingSessionForKey();
    if (range) {
      pendingFormat = null; // typing over a selection uses the selection's own runs
      if (reviewMode === "suggesting" && anchor.node !== focus.node) {
        setStatus("Cross-paragraph replacement cannot be tracked yet; switch to Editing", "error");
        return;
      }
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestReplace(
            anchor.node,
            Math.min(anchor.offset, focus.offset),
            Math.max(anchor.offset, focus.offset),
            key,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(anchor.node, anchor.offset, focus.node, focus.offset, key, session),
        { typing: true },
      );
    } else if (pendingFormat) {
      const pf = pendingFormat; // armed format persists across consecutive typing
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestStyledInsert(
            focus.node,
            focus.offset,
            key,
            pf.bold,
            pf.italic,
            pf.underline,
            pf.strike,
            pf.sizeHalfPoints,
            pf.color,
            pf.highlight,
            pf.vertAlign,
            pf.font,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeStyledText(
              focus.node,
              focus.offset,
              key,
              pf.bold,
              pf.italic,
              pf.underline,
              pf.strike,
              pf.sizeHalfPoints,
              pf.color,
              pf.highlight,
              pf.vertAlign,
              pf.font,
              session,
            ),
        { typing: true },
      );
    } else {
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestInsert(
            focus.node,
            focus.offset,
            key,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(focus.node, focus.offset, focus.node, focus.offset, key, session),
        { typing: true },
      );
    }
  }
});

async function handleFile(file) {
  if (!file) return;
  // The WASM `open` auto-detects any registered format from the bytes, so accept
  // every format the picker offers (DOCX, ODT, normalized JSON, plain text) — the
  // extension is only a friendly pre-filter; detection is authoritative.
  if (!/\.(docx|odt|json|txt)$/.test(file.name.toLowerCase())) {
    setStatus("Please choose a .docx, .odt, .json, or .txt file", "error");
    return;
  }
  const buf = await file.arrayBuffer();
  await openBytes(new Uint8Array(buf), file.name);
}

fileEl.addEventListener("change", (e) => handleFile(e.target.files[0]));
// ---- Zoom (Q4): editable %, Fit width / Fit page, Ctrl+scroll ---------------
const zoomMenu = document.getElementById("zoomMenu");
const zoomMenuBtn = document.getElementById("zoomMenuBtn");
const clampZoom = (z) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));

/** Fit-to-viewport factor: constrain the first page's width (fit-width) or both
 *  width and height (fit-page) to the viewport, minus comfortable gutters. */
function computeFitZoom(mode) {
  if (!doc) return zoomFactor;
  const size = doc.pageSize(0);
  const wIn = size.widthTwip / TWIPS_PER_INCH;
  const hIn = size.heightTwip / TWIPS_PER_INCH;
  size.free();
  const rect = viewportEl.getBoundingClientRect();
  const availW = Math.max(120, rect.width - 64);
  const availH = Math.max(120, rect.height - 48);
  const fitW = availW / (wIn * BASE_DPI);
  const factor = mode === "fit-page" ? Math.min(fitW, availH / (hIn * BASE_DPI)) : fitW;
  return clampZoom(factor);
}

/** Repaints the zoom input (unless the user is mid-edit) and the preset checks. */
function updateZoomDisplay() {
  if (document.activeElement !== zoomEl) {
    zoomEl.value =
      zoomMode === "fit-width" ? "Fit width"
        : zoomMode === "fit-page" ? "Fit page"
          : `${Math.round(zoomFactor * 100)}%`;
  }
  for (const b of zoomMenu.querySelectorAll(".zoom-preset")) {
    b.setAttribute("aria-checked", String(zoomMode === "custom" && Math.abs(Number(b.dataset.zoom) - zoomFactor) < 1e-6));
  }
  for (const b of zoomMenu.querySelectorAll(".zoom-fit")) {
    b.setAttribute("aria-checked", String(zoomMode === b.dataset.zoomMode));
  }
}

/** Sets a fixed zoom factor (exits any fit mode) and re-renders. */
function setZoom(factor) {
  zoomMode = "custom";
  zoomFactor = clampZoom(factor);
  renderAll();
}
/** Enters a fit mode; the factor is computed at render time. */
function setZoomMode(mode) {
  zoomMode = mode;
  renderAll();
}
function stepZoom(dir) {
  const steps = [0.5, 0.75, 0.9, 1, 1.25, 1.5, 2, 3];
  const cur = zoomFactor;
  const next = dir > 0
    ? steps.find((s) => s > cur + 1e-6) ?? clampZoom(cur + 0.1)
    : [...steps].reverse().find((s) => s < cur - 1e-6) ?? clampZoom(cur - 0.1);
  setZoom(next);
}

/** Commit the typed zoom value: a number (with optional %) sets a fixed zoom;
 *  "fit width"/"fit page" enter the matching fit mode; anything else reverts. */
function commitZoomInput() {
  const raw = zoomEl.value.trim().toLowerCase();
  if (raw.startsWith("fit w") || raw === "width") return setZoomMode("fit-width");
  if (raw.startsWith("fit p") || raw === "page") return setZoomMode("fit-page");
  const pct = parseFloat(raw.replace("%", ""));
  if (Number.isFinite(pct) && pct > 0) setZoom(pct / 100);
  else updateZoomDisplay(); // reject: restore the last valid display
}
zoomEl.addEventListener("change", commitZoomInput);
zoomEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    commitZoomInput();
    zoomEl.blur();
  } else if (e.key === "Escape") {
    updateZoomDisplay();
    zoomEl.blur();
  }
});
zoomEl.addEventListener("focus", () => zoomEl.select());

const zoomPopover = registerPopover(zoomMenuBtn, zoomMenu, updateZoomDisplay);
zoomMenu.addEventListener("click", (e) => {
  const preset = e.target.closest(".zoom-preset");
  const fit = e.target.closest(".zoom-fit");
  if (preset) setZoom(Number(preset.dataset.zoom));
  else if (fit) setZoomMode(fit.dataset.zoomMode);
  else return;
  closePopover(zoomPopover);
});
zoomInBtn.addEventListener("click", () => stepZoom(1));
zoomOutBtn.addEventListener("click", () => stepZoom(-1));

// Ctrl/⌘+scroll over the document zooms (a fixed % centered on the pointer's
// intent), the desktop-editor convention. Passive:false so we can preventDefault
// the page zoom the browser would otherwise do.
viewportEl.addEventListener(
  "wheel",
  (e) => {
    if (!(e.ctrlKey || e.metaKey) || !doc) return;
    e.preventDefault();
    const base = zoomMode === "custom" ? zoomFactor : computeFitZoom(zoomMode);
    setZoom(clampZoom(base * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
  },
  { passive: false },
);

// Re-fit on viewport resize while a fit mode is active.
let fitResizeRaf = 0;
window.addEventListener("resize", () => {
  if (zoomMode === "custom") return;
  cancelAnimationFrame(fitResizeRaf);
  fitResizeRaf = requestAnimationFrame(() => renderAll());
});

// Drag-and-drop anywhere over the viewport.
for (const type of ["dragover", "drop"]) {
  viewportEl.addEventListener(type, (e) => e.preventDefault());
}
viewportEl.addEventListener("dragover", () => viewportEl.classList.add("dragging"));
viewportEl.addEventListener("dragleave", () => viewportEl.classList.remove("dragging"));
viewportEl.addEventListener("drop", (e) => {
  viewportEl.classList.remove("dragging");
  handleFile(e.dataTransfer?.files?.[0]);
});

// ---- Settings: theme + accent + reviewer identity, persisted (OSS-customizable) ----
const settingsBtn = document.getElementById("settingsBtn");
const settingsPanel = document.getElementById("settingsPanel");
const themeSeg = document.getElementById("themeSeg");
const accentSwatches = document.getElementById("accentSwatches");
const accentCustom = document.getElementById("accentCustom");
const settingsReset = document.getElementById("settingsReset");
const authorNameInput = document.getElementById("authorName");
const authorInitialsInput = document.getElementById("authorInitials");

const DEFAULT_SETTINGS = { theme: "system", accent: "#3355c4", authorName: "", authorInitials: "" };
let settings = loadSettings();

function loadSettings() {
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(localStorage.getItem("opendoc.settings") || "{}") };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings() {
  try {
    localStorage.setItem("opendoc.settings", JSON.stringify(settings));
  } catch {
    /* storage disabled — settings apply for the session only */
  }
}

/**
 * Pushes the host's reviewer identity into the open document through the
 * explicit `setActiveAuthor` seam (see docs/68 "Host identity seam" and
 * docs/81 REVIEW-GAP-013) — this is the one place identity crosses from the
 * host UI into the engine. A blank name still resolves to "You" so
 * `suggestInsert`/`suggestDelete`/etc. (which require a non-empty author)
 * keep working out of the box; a blank initials field lets the engine derive
 * initials from the name instead of duplicating that logic here.
 */
function applyActiveAuthorToDocument() {
  if (!doc) return;
  const name = settings.authorName.trim() || "You";
  const initials = settings.authorInitials.trim() || undefined;
  doc.setActiveAuthor(name, initials, undefined);
}

/** Applies the current settings to the document root + reflects them in the panel. */
function applySettings() {
  const root = document.documentElement;
  if (settings.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", settings.theme);
  root.style.setProperty("--accent", settings.accent);

  for (const b of themeSeg.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b.dataset.theme === settings.theme));
  }
  for (const b of accentSwatches.querySelectorAll(".acc[data-accent]")) {
    b.setAttribute(
      "aria-pressed",
      String(b.dataset.accent.toLowerCase() === settings.accent.toLowerCase()),
    );
  }
  accentCustom.value = settings.accent;
  authorNameInput.value = settings.authorName;
  authorInitialsInput.value = settings.authorInitials;
  applyActiveAuthorToDocument();
}

themeSeg.addEventListener("click", (e) => {
  const b = e.target.closest("button[data-theme]");
  if (!b) return;
  settings.theme = b.dataset.theme;
  saveSettings();
  applySettings();
});
accentSwatches.addEventListener("click", (e) => {
  const b = e.target.closest(".acc[data-accent]");
  if (!b) return;
  settings.accent = b.dataset.accent;
  saveSettings();
  applySettings();
});
accentCustom.addEventListener("input", () => {
  settings.accent = accentCustom.value;
  saveSettings();
  applySettings();
});
authorNameInput.addEventListener("input", () => {
  settings.authorName = authorNameInput.value;
  saveSettings();
  applyActiveAuthorToDocument();
});
authorInitialsInput.addEventListener("input", () => {
  settings.authorInitials = authorInitialsInput.value;
  saveSettings();
  applyActiveAuthorToDocument();
});
settingsReset.addEventListener("click", () => {
  settings = { ...DEFAULT_SETTINGS };
  saveSettings();
  applySettings();
});

function toggleSettings(open) {
  const show = open ?? settingsPanel.hidden;
  settingsPanel.hidden = !show;
  settingsBtn.setAttribute("aria-expanded", String(show));
}
settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleSettings();
});
document.addEventListener("click", (e) => {
  if (!settingsPanel.hidden && !settingsPanel.contains(e.target) && e.target !== settingsBtn) {
    toggleSettings(false);
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !settingsPanel.hidden) toggleSettings(false);
});
applySettings();

// ---- Document properties (docProps/core.xml — title, author, subject, …) ----
const propertiesBtn = document.getElementById("propertiesBtn");
const propertiesPanel = document.getElementById("propertiesPanel");
const propTitle = document.getElementById("propTitle");
const propCreator = document.getElementById("propCreator");
const propSubject = document.getElementById("propSubject");
const propCategory = document.getElementById("propCategory");
const propKeywords = document.getElementById("propKeywords");
const propDescription = document.getElementById("propDescription");
const propertiesApplyBtn = document.getElementById("propertiesApply");
const propertiesCancelBtn = document.getElementById("propertiesCancel");
const propertiesCloseBtn = document.getElementById("propertiesClose");
const metaCreated = document.getElementById("metaCreated");
const metaModified = document.getElementById("metaModified");
const metaLastModifiedBy = document.getElementById("metaLastModifiedBy");
const metaLastPrinted = document.getElementById("metaLastPrinted");
const metaRevision = document.getElementById("metaRevision");
const metaLanguage = document.getElementById("metaLanguage");
const metaContentStatus = document.getElementById("metaContentStatus");
const metaVersion = document.getElementById("metaVersion");
const metaApplication = document.getElementById("metaApplication");
const metaAppVersion = document.getElementById("metaAppVersion");
const metaTemplate = document.getElementById("metaTemplate");
const metaCompany = document.getElementById("metaCompany");
const metaManager = document.getElementById("metaManager");
const metaTotalTime = document.getElementById("metaTotalTime");
const metaSavedStats = document.getElementById("metaSavedStats");
const metaCustomSection = document.getElementById("metaCustomSection");
const metaCustomList = document.getElementById("metaCustomList");

const PROP_FIELDS = [
  ["title", propTitle],
  ["creator", propCreator],
  ["subject", propSubject],
  ["category", propCategory],
  ["keywords", propKeywords],
  ["description", propDescription],
];

function displayMetadataValue(element, value, formatter = String) {
  const hasValue = value !== null && value !== undefined && value !== "";
  element.textContent = hasValue ? formatter(value) : "Not set";
  element.classList.toggle("metadata-empty", !hasValue);
  if (hasValue) element.title = String(value);
  else element.removeAttribute("title");
}

function formatMetadataDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function customMetadataValue(value) {
  if (!value || typeof value !== "object") return "";
  if (value.type === "bool") return value.value ? "True" : "False";
  return value.value ?? "";
}

function reflectDocumentMetadata() {
  const metadata = JSON.parse(doc.documentMetadata());
  const core = metadata.core ?? {};
  const app = metadata.app ?? {};

  displayMetadataValue(metaCreated, core.created, formatMetadataDate);
  displayMetadataValue(metaModified, core.modified, formatMetadataDate);
  displayMetadataValue(metaLastModifiedBy, core.lastModifiedBy);
  displayMetadataValue(metaLastPrinted, core.lastPrinted, formatMetadataDate);
  displayMetadataValue(metaRevision, core.revision);
  displayMetadataValue(metaLanguage, core.language);
  displayMetadataValue(metaContentStatus, core.contentStatus);
  displayMetadataValue(metaVersion, core.version);

  displayMetadataValue(metaApplication, app.application);
  displayMetadataValue(metaAppVersion, app.appVersion);
  displayMetadataValue(metaTemplate, app.template);
  displayMetadataValue(metaCompany, app.company);
  displayMetadataValue(metaManager, app.manager);
  displayMetadataValue(
    metaTotalTime,
    app.totalTime,
    (minutes) => `${Number(minutes).toLocaleString()} min`,
  );

  const savedCounts = [
    ["pages", app.pages],
    ["words", app.words],
    ["characters", app.characters],
    ["paragraphs", app.paragraphs],
  ]
    .filter(([, value]) => value !== null && value !== undefined)
    .map(([label, value]) => `${Number(value).toLocaleString()} ${label}`)
    .join(" · ");
  displayMetadataValue(metaSavedStats, savedCounts);

  metaCustomList.replaceChildren();
  const custom = Array.isArray(metadata.custom) ? metadata.custom : [];
  for (const property of custom) {
    const row = document.createElement("div");
    const name = document.createElement("dt");
    const value = document.createElement("dd");
    name.textContent = property.name;
    value.textContent = customMetadataValue(property.value) || "Not set";
    row.append(name, value);
    metaCustomList.append(row);
  }
  metaCustomSection.hidden = custom.length === 0;
}

function syncModalLock() {
  const modalOpen =
    !propertiesPanel.hidden ||
    (typeof pageSetupMenu !== "undefined" && !pageSetupMenu.hidden);
  document.body.classList.toggle("modal-open", modalOpen);
}

function trapModalFocus(e, modal) {
  if (e.key !== "Tab") return;
  const focusable = [
    ...modal.querySelectorAll(
      'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((element) => element.getClientRects().length > 0);
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

function toggleProperties(open) {
  const show = open ?? propertiesPanel.hidden;
  if (show && doc) {
    const current = JSON.parse(doc.documentProperties());
    for (const [key, input] of PROP_FIELDS) input.value = current[key] ?? "";
    reflectDocumentMetadata();
  }
  const returnFocus = !show && propertiesPanel.contains(document.activeElement);
  propertiesPanel.hidden = !show;
  propertiesBtn.setAttribute("aria-expanded", String(show));
  syncModalLock();
  if (show) {
    queueMicrotask(() => propTitle.focus());
  } else if (returnFocus) {
    propertiesBtn.focus({ preventScroll: true });
  }
}
propertiesBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleProperties();
});
propertiesCancelBtn.addEventListener("click", () => toggleProperties(false));
propertiesCloseBtn.addEventListener("click", () => toggleProperties(false));
propertiesApplyBtn.addEventListener("click", async () => {
  if (!doc) return;
  const current = JSON.parse(doc.documentProperties());
  for (const [key, input] of PROP_FIELDS) {
    const value = input.value.trim();
    current[key] = value ? value : null;
  }
  await runEdit(() => doc.setDocumentProperties(JSON.stringify(current)), { gate: true });
  toggleProperties(false);
});
propertiesPanel.addEventListener("mousedown", (e) => {
  if (e.target === propertiesPanel) toggleProperties(false);
});
document.addEventListener("keydown", (e) => {
  if (!propertiesPanel.hidden) {
    if (e.key === "Escape") {
      e.preventDefault();
      toggleProperties(false);
    } else {
      trapModalFocus(e, propertiesPanel);
    }
  }
});

// ---- Page setup (page size, margins, orientation) ----------------------------
const pageSetupBtn = document.getElementById("pageSetupBtn");
const pageSetupMenu = document.getElementById("pageSetupMenu");
const pageOrientationSeg = document.getElementById("pageOrientationSeg");
const pageWidthInput = document.getElementById("pageWidth");
const pageHeightInput = document.getElementById("pageHeight");
const pageMarginTopInput = document.getElementById("pageMarginTop");
const pageMarginBottomInput = document.getElementById("pageMarginBottom");
const pageMarginLeftInput = document.getElementById("pageMarginLeft");
const pageMarginRightInput = document.getElementById("pageMarginRight");
const pageSetupApplyBtn = document.getElementById("pageSetupApply");
const pageSetupCancelBtn = document.getElementById("pageSetupCancel");
const pageSetupCloseBtn = document.getElementById("pageSetupClose");
const pagePreviewSheet = document.getElementById("pagePreviewSheet");
const pagePreviewMargins = document.getElementById("pagePreviewMargins");
const pagePreviewLabel = document.getElementById("pagePreviewLabel");
const pageSetupSection = document.getElementById("pageSetupSection");
const pageColumnCount = document.getElementById("pageColumnCount");
const pageColumnGap = document.getElementById("pageColumnGap");
const pageColumnSeparator = document.getElementById("pageColumnSeparator");

let pageSetupCurrent = null; // the last-fetched {section, pageSize, pageMargins, orientation}

function reflectPageSetupColumns(columns) {
  const value = columns ?? { count: 1, spaceTwips: 0, separator: false };
  pageColumnCount.value = String(Math.min(4, Math.max(1, value.count ?? 1)));
  pageColumnGap.value = pageInchStr(value.spaceTwips ?? 0);
  pageColumnSeparator.checked = value.separator === true;
}

function pageSetupColumnsPayload() {
  const current = pageSetupCurrent.columns;
  const count = Number(pageColumnCount.value) || 1;
  const spaceTwips = inchTwips(pageColumnGap);
  const separator = pageColumnSeparator.checked;
  // Opening Page Setup and changing only page size/margins must not erase
  // explicit unequal column widths. Normalize to equal columns only when a
  // column control itself actually changed.
  if (
    current &&
    count === current.count &&
    spaceTwips === (current.spaceTwips ?? 0) &&
    separator === (current.separator === true)
  ) {
    return current;
  }
  return {
    ...(current ?? {}),
    count,
    spaceTwips,
    separator,
    equalWidth: true,
    columns: [],
  };
}

/** Twips → inches string for a page-geometry field (unlike inchStr, 0 shows
 * as "0" — a page dimension/margin is never meaningfully "unset"). */
function pageInchStr(twip) {
  return (twip / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

function updatePageSetupPreview() {
  const width = Math.max(1, Number(pageWidthInput.value) || 1);
  const height = Math.max(1, Number(pageHeightInput.value) || 1);
  const top = Math.max(0, Number(pageMarginTopInput.value) || 0);
  const bottom = Math.max(0, Number(pageMarginBottomInput.value) || 0);
  const left = Math.max(0, Number(pageMarginLeftInput.value) || 0);
  const right = Math.max(0, Number(pageMarginRightInput.value) || 0);
  const previewPercent = (value, dimension) =>
    `${Math.min(38, Math.max(3, (value / dimension) * 100))}%`;

  pagePreviewSheet.dataset.orientation = width > height ? "landscape" : "portrait";
  pagePreviewSheet.style.setProperty("--page-ratio", `${width} / ${height}`);
  pagePreviewMargins.style.setProperty("--preview-margin-top", previewPercent(top, height));
  pagePreviewMargins.style.setProperty("--preview-margin-bottom", previewPercent(bottom, height));
  pagePreviewMargins.style.setProperty("--preview-margin-left", previewPercent(left, width));
  pagePreviewMargins.style.setProperty("--preview-margin-right", previewPercent(right, width));
  pagePreviewLabel.textContent = `${pageInchStr(width * TWIPS_PER_INCH)} × ${pageInchStr(height * TWIPS_PER_INCH)} in`;
}

function reflectPageSetup() {
  if (!doc) return false;
  const raw = doc.pageSetupSections(selection?.focus?.node ?? "");
  const list = raw === "null" ? null : JSON.parse(raw);
  if (!list?.sections?.length) return false;
  pageSetupSection.replaceChildren();
  for (const [index, section] of list.sections.entries()) {
    const option = document.createElement("option");
    option.value = section.section;
    option.textContent = `Section ${index + 1}`;
    pageSetupSection.appendChild(option);
  }
  pageSetupSection.value = list.current;
  pageSetupCurrent = list.sections.find((section) => section.section === list.current) ?? list.sections[0];
  const { pageSize, pageMargins, orientation } = pageSetupCurrent;
  pageWidthInput.value = pageInchStr(pageSize.widthTwips);
  pageHeightInput.value = pageInchStr(pageSize.heightTwips);
  pageMarginTopInput.value = pageInchStr(pageMargins.topTwips);
  pageMarginBottomInput.value = pageInchStr(pageMargins.bottomTwips);
  pageMarginLeftInput.value = pageInchStr(pageMargins.startTwips);
  pageMarginRightInput.value = pageInchStr(pageMargins.endTwips);
  reflectPageSetupColumns(pageSetupCurrent.columns);
  const activeOrientation =
    orientation ?? (pageSize.widthTwips > pageSize.heightTwips ? "landscape" : "portrait");
  for (const btn of pageOrientationSeg.querySelectorAll("button")) {
    btn.setAttribute("aria-pressed", String(btn.dataset.orientation === activeOrientation));
  }
  updatePageSetupPreview();
  return true;
}

pageSetupSection.addEventListener("change", () => {
  if (!doc) return;
  const raw = doc.pageSetupSections(selection?.focus?.node ?? "");
  const list = raw === "null" ? null : JSON.parse(raw);
  pageSetupCurrent = list?.sections?.find((section) => section.section === pageSetupSection.value) ?? null;
  if (!pageSetupCurrent) return;
  const { pageSize, pageMargins, orientation } = pageSetupCurrent;
  pageWidthInput.value = pageInchStr(pageSize.widthTwips);
  pageHeightInput.value = pageInchStr(pageSize.heightTwips);
  pageMarginTopInput.value = pageInchStr(pageMargins.topTwips);
  pageMarginBottomInput.value = pageInchStr(pageMargins.bottomTwips);
  pageMarginLeftInput.value = pageInchStr(pageMargins.startTwips);
  pageMarginRightInput.value = pageInchStr(pageMargins.endTwips);
  reflectPageSetupColumns(pageSetupCurrent.columns);
  const activeOrientation = orientation ?? (pageSize.widthTwips > pageSize.heightTwips ? "landscape" : "portrait");
  for (const btn of pageOrientationSeg.querySelectorAll("button")) {
    btn.setAttribute("aria-pressed", String(btn.dataset.orientation === activeOrientation));
  }
  updatePageSetupPreview();
});

function togglePageSetup(open) {
  const show = open ?? pageSetupMenu.hidden;
  if (show && !reflectPageSetup()) return; // no section geometry to edit
  const returnFocus = !show && pageSetupMenu.contains(document.activeElement);
  pageSetupMenu.hidden = !show;
  pageSetupBtn.setAttribute("aria-expanded", String(show));
  syncModalLock();
  if (show) {
    queueMicrotask(() =>
      pageOrientationSeg.querySelector('button[aria-pressed="true"]')?.focus(),
    );
  } else if (returnFocus) {
    pageSetupBtn.focus({ preventScroll: true });
  }
}
pageSetupBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  togglePageSetup();
});
pageOrientationSeg.addEventListener("click", (e) => {
  const btn = e.target.closest("button[data-orientation]");
  if (!btn) return;
  for (const b of pageOrientationSeg.querySelectorAll("button")) {
    b.setAttribute("aria-pressed", String(b === btn));
  }
  // Swap width/height to match, mirroring Word's orientation toggle.
  const w = Number(pageWidthInput.value) || 0;
  const h = Number(pageHeightInput.value) || 0;
  const wantLandscape = btn.dataset.orientation === "landscape";
  if (wantLandscape === w > h) return; // already matches
  const widthTwips = inchTwips(pageWidthInput);
  const heightTwips = inchTwips(pageHeightInput);
  pageWidthInput.value = pageInchStr(heightTwips);
  pageHeightInput.value = pageInchStr(widthTwips);
  updatePageSetupPreview();
});
pageSetupCancelBtn.addEventListener("click", () => togglePageSetup(false));
pageSetupCloseBtn.addEventListener("click", () => togglePageSetup(false));
for (const input of [
  pageWidthInput,
  pageHeightInput,
  pageMarginTopInput,
  pageMarginBottomInput,
  pageMarginLeftInput,
  pageMarginRightInput,
]) {
  input.addEventListener("input", updatePageSetupPreview);
}
pageSetupApplyBtn.addEventListener("click", async () => {
  if (!doc || !pageSetupCurrent) return;
  const orientation =
    pageOrientationSeg.querySelector('button[aria-pressed="true"]')?.dataset.orientation ??
    "portrait";
  const payload = {
    section: pageSetupCurrent.section,
    pageSize: {
      widthTwips: inchTwips(pageWidthInput),
      heightTwips: inchTwips(pageHeightInput),
    },
    pageMargins: {
      ...pageSetupCurrent.pageMargins,
      topTwips: inchTwips(pageMarginTopInput),
      bottomTwips: inchTwips(pageMarginBottomInput),
      startTwips: inchTwips(pageMarginLeftInput),
      endTwips: inchTwips(pageMarginRightInput),
    },
    columns: pageSetupColumnsPayload(),
    orientation,
  };
  await runEdit(() => doc.setPageSetup(JSON.stringify(payload)), { gate: true });
  togglePageSetup(false);
});
pageSetupMenu.addEventListener("mousedown", (e) => {
  if (e.target === pageSetupMenu) togglePageSetup(false);
});
document.addEventListener("keydown", (e) => {
  if (!pageSetupMenu.hidden) {
    if (e.key === "Escape") {
      e.preventDefault();
      togglePageSetup(false);
    } else {
      trapModalFocus(e, pageSetupMenu);
    }
  }
});

fileEl.disabled = true;
updateToolbar(); // start with the toolbar controls disabled (no selection yet)
boot();
