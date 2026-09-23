// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads the `casual-doc-wasm` module, opens a user-selected `.docx` fully
// client-side, and blits each rendered page onto a canvas. This is the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import init, { open, engineVersion } from "../pkg/casual_doc_wasm.js";
import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { embedMarker, extractMarker, htmlToRuns, htmlToStructured, runsToHtml } from "./clipboard.mjs";
import { escapeHtml } from "./text_rules.mjs";
import { EXPORT_COMMANDS, exportCommands } from "./export_commands.mjs";
import { editRefusalMessage, mutationBlockedMessage } from "./edit_errors.mjs";
import { renderAccessibilityMirror } from "./a11y_mirror.mjs";
import { createAboutDialog } from "./about_dialog.mjs";
import { renderPagesPanel, reflectPagesPanelSelection } from "./pages_panel.mjs";
import { createBookmarkManager } from "./bookmark_manager.mjs";
import { createGlyphPicker } from "./glyph_picker.mjs";
import { EMOJI_GROUPS, SYMBOL_GROUPS } from "./glyph_sets.mjs";
import { createSpellChecker, spellingContextCommands } from "./spell_check.mjs";
import { OBJECT_LABELS, escapeClimbsToGroup, groupClickAction, nextObjectIndex, traversalAnnouncement, traversalRoot } from "./object_traversal.mjs";
import { RECOMMENDED_STYLES, caretContexts, offeredStyleNames, previewPx, styleMenuGroups, styleSlug } from "./style_picker.mjs";
import { applyPreviewInk, applyStylePreview, refreshStylePreviews } from "./style_preview.mjs";
import { renderShortcutsReference, shortcutGroups } from "./shortcuts_reference.mjs";
import { printDocument } from "./print.mjs";
import { createCompactToolbar } from "./compact_toolbar.mjs";
import {
  MAX_SCROLL_PX,
  PAGE_GAP_PX,
  PAGE_WINDOW_OVERSCAN_PX,
  buildPageBand,
  docToScroll,
  pageClientRect,
  pageRangeAt,
  scrollToDoc,
} from "./page_scroll.mjs";
import {
  compatibilityOccurrenceCount,
  downloadNameForFormat,
  formatInfo,
} from "./format_io.mjs";
import {
  formatShortcut,
  keyboardPlatform,
  lineDeletionDirection,
  navigationDirection,
  navigationShortcuts,
  wordDeletionDirection,
} from "./keyboard.mjs";
import {
  clampContextMenuPosition,
  moveMenuIndex,
  normalizeMenuEntries,
} from "./context_menu.mjs";
import {
  focusMenuIndex,
  menuItemAt,
  renderMenuLevel as renderMenuLevelRows,
} from "./menu_render.mjs";
import { modalIsOpen, registerModal, setModalHooks } from "./modal.mjs";
import { countLabels } from "./status_counts.mjs";
import { startLocalisation } from "./locale_boot.mjs";
import { isShortcutLike, localizeShortcutGlyphs, localizeShortcutText } from "./shortcut_labels.mjs";
import {
  DRAFT_EXPORT_MODES,
  DraftPresence,
  DraftScheduler,
  HEARTBEAT_MS,
  describeDraftAge,
  describeDraftSize,
  documentKey,
  draftFormatFor,
  draftSlotId,
  evictableSlots,
  offerableDrafts,
  openDraftStore,
  openWordStore,
  slotIsLive,
} from "./drafts.mjs";
import { rovingIndex, tabStopIndex } from "./ribbon_nav.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { createVerticalGoal, orderedSelectionEnds, recoverVerticalMove, sameModelPosition, selectionMatchesRange } from "./caret_navigation.mjs";
import {
  reviewCardSignature,
  reviewCommentIsReplyTo,
  reviewStackHeight,
  stackReviewCards,
} from "./review_layout.mjs";
import {
  formatReviewDate,
  reviewAuthorDisplay,
  reviewCardAriaLabel,
  reviewChangeTypeLabel,
  reviewCommentTooltip,
  reviewFormattingDescription,
  reviewRevisionTooltip,
} from "./review_labels.mjs";
import { previewInkIsLegible } from "./contrast.mjs";
import {
  byteOffsetToStringIndex,
  isWholeWordAt,
  smartQuoteChar,
  transformCase,
} from "./text_rules.mjs";
import {
  EMU_PER_TWIP,
  TWIPS_PER_INCH,
  inchesToTwips,
  optionalInchesToTwips,
  round2,
  signedInchesToTwips,
  twipsToDialogInches,
  twipsToInchText,
} from "./units.mjs";
import {
  announcementRegion,
  documentStateBadge,
  documentTabTitle,
  isObjectSelectionStatus,
  statusClassName,
} from "./status_policy.mjs";
import {
  APP_MENU_SECTIONS,
  flattenCommandTree,
  tableCommandLabel,
  tableMenuPlaceholders,
} from "./command_taxonomy.mjs";
import { FILE_SURFACE, fileMenuSections } from "./command_taxonomy.mjs";
import {
  createMenuBar,
  renderCommandRows,
  renderExportPane,
  renderFilePageInfo,
} from "./command_menu.mjs";
import { reviewCurrentTargetIndex } from "./review_target.mjs";
import {
  fileCategoryRowFor,
  releaseSettingsPanel,
  renderFilePane,
  setFilePane,
} from "./file_pane.mjs";
import { createBackgroundMeasure, pageTotalLabel } from "./background_measure.mjs";
import {
  BLANK_DOCX_PARTS,
  DOCUMENT_TEMPLATES,
  UNTITLED_DOCUMENT_NAME,
  templateBytes,
  templateName,
  templateThumbnail,
  zipStore,
} from "./blank_document.mjs";
import { followExternalTarget } from "./link_targets.mjs";
import { createPointerHover } from "./pointer_hover.mjs";


/** url → Uint8Array of already-fetched font bytes (persists across documents). */
const fontCache = new Map();

// ---- Persisted preferences --------------------------------------------------
// `localStorage` is host policy, not a dependency: with site data blocked, in a
// cross-origin embed, or in some private modes, even *touching* `window
// .localStorage` throws. Every preference read and write goes through these two
// helpers so no future one can be written without the guard — an unguarded
// module-scope read used to throw before the first listener was attached and
// left the whole editor inert (blank page, no ribbon, no keyboard).
// Preferences are a convenience; the editor is fully usable without them, so a
// failure is silent by design and costs the session nothing but persistence.

/** The stored value for `key`, or `fallback` when storage is unavailable. */
function readPref(key, fallback = null) {
  try {
    const value = window.localStorage.getItem(key);
    return value === null ? fallback : value;
  } catch {
    return fallback;
  }
}

/** Persists `value` under `key`; a no-op when storage is unavailable. */
function writePref(key, value) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    /* private mode / storage disabled — the preference applies for this session only */
  }
}

const statusEl = document.getElementById("status");
// Off-screen mirrors of the status line: routine messages announce politely,
// failures assertively (see the markup comment beside them in editor.html).
const statusLiveRegion = document.getElementById("statusLiveRegion");
const statusAlertRegion = document.getElementById("statusAlertRegion");
const reviewLiveRegion = document.getElementById("reviewLiveRegion");
const fileEl = document.getElementById("file");
// The header's Open button. It is the only keyboard route into the app before a
// document exists, so it owns the file picker rather than the picker owning it.
const openBtn = document.getElementById("openBtn");
if (openBtn) openBtn.addEventListener("click", () => fileEl.click());
const zoomEl = document.getElementById("zoom");
// Zoom (Q4): an editable % plus Fit width / Fit page. `zoomFactor` is the live
// scale the renderer/ruler read; `zoomMode` is "custom" for a fixed % or a fit
// mode that recomputes from the viewport on every render/resize.
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 5;
let zoomFactor = 1;
let zoomMode = "custom";
const pagesEl = document.getElementById("pages");
/** The editable focus owner (docs/105 UX-001). `#pages` paints pixels and cannot
 *  own text input: a non-editable element raises no soft keyboard on touch and
 *  fires no `compositionstart/update/end`, so IME, dictation and platform text
 *  services were all unreachable — and the IME spec was green only because it
 *  dispatched synthetic composition events at `document`. This 1px transparent
 *  textarea takes focus instead and rides the caret. It is never a source of
 *  truth: the engine owns the text, keydown still does the inserting, and this
 *  element's value is cleared on every input so it can never accumulate. */
const editorTextInputEl = document.getElementById("editorTextInput");
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
const underlineMenuBtn = document.getElementById("underlineMenuBtn");
const underlineMenu = document.getElementById("underlineMenu");
const underlineColorInput = document.getElementById("underlineColorCustom");
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
const insertPictureBtn = document.getElementById("insertPictureBtn");
const insertShapeBtn = document.getElementById("insertShapeBtn");
const insertTextBoxBtn = document.getElementById("insertTextBoxBtn");
const insertLinkBtn = document.getElementById("insertLinkBtn");
const insertBookmarkBtn = document.getElementById("insertBookmarkBtn");
const insertFieldBtn = document.getElementById("insertFieldBtn");
const insertHeaderBtn = document.getElementById("insertHeaderBtn");
const insertFooterBtn = document.getElementById("insertFooterBtn");
const insertFirstPageVariantBtn = document.getElementById("insertFirstPageVariantBtn");
const insertEvenOddVariantBtn = document.getElementById("insertEvenOddVariantBtn");
const insertSymbolBtn = document.getElementById("insertSymbolBtn");
const insertEmojiBtn = document.getElementById("insertEmojiBtn");
// Layout band (docs/105 UX-010).
const layoutMarginsBtn = document.getElementById("layoutMarginsBtn");
const layoutOrientationBtn = document.getElementById("layoutOrientationBtn");
const layoutSizeBtn = document.getElementById("layoutSizeBtn");
const layoutColumnsBtn = document.getElementById("layoutColumnsBtn");
const layoutIndentDecBtn = document.getElementById("layoutIndentDecBtn");
const layoutIndentIncBtn = document.getElementById("layoutIndentIncBtn");
const layoutIndentFieldsBtn = document.getElementById("layoutIndentFieldsBtn");
const layoutSpacingFieldsBtn = document.getElementById("layoutSpacingFieldsBtn");
const layoutWrapBtn = document.getElementById("layoutWrapBtn");
const layoutPositionBtn = document.getElementById("layoutPositionBtn");
const layoutBringForwardBtn = document.getElementById("layoutBringForwardBtn");
// References band (the IA half of OO-001/OO-005).
const refTocBtn = document.getElementById("refTocBtn");
const refBookmarkBtn = document.getElementById("refBookmarkBtn");
const refCrossRefBtn = document.getElementById("refCrossRefBtn");
const refFootnoteBtn = document.getElementById("refFootnoteBtn");
const refEndnoteBtn = document.getElementById("refEndnoteBtn");
const refFieldBtn = document.getElementById("refFieldBtn");
const refUpdateFieldsBtn = document.getElementById("refUpdateFieldsBtn");
const tabReviewBtn = document.getElementById("tabReview");
const reviewTrackBtn = document.getElementById("reviewTrackBtn");
const reviewShowChangesBtn = document.getElementById("reviewShowChangesBtn");
const reviewPrevBtn = document.getElementById("reviewPrevBtn");
const reviewNextBtn = document.getElementById("reviewNextBtn");
const reviewAcceptBtn = document.getElementById("reviewAcceptBtn");
const reviewRejectBtn = document.getElementById("reviewRejectBtn");
const reviewAcceptAllBtn = document.getElementById("reviewAcceptAllBtn");
const reviewRejectAllBtn = document.getElementById("reviewRejectAllBtn");
const reviewCommentBtn = document.getElementById("reviewCommentBtn");
const reviewPanelBtn = document.getElementById("reviewPanelBtn");
const reviewSpellCheckBtn = document.getElementById("reviewSpellCheckBtn");
const reviewGrammarCheckBtn = document.getElementById("reviewGrammarCheckBtn");
const reviewSmartQuotesBtn = document.getElementById("reviewSmartQuotesBtn");
const insertTableMenu = document.getElementById("insertTableMenu");
const gridPicker = document.getElementById("gridPicker");
const gridLabel = document.getElementById("gridLabel");
const ribbonTabs = [...document.querySelectorAll(".ribbon-tab")];
const ribbonPanels = [...document.querySelectorAll(".ribbon-panel")];
const tabTable = document.getElementById("tabTable");
// Declared with the other chrome elements rather than beside `renderFilePage`:
// `selectRibbonTab` reaches for it, and a `const` in the temporal dead zone
// would throw on the first tab switch of the boot sweep.
const filePageBody = document.getElementById("filePageBody");
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

/** Shows the named ribbon tab's panel and marks its tab selected.
 *
 *  `file` is the one tab whose panel is a PAGE rather than a band: `body`
 *  carries `.file-page-open` while it shows, which is what lifts it over the
 *  work area (ONLYOFFICE's File tab is declared `haspanel: false` for the same
 *  reason). Its rows are rebuilt on entry, so they can never show a stale
 *  `enabled` — the shape `renderCompactToolbar` already uses. */
function selectRibbonTab(name) {
  for (const t of ribbonTabs) {
    const selected = t.dataset.tab === name;
    t.setAttribute("aria-selected", String(selected));
    t.tabIndex = selected ? 0 : -1;
  }
  for (const p of ribbonPanels) p.hidden = p.dataset.panel !== name;
  document.body.classList.toggle("file-page-open", name === "file");
  if (name === "file" && typeof renderFilePage === "function") renderFilePage();
  // Recompute overflow synchronously (not on a later frame) so a control is
  // already in its final inline-or-overflow location the moment the panel shows —
  // the newly shown panel reflows and the previous panel's groups are restored.
  if (typeof updateRibbonOverflow === "function") updateRibbonOverflow();
  // The band's single Tab stop belongs to the panel now showing, not to a
  // control that just went hidden with the previous one.
  ribbonTabStop = null;
  syncRibbonTabStops();
}
for (const t of ribbonTabs) {
  t.addEventListener("click", (event) => {
    if (t.disabled) return;
    selectRibbonTab(t.dataset.tab);
    // Clicking a tab while collapsed brings the ribbon back (Word behavior).
    if (ribbonViewCollapsed) setRibbonCollapsed(false);
    // A tab is a VIEW switch, not an editing target. Clicking one left the
    // button holding the keyboard, so the next thing typed went to the ribbon
    // and vanished — including straight after an insert that had just put the
    // caret in a new text box. Word and Docs never take the caret away for a
    // tab change. Keyboard activation (`detail === 0`, and the arrow-key
    // navigation below) deliberately keeps focus on the tab strip, because
    // there the tabs ARE the thing being operated.
    //
    // The File tab is the exception in both directions: it is a page, not a
    // band, so focus belongs INSIDE it however it was opened. Sending focus back
    // to the document would leave a full-window page showing with the caret
    // behind it — the "opens but never takes focus" defect (`109` HF-070).
    if (t.dataset.tab === "file") {
      filePageBody?.querySelector(".file-page-item:not(:disabled)")?.focus({ preventScroll: true });
    } else if (event.detail !== 0) {
      focusEditorSurface();
    }
  });
}

/** Leaves the File page for the document. Both Escape and the explicit Back
 *  button land here: a full-window route whose only exit is another tab is a
 *  trap, and ONLYOFFICE's own page carries a "Back to Document" item. */
function closeFilePage() {
  if (!document.body.classList.contains("file-page-open")) return false;
  // The settings form lives in the pane while the page is open; it has to go
  // back before the page does, or the document is left with its only settings
  // form inside a hidden panel.
  releaseSettingsPanel();
  selectRibbonTab("home");
  focusEditorSurface();
  return true;
}
// Capture, so Escape leaves the page before any of the editor's own Escape
// handlers see the key. Without capture the first handler to claim the event
// would decide, and which one that is depends on binding order — the
// light-dismiss contract this repo already enforces for dialogs.
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (closeFilePage()) event.stopPropagation();
}, true);

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
  writePref("opendoc.ribbonCollapsed", collapsed ? "1" : "0");
  if (!collapsed && typeof scheduleRibbonOverflow === "function") scheduleRibbonOverflow();
}

if (ribbonViewToggle) {
  ribbonViewToggle.addEventListener("click", () => setRibbonCollapsed(!ribbonViewCollapsed));
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
// second code path. Replace opens the same Find & Replace panel as Find.
pasteBtn.addEventListener("click", () => { paste(); });
cutBtn.addEventListener("click", () => { cut(); });
copyBtn.addEventListener("click", () => { copySelection(); });
replaceBtn.addEventListener("click", () => { if (!findBtn.disabled) findBtn.click(); });

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
  label.textContent = name;
  applyStylePreview(label, name, (style) => doc?.stylePreview?.(style));
  option.appendChild(label);
  option.addEventListener("click", () => {
    runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, name), { paragraphLevel: true });
    closePopover(stylesPopover);
    // Back to the document, as Word and Docs do after a style pick and as
    // `chooseFont` already does.
    focusEditorSurface();
  });
  return option;
}

/** Whether a style can be applied right now. `updateToolbar` owns this; the trigger
 *  carries it, and the refusal REASON lives once on the trigger's title rather than
 *  on each option — a disabled option in an open menu is worse than a control that
 *  says it cannot open. */
let styleCardsEnabled = false;

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
  const node = selection?.focus?.node;
  if (!doc || !node) return {};
  let story = "body";
  if (runningEditBand === "header" || runningEditBand === "footer") story = runningEditBand;
  let inTable = false;
  let listKind = "";
  try {
    inTable = doc.inTable(node);
    listKind = doc.listStyleAt(node) || "";
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
  stylesTriggerLabel.textContent = active || "Normal";
  stylesTrigger.classList.toggle("is-placeholder", !active);
  stylesTrigger.setAttribute(
    "aria-label",
    active ? `Paragraph style: ${active}` : "Paragraph style",
  );
  for (const option of stylesMenuList.querySelectorAll(".style-option")) {
    const selected = option.dataset.style === active;
    option.setAttribute("aria-selected", String(selected));
    option.classList.toggle("is-selected", selected);
  }
  // The band owns exactly one Tab stop across everything it holds. Re-assert that
  // after the control publishes its own, or the band grows a second one on every
  // caret move — which is precisely how it came to have three.
  syncRibbonTabStops();
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
const styleNameDialog = document.getElementById("styleNameDialog");
const styleNameInput = document.getElementById("styleNameInput");
const styleNameConfirm = document.getElementById("styleNameConfirm");
const styleNameCancel = document.getElementById("styleNameCancel");
const styleNameClose = document.getElementById("styleNameClose");
let styleNameResolve = null;

/** The name the dialog will resolve with when it closes. Escape and the
 *  backdrop go through the primitive, which knows nothing about our promise, so
 *  the result is staged here and read back in onClose — a dismissal that skipped
 *  our own close path used to leave promptStyleName pending forever. */
let styleNameResult = null;

const styleNameModal = styleNameDialog
  ? registerModal(styleNameDialog, {
      initialFocus: () => styleNameInput,
      fallbackFocus: () => pagesEl,
      defaultAction: () => finishStyleName(styleNameInput.value.trim()),
      onClose: () => {
        const resolve = styleNameResolve;
        const result = styleNameResult;
        styleNameResolve = null;
        styleNameResult = null;
        if (resolve) resolve(result);
      },
    })
  : null;

function finishStyleName(result) {
  styleNameResult = result;
  styleNameModal?.close();
}

/** Opens the create-style dialog and resolves to the entered name, or null if
 *  cancelled. A single in-flight prompt at a time. */
function promptStyleName() {
  if (!styleNameModal) return Promise.resolve(null);
  return new Promise((resolve) => {
    styleNameResolve = resolve;
    styleNameResult = null;
    styleNameInput.value = "";
    styleNameModal.open();
  });
}

if (styleNameDialog) {
  styleNameConfirm.addEventListener("click", () => finishStyleName(styleNameInput.value.trim()));
  styleNameCancel.addEventListener("click", () => finishStyleName(null));
  styleNameClose.addEventListener("click", () => finishStyleName(null));
}

// ---- Confirmation ----------------------------------------------------------
// The application's single yes/no question. `window.alert`/`confirm`/`prompt`
// are barred in this editor: they cannot be styled or labelled, they freeze the
// wasm engine's event loop while they are up, and browsers increasingly refuse
// them outright — so a "confirmation" the user never sees would silently read
// as cancelled. This card goes through the same modal contract as every other
// dialog, which means Escape and the backdrop already answer it (as "no").
const confirmDialog = document.getElementById("confirmDialog");
const confirmTitleEl = document.getElementById("confirmTitle");
const confirmDescriptionEl = document.getElementById("confirmDescription");
const confirmNoteEl = document.getElementById("confirmNote");
const confirmIconEl = document.getElementById("confirmIcon");
const confirmAcceptBtn = document.getElementById("confirmAccept");
const confirmCancelBtn = document.getElementById("confirmCancel");
const confirmCloseBtn = document.getElementById("confirmClose");
let confirmResolve = null;
let confirmAnswer = false;

const confirmModalController = registerModal(confirmDialog, {
  // Cancel holds focus, so Enter and Space answer "no". Every question this
  // card asks is asked because the alternative destroys something; the safe
  // answer is the one a reflexive keypress lands on.
  initialFocus: () => confirmCancelBtn,
  fallbackFocus: () => pagesEl,
  defaultAction: () => finishConfirm(true),
  onClose: () => {
    const resolve = confirmResolve;
    const answer = confirmAnswer;
    confirmResolve = null;
    confirmAnswer = false;
    // Escape, the backdrop and ✕ never set an answer, so they resolve false —
    // dismissal is a refusal, not a silent yes.
    if (resolve) resolve(answer);
  },
});

function finishConfirm(answer) {
  confirmAnswer = answer;
  confirmModalController.close();
}

/** Asks one yes/no question and resolves to the user's answer. Resolves false
 *  for every form of dismissal. A second call while one is open resolves the
 *  first as refused rather than stacking two questions. */
function confirmModal({ title, message, confirmLabel = "OK", cancelLabel = "Cancel", note = "", icon = "help" }) {
  if (confirmModalController.isOpen) finishConfirm(false);
  confirmTitleEl.textContent = title;
  confirmDescriptionEl.textContent = message;
  confirmNoteEl.textContent = note;
  confirmIconEl.textContent = icon;
  confirmAcceptBtn.textContent = confirmLabel;
  confirmCancelBtn.textContent = cancelLabel;
  return new Promise((resolve) => {
    confirmResolve = resolve;
    confirmAnswer = false;
    confirmModalController.open();
  });
}

confirmAcceptBtn.addEventListener("click", () => finishConfirm(true));
confirmCancelBtn.addEventListener("click", () => finishConfirm(false));
confirmCloseBtn.addEventListener("click", () => finishConfirm(false));

/** Gate for anything that replaces the open document. Returns true when it is
 *  safe to proceed. Reads `documentIsDirty()` — the engine revision watermark,
 *  which fails dirty — so this and the beforeunload guard can never disagree
 *  about whether work would be lost (docs/104 T-14: one dirty signal, one
 *  reader). */
async function confirmDiscardIfEdited() {
  if (!documentIsDirty()) return true;
  return confirmModal({
    title: "Discard unsaved changes?",
    message: `Opening another file replaces “${currentName || "this document"}”. Your edits have not been saved and there is no copy of them anywhere else.`,
    confirmLabel: "Discard and open",
    cancelLabel: "Keep editing",
    note: "Save the .docx first to keep your work.",
    icon: "warning",
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
  ribbonOverflowFrame = requestAnimationFrame(() => {
    updateRibbonOverflow();
    syncRibbonTabStops();
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
}

// --- Ribbon keyboard navigation (WAI-ARIA toolbar pattern) -------------------
// The band is ONE Tab stop; Left/Right/Home/End move between its controls. It
// used to be roughly forty-five stops on Home alone, so anyone driving the
// editor from the keyboard had to walk every formatting button to get past the
// ribbon. Each `.rgroup` also becomes a named `role="group"`, taking its name
// from the caption already printed under it, so a screen reader announces
// "Font, toolbar group" instead of twenty-four anonymous button runs.

/** The controls the band navigates, in visual order.
 *
 * A composite that runs its own roving tabindex (the styles listbox, the table
 * grid picker) counts as ONE item: the band moves focus to it and its own keys
 * take over from there, which is how Word treats a gallery. The "⋯" button is
 * last when it is showing, matching where it sits.
 */
const RIBBON_ITEM_SELECTOR = "button, input, select, [tabindex]";
const RIBBON_COMPOSITE_SELECTOR =
  '[role="listbox"], [role="grid"], [role="menu"], [role="radiogroup"]';

function ribbonBandItems() {
  const panel = ribbonPanels.find((p) => !p.hidden);
  if (!panel || ribbonViewCollapsed) return [];
  const items = [];
  const composites = new Set();
  for (const el of panel.querySelectorAll(RIBBON_ITEM_SELECTOR)) {
    if (el.disabled || el.hidden || el.closest("[hidden]")) continue;
    if (el.getAttribute("aria-hidden") === "true") continue;
    if (!el.offsetWidth && !el.offsetHeight) continue;
    const composite = el.closest(RIBBON_COMPOSITE_SELECTOR);
    if (composite && panel.contains(composite)) {
      if (composites.has(composite)) continue;
      composites.add(composite);
      items.push(composite.querySelector('[tabindex="0"]') || el);
      continue;
    }
    items.push(el);
  }
  if (ribbonOverflowBtn && !ribbonOverflowBtn.hidden) items.push(ribbonOverflowBtn);
  return items;
}

// The control the band hands focus back to. Remembered rather than recomputed
// because overflow and enablement churn the item list constantly, and a band
// that resets to its first control every time undo greys out is worse than no
// memory at all.
let ribbonTabStop = null;
// Callers earlier in this file re-publish their own Tab stops (the styles strip
// does it on every caret move) and must be able to ask the band to re-assert
// itself. Until the band is wired below there is nothing to re-assert, and the
// bindings it reads do not exist yet.
let ribbonNavReady = false;

function syncRibbonTabStops() {
  if (!ribbonNavReady) return;
  const panel = ribbonPanels.find((p) => !p.hidden);
  const items = ribbonBandItems();
  const index = tabStopIndex(items, ribbonTabStop);
  ribbonTabStop = index < 0 ? null : items[index];
  // Neutralize EVERY focusable control in the band, not just the collected
  // items: a composite runs its own roving and re-publishes an internal
  // `tabindex="0"` whenever it rebuilds, which is how the styles gallery kept
  // handing the band two extra Tab stops after this pattern was already in
  // place. One pass over everything, then one control is granted the stop.
  for (const el of panel ? panel.querySelectorAll(RIBBON_ITEM_SELECTOR) : []) {
    el.tabIndex = el === ribbonTabStop ? 0 : -1;
  }
  if (ribbonOverflowBtn) {
    ribbonOverflowBtn.tabIndex = ribbonOverflowBtn === ribbonTabStop ? 0 : -1;
  }
}

if (ribbonBodyEl) {
  // Name every group from the caption already rendered under it, so the two can
  // never disagree the way a hand-written `aria-label` would.
  for (const group of document.querySelectorAll(".rgroup")) {
    const caption = group.querySelector(".rgroup-label")?.textContent?.trim();
    if (!caption) continue;
    group.setAttribute("role", "group");
    group.setAttribute("aria-label", caption);
  }
  for (const panel of ribbonPanels) panel.setAttribute("aria-orientation", "horizontal");

  ribbonBodyEl.addEventListener("keydown", (event) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.target.closest("#ribbonOverflowMenu")) return;
    // A field wants Home/End for its own text, and a composite that runs its own
    // roving owns the arrows once focus is inside it.
    const editable =
      event.target.isContentEditable ||
      /^(?:INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName);
    if (editable) return;
    const inComposite = event.target.closest(RIBBON_COMPOSITE_SELECTOR);
    if (inComposite && ribbonBodyEl.contains(inComposite)) return;
    const items = ribbonBandItems();
    const next = rovingIndex(event.key, items.indexOf(event.target), items.length);
    if (next === null) return;
    event.preventDefault();
    ribbonTabStop = items[next];
    syncRibbonTabStops();
    items[next].focus();
  });
  // Clicking a control makes it the band's Tab stop, so returning by Tab lands
  // where the user last worked rather than back at the start of the row.
  ribbonBodyEl.addEventListener("focusin", (event) => {
    if (event.target.closest("#ribbonOverflowMenu")) return;
    if (ribbonBandItems().includes(event.target)) ribbonTabStop = event.target;
    syncRibbonTabStops();
  });
  ribbonNavReady = true;
  syncRibbonTabStops();
}

// --- Delayed tooltips for icon-only ribbon controls (docs/64 §3) -------------
// A single custom tooltip (~350ms hover/focus delay) shows the control's name +
// shortcut. Reuses the existing `title`/`aria-label` content; the native title
// is suppressed only while the control is actively hovered so it never appears
// alongside the custom one, and is restored on leave (keeping dynamic titles and
// accessibility intact).
const TIP_SELECTOR = ".fmt, .ribbon-tab, .review-mode-seg, .styles-trigger";
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
  // The parenthetical is a shortcut only if it reads like one. "(3×3)" and
  // "(compact view)" are part of the name, and translating them would have
  // printed nonsense in the shortcut slot.
  const parenthetical = match ? match[2].trim() : "";
  // `isShortcutLike`, not a glyph test: the boot sweep has already rewritten
  // these titles to "Ctrl+B" on a non-Apple keyboard, and a glyph-only test
  // would then drop the chord from the tooltip on exactly the platform
  // HF-025 exists for.
  const shortcut = isShortcutLike(parenthetical) ? formatShortcut(parenthetical) : "";
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
const viewingBannerText = document.getElementById("viewingBannerText");
const viewingBannerEdit = document.getElementById("viewingBannerEdit");
/** The banner's own authored sentence, so a read-only reason can replace it and
 *  be put back without a second copy of the string here. */
const VIEWING_BANNER_DEFAULT = viewingBannerText?.textContent ?? "";
const reviewSidebar = document.getElementById("reviewSidebar");
const reviewSidebarBody = document.getElementById("reviewSidebarBody");
const reviewSidebarHeader = document.getElementById("reviewSidebarHeader");
let reviewMode = "editing";
/** Why this DOCUMENT cannot be edited at all, or "" when it can be.
 *
 *  The engine answers this (`editingUnavailableReason`), and exactly one
 *  document in the product says anything: one too large to lay out whole, which
 *  the viewer opens one page-window at a time (`docs/113` §8.3/§8.7). Unlike
 *  Viewing mode — a choice the reader can reverse — this one cannot be switched
 *  off, which is why the mode buttons are disabled and the banner loses its
 *  "Switch to editing" escape. */
let readOnlyReason = "";
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
// HF-088. The width below which the comment column stops being a margin and
// becomes a bottom sheet. Declared here rather than only in a media query
// because the CARD LAYOUT changes shape too, not just the container's box, and
// `style.css` cannot tell main.js which shape it picked. `review_narrow.test`
// asserts the stylesheet and this constant still name the same width.
const REVIEW_SHEET_MAX_WIDTH = 700;
const reviewSheetQuery = window.matchMedia?.(`(max-width: ${REVIEW_SHEET_MAX_WIDTH}px)`) ?? null;
/** True while the review surface should render as a bottom sheet. */
function reviewSheetMode() {
  return !!reviewSheetQuery?.matches;
}
// Crossing the rung swaps the layout shape, so it needs a re-render, not just
// a repaint: the cards' tops mean different things on either side of it.
reviewSheetQuery?.addEventListener?.("change", () => scheduleReviewMarginRender());

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

/** Revision kinds that are real, visitable changes despite carrying no text. */
const REVIEW_TEXTLESS_KINDS = new Set([
  "formatting",
  "paragraph_format",
  "paragraph_mark_insertion",
  "paragraph_mark_deletion",
]);

/** Where a paragraph-level revision is drawn (docs/108 Decision 4).
 *
 *  A paragraph mark revision is a pilcrow-sized marker at the end of the
 *  paragraph, where the mark is. A paragraph formatting change is a bar in the
 *  margin beside the whole paragraph, one per page it spans — Word's "changed
 *  lines" bar — because the change applies to the paragraph, not to a span of
 *  its text. Returns flat `[page, x, y, w, h]` rects in twips, like the engine's
 *  own rect calls. */
function paragraphRevisionRects(revision) {
  const { node, start, end } = revision.anchor ?? {};
  if (!node) return [];
  const caret = () => doc.caretRect(node, Number(end) || 0);
  if (revision.kind !== "paragraph_format") {
    const at = caret();
    if (at.length < 5) return [];
    // A glyph-wide box starting at the caret, so the pilcrow has room to show.
    return [at[0], at[1], at[2], Math.max(at[4] * 0.6, 120), at[4]];
  }
  const lines = doc.selectionRects(node, Number(start) || 0, node, Number(end) || 0);
  const pagesSeen = new Map();
  const include = (page, x, y, h) => {
    const box = pagesSeen.get(page) ?? { left: x, top: y, bottom: y + h };
    box.left = Math.min(box.left, x);
    box.top = Math.min(box.top, y);
    box.bottom = Math.max(box.bottom, y + h);
    pagesSeen.set(page, box);
  };
  for (let i = 0; i + 4 < lines.length; i += 5) include(lines[i], lines[i + 1], lines[i + 2], lines[i + 4]);
  if (!pagesSeen.size) {
    // An empty paragraph selects nothing; its caret still has a line box.
    const at = caret();
    if (at.length >= 5) include(at[0], at[1], at[2], at[4]);
  }
  // The bar belongs in the page margin, beside the text column, wherever the
  // paragraph's own text happens to sit. Anchoring it to the text's left edge put
  // it mid-page for any centred or right-aligned paragraph — the very paragraphs a
  // formatting change most often touches. The margin is the paragraph's own
  // section's, because a document can change margins between sections.
  const BAR_OFFSET = 200; // twips out from the text column
  const BAR_WIDTH = 40;
  const columnStart = paragraphColumnStart(node);
  const flat = [];
  for (const [page, box] of pagesSeen) {
    const edge = Number.isFinite(columnStart) ? columnStart : box.left;
    flat.push(page, Math.max(edge - BAR_OFFSET, 0), box.top, BAR_WIDTH, box.bottom - box.top);
  }
  return flat;
}

// Per-paint memo for `paragraphColumnStart`: `pageSetupSections` walks every
// paragraph, and a heavily reviewed document can carry a formatting change on
// most of them. Reset at the start of each marker paint.
let reviewColumnStartMemo = new Map();

/** The text column's start edge (twips from the page edge) for the section that
 *  owns `node`, or NaN when the engine cannot say. */
function paragraphColumnStart(node) {
  if (reviewColumnStartMemo.has(node)) return reviewColumnStartMemo.get(node);
  let start = Number.NaN;
  try {
    const raw = doc.pageSetupSections(node);
    const list = raw === "null" ? null : JSON.parse(raw);
    const section = list?.sections?.find((item) => item.section === list.current) ?? list?.sections?.[0];
    start = Number(section?.pageMargins?.startTwips);
  } catch {
    start = Number.NaN;
  }
  reviewColumnStartMemo.set(node, start);
  return start;
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
  // A document the engine will not let anyone edit cannot offer Editing or
  // Suggesting: the buttons are disabled WITH the reason, not left live.
  for (const button of reviewModeButtons) {
    button.disabled = !!readOnlyReason;
    if (readOnlyReason) button.title = readOnlyReason;
    else button.removeAttribute("title");
  }
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
  // A read-only document has one mode. Asked for another — by a shortcut, the
  // palette, or a stale click — it stays where it is and says why.
  if (readOnlyReason && mode !== "viewing") {
    setStatus(readOnlyReason, "error");
    mode = "viewing";
  }
  reviewMode =
    mode === "suggesting" ? "suggesting" : mode === "viewing" ? "viewing" : "editing";
  suggestingBanner.hidden = reviewMode !== "suggesting";
  if (viewingBanner) viewingBanner.hidden = reviewMode !== "viewing";
  if (viewingBannerText) {
    viewingBannerText.textContent = readOnlyReason || VIEWING_BANNER_DEFAULT;
  }
  // There is nothing to switch to: the offer would be a dead control.
  if (viewingBannerEdit) viewingBannerEdit.hidden = !!readOnlyReason;
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
  // Paragraph formatting is tracked for as long as Suggesting is on (HF-131).
  // Every paragraph formatting command funnels through one engine choke point,
  // and review decisions build their own operations, so this cannot accidentally
  // track an Editing-mode change or a decision. Dated per command by the engine.
  doc?.setParagraphTracking(reviewMode === "suggesting", undefined);
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

  // Suggesting mode reserves the column whether or not a change exists yet.
  // Keying off `items.length` meant the gutter appeared with the FIRST tracked
  // change, so the whole page slid 85px sideways the moment a reviewer began
  // editing — and slid back on Undo. Everything the user was aiming at moved
  // under the pointer, which is the worst possible moment for the page to jump.
  const show = reviewSidebarPreference ?? (items.length > 0 || reviewMode === "suggesting");
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
  // HF-088: below `REVIEW_SHEET_MAX_WIDTH` there is no margin to put a column
  // in — a 320px column over a 390px window leaves the document about 70px and
  // covers the text it annotates. The column becomes a bottom sheet instead:
  // the page keeps its full width and the top half of the screen, and the
  // comments are a scrollable, dismissable list.
  const sheet = show && reviewSheetMode();
  viewportEl.classList.toggle("review-sheet", sheet);
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
  // canvas anchors (the header floats above via `position: sticky`). In the
  // sheet there are no canvas anchors to align to and the header is the sheet's
  // own title bar, so that offset would hide the first card behind it.
  if (reviewSidebarHeader) {
    reviewSidebarBody.style.marginTop = sheet
      ? "0px"
      : `-${reviewSidebarHeader.offsetHeight}px`;
  }

  if (!items.length) {
    // The sheet is only as tall as its content; the page-height reservation
    // above is a margin-column idea and would leave the sheet scrolling over
    // an empty area taller than the phone.
    if (sheet) reviewSidebarBody.style.height = "auto";
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
    const sig = reviewCardSignature(item, comments, {
      activeItemId: activeReviewItemId,
      deleteConfirmId: reviewDeleteConfirmId,
    });
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
      } else if (item.data.kind === "paragraph_format") {
        const details = reviewFormattingDescription(item.data.formattingDelta);
        body.textContent = `Formatted paragraph${details ? `\n${details}` : ""}`;
      } else if (item.data.kind === "paragraph_mark_insertion") {
        body.textContent = "Added a paragraph break";
      } else if (item.data.kind === "paragraph_mark_deletion") {
        // Say what accepting will do: the break goes and the paragraph joins the
        // next one, which is not obvious from "deleted" alone.
        body.textContent = "Deleted a paragraph break — accepting joins this paragraph to the next";
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

  // Position pass. `stackReviewCards` owns the arithmetic for both shapes: the
  // anchored margin column, and the narrow-width bottom sheet where cards are a
  // plain list (HF-088). Anchors are still computed for the margin shape in
  // document-scroll coordinates, from each item's on-canvas marker.
  const seen = new Set();
  // BAND coordinates, not scroll coordinates. `bandOffset` moves on every
  // scroll once a document is compressed (`page_scroll.mjs`), so an anchor
  // stored in scroll coordinates drifts from its marker by (scale - 1) × the
  // distance scrolled; subtracting it here and adding it back at mount time
  // pins the card. At scale 1 the offset is 0 and nothing changes.
  const anchorY = built.map(
    ({ item }) => item.rect.top - viewportRect.top + viewportEl.scrollTop - bandOffset,
  );
  const heights = built.map(({ entry }) => entry.height);
  const tops = stackReviewCards({
    anchorY,
    heights,
    activeIndex: activeReviewItemId
      ? built.findIndex((b) => b.itemId === activeReviewItemId)
      : -1,
    sheet,
  });
  const layout = built.map(({ itemId, entry }, i) => {
    seen.add(itemId);
    return { itemId, top: tops[i], entry };
  });
  // In the sheet the card body is the scrolled content, so it must be exactly as
  // tall as the stack; in the margin it spans the page stack (set above).
  if (sheet) {
    reviewSidebarBody.style.height = `${Math.round(reviewStackHeight(tops, heights))}px`;
  }
  // Drop cache entries (and their retained DOM) for items no longer present.
  for (const key of [...reviewCardCache.keys()]) {
    if (!seen.has(key)) reviewCardCache.delete(key);
  }
  reviewLayout = layout;
  mountReviewWindow();
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
  // Whichever element owns the scroll owns the window: the viewport in the
  // margin shape, the sheet itself when the sheet is the scroller (HF-088).
  const scroller = reviewSheetMode() ? reviewSidebar : viewportEl;
  const scrollTop = scroller.scrollTop;
  // Stored in band coordinates; the live offset converts them back. Here
  // rather than in the layout pass because this runs on every scroll frame.
  // The bottom sheet scrolls itself and is never the compressed band, so its
  // offset is 0 — taking it unconditionally keeps one expression for both.
  const offset = reviewSheetMode() ? 0 : bandOffset;
  const bandTop = scrollTop - offset - REVIEW_WINDOW_OVERSCAN;
  const bandBottom = scrollTop - offset + scroller.clientHeight + REVIEW_WINDOW_OVERSCAN;
  for (const { itemId, top, entry } of reviewLayout) {
    // The composer and the active/expanded card are always kept mounted: they
    // own live focus/controls the user is interacting with.
    const force = itemId === "composer" || itemId === activeReviewItemId;
    const visible = force || (top + entry.height >= bandTop && top <= bandBottom);
    const mounted = entry.el.parentNode === reviewSidebarBody;
    if (visible) {
      entry.el.style.top = `${Math.round(top + offset)}px`;
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
// In the sheet the cards ride the sheet's own scroll instead (HF-088).
reviewSidebar.addEventListener("scroll", scheduleReviewWindow, { passive: true });
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
const runControls = [
  superBtn,
  subBtn,
  fontSizeSel,
  textColorCaret,
  textColorApplyBtn,
  underlineMenuBtn,
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
];
const saveBtn = document.getElementById("save");
const saveFormatEl = document.getElementById("saveFormat");
const compatibilityStatusEl = document.getElementById("compatibilityStatus");
const zoomInBtn = document.getElementById("zoomIn");
const zoomOutBtn = document.getElementById("zoomOut");
const documentChrome = document.getElementById("documentChrome");
// The menu bar is the ONLY entry point now that Open has left the header, so it
// can no longer wait for a document — a fresh editor would otherwise have no
// keyboard- or pointer-reachable way to open or create one. The `noDoc` commands
// (New, Open) are the only ones enabled until a document exists; every other row
// renders disabled, which is the honest state rather than a hidden one.
if (documentChrome) documentChrome.hidden = false;
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

/** Cap on the devicePixelRatio factor for the raster *backing store*. Was 1.5,
 * which on a Retina display rasterized at 1.5x and stretched by 1.33x — read as
 * "pixelated". The memory it bought is no longer needed (#566/#570, and canvases
 * mount only near the viewport). Capped so a dpr-3 phone does not pay 4x for an
 * invisible gain. Logical page size is dpr-independent: never moves a hit-test. */
const MAX_BACKING_DPR = 2;

/** The clamped devicePixelRatio used only for the raster backing store. */
function backingDpr() {
  return Math.min(window.devicePixelRatio || 1, MAX_BACKING_DPR);
}

/** The currently open document handle (or null). Kept so a zoom change re-renders. */
let doc = null;
/** Measures the rest of a document that opened on a prefix (`docs/116` §7). */
let backgroundMeasure = null;
/** Monotonic token so a slow render from a previous file/zoom is discarded. */
let renderToken = 0;
/** Per-page DOM records: { pageNumber (1-based), wrap, overlay, canvas, wTwip,
 * hTwip, visible }. `wrap` (the sheet box) and `overlay` (caret/selection layer)
 * always exist and are sized from the page geometry; `canvas` is the live raster
 * that is mounted only for pages in/near the viewport (virtualized — see
 * `observePages`) and is `null` for off-screen pages. `visible` mirrors the
 * IntersectionObserver so repaints know whether a live canvas exists. */
let pages = [];
/** The document's insertion point as model anchors — a range when `anchor` and
 * `focus` differ, a caret when they coincide. `focus` trails the pointer.
 *
 * `null` means "no document is open", NOT "the user has not clicked yet": Word
 * and Google Docs give an open document a live insertion point at the start of
 * the body, which is why Insert ▸ Picture / Symbol / Field never ask you to
 * click first. `openBytes` seeds this from the engine's own `firstPosition()`
 * for exactly that reason. */
let selection = null; // { anchor: {node, offset}, focus: {node, offset} }
/** The position `openBytes` seeded the insertion point at, or `null` once the
 * user has taken ownership of the caret. While it is set AND the selection still
 * sits exactly on it, the caret is "implicit": every command acts on it, but the
 * blinking caret is not painted and Find still searches from the top.
 *
 * The paint suppression is not cosmetic. `.overlay .caret` animates
 * unconditionally (style.css) and the editor's keydown handler drops every key
 * unless `#pages` holds focus (`eventTargetsEditor`), so painting a cursor on a
 * document nobody has focused yet would advertise a caret that silently swallows
 * typing.
 *
 * Recording the position rather than a bare flag means any selection MOVE — a
 * click, a Find hit, Select All, arrow keys — makes the caret explicit for free.
 * The three places that clear it outright are the ones where a deliberate action
 * can land back ON the seed and must still show a caret: focusing the surface,
 * navigating from the outline, and applying an edit. */
let implicitCaretAt = null; // { node, offset } | null

/** Whether `selection` is still the untouched load-time default caret. */
function caretIsImplicit() {
  if (!implicitCaretAt || !selection) return false;
  const { anchor, focus } = selection;
  return (
    anchor.node === implicitCaretAt.node &&
    anchor.offset === implicitCaretAt.offset &&
    focus.node === implicitCaretAt.node &&
    focus.offset === implicitCaretAt.offset
  );
}
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
/** True while an IME composition is active on the canvas editor surface. */
let composingText = false;
/** Host gesture identity for history coalescing. The engine also validates exact
 * caret continuity, so this id is permission to merge, never the sole criterion. */
let typingSession = 0;
let typingSessionActive = false;
let lastTypingAt = 0;
const TYPING_PAUSE_MS = 1000;
const EDITOR_KEYBOARD_PLATFORM = keyboardPlatform(navigator);
// HF-025: every shortcut label authored in `editor.html` is declared in Apple
// notation. One sweep of the chrome renders them all for the keyboard actually
// in front of the user; document content is excluded, because a ⌘ in a comment
// or a paragraph is the user's text, not our label.
localizeShortcutGlyphs(document.body, EDITOR_KEYBOARD_PLATFORM);

function focusEditorSurface() {
  // Never steal the keyboard from a modal. Repaints and deferred edit results
  // both call this, and either can land while a dialog is open; the dialog's
  // focus trap would then be fighting the editor for the caret.
  if (modalIsOpen()) return;
  // Focus the editable proxy, not `#pages`. Both are treated as the editor
  // surface by `eventTargetsEditor`, so the existing document-level handlers are
  // unaffected — but only a focused *editable* element raises the soft keyboard
  // and delivers composition events (docs/105 UX-001).
  const target = editorTextInputEl ?? pagesEl;
  target.focus({ preventScroll: true });
  positionEditorTextInput();
}

/** Moves the editable proxy to the caret. An IME candidate window and the iOS
 *  autocorrect bar anchor to the focused element's box, so a proxy parked
 *  off-screen would put the candidate list somewhere the user is not looking.
 *  Viewport coordinates, because the proxy is `position: fixed` and must not
 *  re-parent as pages mount and unmount under virtualization. */
function positionEditorTextInput(focus = selection?.focus) {
  if (!editorTextInputEl) return;
  if (!doc || !focus) return;
  const flat = doc.caretRect(focus.node, focus.offset);
  if (!flat || flat.length < 5) return;
  const [pageNumber, x, y, , h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return;
  // `pages[]` holds page RECORDS, not elements — the sheet element is
  // `page.wrap`, and `scaleOf` already measures it, so reuse that rect rather
  // than taking a second one.
  const { rect, sx, sy } = scaleOf(page);
  editorTextInputEl.style.left = `${rect.left + x * sx}px`;
  editorTextInputEl.style.top = `${rect.top + y * sy}px`;
  editorTextInputEl.style.height = `${Math.max(1, h * sy)}px`;
}

function resetPointerGesture() {
  pointerGesture = null;
  dragging = false;
  if (selectionAutoScrollFrame) cancelAnimationFrame(selectionAutoScrollFrame);
  selectionAutoScrollFrame = 0;
}

function isInteractiveChromeTarget(target) {
  if (!(target instanceof Element)) return false;
  // The editable proxy IS the editor surface. It must be excluded before the
  // selector below, which matches bare `textarea` — otherwise the proxy
  // classifies itself as chrome and every composition event is rejected.
  if (editorTextInputEl && target === editorTextInputEl) return false;
  return !!target.closest(
    "input, select, textarea, button, [contenteditable='true'], .context-menu, .settings-panel, .cmd-overlay, .find-panel, .link-chip",
  );
}

/** True when focus is anywhere on the editing surface — either the paint
 *  surface itself or the editable proxy that now owns text input. Several call
 *  sites used `document.activeElement === pagesEl` as shorthand for this; that
 *  shorthand became wrong the moment the proxy started taking focus. */
function editorSurfaceHasFocus() {
  const active = document.activeElement;
  return active === pagesEl || (!!editorTextInputEl && active === editorTextInputEl);
}

function eventTargetsEditor(event) {
  const active = document.activeElement;
  return (
    event.target === pagesEl ||
    pagesEl.contains(event.target) ||
    editorSurfaceHasFocus() ||
    // The editable proxy (docs/105 UX-001) is the editor surface too. `#pages`
    // stays valid so anything that focuses it directly — including the browser
    // suite — behaves exactly as before.
    (!!editorTextInputEl && (event.target === editorTextInputEl || active === editorTextInputEl))
  );
}

function clientPointEvent(clientX, clientY) {
  return { clientX, clientY };
}

let statusClearTimer = 0;

function setStatus(text, kind = "", { timeout = 0 } = {}) {
  clearTimeout(statusClearTimer);
  statusClearTimer = 0;
  statusEl.textContent = text;
  statusEl.className = statusClassName(kind);
  announceStatus(text, kind);
  if (text && timeout > 0) {
    statusClearTimer = window.setTimeout(() => {
      statusEl.textContent = "";
      statusEl.className = "status";
      statusClearTimer = 0;
    }, timeout);
  }
}

/** Speaks a status message. `announcementRegion` decides which region hears
 *  it. The clear-then-next-frame write is what makes a repeated identical
 *  message (the same edit refused twice) count as a change worth announcing —
 *  the same trick `announceReview` uses. */
function announceStatus(text, kind) {
  const region =
    announcementRegion(kind) === "assertive" ? statusAlertRegion : statusLiveRegion;
  if (!region) return;
  // Only one of the two regions may hold text, or a screen reader browsing the
  // footer meets the last error long after it stopped being true.
  if (statusLiveRegion) statusLiveRegion.textContent = "";
  if (statusAlertRegion) statusAlertRegion.textContent = "";
  if (!text) return;
  requestAnimationFrame(() => {
    region.textContent = text;
  });
}

function setDocumentState(state) {
  const next = documentStateBadge(state);
  documentStateEl.dataset.state = next.state;
  documentStateEl.querySelector(".ms").textContent = next.icon;
  documentStateText.textContent = next.text;
  documentStateEl.title = next.text;
  // Every path that changes document identity or saved-ness passes through
  // here — open, edit, rename, save, restore — so the browser tab is refreshed
  // from one place rather than from five call sites that will drift.
  refreshDocumentTitle();
}

/** The static title in `editor.html`, kept as the no-document fallback. */
const FALLBACK_DOCUMENT_TITLE = document.title;

/**
 * Names the open document in the browser tab.
 *
 * `document.title` was assigned nowhere in `webapp/src`, so every editor tab
 * read the same static string and several open documents were indistinguishable
 * in a tab strip. `documentTabTitle` owns the composition rule.
 *
 * The dirty marker is driven by `documentIsDirty()`, the same engine revision
 * watermark the `beforeunload` guard and `confirmDiscardIfEdited()` read, so
 * the tab, the close warning and the discard gate can never disagree.
 */
function refreshDocumentTitle() {
  document.title = documentTabTitle({
    name: doc ? currentName : "",
    dirty: !!doc && documentIsDirty(),
    fallback: FALLBACK_DOCUMENT_TITLE,
  });
}

function clearObjectStatus() {
  if (isObjectSelectionStatus(statusEl.textContent)) setStatus("");
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
  const labels = countLabels({
    words: s.words,
    characters: s.charactersWithSpaces,
    charactersNoSpaces: s.characters,
    paragraphs: s.paragraphs,
  });
  s.free();
  statWords.textContent = labels.words;
  statChars.textContent = labels.characters;
  statChars.title = labels.charactersTitle;
  statParas.textContent = labels.paragraphs;
  statsEl.title = labels.allTitle;
  statsEl.hidden = false;
  statPages.hidden = false;
  updatePageNumber();
}

/** Measures the rest of a document that opened on a prefix, between frames.
 *
 *  A document past the engine's open budget opens on a measured PREFIX
 *  (`docs/116` §7): the pages it shows are real, there are just fewer of them
 *  than the document has. Left there, the page count would stay wrong forever
 *  and the end of the document would be unreachable — so the rest is measured
 *  from idle time, on a budget the ticker adapts to this machine.
 *
 *  `doc` is captured rather than read: opening another document frees the
 *  wrapper these ticks would call, and a tick that survives that crosses into
 *  freed memory. The capture plus the identity check is what makes a late tick
 *  a no-op instead of a "null pointer passed to rust". */
function armBackgroundMeasure() {
  backgroundMeasure?.stop();
  backgroundMeasure = null;
  const measuring = doc;
  if (!measuring || measuring.pageCountIsExact) return;
  backgroundMeasure = createBackgroundMeasure({
    extend: (blocks) => doc !== measuring || measuring.extendMeasures(blocks),
    // Progress moves the ESTIMATE, which is one string; the page set is not
    // rebuilt for it. Rebuilding on every tick would put a whole-document
    // `renderAll` between the ticks this exists to keep small — the jank it
    // was written to avoid — and would do it repeatedly for a scroll height
    // nobody is looking at yet, since the `~` already says the end of the
    // document is not there.
    onProgress: () => {
      if (doc === measuring) updatePageNumber();
    },
    // Once. The count is now exact, so the page set is rebuilt to match and
    // every page of the document becomes reachable.
    onDone: () => {
      if (doc !== measuring) return;
      void renderAll().then(() => {
        if (doc === measuring) updatePageNumber();
      });
    },
    // `requestIdleCallback` is the right scheduler and Safari does not have
    // it; a timeout is the fallback that keeps the yield rather than the
    // priority. Either way the tick runs BETWEEN frames, which is the point.
    schedule: (run) =>
      typeof requestIdleCallback === "function"
        ? requestIdleCallback(run, { timeout: 500 })
        : setTimeout(run, 0),
    now: () => performance.now(),
  });
  backgroundMeasure.start();
}

/** Cheap current-page update (caret's page / total), for caret moves. */
function updatePageNumber() {
  if (!doc || !pages.length) return;
  let cur = 1;
  if (selection) {
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (flat.length) cur = flat[0];
  }
  statPages.textContent = `Page ${cur} of ${pageTotalLabel(
    pages.length,
    doc.estimatedPageCount,
    doc.pageCountIsExact,
  )}`;
  reflectPagesSelection(cur);
}

async function boot() {
  // Armed before the engine loads, because the hole it closes is open from the
  // first keystroke. The document lives in the wasm heap and nowhere else —
  // there is no server, no autosave and no local copy — so closing the tab,
  // reloading, or pressing Back discarded every edit without a word.
  //
  // A document that has been saved does not warn, because the user has the
  // bytes; `documentIsDirty` decides that from the revision watermark rather
  // than from the state pill, so a save between two edits clears it correctly.
  // Everything past `preventDefault` is legacy the browsers still want, and the
  // wording is the browser's own — a page cannot choose it.
  window.addEventListener("beforeunload", (e) => {
    if (!documentIsDirty()) return;
    e.preventDefault();
    e.returnValue = "";
  });

  try {
    await init();
    setStatus("Ready — open a .docx, .odt, .rtf, .json, or .txt");
    fileEl.disabled = false;
    if (openBtn) openBtn.disabled = false;
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
  } else if (params.get("fixture") === "styled") {
    // A document whose styles carry EXPLICIT colors — Word's own built-in
    // #2F5496 headings plus a near-black body — because no other fixture does.
    // Every shipped sample leaves style colors automatic, which is why the
    // Styles gallery could paint an unreadable label on the dark theme and every
    // test still passed.
    await loadStartupDocument("./styled.docx", "styled.docx");
  } else if (params.get("fixture") === "sections") {
    // A document with a SECTION BREAK that turns landscape at page 5, each
    // section owning its own header/footer — for the running-content e2e, where
    // "which section does this page belong to" is the whole question. No shipped
    // sample has a section break. Generated by the
    // `generate_sections_fixture_docx` engine test.
    await loadStartupDocument("./sections.docx", "sections.docx");
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

  // Last, and deliberately after the startup document: the recovery bar is an
  // offer about work the user already has a document open next to, and it
  // reads the store rather than the engine, so nothing above waits on it
  // (HF-011, docs/112 §4.6).
  await startDrafts();
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


/** Opens a new, empty document. Goes through the same `confirmDiscardIfEdited`
 *  gate as Open, because it replaces what is on screen exactly as Open does —
 *  a new document that silently discarded the last one would be the worst kind
 *  of data loss, the kind the user asked for without knowing. */
async function newBlankDocument() {
  if (!(await confirmDiscardIfEdited())) return;
  await openBytes(zipStore(BLANK_DOCX_PARTS), UNTITLED_DOCUMENT_NAME, () => {
    // A brand-new document is where the user wants to type, not somewhere they
    // have to click first.
    focusEditorSurface();
  });
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
    // Parse BEFORE anything on screen is touched. Freeing the open document
    // first and parsing second meant any file the engine rejected — a corrupt
    // .docx, an unsupported .odt, the wrong file picked by mistake — destroyed
    // the document the user was editing: its pages stayed painted and every
    // control stayed enabled, but `doc` held a freed wrapper, so the next
    // keystroke, click or Save threw "null pointer passed to rust" and there was
    // no way back. Only a successful parse replaces what is on screen; a failed
    // one leaves the previous document open, because the previous document is
    // what the user still has.
    const next = open(bytes);
    hideLinkChip();
    pointerHover.clear();
    // Only now that the new document has PARSED — a failed open leaves the
    // previous document on screen, and handing its draft over before knowing
    // that would orphan the draft of a document the user is still editing.
    // Still before `free()`, because the snapshot needs the live wrapper: the
    // user may have chosen "Discard and open", but that discards the document
    // from the screen, not the only copy of unsaved work from the disk.
    handOverDraftBeforeOpen();
    // Before the free, not after: a background measure tick calls into this
    // wrapper, and between `free()` and the point further down where the new
    // document arms its own ticker there is an `await` — so an idle tick can
    // land in exactly that gap and read freed memory. Stopping here closes the
    // gap by construction instead of relying on the gap being short.
    backgroundMeasure?.stop();
    backgroundMeasure = null;
    if (doc) doc.free();
    doc = next;
    currentSourceFormat = doc.sourceFormat;
    applyActiveAuthorToDocument();
    // Word/Docs: an open document always has an insertion point, so Insert ▸
    // Picture / Symbol / Emoji / Field / Table are live the instant it loads
    // instead of demanding a click first. Seeded from the engine's own
    // body-start position — `firstPosition()` walks the body only, so it is
    // never a header, footer or footnote, and it descends into a leading table
    // exactly as Word's insertion point does. Set here, before `onOpened()` and
    // the first render/`drawSelection`, so the very first `updateToolbar` is
    // already correct and the ribbon is never briefly wrong.
    const startPosition = doc.firstPosition();
    selection = {
      anchor: { node: startPosition.node, offset: startPosition.offset },
      focus: { node: startPosition.node, offset: startPosition.offset },
    };
    // ...but only implicit while the surface is UNFOCUSED. If `#pages` already
    // holds focus when a document opens — drop a .docx onto the viewport after
    // clicking into the previous one, and HTML5 drag never moves focus — no
    // `focus` event will fire to promote it, and the pair would sit in the one
    // state that is genuinely a lie: typing accepted, no caret painted. Decide
    // it here from the live focus instead of waiting for an event that has
    // already happened.
    implicitCaretAt = editorSurfaceHasFocus()
      ? null
      : { node: startPosition.node, offset: startPosition.offset };
    startPosition.free();
    tableSelection = null;
    objectCropSession = null; // a new document invalidates any in-progress crop
      // Ask the engine, before anything is offered, whether this document can be
    // edited at all: everything downstream — the mode buttons, the banner,
    // every refusal message — reads this one answer (`docs/113` §8.7).
    readOnlyReason = String(doc.editingUnavailableReason ?? "");
    reviewMode = readOnlyReason ? "viewing" : "editing";
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
      // Through the one function that owns mode state, so the banner, its
    // (removed) escape hatch and the disabled buttons cannot drift from
    // `reviewMode`.
    if (readOnlyReason) setReviewMode("viewing");
    breakTypingSession();
    currentName = name;
    docTitleEl.value = name;
    documentChrome.hidden = false;
    resetDirtyTracking(currentName);
    // The identity a draft records, computed from the bytes the document was
    // opened from. O(1) in document size — see `documentKey`.
    adoptDraftDocument(name, bytes);
    // Ignored words and cached paragraphs belong to the document that is being
    // replaced; the personal dictionary and the fetched word lists do not, and
    // are kept. Reading the personal words is one IndexedDB round trip and
    // never blocks the open.
    spellChecker.reset();
    void spellChecker.loadPersonal();
    setDocumentState("opened");
    if (saveBtn) saveBtn.disabled = false;
    populateSaveFormats();
    showCompatibilityFindings(0, "export");
    railOutline.disabled = false;
    railPages.disabled = false;
    populateStyles();
    populateTableStyles();
    dropEl.hidden = true;
    document.body.classList.add("doc-loaded");
    // The compact bar is built from the command registry, and at import time
    // that registry holds only the `noDoc` commands — so a reload straight into
    // compact mode rendered an EMPTY bar (every lookup missed, leaving just the
    // four adopted controls). Rebuild it now the document exists.
    if (document.body.classList.contains("compact-mode")) compactToolbarUi.render();
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
    // Paint from the target-bundled metric-compatible faces FIRST. The named web
    // families are ~9.5 MB of variable fonts from a CDN, and awaiting them here
    // meant the document stayed invisible until every one of them had landed —
    // on top of the engine's own download, seconds of blank editor before a
    // single glyph appeared. The bundled faces are metric-compatible substitutes,
    // so this first pass is laid out on the right advance widths rather than
    // being a throwaway approximation, and the upgrade below re-renders once the
    // real faces register. Word and Docs both show the document before every
    // font it references is resolved; so do browsers, for the same reason.
    await renderAll();
    buildOutline();
    buildAccessibilityTree();
    drawSelection();
    armBackgroundMeasure();
    if (typeof onRendered === "function") onRendered();

    // Then upgrade in the background: fetch, register, and re-render. Not
    // awaited, so the caller — and the user — are not held behind the network.
    void provisionFonts(name).then(async (fontWarnings) => {
      // A newer document may have been opened while these bytes were in flight;
      // its own provisioning owns the screen, so this one must not repaint it.
      if (currentName !== name || !doc) return;
      await renderAll();
      buildOutline();
      buildAccessibilityTree();
      drawSelection();
      if (fontWarnings.length > 0) {
        setStatus(
          `Opened ${name}; unavailable web fonts: ${[...new Set(fontWarnings)].join(", ")}`,
          "error",
        );
      }
      // The signal that the document is now painted with its real faces. Tests
      // wait on this so their geometry assertions are never racing an upgrade
      // repaint; nothing in the product blocks on it.
      document.body.dataset.fontsReady = "true";
    });
  } catch (err) {
    // An admission refusal ("this document is too large for the browser") is an
    // expected answer, not a fault: logging it as a console error made a handled
    // case look like a crash, and every spec that asserts a clean console would
    // fail on a deliberately oversized file.
    const message = String(err?.message ?? err);
    if (!/browser editor can hold in memory/.test(message)) console.error(err);
    // `doc` is only ever reassigned after a successful parse, so if one was open
    // it is still open, still painted and still editable. Say so: the failure
    // message is the only thing the user gets, and "could not open" alone reads
    // like the editor is now empty.
    const kept = doc ? " — the document you had open is still here" : "";
    setStatus(`Could not open ${name}: ${message}${kept}`, "error");
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
  if (changed) {
    setDocumentState("edited");
    // A rename changes what Save produces without touching the model, so no
    // revision observes it — but the draft's name is now stale, and a recovery
    // bar naming the old file is a recovery bar the user does not recognise.
    noteDraftDirty();
  }
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
// Buckets whose fetch is in flight. Coverage is now checked after every edit, so
// several checks can overlap — typing three emoji fires three — and without this
// each would see an empty `provisionedFallbackKeys` and start its own ~2 MB
// download of the same face. Membership is cleared on failure so a genuine
// network error is still retried by the next edit.
const inFlightFallbackKeys = new Set();

/** Fetches and registers any script fallback fonts the document now needs but
 *  hasn't got yet (`doc.missingCoverage()` → buckets), skipping ones already
 *  provisioned. Used both on open and after an edit that introduces new glyphs
 *  (e.g. a checklist's `☐`/`☒` markers), so newly-added symbols render instead of
 *  tofu. Returns the keys that failed to load. */
async function provisionMissingFallbacks(label) {
  const warnings = [];
  if (!doc) return warnings;
  const missing = doc.missingCoverage();
  const keys = fallbackKeysFor(missing).filter(
    (key) => !provisionedFallbackKeys.has(key) && !inFlightFallbackKeys.has(key),
  );
  if (keys.length === 0) return warnings;
  setStatus(`Fetching fonts for ${label} (${keys.join(", ")})…`);
  for (const key of keys) inFlightFallbackKeys.add(key);
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
    } finally {
      inFlightFallbackKeys.delete(key);
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
// The pages live inside ONE positioned element, `.page-band`, whose height is
// the scroll container's height. A sheet element (`.page-wrap`) exists only for
// the pages in or near the viewport; every other page is arithmetic in
// `page_scroll.mjs` and nothing in the DOM at all. A live raster `<canvas>` is
// mounted inside a sheet as it comes on screen and released as it leaves, so
// tab memory AND node count are bounded by the viewport, not the page count.
//
// Why a band rather than one sheet per page in flow, and what the mapping from
// scroll space to document space costs: `page_scroll.mjs`, `docs/113` §8.6.

/** Keep a live canvas for pages within ~one viewport-height of the visible band
 *  above and below, so a normal scroll never reveals an unpainted page. */
const PAGE_VIRTUALIZATION_ROOT_MARGIN = "100% 0px";
let pageObserver = null;

/** The current page stack + scroll mapping (`buildPageBand`), or null. */
let pageBandModel = null;
/** The `.page-band` element holding the materialized sheets, or null. */
let bandEl = null;
/** What a page's document-space top is shifted by to place it in the band;
 *  always 0 while the document fits under `MAX_SCROLL_PX`. */
let bandOffset = 0;
/** The band's top in the scroller's own scroll coordinates (the ruler and the
 *  viewport padding sit above it). Measured once per render, not per scroll. */
let bandTopInScroller = 0;
/** The materialized page range, inclusive; `last < first` means none. The
 *  Pages navigator keeps its own, because it windows separately. */
let pageWindow = { first: 0, last: -1 };
let pagesPanelRange = { start: 0, end: -1 };

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

/** Resolve an observed wrap back to its (still-current) page index, or -1. */
function pageIndexOfWrap(wrap) {
  const idx = wrap.__pageIndex;
  return Number.isInteger(idx) && pages[idx]?.wrap === wrap ? idx : -1;
}

/** The pages that currently have a sheet, in page order. Every loop that walks
 *  `pages` looking for DOM goes through this: bounded by the window, not by a
 *  page count that can be 25,556. */
function materializedPages() {
  const out = [];
  for (let i = pageWindow.first; i <= pageWindow.last; i++) {
    if (pages[i]?.wrap) out.push(pages[i]);
  }
  return out;
}

/** How far outside the viewport a page stays materialized: at least a viewport
 *  height (so an ordinary scroll never reveals a missing sheet) and at least
 *  `PAGE_WINDOW_OVERSCAN_PX`, which covers the comment column's mounting band
 *  so a card never anchors to a page that does not exist. */
function pageWindowOverscan() {
  return Math.max(viewportEl.clientHeight, PAGE_WINDOW_OVERSCAN_PX);
}

/** The band-local y a page's sheet is positioned at, in CSS px. */
function pageBandTop(index) {
  return bandOffset + pageBandModel.tops[index];
}

/** The client rect a page's sheet has, or WOULD have if it were materialized.
 *
 *  Every twip→pixel conversion goes through `scaleOf`, and the pages it is
 *  asked about are no longer all in the DOM: a comment eight pages down, a
 *  find match on page 20,000, the caret after a jump. The band's rect plus the
 *  page's position answers those exactly — and identically to
 *  `getBoundingClientRect()`, because that is where the sheet was put. */
function virtualPageRect(index) {
  return pageClientRect(bandEl.getBoundingClientRect(), pageBandModel, index, bandOffset);
}

/** Build the sheet (and overlay) for one page, in page order in the band. */
function materializePage(index) {
  const page = pages[index];
  if (!page || page.wrap) return;
  const wrap = document.createElement("div");
  wrap.className = "page-wrap";
  wrap.dataset.pageNumber = String(index + 1);
  wrap.style.width = `${pageBandModel.widths[index]}px`;
  wrap.style.height = `${pageBandModel.heights[index]}px`;
  wrap.style.left = `${Math.round((pageBandModel.width - pageBandModel.widths[index]) / 2)}px`;
  wrap.style.top = `${pageBandTop(index)}px`;
  wrap.__pageIndex = index;

  // A transparent overlay above the canvas holds the caret/selection we draw
  // ourselves from engine geometry — so the highlight matches the raster
  // exactly (doc 58: custom engine-driven selection, no overlay-vs-glyph drift).
  const overlay = document.createElement("div");
  overlay.className = "overlay";
  wrap.appendChild(overlay);

  // Keep the DOM in page order: hit-testing and the browser suite both read
  // document order off element order.
  let before = null;
  for (const el of bandEl.children) {
    if (Number(el.dataset.pageNumber) > index + 1) {
      before = el;
      break;
    }
  }
  bandEl.insertBefore(wrap, before);
  page.wrap = wrap;
  page.overlay = overlay;
  ensurePageObserver().observe(wrap);
}

/** Drop one page's sheet, canvas and overlay. The page RECORD survives. */
function dematerializePage(page) {
  if (!page.wrap) return;
  pageObserver?.unobserve(page.wrap);
  page.wrap.remove();
  page.wrap = null;
  page.overlay = null;
  page.canvas = null;
  page.visible = false;
}

/** Re-place the window of sheets for the current scroll position.
 *
 *  Called from the scroll handler, so it must be cheap: a binary search, a few
 *  node insertions and one style write per sheet. It repaints the overlay layer
 *  only when the window moved — the sheets that just appeared have empty
 *  overlays, and the markers on them have to be drawn again. */
function updatePageWindow({ force = false } = {}) {
  if (!doc || !pageBandModel || !bandEl) return;
  const viewportHeight = viewportEl.clientHeight;
  const { docY, offset } = scrollToDoc(
    pageBandModel,
    viewportHeight,
    viewportEl.scrollTop - bandTopInScroller,
  );
  const overscan = pageWindowOverscan();
  const range = pageRangeAt(pageBandModel, docY - overscan, docY + viewportHeight + overscan);
  const moved = range.first !== pageWindow.first || range.last !== pageWindow.last;
  if (!force && !moved && offset === bandOffset) return;

  bandOffset = offset;
  const previous = pageWindow;
  pageWindow = range;
  // Only a page that HAD a sheet can lose one: bounded by the window's size.
  for (let i = previous.first; i <= previous.last; i++) {
    if ((i < range.first || i > range.last) && pages[i]) dematerializePage(pages[i]);
  }
  for (let i = range.first; i <= range.last; i++) {
    materializePage(i);
    // The offset moves on every scroll once the document is compressed, so the
    // sheets that stayed have to move with it.
    pages[i].wrap.style.top = `${pageBandTop(i)}px`;
  }
  if (moved || force) {
    spellChecker.noteWindowChanged();
    paintOverlayLayer();
    if (runningEditBand) drawRunningBands(runningEditBand);
    syncPagesPanelToViewport();
  }
}

/** The scroll owner for page materialization. Synchronous on purpose: a sheet
 *  one frame late is a blank page under the reader's eyes, and every geometry
 *  answer (`scaleOf`) before that frame would use a stale offset. */
viewportEl.addEventListener("scroll", () => updatePageWindow(), { passive: true });

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
  for (let i = pageWindow.first; i <= pageWindow.last; i++) {
    const page = pages[i];
    if (!page?.wrap) continue;
    const r = page.wrap.getBoundingClientRect();
    const onscreen = r.bottom >= vr.top - margin && r.top <= vr.bottom + margin;
    page.visible = onscreen;
    if (onscreen) paintPageCanvas(page, i);
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

  // Build the replacement page set off-DOM and publish it atomically. NO sheet
  // elements are created here — only the page records and the band geometry
  // they imply. Sheets, overlays and raster canvases are materialized for the
  // pages near the viewport by `updatePageWindow`, so neither the node count
  // nor the scroll height depends on how long the document is.
  const nextPages = [];
  const sizes = [];
  const renderingStatus =
    `Rendering ${count} page${count === 1 ? "" : "s"} at ${Math.round(zoom * 100)}%…`;
  setStatus(renderingStatus);

  for (let i = 0; i < count; i++) {
    if (token !== renderToken) return;

    // The page box in twips — the domain of hit-testing and selection geometry,
    // and the source of the sheet's CSS size, so the page holds its space
    // whether or not it currently has a sheet at all.
    const size = doc.pageSize(i);
    const wTwip = size.widthTwip;
    const hTwip = size.heightTwip;
    size.free();
    sizes.push({ widthTwip: wTwip, heightTwip: hTwip });
    nextPages.push({
      pageNumber: i + 1,
      wrap: null,
      overlay: null,
      canvas: null,
      wTwip,
      hTwip,
      visible: false,
    });
  }

  if (token !== renderToken) return;
  pages = nextPages;
  pageBandModel = buildPageBand(sizes, cssPerTwip, { gap: PAGE_GAP_PX, maxScroll: MAX_SCROLL_PX });
  // Publish the sheet's rendered width so the stylesheet can size the review
  // gutter against the space that is ACTUALLY spare. CSS cannot know this —
  // it depends on paper size and zoom — and a gutter reserved from space that
  // does not exist pushes the page off the side of the window (docs/64).
  if (count > 0) {
    viewportEl.style.setProperty("--page-width", `${pageBandModel.widths[0]}px`);
  }
  bandEl = document.createElement("div");
  bandEl.className = "page-band";
  bandEl.style.width = `${pageBandModel.width}px`;
  bandEl.style.height = `${pageBandModel.height}px`;
  pageWindow = { first: 0, last: -1 };
  bandOffset = 0;
  pagesEl.replaceChildren(ruler, bandEl); // ruler sits above the pages, same width
  buildRuler();
  // Measured after the band is in the document and before any sheet is placed:
  // the ruler and the viewport's padding sit above the band, and the mapping
  // between scroll space and document space is band-local.
  bandTopInScroller =
    bandEl.getBoundingClientRect().top - viewportEl.getBoundingClientRect().top + viewportEl.scrollTop;
  updatePageWindow({ force: true }); // materialize the sheets around the viewport
  paintPagesInView(); // paint what is on screen now; the observer handles scroll
  // The page set was replaced, so host chrome attached to the old wraps no
  // longer exists. Running-content identity remains model-owned; reconstruct
  // only its visual band from that retained context at the new geometry.
  if (runningEditBand) drawRunningBands(runningEditBand);
  drawSelection(); // re-place any existing selection at the new zoom
  if (token === renderToken) {
    // A command may have reported a more important status while this async
    // render was running. Clear only the progress message this render owns.
    // Clearing it hands the line back to whatever standing condition still
    // wants it — today, "this document is in a language we have no dictionary
    // for", which would otherwise be wiped by the font-upgrade re-render and
    // leave an unchecked document looking like a clean one.
    if (statusEl.textContent === renderingStatus) setStatus(spellChecker.statusNote());
    updateStats();
    if (!pagesPanel.hidden) buildPages();
  }
}

// ---- Selection & copy (doc 58 pipeline: hit-test → selection → draw → copy) ---

/** twip → CSS px scale for a page, from the wrap's live on-screen size, so it
 *  tracks the render under any zoom / DPR / CSS scaling. */
function scaleOf(page) {
  // Measured from the sheet when there is one — so hit-test/selection geometry
  // holds even when the page's raster canvas is virtualized away — and computed
  // from the band otherwise. The two agree: the computed rect is precisely
  // where `materializePage` would put the sheet. What this buys is that a
  // question about a page nowhere near the viewport (a comment's anchor, a find
  // match 20,000 pages down) has an answer instead of throwing.
  const rect = page.wrap ? page.wrap.getBoundingClientRect() : virtualPageRect(page.pageNumber - 1);
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
/** Identifies the story the caret is currently in — the body, a running band, or
 *  a specific text box. Two positions belong to the same range only if this
 *  agrees for both; a range across stories is not representable in
 *  WordprocessingML and no edit can apply to one. */
function currentStoryKey() {
  if (objectSelection?.mode === "editing") return `object:${objectSelection.node}`;
  if (runningEditBand) return `running:${runningEditBand}:${runningEditPage}`;
  return "body";
}

/** Ticks the form checkbox at the caret position `node`/`offset`, and reports
 *  whether it did.
 *
 *  The engine answers "is this inside a checkbox control", so the host never
 *  has to know where content controls are — it asks about whatever the pointer
 *  or the caret landed on. Gated through `runEdit` like every other mutation,
 *  so Viewing refuses it and Suggesting tracks it. */
function toggleFormCheckboxAt(node, offset) {
  if (!doc || !node) return false;
  let control = null;
  try {
    control = doc.formCheckboxAt(node, offset ?? 0) ?? null;
  } catch {
    return false;
  }
  if (!control) return false;
  runEdit(() => doc.toggleFormCheckbox(control));
  return true;
}

function anchorAt(page, event) {
  const { x, y } = pointToTwip(page, event);
  // While a header or footer is open, a point inside a running band belongs to
  // THAT content: without this every click and drag in the band resolved through
  // the body walk, so the caret jumped into the body and text in the header
  // could not be selected at all. Outside the context the body walk still owns
  // margin clicks, which is what keeps entering a header deliberate.
  // Editing a text box: a point inside it belongs to the box's text, so clicks
  // and drags select within it instead of jumping out to the body.
  if (objectSelection?.mode === "editing") {
    // Geometry decides the story, exactly as it does for a header band: the
    // box's own rect says whether this point belongs to the box. `textBoxHitTest`
    // snaps to the nearest line across the WHOLE page, so asking it first meant
    // a click far down in the body still resolved inside the box — the caret
    // moved into the box's text while the user was aiming at a paragraph, and
    // there was no way to click out at all.
    if (pointInsideObject(objectSelection.node, page, x, y)) {
      const inBox = doc.textBoxHitTest(page.pageNumber, x, y);
      if (inBox) {
        const anchor = { node: inBox.node, offset: inBox.offset };
        inBox.free?.();
        return anchor;
      }
      return null; // inside the box but it has no line to land on
    }
    // Outside the box: leave it, then resolve as an ordinary body click — the
    // same deliberate way out that leaving a header uses.
    exitObjectEditMode();
  }
  if (runningEditBand) {
    // One rule, in the engine: geometry decides the story, content decides only
    // the offset inside it. The host used to infer "the user left the header"
    // from a hit-test finding no glyph, so clicking the empty space to the right
    // of a header line — or below its last line, still inside the band — threw
    // the user out of the context they were typing in.
    const hit = doc.resolveClick(page.pageNumber, x, y);
    if (!hit) return null;
    const anchor = { node: hit.node, offset: hit.offset };
    const band = hit.band;
    hit.free?.();
    if (band) {
      // Still in running content: stay there, and follow the user if they moved
      // to the other band or to another page's copy.
      if (!anchor.node) {
        // That band has no content on this page. Entering it is the ask — the
        // same one the double-click makes — so probe it, and create it when the
        // document has none, rather than letting the click do nothing.
        void editRunningContent(band, page);
        return null;
      }
      if (band !== runningEditBand || page.pageNumber !== runningEditPage) {
        setRunningContext(band, page);
      }
      return anchor;
    }
    // A click in the body area leaves the context — the deliberate way back, as
    // in Word, Docs, OnlyOffice and LibreOffice.
    exitRunningEdit();
    return anchor;
  }
  const hit = doc.hitTest(page.pageNumber, x, y);
  if (!hit) return null;
  const anchor = { node: hit.node, offset: hit.offset };
  hit.free(); // release the WASM-owned payload; we copied out its fields
  return anchor;
}

/** Whether a page-local point falls inside `node`'s placed rectangle.
 *
 *  Asked of the engine's own geometry (`objectRect`), so the host never carries
 *  a second opinion about where an object is. */
function pointInsideObject(node, page, x, y) {
  if (!doc || !node) return false;
  let rect = [];
  try {
    rect = doc.objectRect(node); // [page, x, y, w, h] in twips
  } catch {
    return false;
  }
  if (rect.length < 5 || rect[0] !== page.pageNumber) return false;
  return x >= rect[1] && x <= rect[1] + rect[3] && y >= rect[2] && y <= rect[2] + rect[4];
}

/** Leaves a text box's editing context, dropping the object selection with it —
 *  a click in the body is a click in the body, not a half-exit that leaves the
 *  box's chrome on screen. */
function exitObjectEditMode() {
  if (!objectSelection) return;
  objectSelection = null;
  updateObjectSelectionState();
  updateObjectContextBar();
}

/** Clears every page's caret/selection layer. */
function clearOverlays() {
  for (let i = pageWindow.first; i <= pageWindow.last; i++) pages[i]?.overlay?.replaceChildren();
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
  reviewColumnStartMemo = new Map();
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
    const paragraphLevel = revision.kind === "paragraph_format"
      || revision.kind === "paragraph_mark_insertion"
      || revision.kind === "paragraph_mark_deletion";
    const deletionLike = revision.kind === "deletion"
      || revision.kind === "move_from"
      || revision.kind === "paragraph_mark_deletion";
    let rects = paragraphLevel
      ? paragraphRevisionRects(revision)
      : doc.selectionRects(range.startNode, range.startOffset, range.endNode, range.endOffset);
    if (!paragraphLevel && rects.length < 5 && deletionLike && range.startNode === range.endNode) {
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
    // `activeIds` holds `null` for any revision that is not a move or in a group,
    // and `null` is also what `activeReviewItemId` holds when NOTHING is active —
    // so `includes` matched and every such marker rendered active the moment a
    // document opened. Only a real, selected id may activate a marker.
    const active = activeReviewItemId != null && activeIds.filter(Boolean).includes(activeReviewItemId)
      ? " review-revision-marker-active"
      : "";
    const kind = `${revision.kind === "paragraph_format"
      ? "review-revision-marker review-paragraph-format-bar"
      : paragraphLevel
        ? `review-revision-marker review-paragraph-mark-marker ${deletionLike ? "review-paragraph-mark-deleted" : "review-paragraph-mark-inserted"}`
        : deletionLike
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

// ---- Spelling (docs/114, `109` HF-035) --------------------------------------
// Everything but the wiring is in `spell_check.mjs` / `spelling.mjs`; what is
// here is this application's answers to the questions that module asks. The
// three hooks below are the whole of its contact with the editor: `paint()`
// from the overlay repaint, `noteEdited()` from the edit choke point (O(1) —
// it re-arms a timer and nothing else), and `noteWindowChanged()` from the
// scroll that moves the page window.
const spellChecker = createSpellChecker({
  getDoc: () => doc,
  windowPages: () => {
    const inWindow = [];
    for (let i = pageWindow.first; i <= pageWindow.last; i++) {
      const page = pages[i];
      if (page?.overlay) inWindow.push(page);
    }
    return inWindow;
  },
  place,
  caret: () => (selection ? selection.focus : null),
  enabled: () => settings.spellCheck !== false,
  grammarEnabled: () => settings.grammarCheck !== false,
  defaultLanguage: () => settings.spellLanguage || "en-US",
  status: (text, kind) => setStatus(text, kind),
  repaint: () => paintOverlayLayer(),
  openWords: () => openWordStore({}),
});

/** Replaces a flagged word with a suggestion through the SAME path Replace All
 *  uses — one undoable action, closed in Viewing, tracked in Suggesting. */
function replaceMisspelling(flagged, word) {
  void runEdit(
    () => doc.replaceRanges([flagged.node], [flagged.start], [flagged.node], [flagged.end], word),
    { gate: true },
  );
}

/** Draws the current selection from engine geometry: a highlight for a real
 *  range, else a caret at the focus (so a click — or a range with no visible
 *  rects — always shows a cursor). */
/** Repaints everything the page overlays carry, and nothing else.
 *
 *  Split out of `drawSelection` because scrolling now creates and destroys
 *  overlays: a sheet materialized by `updatePageWindow` arrives empty, and the
 *  caret, selection, comment markers and checklist boxes that belong on it have
 *  to be drawn again. The chrome around them (toolbar state, page number, the
 *  comment column) does not change when the window moves, so it stays in
 *  `drawSelection` and is not paid for on every scroll. */
function paintOverlayLayer() {
  if (!doc) return;
  clearOverlays();
  paintReviewMarkers();
  paintChecklistMarkers();
  spellChecker.paint();
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
}

function drawSelection() {
  if (!doc) return;
  paintOverlayLayer();
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

/** Paints the selected object's outline + exact supported resize handles from engine
 *  geometry (docs/85 §3.3), the same overlay mechanism as the caret/highlight so
 *  the chrome matches the raster exactly. Unsupported carrier/edge combinations
 *  are omitted by the engine. */
function paintObjectSelection() {
  const { node } = objectSelection;
  // In crop mode the image shows crop chrome (dimmed cut region + crop handles)
  // in place of the resize handles — the Word/Docs crop experience.
  if (objectCropSession && objectCropSession.node === node) {
    paintObjectCrop();
    return;
  }
  place(doc.objectRect(node), "object-outline");
  const handles = doc.objectHandles(node); // [page, cx, cy, kind] * supported handle count
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
// The engine returns the authored source crop, so entering crop is a refinement
// of the current intent rather than a blind replacement.

/** Enters crop mode on the selected image (or, if already cropping, commits). */
function enterCropMode() {
  if (!doc || !objectSelection || !objectSelection.canCrop) return;
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
  const authoredCrop = doc.objectCrop?.(objectSelection.node);
  const cropValues = authoredCrop && authoredCrop.length === 4
    ? authoredCrop
    : [0, 0, 0, 0];
  objectCropSession = {
    node: objectSelection.node,
    box: [x, y, w, h],
    crop: { l: cropValues[0], t: cropValues[1], r: cropValues[2], b: cropValues[3] },
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
  if (
    !doc ||
    !objectSelection ||
    objectSelection.node !== node ||
    !objectSelection.canResize
  ) return;
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
    root: objectSelection.ref.root,
    handleKind,
    page,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startX: x,
    startY: y,
    startW: w,
    startH: h,
    lastX: x,
    lastY: y,
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
  const isImage = objectSelection?.kind === "image";
  const lockAspect = isCorner && (isImage ? !event.shiftKey : event.shiftKey);
  if (lockAspect) {
    if (Math.abs(newW - drag.startW) >= Math.abs(newH - drag.startH)) {
      newH = Math.max(MIN_OBJECT_TWIP, Math.round(newW / drag.aspect));
    } else {
      newW = Math.max(MIN_OBJECT_TWIP, Math.round(newH * drag.aspect));
    }
  }
  const newX = fw < 0 ? drag.startX + drag.startW - newW : drag.startX;
  const newY = fh < 0 ? drag.startY + drag.startH - newH : drag.startY;
  drag.lastX = newX;
  drag.lastY = newY;
  drag.lastW = newW;
  drag.lastH = newH;
  drag.preview.style.left = `${newX * sx}px`;
  drag.preview.style.top = `${newY * sy}px`;
  drag.preview.style.width = `${newW * sx}px`;
  drag.preview.style.height = `${newH * sy}px`;
  // Say what size you are dragging TO. The preview was an outline and nothing
  // else, so resizing to a specific size meant releasing, opening the properties
  // panel to read what you got, and correcting it there. Docs shows a W×H bubble
  // during the drag and Word live-updates its Size box; this is the same promise
  // in the place the eye already is.
  updateObjectResizeReadout(drag, newW, newH);
  event.preventDefault();
}

/** Paints the live dimensions onto the resize preview, in inches to two places
 *  because that is the unit the properties panel accepts — a readout in a unit
 *  the user cannot then type back is decoration. Announced politely as well: the
 *  bubble is meaningless to a screen reader, and resize is keyboard-reachable. */
function updateObjectResizeReadout(drag, widthTwip, heightTwip) {
  let readout = drag.readout;
  if (!readout) {
    readout = document.createElement("span");
    readout.className = "object-resize-readout";
    // Not a live region itself: it changes on every pointer move, and a live
    // region here would flood the buffer. The committed size is announced once
    // by `finishObjectResize`.
    readout.setAttribute("aria-hidden", "true");
    drag.preview.appendChild(readout);
    drag.readout = readout;
  }
  const inches = (twip) => (twip / TWIPS_PER_INCH).toFixed(2);
  readout.textContent = `${inches(widthTwip)} × ${inches(heightTwip)} in`;
}

/** Commits (or cancels) the resize on release through one engine geometry
 * transaction, converting the final page-local rectangle from twips to EMU.
 * Returns whether a drag was active. */
function finishObjectResize(event) {
  if (!objectResizeDrag) return false;
  const drag = objectResizeDrag;
  objectResizeDrag = null;
  drag.preview.remove();
  event.preventDefault();
  const changed =
    Math.abs(drag.lastX - drag.startX) >= 8 ||
    Math.abs(drag.lastY - drag.startY) >= 8 ||
    Math.abs(drag.lastW - drag.startW) >= 8 ||
    Math.abs(drag.lastH - drag.startH) >= 8;
  if (changed) {
    runEdit(
      () =>
        doc.resizeObject(
          drag.root,
          drag.lastX * EMU_PER_TWIP,
          drag.lastY * EMU_PER_TWIP,
          drag.lastW * EMU_PER_TWIP,
          drag.lastH * EMU_PER_TWIP,
        ),
      { gate: true },
    );
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
    pagesEl.dataset.objectSurface = objectSelection.ref.surface;
    pagesEl.dataset.objectRoot = objectSelection.ref.root;
    pagesEl.dataset.objectSubject = objectSelection.ref.subject;
    pagesEl.dataset.objectPath = objectSelection.ref.path.join(".");
    pagesEl.dataset.objectKind = objectSelection.kind;
    pagesEl.dataset.objectMode = objectSelection.mode;
    pagesEl.dataset.objectCapabilities = OBJECT_CAPABILITY_KEYS
      .filter((key) => objectSelection[key])
      .join(",");
  } else {
    delete pagesEl.dataset.objectSelected;
    delete pagesEl.dataset.objectSurface;
    delete pagesEl.dataset.objectRoot;
    delete pagesEl.dataset.objectSubject;
    delete pagesEl.dataset.objectPath;
    delete pagesEl.dataset.objectKind;
    delete pagesEl.dataset.objectMode;
    delete pagesEl.dataset.objectCapabilities;
  }
  reflectShapeFormatState();
}

/** Reflects a selected shape's own fill and outline onto `#pages`, the same way
 *  the selection grammar is reflected. It is what the Fill/Outline swatches
 *  paint from, so what a test reads here is what the user sees on the control —
 *  `none` when the shape has no fill or no outline. */
function reflectShapeFormatState() {
  const format = objectSelection?.kind === "shape" ? selectedShapeFormat() : null;
  if (!format) {
    delete pagesEl.dataset.shapeFill;
    delete pagesEl.dataset.shapeOutline;
    delete pagesEl.dataset.shapeOutlineWidth;
    return;
  }
  pagesEl.dataset.shapeFill = format.fill ?? "none";
  pagesEl.dataset.shapeOutline = format.outline ?? "none";
  pagesEl.dataset.shapeOutlineWidth =
    format.outlineWidthEmu == null ? "none" : String(format.outlineWidthEmu);
}

/** The lazily-created placeholder object context bar (docs/85 §4.1). */
let objectContextBarEl = null;
let objectInspectorEl = null;

/** The object the inspector's fields currently describe. It is what tells a
 *  repaint of the SAME object (where a field the user is typing in must be left
 *  alone) apart from a move to a DIFFERENT one (where leaving the old numbers is
 *  the data-loss defect: Apply then wrote one object's geometry onto another). */
let objectInspectorNode = null;

/** Fills the inspector from the model's live geometry for the selected object.
 *
 *  This runs on every repaint while the panel is open (docs/104 HF-057). It used
 *  to run only on the opening call, so a drag-resize left the panel showing the
 *  pre-drag numbers and the next Apply — or a nudge — put the object back where
 *  it had been; and selecting a second object left the first one's size, alt text
 *  and wrap in the fields, aimed at the new object.
 *
 *  A field the user is mid-edit in is not overwritten, because typing "3.2" and
 *  having the caret jump is its own defect — but that courtesy stops at the
 *  object boundary: when the panel switches objects every field is rewritten,
 *  focused or not, because a stale value there is a wrong write, not a nuisance. */
function reflectObjectInspector() {
  if (!objectInspectorEl || !doc || !objectSelection) return;
  const rect = doc.objectRect(objectSelection.node);
  if (rect.length < 5) return;
  const sameObject = objectInspectorNode === objectSelection.node;
  objectInspectorNode = objectSelection.node;
  // `value` writes are skipped for the focused control only while the panel is
  // still describing the same object.
  const setValue = (element, value) => {
    if (!element) return;
    if (sameObject && element === document.activeElement) return;
    element.value = value;
  };
  const inches = (twips) => String(Math.round(twips / TWIPS_PER_INCH * 100) / 100);
  setValue(objectInspectorEl.querySelector("[data-object-prop=width]"), inches(rect[3]));
  setValue(objectInspectorEl.querySelector("[data-object-prop=height]"), inches(rect[4]));
  setValue(objectInspectorEl.querySelector("[data-object-prop=left]"), inches(rect[1]));
  setValue(objectInspectorEl.querySelector("[data-object-prop=top]"), inches(rect[2]));
  objectInspectorEl.querySelector("[data-object-inspector-kind]").textContent = OBJECT_LABELS[objectSelection.kind] ?? "Object";
  const altField = objectInspectorEl.querySelector("[data-object-inspector-alt]");
  altField.hidden = !objectSelection.canAltText;
  if (objectSelection.canAltText) setValue(altField.querySelector("input"), doc.objectDescr(objectSelection.node) ?? "");
  const wrapField = objectInspectorEl.querySelector("[data-object-inspector-wrap]");
  wrapField.hidden = !objectSelection.canWrap;
  if (objectSelection.canWrap) setValue(wrapField.querySelector("select"), doc.objectWrap(objectSelection.ref.root) || "square");
  const appearance = objectInspectorEl.querySelector("[data-object-inspector-appearance]");
  appearance.hidden = objectSelection.kind !== "shape" || (!objectSelection.canFill && !objectSelection.canStroke);
  appearance.querySelector("[data-object-inspector-fill]").hidden = !objectSelection.canFill;
  appearance.querySelector("[data-object-inspector-stroke]").hidden = !objectSelection.canStroke;
  const bodyField = objectInspectorEl.querySelector("[data-object-inspector-textbox]");
  bodyField.hidden = objectSelection.kind !== "textbox";
  if (objectSelection.kind === "textbox" && typeof doc.textBoxBodyProperties === "function") {
    try {
      const raw = doc.textBoxBodyProperties(objectSelection.node);
      const props = raw ? JSON.parse(raw) : null;
      if (props) {
        const insets = props.insets ?? {};
        for (const side of ["left", "top", "right", "bottom"]) {
          const input = bodyField.querySelector(`[data-object-textbox-inset=${side}]`);
          if (input) setValue(input, String(Math.round(Number(insets[`${side}Emu`] ?? 0) / 914400 * 100) / 100));
        }
        setValue(bodyField.querySelector("[data-object-textbox-anchor]"), props.vertical_anchor ?? "top");
        setValue(bodyField.querySelector("[data-object-textbox-h-overflow]"), props.horizontal_overflow ?? "overflow");
        setValue(bodyField.querySelector("[data-object-textbox-v-overflow]"), props.vertical_overflow ?? "overflow");
        setValue(bodyField.querySelector("[data-object-textbox-autofit]"), props.auto_fit?.mode ?? "none");
      }
    } catch {
      // A malformed/unsupported payload fails closed; authored data is not overwritten.
    }
  }
}

function toggleObjectInspector(open) {
  if (!objectInspectorEl) return;
  const show = open ?? objectInspectorEl.hidden;
  if (show) {
    // Opening always re-reads the model, even for the object the fields already
    // name: what the panel held may pre-date a drag, an undo or an engine edit.
    objectInspectorNode = null;
    reflectObjectInspector();
  } else {
    objectInspectorNode = null;
  }
  objectInspectorEl.hidden = !show;
}

/** Fail-closed precondition for every Apply button in the inspector: the numbers
 *  in the fields must belong to the object that is selected now. `reflectObject-
 *  Inspector` keeps that true on every repaint, so this only fires if some path
 *  changed the selection without one — in which case applying would write the
 *  previous object's geometry onto this one (docs/104 HF-057). It re-reads the
 *  model and refuses, rather than silently doing the wrong write or nothing. */
function objectInspectorMatchesSelection() {
  if (!objectSelection) return false;
  if (objectInspectorNode === objectSelection.node) return true;
  reflectObjectInspector();
  setStatus("Object properties now show the selected object — check the values, then apply", "error");
  return false;
}

function ensureObjectInspector() {
  if (objectInspectorEl) return objectInspectorEl;
  objectInspectorEl = document.createElement("aside");
  objectInspectorEl.className = "object-inspector side-panel";
  objectInspectorEl.hidden = true;
  objectInspectorEl.setAttribute("aria-label", "Object properties");
  objectInspectorEl.innerHTML = `
    <header class="panel-head properties-panel-head"><div class="properties-panel-heading"><span class="ms properties-panel-icon" aria-hidden="true">tune</span><span><strong class="panel-title">Object properties</strong><small data-object-inspector-kind></small></span></div><button type="button" class="panel-close" aria-label="Close object properties"><span class="ms" aria-hidden="true">close</span></button></header>
    <div class="panel-body properties-panel-body"><p class="properties-panel-intro">Exact model geometry. Changes apply as one undoable resize.</p><fieldset class="dialog-group property-section"><legend>Position</legend><label class="dialog-field">Left<span class="number-control"><input data-object-prop="left" type="number" step="0.01" /><span>in</span></span></label><label class="dialog-field">Top<span class="number-control"><input data-object-prop="top" type="number" step="0.01" /><span>in</span></span></label></fieldset><fieldset class="dialog-group property-section"><legend>Size</legend><label class="dialog-field">Width<span class="number-control"><input data-object-prop="width" type="number" min="0.1" step="0.01" /><span>in</span></span></label><label class="dialog-field">Height<span class="number-control"><input data-object-prop="height" type="number" min="0.1" step="0.01" /><span>in</span></span></label><button type="button" class="dialog-button dialog-button-primary" data-object-inspector-apply>Apply geometry</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-wrap hidden><legend>Text wrapping</legend><label class="dialog-field">Wrap<select data-object-inspector-wrap-select><option value="square">Square</option><option value="tight">Tight</option><option value="through">Through</option><option value="topAndBottom">Top &amp; bottom</option><option value="behind">Behind text</option><option value="front">In front of text</option></select></label><button type="button" class="dialog-button" data-object-inspector-wrap-apply>Apply wrap</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-textbox hidden><legend>Text box body</legend><div class="property-grid-2"><label class="dialog-field">Left inset<span class="number-control"><input data-object-textbox-inset="left" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Right inset<span class="number-control"><input data-object-textbox-inset="right" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Top inset<span class="number-control"><input data-object-textbox-inset="top" type="number" min="0" step="0.01" /><span>in</span></span></label><label class="dialog-field">Bottom inset<span class="number-control"><input data-object-textbox-inset="bottom" type="number" min="0" step="0.01" /><span>in</span></span></label></div><label class="dialog-field">Vertical alignment<select data-object-textbox-anchor><option value="top">Top</option><option value="center">Center</option><option value="bottom">Bottom</option></select></label><label class="dialog-field">Horizontal overflow<select data-object-textbox-h-overflow><option value="overflow">Overflow</option><option value="clip">Clip</option></select></label><label class="dialog-field">Vertical overflow<select data-object-textbox-v-overflow><option value="overflow">Overflow</option><option value="clip">Clip</option><option value="ellipsis">Ellipsis</option></select></label><label class="dialog-field">Autofit<select data-object-textbox-autofit><option value="none">Fixed shape</option><option value="shape">Grow shape to fit</option><option value="normal">Scale text</option></select></label><button type="button" class="dialog-button" data-object-inspector-textbox-apply>Apply text box body</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-alt hidden><legend>Accessibility</legend><label class="dialog-field">Description<input data-object-inspector-alt-input type="text" maxlength="255" placeholder="Describe this object" /></label><button type="button" class="dialog-button" data-object-inspector-alt-apply>Apply description</button></fieldset><fieldset class="dialog-group property-section" data-object-inspector-appearance hidden><legend>Appearance</legend><button type="button" class="dialog-button" data-object-inspector-fill>Shape fill</button><button type="button" class="dialog-button" data-object-inspector-stroke>Shape outline</button></fieldset></div>`;
  objectInspectorEl.querySelector(".panel-close").addEventListener("click", () => toggleObjectInspector(false));
  objectInspectorEl.querySelector("[data-object-inspector-apply]").addEventListener("click", () => {
    if (!doc || !objectSelection?.canResize || !objectInspectorMatchesSelection()) return;
    const rect = doc.objectRect(objectSelection.node);
    const width = Number(objectInspectorEl.querySelector("[data-object-prop=width]").value);
    const height = Number(objectInspectorEl.querySelector("[data-object-prop=height]").value);
    const left = Number(objectInspectorEl.querySelector("[data-object-prop=left]").value);
    const top = Number(objectInspectorEl.querySelector("[data-object-prop=top]").value);
    if (rect.length < 5 || ![left, top, width, height].every(Number.isFinite) || width <= 0 || height <= 0) return;
    runEdit(() => doc.resizeObject(objectSelection.ref.root, left * TWIPS_PER_INCH * 635, top * TWIPS_PER_INCH * 635, width * TWIPS_PER_INCH * 635, height * TWIPS_PER_INCH * 635), { gate: true });
  });
  objectInspectorEl.querySelector("[data-object-inspector-wrap-apply]").addEventListener("click", () => {
    if (!doc || !objectSelection?.canWrap || !objectInspectorMatchesSelection()) return;
    const mode = objectInspectorEl.querySelector("[data-object-inspector-wrap-select]").value;
    runEdit(() => doc.setObjectWrap(objectSelection.ref.root, mode), { gate: true });
  });
  objectInspectorEl.querySelector("[data-object-inspector-textbox-apply]").addEventListener("click", () => {
    if (!doc || objectSelection?.kind !== "textbox" || typeof doc.textBoxBodyProperties !== "function" || typeof doc.setTextBoxBodyProperties !== "function") return;
    if (!objectInspectorMatchesSelection()) return;
    let props;
    try { props = JSON.parse(doc.textBoxBodyProperties(objectSelection.node)); } catch { return; }
    if (!props?.insets) return;
    for (const side of ["left", "top", "right", "bottom"]) {
      const inches = Number(objectInspectorEl.querySelector(`[data-object-textbox-inset=${side}]`).value);
      if (!Number.isFinite(inches) || inches < 0) return;
      props.insets[`${side}Emu`] = Math.round(inches * 914400);
    }
    props.vertical_anchor = objectInspectorEl.querySelector("[data-object-textbox-anchor]").value;
    props.horizontal_overflow = objectInspectorEl.querySelector("[data-object-textbox-h-overflow]").value;
    props.vertical_overflow = objectInspectorEl.querySelector("[data-object-textbox-v-overflow]").value;
    const autofitMode = objectInspectorEl.querySelector("[data-object-textbox-autofit]").value;
    // Keep the authored scale/reduction values for normal autofit. The compact
    // inspector changes the mode only; it must never erase unsupported detail.
    if (autofitMode === "normal") {
      props.auto_fit = props.auto_fit?.mode === "normal"
        ? props.auto_fit
        : { mode: "normal", font_scale: 100000, line_spacing_reduction: 0 };
    } else {
      props.auto_fit = { mode: autofitMode };
    }
    runEdit(() => doc.setTextBoxBodyProperties(objectSelection.node, JSON.stringify(props)), { gate: true });
  });
  objectInspectorEl.querySelector("[data-object-inspector-alt-apply]").addEventListener("click", () => {
    if (!doc || !objectSelection?.canAltText || !objectInspectorMatchesSelection()) return;
    const value = objectInspectorEl.querySelector("[data-object-inspector-alt-input]").value.trim();
    runEdit(() => doc.setObjectDescr(objectSelection.node, value || null), { gate: true });
  });
  objectInspectorEl.querySelector("[data-object-inspector-fill]").addEventListener("click", () => shapeFillBtn.click());
  objectInspectorEl.querySelector("[data-object-inspector-stroke]").addEventListener("click", () => shapeOutlineBtn.click());
  // Mount inside `.workarea`, not on `<body>`. As a body child it had to be
  // `position: fixed`, which made it float OVER the canvas and over the footer
  // instead of taking space in the row — so the page never shifted aside for it
  // the way it does for the outline/pages/review panels, and the status bar was
  // covered. `.workarea` is a flex row and `.side-panel` is already a flex item,
  // so joining it gives the shift and the footer clearance for free.
  (document.querySelector(".workarea") ?? document.body).appendChild(objectInspectorEl);
  return objectInspectorEl;
}

/** Shows/positions a context bar above a selected object. It describes only
 *  interactions that work in the current build; deferred actions never appear
 *  as product placeholders. */
function updateObjectContextBar() {
  ensureObjectInspector();
  if (!objectContextBarEl) {
    objectContextBarEl = document.createElement("div");
    objectContextBarEl.className = "object-context-bar";
    objectContextBarEl.hidden = true;
    document.body.appendChild(objectContextBarEl);
  }
  if (!objectSelection || objectSelection.mode !== "selected") {
    objectContextBarEl.hidden = true;
    toggleObjectInspector(false);
    return;
  }
  const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h]
  const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
  if (!page) {
    objectContextBarEl.hidden = true;
    return;
  }
  const label = OBJECT_LABELS[objectSelection.kind] ?? "Object";
  objectContextBarEl.replaceChildren();
  const strong = document.createElement("strong");
  strong.textContent = label;
  objectContextBarEl.appendChild(strong);
  if (objectSelection.canWrap) {
    // A floating object exposes a live Wrap control; move + resize are drags.
    const active = doc.objectWrap(objectSelection.ref.root);
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
    hint.textContent = objectSelection.canResize
      ? "Drag to move · handles to resize"
      : "Drag to move";
    objectContextBarEl.appendChild(hint);
  } else if (objectSelection.canResize) {
    const hint = document.createElement("small");
    hint.textContent = "Drag handles to resize";
    objectContextBarEl.appendChild(hint);
  }
  // Object-editing actions (alt text / crop / delete). Each keeps the selection
  // on pointerdown (like the wrap buttons) and opens its dialog / runs its op.
  const divider = document.createElement("span");
  divider.className = "object-bar-divider";
  const actions = document.createElement("div");
  actions.className = "object-bar-actions";
  if (objectSelection.canAltText) {
    actions.appendChild(objectBarButton("description", "Alt text", "Edit alt text", openAltTextDialog));
  }
  if (objectSelection.canResize) {
    actions.appendChild(objectBarButton("tune", "Properties", "Open object properties", () => toggleObjectInspector(true)));
  }
  if (objectSelection.kind === "shape" && (objectSelection.canFill || objectSelection.canStroke)) {
    // Word's Shape Format tab reduces to its two live controls: Shape Fill and
    // Shape Outline. The buttons are module-level so their popovers stay
    // registered across the bar's re-renders.
    if (objectSelection.canFill) actions.appendChild(shapeFillBtn);
    if (objectSelection.canStroke) actions.appendChild(shapeOutlineBtn);
    reflectShapeSwatches();
  }
  if (objectSelection.canCrop) {
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
  if (objectSelection.canDelete) {
    actions.appendChild(objectBarButton("delete", "Delete", "Delete object", deleteSelectedObject, true));
  }
  if (actions.childElementCount > 0) {
    objectContextBarEl.appendChild(divider);
    objectContextBarEl.appendChild(actions);
  }
  objectContextBarEl.hidden = false;
  positionObjectContextBar();
  // The panel is a live view of the selected object, so it is re-read here —
  // this runs on every repaint, which is what makes a drag-resize, a nudge and a
  // change of selection all show up in the fields (docs/104 HF-057).
  if (objectInspectorEl && !objectInspectorEl.hidden) reflectObjectInspector();
}

/** Puts the object action bar just above the object it acts on, and takes it off
 *  screen when that object is not on screen.
 *
 *  The bar is body-level with fixed viewport coordinates, so nothing about a
 *  scroll moves it on its own: it used to stay parked at its original position
 *  over unrelated paragraphs or the ribbon after the object had scrolled away —
 *  with a live Delete button still aimed at an object the user could no longer
 *  see (docs/104 HF-058). Separated from `updateObjectContextBar` so a scroll
 *  only re-measures; it never rebuilds the bar's contents. */
function positionObjectContextBar() {
  if (!objectContextBarEl || !doc) return;
  if (!objectSelection || objectSelection.mode !== "selected") {
    objectContextBarEl.hidden = true;
    return;
  }
  const rect = doc.objectRect(objectSelection.node); // [page, x, y, w, h] twips
  const page = rect.length >= 5 ? pages[rect[0] - 1] : null;
  if (!page) {
    objectContextBarEl.hidden = true;
    return;
  }
  const { rect: pageRect, sx, sy } = scaleOf(page);
  const objectLeft = pageRect.left + rect[1] * sx;
  const objectTop = pageRect.top + rect[2] * sy;
  const objectBottom = objectTop + rect[4] * sy;
  const view = viewportEl.getBoundingClientRect();
  if (objectBottom <= view.top || objectTop >= view.bottom) {
    objectContextBarEl.hidden = true; // the object is scrolled out of the page view
    return;
  }
  objectContextBarEl.hidden = false; // must be visible to be measured
  const height = objectContextBarEl.offsetHeight;
  const top = objectTop - height - 8;
  objectContextBarEl.style.left = `${Math.max(8, objectLeft)}px`;
  // Clamp into the scrolling page view rather than the window, so an object at
  // the top of the view does not push the bar up behind the ribbon.
  objectContextBarEl.style.top =
    `${Math.round(Math.max(view.top + 8, Math.min(top, view.bottom - height - 8)))}px`;
}

// The five viewport scroll listeners never touched the object bar, and the page
// IntersectionObserver never calls drawSelection, so scrolling left it behind.
// Window scroll and resize matter too: the whole shell can move under a bar that
// is positioned in viewport coordinates.
viewportEl.addEventListener("scroll", positionObjectContextBar, { passive: true });
window.addEventListener("scroll", positionObjectContextBar, { passive: true });
window.addEventListener("resize", positionObjectContextBar);

/** Selects an object as a unit (docs/85 §4.1). Keeps `selection` as a caret at
 *  the object's surrounding-text anchor so the two-step Escape can return to it.
 *  `anchored` records placement only; the engine-owned capability bits below
 *  are the authority for every exposed operation. */
const OBJECT_CAPABILITY_KEYS = [
  "canResize",
  "canMove",
  "canWrap",
  "canDelete",
  "canAltText",
  "canCrop",
  "canFill",
  "canStroke",
  "canEditText",
];

/** Copies the engine-declared structural capabilities before a wasm hit payload
 *  is freed. JSON object-order entries use the same camelCase field names. */
function objectCapabilities(source) {
  if (source && OBJECT_CAPABILITY_KEYS.some((key) => key in source)) {
    return Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, source[key] === true]));
  }
  // A missing or stale engine payload must never make an unsupported mutation
  // appear safe. A correctly built production bridge always supplies the bits.
  return Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, false]));
}

/** Copies the engine-owned object identity before a wasm payload is freed.
 *  `node` remains the subject compatibility alias; root owns structural and
 *  anchor commands while subject owns kind-specific properties/text. */
function objectReference(source, node) {
  const value = source?.ref ?? source;
  const subject = value?.subject || value?.node || node;
  return {
    surface: value?.surface || "body",
    root: value?.root || subject,
    subject,
    path: Array.from(value?.path ?? []),
  };
}

function selectObject(node, kind, anchor, anchored = false, descriptor = null) {
  if (anchor) selection = { anchor, focus: anchor };
  const ref = objectReference(descriptor, node);
  objectSelection = {
    node: ref.subject,
    ref,
    kind,
    mode: "selected",
    anchored,
    ...objectCapabilities(descriptor),
  };
  pendingFormat = null;
  tableSelection = null;
  drawSelection();
}

/** The object context for whatever is selected right now.
 *
 *  `objectContextAtEvent` builds the same shape from a pointer hit, which is why
 *  every object command used to need a mouse to exist. `objectSelection` already
 *  carries the identity and the engine-declared capabilities, so this is a
 *  reshape rather than a second source of truth — the capabilities are never
 *  re-derived here, or they could disagree with what the bar offers. */
function selectedObjectContext() {
  if (!objectSelection) return null;
  const { node, ref, kind, anchored, ...rest } = objectSelection;
  return {
    surface: "object",
    node,
    ref,
    kind,
    anchored,
    ...Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, rest[key] === true])),
  };
}

/** Whether the document holds any floating object at all, which is what decides
 *  if the "select next object" commands are offered rather than greyed. Asked of
 *  the engine's own paint order so the answer cannot drift from what traversal
 *  would actually land on. */
function documentHasObjects() {
  if (!doc) return false;
  try {
    const objects = JSON.parse(doc.objectOrder());
    return Array.isArray(objects) && objects.length > 0;
  } catch {
    return false;
  }
}

/** Moves the object selection `step` places through the document's objects, in
 *  the engine's own paint order, wrapping at both ends so the gesture never
 *  dead-ends. Reads the order fresh each time because an edit may have added or
 *  removed an object since the last keystroke. */
function traverseObjects(step) {
  // Deliberately NOT gated on an existing selection. It used to be, which made
  // the whole object surface mouse-only: Tab cycles objects once one is
  // selected, but nothing else selected one, so a keyboard user could not reach
  // an image or a text box at all — while the comment beside the Tab handler
  // claimed this was "the only way to reach an object without a pointer". The
  // wrap-around below already handles "no current object" by starting at either
  // end, so seeding a first selection needs no extra machinery.
  if (!doc) return;
  // Inside a group Tab walks that group's children; at the top level, the
  // top-level objects. `object_traversal.mjs` carries why the one flat list
  // that used to serve both made every shape but a group's first unreachable.
  const inside = traversalRoot(objectSelection?.ref);
  let objects;
  try {
    objects = JSON.parse(inside ? doc.objectDescendants(inside) : doc.objectOrder());
  } catch {
    return;
  }
  if (!Array.isArray(objects) || objects.length === 0) return;
  const current = objectSelection
    ? objects.findIndex((entry) => entry.node === objectSelection.node)
    : -1;
  const next = nextObjectIndex(objects.length, current, step);
  const target = objects[next];
  selectObject(target.node, target.kind, null, target.anchored, target);
  // `selectObject` repaints synchronously, so the outline it just drew is the
  // marker to reveal — a traversal that lands off-screen would otherwise look
  // like nothing happened.
  scrollOverlayIntoView(pagesEl.querySelector(".overlay .object-outline"));
  // Screen readers get no handles to look at, so the position is announced.
  setStatus(traversalAnnouncement(target.kind, next, objects.length, Boolean(inside)));
}

/** Escape from a shape inside a group selects the GROUP. `escapeClimbsToGroup`
 *  is the rule; this is the DOM half. */
function climbOutOfGroup() {
  const ref = objectSelection?.ref;
  if (!escapeClimbsToGroup(ref)) return false;
  // The parent group is a TOP-LEVEL object, so it is in the ordinary order;
  // there is no second lookup to add to the engine for this.
  let group = null;
  try {
    group = JSON.parse(doc.objectOrder()).find((entry) => entry.node === ref.root) ?? null;
  } catch {
    return false;
  }
  // The engine can no longer describe the parent — a group deleted out from
  // under the selection. Collapsing to the caret is the honest fallback: a
  // selection naming a node that is gone is worse than none.
  if (!group) return false;
  selectObject(
    group.node,
    group.kind,
    selection?.focus || null,
    group.anchored,
    group,
  );
  return true;
}

/** Selects the shape under the pointer when the click lands in a group that is
 *  already held. `clickDescendsIntoGroup` is the rule; this is the DOM half. */
function descendIntoSelectedGroup(object, page, x, y, event) {
  const held = objectSelection?.ref;
  const onHeldChild =
    !!held && held.subject !== held.root && pointInsideObject(objectSelection.node, page, x, y);
  const action = groupClickAction(held, object, onHeldChild);
  // Pressing down on the shape already held begins its DRAG. Falling through
  // to the ordinary path re-selected the group and dragged that instead, so a
  // grouped shape could be picked up and never moved (`docs/109` HF-173).
  if (action === "keep") {
    if (objectSelection.canMove) startObjectMove(event, page, objectSelection.node);
    return true;
  }
  if (action !== "descend") return false;
  const child = doc.objectDescendantAt(object.root, page.pageNumber, x, y);
  if (!child) return false;
  const node = child.node;
  const kind = child.kind;
  const anchored = child.anchored;
  const descriptor = { ...objectCapabilities(child), ...objectReference(child, node) };
  child.free?.();
  selectObject(node, kind, selection?.focus || null, anchored, descriptor);
  // The SAME pointerdown that reached the shape also begins its drag, exactly
  // as a click on a top-level object does.
  if (descriptor.canMove) startObjectMove(event, page, node);
  setStatus(`${OBJECT_LABELS[kind] ?? "Object"} inside group selected`);
  return true;
}

/** Descends a selected multi-child group to its first paint-ordered leaf. The
 *  engine returns the complete root/subject/path reference and capabilities;
 *  the host never reconstructs group structure from canvas geometry. */
function descendSelectedGroup() {
  if (!doc || objectSelection?.kind !== "group") return false;
  let descendants;
  try {
    descendants = JSON.parse(doc.objectDescendants(objectSelection.ref.root));
  } catch {
    return false;
  }
  const target = Array.isArray(descendants) ? descendants[0] : null;
  if (!target) return false;
  selectObject(target.node, target.kind, null, target.anchored, target);
  setStatus(`${OBJECT_LABELS[target.kind] ?? "Object"} inside group selected`);
  return true;
}

// ---- Header / footer editing -----------------------------------------------
// Word, Docs, OnlyOffice and LibreOffice all treat this as an editing CONTEXT
// SWITCH — double-click into the band, edit in place with the body
// de-emphasised, leave with Esc or a click in the body — never a modal dialog
// and never a document mutation in itself (docs/85 §10.2).
//
// There is no sub-document address behind it: `runningHitTest` answers with an
// ordinary NodeId + offset, and the edit ops already resolve a position in
// whichever surface owns it, so editing a header is ordinary text editing with a
// different caret position. This state exists only to drive the chrome and to
// know what Esc should do.
let runningEditBand = null; // "header" | "footer" | null
/** The 1-based page whose band is open — the other half of the editing context. */
let runningEditPage = null;
/** One viewport-driven projection update per animation frame. */
let runningContextScrollFrame = 0;

// ---- Header / footer markers -------------------------------------------------
// LibreOffice Writer raises a marker with a `+` button when the pointer is in the
// top or bottom margin, and that is the affordance adopted in docs/85 §8d. Word
// and Docs rely on a bare double-click, which is invisible to anyone who has not
// been told about it — and an ABSENT header has nothing to double-click on, so
// the capability is unreachable without knowing the command exists. This is the
// same one-surface-only reachability failure the command-surface audit kept
// finding, so it is worth the small piece of chrome.
let runningMarkerEl = null;
let runningMarkerBand = null;

/** ONE page's running-content context: its top/bottom margin bands in page-local
 *  twips, which section (1-based, of how many) the page belongs to, and whether
 *  each band is inherited from the previous section ("Same as Previous").
 *
 *  Asked of the engine PER PAGE. This used to read `pageSetup()` — the FIRST
 *  section's geometry — so on a document whose later section changes orientation
 *  or margins (the reported case: page 5 turns landscape) every page of that
 *  section drew its header boundary at the wrong height, and nothing on screen
 *  could say which section's header was open. */
function runningBandsOf(page) {
  if (!doc || !page) return null;
  let bands;
  try {
    bands = JSON.parse(doc.runningBands(page.pageNumber));
  } catch {
    return null;
  }
  if (!bands) return null;
  return {
    top: bands.headerTwips,
    bottom: bands.footerTwips,
    section: bands.section,
    sectionCount: bands.sectionCount,
    headerLinked: bands.headerLinked === true,
    footerLinked: bands.footerLinked === true,
  };
}

function hideRunningMarker() {
  if (!runningMarkerEl) return;
  runningMarkerEl.hidden = true;
  runningMarkerBand = null;
}

/** Shows the marker over `band` on `page`, creating the element on first use. */
function showRunningMarker(page, band) {
  if (runningMarkerBand === band && runningMarkerEl && !runningMarkerEl.hidden) return;
  if (!runningMarkerEl) {
    runningMarkerEl = document.createElement("button");
    runningMarkerEl.type = "button";
    runningMarkerEl.className = "running-marker";
    runningMarkerEl.addEventListener("click", (event) => {
      event.preventDefault();
      const target = runningMarkerBand;
      hideRunningMarker();
      if (target) editRunningContent(target);
    });
  }
  if (page.wrap && runningMarkerEl.parentElement !== page.wrap) {
    page.wrap.appendChild(runningMarkerEl);
  }
  runningMarkerBand = band;
  runningMarkerEl.textContent = band === "header" ? "+ Header" : "+ Footer";
  runningMarkerEl.title = `Edit the page ${band}`;
  runningMarkerEl.setAttribute("aria-label", `Edit the page ${band}`);
  // Page-local, inside the sheet: positioned against the page's own wrap (which
  // the overlays already use as their containing block) rather than against the
  // scroller, where the offsets put it above the paper entirely.
  runningMarkerEl.style.left = "10px";
  runningMarkerEl.style.top = band === "header" ? "10px" : "auto";
  runningMarkerEl.style.bottom = band === "footer" ? "10px" : "auto";
  runningMarkerEl.hidden = false;
}

/** Which margin band a page-local y falls in, or `null` for the body. The one
 *  place the band geometry is interpreted, so the marker, the double-click and
 *  the context all agree on where the header is. */
function runningBandAt(page, yTwip) {
  if (!doc || !page) return null;
  // Asked of the engine, which owns page geometry. Deriving it here from the
  // DOCUMENT's page setup was wrong the moment a document had two sections with
  // different margins: the host and the engine then disagreed about where a
  // page's header ends, and the same click meant two different things.
  return doc.bandAt(page.pageNumber, 0, yTwip) || null;
}

/** Decides whether the pointer is in a margin band and shows/hides the marker.
 *  Suppressed while already editing running content — the context is open, so
 *  the marker would only be an invitation to do what is already happening. */
function updateRunningMarker(page, event) {
  // Moving onto the marker itself must not dismiss it: the pointer is then over
  // the button rather than a page, so `pageFromEvent` finds nothing and the
  // naive read of that is "left the band". Hovering an affordance cannot be the
  // thing that destroys it.
  if (runningMarkerEl && event?.target && runningMarkerEl.contains(event.target)) return;
  if (!doc || runningEditBand || !page) {
    hideRunningMarker();
    return;
  }
  // `bandAt` alone answers this. Asking `runningBands` first — a second engine
  // call and a JSON parse, per pointer move — only proved the page had margins.
  const { y } = pointToTwip(page, event);
  const band = runningBandAt(page, y);
  if (band) showRunningMarker(page, band);
  else hideRunningMarker();
}

/** Which running-content variants are on for the page the user is on, read from
 *  the engine rather than a local flag so the labels can never drift from the
 *  document.
 *
 *  `w:titlePg` is per SECTION, so this is a question about a PAGE: on a document
 *  whose page 5 changed orientation, the first section's answer is not the answer
 *  for the section the user is reading. */
function runningVariantState(page = runningEditPageOrView()) {
  if (!doc) return { firstPage: false, evenOdd: false };
  try {
    return JSON.parse(doc.runningVariants(page?.pageNumber ?? 1));
  } catch {
    return { firstPage: false, evenOdd: false };
  }
}

/** The page every running-content command acts on: the page whose band is open
 *  while editing one, otherwise the page in view. */
function runningEditPageOrView() {
  if (runningEditPage) {
    return pages.find((page) => page.pageNumber === runningEditPage) ?? pageInView();
  }
  return pageInView();
}

/** Toggles Word's "Different First Page" / "Different Odd & Even Pages".
 *
 *  Each only says a variant APPLIES; its content is a separate header/footer
 *  reference, so turning one on for a document that has no such variant shows an
 *  empty band until one is created — which is what Word does too.
 *
 *  "Different first page" is per section, so it applies to the section owning the
 *  page the user is on; "Different odd & even pages" is document-scoped, as OOXML
 *  carries it. */
async function toggleRunningVariant(which) {
  if (!doc) return;
  if (blockMutationInViewing()) return;
  if (reviewMode === "suggesting") {
    setStatus("Page-setup changes cannot be tracked yet; switch to Editing", "error");
    return;
  }
  const page = runningEditPageOrView();
  const state = runningVariantState(page);
  const next = !state[which];
  try {
    const result =
      which === "firstPage"
        ? doc.setFirstPageVariant(next, page?.pageNumber ?? 1)
        : doc.setEvenOddVariant(next);
    await applyEditResult(result);
  } catch (err) {
    setStatus(`Could not change the page setup: ${err.message ?? err}`, "error");
    return;
  }
  setStatus(
    which === "firstPage"
      ? `Different first page ${next ? "on" : "off"}`
      : `Different odd & even pages ${next ? "on" : "off"}`,
  );
}

/** Inserts a footnote or endnote at the caret and puts the caret IN the new
 *  note's body, which is where Word and Docs both leave it.
 *
 *  The engine ops existed but nothing called them, precisely because a note body
 *  could not be typed into: inserting one would have created a note the user
 *  could not fill in. That is fixed — a note body is an ordinary block surface
 *  now — so the command can finally ship. */
async function insertNote(kind) {
  if (!doc || !selection) return;
  if (blockMutationInViewing()) return;
  if (reviewMode === "suggesting") {
    setStatus(`Inserting a ${kind} cannot be tracked yet; switch to Editing`, "error");
    return;
  }
  let result;
  try {
    result =
      kind === "footnote"
        ? doc.insertFootnote(selection.focus.node, selection.focus.offset)
        : doc.insertEndnote(selection.focus.node, selection.focus.offset);
  } catch (err) {
    setStatus(`Could not insert the ${kind}: ${err.message ?? err}`, "error");
    return;
  }
  const node = result.node;
  const offset = result.offset;
  await applyEditResult(result);
  // `applyEditResult` already left the caret on the note's first paragraph; make
  // that explicit and tell the user where they now are.
  selection = { anchor: { node, offset }, focus: { node, offset } };
  drawSelection();
  updateToolbar();
  setStatus(`${kind === "footnote" ? "Footnote" : "Endnote"} added — type the note text`);
  focusEditorSurface();
}

/** Creates an empty header or footer **for the section `page` belongs to** and
 *  enters it. Gated like any other mutation: refused read-only in Viewing, and
 *  blocked in Suggesting because adding running content has no tracked-change
 *  representation. */
async function createRunningContent(band, page = pageInView()) {
  if (!doc || !page) return;
  if (blockMutationInViewing()) return;
  if (reviewMode === "suggesting") {
    setStatus(`Adding a ${band} cannot be tracked yet; switch to Editing`, "error");
    return;
  }
  // The context is known BEFORE the edit — the user asked for this page's band —
  // and the edit needs it: applying an edit places and scrolls to its caret, and
  // a caret in running content has a placement on every page. Entering
  // afterwards meant that scroll resolved without a context and landed on the
  // last page's copy, throwing the user to the end of the document.
  const previous = { band: runningEditBand, page: runningEditPage };
  setRunningContext(band, page);
  let result;
  try {
    result = doc.createRunningContent(band, page.pageNumber);
  } catch (err) {
    if (previous.band) setRunningContext(previous.band, { pageNumber: previous.page });
    else exitRunningEdit();
    setStatus(`Could not add a ${band}: ${err.message ?? err}`, "error");
    return;
  }
  const node = result.node;
  const offset = result.offset;
  await applyEditResult(result);
  // The new body is now laid out, so its paragraph has geometry to put a caret
  // on; entering after the render is what makes the caret visible.
  enterRunningEdit(band, node, offset, page);
}

/** Finds a caret position inside a page's header or footer by walking the band
 *  inward from the page edge until the engine answers.
 *
 *  `runningHitTest` resolves only points actually inside placed running content,
 *  and the band's height depends on the document's own margins, so a fixed probe
 *  point would miss. Walking in from the edge finds the first line whatever the
 *  geometry, and stops well before the body's content area so a document with no
 *  running content answers nothing rather than capturing a body line. */
function probeRunningBand(page, band) {
  if (!doc) return null;
  const x = Math.round(page.wTwip / 2);
  // A generous fraction of the page, which covers ordinary and large margins
  // without reaching the middle of the sheet.
  const span = Math.round(page.hTwip * 0.2);
  const STEP = 40; // ~0.7mm; fine enough to land inside a single line box
  for (let d = 0; d <= span; d += STEP) {
    const y = band === "header" ? d : page.hTwip - d;
    const hit = doc.runningHitTest(page.pageNumber, x, y);
    if (!hit) continue;
    const found = { band: hit.band, node: hit.node, offset: hit.offset };
    hit.free?.();
    if (found.band === band) return found;
  }
  return null;
}

/** Enters header/footer editing at `position`, an ordinary caret position that
 *  happens to live in running content. */
function enterRunningEdit(band, node, offset, page) {
  setRunningContext(band, page);
  objectSelection = null;
  tableSelection = null;
  selection = { anchor: { node, offset }, focus: { node, offset } };
  pagesEl.dataset.runningEdit = band;
  document.body.classList.add("running-edit");
  hideRunningMarker();
  drawRunningBands(band);
  setStatus(`Editing the ${band} — press Esc to return to the document`);
  focusEditorSurface();
  drawSelection();
  updateToolbar();
}

/** Records which page's band is open, on the host AND in the engine.
 *
 *  Running content is ONE body drawn on every page, so a caret in it has a
 *  placement per page and "where is this caret" is unanswerable without naming
 *  one. The engine holds the context so every geometry answer uses it; keeping
 *  it only here is what put the caret on page 1's header while the user was
 *  editing page 3's and scrolled the view there. */
function setRunningContext(band, page) {
  runningEditBand = band;
  runningEditPage = page?.pageNumber ?? runningEditPage ?? 1;
  pagesEl.dataset.runningEdit = band;
  doc?.setEditContext(band, runningEditPage);
}

/** Marks the band being edited on every page: a dashed boundary at the edge of
 *  the running area plus a small label, the way Word and LibreOffice show it.
 *
 *  This replaced dimming the canvas. The header is painted INTO the page raster,
 *  so dimming the page washed out the content being edited and greyed the whole
 *  sheet — the cue has to sit beside the band, not over it. */
function drawRunningBands(band) {
  clearRunningBands();
  for (const page of materializedPages()) {
    // Per PAGE, because the band belongs to the page's own section: a document
    // that turns landscape at page 5 has different margins there, and one
    // document-level band height drew the boundary in the wrong place on every
    // page of that section.
    const bands = runningBandsOf(page);
    if (!bands) continue;
    const { sy } = scaleOf(page);
    const el = document.createElement("div");
    el.className = `running-band${band === "footer" ? " is-footer" : ""}`;
    if (band === "header") {
      el.style.top = "0px";
      el.style.height = `${Math.round(bands.top * sy)}px`;
    } else {
      el.style.bottom = "0px";
      el.style.height = `${Math.round(bands.bottom * sy)}px`;
    }
    const label = document.createElement("span");
    label.className = "running-band-label";
    label.textContent = runningBandLabel(band, bands);
    el.appendChild(label);
    page.wrap.appendChild(el);
  }
}

/** The band's label, which in Word NAMES THE SECTION once a document has more
 *  than one ("Header -Section 2-") and says when the band is inherited ("Same as
 *  Previous").
 *
 *  Without it two pages showing different headers looked identical, so a user
 *  editing one section's header and seeing another section's pages not change had
 *  nothing on screen to explain why — the reported confusion. */
function runningBandLabel(band, bands) {
  const name = band === "header" ? "Header" : "Footer";
  if (!bands || (bands.sectionCount ?? 1) <= 1) return name;
  const linked = band === "header" ? bands.headerLinked : bands.footerLinked;
  return `${name} -Section ${bands.section}-${linked ? " · Same as Previous" : ""}`;
}

function clearRunningBands() {
  for (const el of pagesEl.querySelectorAll(".running-band")) el.remove();
}

/** Leaves header/footer editing. The caret stays where it is rather than
 *  jumping: Word and Docs both leave the insertion point alone and simply end
 *  the context, and a caret that teleports on Esc loses the user's place. */
function exitRunningEdit() {
  if (!runningEditBand) return false;
  runningEditBand = null;
  runningEditPage = null;
  doc?.setEditContext(undefined, undefined);
  delete pagesEl.dataset.runningEdit;
  document.body.classList.remove("running-edit");
  clearRunningBands();
  setStatus("");
  drawSelection();
  updateToolbar();
  return true;
}

/** The page the user is looking at: the one covering the middle of the viewport,
 *  or the nearest page when the midpoint is in a sheet gap. Entering a header
 *  from the ribbon or the palette must act on THAT page — `pages[0]` sent the
 *  caret to page one and scrolled the view off whatever the user was working on. */
function pageInView() {
  if (!pages.length) return null;
  const viewport = document.getElementById("viewport");
  if (!viewport) return pages[0];
  const middle = viewport.getBoundingClientRect().top + viewport.clientHeight / 2;
  let nearest = pages[0];
  let nearestDistance = Number.POSITIVE_INFINITY;
  for (const page of materializedPages()) {
    const rect = page.wrap.getBoundingClientRect();
    if (rect.top <= middle && rect.bottom >= middle) return page;
    const distance = rect.bottom < middle ? middle - rect.bottom : rect.top - middle;
    if (distance < nearestDistance) {
      nearest = page;
      nearestDistance = distance;
    }
  }
  return nearest;
}

/** Re-project a repeated running-content caret onto the page now in view.
 *
 * The selection's model node/offset and owning story do not change. Only the
 * page copy used for geometry changes, through the engine's edit-context API.
 * Throttling to one animation frame keeps wheel/touch scrolling bounded and
 * prevents overlay churn for every raw scroll event. */
function syncRunningContextToViewport() {
  runningContextScrollFrame = 0;
  if (!runningEditBand || !selection) return;
  const visiblePage = pageInView();
  if (!visiblePage || visiblePage.pageNumber === runningEditPage) return;
  // Different-first/even/odd sections can put a different running story on the
  // visible page. Ask the engine whether the current model focus is actually
  // placed there before changing projection; scrolling must never retarget the
  // selection into an unrelated header/footer body.
  let projected = [];
  try {
    doc.setEditContext(runningEditBand, visiblePage.pageNumber);
    projected = doc.caretRect(selection.focus.node, selection.focus.offset);
  } catch {
    return;
  } finally {
    doc.setEditContext(runningEditBand, runningEditPage);
  }
  if (projected.length < 5 || projected[0] !== visiblePage.pageNumber) return;
  breakTypingSession();
  setRunningContext(runningEditBand, visiblePage);
  drawSelection();
}

viewportEl.addEventListener(
  "scroll",
  () => {
    if (!runningEditBand || runningContextScrollFrame) return;
    runningContextScrollFrame = requestAnimationFrame(syncRunningContextToViewport);
  },
  { passive: true },
);

/** Puts the caret in a page's header or footer, entering the context. Used by
 *  the double-click in the band and by the Insert menu / palette commands —
 *  a double-click alone is invisible to anyone who has not been told about it,
 *  and an ABSENT header has nothing to double-click on (docs/85 §8d). */
function editRunningContent(band, page = pageInView()) {
  if (!doc || !page) return;
  const hit = probeRunningBand(page, band);
  if (hit) {
    enterRunningEdit(hit.band, hit.node, hit.offset, page);
    return;
  }
  // Geometry can miss an existing story when its content is outside the
  // narrow probe band. Resolve the model's existing body before considering
  // creation; creating here would relink the section and orphan the real
  // header/footer. Asked about THIS page, so the body is the one this page
  // shows — resolving it document-wide sent the edit into the first section's
  // header while the user was looking at a later, differently-oriented one.
  const existing = doc.runningContentCaret?.(band, page.pageNumber);
  if (existing) {
    const position = { node: existing.node, offset: existing.offset };
    existing.free?.();
    enterRunningEdit(band, position.node, position.offset, page);
    return;
  }
  // Nothing placed in that band: make one. Word and Docs both create the
  // header the moment you ask to edit a document that has none, rather than
  // refusing — the ask IS the intent. One undoable action creates the body and
  // links the section to it, and the caret it returns is the new body's first
  // (empty) paragraph.
  void createRunningContent(band, page);
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
  if (!objectSelection?.canWrap) return;
  runEdit(() => doc.setObjectWrap(objectSelection.ref.root, mode), { gate: true });
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
  if (!objectSelection || objectSelection.mode !== "selected" || !objectSelection.canDelete) return;
  const root = objectSelection.ref.root;
  runEdit(
    () => {
      const res = doc.deleteObject(root);
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
/** The object a currently-open object dialog is editing. The dialog stays bound
 *  to it even though `objectSelection` keeps moving behind the modal. */
let objectDialogNode = null;

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

const altTextModal = altTextDialog
  ? registerModal(altTextDialog, {
      initialFocus: () => altTextInput,
      fallbackFocus: () => pagesEl,
      onOpen: () => altTextInput.select(),
      onClose: () => {
        objectDialogNode = null;
      },
    })
  : null;

function openAltTextDialog() {
  if (!doc || !objectSelection?.canAltText || !altTextModal) return;
  objectDialogNode = objectSelection.node;
  // Prefill the current alt text so the user refines it rather than blind-
  // overwriting (Word/Docs both show the existing description).
  altTextInput.value = doc.objectDescr(objectSelection.node) ?? "";
  setAltTextNote("", false);
  altTextModal.open();
}

function closeAltTextDialog() {
  altTextModal?.close();
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
    closeAltTextDialog();
    setStatus(text ? "Alt text updated" : "Alt text removed");
  });
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
}

// Threshold (twips) beyond which a float body drag counts as a move, not a click.
const MOVE_THRESHOLD_TWIP = 40;

/** Begins a floating-object move drag (docs/85 §5.3): previews an outline that
 *  follows the pointer; the model is untouched until release. */
function startObjectMove(event, page, node) {
  if (!doc || !objectSelection?.canMove || objectSelection.node !== node) return;
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
    root: objectSelection.ref.root,
    // Set only for a shape INSIDE a group, which moves within its group
    // instead of moving the group. `commitObjectMove` branches on it.
    child: insideGroupSelection() ? node : null,
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
    runEdit(() => commitObjectMove(drag), { gate: true });
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
  if (!doc || !objectSelection?.canMove) return;
  const subject = objectSelection.node;
  const root = objectSelection.ref.root;
  const rect = doc.objectRect(subject); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
  const step = large ? NUDGE_TWIP_LARGE : NUDGE_TWIP;
  if (insideGroupSelection()) {
    runEdit(
      () => doc.moveGroupChildBy(subject, dx * step * EMU_PER_TWIP, dy * step * EMU_PER_TWIP),
      { gate: true },
    );
    return;
  }
  const nx = Math.max(0, rect[1] + dx * step);
  const ny = Math.max(0, rect[2] + dy * step);
  runEdit(() => doc.setObjectAnchorPosition(root, nx * EMU_PER_TWIP, ny * EMU_PER_TWIP), {
    gate: true,
  });
}

/** Whether the selection is a shape INSIDE a group, which moves within the
 *  group rather than moving the group. */
function insideGroupSelection() {
  return traversalRoot(objectSelection?.ref) !== null;
}

/** Commits a finished drag.
 *
 *  A shape inside a group moves by a DELTA in its own parent's space;
 *  everything else sets an absolute page position. Using the absolute call for
 *  both is what made dragging a grouped shape move the whole group
 *  (`docs/109` HF-173) — the object being dragged was the child and the object
 *  being moved was its root. */
function commitObjectMove(drag) {
  if (drag.child) {
    return doc.moveGroupChildBy(
      drag.child,
      (drag.lastX - drag.startX) * EMU_PER_TWIP,
      (drag.lastY - drag.startY) * EMU_PER_TWIP,
    );
  }
  return doc.setObjectAnchorPosition(
    drag.root,
    drag.lastX * EMU_PER_TWIP,
    drag.lastY * EMU_PER_TWIP,
  );
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
function enterObjectEditMode(at = null) {
  if (!objectSelection) return;
  if (objectSelection.kind === "group" && descendSelectedGroup()) return;
  if (!objectSelection.canEditText) {
    const label = OBJECT_LABELS[objectSelection.kind] ?? "Object";
    const hint = objectSelection.canResize ? " — drag its handles to resize" : "";
    setStatus(`${label} selected${hint}`, "", { timeout: 3000 });
    return;
  }
  objectSelection = { ...objectSelection, mode: "editing" };
  // Put the caret IN the box. Edit mode used to be a state flag and nothing
  // else: the border changed, and there was no caret and nowhere for a keystroke
  // to go, so "entering" a text box did not let you type in it. The box's
  // content is ordinary block content in the same id space, so this is an
  // ordinary caret position once the geometry can be resolved.
  // Prefer the point the user actually clicked; fall back to probing the box
  // when entry came from the keyboard (Enter on a selected box), which has no
  // point of its own.
  const caret = at ?? caretInsideObject(objectSelection.node);
  if (caret) {
    selection = { anchor: caret, focus: caret };
    setStatus("Editing the text box — press Esc to select it, Esc again to leave");
  } else {
    setStatus("This text box has no editable content yet", "error");
  }
  drawSelection();
  updateToolbar();
}

/** A caret position inside object `node`'s text, found by probing its own
 *  rectangle — the engine resolves a point to text-box content, and the object's
 *  placed rect is the one region guaranteed to be inside it. */
function caretInsideObject(node) {
  const modelCaret = doc?.textBoxCaret?.(node);
  if (modelCaret) {
    const at = { node: modelCaret.node, offset: modelCaret.offset };
    modelCaret.free?.();
    return at;
  }
  const flat = doc?.objectRect(node);
  if (!flat || flat.length < 5) return null;
  const [pageNumber, x, y, w, h] = flat;
  // Probe down the box's own left-to-middle band; the first line of content is
  // near the top, but vertical anchoring can push it down.
  for (let dy = 4; dy < h; dy += Math.max(20, Math.round(h / 12))) {
    const hit = doc.textBoxHitTest(pageNumber, Math.round(x + Math.min(w / 2, 200)), Math.round(y + dy));
    if (!hit) continue;
    const at = { node: hit.node, offset: hit.offset };
    hit.free?.();
    return at;
  }
  return null;
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
  // Keep the editable proxy on the caret so an IME candidate window and the iOS
  // autocorrect bar anchor where the user is actually typing (docs/105 UX-001).
  positionEditorTextInput(focus);
  const collapsed = anchor.node === focus.node && anchor.offset === focus.offset;
  if (!collapsed) {
    const rects = doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
    if (rects.length >= 5) {
      for (let i = 0; i < rects.length; i += 5) place(rects.slice(i, i + 5), "highlight");
      return;
    }
    // No visible rects (e.g. a tiny drag within one caret slot) → fall to caret.
  }
  // The load-time insertion point is real — every Insert command acts on it —
  // but it is not yet the user's caret, and the editor surface it belongs to
  // does not have focus, so keystrokes would go nowhere. Painting a blinking
  // cursor there would be a lie; it appears the moment `#pages` takes focus.
  if (collapsed && caretIsImplicit()) return;
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
  // No sheet means the page is outside the window: nothing on screen to mark.
  // A caller that must SHOW what it places scrolls there first
  // (`scrollModelRectIntoView`), which materializes the page and repaints.
  if (!page?.overlay) return null;
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
  linkChipShownBy = null;
  activeLink = null;
  linkChip.hidden = true;
}

/** The hover router (`docs/109` HF-179), so the pointer shape says what the
 *  thing under it will do. This is only the live state it reads. */
const pointerHover = createPointerHover({
  doc: () => doc,
  pages: () => pages,
  materializedPages,
  pointToTwip,
  linkAt,
  pointInsideObject,
  objectCapabilities,
  state: () => ({
    formatPainting: !!formatPainter,
    editsBlocked: reviewMode === "viewing" || !!readOnlyReason,
    geometryBlocked: reviewMode !== "editing" || !!readOnlyReason,
    inRunningStory: !!runningEditBand,
    insideObjectNode: objectSelection?.mode === "editing" ? objectSelection.node : null,
    resizeDrag: objectResizeDrag,
    cropDrag: objectCropSession?.handleDrag ?? null,
    moveDrag: objectMoveDrag,
    tableDrag: tableResizeDrag,
    textDrag: dragging,
  }),
});

/** Shows a bounded link chip and selects the exact authored model range, making
 * the target discoverable without hijacking drag or Shift-selection behavior. */
function showLinkChipAt(page, event) {
  const link = linkAt(page, event);
  if (!link) {
    hideLinkChip();
    return false;
  }
  // Selecting the link's own text is what a click on TEXT does. A click on a
  // linked OBJECT has already selected the object, and replacing that with a
  // text range would drop the handles the same gesture just put up.
  selection = {
    anchor: { node: link.startNode, offset: link.startOffset },
    focus: { node: link.endNode, offset: link.endOffset },
  };
  drawSelection();
  return showLinkChip(link, event);
}

/** The pointer event that opened the chip, so the light-dismiss listener below
 *  does not close it on the very event that asked for it.
 *
 *  A link on TEXT is offered on pointer-UP, after the dismiss listener has run
 *  on the matching pointer-down, so it never collided. A link on an OBJECT is
 *  resolved during pointer-DOWN (selecting it is a pointer-down gesture), so the
 *  chip opened and the same event closed it — which looked exactly like the
 *  engine not reporting the link. */
let linkChipShownBy = null;

/** Renders the chip for a link already resolved, leaving the selection alone. */
function showLinkChip(link, event) {
  activeLink = link;
  linkChipShownBy = event;
  pendingFormat = null;

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

  followExternalTarget(link.url, setStatus);
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
  pointerHover.clear();
  pointerGesture = null;
  // Object selection (docs/85 §3.1) takes precedence over a text caret: a click
  // on a drawing/image/text box selects it as a unit and shows its handles.
  const { x, y } = pointToTwip(page, event);
  // An object you are INSIDE is a text surface, not a target. Selecting it as a
  // unit here meant that while editing a text box, a click in its own empty
  // space dropped you from editing back to "selected" — the box stopped being
  // something you were typing in and became something you had picked up.
  const editingHere =
    objectSelection?.mode === "editing" &&
    pointInsideObject(objectSelection.node, page, x, y);
  const object = editingHere ? null : doc.objectAt(page.pageNumber, x, y);
  // A SECOND click inside a group selects the shape under the pointer, the way
  // Word and PowerPoint do: the first click picks the group up as a unit, the
  // next one reaches into it.
  //
  // Without this a group was a wall. `objectAt` deliberately answers with the
  // group root — correct for the first click — and nothing ever asked a
  // different question, so clicking a shape inside a group selected the group,
  // every time, however many times you clicked. The only way in was Tab (and
  // only since #574) or a double-click, which skips selection entirely and
  // drops straight into typing. On the owner's Medical form, whose drawings are
  // one group of four, that is the whole of "they're all grouped and I can't
  // edit them".
  const descended = object ? descendIntoSelectedGroup(object, page, x, y, event) : null;
  if (descended) {
    object.free?.();
    // On the owner's Medical form the link is on the CHILD — four of them, none
    // on the group — so without this the document that motivated the feature is
    // the one where it never appears.
    const childLink = linkAt(page, event);
    if (childLink) showLinkChip(childLink, event);
    event.preventDefault();
    return;
  }
  if (object) {
    const node = object.node;
    const kind = object.kind;
    const anchored = object.anchored;
    const descriptor = {
      ...objectCapabilities(object),
      ...objectReference(object, node),
    };
    object.free?.();
    // A caret at the nearest text slot is the object's surrounding-text anchor
    // (for the two-step Escape); fall back to the current caret.
    const anchor = anchorAt(page, event) || selection?.focus || null;
    selectObject(node, kind, anchor, anchored, descriptor);
    // A linked picture offers its target as linked text does (`linkAt` answers
    // for both), but AFTER selection, so the handles stay and the chip is an
    // addition. The pointer stays `move`: `pointer_cursor.mjs` `object-movable`
    // records why, and ONLYOFFICE and Word agree.
    const objectLink = linkAt(page, event);
    if (objectLink) showLinkChip(objectLink, event);
    // A floating object is movable: the same gesture that selects it can drag it
    // (a bare click commits nothing). Inline objects flow with the text.
    if (descriptor.canMove) startObjectMove(event, page, node);
    else startSelectionAutoScroll();
    event.preventDefault();
    return;
  }
  // A click that is not on an object deselects any selected object and proceeds
  // with ordinary text hit-testing. The chrome has to go with it: clearing the
  // variable alone left the handles, the context bar and `data-object-mode` on
  // screen, so a text box the user had clicked away from still looked — and to
  // anything reading the state, still was — the thing being edited.
  // ...unless the click is a caret placement INSIDE the object being edited, in
  // which case there is nothing to deselect: the object is the surface, not the
  // target. Clearing it here dropped the user out of the box on every click in
  // its own empty space.
  const wasSelected = objectSelection !== null;
  if (!editingHere) {
    objectSelection = null;
    clearObjectStatus();
    if (wasSelected) {
      updateObjectSelectionState();
      updateObjectContextBar();
    }
  }
  // Which STORY the caret is in before the click resolves. `anchorAt` may leave a
  // header, footer or text box on the way to answering — that is the deliberate
  // way out of running content — and when it does, the existing selection anchor
  // belongs to a story the new focus does not.
  const storyBefore = currentStoryKey();
  const anchor = anchorAt(page, event);
  if (!anchor) {
    updateObjectSelectionState();
    updateObjectContextBar();
    return;
  }
  // A click on a FORM CHECKBOX ticks it rather than placing a caret beside a
  // glyph. Word and ONLYOFFICE both do this (`docs/118` §2), and it is the
  // difference between the owner's Medical Incident Report Form being a form
  // and being a picture of one: eight controls, none of them tickable.
  if (toggleFormCheckboxAt(anchor.node, anchor.offset)) {
    event.preventDefault();
    return;
  }
  pendingFormat = null; // a click moves the caret → disarm typing format
  verticalGoal.clear(); // …and puts the caret in a column of its own choosing
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
    // A drag cannot construct a range across WordprocessingML stories. Retain
    // the surface that owned pointer-down so pointer-move can clip at its edge
    // instead of reusing click-away behavior and silently entering the body.
    runningBand: runningEditBand,
    objectNode: editingHere ? objectSelection.node : null,
  };
  // Shift+Click extends the current selection to the click (keeps the anchor) —
  // but only WITHIN one story. A range cannot span WordprocessingML stories, and
  // shift-clicking from an open header into the body used to build exactly that:
  // the anchor stayed in the header while the focus landed in the body, which
  // painted 422 highlight rectangles across every page of the document and then
  // made the editor swallow every keystroke, because no edit can apply to a range
  // whose ends live in different stories.
  //
  // `updateDragSelection` already clips a DRAG at the story edge; the click path
  // never got the same rule. Crossing stories with Shift is treated as a plain
  // click, which is what both Word and Docs do.
  const crossedStory = currentStoryKey() !== storyBefore;
  selection =
    event.shiftKey && selection && !crossedStory
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
  pointerHover.clear();
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
    pointerHover.schedule(page, event);
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
  const { x, y } = pointToTwip(page, event);
  // Click-away is an entry/exit gesture; crossing a boundary while the button
  // is already down is not. Keep the last valid endpoint when the pointer leaves
  // the text-box story that owned pointer-down.
  if (
    pointerGesture.objectNode &&
    !pointInsideObject(pointerGesture.objectNode, page, x, y)
  ) {
    return;
  }
  // `anchorAt` deliberately exits running-content editing for an ordinary body
  // click. Do not call that mutating path until the moving point is proven to be
  // in the same running story as pointer-down. Holding the last valid endpoint
  // clips the range at the story boundary and keeps the model selection valid.
  if (pointerGesture.runningBand) {
    const hit = doc.resolveClick(page.pageNumber, x, y);
    if (!hit) return;
    const sameStory = hit.band === pointerGesture.runningBand && !!hit.node;
    hit.free?.();
    if (!sameStory) return;
  }
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

    // The same rule on the other axis. Only `dy` existed, so at any zoom where
    // the sheet is wider than the window a drag-selection simply stopped at the
    // window edge and the end of the line was unreachable by mouse.
    const x = pointerGesture.lastClientX;
    let dx = 0;
    if (x < rect.left + AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (rect.left + AUTO_SCROLL_EDGE_PX - x) / AUTO_SCROLL_EDGE_PX);
      dx = -Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    } else if (x > rect.right - AUTO_SCROLL_EDGE_PX) {
      const ratio = Math.min(1, (x - (rect.right - AUTO_SCROLL_EDGE_PX)) / AUTO_SCROLL_EDGE_PX);
      dx = Math.ceil(ratio * AUTO_SCROLL_MAX_PX);
    }

    if (dy !== 0 || dx !== 0) {
      const beforeTop = viewportEl.scrollTop;
      const beforeLeft = viewportEl.scrollLeft;
      if (dy !== 0) viewportEl.scrollTop = Math.max(0, beforeTop + dy);
      if (dx !== 0) viewportEl.scrollLeft = Math.max(0, beforeLeft + dx);
      if (viewportEl.scrollTop !== beforeTop || viewportEl.scrollLeft !== beforeLeft) {
        updateDragSelection(clientPointEvent(pointerGesture.lastClientX, pointerGesture.lastClientY));
      }
    }

    selectionAutoScrollFrame = requestAnimationFrame(tick);
  };
  selectionAutoScrollFrame = requestAnimationFrame(tick);
}

function onPointerUp(event) {
  document.body.style.cursor = ""; // the gesture no longer owns the cursor
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
  // The sheet's OWN page number, not its position among the sheets: the nth
  // sheet is the nth page only while the reader is at the top of the document,
  // so indexing by position hit-tests a scrolled-to click on the wrong page.
  const idx = pageIndexOfWrap(wrap);
  return pages[idx] ?? null;
}

function pageFromClientPoint(clientX, clientY) {
  const target = document.elementFromPoint(clientX, clientY);
  const direct = target ? pageFromEvent({ target }) : null;
  if (direct) return direct;
  if (!pages.length) return null;

  // Only the materialized pages: the page nearest a pointer is on screen by
  // construction, and walking 25,556 records per pointer event is not.
  let best = null;
  let bestDistance = Infinity;
  for (let i = pageWindow.first; i <= pageWindow.last; i++) {
    const page = pages[i];
    if (!page?.wrap) continue;
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
// Focus is the moment the load-time insertion point stops being implicit: the
// surface can now receive typing (`#pages` is tabindex="0"), so the blinking
// caret is finally honest. This covers keyboard entry (Tab) as much as clicks,
// and every `focusEditorSurface()` a command ends with — which is why an insert
// run from the ribbon leaves a visible caret after the thing it inserted.
/** Promotes the load-time insertion point to a real caret. Until the editing
 *  surface has focus the caret is implicit and deliberately unpainted — a
 *  blinking cursor you cannot type into is a lie (see `implicitCaretAt`).
 *
 *  Bound to BOTH focusable parts of the surface. It used to hang off `#pages`
 *  alone, which silently stopped firing when the editable proxy took over focus
 *  (docs/105 UX-001): the caret then stayed implicit forever and nothing was
 *  painted, which broke every test that reads `.overlay .caret`. */
function promoteImplicitCaret() {
  if (!implicitCaretAt) return;
  implicitCaretAt = null;
  if (doc) drawSelection();
}

pagesEl.addEventListener("focus", promoteImplicitCaret);
if (editorTextInputEl) editorTextInputEl.addEventListener("focus", promoteImplicitCaret);
pagesEl.addEventListener("pointermove", (e) => {
  const page = pageFromEvent(e);
  if (page && !dragging) onPointerMove(page, e);
  // The header/footer marker follows the pointer into a margin band.
  if (!dragging) updateRunningMarker(page, e);
});
pagesEl.addEventListener("pointerleave", () => hideRunningMarker());
window.addEventListener("pointermove", (e) => {
  // A gesture in flight owns the cursor wherever the pointer goes, including
  // off the sheet it started on — so the router runs here too, not only over
  // `#pages`. It short-circuits on the drag kind and asks the engine nothing.
  if (pointerHover.dragKind()) pointerHover.schedule(null, e);
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
pagesEl.addEventListener("pointerleave", () => pointerHover.clear());
pagesEl.addEventListener("dblclick", (e) => {
  const page = pageFromEvent(e);
  if (!page) return;
  // Double-click on an object enters its edit mode (container) or selects it
  // (leaf) — docs/85 §4.3; otherwise it selects the word under the caret.
  const { x, y } = pointToTwip(page, e);
  // An object you are INSIDE is a text surface, not a target — the same rule
  // pointer-down already applies. Without it, double-clicking a word inside a
  // text box you are editing re-selected the BOX and never reached word
  // selection, so the word was never selected and the next keystroke inserted
  // instead of replacing.
  const editingHere =
    objectSelection?.mode === "editing" &&
    pointInsideObject(objectSelection.node, page, x, y);
  const object = editingHere ? null : doc?.objectAt(page.pageNumber, x, y);
  if (object) {
    let node = object.node;
    let kind = object.kind;
    let anchored = object.anchored;
    let descriptor = {
      ...objectCapabilities(object),
      ...objectReference(object, node),
    };
    // Initial hit testing selects a true multi-child group as one unit. A
    // double-click is the explicit descent gesture: ask the engine which leaf
    // owns this painted point and retain its root for structural commands.
    if (kind === "group") {
      const descendant = doc.objectDescendantAt(descriptor.root, page.pageNumber, x, y);
      if (descendant) {
        node = descendant.node;
        kind = descendant.kind;
        anchored = descendant.anchored;
        descriptor = {
          ...objectCapabilities(descendant),
          ...objectReference(descendant, node),
        };
        descendant.free?.();
      }
    }
    object.free?.();
    focusEditorSurface();
    if (!objectSelection || objectSelection.node !== node) {
      selectObject(
        node,
        kind,
        anchorAt(page, e) || selection?.focus || null,
        anchored,
        descriptor,
      );
    }
    let clicked = null;
    const inBox = doc.textBoxHitTest(page.pageNumber, x, y);
    if (inBox) {
      clicked = { node: inBox.node, offset: inBox.offset };
      inBox.free?.();
    }
    enterObjectEditMode(clicked);
    e.preventDefault();
    return;
  }
  // A double-click in the header/footer band enters that context — the gesture
  // every reference editor uses (docs/85 §10.2). Decided from the BAND GEOMETRY,
  // not from whether running content happens to exist: a document with no header
  // has nothing to hit-test, so keying off a hit meant double-clicking the top
  // margin fell through to word-selection and grabbed a word out of the BODY —
  // the opposite of what the gesture asks for.
  // Entering the band is what a double-click means from OUTSIDE it. Once you are
  // already editing that band, the same gesture means what it means everywhere
  // else — select the word — and re-entering the context you are already in
  // selected nothing at all. Triple-click at the identical pixel already worked,
  // which is what showed the hit-testing was fine and only this routing was not.
  const bandAtPoint = runningBandAt(page, y);
  if (bandAtPoint && !(bandAtPoint === runningEditBand && page.pageNumber === runningEditPage)) {
    e.preventDefault();
    void editRunningContent(bandAtPoint, page);
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
  if (event === linkChipShownBy) return; // this event is what opened it
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
  // The most specific revision at the caret wins. A paragraph formatting change
  // spans its whole paragraph and lists first, so a plain `find` resolved a caret
  // inside an inline insertion to the paragraph change instead, and Accept at the
  // caret decided the wrong suggestion (docs/108). Inline revisions first, then
  // the paragraph mark, then the paragraph formatting change.
  const specificity = (item) =>
    item.kind === "paragraph_format" ? 2 : item.kind?.startsWith("paragraph_mark_") ? 1 : 0;
  const revision = (summary.revisions ?? [])
    .filter((item) => anchorInsideRange(anchor, revisionRange(item)))
    .sort((a, b) => specificity(a) - specificity(b))[0] ?? null;
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
    misspelling: spellChecker.misspellingAt(anchor),
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
    followExternalTarget(link.url, setStatus);
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
// The table tools, built from a context the caller supplies. Extracted from the
// right-click menu so the command palette can offer the SAME rows when the caret
// is in a table: every structural table command used to exist ONLY on the
// contextual Table ribbon tab and the right-click menu, so typing "insert row"
// or "merge cells" into the palette found nothing. Word reaches all of them from
// Tell Me, and Docs files them under Format ▸ Table.
function tableToolCommands(context) {
  // Structural table edits rewrite the grid, which the engine cannot represent as
  // a tracked change, so Suggesting disables them with that as the stated reason.
  const structuralEnabled = !context.suggesting;
  const structuralReason = structuralEnabled
    ? ""
    : "This structural change cannot be tracked in Suggesting mode";
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

  return [
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
  ];
}

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

  // 0 — Spelling, ABOVE the clipboard block. Word and Docs both put the
  // suggestions first because that is the reason the user right-clicked a
  // squiggled word; anything else at the top makes them read past it. The rows
  // themselves are declared in `spell_check.mjs` so the "no suggestions" case
  // cannot be dropped on the way here.
  if (context.misspelling) {
    commands.push(
      ...spellingContextCommands(
        context.misspelling,
        spellChecker.suggestions(context.misspelling),
        {
          replace: replaceMisspelling,
          ignoreOnce: (flagged) => spellChecker.ignoreOnce(flagged),
          ignoreAll: (word) => spellChecker.ignoreAll(word),
          ignoreRule: (rule) => spellChecker.ignoreRule(rule),
          addToDictionary: (word) => void addWordToDictionary(word),
        },
        reviewMode === "viewing"
          ? "Switch to Editing mode to correct the spelling"
          : "",
      ),
    );
  }

  // 1 — The universal primary actions: history, then clipboard, in the order
  // every text field on the platform offers them.
  //
  // Membership is READ from the command's own `contextMenu: true` declaration
  // rather than hand-picked here. Seven commands declared the flag and this
  // function looked at none of them, so "Paste without formatting" — a top-five
  // action in both Word and Docs — and "Select all" were declared for the
  // right-click menu and absent from it (docs/104 HF-076). A capability that is
  // reachable from one surface only is this repo's recurring defect; the cure is
  // one declaration feeding every surface instead of three hand-wired copies.
  const CONTEXT_MENU_ICONS = { "edit.cut": "cut", "edit.copy": "copy", "edit.paste": "paste" };
  commands.push(
    ...[...registry.values()]
      .filter((command) => command.contextMenu)
      .map((command) => ({
        ...command,
        icon: CONTEXT_MENU_ICONS[command.id],
        // The palette's own grouping decides the divider: "Edit" (undo/redo)
        // and "Clipboard" become two blocks, as in the platform's text menus.
        group: command.group === "Clipboard" ? "clipboard" : "history",
      })),
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
      run: () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet"), { paragraphLevel: true }),
    },
    {
      id: "paragraph.numbering",
      label: context.listKind === "numbered" ? "Remove numbering" : "Numbered list",
      group: "list",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered"), { paragraphLevel: true }),
    },
    {
      // The engine has had checklists as long as the ribbon button has, but the
      // list submenu offered only Bulleted and Numbered — a user browsing the
      // menus concluded the editor had none (docs/104 HF-076).
      id: "paragraph.list.checklist",
      label: context.listKind === "checklist" ? "Remove checklist" : "Checklist",
      group: "list",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => toggleChecklistCommand(),
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
  commands.push(...tableToolCommands(context));

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
  // Object edits are untrackable, so they are read-only in Viewing and blocked
  // (untracked) in Suggesting — the same gate `runEdit({ gate:true })` applies.
  const mutationEnabled = reviewMode === "editing";
  const mutationReason =
    // Same rule as `blockMutationInViewing`: "turn on Editing" is advice the
    // reader cannot act on when the DOCUMENT is the thing that is read-only.
    readOnlyReason ||
    (reviewMode === "viewing"
      ? "Turn on Editing to change this object"
      : "Object changes cannot be tracked in Suggesting mode");
  const commands = [];

  // Wrap text — a submenu of wrap modes, only for a floating (anchored) object,
  // exactly like the context bar. The active mode is checked on the right.
  if (context.canWrap) {
    const active = doc.objectWrap(context.ref.root);
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
  if (context.canAltText) {
    commands.push({
      id: "object.altText",
      label: "Alt text…",
      group: "arrange",
      icon: "altText",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
      run: () => openAltTextDialog(),
    });
  }

  // Shape Fill / Shape Outline — the two live controls of Word's Shape Format
  // tab, reachable from the menu as well as the bar so neither surface is the
  // only way in.
  if (context.kind === "shape" && (context.canFill || context.canStroke)) {
    const swatch = (hex) => ({
      id: `object.fill.${hex}`,
      label: hex.toUpperCase(),
      group: "swatch",
      enabled: mutationEnabled,
      disabledReason: mutationReason,
    });
    if (context.canFill) {
      commands.push({
        id: "object.fill",
        label: "Shape fill",
        group: "arrange",
        icon: "format",
        submenu: [
          {
            id: "object.fill.none",
            label: "No fill",
            group: "reset",
            enabled: mutationEnabled,
            disabledReason: mutationReason,
            run: () => applyShapeFill(null),
          },
          ...SHAPE_MENU_COLORS.map((hex) => ({
            ...swatch(hex),
            run: () => applyShapeFill(hex),
          })),
        ],
      });
    }
    if (context.canStroke) {
      commands.push({
        id: "object.outline",
        label: "Shape outline",
        group: "arrange",
        icon: "format",
        submenu: [
          {
            id: "object.outline.none",
            label: "No outline",
            group: "reset",
            enabled: mutationEnabled,
            disabledReason: mutationReason,
            run: () => applyShapeOutline({ color: null }),
          },
          ...SHAPE_MENU_COLORS.map((hex) => ({
            ...swatch(hex),
            id: `object.outline.${hex}`,
            run: () => applyShapeOutline({ color: hex }),
          })),
        ],
      });
    }
  }

  // Crop — picture-only; a text box has no source rectangle to crop.
  if (context.canCrop) {
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

  // Properties — Word's "Size and Position…", Docs' "All image options". It had
  // exactly ONE route in the whole product: a button on the floating bar, which
  // a keyboard user cannot reach because Tab is bound to object traversal while
  // an object is selected. Adding it here puts it on the right-click menu and,
  // through the palette flattening in `editorCommands`, on the palette too.
  //
  // Not gated on `mutationEnabled`: reading an object's exact geometry is useful
  // in Viewing and Suggesting, and the panel's own Apply buttons already refuse
  // the write.
  commands.push({
    id: "object.properties",
    label: "Properties…",
    group: "arrange",
    icon: "tune",
    enabled: true,
    run: () => toggleObjectInspector(true),
  });

  // Delete — the destructive action, kept in its own trailing group.
  if (context.canDelete) {
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
  }
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
    const capabilities = objectCapabilities(object);
    const ref = objectReference(object, object.node);
    const ctx = {
      surface: "object",
      node: object.node,
      ref,
      kind: object.kind,
      anchored: object.anchored,
      ...capabilities,
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
        ref: objectSelection.ref,
        kind: objectSelection.kind,
        anchored: objectSelection.anchored,
        ...Object.fromEntries(
          OBJECT_CAPABILITY_KEYS.map((key) => [key, objectSelection[key] === true]),
        ),
      };
    }
  }
  return null;
}

// ---- Menu rendering engine (root context menu + nested submenu flyouts) -----
function activeMenuLevel() {
  return menuLevels[keyboardLevelIndex] ?? null;
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

// The row renderer lives in menu_render.mjs and owns no state; these hooks are
// the only way it reaches the open level stack.
const MENU_ROW_HOOKS = {
  shortcutText: formatShortcut,
  onHover(depth, index, button, entry, hasSub) {
    const level = menuLevels[depth];
    if (!level) return;
    keyboardLevelIndex = depth;
    focusMenuIndex(level, index, false);
    if (hasSub) openSubmenu(depth, button, entry);
    else closeMenuLevelsAbove(depth);
  },
  onActivate(depth, button, entry, hasSub) {
    if (hasSub) openSubmenu(depth, button, entry, true);
    else runMenuEntry(entry);
  },
};

function renderMenuLevel(el, entries, depth) {
  renderMenuLevelRows(el, entries, depth, MENU_ROW_HOOKS);
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
  if (entries.length === 0) {
    contextMenuReturnFocus = null;
    if (context.surface === "object") {
      const label = (OBJECT_LABELS[context.kind] ?? "Object").toLowerCase();
      setStatus(`No editable properties are available for this nested ${label} yet`);
    }
    return false;
  }
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
  return true;
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
      objectContext,
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
        ref: objectSelection.ref,
        kind: objectSelection.kind,
        anchored: objectSelection.anchored,
        ...Object.fromEntries(
          OBJECT_CAPABILITY_KEYS.map((key) => [key, objectSelection[key] === true]),
        ),
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
let tabInsertCode = 0; // the type new ruler tabs get: 0 L, 1 C, 2 R, 3 decimal
const TAB_LETTER = ["L", "C", "R", "."];

/** Rebuilds the ruler scale, margin zones, and ticks for the current page/zoom. */
function buildRuler() {
  if (!doc || !pages.length || !pageBandModel) {
    ruler.hidden = true;
    return;
  }
  const g = doc.pageGeometry();
  rulerGeom = {
    width: g.widthTwip,
    marginStart: g.marginStartTwip,
    marginEnd: g.marginEndTwip,
  };
  // The first page's rendered width, from the band geometry rather than from a
  // sheet element: page 1 has no sheet at all once the reader has scrolled away
  // from it, and the ruler still has to be the width of the paper.
  const pageWidthPx = pageBandModel.widths[0];
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
    runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, tabInsertCode), { paragraphLevel: true });
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
      runToolbarEdit((a, b, c, d) => doc.removeTabStop(a, b, c, d, pos), { paragraphLevel: true });
    } else if (moved && curPos !== pos) {
      runToolbarEdit((a, b, c, d) => doc.moveTabStop(a, b, c, d, pos, curPos), { paragraphLevel: true });
    } else {
      runToolbarEdit((a, b, c, d) => doc.setTabStop(a, b, c, d, pos, (code + 1) % TAB_LETTER.length), { paragraphLevel: true });
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

// ---- Insert surface: one declaration per Insert command ---------------------
// The Insert ribbon tab, the Insert app menu, and the command palette are three
// faces of the SAME command set. Their membership used to be authored three
// times — hand-written HTML buttons, an id list in APP_MENU_SECTIONS, and
// bespoke `someBtn.disabled = …` lines in `updateToolbar` — with nothing
// reconciling them, and they drifted: Picture, Symbol, Emoji, Bookmark and Field
// shipped to the menu and the palette while the ribbon still showed only Table
// and Link, so a user looking at the Insert tab could not insert a picture at
// all. This table is the single declaration the ribbon is built from: each row
// names the command it mirrors, the button that renders it, what the command
// genuinely requires, and how the button activates it.
// It unifies enablement and activation, not membership: the buttons are still
// authored in editor.html and the menu roster is still its own id list, so a new
// command CAN still reach one surface and miss another. What closes that gap is
// the test — each row stamps its command id onto its button, and
// `insert-surface.spec.mjs` asserts the ribbon's id set equals the Insert menu's,
// so the omission that shipped Picture unreachable now fails CI instead.
//
// `buttons` is a LIST because one command can legitimately have more than one
// ribbon face: Word shows Bookmark on both Insert ▸ Links and References ▸
// Navigation, and Insert field on both Insert ▸ Text and References ▸ Fields.
// Declaring the faces here keeps enablement and activation single-sourced —
// a second `someBtn.disabled = …` line written next to the new tab is exactly
// how the Insert ribbon drifted out of sync with its menu the first time.
const INSERT_SURFACE = [
  // "doc" is Word's rule: an open document has an insertion point, so the
  // command is live the moment a document loads (see `implicitCaretAt`).
  // Viewing/Suggesting are deliberately NOT expressed here — every run path
  // already fails closed through `blockMutationInViewing` /
  // `blockUntrackedInSuggesting`, which tells the user WHY the insert did not
  // happen. A greyed button would only say "no".
  { command: "insert.table", buttons: [insertTableBtn], requires: "doc", activate: null },
  { command: "insert.image", buttons: [insertPictureBtn], requires: "doc", activate: () => insertImageFromFile() },
  // Link is the one Insert command that genuinely needs a range: it hyperlinks
  // selected text. A cross-paragraph range still enables the button and is
  // answered by `editSelectionLink`'s "Links must stay within one paragraph",
  // which is more useful than a silently dead control.
  // The gallery popover registers its own toggle on this button (like the list
  // galleries), so wiring `activate` here too would open it on mousedown and
  // immediately close it again.
  { command: "insert.shape", buttons: [insertShapeBtn], requires: "doc", activate: null },
  { command: "insert.textbox", buttons: [insertTextBoxBtn], requires: "doc", activate: () => void insertTextBoxObject() },
  { command: "insert.link", buttons: [insertLinkBtn], requires: "range", activate: () => editSelectionLink() },
  { command: "insert.bookmark", buttons: [insertBookmarkBtn, refBookmarkBtn], requires: "doc", activate: () => openBookmarkManager() },
  { command: "insert.field", buttons: [insertFieldBtn, refFieldBtn], requires: "doc", activate: () => openFieldDialog() },
  // Notes live on References only, as they do in Word. The app-menu row and the
  // palette entry are untouched, so the command keeps three surfaces.
  { command: "insert.footnote", buttons: [refFootnoteBtn], requires: "doc", activate: () => insertNote("footnote") },
  { command: "insert.endnote", buttons: [refEndnoteBtn], requires: "doc", activate: () => insertNote("endnote") },
  { command: "insert.header", buttons: [insertHeaderBtn], requires: "doc", activate: () => editRunningContent("header") },
  { command: "insert.footer", buttons: [insertFooterBtn], requires: "doc", activate: () => editRunningContent("footer") },
  { command: "insert.symbol", buttons: [insertSymbolBtn], requires: "doc", activate: () => openSymbolPicker() },
  { command: "insert.emoji", buttons: [insertEmojiBtn], requires: "doc", activate: () => openEmojiPicker() },
];

// ---- Layout and References surfaces ----------------------------------------
// The same seam as INSERT_SURFACE/REVIEW_SURFACE, for the two tabs the ribbon
// did not have (docs/105 UX-010, and the IA half of OO-001/OO-005). Each row is
// one command: the palette entry, the ribbon button, and the enablement rule all
// read from this table, so there is nowhere to write a second opinion.
//
// `requires` values:
//   "doc"      — an open document is the only precondition.
//   "caret"    — a paragraph to act on (the caret, wherever it is).
//   "object"   — a selected image, shape or text box.
//   "missing"  — the command is REAL as a user intention but has no engine
//                operation behind it. It ships permanently disabled carrying the
//                reason, because a control that silently does nothing is the one
//                thing docs/63 forbids outright, and hiding it would make the
//                gap invisible to the person deciding what to build next.
const LAYOUT_SURFACE = [
  // Page setup: one dialog, four fieldsets. Word's four buttons are four routes
  // into the same section geometry; each one opens the dialog with its own
  // fieldset focused rather than pretending to be a separate dialog.
  { command: "layout.margins", label: "Page margins", kw: "margins page setup top bottom left right gutter", buttons: () => [layoutMarginsBtn], requires: "doc", run: () => togglePageSetup(true, () => pageMarginTopInput) },
  { command: "layout.orientation", label: "Page orientation", kw: "orientation portrait landscape rotate page setup", buttons: () => [layoutOrientationBtn], requires: "doc", run: () => togglePageSetup(true, () => pageOrientationSeg.querySelector('button[aria-pressed="true"]')) },
  { command: "layout.size", label: "Page size", kw: "size paper a4 letter legal width height page setup", buttons: () => [layoutSizeBtn], requires: "doc", run: () => togglePageSetup(true, () => pageWidthInput) },
  { command: "layout.columns", label: "Text columns", kw: "columns newspaper two three spacing separator page setup", buttons: () => [layoutColumnsBtn], requires: "doc", run: () => togglePageSetup(true, () => pageColumnCount) },
  // Paragraph: the existing relative nudges, plus the two fieldsets of the
  // paragraph-properties panel that hold the absolute indent and spacing values.
  // The ribbon deliberately does NOT carry its own numeric fields: the panel
  // reflects engine state and applies through the gated edit path, and a second
  // pair of inputs would be a second answer to "what is this paragraph's indent".
  { command: "paragraph.indent.decrease", buttons: () => [layoutIndentDecBtn], requires: "caret", run: () => adjustIndentCommand(-360) },
  { command: "paragraph.indent.increase", buttons: () => [layoutIndentIncBtn], requires: "caret", run: () => adjustIndentCommand(360) },
  { command: "layout.indent", label: "Indentation…", kw: "indent left right first line hanging exact fields paragraph", buttons: () => [layoutIndentFieldsBtn], requires: "caret", run: () => toggleParagraphProperties(true, () => indentLeftInput) },
  { command: "layout.spacing", label: "Paragraph spacing…", kw: "spacing line before after leading paragraph fields", buttons: () => [layoutSpacingFieldsBtn], requires: "caret", run: () => toggleParagraphProperties(true, () => paraLineSpacing) },
  // Arrange: the object inspector's own wrap and geometry sections, reached from
  // a durable affordance instead of only the floating bar that appears on hover.
  // The two running-content variants. No `label`/`kw`: `editorCommands` already
  // declares both rows (their labels read as switches — "Different first page:
  // on"), and a second declaration here would put two rows for one command in
  // the palette. What they needed was a RIBBON FACE: they were reachable from
  // the Insert menu and the palette only, so a chrome with no menu bar left them
  // palette-only. `pressed` is what makes the button say which way the switch is
  // set, the way the Review toggles do.
  { command: "layout.firstPageVariant", buttons: () => [insertFirstPageVariantBtn], requires: "doc", pressed: () => runningVariantState().firstPage, run: () => toggleRunningVariant("firstPage") },
  { command: "layout.evenOddVariant", buttons: () => [insertEvenOddVariantBtn], requires: "doc", pressed: () => runningVariantState().evenOdd, run: () => toggleRunningVariant("evenOdd") },
  { command: "layout.arrange.wrap", label: "Wrap text around object", kw: "wrap text square tight through behind front object image shape arrange", buttons: () => [layoutWrapBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-inspector-wrap-select]") },
  { command: "layout.arrange.position", label: "Object position and size", kw: "position size move object image shape arrange exact geometry", buttons: () => [layoutPositionBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-prop=left]") },
  {
    command: "layout.arrange.bringForward",
    label: "Bring object forward",
    kw: "bring forward z order layer front back send backward arrange",
    buttons: () => [layoutBringForwardBtn],
    requires: "missing",
    // `objectOrder()` READS paint order; nothing writes it, and there is no
    // z-order op in the wasm facade. Adding one is engine work, not UI work.
    reason: "Bring forward needs a z-order operation the engine does not expose yet",
  },
];

const REFERENCE_SURFACE = [
  {
    command: "reference.tableOfContents",
    label: "Table of contents",
    kw: "table of contents toc outline headings index navigation",
    buttons: () => [refTocBtn],
    requires: "missing",
    reason: "A table of contents needs field evaluation and update, which the engine does not expose yet",
  },
  {
    command: "reference.crossReference",
    label: "Cross-reference",
    kw: "cross reference ref heading bookmark figure numbered item",
    buttons: () => [refCrossRefBtn],
    requires: "missing",
    reason: "A cross-reference needs the REF field engine, which does not exist yet",
  },
  {
    command: "reference.updateFields",
    label: "Update fields",
    kw: "update fields refresh recalculate page number date time",
    buttons: () => [refUpdateFieldsBtn],
    requires: "missing",
    // PAGE/NUMPAGES already recompute at pagination; the cached kinds
    // (date/time/filename/author) would need a re-evaluation pass, and
    // `insertField` is the only field op the facade exposes.
    reason: "Updating cached fields needs a field-evaluation pass the engine does not expose yet; page numbers already recompute at pagination",
  },
];

/** Whether a Layout/References row's precondition is met right now. */
function ribbonSurfaceEnabled(entry) {
  if (entry.requires === "missing") return false;
  if (!doc) return false;
  if (entry.requires === "object") return !!(objectSelection && objectSelection.mode === "selected");
  if (entry.requires === "caret") return !!selection;
  return true;
}

/** Why a Layout/References row is unavailable, for the button title and the
 *  palette's `disabledReason`. */
function ribbonSurfaceReason(entry) {
  if (entry.requires === "missing") return entry.reason;
  if (!doc) return "Open a document first";
  if (entry.requires === "object") return "Select an image, shape or text box first";
  if (entry.requires === "caret") return "Place the caret in a paragraph";
  return "";
}

/** Opens the object inspector and focuses the control the caller asked for, so
 *  Layout ▸ Wrap text lands on the wrap select rather than on the panel's first
 *  field. A no-op without a selected object; the button is disabled then. */
function openObjectInspectorAt(selector) {
  if (!objectSelection || objectSelection.mode !== "selected") return;
  // The panel is created lazily by the object context bar. Reaching it from the
  // ribbon must not depend on that bar having been drawn first.
  ensureObjectInspector();
  toggleObjectInspector(true);
  queueMicrotask(() => objectInspectorEl?.querySelector(selector)?.focus({ preventScroll: true }));
}

/** The one enablement rule for an Insert command, shared by its ribbon button,
 *  its app-menu row, and its palette entry. `context.hasRange` lets a surface
 *  that already computed the selection state (the context menu) pass it in. */
// ---- Review surface: one declaration per Review command ---------------------
// Word's Review tab — Tracking, Changes, Comments. Every command below already
// existed and was reachable ONLY from the command palette, or from buttons
// living inside the review sidebar, which exist only while that sidebar is open.
// So a user with a document full of tracked changes had no durable affordance
// for accepting one. Each row names the command its button runs and whether the
// button is a toggle, and `review-surface.spec.mjs` asserts the ribbon's id set
// equals the Review menu's, so a command cannot reach one surface and miss the
// other. Enablement is deliberately just "a document is open": the commands
// themselves report why nothing happened (no change at the cursor, none left to
// accept), which is more use than a button that is silently dead.
const REVIEW_SURFACE = [
  { command: "review.mode.suggesting", button: () => reviewTrackBtn, run: () => setReviewMode(reviewMode === "suggesting" ? "editing" : "suggesting"), pressed: () => reviewMode === "suggesting" },
  { command: "view.showChanges", button: () => reviewShowChangesBtn, run: () => toggleShowChanges(), pressed: () => showingChanges },
  { command: "review.previous", button: () => reviewPrevBtn, run: () => navigateReview(-1) },
  { command: "review.next", button: () => reviewNextBtn, run: () => navigateReview(1) },
  { command: "review.acceptNext", button: () => reviewAcceptBtn, run: () => decideReviewAndAdvance(true) },
  { command: "review.rejectNext", button: () => reviewRejectBtn, run: () => decideReviewAndAdvance(false) },
  { command: "review.acceptAll", button: () => reviewAcceptAllBtn, run: () => void decideAllReviewChanges(true) },
  { command: "review.rejectAll", button: () => reviewRejectAllBtn, run: () => void decideAllReviewChanges(false) },
  { command: "review.comment", button: () => reviewCommentBtn, requires: "range", run: () => openReviewComposer() },
  { command: "review.toggle", button: () => reviewPanelBtn, run: () => toggleReview(), pressed: () => !reviewSidebar.hidden },
  // Proofing. Both switches were the whole content of a `Tools` menu, which is
  // one top-level name for two toggles — and one of the two names that scrolled
  // off the end of the menu bar (`109` HF-097). Word's Review tab opens with a
  // Proofing group, so that is where they go now that the ribbon is the only
  // navigation axis in this chrome. `requires: "always"`: both are `noDoc`
  // commands — they are preferences, and switching one with no document open is
  // meaningful and already supported.
  { command: "tools.spellCheck", button: () => reviewSpellCheckBtn, requires: "always", pressed: () => settings.spellCheck !== false, run: () => setSpellCheckEnabled(settings.spellCheck === false) },
  { command: "tools.grammarCheck", button: () => reviewGrammarCheckBtn, requires: "always", pressed: () => settings.grammarCheck !== false, run: () => setGrammarCheckEnabled(settings.grammarCheck === false) },
  { command: "tools.smartQuotes", button: () => reviewSmartQuotesBtn, requires: "always", pressed: () => smartQuotesEnabled, run: () => setSmartQuotes(!smartQuotesEnabled) },
];

function insertCommandEnabled(commandId, context = {}) {
  const entry = INSERT_SURFACE.find((candidate) => candidate.command === commandId);
  if (!entry || !doc) return false;
  if (entry.requires === "range") return !!(context.hasRange ?? hasRange());
  return true;
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

/** Scroll to a rectangle the ENGINE reported — `[page, x, y, w, h]` in twips —
 *  rather than to a DOM node that may not exist.
 *
 *  A find match 20,000 pages away, a comment anchor, a caret after a jump: none
 *  has an overlay element until its page is materialized, and its page is not
 *  materialized until something scrolls there. Model geometry plus the band's
 *  arithmetic answers "where is that, in scroll coordinates" without either.
 *  `block` matches `scrollOverlayIntoView`. Returns whether it scrolled. */
/** Breathing room between a revealed caret and the edge it was revealed past.
 *  Flush looks right for one frame; the next repaint — and arrowing through a
 *  paginated document repaints constantly — puts it back outside, which is the
 *  "3 px above" `viewer-scroll-ceiling` reported. Word and Docs keep a line's
 *  worth. */
const SCROLL_INTO_VIEW_MARGIN = 8;

function scrollModelRectIntoView(flat, block = "nearest") {
  if (!pageBandModel || !flat || flat.length < 5) return false;
  const [pageNumber, , y, , h] = flat;
  const page = pages[pageNumber - 1];
  if (!page) return false;
  const { sy } = scaleOf(page);
  const top = pageBandModel.tops[pageNumber - 1] + y * sy;
  const bottom = top + Math.max(1, h * sy);
  const viewportHeight = viewportEl.clientHeight;
  const { docY } = scrollToDoc(pageBandModel, viewportHeight, viewportEl.scrollTop - bandTopInScroller);
  let wanted = docY;
  // The same margin the overlay path keeps, in document space.
  const margin = SCROLL_INTO_VIEW_MARGIN;
  if (block === "center") wanted = top + (bottom - top) / 2 - viewportHeight / 2;
  else if (top < docY) wanted = top - margin;
  else if (bottom > docY + viewportHeight) wanted = bottom - viewportHeight + margin;
  else return false;
  const target = bandTopInScroller + docToScroll(pageBandModel, viewportHeight, Math.max(0, wanted));
  const max = Math.max(0, viewportEl.scrollHeight - viewportEl.clientHeight);
  viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
  // Synchronously, not on the scroll event: the caller is about to look for the
  // marker it just asked to be shown, and an event a frame later would hand it
  // an empty page.
  updatePageWindow();
  paintPagesInView();
  return true;
}

/** The engine rectangle the caret or selection occupies, or null. */
function selectionModelRect() {
  if (!doc || !selection) return null;
  const { anchor, focus } = selection;
  if (anchor.node !== focus.node || anchor.offset !== focus.offset) {
    const rects = doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
    if (rects.length >= 5) return rects.slice(0, 5);
  }
  const caret = doc.caretRect(focus.node, focus.offset);
  return caret.length >= 5 ? caret.slice(0, 5) : null;
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
  // A pixel of scroll is not a pixel of content once the document is
  // compressed onto a bounded scroll range: it is `scale` of them (see
  // `page_scroll.mjs`). A delta measured on screen therefore has to be divided
  // by that before it becomes a scroll position, or every "scroll this into
  // view" overshoots by the compression factor — which, above 2, oscillates
  // instead of converging.
  const perScrollPx = pageBandModel?.scale > 1 ? pageBandModel.scale : 1;
  let delta = 0;
  if (block === "center") {
    delta = markerRect.top + markerRect.height / 2 - (viewportRect.top + viewportRect.height / 2);
  } else if (markerRect.top < viewportRect.top) {
    delta = markerRect.top - viewportRect.top - SCROLL_INTO_VIEW_MARGIN;
  } else if (markerRect.bottom > viewportRect.bottom) {
    delta = markerRect.bottom - viewportRect.bottom + SCROLL_INTO_VIEW_MARGIN;
  } else {
    return;
  }
  const target = current + delta / perScrollPx;
  viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
}

/** Scroll the caret in the editor viewport. Navigation callers can request a
 * centered target so headings/anchors retain useful reading room. */
function scrollCaretIntoView(block = "nearest") {
  const marker = pagesEl.querySelector(".overlay .caret");
  // Two cases take the model path. No marker: the caret's page has no sheet,
  // which is where a 25,556-page document spends most of its time. And a
  // COMPRESSED band: a target computed from a screen delta is only as exact as
  // the compression factor it is divided by, and near the ends of the scroll
  // range it is not exact at all — measured 57 px short at the bottom of a
  // 3,300-page document, which is a caret just off screen after an arrow key.
  // Document space has no such error: `docToScroll` is the exact inverse of
  // the mapping that placed the page.
  if (!marker || pageBandModel?.scale > 1) {
    if (scrollModelRectIntoView(selectionModelRect(), block)) paintOverlayLayer();
    return;
  }
  scrollOverlayIntoView(marker, block);
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
  if (!marker || pageBandModel?.scale > 1) {
    if (scrollModelRectIntoView(selectionModelRect(), "nearest")) paintOverlayLayer();
    return;
  }
  scrollOverlayIntoView(marker, "nearest");
}

/** Find selects a real range, so paintSelection deliberately emits highlights
 * and no caret. Scroll its first rectangle into view; querying only `.caret`
 * made Previous/Next update the selection on an off-screen page without moving
 * the canvas. */
function scrollFindMatchIntoView() {
  const marker = pagesEl.querySelector(".overlay .highlight");
  // A match found on a page the reader is nowhere near has no highlight to
  // scroll to until its page exists. This is the guard `find-far-page` covers:
  // Find used to update the selection on an off-screen page and leave the
  // canvas exactly where it was.
  if (!marker || pageBandModel?.scale > 1) {
    if (scrollModelRectIntoView(selectionModelRect(), "center")) paintOverlayLayer();
    return;
  }
  scrollOverlayIntoView(marker, "center");
}

// ---- Unsaved-work tracking ---------------------------------------------------
//
// Engine-authoritative and fail-dirty. Two independent axes make a document
// unsaved, and BOTH must be tracked or the guard is silently wrong for one of
// them:
//
//   * the MODEL — every applied edit carries the engine's own monotonic
//     revision, so the question "has this changed since it was last written
//     out?" is answered by the engine rather than inferred by the UI;
//   * the NAME — renaming changes what Save produces without touching the model
//     at all, so no revision can ever observe it.
//
// A watermark, not a boolean: a boolean cannot be cleared correctly by a save
// that lands between two edits. And when the revision cannot be read for any
// reason, `revisionUnreadable` latches and the document reports dirty — a
// needless warning costs one click, the opposite mistake costs the document.
//
// Undo back to the original content still reports dirty, because revisions are
// monotonic. That is the intended direction of the error.
let savedRevision = 0;
let currentRevision = 0;
let savedName = "";
let revisionUnreadable = false;
// A third axis, and the only one that is not about editing: a document restored
// from a crash draft has never been written out ANYWHERE. Its revision starts
// at the open-time baseline and its name matches, so both axes above would call
// it clean — which would re-arm, one level up, exactly the data loss the draft
// exists to prevent. Cleared by the same two places that clear the others.
let restoredFromDraft = false;

/** The engine's post-edit revision, or null if it cannot be read. Never throws:
 *  a wrapper that has already been freed, or a build whose `EditResult` predates
 *  the getter, must degrade to "unknown" (and therefore dirty) rather than take
 *  down the edit that just succeeded. */
function readRevision(res) {
  try {
    const revision = res.revision;
    return Number.isFinite(revision) ? revision : null;
  } catch {
    return null;
  }
}

/** Everything true of EVERY landed edit, whichever path applied it.
 *
 *  Four apply paths grew independently and diverged: `applyEditResult` cleared
 *  the find cache but never marked the document edited, `runToolbarEdit` marked
 *  it but never cleared the cache, `applyBookmarkEdit` did both, and
 *  `runNodeEdit` — the path every table and list structural edit takes — did
 *  neither. Shading a cell or converting a list left the header reading
 *  "Opened" and left Find matching against pre-edit text.
 *
 *  A rule that enumerates its subjects is one omission from being wrong, and the
 *  omission is the write path somebody adds last. What is true of every edit
 *  belongs here and nowhere else; what genuinely differs between the paths
 *  (caret movement, stats, sync vs async repaint) stays with them.
 *
 *  Call with the revision read BEFORE `res.free()`. */
function noteDocumentEdited(revision) {
  clearFindParagraphCache();
  if (revision === null || revision === undefined) revisionUnreadable = true;
  else currentRevision = revision;
  setDocumentState("edited");
  // Autosave's only contact with the edit path, and it must stay O(1) in
  // document size (docs/107 §4): this stores an integer and re-arms a timer.
  // The snapshot itself is taken from that timer, never from a keystroke.
  noteDraftDirty();
  // Spelling's only contact with it, under the same constraint and for the
  // same reason: a keystroke must never touch the dictionary (docs/114 §3).
  spellChecker.noteEdited();
}

/** Re-baseline onto a freshly opened document: nothing is unsaved yet. */
function resetDirtyTracking(name) {
  savedRevision = 0;
  currentRevision = 0;
  savedName = name;
  revisionUnreadable = false;
  restoredFromDraft = false;
}

/** The bytes have left the editor, so the current state is now the saved one. */
function markDocumentSaved() {
  savedRevision = currentRevision;
  savedName = currentName;
  revisionUnreadable = false;
  restoredFromDraft = false;
  setDocumentState("downloaded");
  // The user has the bytes, so the draft has nothing left to protect. Keeping
  // it would mean offering back, after the next crash, work that is already on
  // disk — and the offer would look like the save had not happened.
  discardOwnDraft();
}

/** Whether closing right now would lose work. */
function documentIsDirty() {
  if (!doc) return false;
  if (revisionUnreadable || restoredFromDraft) return true;
  return currentRevision !== savedRevision || currentName !== savedName;
}

/** Apply an EditResult: place the caret, repaint only the dirty pages (or rebuild
 *  on a page-count change), redraw the caret, and keep it in view. */
/** Adopts the position an edit reports, or the nearest one that exists.
 *
 *  The engine's `EditResult` position was trusted unconditionally, and for undo,
 *  redo and accept/reject-all it can name a node that the very same operation
 *  removed. Nothing then painted a caret — `caretRect` returns an empty rect, so
 *  `place()` bailed — and every following keystroke threw inside `runEdit`, which
 *  turned it into a console warning and the generic status "That edit isn't
 *  supported for this selection yet". The document was still intact and the
 *  editor still looked alive; it simply ignored the keyboard until the user
 *  happened to click. That is the worst failure shape available to a text
 *  editor, and it is why this validates rather than trusts.
 *
 *  `caretRect` is the oracle because it is the same call the painter makes: if
 *  it cannot place the caret, neither can the user. The fallback is the body's
 *  first position, which the open path already relies on.
 *
 *  The engine should also not report a position it has just deleted; this is the
 *  host-side belt, not a reason to leave that unfixed. */
function adoptEditPosition(node, offset) {
  const at = { anchor: { node, offset }, focus: { node, offset } };
  try {
    if (doc.caretRect(node, offset).length >= 5) return at;
  } catch {
    // fall through — an unplaceable position is exactly what we are guarding
  }
  try {
    const fallback = doc.firstPosition();
    const recovered = { node: fallback.node, offset: fallback.offset };
    fallback.free?.();
    if (doc.caretRect(recovered.node, recovered.offset).length >= 5) {
      return { anchor: recovered, focus: recovered };
    }
  } catch {
    // no recoverable position: keep the reported one rather than crashing, and
    // let the caller repaint. Better a missing caret than a dead editor.
  }
  return at;
}

async function applyEditResult(res) {
  const node = res.node;
  const offset = res.offset;
  const dirty = res.dirtyPages;
  const newCount = res.pageCount;
  const revision = readRevision(res);
  res.free();
  noteDocumentEdited(revision);
  // An edit has landed, so the caret the editor now shows is the result of the
  // user's own action — never the untouched load-time seed, even if the two
  // positions coincide.
  implicitCaretAt = null;
  verticalGoal.clear(); // an edit ends a run of vertical moves (HF-164)
  selection = adoptEditPosition(node, offset);
  // A content mutation invalidates a row/column/table selection the same way it
  // invalidates an object selection. Without this the accent fill survives
  // typing, deleting, undo and arrow keys, so the editor claims a whole table is
  // selected while the real selection is a collapsed caret in one cell — Copy
  // returns "" and Backspace removes a single character.
  tableSelection = null;
  // The paste-options chip only ever means "redo the paste you just made,
  // differently" — it acts by undoing it. Once any other edit has landed, an
  // undo removes that edit instead, so the offer has expired. Retiring it here,
  // where every edit passes, is what stops a ⌘Z before the click from silently
  // deleting the sentence the user typed before the paste.
  if (pasteOptionsShowing()) hidePasteOptions();
  dropVanishedObjectSelection();
  if (newCount !== pages.length) {
    await renderAll(); // structural change (page added/removed): rebuild the list
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ stats: true, outline: true });
  scrollCaretIntoView();
  // Any edit can introduce a scalar no provisioned face covers — an emoji from
  // the picker, a paste from another app, an IME commit. Coverage used to be
  // checked only on open and for the two edits known to add symbols (checklist
  // and list markers), so everything else rendered as notdef boxes until the
  // document was reopened. `missingCoverage()` is an engine-side scan that
  // returns nothing in the overwhelmingly common case, and the fetch only
  // happens when a bucket is genuinely absent, so this stays off the hot path.
  void ensureGlyphCoverage("this edit");
}

/** Drops an object selection whose object is no longer in the document.
 *
 *  Undoing an insert (or any edit that removes the selected object) left the
 *  selection, its handles and its context bar pointing at a node the model no
 *  longer holds — chrome hovering over nothing, and a Fill button that would
 *  fail. `deleteSelectedObject` clears the selection itself; every other path
 *  that can remove an object needed this. */
function dropVanishedObjectSelection() {
  if (!doc || !objectSelection) return;
  let rect = [];
  try {
    rect = doc.objectRect(objectSelection.node);
  } catch {
    rect = [];
  }
  if (rect.length >= 5) return;
  objectSelection = null;
  objectCropSession = null;
  updateObjectSelectionState();
  updateObjectContextBar();
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
function blockUntrackedInSuggesting({ paragraphLevel = false } = {}) {
  if (reviewMode !== "suggesting") return false;
  // Paragraph formatting IS trackable now: the engine records a `w:pPrChange`
  // holding the prior while Suggesting is on (docs/108 phase 2, HF-131).
  if (paragraphLevel) return false;
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
  // "Switch to Editing" is advice the reader can act on — unless the document
  // itself cannot be edited, in which case it is a wrong instruction. One
  // policy function decides, so this and the engine-refusal path cannot drift
  // (`docs/113` §8.3).
  setStatus(mutationBlockedMessage({ editingUnavailableReason: readOnlyReason }), "error");
  focusEditorSurface();
  return true;
}

/** Runs an edit thunk and applies its result; unsupported edits are ignored.
 *  `gate: true` marks a mutation that has no tracked-revision representation
 *  (yet, or ever, per REVIEW-GAP-009's structural backlog): it is blocked
 *  outright in Suggesting mode rather than silently applying untracked.
 *
 *  Returns whether the edit actually landed, so a caller chaining several edits
 *  can stop instead of continuing against a document that never changed. */
async function runEdit(thunk, { typing = false, gate = false } = {}) {
  if (blockMutationInViewing()) return false;
  if (!typing) breakTypingSession();
  if (gate && blockUntrackedInSuggesting()) return false;
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
    //
    // A refused undo or redo is the one case the generic sentence actively
    // misdescribes: nothing about the *selection* is wrong, the history step
    // simply no longer applies to this document. `apply_group` now restores
    // the pre-edit document and pushes the entry back before returning, so the
    // honest thing to say is that nothing changed (HF-045).
    setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason }), "error");
    return false;
  }
  await applyEditResult(res);
  return true;
}

/** The column a run of vertical caret moves is aiming at (HF-164). */
const verticalGoal = createVerticalGoal();

/** Move the caret by arrow key. Shift extends (moves the focus); plain collapses. */
function navCaret(dir, extend) {
  if (!selection) return;
  if (objectCropSession) cancelCrop(); // arrow-navigating away discards the crop preview
  objectSelection = null; // moving the caret leaves any object selection
  // ...and so does it leave a row/column/table selection. These two were written
  // as one rule and only one of them was applied here, which is how arrowing out
  // of a table left the whole table still painted as selected.
  tableSelection = null;
  clearObjectStatus();
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const collapseToStart = dir === "left" || dir === "wordLeft";
  const collapseToEnd = dir === "right" || dir === "wordRight";
  const goalX = verticalGoal.columnFor(dir, selection.focus, () => caretColumn(selection.focus));
  const c =
    !extend && hasRange() && (collapseToStart || collapseToEnd)
      ? doc.selectionEdge(
          selection.anchor.node,
          selection.anchor.offset,
          selection.focus.node,
          selection.focus.offset,
          collapseToEnd,
        )
      : doc.moveCaret(selection.focus.node, selection.focus.offset, dir, goalX ?? undefined);
  const to = { node: c.node, offset: c.offset };
  c.free();
  // The engine still dead-ends going UP out of a table: it answers with the
  // position it was given and the key does nothing visible. `recoverVerticalMove`
  // takes the engine's answer whenever it really moved and only otherwise finds
  // the neighbouring line by hit-testing — and a rescued move is then put back
  // on the run's goal column, which the probe's own geometry cannot know about.
  const rescued =
    dir === "up" || dir === "down"
      ? recoverVerticalMove(dir, selection.focus, to, caretProbeIO())
      : to;
  const next = rescued === to ? to : atGoalColumn(rescued, goalX);
  verticalGoal.keep(goalX, next);
  selection = extend ? { anchor: selection.anchor, focus: next } : { anchor: next, focus: next };
  drawSelection();
  scrollCaretIntoView();
}

/** The caret's own column at `position` — page-local twips, the units the goal
 *  column is held in — or null where the position has no geometry. */
function caretColumn(position) {
  return doc.caretRect(position.node, position.offset)[1] ?? null;
}

/** `position`, moved sideways onto `goalX` on its own line. The rescue probe
 *  finds a LINE by hit-testing below/above the painted caret, and the caret's
 *  x is not the column the run aims at once a short line has clamped it. */
function atGoalColumn(position, goalX) {
  const flat = goalX === null ? [] : doc.caretRect(position.node, position.offset);
  if (flat.length < 5) return position;
  const hit = doc.hitTest(flat[0], goalX, flat[2] + Math.round(flat[4] / 2));
  if (!hit) return position;
  const at = { node: hit.node, offset: hit.offset };
  hit.free();
  return at;
}

/** The geometry and model access `probeVerticalNeighbour` needs, bound to this
 *  editor's DOM and engine. */
function caretProbeIO() {
  return {
    caretRect: () => pagesEl.querySelector(".overlay .caret")?.getBoundingClientRect() ?? null,
    resolveAt: (x, y) => {
      const page = pageFromClientPoint(x, y);
      return page ? anchorAt(page, clientPointEvent(x, y)) : null;
    },
    positionRect: (position) => {
      const flat = doc.caretRect(position.node, position.offset);
      return flat.length >= 5 ? { page: flat[0], y: flat[2] } : null;
    },
  };
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
    await runEdit(() => text == null
      ? doc.suggestDeleteRange(start.node, start.offset, end.node, end.offset, undefined, new Date().toISOString())
      : doc.suggestReplaceRange(start.node, start.offset, end.node, end.offset, text, undefined, new Date().toISOString()));
    return true;
  } catch (error) {
    setStatus(String(error?.message ?? error), "error");
    return false;
  }
}

async function runToolbarEdit(thunk, { allowInSuggesting = false, paragraphLevel = false } = {}) {
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (!allowInSuggesting && blockUntrackedInSuggesting({ paragraphLevel })) return;
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
  const revision = readRevision(res);
  res.free();
  noteDocumentEdited(revision);
  if (newCount !== pages.length) await renderAll();
  else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ outline: true });
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
let currentUnderlineStyle = "single";
let currentUnderlineColor = "";
let currentUnderlineMixed = false;

/** Reflect the effective typed underline without writing renderer fallback data
 *  back into the document. Empty color is the model's automatic/text-color state. */
function reflectUnderlineControl(style, color, mixed = false) {
  const canonical = style || "single";
  currentUnderlineStyle = canonical;
  currentUnderlineColor = color || "";
  currentUnderlineMixed = mixed;
  fmtButtons.underline.dataset.underlineStyle = canonical;
  fmtButtons.underline.style.setProperty("--underline-color", color || "currentColor");
  underlineMenuBtn.closest(".underline-split")?.classList.toggle("is-mixed", mixed);
  underlineMenuBtn.title = mixed
    ? "Underline style and color: Mixed"
    : `Underline: ${UNDERLINE_STYLE_LABELS.get(canonical) || canonical}${color ? ` · ${color.toUpperCase()}` : " · Automatic color"}`;
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

/** Every control that toggles run format `key`, on any surface. The ribbon
 *  button is the one held by id; anything else declares itself with `data-fmt`,
 *  so a new surface inherits the state sync by existing rather than by being
 *  added to a list here. Cached because `updateToolbar` runs on every repaint
 *  and the set is fixed markup. */
const formatToggleCache = new Map();
/** The compact bar, assigned once its dependencies exist (bottom of the file).
 *  Declared here because `updateToolbar` runs before that point and reflecting
 *  alignment into an uninitialised `const` would be a temporal-dead-zone throw
 *  rather than a no-op. */
let compactToolbarUi = null;
function formatToggleButtons(key) {
  let buttons = formatToggleCache.get(key);
  if (!buttons) {
    buttons = [...new Set([fmtButtons[key], ...document.querySelectorAll(`[data-fmt="${key}"]`)])]
      .filter(Boolean);
    formatToggleCache.set(key, buttons);
  }
  return buttons;
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
    const state = mixed ? "mixed" : String(!!pressed);
    // Every surface that toggles this format, not just the ribbon. The floating
    // bar sits directly over the selection — it is the one the user is looking
    // at — and reading neutral over already-bold text made clicking B remove
    // bold instead of applying it, with no pressed state announced either.
    for (const button of formatToggleButtons(key)) button.setAttribute("aria-pressed", state);
  }
  // Run controls work with a range (apply) or a caret (arm for typing), so they are
  // enabled whenever there is a selection.
  for (const el of runControls) el.disabled = !hasSel;
  for (const el of paraControls) el.disabled = !hasSel;
  // Style cards are rebuilt as the offered set changes, so they cannot live in the
  // static `paraControls` list — but they must still refuse honestly rather than being
  // clickable buttons that do nothing (§10, "never a dead control").
  styleCardsEnabled = hasSel;
  stylesTrigger.disabled = !hasSel;
  stylesTrigger.title = hasSel ? "Paragraph style" : "Place the caret in a paragraph to apply a style";

  const align = hasSel && doc ? doc.alignmentAt(selection.focus.node, selection.focus.offset) : "start";
  for (const [key, btn] of Object.entries(alignBtns)) {
    btn.setAttribute("aria-pressed", String(key === align));
  }
  // The compact bar carries ONE align control, Docs-style, so it reports the
  // caret's alignment through its icon rather than through four pressed states.
  compactToolbarUi?.reflectAlign(align);

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
  let underlineStyle = "single";
  let underlineColor = "";
  let underlineStyleMixed = false;
  let underlineColorMixed = false;
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
    underlineStyle = rs.underlineStyle || "single";
    underlineStyleMixed = rs.underlineStyleMixed;
    underlineColor = rs.underlineColor || "";
    underlineColorMixed = rs.underlineColorMixed;
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
    if (pendingFormat.underlineStyle != null) underlineStyle = pendingFormat.underlineStyle;
    if (pendingFormat.underlineColor != null) underlineColor = pendingFormat.underlineColor;
    if (pendingFormat.vertAlign != null) {
      sup = pendingFormat.vertAlign === "super";
      sub = pendingFormat.vertAlign === "sub";
    }
    sizeMixed = false;
    fontMixed = false;
    colorMixed = false;
    highlightMixed = false;
    underlineStyleMixed = false;
    underlineColorMixed = false;
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
  const underlineToggleMixed = fmtButtons.underline.getAttribute("aria-pressed") === "mixed";
  const underlineOn = fmtButtons.underline.getAttribute("aria-pressed") === "true";
  reflectUnderlineControl(
    underlineStyleMixed ? "" : (underlineOn ? underlineStyle : "none"),
    underlineColorMixed ? "" : underlineColor,
    underlineToggleMixed || underlineStyleMixed || underlineColorMixed,
  );
  superBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sup));
  subBtn.setAttribute("aria-pressed", verticalAlignMixed ? "mixed" : String(sub));

  // Reflect the current paragraph style + spacing + list kind. The style at the
  // caret is a style the document is USING, so reflecting it also keeps it offered
  // by the gallery after the caret moves on (docs/115 §5.3).
  reflectParagraphStyle(hasSel && doc ? doc.paragraphStyleAt(selection.focus.node) : "");
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

  // The Insert ribbon takes its enablement from the shared INSERT_SURFACE
  // descriptors — the same rule the Insert menu and the palette use. Nothing
  // about Insert is decided here any more; a per-button rule written at this
  // spot is exactly how the ribbon drifted out of sync with the menu.
  for (const entry of INSERT_SURFACE) {
    const enabled = insertCommandEnabled(entry.command, { hasRange: range });
    for (const button of entry.buttons) button.disabled = !enabled;
  }
  // Layout and References take their enablement from the same tables their
  // palette rows read, for the same reason: one rule, one place.
  for (const entry of [...LAYOUT_SURFACE, ...REFERENCE_SURFACE]) {
    const enabled = ribbonSurfaceEnabled(entry);
    const reason = ribbonSurfaceReason(entry);
    for (const button of entry.buttons()) {
      if (!button) continue;
      button.disabled = !enabled;
      // A disabled control has to SAY why. `title` is the only channel a
      // disabled button has (it takes no focus and fires no events), and the
      // authored title is the "missing" reason already, so only the transient
      // preconditions rewrite it.
      if (entry.requires !== "missing") {
        button.title = enabled ? (button.dataset.enabledTitle ?? button.title) : reason;
      }
      // A switch has to say which way it is set, whichever table declares it.
      if (entry.pressed) button.setAttribute("aria-pressed", String(entry.pressed()));
    }
  }
  // Review: a document is the only precondition, except commenting, which needs
  // text to attach to. The two toggles reflect engine state rather than a local
  // flag, so the ribbon always agrees with the footer's mode control.
  // `pressed` replaced three hand-written `setAttribute("aria-pressed", …)`
  // lines. Declaring it beside the command is what let the two proofing switches
  // reflect their state without a fourth and fifth copy of the same line, and
  // what stops the next toggle shipping mute.
  for (const entry of REVIEW_SURFACE) {
    const button = entry.button();
    if (!button) continue;
    if (entry.requires !== "always") button.disabled = !doc || (entry.requires === "range" && !range);
    if (entry.pressed) button.setAttribute("aria-pressed", String(entry.pressed()));
  }
  // Ribbon: undo/redo/view controls need a document; the Table tab is contextual.
  undoBtn.disabled = !doc || !doc.canUndo;
  redoBtn.disabled = !doc || !doc.canRedo;
  const undoLabel = doc?.undoLabel || "";
  const redoLabel = doc?.redoLabel || "";
  const undoName = undoLabel ? `Undo ${undoLabel}` : "Undo";
  const redoName = redoLabel ? `Redo ${redoLabel}` : "Redo";
  undoBtn.setAttribute("aria-label", undoName);
  redoBtn.setAttribute("aria-label", redoName);
  // Reassigned on every state sync, so these outlive the boot sweep and have to
  // be localized at the point of assignment (HF-025).
  undoBtn.title = localizeShortcutText(`${undoName} (⌘Z)`, EDITOR_KEYBOARD_PLATFORM);
  redoBtn.title = localizeShortcutText(`${redoName} (⌘⇧Z)`, EDITOR_KEYBOARD_PLATFORM);
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

/** Republishes the open document's styles. Paragraph properties ▸ Style gets the
 *  COMPLETE list — it plays the part of Word's Styles pane, and with the palette's
 *  `Style: <name>` rows it is what keeps every style in the stylesheet reachable now
 *  that the band offers a short list (docs/115 §5.5). */
function populateStyles() {
  const styles = doc ? doc.listStyles() : [];
  paraPanelStyle.replaceChildren();
  for (const [value, label] of [["", "Style"], ...styles.map((s) => [s, s])]) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = label;
    paraPanelStyle.appendChild(opt);
  }
  // The band's gallery offers the short list drawn from the same registry.
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
function onButton(el, handler) {
  el.addEventListener("mousedown", (e) => e.preventDefault());
  el.addEventListener("click", (e) => {
    e.preventDefault();
    handler(e);
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

// Every Insert ribbon button runs the very command its menu row and palette
// entry run — one implementation behind three surfaces, wired through the shared
// `onButton` seam so pointer activation never steals the selection. Insert table
// is the exception: it has no direct action, it opens the row × column grid
// popover, and is wired by `registerPopover` where that popover is built.
// Every Review ribbon button runs the very command its menu row and palette
// entry run, through the shared `onButton` seam, and carries its command id so
// the ribbon's membership is readable from the DOM for the parity test.
for (const entry of REVIEW_SURFACE) {
  const button = entry.button();
  if (!button) continue;
  onButton(button, entry.run);
  button.dataset.command = entry.command;
}

for (const entry of INSERT_SURFACE) {
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
for (const entry of [...LAYOUT_SURFACE, ...REFERENCE_SURFACE]) {
  for (const button of entry.buttons()) {
    if (!button) continue;
    button.dataset.command = entry.command;
    // Remember the authored tooltip so `updateToolbar` can put it back after a
    // precondition message has replaced it.
    button.dataset.enabledTitle = button.title;
    if (entry.run) onButton(button, entry.run);
  }
}
for (const key of ["bold", "italic", "underline", "strike"]) {
  onButton(fmtButtons[key], () => toggleFormat(key));
}
onButton(clearFormattingBtn, () => {
  if (!hasRange()) {
    // A caret is not a range, so there is nothing to clear — but the control is
    // enabled (the caret IS a selection), so returning in silence made Clear
    // formatting a dead control on every surface that runs it: the ribbon
    // button, the Format menu row, the palette and the compact bar all end up
    // here. It refuses out loud instead, the same way the suggesting-mode
    // branch below already does.
    setStatus("Select the text whose formatting you want to clear", "error");
    return;
  }
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
  if (fmt.underline && !rs.underlineStyleMixed && rs.underlineStyle) {
    fmt.underlineStyle = rs.underlineStyle;
  }
  if (fmt.underline && !rs.underlineColorMixed) fmt.underlineColor = rs.underlineColor;
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
  if (fmt.paraStyle) await runToolbarEdit((a, b, c, d) => doc.setParagraphStyle(a, b, c, d, fmt.paraStyle), { paragraphLevel: true });
  await runToolbarEdit((a, b, c, d) =>
    doc.formatSelection(a, b, c, d, fmt.bold, fmt.italic, fmt.underline, fmt.strike),
  );
  if (fmt.underlineStyle) {
    await runToolbarEdit((a, b, c, d) => doc.setUnderlineStyle(a, b, c, d, fmt.underlineStyle));
  }
  if (fmt.underlineColor != null) {
    await runToolbarEdit((a, b, c, d) => doc.setUnderlineColor(a, b, c, d, fmt.underlineColor));
  }
  if (fmt.sizePoints != null) await runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, fmt.sizePoints));
  if (fmt.font) await runToolbarEdit((a, b, c, d) => doc.setFont(a, b, c, d, fmt.font));
  if (fmt.color) {
    const [r, g, b] = hexToRgb(fmt.color);
    await runToolbarEdit((a, x, c, d) => doc.setTextColor(a, x, c, d, r, g, b));
  }
  if (fmt.highlight) await runToolbarEdit((a, b, c, d) => doc.setHighlight(a, b, c, d, fmt.highlight));
  if (fmt.vertAlign) await runToolbarEdit((a, b, c, d) => doc.setVertAlign(a, b, c, d, fmt.vertAlign));
  if (fmt.align) await runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, fmt.align), { paragraphLevel: true });
  if (fmt.lineSpacing) await runToolbarEdit((a, b, c, d) => doc.setLineSpacing(a, b, c, d, fmt.lineSpacing), { paragraphLevel: true });
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
  onButton(btn, () => runToolbarEdit((a, b, c, d) => doc.setAlignment(a, b, c, d, key), { paragraphLevel: true }));
}
/** Word-style indent commands: list items change numbering level, while ordinary
 * paragraphs retain the existing 0.25in paragraph-indent behavior. */
function adjustIndentCommand(delta) {
  if (!selection || !doc) return;
  const listKind = doc.listStyleAt(selection.focus.node);
  runToolbarEdit((a, b, c, d) =>
    listKind
      ? doc.adjustListLevel(a, b, c, d, delta > 0 ? 1 : -1)
      : doc.adjustIndent(a, b, c, d, delta), { paragraphLevel: true });
}
onButton(indentDecBtn, () => adjustIndentCommand(-360));
onButton(indentIncBtn, () => adjustIndentCommand(360));
onButton(bulletListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "bullet"), { paragraphLevel: true }));
onButton(numberedListBtn, () => runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "numbered"), { paragraphLevel: true }));
/** Toggles the caret's paragraphs into (or out of) a checklist. Shared by the
 *  ribbon button and the `paragraph.list.checklist` command, so the two cannot
 *  diverge — the command used not to exist at all, which left the checklist
 *  reachable only by finding one button on the Home tab. */
function toggleChecklistCommand() {
  runToolbarEdit((a, b, c, d) => doc.toggleList(a, b, c, d, "checklist"), { paragraphLevel: true });
  // A brand-new checklist introduces the `☐` marker glyph; fetch its covering
  // symbol font (once) so it renders instead of a .notdef box, then re-render.
  void ensureGlyphCoverage("checklist");
}
onButton(checkListBtn, toggleChecklistCommand);
onButton(restartListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.restartList(selection.focus.node));
});
onButton(continueListBtn, () => {
  if (selection && doc) runNodeEdit(() => doc.continueList(selection.focus.node));
});

/** Inserts a SOFT line break at the caret — the Shift+Enter gesture.
 *
 * The text stays in one paragraph and so keeps its list membership, style,
 * numbering and spacing. Extracted from the Enter handler so `insert.lineBreak`
 * runs exactly this and not a second, quietly different version of it: the
 * gesture was keyboard-only, which is the same one-surface defect docs/104 keeps
 * recording. */
async function insertLineBreakAtSelection() {
  if (!doc || !selection) return;
  if (reviewMode === "suggesting") {
    setStatus("Line breaks cannot be tracked yet; switch to Editing to insert one", "error");
    return;
  }
  // A non-collapsed selection is replaced first, exactly as typing a character
  // would — otherwise the break lands beside text the user meant to overwrite.
  if (hasRange()) {
    const { anchor, focus } = selection;
    const ok = await runEdit(() =>
      doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset),
    );
    if (!ok || !selection) return;
  }
  const at = selection.focus;
  await runEdit(() => doc.insertLineBreak(at.node, at.offset));
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
    fontSizeSel.setCustomValidity(
      valid ? "" : "Enter a font size from 1 to 1638 pt in 0.5 pt steps.",
    );
  }
  if (!valid) {
    if (report) {
      fontSizeSel.reportValidity();
      updateToolbar();
    }
    return false;
  }
  armOrApplyRun({ sizeHalfPoints: Math.round(pt * 2) }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, pt)),
  );
  return true;
}

fontSizeSel.addEventListener("change", () => {
  if (fontSizeSel.value.trim() === "") {
    fontSizeSel.setCustomValidity("Enter a font size from 1 to 1638 pt in 0.5 pt steps.");
    fontSizeSel.reportValidity();
    updateToolbar();
    return;
  }
  applyFontSize(fontSizeSel.value);
});
// ---- Toolbar popovers (compact anchored menus such as spacing) --------------
// One lightweight manager: anchor a menu under its button, only one open at a
// time, dismiss on outside-pointerdown / Escape. Each popover registers a
// `reflect()` that syncs its controls to the caret paragraph.
const TWIPS_PER_POINT = 20;
const popovers = [];

function openPopover(p, { keyboard = false } = {}) {
  // An object can be selected with no text caret behind it (clicking a float
  // first thing), and its Fill/Outline pickers must still open.
  if (!selection && !objectSelection) return;
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
  // Opened from the keyboard, the popover takes focus (docs/104 HF-070: it used
  // to leave focus on the trigger, so reaching a swatch meant tabbing through
  // the rest of the ribbon first). Opened by pointer it deliberately does NOT,
  // because these menus preserve the document selection on mouse interaction —
  // that is what the mousedown preventDefault in registerPopover is for.
  if (keyboard) focusFirstIn(p.menu);
}

// A control that reports itself as the current choice. Focus belongs on it
// rather than on the first row: opening the spacing menu should land on the
// spacing this paragraph already has, the way a native menu opens on its checked
// item, so the arrow keys start from where the user is (docs/104 HF-070).
const CHECKED_SELECTOR = '[aria-checked="true"], [aria-pressed="true"], [aria-selected="true"]';

/** Focuses the checked control inside `container`, or the first visible, enabled
 *  one when nothing is checked. */
function focusFirstIn(container) {
  const focusable = [
    ...container.querySelectorAll(
      "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
    ),
  ].filter((element) => element.getClientRects().length > 0);
  const target = focusable.find((element) => element.matches(CHECKED_SELECTOR)) ?? focusable[0];
  target?.focus({ preventScroll: true });
  return target ?? null;
}

function closePopover(p) {
  // Closing must not strand the keyboard: if focus is inside the menu it goes
  // back to the trigger, which is where the user's place was.
  const holdsFocus = p.menu.contains(document.activeElement);
  p.menu.hidden = true;
  p.btn.setAttribute("aria-expanded", "false");
  if (holdsFocus) p.btn.focus({ preventScroll: true });
}

function registerPopover(btn, menu, reflect) {
  const p = { btn, menu, reflect };
  popovers.push(p);
  onButton(btn, (event) =>
    menu.hidden ? openPopover(p, { keyboard: event?.detail === 0 }) : closePopover(p),
  );
  // Keep clicks inside the menu from stealing the selection focus, but let form
  // controls (inputs, selects) focus, toggle, and open normally.
  menu.addEventListener("mousedown", (e) => {
    if (!["INPUT", "SELECT", "OPTION"].includes(e.target.tagName)) e.preventDefault();
  });
  return p;
}

tableStylePopover = registerPopover(tableStyleBtn, tableStyleMenu, () => {});

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
stylesMenuInput.addEventListener("input", () => renderStylesGallery());
const stylesPopover = registerPopover(stylesTrigger, stylesMenu, () => {
  // Rebuild on open rather than on every caret move: the suggested set depends
  // on the caret's style, and the menu is not on screen while the caret moves.
  stylesMenuInput.value = "";
  renderStylesGallery();
  syncStylesGalleryActive();
});

// `pointerdown` — the phase every other dismissable surface uses (#556). The
// last holdout on `mousedown`, which a pen or a consumed touch never produces.
document.addEventListener("pointerdown", (e) => {
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
const recentUnderlineColors = [];
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

function makeUnderlineStyleOption(style, label) {
  const option = document.createElement("button");
  option.type = "button";
  option.className = "color-row-action underline-style-option";
  option.dataset.underlineStyle = style;
  option.setAttribute("role", "radio");
  option.setAttribute(
    "aria-checked",
    String(!currentUnderlineMixed && style === currentUnderlineStyle),
  );
  const preview = document.createElement("span");
  preview.className = `underline-style-preview underline-style-${style}`;
  preview.textContent = style === "none" ? "ab" : "Sample";
  option.append(preview, document.createTextNode(label));
  return option;
}

/** Builds the combined underline style/color menu from canonical engine tokens.
 *  The explicit Automatic row maps to `Some(None)` in the edit delta. */
function renderUnderlineMenu() {
  underlineMenu.replaceChildren();
  underlineMenu.appendChild(makeMenuHeading("Underline style"));
  const styleGroup = document.createElement("div");
  styleGroup.setAttribute("role", "radiogroup");
  styleGroup.setAttribute("aria-label", "Underline style");
  for (const [style, label] of UNDERLINE_STYLE_LABELS) {
    styleGroup.appendChild(makeUnderlineStyleOption(style, label));
  }
  underlineMenu.appendChild(styleGroup);

  underlineMenu.appendChild(makeMenuHeading("Underline color"));
  const automatic = document.createElement("button");
  automatic.type = "button";
  automatic.className = "color-row-action";
  automatic.dataset.underlineAuto = "1";
  automatic.innerHTML = '<span class="color-chip" style="--sw:#000000"></span><span>Automatic (text color)</span>';
  automatic.classList.toggle("is-active", !currentUnderlineMixed && !currentUnderlineColor);
  underlineMenu.appendChild(automatic);
  underlineMenu.appendChild(makeSwatchGrid(
    TEXT_STANDARD_COLORS.map((hex) =>
      makeSwatchCell(
        "text",
        hex,
        hex,
        `Underline ${hex.toUpperCase()}`,
        !currentUnderlineMixed && hex.toLowerCase() === currentUnderlineColor.toLowerCase(),
      )),
  ));
  if (recentUnderlineColors.length) {
    underlineMenu.appendChild(makeMenuHeading("Recent"));
    underlineMenu.appendChild(makeSwatchGrid(
      recentUnderlineColors.map((hex) =>
        makeSwatchCell(
          "text",
          hex,
          hex,
          `Underline ${hex.toUpperCase()}`,
          !currentUnderlineMixed && hex.toLowerCase() === currentUnderlineColor.toLowerCase(),
        )),
    ));
  }
  const more = document.createElement("button");
  more.type = "button";
  more.className = "color-row-action color-more";
  more.dataset.underlineMore = "1";
  more.innerHTML = '<span class="ms" aria-hidden="true">colorize</span><span>More colors…</span>';
  underlineMenu.appendChild(more);
}

const textColorPopover = registerPopover(textColorCaret, textColorMenu, () => renderColorMenu("text"));
const highlightPopover = registerPopover(highlightCaret, highlightMenu, () => renderColorMenu("highlight"));
const underlinePopover = registerPopover(underlineMenuBtn, underlineMenu, renderUnderlineMenu);

function underlineEditBlockedByReview() {
  if (reviewMode !== "suggesting") return false;
  setStatus(
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
  armOrApplyRun(patch, () =>
    runToolbarEdit((a, b, c, d) => doc.setUnderlineStyle(a, b, c, d, style)),
  );
}

function applyUnderlineColor(color) {
  if (underlineEditBlockedByReview()) return;
  const patch = color ? { underline: true, underlineColor: color } : { underlineColor: "" };
  armOrApplyRun(patch, () =>
    runToolbarEdit((a, b, c, d) => doc.setUnderlineColor(a, b, c, d, color)),
  );
}

underlineMenu.addEventListener("click", (e) => {
  const style = e.target.closest("[data-underline-style]")?.dataset.underlineStyle;
  if (style) {
    applyUnderlineStyle(style);
    closePopover(underlinePopover);
    focusEditorSurface();
    return;
  }
  if (e.target.closest("[data-underline-more]")) {
    underlineColorInput.value = currentUnderlineColor || "#000000";
    underlineColorInput.click();
    return;
  }
  const colorCell = e.target.closest("[data-color], [data-underline-auto]");
  if (!colorCell) return;
  const color = colorCell.dataset.underlineAuto ? "" : colorCell.dataset.color;
  applyUnderlineColor(color);
  if (color) recordRecent(recentUnderlineColors, color);
  closePopover(underlinePopover);
  focusEditorSurface();
});
underlineColorInput.addEventListener("change", () => {
  const color = underlineColorInput.value;
  applyUnderlineColor(color);
  recordRecent(recentUnderlineColors, color);
  closePopover(underlinePopover);
  focusEditorSurface();
});

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

// --- Shape Fill / Shape Outline (Word's Shape Format tab) --------------------
// A drawing was renderable, selectable, movable and resizable but had no way to
// change what it LOOKS like: the model carried `fill` and `stroke` and nothing
// could write either. These are the two controls Word's Shape Format tab is
// built around, on the same palette as the text pickers.

/** What the context bar calls each kind of object. A shape used to read
 *  "Image", which also handed it a Crop button it has no source rectangle for. */
/** The preset shapes Insert ▸ Shapes offers — every geometry the engine models
 *  and the renderer draws. Naming them here keeps the menu honest: a gallery of
 *  shapes that do not render would be worse than a short one that does. */
const SHAPE_CHOICES = [
  ["rectangle", "Rectangle"],
  ["roundRectangle", "Rounded rectangle"],
  ["ellipse", "Ellipse"],
  ["triangle", "Triangle"],
  ["rightTriangle", "Right triangle"],
  ["diamond", "Diamond"],
  ["line", "Line"],
];
const SHAPE_LABELS = Object.fromEntries(SHAPE_CHOICES);

const EMU_PER_POINT = 12700;
/** The short palette the right-click submenu offers; the bar's picker has the
 *  full grid. A menu is a list, so it gets the colors people actually reach for. */
const SHAPE_MENU_COLORS = ["#000000", "#ffffff", "#ff0000", "#ff9900", "#ffff00", "#00ff00", "#4a86e8", "#9900ff"];
/** Word's Shape Outline > Weight list, in points. */
const OUTLINE_WEIGHTS = [0.25, 0.5, 1, 1.5, 2.25, 3, 4.5, 6];

const shapeFillMenu = document.getElementById("shapeFillMenu");
const shapeOutlineMenu = document.getElementById("shapeOutlineMenu");

/** The selected shape's current fill/outline, or an empty record when there is
 *  no shape selected. Read from the model, never remembered from the last
 *  apply — the swatch must describe THIS shape. */
function selectedShapeFormat() {
  if (
    !doc ||
    !objectSelection ||
    objectSelection.kind !== "shape" ||
    (!objectSelection.canFill && !objectSelection.canStroke)
  ) return {};
  try {
    return doc.shapeFormat(objectSelection.node) ?? {};
  } catch {
    return {};
  }
}

/** Paints each bar button's underline bar with the shape's own color (or the
 *  none-hatch when it has none), so the control reads before it is opened. */
function reflectShapeSwatches() {
  reflectShapeFormatState();
  const format = selectedShapeFormat();
  for (const [btn, color] of [
    [shapeFillBtn, format.fill],
    [shapeOutlineBtn, format.outline],
  ]) {
    const bar = btn.querySelector(".color-apply-bar");
    if (!bar) continue;
    bar.style.background = color || "transparent";
    bar.classList.toggle("is-none", !color);
  }
}

/** A bar button carrying a color bar, matching the ribbon's split controls. */
function makeShapeBarButton(icon, label, title) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "object-bar-btn object-bar-color";
  btn.title = title;
  btn.setAttribute("aria-label", title);
  btn.setAttribute("aria-haspopup", "dialog");
  btn.setAttribute("aria-expanded", "false");
  btn.innerHTML =
    `<span class="ms" aria-hidden="true">${icon}</span><span>${label}</span>` +
    `<span class="color-apply-bar" aria-hidden="true"></span>`;
  // The other bar buttons cancel `pointerdown` to keep the selection, but these
  // open popovers, and cancelling pointerdown suppresses the compatibility
  // `mousedown` the popover manager opens on — the button would do nothing at
  // all. `onButton` cancels that mousedown itself, preserving the selection.
  return btn;
}

const shapeFillBtn = makeShapeBarButton("format_color_fill", "Fill", "Shape fill");
const shapeOutlineBtn = makeShapeBarButton("border_color", "Outline", "Shape outline");

/** Renders a shape color menu: the reset row Word leads with ("No fill" /
 *  "No outline"), the standard palette, and — for the outline — its weights. */
function renderShapeMenu(kind) {
  const menu = kind === "fill" ? shapeFillMenu : shapeOutlineMenu;
  const format = selectedShapeFormat();
  const active = (kind === "fill" ? format.fill : format.outline)?.toLowerCase() ?? null;
  menu.replaceChildren();

  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "color-row-action";
  reset.dataset.shapeNone = "1";
  reset.innerHTML =
    '<span class="color-chip color-chip-none"></span>' +
    `<span>${kind === "fill" ? "No fill" : "No outline"}</span>`;
  menu.appendChild(reset);

  menu.appendChild(makeMenuHeading("Standard colors"));
  menu.appendChild(
    makeSwatchGrid(
      TEXT_STANDARD_COLORS.map((hex) => {
        const cell = makeSwatchCell("text", hex, hex, hex.toUpperCase(), hex.toLowerCase() === active);
        // The grid helper tags cells for the TEXT handler; retag so a shape click
        // cannot be mistaken for a text-color one.
        delete cell.dataset.color;
        cell.dataset.shapeColor = hex;
        return cell;
      }),
    ),
  );

  if (kind === "outline") {
    const current = format.outlineWidthEmu ?? null;
    menu.appendChild(makeMenuHeading("Weight"));
    const list = document.createElement("div");
    list.className = "shape-weight-list";
    for (const points of OUTLINE_WEIGHTS) {
      const emu = Math.round(points * EMU_PER_POINT);
      const row = document.createElement("button");
      row.type = "button";
      row.className = "color-row-action shape-weight";
      row.dataset.shapeWeight = String(emu);
      if (current === emu) row.classList.add("is-active");
      row.innerHTML =
        `<span class="shape-weight-rule" style="--w:${Math.max(1, points)}px" aria-hidden="true"></span>` +
        `<span>${points} pt</span>`;
      list.appendChild(row);
    }
    menu.appendChild(list);
  }
}

const shapeFillPopover = registerPopover(shapeFillBtn, shapeFillMenu, () => renderShapeMenu("fill"));
const shapeOutlinePopover = registerPopover(shapeOutlineBtn, shapeOutlineMenu, () =>
  renderShapeMenu("outline"),
);

/** Applies a fill to the selected shape (`null` clears it). One undoable action,
 *  through the same gate every object edit uses. */
function applyShapeFill(hex) {
  if (!objectSelection?.canFill || objectSelection.kind !== "shape") return;
  const node = objectSelection.node;
  runEdit(() => doc.setShapeFill(node, hex ?? undefined), { gate: true });
  reflectShapeSwatches();
}

/** Applies an outline color and/or weight. Setting a weight on an unoutlined
 *  shape gives it Word's default black outline, so the weight is never a no-op. */
function applyShapeOutline({ color, widthEmu }) {
  if (!objectSelection?.canStroke || objectSelection.kind !== "shape") return;
  const node = objectSelection.node;
  // Pass only what was chosen. The engine inherits the rest from the outline the
  // shape already has — including the dash pattern and line ends no control here
  // can express, which a locally-rebuilt stroke would quietly discard.
  runEdit(() => doc.setShapeOutline(node, color ?? undefined, widthEmu ?? undefined), {
    gate: true,
  });
  reflectShapeSwatches();
}

function handleShapeMenuClick(event, kind, popover) {
  const none = event.target.closest("[data-shape-none]");
  const cell = event.target.closest("[data-shape-color]");
  const weight = event.target.closest("[data-shape-weight]");
  if (!none && !cell && !weight) return;
  if (kind === "fill") {
    applyShapeFill(none ? null : cell.dataset.shapeColor);
  } else if (none) {
    applyShapeOutline({ color: null });
  } else if (weight) {
    applyShapeOutline({ widthEmu: Number(weight.dataset.shapeWeight) });
  } else {
    applyShapeOutline({ color: cell.dataset.shapeColor });
  }
  closePopover(popover);
}

shapeFillMenu.addEventListener("click", (e) => handleShapeMenuClick(e, "fill", shapeFillPopover));
shapeOutlineMenu.addEventListener("click", (e) =>
  handleShapeMenuClick(e, "outline", shapeOutlinePopover),
);

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
// --- Insert ▸ Shapes gallery -------------------------------------------------
const shapeGalleryMenu = document.getElementById("shapeGalleryMenu");

/** Renders one row per preset the engine models AND the renderer draws, each
 *  previewing its own outline so the list reads as shapes, not words. */
function renderShapeGallery() {
  shapeGalleryMenu.replaceChildren();
  for (const [geometry, label] of SHAPE_CHOICES) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "menu-item shape-choice";
    row.setAttribute("role", "menuitem");
    row.dataset.shapeGeometry = geometry;
    const preview = document.createElement("span");
    preview.className = `shape-choice-preview shape-preview-${geometry}`;
    preview.setAttribute("aria-hidden", "true");
    row.appendChild(preview);
    const text = document.createElement("span");
    text.textContent = label;
    row.appendChild(text);
    shapeGalleryMenu.appendChild(row);
  }
}

const shapeGalleryPopover = registerPopover(insertShapeBtn, shapeGalleryMenu, renderShapeGallery);

/** Opens the gallery, wherever the command came from (ribbon button or palette). */
function openShapeGallery() {
  if (!doc) return;
  if (shapeGalleryMenu.hidden) openPopover(shapeGalleryPopover);
}

shapeGalleryMenu.addEventListener("click", (event) => {
  const choice = event.target.closest("[data-shape-geometry]");
  if (!choice) return;
  closePopover(shapeGalleryPopover);
  void insertShapeObject(choice.dataset.shapeGeometry);
});

const bulletGalleryPopover = registerPopover(bulletListMenuBtn, bulletGalleryMenu, () =>
  reflectListGallery(bulletGalleryMenu),
);
const numberGalleryPopover = registerPopover(numberedListMenuBtn, numberGalleryMenu, () =>
  reflectListGallery(numberGalleryMenu),
);
/** Applies a list-marker spec ("bullet:•", "numbered:decimal", …) to the caret's
 *  list. Shared by the gallery cells and by the `paragraph.listFormat.*`
 *  commands, which are generated from the very same cells. */
function applyListFormatCommand(spec) {
  if (!selection || !doc) return;
  const applied = runNodeEdit(() => doc.setListFormat(selection.focus.node, spec));
  // A newly chosen bullet glyph may need its covering symbol font fetched
  // (once) so it renders instead of a .notdef box, then re-render.
  if (applied && spec.startsWith("bullet:")) void ensureGlyphCoverage("list marker");
}
function wireListGallery(menu, popover) {
  menu.addEventListener("click", (e) => {
    const cell = e.target.closest("[data-spec]");
    if (!cell || !selection || !doc) return;
    closePopover(popover);
    applyListFormatCommand(cell.dataset.spec);
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
    runToolbarEdit((a, x, c, d) => doc.setLineSpacing(a, x, c, d, Number(b.dataset.percent)), { paragraphLevel: true });
    reflectSpacingMenu();
  });
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
    runToolbarEdit((a, x, c, d) => doc.setLineSpacing(a, x, c, d, percent), { paragraphLevel: true });
  } else {
    const twips = Math.max(0, Math.round(v * TWIPS_PER_POINT));
    const atLeast = mode === "atLeast";
    runToolbarEdit((a, x, c, d) => doc.setLineSpacingExact(a, x, c, d, twips, atLeast), { paragraphLevel: true });
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
/** An inches field's value → twips (≥ 0); "" or non-numeric → 0. */
function inchTwips(input) {
  return inchesToTwips(input.value);
}

/** An inches field's value → signed twips; blank/non-numeric → 0. */
function signedInchTwips(input) {
  return signedInchesToTwips(input.value);
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
    indentLeftInput.value = state.startMixed ? "" : twipsToInchText(state.startTwip);
  }
  if (document.activeElement !== indentRightInput) {
    indentRightInput.placeholder = state.endMixed ? "Mixed" : "";
    indentRightInput.value = state.endMixed ? "" : twipsToInchText(state.endTwip);
  }
  if (![indentSpecialByInput, indentSpecialSel].includes(document.activeElement)) {
    if (state.firstLineMixed || state.hangingMixed) {
      indentSpecialSel.value = "";
      indentSpecialByInput.value = "";
      indentSpecialByInput.placeholder = "Mixed";
    } else if (state.firstLineTwip > 0) {
      indentSpecialSel.value = "first";
      indentSpecialByInput.value = twipsToInchText(state.firstLineTwip);
      indentSpecialByInput.placeholder = "";
    } else if (state.hangingTwip > 0) {
      indentSpecialSel.value = "hanging";
      indentSpecialByInput.value = twipsToInchText(state.hangingTwip);
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

// `focusTarget` lets Layout ▸ Indent and Layout ▸ Spacing land on the fieldset
// the user asked for. The panel owns the real indent/spacing values — the ribbon
// deliberately holds no second copy of them — so the deep link is what makes the
// ribbon button honest about what it does.
function toggleParagraphProperties(open, focusTarget = null) {
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
    queueMicrotask(() => (focusTarget?.() ?? paraPanelStyle).focus());
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
    doc.setParagraphStyle(a, b, c, d, paraPanelStyle.value), { paragraphLevel: true }),
);
for (const button of paraPanelAlign.querySelectorAll("button[data-palign]")) {
  onButton(button, () =>
    runToolbarEdit((a, b, c, d) =>
      doc.setAlignment(a, b, c, d, button.dataset.palign), { paragraphLevel: true }),
  );
}
paraLineSpacing.addEventListener("change", () => {
  if (!paraLineSpacing.value) return;
  runToolbarEdit((a, b, c, d) =>
    doc.setLineSpacing(a, b, c, d, Number(paraLineSpacing.value)), { paragraphLevel: true });
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
  const revision = readRevision(res);
  res.free();
  noteDocumentEdited(revision);
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

// Split cell had no keydown listener, no backdrop handler, no focus trap and no
// Enter-to-confirm, and it closed onto `splitCellBtn` — a button inside a hidden
// contextual ribbon panel, so `.focus()` was a no-op and the keyboard fell to
// <body>, which is why the editor appeared frozen afterwards (HF-062). The
// primitive supplies all of it; `fallbackFocus` is what catches the hidden
// button, since the controller only restores focus to a control still on screen.
const splitCellModal = registerModal(splitCellDialog, {
  initialFocus: () => splitCellColumns,
  fallbackFocus: () => pagesEl,
  defaultAction: () => void applySplitCell(),
});

function toggleSplitCellDialog(open) {
  if (open) {
    splitCellRows.value = "1";
    splitCellColumns.value = "2";
    splitCellColumns.setCustomValidity("");
    splitCellModal.open();
  } else {
    splitCellModal.close();
  }
}

/** Splits the caret's merged cell into rows x columns. Refuses out-of-range
 *  input inline and keeps the dialog open with the typed values intact, rather
 *  than closing on a number the engine will not accept. */
async function applySplitCell() {
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
}

onButton(splitCellBtn, () => {
  if (!selection || !doc) return;
  toggleSplitCellDialog(true);
});

onButton(splitCellClose, () => toggleSplitCellDialog(false));
onButton(splitCellCancel, () => toggleSplitCellDialog(false));
onButton(splitCellConfirm, () => void applySplitCell());

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

/** An optional inches field's value → twips, or -1 for "leave it unset". */
function optionalDialogTwips(input) {
  return optionalInchesToTwips(input.value);
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
  tableColumnWidth.value = twipsToDialogInches(info.columnWidthTwips);
  tableColumnWidthNote.textContent = info.regular
    ? "Sets the width of the current column."
    : "Column sizing is unavailable for merged or spanned tables.";
  tableWidth.value = twipsToDialogInches(info.tableWidthTwips);
  tableIndent.value = twipsToDialogInches(info.tableIndentTwips);
  tableRowHeight.value = twipsToDialogInches(info.rowHeightTwips);
  tableRowHeightRule.value = info.rowHeightRule || "auto";
  tableCellMargin.value = twipsToDialogInches(info.cellMarginTwips);
  tableCellSpacing.value = twipsToDialogInches(info.cellSpacingTwips);
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
// It is a real grid, not eighty anonymous buttons: rows carry `role="row"` (with
// `display: contents`, so the ten-column CSS grid is untouched), every cell has
// the size it inserts as its accessible name, and one roving tab stop plus arrow
// keys makes it navigable. Insertion lives on the cell's `click`, so the pointer
// and the keyboard travel the same path instead of the keyboard having none.
const GRID_ROWS = 8;
const GRID_COLS = 10;
const gridCells = [];
gridPicker.setAttribute("role", "grid");
gridPicker.setAttribute("aria-label", "Table size");
for (let r = 1; r <= GRID_ROWS; r++) {
  const row = document.createElement("div");
  row.setAttribute("role", "row");
  row.style.display = "contents";
  const cells = [];
  for (let c = 1; c <= GRID_COLS; c++) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "gc";
    cell.dataset.r = String(r);
    cell.dataset.c = String(c);
    cell.setAttribute("role", "gridcell");
    cell.setAttribute("aria-label", `${c} by ${r} table`);
    cell.tabIndex = r === 1 && c === 1 ? 0 : -1;
    row.appendChild(cell);
    cells.push(cell);
  }
  gridPicker.appendChild(row);
  gridCells.push(cells);
}
/** The cell at 1-based `r`/`c`, or undefined outside the grid. */
const gridCellAt = (r, c) => gridCells[r - 1]?.[c - 1];

function highlightGrid(rows, cols) {
  for (const row of gridCells) {
    for (const cell of row) {
      const on = Number(cell.dataset.r) <= rows && Number(cell.dataset.c) <= cols;
      cell.classList.toggle("on", on);
    }
  }
  gridLabel.textContent = rows ? `${cols} × ${rows}` : "Insert table";
}

/** Moves the single tab stop to `r`/`c`, previews that size, and focuses it —
 *  the roving-tabindex pattern, so Tab enters the grid once and the arrows do
 *  the rest. */
function focusGridCell(r, c) {
  const cell = gridCellAt(r, c);
  if (!cell) return;
  for (const row of gridCells) for (const other of row) other.tabIndex = other === cell ? 0 : -1;
  highlightGrid(r, c);
  cell.focus();
}

async function insertTableFromGrid(cell) {
  if (!cell || !selection || !doc) return;
  const rows = Number(cell.dataset.r);
  const cols = Number(cell.dataset.c);
  await runEdit(() => doc.insertTable(selection.focus.node, rows, cols), { gate: true });
  closePopover(insertTablePopover);
  focusEditorSurface();
}

gridPicker.addEventListener("pointermove", (e) => {
  const cell = e.target.closest(".gc");
  if (cell) highlightGrid(Number(cell.dataset.r), Number(cell.dataset.c));
});
gridPicker.addEventListener("pointerleave", () => highlightGrid(0, 0));
// Keep a pointer press from collapsing the document selection the table is
// about to be inserted into; the click that follows is what inserts.
gridPicker.addEventListener("pointerdown", (e) => {
  if (e.target.closest(".gc")) e.preventDefault();
});
gridPicker.addEventListener("click", (e) => {
  const cell = e.target.closest(".gc");
  if (cell) void insertTableFromGrid(cell);
});
gridPicker.addEventListener("focusin", (e) => {
  const cell = e.target.closest(".gc");
  if (cell) highlightGrid(Number(cell.dataset.r), Number(cell.dataset.c));
});
gridPicker.addEventListener("keydown", (e) => {
  const cell = e.target.closest(".gc");
  if (!cell) return;
  const r = Number(cell.dataset.r);
  const c = Number(cell.dataset.c);
  const step = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[e.key];
  if (step) {
    const next = gridCellAt(r + step[0], c + step[1]);
    if (!next) return; // at an edge: stay put rather than wrapping to a different size
    e.preventDefault();
    focusGridCell(r + step[0], c + step[1]);
  } else if (e.key === "Home") {
    e.preventDefault();
    focusGridCell(e.ctrlKey || e.metaKey ? 1 : r, 1);
  } else if (e.key === "End") {
    e.preventDefault();
    focusGridCell(e.ctrlKey || e.metaKey ? GRID_ROWS : r, GRID_COLS);
  } else if (e.key === "Escape") {
    e.preventDefault();
    closePopover(insertTablePopover);
    insertTableBtn.focus();
  }
  // Enter and Space need no handling: these are real buttons, so the browser
  // turns them into the same `click` the pointer path uses.
});
const insertTablePopover = registerPopover(insertTableBtn, insertTableMenu, () => {
  // Opening puts the keyboard inside the grid at 1×1. Without this the popover
  // opened behind the focus ring and Tab walked past it into the rest of the
  // page, which is what made the whole picker pointer-only.
  focusGridCell(1, 1);
  highlightGrid(0, 0);
});

/** Rebuilds the off-screen accessibility mirror for the caret's part of the
 *  document. The projection itself lives in `a11y_mirror.mjs`; what stays here
 *  is the two pieces of editor state it needs. */
function buildAccessibilityTree() {
  renderAccessibilityMirror(doc, selection?.focus?.node ?? "");
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
  // Picking an outline row IS placing the caret, so the caret must be painted
  // even when the target is the first heading — the one node that happens to
  // sit exactly where the load-time insertion point was seeded.
  implicitCaretAt = null;
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

/** The page the navigator marks as current: the caret's, falling back to the
 *  one being read. It is NOT what the panel centres on — a reader who has
 *  scrolled 6,000 pages away from their caret wants to see where they are. */
function pagesPanelFocusPage() {
  if (selection) {
    const flat = doc.caretRect(selection.focus.node, selection.focus.offset);
    if (flat.length) return flat[0];
  }
  return pageInView()?.pageNumber ?? 1;
}

/** Rebuilds the Pages navigator around one page — by default the one being
 *  read. The panel shows a WINDOW of thumbnails (`pages_panel.mjs`), because a
 *  card per page is a `renderPage` per page. */
function buildPages(centre = null) {
  if (!doc || pagesPanel.hidden) return;
  const focus = pagesPanelFocusPage();
  pagesPanelRange = renderPagesPanel({
    doc,
    pages,
    current: centre ?? pageInView()?.pageNumber ?? focus,
    body: pagesBody,
    onJump: (n) => goToPage(n),
  });
  reflectPagesSelection(focus);
}

/** Follow the reader: when the viewport leaves the range of pages the panel is
 *  showing, rebuild it around where they now are. Scrolling inside the shown
 *  range costs nothing. */
function syncPagesPanelToViewport() {
  if (pagesPanel.hidden || !doc) return;
  const visible = pageInView()?.pageNumber ?? 1;
  if (visible < pagesPanelRange.start || visible > pagesPanelRange.end) buildPages(visible);
}

/** Scrolls page `n` into view using the single scroll owner, then highlights it. */
function goToPage(n) {
  const page = pages[n - 1];
  if (!page || !pageBandModel) return;
  // From the band's geometry, not from the sheet's rect: the page being jumped
  // to is usually the one page in the document that has no sheet yet.
  const viewportHeight = viewportEl.clientHeight;
  const docY = Math.max(0, pageBandModel.tops[n - 1] - 16);
  const target = bandTopInScroller + docToScroll(pageBandModel, viewportHeight, docY);
  const max = Math.max(0, viewportEl.scrollHeight - viewportEl.clientHeight);
  viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
  updatePageWindow();
  paintPagesInView();
  reflectPagesSelection(n);
}

/** Keeps the Pages navigator's active card synchronized with the caret's page. */
function reflectPagesSelection(pageNumber) {
  if (pagesPanel.hidden) return;
  reflectPagesPanelSelection(pagesBody, pageNumber);
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
    case "paragraph_format": return "Formatted paragraph";
    case "paragraph_mark_insertion": return "Added a paragraph break";
    case "paragraph_mark_deletion": return "Deleted a paragraph break";
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
      // A zero-length revision is normally nothing to visit. Formatting changes
      // and paragraph-level revisions are the exceptions: a paragraph mark has no
      // text of its own, and skipping it made Next/Previous step straight past
      // every paragraph break a reviewer suggested (docs/108).
      if (!String(revision.text || "").length && !REVIEW_TEXTLESS_KINDS.has(revision.kind)) continue;
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

/** Next (`+1`) / Previous (`-1`) across the unified comment+change list, wrapping
 *  at the ends. When the caret already sits on an item, it steps by list index
 *  (robust to boundary-adjacent items); otherwise it lands on the nearest item
 *  after/before the caret in reading order. */
function navigateReview(direction) {
  if (!doc) return;
  const targets = reviewNavTargets();
  if (!targets.length) {
    // The Review surface deliberately keeps Next/Previous enabled whatever the
    // document holds, on the stated grounds that "the commands themselves
    // report why nothing happened" (see REVIEW_SURFACE). This one did not: with
    // no comments and no tracked changes it returned in silence, so Review ▸
    // Next was a control that did nothing on a clean document — the same dead
    // control Tools ▸ Settings was, by a different route.
    setStatus("This document has no comments or tracked changes", "", { timeout: 3000 });
    return;
  }
  const current = reviewCurrentTargetIndex(targets, selection);
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
/** Accept or reject every tracked change, for whichever surface asked.
 *
 *  The Review menu and the Review ribbon used to reach this by clicking the
 *  sidebar's own button — and `updateReviewControls` DISABLES that button when
 *  the document holds no changes. A disabled button dispatches no click, so the
 *  menu row and the ribbon button did nothing at all, and said nothing either:
 *  the same dead control Tools ▸ Settings was, reached by a different route.
 *  Surfaces call this instead, and it answers for itself. Viewing mode is still
 *  refused by `runEdit`, which reports that in its own words. */
async function decideAllReviewChanges(accept) {
  if (!doc) return;
  let count = 0;
  try {
    count = (JSON.parse(doc.listRevisions()) ?? []).length;
  } catch {
    count = 0;
  }
  if (!count) {
    setStatus("This document has no tracked changes", "", { timeout: 3000 });
    return;
  }
  await runEdit(() => doc.decideAllRevisions(accept));
  announceReview(accept ? "All changes accepted" : "All changes rejected");
  scheduleReviewMarginRender();
}
reviewAcceptAll.addEventListener("click", () => void decideAllReviewChanges(true));
reviewRejectAll.addEventListener("click", () => void decideAllReviewChanges(false));
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

/** Shared command descriptors for search and contextual surfaces. Dynamic
 * entries are rebuilt so document styles and availability never go stale. */
/** Whether the caret sits in a numbered list — the precondition the Restart and
 *  Continue numbering controls share, on every surface that offers them. */
function numberedListAtCaret() {
  return !!doc && !!selection && doc.listStyleAt(selection.focus.node) === "numbered";
}

function editorCommands(context = { surface: "palette" }) {
  const fmt = (k) => () => toggleFormat(k);
  const align = (a) => () => runToolbarEdit((s, o, e, f) => doc.setAlignment(s, o, e, f, a), { paragraphLevel: true });
  const cmds = [
    // `noDoc`, and first: with no document open this is the only File command
    // that can run, and it is the one a user arriving with nothing to open needs.
    // No keyboard shortcut is claimed — ⌘N/Ctrl+N belongs to the browser window
    // and cannot be intercepted, and a shortcut hint the editor cannot honour
    // would be a lie printed in the palette.
    { id: "file.new", label: "New blank document", group: "File", kw: "new blank empty create start untitled document", noDoc: true, run: () => void newBlankDocument() },
    { id: "file.open", label: "Open…", group: "File", kw: "load docx odt json txt", noDoc: true, run: () => fileEl.click() },
    { id: "file.save", label: "Save", group: "File", kw: "export download", shortcut: "⌘S", run: () => saveDocument() },
    ...exportCommands(exportDocumentAs),
    // Reachable with no document open, because the case it exists for is
    // arriving at a fresh tab after a crash (HF-011). Disabled WITH A REASON
    // when the store is empty — never a control that silently does nothing.
    {
      id: "file.recoverDrafts",
      label: "Recover unsaved work…",
      group: "File",
      kw: "draft autosave recover crash restore unsaved backup",
      noDoc: true,
      enabled: draftOffers.length > 0,
      disabledReason: AUTOSAVE_ALLOWED_HERE
        ? "No unsaved work to recover"
        : "Autosave is off in an embedded editor",
      run: () => showDraftRecovery(),
    },
    { id: "file.print", label: "Print", group: "File", kw: "print pages paper hard copy pdf", shortcut: "⌘P", run: () => printDocument(doc) },
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
    // HF-147 — the face and the exact size were ribbon chrome with no command
    // id, so nothing but a mouse on that one control could set either. These two
    // open the control's own picker; the exact values are generated further down
    // as `format.family.<name>` and `format.size.<pt>`.
    {
      id: "format.family",
      label: "Font…",
      group: "Format",
      kw: "typeface face family font name",
      enabled: !!selection,
      disabledReason: "Place the caret or select text",
      run: () => fontFamilyBtn.click(),
    },
    {
      id: "format.size",
      label: "Font size…",
      group: "Format",
      kw: "points pt exact size font",
      enabled: !!selection,
      disabledReason: "Place the caret or select text",
      run: () => {
        fontSizeSel.focus();
        fontSizeSel.select();
      },
    },
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
    { id: "paragraph.list.bullet", label: "Bullet list", group: "Paragraph", kw: "unordered", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "bullet"), { paragraphLevel: true }) },
    { id: "paragraph.list.numbered", label: "Numbered list", group: "Paragraph", kw: "ordered", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => runToolbarEdit((s, o, e, f) => doc.toggleList(s, o, e, f, "numbered"), { paragraphLevel: true }) },
    { id: "paragraph.list.checklist", label: "Checklist", group: "Paragraph", kw: "checklist checkbox todo task tick check box", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => toggleChecklistCommand() },
    // Restart/continue take the SAME predicate the ribbon buttons take
    // (`updateToolbar`), so a row can never be live in the menu while the button
    // for it is greyed — they are now on both surfaces (docs/104 HF-076).
    { id: "paragraph.list.restart", label: "Restart numbering", group: "Paragraph", kw: "list restart 1", enabled: numberedListAtCaret(), disabledReason: "Place the caret in a numbered list", run: () => selection && runNodeEdit(() => doc.restartList(selection.focus.node)) },
    { id: "paragraph.list.continue", label: "Continue numbering", group: "Paragraph", kw: "list continue resume", enabled: numberedListAtCaret() && doc.canContinueList(selection.focus.node), disabledReason: "There is no earlier numbered list to continue", run: () => selection && runNodeEdit(() => doc.continueList(selection.focus.node)) },
    { id: "paragraph.indent.increase", label: "Increase indent", group: "Paragraph", kw: "", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => adjustIndentCommand(360) },
    { id: "paragraph.indent.decrease", label: "Decrease indent", group: "Paragraph", kw: "outdent", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => adjustIndentCommand(-360) },
    // Insert commands take their `enabled` from `insertCommandEnabled`, the same
    // predicate the Insert ribbon buttons use, so a command can never be live on
    // one surface and greyed on another. None of them asks for a prior click:
    // an open document already has an insertion point (Word/Docs), so the only
    // Insert precondition left is a real one — Link needs text to link.
    // HF-148 — the id used to BE the 3x3 default, label and all, so no caller
    // could ask for any other size. The grid picker remains what a user without
    // a size in mind gets; the argument is what a host or a keyboard caller
    // needs.
    {
      id: "insert.table",
      label: "Insert table…",
      group: "Insert",
      kw: "grid rows columns",
      enabled: insertCommandEnabled("insert.table"),
      run: () => insertTableBtn.click(),
    },
    // Shift+Enter was reachable ONLY from the keyboard — no id, so no palette
    // row, no menu row, and nothing in the shortcut reference. It is placed in
    // the palette here; a ribbon/menu home (Word files it under Insert ▸ Break)
    // is deliberately deferred, because the Insert ribbon and the Insert menu
    // are held at exact parity by `insert-surface.spec.mjs` and adding a control
    // to both is a chrome change, not this fix.
    { id: "insert.lineBreak", label: "Line break", group: "Insert", kw: "soft line break newline same paragraph shift enter", shortcut: "⇧⏎", enabled: !!selection && reviewMode !== "suggesting", disabledReason: reviewMode === "suggesting" ? "Line breaks cannot be tracked yet" : "Place the caret where the break belongs", run: () => void insertLineBreakAtSelection() },
    { id: "insert.link", label: "Add or edit link", group: "Insert", kw: "hyperlink url bookmark toc", shortcut: "⌘K", enabled: insertCommandEnabled("insert.link", context), disabledReason: "Select text to add a link", run: () => editSelectionLink() },
    { id: "layout.firstPageVariant", label: `Different first page: ${runningVariantState().firstPage ? "on" : "off"}`, group: "Layout", kw: "different first page header footer title page cover", enabled: !!doc, disabledReason: "Open a document first", run: () => toggleRunningVariant("firstPage") },
    { id: "layout.evenOddVariant", label: `Different odd & even pages: ${runningVariantState().evenOdd ? "on" : "off"}`, group: "Layout", kw: "different odd even pages header footer mirrored", enabled: !!doc, disabledReason: "Open a document first", run: () => toggleRunningVariant("evenOdd") },
    { id: "insert.footnote", label: "Footnote", group: "Insert", kw: "footnote note reference citation bottom of page", enabled: !!selection, disabledReason: "Place the caret where the note belongs", run: () => insertNote("footnote") },
    { id: "insert.endnote", label: "Endnote", group: "Insert", kw: "endnote note reference citation end of document", enabled: !!selection, disabledReason: "Place the caret where the note belongs", run: () => insertNote("endnote") },
    { id: "insert.header", label: "Edit header", group: "Insert", kw: "header running title page top margin", enabled: !!doc, disabledReason: "Open a document first", run: () => editRunningContent("header") },
    { id: "insert.footer", label: "Edit footer", group: "Insert", kw: "footer running page number bottom margin", enabled: !!doc, disabledReason: "Open a document first", run: () => editRunningContent("footer") },
    { id: "insert.bookmark", label: "Bookmark…", group: "Insert", kw: "bookmark manager navigate create rename delete go to", enabled: insertCommandEnabled("insert.bookmark"), run: () => openBookmarkManager() },
    { id: "insert.field", label: "Field…", group: "Insert", kw: "field placeholder page number of pages date time file name author auto update", enabled: insertCommandEnabled("insert.field"), run: () => openFieldDialog() },
    { id: "insert.image", label: "Picture…", group: "Insert", kw: "image picture insert photo file png jpeg jpg gif paste", enabled: insertCommandEnabled("insert.image"), run: () => insertImageFromFile() },
    { id: "insert.shape", label: "Shape…", group: "Insert", kw: "shape drawing autoshape rectangle rounded ellipse circle triangle diamond line arrow callout", enabled: insertCommandEnabled("insert.shape"), run: () => openShapeGallery() },
    { id: "insert.textbox", label: "Text box", group: "Insert", kw: "text box textbox callout caption floating frame", enabled: insertCommandEnabled("insert.textbox"), run: () => void insertTextBoxObject() },
    { id: "insert.symbol", label: "Symbol…", group: "Insert", kw: "symbol special character glyph currency math greek arrow fraction diacritic omega degree unicode", enabled: insertCommandEnabled("insert.symbol"), run: () => openSymbolPicker() },
    { id: "insert.emoji", label: "Emoji…", group: "Insert", kw: "emoji emoticon smiley face reaction sticker unicode", enabled: insertCommandEnabled("insert.emoji"), run: () => openEmojiPicker() },
    // The per-kind field shortcuts share Field…'s precondition; they are the
    // same dialog's choices reached directly from the palette.
    ...FIELD_KINDS.map((f) => ({
      id: `insert.field.${f.kind}`,
      label: `Insert field: ${f.label}`,
      group: "Insert",
      kw: `field ${f.kw}`,
      enabled: insertCommandEnabled("insert.field"),
      run: () => insertFieldAtCaret(f.kind),
    })),
    { id: "view.outline", label: "Toggle outline", group: "View", kw: "headings navigation", run: () => toggleOutline() },
    { id: "view.showChanges", label: "Show changes (read-only)", group: "View", kw: "tracked changes markup deletions insertions review redline", run: () => toggleShowChanges() },
    { id: "view.zoomIn", label: "Zoom in", group: "View", kw: "", run: () => stepZoom(1) },
    { id: "view.zoomOut", label: "Zoom out", group: "View", kw: "", run: () => stepZoom(-1) },
    // Ribbon density (docs/104 HF-094). The choice was already real and already
    // persisted, but the ONLY way to reach it was a 28px chevron at the right
    // end of the tab strip — so a user who wanted Docs-style compact chrome had
    // to find an unlabelled arrow. The label carries the current state, the same
    // shape `tools.smartQuotes` uses, so the palette and the View menu both read
    // as a switch rather than as an action with an unknown effect.
    { id: "view.compactRibbon", label: `Compact ribbon: ${ribbonViewCollapsed ? "on" : "off"}`, group: "View", kw: "compact ribbon collapse expand band density toolbar chrome docs word full", noDoc: true, run: () => setRibbonCollapsed(!ribbonViewCollapsed) },
    // Settings OPENS the panel; it does not synthesize a click on the gear
    // button. Clicking a control on the user's behalf inherits that control's
    // event semantics — the gear's handler stops propagation of ITS OWN click,
    // which says nothing about the menu row's click that is still bubbling —
    // and it also inherits the gear's toggle, so picking "Settings" from a menu
    // while the panel was open would have closed it. `noDoc` because theme,
    // accent and reviewer identity are host preferences that do not need a
    // document open, and the gear is the only other way to reach them.
    { id: "view.settings", label: "Settings", group: "View", kw: "theme accent dark appearance preferences identity author name initials", noDoc: true, run: () => toggleSettings(true) },
    { id: "layout.pageSetup", label: "Page setup", group: "Layout", kw: "margins orientation paper size", run: () => togglePageSetup(true) },
    { id: "layout.paragraph", label: "Paragraph properties", group: "Layout", kw: "spacing borders shading indent", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => toggleParagraphProperties(true) },
    // The Layout and References tabs' own rows, generated from the SAME tables
    // their buttons are built from, so a tab button and its palette row can
    // never disagree about whether the command is available or why it is not.
    // Rows without a `label` (the indent pair) already have a palette entry
    // above and only borrow the table's button wiring.
    ...LAYOUT_SURFACE.filter((entry) => entry.label).map((entry) => ({
      id: entry.command,
      label: entry.label,
      group: "Layout",
      kw: entry.kw,
      enabled: ribbonSurfaceEnabled(entry),
      disabledReason: ribbonSurfaceReason(entry),
      run: entry.run ?? (() => {}),
    })),
    ...REFERENCE_SURFACE.filter((entry) => entry.label).map((entry) => ({
      id: entry.command,
      label: entry.label,
      group: "References",
      kw: entry.kw,
      enabled: ribbonSurfaceEnabled(entry),
      disabledReason: ribbonSurfaceReason(entry),
      run: entry.run ?? (() => {}),
    })),
    { id: "help.commands", label: "Find a command…", group: "Help", kw: "help command palette search run", shortcut: "⌘⇧P", noDoc: true, run: () => openCmd() },
    { id: "help.shortcuts", label: "Keyboard shortcuts", group: "Help", kw: "help shortcuts keys chords reference cheat sheet", noDoc: true, run: () => toggleShortcutsReference(true) },
    { id: "help.about", label: "About OpenDoc", group: "Help", kw: "about version licence license apache build source repository issue report credits", noDoc: true, run: () => toggleAbout(true) },
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
    { id: "tools.smartQuotes", label: `Smart quotes: ${smartQuotesEnabled ? "on" : "off"}`, group: "Tools", kw: "smart curly typographic quotes apostrophe straight autocorrect", noDoc: true, run: () => setSmartQuotes(!smartQuotesEnabled) },
    // Reads as a switch, the shape `tools.smartQuotes` already uses, so the menu
    // row and the palette row both say which state it is in rather than being an
    // action with an unknown effect. Never disabled: off has to be reversible
    // from the same place it was set (SKILL.md §10).
    { id: "tools.spellCheck", label: `Spell check: ${settings.spellCheck === false ? "off" : "on"}`, group: "Tools", kw: "spelling spell check squiggle dictionary misspelled proofing language red underline glossary", noDoc: true, run: () => setSpellCheckEnabled(settings.spellCheck === false) },
    { id: "tools.grammarCheck", label: `Grammar check: ${settings.grammarCheck === false ? "off" : "on"}`, group: "Tools", kw: "grammar check agreement doubled word article a an punctuation capitalisation capitalization proofing blue underline", noDoc: true, run: () => setGrammarCheckEnabled(settings.grammarCheck === false) },
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
    { id: "review.acceptAll", label: "Accept all changes", group: "Review", kw: "revision suggestion approve", run: () => void decideAllReviewChanges(true) },
    { id: "review.rejectAll", label: "Reject all changes", group: "Review", kw: "revision suggestion discard", run: () => void decideAllReviewChanges(false) },
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
  // Line spacing and the list-marker galleries were ribbon-only: they exist as
  // popover controls on the Home tab and had no command entry at all, so neither
  // the palette nor any app menu could reach them. Docs gives line spacing a
  // whole top-level submenu (Format ▸ Line & paragraph spacing) and both products
  // put the marker galleries behind a searchable menu path.
  //
  // The rows are generated FROM the popovers' own markup rather than restated
  // here: the presets carry their percentage and label, and each gallery cell
  // carries the spec it applies plus the assistive-technology name it already
  // needs. A preset or bullet style added to the markup therefore gets its
  // command for free, and none of these labels can drift from the control they
  // mirror. Both run the same functions the popovers run.
  if (doc) {
    for (const preset of spacingMenu?.querySelectorAll(".spacing-line") ?? []) {
      const percent = Number(preset.dataset.percent);
      cmds.push({
        id: `paragraph.spacing.${percent}`,
        label: `Line spacing: ${preset.textContent.trim()}`,
        group: "Paragraph",
        kw: "line spacing leading single double space",
        enabled: !!selection,
        disabledReason: "Place the caret in a paragraph",
        run: () => runToolbarEdit((a, o, e, f) => doc.setLineSpacing(a, o, e, f, percent), { paragraphLevel: true }),
      });
    }
    for (const [menu, noun] of [[bulletGalleryMenu, "Bullet style"], [numberGalleryMenu, "Numbering format"]]) {
      for (const cell of menu?.querySelectorAll(".list-gallery-cell") ?? []) {
        const spec = cell.dataset.spec;
        cmds.push({
          id: `paragraph.listFormat.${spec}`,
          label: `${noun}: ${cell.getAttribute("aria-label")}`,
          group: "Paragraph",
          kw: `list marker bullet numbering ${cell.getAttribute("aria-label")}`.toLowerCase(),
          enabled: !!selection,
          disabledReason: "Place the caret in a list",
          run: () => applyListFormatCommand(spec),
        });
      }
    }
  }
  // HF-149 / HF-150 — the nine zoom presets and the underline-style menu were
  // chrome with no ids, so the palette, the app menu and any host driving the
  // editor could step zoom but never ASK for 150%, and could turn underline on
  // but never make it wavy. Generated from the same markup and the same label
  // map the controls themselves use, so a preset or a style added there gets its
  // command for free and no label can drift from the control it mirrors.
  for (const preset of zoomMenu?.querySelectorAll(".zoom-preset") ?? []) {
    const factor = Number(preset.dataset.zoom);
    cmds.push({
      id: `view.zoom.${Math.round(factor * 100)}`,
      label: `Zoom: ${preset.querySelector(".menu-item-label")?.textContent?.trim() ?? `${Math.round(factor * 100)}%`}`,
      group: "View",
      kw: "zoom scale magnify percent",
      run: () => setZoom(factor),
    });
  }
  for (const fit of zoomMenu?.querySelectorAll(".zoom-fit") ?? []) {
    const mode = fit.dataset.zoomMode;
    cmds.push({
      id: `view.zoom.${mode === "fit-width" ? "fitWidth" : "fitPage"}`,
      label: `Zoom: ${fit.querySelector(".menu-item-label")?.textContent?.trim() ?? mode}`,
      group: "View",
      kw: "zoom fit width page whole",
      run: () => setZoomMode(mode),
    });
  }
  // The open-ended one: any percentage, not just the nine on the menu. With no
  // argument it focuses the field the user would have typed into anyway.
  cmds.push({
    id: "view.zoom",
    label: "Zoom to…",
    group: "View",
    kw: "zoom percent custom exact scale",
    run: (percent) => {
      const value = Number(percent);
      if (!Number.isFinite(value) || value <= 0) {
        zoomEl.focus();
        zoomEl.select();
        return;
      }
      setZoom(value / 100);
    },
  });
  // The exact values behind the two "…" commands above. Generated from the same
  // inventory the font menu renders and the same step table Grow/Shrink walks,
  // so "Font: Georgia" and "Font size: 14 pt" mean exactly what picking them off
  // the ribbon means, and a face added to the inventory gets its command free.
  if (selection) {
    for (const name of fontInventory()) {
      cmds.push({
        id: `format.family.${name}`,
        label: `Font: ${name}`,
        group: "Format",
        kw: `font typeface family ${name}`.toLowerCase(),
        run: () => applyFontFamily(name),
      });
    }
    for (const points of FONT_STEP_SIZES) {
      cmds.push({
        id: `format.size.${points}`,
        label: `Font size: ${points} pt`,
        group: "Format",
        kw: `font size point ${points}`,
        run: () => applyFontSize(points, { report: false }),
      });
    }
  }
  for (const [style, label] of UNDERLINE_STYLE_LABELS) {
    if (style === "none") continue; // "no underline" is `format.underline` off
    cmds.push({
      id: `format.underline.${style}`,
      label: `Underline: ${label}`,
      group: "Format",
      kw: `underline ${label} line style`.toLowerCase(),
      enabled: !!selection,
      disabledReason: "Place the caret or select text",
      run: () => applyUnderlineStyle(style),
    });
  }
  if (doc) {
    for (const name of doc.listStyles()) {
      cmds.push({
        id: `style.${name}`,
        label: `Style: ${name}`,
        group: "Style",
        kw: "paragraph heading",
        run: () => runToolbarEdit((s, o, e, f) => doc.setParagraphStyle(s, o, e, f, name), { paragraphLevel: true }),
      });
    }
  }
  // Table structure, when the caret is in a table. These commands existed only on
  // the contextual Table ribbon tab and the right-click menu, so searching the
  // palette for "insert row" or "merge cells" found nothing — the same
  // one-surface-only defect that left Picture off the Insert ribbon. Word reaches
  // every one of them from Tell Me; Docs files them under Format ▸ Table. Built
  // from the caret's own context so enablement and the "why not" reasons are the
  // menu's, not a second opinion, and flattened out of their submenus with the
  // parent's name kept ("Table: Insert row above") so the palette reads as a flat
  // searchable list. `surface` is checked because the context menu composes these
  // rows itself — it must not receive them twice.
  if (doc && context.surface !== "context" && selection && plainTableInfo(selection.anchor.node)) {
    cmds.push(
      ...flattenCommandTree(tableToolCommands(contextAt(selection.anchor)), (entry, trail) => ({
        label: tableCommandLabel(entry.id, entry.label, trail, context.surface),
        group: "Table",
        kw: `table ${trail} ${entry.label}`.toLowerCase(),
      })),
    );
  } else if (context.surface === "menu") {
    // The Table MENU must exist even when the caret is not in a table: an empty
    // popover says the editor cannot edit tables (UX-012). `command_taxonomy`
    // builds the greyed rows from the same label map the live rows use, and
    // `menu_taxonomy` fails if that map and the real command set disagree.
    cmds.push(
      ...tableMenuPlaceholders(doc ? "Place the caret in a table" : "Open a document first"),
    );
  }
  // Object commands, on the same terms. Everything a selected image, shape or
  // text box can do lived on the floating bar and (mostly) the right-click menu
  // and nowhere else: the palette had no object command at all, and "Object
  // properties" had exactly one route in the entire product. A capability
  // reachable from one surface is this repo's recurring defect, and tables were
  // already fixed by exactly the flattening above.
  if (objectSelection && objectSelection.mode === "selected") {
    // Flattened the same way as the table rows above, because some object
    // commands are submenus (Wrap text) and a palette entry whose `run` is a
    // submenu is a dead row.
    cmds.push(
      ...flattenCommandTree(buildObjectContextCommands(selectedObjectContext()), (entry, trail) => ({
        label: trail ? `Object: ${trail} ${entry.label}` : `Object: ${entry.label}`,
        group: "Object",
        kw: `object image picture shape text box ${trail} ${entry.label}`.toLowerCase(),
      })),
    );
  }
  // Reaching an object at all is a command too, and it is the one that has to
  // work with no object selected — otherwise every row above is unreachable
  // without a mouse.
  cmds.push(
    {
      id: "object.selectNext",
      label: "Select next object",
      group: "Object",
      kw: "object image picture shape text box select next navigate keyboard",
      enabled: documentHasObjects(),
      disabledReason: "This document has no images, shapes or text boxes",
      run: () => traverseObjects(1),
    },
    {
      id: "object.selectPrevious",
      label: "Select previous object",
      group: "Object",
      kw: "object image picture shape text box select previous navigate keyboard",
      enabled: documentHasObjects(),
      disabledReason: "This document has no images, shapes or text boxes",
      run: () => traverseObjects(-1),
    },
  );
  return cmds.filter((command) => doc || command.noDoc);
}

// ---- One navigation axis: the compact menu bar and the ribbon's File page ---
// The editor used to show TWO navigation systems at once — this menu bar and the
// ribbon tab strip — so a command's home was a guess between two places (`109`
// UX-014). Each chrome now has exactly one axis (docs/122):
//
//   compact mode  the menu bar IS the axis; File is a dropdown, as in Google
//                 Docs and Drive. `style.css` hides the bar in ribbon mode.
//   ribbon mode   the tab strip is the axis; File is its first tab and opens a
//                 PAGE, as in ONLYOFFICE (`app/view/Toolbar.js:182`, the only
//                 tab declared `haspanel: false`).
//
// Both read `FILE_SURFACE` and both render through `command_menu.mjs`, so the
// two File rosters cannot drift and neither can invent its own gating: the rows
// come from the same `editorCommands()` descriptors the palette and the context
// menu use.
const appMenuBar = document.getElementById("appMenuBar");
const appMenuPopover = document.getElementById("appMenuPopover");
const menuRegistry = () =>
  new Map(
    editorCommands({ surface: "menu", hasRange: hasRange() }).map((command) => [command.id, command]),
  );
const appMenu = createMenuBar({
  bar: appMenuBar,
  popover: appMenuPopover,
  sectionsFor: (name) => APP_MENU_SECTIONS[name] ?? [],
  registry: menuRegistry,
  formatShortcut,
});
const openAppMenu = (name, options) => appMenu.open(name, options);
const closeAppMenu = (options) => appMenu.close(options);

// The File page. Same rows, rendered as headed groups instead of a dropdown,
// because a full-window route has the room to say what a group is and a dropdown
// does not. Rebuilt on every open rather than kept in sync — the same reason the
// compact toolbar is — so a row can never hold a stale `enabled`.
function renderFilePage() {
  if (!filePageBody) return 0;
  renderFilePane({
    menuRegistry,
    exportRows: EXPORT_COMMANDS,
    formatInfoOf: formatInfo,
    closeFilePage,
    templates: DOCUMENT_TEMPLATES,
    templateThumbnailFor: templateThumbnail,
    onTemplate: async (id) => {
      closeFilePage();
      if (!(await confirmDiscardIfEdited())) return;
      await openBytes(templateBytes(id), templateName(id));
    },
    // What each panel's dialog does on open, so the pane shows the same thing.
    prepare: (pane) => {
      if (pane === "shortcuts") buildShortcutsReference();
      if (pane === "commands") {
        // The palette renders its rows from the live registry when it opens;
        // shown as a pane nothing had asked it to, so it came up as an empty
        // search box. Same call, and the query starts clean.
        const input = document.getElementById("cmdInput");
        if (input) input.value = "";
        renderCommands("");
        // You came to this pane to type a command name. Both entry points —
        // opening the File tab on this pane, and selecting it in the rail —
        // are moments the person chose it, so taking the keyboard cannot
        // interrupt anything they were doing.
        input?.focus();
      }
      if (pane === "about") toggleAbout.stampVersion?.();
      if (pane === "properties" && doc) {
        const current = JSON.parse(doc.documentProperties());
        for (const [key, input] of PROP_FIELDS) input.value = current[key] ?? "";
        reflectDocumentMetadata();
      }
    },
  });
  // The rail lists every File command, but three kinds of them are rendered as
  // a CATEGORY row that opens a pane instead of a row that runs: the six export
  // formats collapse into one `Export`, and the rows that used to open a dialog
  // over the page — Settings, Document properties, the Help rows, New — open
  // panes. The owner asked for exactly this — "basically in this view replace
  // dialogs with this space", and for export, "one export on left and right
  // list of them with icons, like in onlyoffice".
  //
  // `fileMenuSections()` itself is untouched, so the compact chrome's File
  // DROPDOWN — which has no pane to open — still runs all of them directly, and
  // each category row records the ids it stands in for so the two surfaces can
  // be proved to offer the same roster.
  const count = renderCommandRows(
    filePageBody,
    fileMenuSections(),
    menuRegistry(),
    {
      itemClass: "file-page-item",
      headingFor: (_ids, index) => FILE_SURFACE[index]?.heading,
      formatShortcut,
      rowFor: (id, ids) =>
        fileCategoryRowFor(id, ids, {
          itemClass: "file-page-item",
          onSelect: (pane) => {
            setFilePane(pane);
            renderFilePage();
          },
        }),
      onRun: (command) => {
        // Every File row returns to the document first, which is what a dropdown
        // does implicitly by closing and what ONLYOFFICE's page does through its
        // own "Back to Document". One rule, no exception list: Settings and the
        // Help rows open dialogs, and a modal over a full-window page is two
        // layers of chrome between the user and the document they came for.
        //
        // Through `closeFilePage` rather than `selectRibbonTab` alone, because
        // the row the click landed on is about to be hidden: without moving the
        // keyboard somewhere real first, focus falls to <body> and a dialog
        // opened from here has nowhere to hand it back to on Escape — the
        // HF-062 failure, where the editor looked frozen after a dialog closed.
        closeFilePage();
        command.run();
      },
    },
  );
  return count;
}


{
  const back = document.getElementById("filePageBack");
  if (back) onButton(back, () => closeFilePage());
}

function buildCommands() {
  return editorCommands({ surface: "palette" });
}

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
    shortcutGroups(editorCommands({ surface: "palette" }), navigationShortcuts(), formatShortcut),
  );
}

const shortcutsModal = shortcutsDialog
  ? registerModal(shortcutsDialog, {
      initialFocus: () => shortcutsClose,
      fallbackFocus: () => pagesEl,
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
shortcutsClose?.addEventListener("click", () => toggleShortcutsReference(false));

// ---- About -----------------------------------------------------------------
const toggleAbout = createAboutDialog(engineVersion, () => pagesEl);

// ---- Bookmark manager ------------------------------------------------------
// The dialog is `bookmark_manager.mjs`; what is left here is the two things
// that are not about bookmarks — the review-mode gate, which is about review
// mode, and the repaint, which is about pages.

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
  const revision = readRevision(res);
  res.free();
  noteDocumentEdited(revision);
  if (newCount !== pages.length) {
    await renderAll();
  } else {
    for (const i of dirty) repaintPage(i);
    drawSelection();
  }
  scheduleChromeRefresh({ stats: true, outline: true });
}

const bookmarkManager = createBookmarkManager({
  getDoc: () => doc,
  applyEdit: applyBookmarkEdit,
  mutationBlocked: bookmarkMutationBlocked,
  hasRange,
  selectionEndpoints: selEndpoints,
  navigateTo: (position) => navToPosition(position, false),
  status: (text, kind) => setStatus(text, kind),
  registerModal,
  fallbackFocus: () => pagesEl,
});

function openBookmarkManager() {
  bookmarkManager.open();
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
  // A null `selection` here means no document is open, not "click first" — an
  // open document always carries an insertion point. Defensive guard only; the
  // command is not reachable without a document.
  if (!doc || !selection) return;
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
  // Defensive only — see `insertFieldAtCaret`. The picture lands at the caret,
  // which on a document nobody has clicked into is the start of the body.
  if (!doc || !selection) return;
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

/** Insert ▸ Picture: opens a file picker and inserts the chosen image at the
 *  insertion point — the caret if the user placed one, otherwise the start of
 *  the body, exactly as Word and Google Docs do. */
/** Word's Insert ▸ Text Box. Inserts a floating box at the caret and enters it,
 *  so the user types where they just clicked instead of hunting for the box. */
async function insertTextBoxObject() {
  if (!doc || !selection) return;
  if (blockMutationInViewing()) return;
  if (reviewMode === "suggesting") {
    setStatus("Inserting a text box cannot be tracked yet; switch to Editing", "error");
    return;
  }
  const before = placedObjectIds();
  let result;
  try {
    result = doc.insertTextBox(selection.focus.node, selection.focus.offset);
  } catch (err) {
    setStatus(`Could not insert the text box: ${err.message ?? err}`, "error");
    return;
  }
  await applyEditResult(result);
  // Land INSIDE the box through the SAME path a double-click uses, so entry can
  // never diverge between the two ways of getting there.
  const box = newestObject(before, "textbox");
  if (box) {
    selectObject(box.node, box.kind, null, box.anchored, box);
    enterObjectEditMode();
  }
  setStatus("Text box added — type its text");
  focusEditorSurface();
}

/** The object that appeared since `before` (a set of node ids), optionally of a
 *  given kind. Diffing the placed-object order is how a freshly inserted object
 *  is identified: the engine reports the caret, not the object it created. */
function newestObject(before, kind) {
  let objects;
  try {
    objects = JSON.parse(doc.objectOrder());
  } catch {
    return null;
  }
  return (
    objects.find((entry) => !before.has(entry.node) && (!kind || entry.kind === kind)) ?? null
  );
}

/** The node ids of every placed object right now — the "before" side of the diff. */
function placedObjectIds() {
  try {
    return new Set(JSON.parse(doc.objectOrder()).map((entry) => entry.node));
  } catch {
    return new Set();
  }
}

/** Word's Insert ▸ Shapes. Inserts a floating preset shape at the caret and
 *  leaves it SELECTED (not entered) — a shape has no text to type. */
async function insertShapeObject(geometry) {
  if (!doc || !selection) return;
  if (blockMutationInViewing()) return;
  if (reviewMode === "suggesting") {
    setStatus("Inserting a shape cannot be tracked yet; switch to Editing", "error");
    return;
  }
  const before = placedObjectIds();
  let result;
  try {
    result = doc.insertShape(selection.focus.node, selection.focus.offset, geometry);
  } catch (err) {
    setStatus(`Could not insert the shape: ${err.message ?? err}`, "error");
    return;
  }
  await applyEditResult(result);
  // Word leaves a new shape SELECTED, not entered — a shape has no text — which
  // also puts Fill and Outline within reach straight away.
  const shape = newestObject(before, "shape");
  if (shape) selectObject(shape.node, shape.kind, null, shape.anchored, shape);
  setStatus(`${SHAPE_LABELS[geometry] ?? "Shape"} added`);
  focusEditorSurface();
}

function insertImageFromFile() {
  if (!doc || !selection) return;
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

function fieldChoiceButtons() {
  return fieldList ? [...fieldList.querySelectorAll(".field-choice")] : [];
}

const fieldModal = fieldDialog
  ? registerModal(fieldDialog, {
      initialFocus: () => fieldChoiceButtons()[0],
      fallbackFocus: () => pagesEl,
    })
  : null;

/** Opens the field picker against the current insertion point; the first choice
 *  is focused for keyboard use. Needs only an open document — Word's Insert ▸
 *  Quick Parts ▸ Field is never gated on having clicked into the page first. */
function openFieldDialog() {
  if (!doc || !fieldDialog) return;
  // Viewing is read-only: refuse before opening, as the symbol and emoji pickers
  // do, so the dialog never opens onto an insert that cannot happen. Promoting
  // Field to the ribbon is what makes this reachable without a deliberate detour
  // through the palette, and picking a field only to be told no is a worse
  // answer than not opening.
  if (blockMutationInViewing()) return;
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
  if (!doc || !glyph || !selection) return;
  await pasteText(glyph, "paste");
}

/** Builds a categorized glyph picker (symbol or emoji) over an existing dialog
 *  skeleton in the markup. Returns `{ open }`; the controller owns tab switching,
 *  keyword search, roving arrow-key grid navigation, insertion (keep-open), and
 *  Esc / backdrop / Done dismissal — mirroring the field dialog's focus-trap and
 *  return-focus contract. */

const glyphPickerHost = {
  insertGlyph: insertGlyphAtCaret,
  blockedInViewing: blockMutationInViewing,
  returnFocusToEditor: focusEditorSurface,
  isDocumentOpen: () => doc !== null,
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
  const entries = bookmarkManager.entries();
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
      fallbackFocus: () => pagesEl,
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
  if (!doc || !linkDialog) return;
  if (blockMutationInViewing() || blockUntrackedInSuggesting()) return;
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
  linkDialog.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && event.target === linkPlaceSelect) {
      // Enter on the native select would not submit the form; apply explicitly.
      event.preventDefault();
      void applyLinkDialog();
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
    cmdInput.setAttribute("aria-expanded", "false");
    cmdInput.removeAttribute("aria-activedescendant");
    return;
  }
  cmdInput.setAttribute("aria-expanded", "true");
  cmdMatches.forEach((c, i) => {
    const item = document.createElement("button");
    item.type = "button";
    item.className = `cmd-item${i === cmdSel ? " sel" : ""}`;
    item.setAttribute("role", "option");
    // A stable id per option is what `aria-activedescendant` points at; the
    // option itself must stay out of the tab order because focus never leaves
    // the query input.
    item.id = `cmdOption-${i}`;
    // The command id, on the row, for the same reason the context menu carries
    // it (`data-command-id`) and the ribbon buttons carry `data-command`: it
    // makes "is this capability on more than one surface?" a question the DOM can
    // answer, so a parity guard compares id sets instead of matching labels that
    // are context sensitive and carry their shortcut in the same box.
    item.dataset.commandId = c.id;
    item.tabIndex = -1;
    item.setAttribute("aria-selected", String(i === cmdSel));
    item.disabled = c.enabled === false;
    if (c.disabledReason) item.title = c.disabledReason;
    // The hint column shows the disabled reason when unavailable, else the
    // command's keyboard shortcut when it has one (so the palette teaches the
    // shortcut), else its group.
    const hint = c.enabled === false ? c.disabledReason : (formatShortcut(c.shortcut) || c.group);
    // The shortcut, separately from the hint TEXT. The hint box holds a chord, a
    // group name or a refusal reason depending on state, so a test reading it
    // cannot tell "has a chord" from "has a group" — and a reachability guard
    // that counts a group name as a keyboard surface passes for every command
    // there is, which is the green-but-worthless guard this repo keeps catching.
    if (c.shortcut) item.dataset.commandShortcut = formatShortcut(c.shortcut);
    item.innerHTML = `<span>${escapeHtml(c.label)}</span><span class="cmd-hint">${escapeHtml(hint)}</span>`;
    item.addEventListener("mousemove", () => setCmdSel(i));
    item.addEventListener("click", () => runCommand(i));
    cmdList.appendChild(item);
  });
  reflectCmdSel();
}

function setCmdSel(i) {
  if (i < 0 || cmdMatches[i]?.enabled === false) return;
  cmdSel = i;
  reflectCmdSel();
}

/** Publishes the current selection to both channels: the `.sel` class the eye
 *  reads, and the `aria-selected` / `aria-activedescendant` pair a screen reader
 *  reads. Arrowing used to move only the class, so the palette said nothing
 *  while the highlight travelled and Enter ran whatever happened to be under
 *  it. The option's own text carries the hint column, so a disabled command
 *  announces its reason with its name. */
function reflectCmdSel() {
  const items = cmdList.querySelectorAll(".cmd-item");
  items.forEach((el, k) => {
    el.classList.toggle("sel", k === cmdSel);
    el.setAttribute("aria-selected", String(k === cmdSel));
  });
  const active = items[cmdSel];
  if (active) {
    cmdInput.setAttribute("aria-activedescendant", active.id);
    active.scrollIntoView({ block: "nearest" });
  } else {
    cmdInput.removeAttribute("aria-activedescendant");
  }
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
  // The palette has two faces: the modal, and the File page's "Find a command"
  // PANE. `closeCmd` closes the modal; from the pane there is none to close,
  // and the command ran with the page still covering the document. Every other
  // route into that page returns to the document first. No-op when closed.
  closeFilePage();
  cmd.run();
}

// The palette is modal too — it takes the keyboard and dims the app behind it —
// so it joins the same stack rather than keeping a fourth dismissal contract.
// It brings its own Arrow/Enter handling on #cmdInput; the primitive only owns
// Escape, Tab containment and the shortcut lock.
const cmdModal = registerModal(cmdPalette, {
  initialFocus: () => cmdInput,
  fallbackFocus: () => pagesEl,
  // A shortcut that opens a surface must also close it; the lock would
  // otherwise swallow the second ⌘⇧P and strand the palette open.
  toggleChord: (event) => (event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p",
  onClose: () => searchTrigger?.setAttribute("aria-expanded", "false"),
});

function openCmd() {
  closeAppMenu();
  cmdInput.value = "";
  renderCommands("");
  searchTrigger?.setAttribute("aria-expanded", "true");
  cmdModal.open();
}
function closeCmd() {
  cmdModal.close();
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
  }
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
    cmdModal.isOpen ? closeCmd() : openCmd();
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
    printDocument(doc);
  }
});
// Visible entry point for the palette (doc 69 §1.4.1): the shortcut already
// worked, it just had no on-screen affordance to discover it.
searchTrigger?.addEventListener("click", () => openCmd());

// ---- Find / replace ---------------------------------------------------------
const FIND_SCAN_CAP = 5000;

function setFindStatus(text, miss = false) {
  findStatus.textContent = text;
  findStatus.classList.toggle("miss", miss);
}

const findParagraphTextCache = new Map();

function paragraphTextForFind(node) {
  if (!findParagraphTextCache.has(node)) {
    const length = doc.paragraphLength(node);
    findParagraphTextCache.set(node, doc.copyText(node, 0, node, length));
  }
  return findParagraphTextCache.get(node);
}

/** Whether an engine match stands alone as a word — the "Whole word" option.
 *  The paragraph text comes from the engine; the boundary rule itself is
 *  `isWholeWordAt`, which is pure and unit-tested. */
function isWholeWordMatch(match) {
  const text = paragraphTextForFind(match.startNode);
  return isWholeWordAt(
    text,
    byteOffsetToStringIndex(text, match.startOffset),
    byteOffsetToStringIndex(text, match.endOffset),
  );
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

/** Scans every match in document order, starting from the top, up to
 * FIND_SCAN_CAP — bounded like every other pagination/parse loop in this
 * codebase, since findText wraps around and would otherwise loop forever.
 * Once every match has been visited once, the engine's wrap fallback can
 * re-surface *any* earlier match (not necessarily the first one), so
 * termination checks membership in every key seen so far, not just the
 * first — comparing only to the first match's key under-counts (it can
 * oscillate between two later matches forever without ever revisiting the
 * exact first one). */
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
    if ((!wholeWord || isWholeWordMatch(candidate)) && matchInFindSelection(candidate)) {
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
  // An untouched load-time caret is not a place the user searched from, so it
  // counts as "no position" here: a document whose first characters are the
  // query still reports "N matches", not "1 of N".
  const idx = selection && !caretIsImplicit()
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
  // Same rule as `updateFindStatus`: the untouched load-time caret means "start
  // from the top", so Find Next cannot skip a match sitting at the document
  // start. Selecting the hit moves the caret off the seed, making it explicit.
  const current = selection && !caretIsImplicit()
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
    // No modal check here on purpose: the primitive's keydown lock refuses every
    // application chord while a dialog is open, so Find can no longer open
    // behind one (docs/104 HF-063). Guarding each shortcut individually is the
    // enumeration this change exists to delete.
    e.preventDefault();
    openFind();
  }
});

// Indentation: left/right absolute, and a first-line/hanging "special" indent
// (setFirstLineIndent encodes hanging as a negative value, 0 clears both).
indentLeftInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setLeftIndent(a, b, c, d, inchTwips(indentLeftInput)), { paragraphLevel: true }),
);
indentRightInput.addEventListener("change", () =>
  runToolbarEdit((a, b, c, d) => doc.setRightIndent(a, b, c, d, inchTwips(indentRightInput)), { paragraphLevel: true }),
);
function applyIndentSpecial() {
  const by = inchTwips(indentSpecialByInput);
  const kind = indentSpecialSel.value;
  const twips = kind === "first" ? by : kind === "hanging" ? -by : 0;
  runToolbarEdit((a, b, c, d) => doc.setFirstLineIndent(a, b, c, d, twips), { paragraphLevel: true });
}
indentSpecialSel.addEventListener("change", applyIndentSpecial);
indentSpecialByInput.addEventListener("change", applyIndentSpecial);

paraShade.addEventListener("change", () => {
  const [r, g, b] = hexToRgb(paraShade.value);
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, r, g, b, false), { paragraphLevel: true });
});
onButton(paraShadeNone, () =>
  runToolbarEdit((a, x, c, d) => doc.setParagraphShading(a, x, c, d, 0, 0, 0, true), { paragraphLevel: true }),
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
    markDocumentSaved();
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
saveBtn?.addEventListener("click", saveDocument);

/** Moves the caret to an engine Caret result (document bounds). Shift extends. */
function navToPosition(caret, extend) {
  breakTypingSession();
  pendingFormat = null; // caret moved → disarm typing format
  const to = { node: caret.node, offset: caret.offset };
  verticalGoal.clear();
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
  const column = verticalGoal.columnFor(dir, selection.focus, () => x) ?? x;
  const targetX = pageRect.left + column * sx + Math.max(1, (w * sx) / 2);
  const targetY = pageRect.top + y * sy + (dir === "pageUp" ? -distance : distance);
  const page = pageFromClientPoint(targetX, targetY);
  if (!page) return;
  const to = anchorAt(page, clientPointEvent(targetX, targetY));
  if (!to) return;
  verticalGoal.keep(column, to);

  selection = extend ? { anchor: selection.anchor, focus: to } : { anchor: to, focus: to };
  drawSelection();
  focusEditorSurface();
  scrollCaretIntoView();
}

/** Selects the active cell first, then the whole document on a repeated ⌘A. */
function selectAll() {
  if (!doc) return;
  breakTypingSession();

  if (selection && doc.inTable(selection.focus.node)) {
    const range = doc.cellTextRange(selection.focus.node);
    try {
      if (!range.found) {
        setStatus(
          "Select All is unavailable because this cell crosses another editing surface",
          "error",
        );
        return;
      }
      const start = { node: range.startNode, offset: range.startOffset };
      const end = { node: range.endNode, offset: range.endOffset };
      if (!selectionMatchesRange(selection, start, end)) {
        tableSelection = null;
        selection = { anchor: start, focus: end };
        drawSelection();
        focusEditorSurface();
        setStatus("Cell contents selected — choose Select All again to select the document");
        return;
      }
    } finally {
      range.free();
    }
  }

  const a = doc.firstPosition();
  const b = doc.lastPosition();
  tableSelection = null;
  selection = {
    anchor: { node: a.node, offset: a.offset },
    focus: { node: b.node, offset: b.offset },
  };
  a.free();
  b.free();
  drawSelection();
  focusEditorSurface();
  setStatus("Document selected");
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
  let node = selection.focus.node;
  let offset = selection.focus.offset;
  for (const run of insertable) {
    const applied = await runEdit(
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
    // A refused run has already reported itself; continuing would paste the
    // remainder at an offset the document never reached.
    if (!applied) break;
    // Never synthesize the next insertion point. The engine's offsets are UTF-8
    // bytes and `run.text.length` is UTF-16 code units, so `offset += length`
    // drifted on every curly quote, em dash or accent — the rest of the paste
    // then landed inside an earlier run, or was refused outright. The caret in
    // the EditResult `applyEditResult` just installed is the engine's own answer
    // for where that run ended, so read it back instead (docs/104 T-09).
    node = selection.focus.node;
    offset = selection.focus.offset;
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
    try { plain = await navigator.clipboard.readText(); } catch { /* html-only or image-only clipboard */ }
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
      // Then an image. ⌘V is intercepted as a keydown, so this async path — not
      // the native ClipboardEvent one — is what a screenshot paste actually
      // reaches, and it had no image branch at all: ⌘V over a copied image did
      // nothing and said nothing (docs/104 HF-059). HTML is tried first because
      // a fragment copied from a web page carries BOTH an image and its markup,
      // and Word and Docs paste the markup in that case; a screenshot carries
      // only the bitmap. The insertable-format decision stays where it already
      // lives, in `insertImageFromBlob`, so both paste paths refuse alike.
      for (const item of items) {
        const imageType = item.types.find((type) => type.startsWith("image/"));
        if (!imageType) continue;
        await insertImageFromBlob(await item.getType(imageType));
        return;
      }
    }
    if (plain) {
      await pasteText(plain);
      return;
    }
    // Nothing on the clipboard this editor can place. Saying so is the point:
    // a command that looks live and quietly does nothing is indistinguishable
    // from a broken editor.
    setStatus("There is nothing on the clipboard to paste here", "error");
  } catch (err) {
    console.warn("paste failed:", err);
    setStatus("Clipboard paste was blocked by the browser", "error");
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
    if (text) {
      await pasteText(text);
      return;
    }
    // ⌘⇧V over an image-only clipboard has nothing to keep the text of. Say so
    // rather than returning silently (docs/104 HF-059). The status kind is
    // "error" — the two clipboard failures here used to pass "err", which is not
    // a class the stylesheet defines, so they were reported in the plain style.
    setStatus("There is no text on the clipboard to paste", "error");
  } catch (err) {
    console.warn("paste text failed:", err);
    setStatus("Clipboard paste was blocked by the browser", "error");
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
// The document revision the chip's own paste produced. Both chip actions work by
// undoing that paste and redoing it differently, so they are only safe while the
// paste is still the change an undo would remove. Captured when the chip is
// offered; `pasteOptionsStillOnTop` is the fail-closed check.
let pasteOptionsRevision = null;
// The rich-run JSON of the most recent paste, captured by `pasteRichRunsJson`
// and drained by `offerPasteOptions` for the "Merge formatting" option.
let lastRichPasteJson = null;

/** Whether the chip is currently offered. Read from `applyEditResult`, which is
 *  defined earlier in the file but only ever runs from an event or `boot()`,
 *  by which time this section has evaluated. */
function pasteOptionsShowing() {
  return !pasteOptionsEl.hidden;
}

function hidePasteOptions() {
  pasteOptionsEl.hidden = true;
  pasteOptionsPlain = null;
  pasteOptionsRuns = null;
  pasteOptionsRevision = null;
}

/** True while the chip's paste is still the change an undo would remove. An
 *  unreadable revision counts as "moved on": the chip may not gamble with the
 *  user's previous edit on a number it could not read. */
function pasteOptionsStillOnTop() {
  return (
    pasteOptionsRevision !== null && !revisionUnreadable && currentRevision === pasteOptionsRevision
  );
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
  pasteOptionsRevision = revisionUnreadable ? null : currentRevision;
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
  const onTop = pasteOptionsStillOnTop();
  hidePasteOptions();
  if (!text || !doc) return;
  if (!onTop) return refusePasteOptions();
  if (doc.canUndo) await runEdit(() => doc.undo());
  await pasteText(text);
  focusEditorSurface();
}
/** Says no rather than undoing whatever is on top of the stack instead. */
function refusePasteOptions() {
  setStatus("The paste is no longer the most recent change; paste options can't be applied", "error");
  focusEditorSurface();
}

/** "Merge formatting": undo the rich paste and re-insert it with each run's
 *  properties reduced to the emphasis flags only (bold/italic/underline/
 *  strike/vertAlign). Dropping font, size, color, and highlight lets the text
 *  inherit the destination paragraph's formatting — the Word/Docs behavior. */
async function switchPasteToMergeFormatting() {
  const runsJson = pasteOptionsRuns;
  const onTop = pasteOptionsStillOnTop();
  hidePasteOptions();
  if (!runsJson || !doc) return;
  if (!onTop) return refusePasteOptions();
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
  const editingKey = [...e.key].length === 1
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
// `#pages` stays in the tab order (it is the skip-link target and the document's
// landmark), but it cannot accept text. Hand focus to the proxy when it lands
// there, so reaching the document by keyboard gives a real text-input surface.
pagesEl.addEventListener("focus", () => {
  if (modalIsOpen()) return;
  if (!editorTextInputEl) return;
  if (document.activeElement === editorTextInputEl) return;
  editorTextInputEl.focus({ preventScroll: true });
  positionEditorTextInput();
});

// ---- Soft-keyboard / dictation text entry (docs/105 UX-001) ----------------
// The keydown path below handles hardware keyboards, and it is still the only
// thing that inserts a character there. It cannot carry touch input: Android
// and iOS soft keyboards deliver printable characters as `keyCode 229` /
// `key: "Unidentified"`, and swipe-typing, dictation and autocorrect emit no
// usable keydown at all. `beforeinput` is the event those surfaces do fire.
//
// Deliberately narrow, so this does NOT become a second, divergent apply path —
// the defect class HF-007 recorded when four copy-pasted edit paths drifted:
//   * insertion reuses `pasteText(..., "typing")`, the exact primitive
//     `commitComposedText` already uses, so review mode, selection replacement
//     and history coalescing behave identically to an IME commit;
//   * DELETION is intentionally left to keydown. Backspace and Delete are real
//     named keys that soft keyboards do report, so there is no gap to close and
//     every reason not to open a second delete path.
// If you are here to add `deleteContentBackward`, first prove keydown misses it.
document.addEventListener("beforeinput", async (e) => {
  if (!editorTextInputEvent(e)) return;
  // A composition in flight owns its own text; `compositionend` commits it.
  if (composingText) return;
  const type = e.inputType;
  if (type === "insertText" || type === "insertFromPaste" || type === "insertReplacementText") {
    const data = e.data ?? "";
    if (!data) return;
    e.preventDefault();
    pendingFormat = null;
    await pasteText(data, "typing");
    return;
  }
  if (type === "insertLineBreak" || type === "insertParagraph") {
    e.preventDefault();
    await pasteText("\n", "typing");
  }
});

// The proxy must never accumulate text: the engine is the source of truth, and
// a textarea holding a stale value would resend it on the next composition.
if (editorTextInputEl) {
  editorTextInputEl.addEventListener("input", () => {
    if (composingText) return; // clearing mid-composition cancels the IME
    editorTextInputEl.value = "";
  });
}

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
  // `preventDefault` on compositionend does not reliably stop the browser from
  // leaving the composed text in the textarea, so clear it explicitly.
  if (editorTextInputEl) editorTextInputEl.value = "";
  await commitComposedText(e.data || "");
});

// ---- Smart quotes -----------------------------------------------------------
// Word and Docs both replace the typewriter quotes as you type: `"` becomes “ or
// ” and `'` becomes ‘ or ’, chosen from what precedes the caret. Documents from
// either product therefore arrive full of curly quotes, and typing into one used
// to introduce straight quotes beside them — visibly different glyphs in the same
// sentence.
//
// The decision is purely local: a quote OPENS at the start of a paragraph or
// after whitespace or an opening bracket, and CLOSES otherwise. Closing is the
// right default for the ambiguous case because that is what makes "don't" and
// "it's" correct, which is far more common in prose than a leading elision.
//
// Read through `copyText` of the single position before the caret rather than
// any cached text, so it is the engine's own content that decides — including
// after an undo, a paste, or a caret move the editor did not originate.
const SMART_QUOTE_PREF = "opendoc.smartQuotes";
let smartQuotesEnabled = readPref(SMART_QUOTE_PREF) !== "off";

function setSmartQuotes(enabled) {
  smartQuotesEnabled = enabled;
  writePref(SMART_QUOTE_PREF, enabled ? "on" : "off");
  setStatus(enabled ? "Smart quotes on" : "Smart quotes off");
  // Both proofing switches now have a Review-band face, and a switch that does
  // not move when you flip it is worse than no switch. `updateToolbar` is this
  // file's one "state changed, re-reflect every surface" call, so the reflection
  // stays in one place rather than gaining a second copy here.
  updateToolbar();
}

/** Spelling on or off, remembered with the other preferences. */
function setSpellCheckEnabled(enabled) {
  settings.spellCheck = enabled;
  saveSettings();
  if (spellCheckToggle) spellCheckToggle.checked = enabled;
  spellChecker.setEnabled(enabled);
  setStatus(enabled ? "Spell check on" : "Spell check off");
  updateToolbar();
}

/** Grammar on or off, remembered beside spelling and independent of it. */
function setGrammarCheckEnabled(enabled) {
  settings.grammarCheck = enabled;
  saveSettings();
  if (grammarCheckToggle) grammarCheckToggle.checked = enabled;
  spellChecker.refresh();
  drawSelection();
  setStatus(enabled ? "Grammar check on" : "Grammar check off");
  updateToolbar();
}

/** Adds a word and SAYS whether it was stored. A personal dictionary that
 *  silently failed to persist is indistinguishable from one that worked until
 *  the next reload, and there is no management surface yet to notice with. */
async function addWordToDictionary(word) {
  const stored = await spellChecker.addToDictionary(word);
  setStatus(
    stored
      ? `Added “${word}” to your dictionary`
      : `“${word}” is accepted for this session — your dictionary could not be saved`,
    stored ? "" : "error",
  );
}

/** The character to actually insert for `key` at `node`/`offset`. Returns `key`
 *  unchanged for everything that is not a straight quote, and whenever the
 *  preference is off — so the user can always type a literal quote for code. */
function smartQuoteFor(key, node, offset) {
  if (!smartQuotesEnabled || (key !== '"' && key !== "'")) return key;
  if (!doc || !node) return key;
  // Offset 0 is the start of the paragraph: nothing precedes, so it opens.
  let previous = "";
  if (offset > 0) {
    try {
      // Read the whole prefix and take its last code point. `offset` is an
      // engine offset — a UTF-8 BYTE index — so `offset - 1` is only the
      // preceding character when that character is ASCII. After "Müller",
      // "café" or any Cyrillic/CJK word it lands INSIDE a multi-byte character;
      // the engine's clamp then snaps it forward past `offset`, `copyText`
      // returns "", and an empty prefix reads as start-of-paragraph — so every
      // apostrophe typed after a non-ASCII letter came out as an opening quote
      // (docs/104 HF-055). Never synthesize an engine offset in JS: `offset`
      // itself is the caret the last EditResult reported, and 0 is a boundary by
      // definition, so those are the only two this can name.
      const prefix = doc.copyText(node, 0, node, offset);
      previous = [...prefix].at(-1) ?? "";
    } catch {
      return key; // an engine that cannot read the position gets the literal key
    }
  }
  return smartQuoteChar(key, previous);
}

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
  // Esc leaves header/footer editing first: while that context is open it is the
  // thing Esc most obviously means, and Word closes the header on Esc too.
  if (runningEditBand && key === "Escape") {
    e.preventDefault();
    exitRunningEdit();
    return;
  }
  if (objectSelection) {
    if (key === "Escape") {
      e.preventDefault();
      if (objectSelection.mode === "editing") {
        objectSelection = { ...objectSelection, mode: "selected" };
      } else if (!climbOutOfGroup()) {
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
      // Tab / Shift+Tab move to the next / previous object (docs/85 §8d, Q1).
      // Word cycles floating objects with Tab once one is selected, and Word for
      // the web moves between graphics the same way — so the gesture belongs to
      // object-selected mode. From a text caret Tab keeps its indent /
      // list-demote / next-cell meaning, which the handler further down still
      // owns. This is the only way to reach an object without a pointer: nothing
      // else selects one, so a keyboard user could not reach an image or text
      // box at all.
      if (key === "Tab" && !mod) {
        e.preventDefault();
        traverseObjects(e.shiftKey ? -1 : 1);
        return;
      }
      if (key === "Delete" || key === "Backspace") {
        e.preventDefault();
        if (objectSelection.canDelete) {
          deleteSelectedObject(); // one undoable delete; gated in Viewing/Suggesting
        } else {
          setStatus("This nested object cannot be deleted separately yet", "error");
        }
        return;
      }
      // Arrow keys nudge a FLOATING object's position (Word/Docs); Shift takes a
      // larger step. Only anchored objects have a position — an inline image has
      // none, so its arrows still fall through to move the caret off it.
      const nudge = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[key];
      if (nudge && objectSelection.canMove && !mod) {
        e.preventDefault();
        nudgeSelectedObject(nudge[0], nudge[1], e.shiftKey);
        return;
      }
      // Swallow text-producing keys; navigation/modifier combos fall through so
      // the user can still move the caret off the object. Code points, not UTF-16
      // units, so an emoji is swallowed here too rather than leaking through.
      if ([...key].length === 1 && !mod) {
        e.preventDefault();
        return;
      }
    }
  }

  // Space ticks the form checkbox the caret is in, as in Word — the keyboard
  // half of the click in `onPointerDown` (`docs/118` §2). Collapsed caret and
  // no modifier only: Ctrl+Space below is Clear Formatting, and a Space over a
  // selection is a replacement, not a toggle.
  if (key === " " && !e.ctrlKey && !e.metaKey && !e.altKey && !hasRange() && selection) {
    if (toggleFormCheckboxAt(selection.focus.node, selection.focus.offset)) {
      e.preventDefault();
      return;
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
      await runToolbarEdit((a, b, c, d) => doc.adjustIndent(a, b, c, d, e.shiftKey ? -360 : 360), { paragraphLevel: true });
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
      } else if (start.node !== end.node) {
        await suggestAcrossParagraphs(start, end, null);
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
      } else if (start.node !== end.node) {
        // A no-op (the caret is already at the line boundary) is swallowed; a real
        // cross-paragraph span is now a tracked suggestion.
        await suggestAcrossParagraphs(start, end, null);
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
            : doc.toggleList(a, b, c, d, listKind), { paragraphLevel: true });
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
      // Backspace at a paragraph start suggests deleting the PREVIOUS paragraph's
      // mark: that mark is the break being removed.
      if (!range && start.node !== end.node) {
        try {
          await runEdit(() => doc.suggestDeleteParagraphMark(start.node, undefined, new Date().toISOString()));
        } catch (error) {
          setStatus(String(error?.message ?? error), "error");
        }
        return;
      }
      if (start.node !== end.node) await suggestAcrossParagraphs(start, end, null);
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
      // Delete at a paragraph end suggests deleting THIS paragraph's mark.
      if (!range && start.node !== end.node) {
        try {
          await runEdit(() => doc.suggestDeleteParagraphMark(start.node, undefined, new Date().toISOString()));
        } catch (error) {
          setStatus(String(error?.message ?? error), "error");
        }
        return;
      }
      if (start.node !== end.node) await suggestAcrossParagraphs(start, end, null);
      return;
    }
    await runEdit(() => range ? doc.deleteSelection(anchor.node, anchor.offset, focus.node, focus.offset) : doc.deleteForward(focus.node, focus.offset));
    return;
  }
  if (key === "Enter") {
    e.preventDefault();
    // Shift+Enter is a SOFT line break: the text stays in one paragraph and so
    // keeps its list membership, style, numbering and spacing. It used to fall
    // through to the paragraph split below, which in a bulleted list produced a
    // second bullet and at the end of a heading dropped the next line into body
    // text — the opposite of what the gesture means in Word and Docs.
    //
    // Checked before the Suggesting gate and before the empty-list-item rule,
    // because neither is about this gesture: a line break inside an empty list
    // item must not exit the list.
    if (e.shiftKey && !mod) {
      await insertLineBreakAtSelection();
      return;
    }
    if (reviewMode === "suggesting") {
      // A tracked Enter. Over a selection the range goes first, as its own
      // suggestion, then the break — the order Word writes them in.
      if (range) {
        const [s, e] = orderedSelectionEnds(selection, doc);
        const ok = s.node === e.node
          ? await runEdit(() => doc.suggestDelete(s.node, s.offset, e.offset, undefined, new Date().toISOString())).then(() => true)
          : await suggestAcrossParagraphs(s, e, null);
        if (!ok) return;
      }
      const at = selection?.focus ?? focus;
      try {
        await runEdit(() => doc.suggestSplit(at.node, at.offset, undefined, new Date().toISOString()));
      } catch (error) {
        setStatus(String(error?.message ?? error), "error");
      }
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
            : doc.toggleList(a, b, c, d, listKind), { paragraphLevel: true });
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
  // A printable character (single key, no modifiers). Counted in CODE POINTS,
  // not UTF-16 units: an emoji arrives as a surrogate pair, so `key.length === 1`
  // silently discarded every astral character the user typed or an IME committed
  // — while the same character pasted or inserted from a picker went in fine.
  // Named keys ("Enter", "F1", "Tab") are still longer than one code point.
  if ([...key].length === 1) {
    e.preventDefault();
    const session = typingSessionForKey();
    // Straight quotes become typographic ones, decided from what precedes the
    // insertion point — the start of the replaced range when there is a
    // selection, since that is what the quote will actually follow.
    const typed = smartQuoteFor(
      key,
      range ? (anchor.offset <= focus.offset ? anchor.node : focus.node) : focus.node,
      range ? Math.min(anchor.offset, focus.offset) : focus.offset,
    );
    if (range) {
      pendingFormat = null; // typing over a selection uses the selection's own runs
      if (reviewMode === "suggesting" && anchor.node !== focus.node) {
        const [s, e] = orderedSelectionEnds(selection, doc);
        await suggestAcrossParagraphs(s, e, typed);
        return;
      }
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestReplace(
            anchor.node,
            Math.min(anchor.offset, focus.offset),
            Math.max(anchor.offset, focus.offset),
            typed,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(anchor.node, anchor.offset, focus.node, focus.offset, typed, session),
        { typing: true },
      );
    } else if (pendingFormat) {
      const pf = pendingFormat; // armed format persists across consecutive typing
      if (
        reviewMode === "suggesting" &&
        (pf.underlineStyle != null || pf.underlineColor != null)
      ) {
        setStatus(
          "Underline style and color are not tracked yet; switch to Editing to type with them",
          "error",
        );
        return;
      }
      await runEdit(
        () => reviewMode === "suggesting"
          ? doc.suggestStyledInsert(
            focus.node,
            focus.offset,
            typed,
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
          : (pf.underlineStyle != null || pf.underlineColor != null)
            ? doc.typeStyledTextWithUnderline(
                focus.node,
                focus.offset,
                typed,
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
                pf.underlineStyle,
                pf.underlineColor,
              )
            : doc.typeStyledText(
                focus.node,
                focus.offset,
                typed,
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
            typed,
            undefined,
            new Date().toISOString(),
            session,
          )
          : doc.typeText(focus.node, focus.offset, focus.node, focus.offset, typed, session),
        { typing: true },
      );
    }
  }
});

/** The viewer's package-admission limit, mirroring `max_input_bytes` in
 *  `casual-doc-wasm` (64 MiB). Mirrored rather than imported because the engine
 *  applies it inside `open(bytes)`, i.e. only once the whole file has already
 *  been read; this is the pre-check that lets the editor say no first. */
const MAX_OPEN_BYTES = 64 * 1024 * 1024;

/** The one entry point for "open these bytes as the document" — the picker and
 *  the viewport drop both land here, which is why the unsaved-work question is
 *  asked here and nowhere else (docs/104 HF-002). Two paths asking the same
 *  question two ways is how one of them ends up not asking it. */
async function handleFile(file) {
  if (!file) return;
  // The WASM `open` auto-detects any registered format from the bytes, so accept
  // every format the picker offers (DOCX, ODT, RTF, normalized JSON, plain text) —
  // the extension is only a friendly pre-filter; detection is authoritative.
  if (!/\.(docx|odt|rtf|json|txt)$/.test(file.name.toLowerCase())) {
    setStatus("Please choose a .docx, .odt, .rtf, .json, or .txt file", "error");
    return;
  }
  // The open document lives in the wasm heap and nowhere else, so replacing it
  // is unrecoverable. Ask before, not after.
  // The engine refuses anything over its package-admission limit, but it only
  // gets to refuse AFTER the whole file has been read into memory. Checking the
  // size the picker already told us is both faster and a message the user can
  // act on ("this file is too big"), instead of a decode failure deep in open().
  if (file.size > MAX_OPEN_BYTES) {
    setStatus(
      `${file.name} is ${Math.round(file.size / 1024 / 1024)} MB — the editor opens files up to 64 MB`,
      "error",
    );
    return;
  }
  if (!(await confirmDiscardIfEdited())) {
    setStatus("Open cancelled — your changes are still here");
    return;
  }
  // Reading a file can fail for reasons that have nothing to do with its
  // contents: it moved or was renamed since it was picked, the volume went away,
  // permission was withdrawn. That rejection used to land on a discarded promise
  // at both call sites, so a dropped file that could not be read did nothing at
  // all — no status, no error, no clue (docs/104 HF-065). Reported here, at the
  // one entry point both the picker and the drop go through, so neither caller
  // can be the one that forgets.
  let buf;
  try {
    buf = await file.arrayBuffer();
  } catch (err) {
    console.warn("file read failed:", err);
    setStatus(`${file.name} could not be read — it may have moved or been renamed`, "error");
    return;
  }
  await openBytes(new Uint8Array(buf), file.name);
}

fileEl.addEventListener("change", async (e) => {
  const file = e.target.files[0];
  // Clear the picker's value either way. A file input fires `change` only when
  // the selection differs from what it already holds, so keeping the value
  // makes "cancel the discard prompt, then pick the same file again" a dead
  // control — and re-opening the same file impossible generally (HF-065).
  e.target.value = "";
  await handleFile(file);
});
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

/** Sets a fixed zoom factor (exits any fit mode) and re-renders. A deliberate
 * zoom command restores the editor; a native input `change` caused by focusing
 * some other control does not steal focus back from that control. */
function setZoom(factor, restoreFocus = true) {
  zoomMode = "custom";
  zoomFactor = clampZoom(factor);
  renderAll();
  if (restoreFocus) focusEditorSurface();
}
/** Enters a fit mode; the factor is computed at render time. */
function setZoomMode(mode, restoreFocus = true) {
  zoomMode = mode;
  renderAll();
  if (restoreFocus) focusEditorSurface();
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
function commitZoomInput({ restoreFocus = false } = {}) {
  const raw = zoomEl.value.trim().toLowerCase();
  if (raw.startsWith("fit w") || raw === "width") return setZoomMode("fit-width", restoreFocus);
  if (raw.startsWith("fit p") || raw === "page") return setZoomMode("fit-page", restoreFocus);
  const pct = parseFloat(raw.replace("%", ""));
  if (Number.isFinite(pct) && pct > 0) setZoom(pct / 100, restoreFocus);
  else updateZoomDisplay(); // reject: restore the last valid display
}
zoomEl.addEventListener("change", () => commitZoomInput());
zoomEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    commitZoomInput({ restoreFocus: true });
    zoomEl.blur();
  } else if (e.key === "Escape") {
    updateZoomDisplay();
    zoomEl.blur();
    focusEditorSurface();
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
  if (zoomMode === "custom") {
    // The zoom is unchanged but the window is not: a different viewport height
    // needs different sheets and gives the scroll mapping a new travel.
    updatePageWindow({ force: true });
    paintPagesInView();
    return;
  }
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
  // `handleFile` reports every failure it can name; this catch is for the ones
  // it cannot, so a dropped file can never fail into silence (docs/104 HF-065).
  handleFile(e.dataTransfer?.files?.[0]).catch((err) => {
    console.error("open failed:", err);
    setStatus("That file could not be opened", "error");
  });
});

// ---- Settings: theme + accent + reviewer identity, persisted (OSS-customizable) ----
const settingsBtn = document.getElementById("settingsBtn");
// ---- Autosave drafts and crash recovery (HF-011 / OO-004, docs/112) --------
//
// The hole this closes: the open document lives in the wasm heap and nowhere
// else, so an OOM kill, a wasm trap or a power cut — none of which run a single
// line of shutdown code — took the whole session with it. The `beforeunload`
// guard above catches a deliberate close and nothing else.
//
// The shape is the sibling's (opencalc `editor.drafts.js`), as owner decision
// D-1 requires: an IndexedDB store with a meta row and a bytes row per tab
// slot, written on quiesce with a ceiling, and a boot-time bar that OFFERS the
// work back and never applies it. The policy lives in `drafts.mjs` where it can
// be unit-tested; what is here is wiring, the surfaces, and the export ladder.
//
// Two invariants, both load-bearing:
//   * nothing on the keystroke path is O(document) — `noteDraftDirty` stores an
//     integer and re-arms a timer, and the snapshot runs from that timer;
//   * the draft is written in the document's OWN format, because the
//     normalized-JSON snapshot measurably drops embedded images (docs/112 §3.2)
//     and handing back a document with its pictures gone is silent data loss.
const draftRecoveryBar = document.getElementById("draftRecoveryBar");
const draftRecoveryList = document.getElementById("draftRecoveryList");
const draftRecoveryTitle = document.getElementById("draftRecoveryTitle");
const draftRecoveryDismissBtn = document.getElementById("draftRecoveryDismiss");
const draftStatusEl = document.getElementById("draftStatus");
const autosaveToggle = document.getElementById("autosaveToggle");
const spellCheckToggle = document.getElementById("spellCheckToggle");
const grammarCheckToggle = document.getElementById("grammarCheckToggle");
const draftsClearBtn = document.getElementById("draftsClearBtn");

/** This tab's slot. `let`, because opening another document hands the current
 *  slot's draft over as an orphan and takes a fresh one (see
 *  `handOverDraftBeforeOpen`). */
let draftSlot = draftSlotId(safeSessionStorage());
let draftStore = null;
let draftStoreOpening = null;
/** Non-empty once autosave has given up, and the reason it gave. */
let draftUnavailableReason = "";
let draftDocKey = "";
let draftOffers = [];
let draftBarDismissed = false;
let ownSlotHasDraft = false;
let draftHeartbeatTimer = 0;
/** Serializes store work: two overlapping writes would race on one slot. */
let draftQueue = Promise.resolve();

/** `sessionStorage`, or null where touching it throws (cross-origin embed,
 *  site data blocked) — the same posture `readPref` takes for preferences. */
function safeSessionStorage() {
  try {
    return window.sessionStorage;
  } catch {
    return null;
  }
}

/** Whether this page may keep drafts at all.
 *
 *  Off inside a frame: `home-embed.js` boots `editor.html?demo=1` in an iframe
 *  on the marketing home page, and without this every visitor who typed in the
 *  hero demo would leave a draft on the origin — which a later real session
 *  would then be offered. `?autosave=1` lets a host opt back in, and
 *  `?autosave=0` lets one opt out of the top-level case (docs/112 O-2). */
function autosaveAllowedHere() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("autosave") === "0") return false;
  if (params.get("autosave") === "1") return true;
  try {
    return window.self === window.top;
  } catch {
    return false; // cross-origin frame: `window.top` throws, so we are framed
  }
}
const AUTOSAVE_ALLOWED_HERE = autosaveAllowedHere();

/** The user's switch. Defaults on; `settings` is read at call time. */
function autosaveEnabled() {
  return AUTOSAVE_ALLOWED_HERE && settings.autosave !== false && !draftUnavailableReason;
}

/** Autosave has failed in a way the user should know about. It is never silent:
 *  a draft that is not being written is a promise the editor is not keeping. */
function reportAutosaveUnavailable(reason) {
  if (draftUnavailableReason) return;
  draftUnavailableReason = reason;
  stopDraftHeartbeat();
  setDraftStatus("Autosave unavailable", "unavailable", `Autosave is not running: ${reason}. Save the document to keep your work.`);
  announceStatus(`Autosave unavailable: ${reason}`, "error");
}

function setDraftStatus(text, state = "", title = "") {
  if (!draftStatusEl) return;
  draftStatusEl.textContent = text;
  draftStatusEl.hidden = !text;
  if (state) draftStatusEl.dataset.draftState = state;
  else delete draftStatusEl.dataset.draftState;
  draftStatusEl.title = title || text;
}

/** hh:mm in the user's locale, for "Draft saved 14:32". */
function clockTime(at) {
  return new Date(at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** Opens the store once, lazily. Returns null when it cannot be opened, having
 *  already said so. */
async function ensureDraftStore() {
  if (draftStore) return draftStore;
  if (!AUTOSAVE_ALLOWED_HERE || draftUnavailableReason) return null;
  if (!draftStoreOpening) {
    draftStoreOpening = openDraftStore({}).then(
      (store) => {
        draftStore = store;
        return store;
      },
      (err) => {
        reportAutosaveUnavailable(`this browser refused local storage (${err?.message ?? err})`);
        return null;
      },
    );
  }
  return draftStoreOpening;
}

/** Runs store work one job at a time, so two writes never race on one slot. */
function queueDraftWork(job) {
  draftQueue = draftQueue.then(job).catch((err) => {
    console.error("draft store", err);
  });
  return draftQueue;
}

// The cadence. Injected `write` is the only thing that ever costs document-sized
// work, and the scheduler only ever calls it from a timer.
const draftScheduler = new DraftScheduler({ write: (reason) => queueDraftWork(() => writeDraft(reason)) });

// Cross-tab presence. Answering "is that tab still open?" with a heartbeat
// timestamp is wrong in exactly the case that matters: a crashed tab's last
// heartbeat is seconds old, so a heartbeat-only lease withholds the draft from
// the person reopening the editor to get it back. Asking the other tabs
// directly distinguishes "still open" from "died" immediately.
const draftPresence = new DraftPresence({ slotId: draftSlot });

/** O(1). The edit path's entire contribution to autosave. */
function noteDraftDirty() {
  if (!autosaveEnabled() || !doc) return;
  draftScheduler.noteDirty();
}

/** Re-keys this tab onto a freshly opened document. */
function adoptDraftDocument(name, bytes) {
  draftDocKey = documentKey(name, bytes);
  draftScheduler.reset();
}

/**
 * Takes the snapshot, in the document's own format.
 *
 * The mode ladder is `exportDocumentAs`'s, plus `semantic` as the last resort,
 * so a draft and a save can never be produced by different ladders. There is
 * deliberately no cross-format fallback: normalized JSON would succeed where
 * DOCX failed and would drop every image while doing it.
 */
function takeDraftSnapshot() {
  const formatId = draftFormatFor(currentSourceFormat);
  let lastError = null;
  for (const mode of DRAFT_EXPORT_MODES) {
    try {
      const artifact = doc.exportAs(formatId, mode);
      const bytes = artifact.bytes;
      let findings = 0;
      try {
        findings = compatibilityOccurrenceCount(artifact.reportJson);
      } catch {
        findings = 0; // a report we cannot parse must not lose us the draft
      }
      artifact.free();
      return { bytes, formatId, mode, findings };
    } catch (err) {
      lastError = err;
    }
  }
  throw lastError ?? new Error("no export mode produced a snapshot");
}

/** Writes this tab's draft. Only ever called from the scheduler or a flush. */
async function writeDraft(reason) {
  if (!doc || !autosaveEnabled() || !documentIsDirty()) return;
  const store = await ensureDraftStore();
  if (!store) return;

  let snapshot;
  try {
    snapshot = takeDraftSnapshot();
  } catch (err) {
    reportAutosaveUnavailable(`this document could not be exported (${err?.message ?? err})`);
    return;
  }

  const now = Date.now();
  const meta = {
    slotId: draftSlot,
    docKey: draftDocKey,
    name: currentName,
    formatId: snapshot.formatId,
    exportMode: snapshot.mode,
    findings: snapshot.findings,
    revision: currentRevision,
    bytes: snapshot.bytes.length,
    savedAt: now,
    heartbeatAt: now,
    engine: engineVersion(),
    reason,
  };

  try {
    await pruneDraftSlots(store, now);
    await store.putDraft(meta, snapshot.bytes);
  } catch (err) {
    // Quota is the one failure worth a second attempt: drop the oldest draft
    // that is not ours and try again before giving up on autosave entirely.
    if (err?.name === "QuotaExceededError") {
      const freed = await freeOldestOtherSlot(store);
      if (freed) {
        try {
          await store.putDraft(meta, snapshot.bytes);
        } catch (retryErr) {
          reportAutosaveUnavailable(`this browser is out of storage (${retryErr?.name ?? "quota"})`);
          return;
        }
      } else {
        reportAutosaveUnavailable("this browser is out of storage for local drafts");
        return;
      }
    } else {
      reportAutosaveUnavailable(`the draft could not be written (${err?.message ?? err})`);
      return;
    }
  }

  ownSlotHasDraft = true;
  setDraftStatus(
    `Draft saved ${clockTime(now)}`,
    "saved",
    `${describeDraftSize(meta.bytes)} kept in this browser only, until you save. Written ${clockTime(now)}.`,
  );
  startDraftHeartbeat();
}

/** Deletes expired rows and anything past the slot cap, never our own. */
async function pruneDraftSlots(store, now) {
  const metas = await store.listMeta();
  for (const slotId of evictableSlots(metas, { now, keepSlotId: draftSlot })) {
    await store.deleteSlot(slotId);
  }
}

/** Frees the oldest slot that is not ours. Returns whether anything went. */
async function freeOldestOtherSlot(store) {
  const metas = (await store.listMeta())
    .filter((meta) => meta.slotId !== draftSlot)
    .sort((a, b) => (a.savedAt ?? 0) - (b.savedAt ?? 0));
  if (metas.length === 0) return false;
  await store.deleteSlot(metas[0].slotId);
  return true;
}

/** Tells other tabs this slot is still owned. Meta only — a few hundred bytes,
 *  never the snapshot — and only while a draft actually exists. */
function startDraftHeartbeat() {
  if (draftHeartbeatTimer || !ownSlotHasDraft) return;
  draftHeartbeatTimer = setInterval(() => {
    if (!ownSlotHasDraft) return stopDraftHeartbeat();
    queueDraftWork(async () => {
      const store = await ensureDraftStore();
      if (store) await store.touch(draftSlot, Date.now());
    });
  }, HEARTBEAT_MS);
}

function stopDraftHeartbeat() {
  if (!draftHeartbeatTimer) return;
  clearInterval(draftHeartbeatTimer);
  draftHeartbeatTimer = 0;
}

/** The work is on disk now, so this tab's draft has nothing left to protect. */
function discardOwnDraft() {
  draftScheduler.reset();
  if (!ownSlotHasDraft) return;
  ownSlotHasDraft = false;
  stopDraftHeartbeat();
  setDraftStatus("");
  queueDraftWork(async () => {
    const store = await ensureDraftStore();
    if (store) await store.deleteSlot(draftSlot);
  });
}

/**
 * Called just before another document replaces the open one.
 *
 * If there is unsaved work, it is written one last time and the slot is left
 * behind as an ORPHAN — heartbeat cleared, so the next scan offers it — while
 * this tab takes a fresh slot for the incoming document. The user may have
 * pressed "Discard and open" at the confirm gate, but that discards the
 * document from the screen; it is not a request to shred the only copy.
 */
function handOverDraftBeforeOpen() {
  if (!doc) return;
  const hadWork = documentIsDirty() && autosaveEnabled();
  const previousSlot = draftSlot;
  if (hadWork) {
    // Synchronous snapshot, queued write: the wrapper is freed by the caller
    // moments from now, so the bytes must be taken before this returns.
    let snapshot = null;
    try {
      snapshot = takeDraftSnapshot();
    } catch {
      snapshot = null; // nothing to hand over; the open still proceeds
    }
    if (snapshot) {
      const meta = {
        slotId: previousSlot,
        docKey: draftDocKey,
        name: currentName,
        formatId: snapshot.formatId,
        exportMode: snapshot.mode,
        findings: snapshot.findings,
        revision: currentRevision,
        bytes: snapshot.bytes.length,
        savedAt: Date.now(),
        // Orphaned on purpose: this tab no longer owns the slot, so the next
        // scan must not mistake it for a live tab's live draft.
        heartbeatAt: 0,
        engine: engineVersion(),
        reason: "handover",
      };
      queueDraftWork(async () => {
        const store = await ensureDraftStore();
        if (store) await store.putDraft(meta, snapshot.bytes);
      });
      draftSlot = rotateDraftSlot();
      // Presence must follow the rotation, or this tab keeps answering pings
      // for the slot it just handed over — which would mark that orphan draft
      // as belonging to a live tab and hide it from every recovery scan.
      draftPresence.slotId = draftSlot;
    }
  } else if (ownSlotHasDraft) {
    discardOwnDraft();
  }
  ownSlotHasDraft = false;
  stopDraftHeartbeat();
  draftScheduler.reset();
  setDraftStatus("");
}

/** A fresh slot id for this tab, persisted like the first one. */
function rotateDraftSlot() {
  const storage = safeSessionStorage();
  try {
    storage?.removeItem("opendoc.draftSlot");
  } catch {
    /* storage is optional; a session-only slot still works */
  }
  return draftSlotId(storage);
}

// ---- The recovery offer -----------------------------------------------------

/** Reads the meta store, prunes what has expired, and renders the bar. */
async function refreshDraftOffers({ announce = false } = {}) {
  const store = await ensureDraftStore();
  if (!store) return;
  const now = Date.now();
  let metas = [];
  try {
    metas = await store.listMeta();
  } catch (err) {
    reportAutosaveUnavailable(`the draft store could not be read (${err?.message ?? err})`);
    return;
  }
  for (const slotId of evictableSlots(metas, { now, keepSlotId: draftSlot })) {
    try {
      await store.deleteSlot(slotId);
    } catch {
      /* a row we cannot delete is not a reason to withhold the offer */
    }
  }
  // Ask the other tabs which of these slots they still own. Only rows whose
  // heartbeat is recent enough to be ambiguous are worth asking about, so a
  // boot with nothing else running waits for nothing.
  const ambiguous = metas
    .filter((meta) => meta.slotId !== draftSlot && slotIsLive(meta, now, draftSlot))
    .map((meta) => meta.slotId);
  const liveSlotIds = await draftPresence.liveSlots(ambiguous);

  draftOffers = offerableDrafts(metas, {
    now,
    ownSlotId: draftSlot,
    liveSlotIds,
    // Silence is only proof when there was a way to ask.
    heartbeatFallback: draftPresence.unavailable,
  }).filter(
    // Once this tab has written its own draft, that draft is the document on
    // screen. Offering it back would be offering the user their own live work.
    (meta) => !(ownSlotHasDraft && meta.slotId === draftSlot),
  );
  renderDraftRecovery({ announce });
}

function renderDraftRecovery({ announce = false } = {}) {
  if (!draftRecoveryBar) return;
  if (draftBarDismissed || draftOffers.length === 0) {
    draftRecoveryBar.hidden = true;
    draftRecoveryList.replaceChildren();
    return;
  }
  const now = Date.now();
  draftRecoveryTitle.textContent =
    draftOffers.length === 1
      ? "Unsaved work recovered"
      : `Unsaved work recovered from ${draftOffers.length} sessions`;

  draftRecoveryList.replaceChildren(
    ...draftOffers.map((meta) => {
      const row = document.createElement("li");
      row.className = "draft-recovery-row";
      row.dataset.slot = meta.slotId;

      const name = document.createElement("span");
      name.className = "draft-recovery-name";
      name.textContent = meta.name || "Untitled document";
      name.title = meta.name || "Untitled document";

      const detail = document.createElement("span");
      detail.className = "draft-recovery-meta";
      const sameDocument = meta.docKey && meta.docKey === draftDocKey ? " · this document" : "";
      detail.textContent = `autosaved ${describeDraftAge(now - (meta.savedAt ?? now))} · ${describeDraftSize(meta.bytes)}${sameDocument}`;

      const restore = document.createElement("button");
      restore.type = "button";
      restore.dataset.draftRestore = meta.slotId;
      restore.textContent = "Restore";
      restore.setAttribute("aria-label", `Restore ${meta.name || "the recovered document"}`);

      const remove = document.createElement("button");
      remove.type = "button";
      remove.dataset.draftDelete = meta.slotId;
      remove.textContent = "Delete";
      remove.setAttribute("aria-label", `Delete the recovered draft of ${meta.name || "this document"}`);

      row.append(name, detail, restore, remove);
      return row;
    }),
  );
  draftRecoveryBar.hidden = false;
  if (announce) {
    const first = draftOffers[0];
    announceStatus(
      `Unsaved work recovered: ${first.name}, autosaved ${describeDraftAge(now - (first.savedAt ?? now))}. Restore or delete it from the recovery bar.`,
    );
  }
}

/** Brings the offer back after a dismissal — the File menu / palette route. */
function showDraftRecovery() {
  draftBarDismissed = false;
  renderDraftRecovery();
  queueDraftWork(() => refreshDraftOffers());
  draftRecoveryBar?.querySelector("button[data-draft-restore]")?.focus({ preventScroll: true });
}

/** Restores one draft. Offers first, applies only on this call. */
async function restoreDraft(slotId) {
  const meta = draftOffers.find((row) => row.slotId === slotId);
  const store = await ensureDraftStore();
  if (!meta || !store) return;
  // The same gate File ▸ Open uses: restoring replaces what is on screen.
  if (!(await confirmDiscardIfEdited())) return;
  let bytes;
  try {
    bytes = await store.readBytes(slotId);
  } catch (err) {
    setStatus(`The recovered draft could not be read: ${err?.message ?? err}`, "error");
    return;
  }
  if (!bytes) {
    setStatus("That draft is no longer in this browser's storage", "error");
    await forgetDraft(slotId);
    return;
  }
  // A promoted draft (a .txt document kept as DOCX so its formatting survived)
  // comes back under the extension it will actually save as.
  const name = downloadNameForFormat(meta.name, formatInfo(meta.formatId).extension);
  // `openBytes` swallows a failed parse by design — it leaves the previous
  // document on screen rather than destroying it — so success has to be
  // observed, not assumed. Its opened-document hook runs only after the parse
  // succeeded. Treating a failed restore as a success and then deleting the
  // row would delete the only copy of the work the restore was trying to save.
  let opened = false;
  await openBytes(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes), name, () => {
    opened = true;
  });
  if (!opened) {
    setStatus(`“${meta.name}” could not be restored — the draft has been kept`, "error");
    return;
  }
  // Restored work has never been written out. Saying "Opened" here would tell
  // the user their document is safe on disk when it is not.
  restoredFromDraft = true;
  setDocumentState("edited");
  // Take our own copy BEFORE dropping the row it came from, so the work is
  // never momentarily in neither place — a crash in the gap would otherwise
  // lose exactly the document the user had just recovered.
  await queueDraftWork(() => writeDraft("restored"));
  if (slotId === draftSlot) {
    // This tab reclaimed its own slot, so the write above IS that row, now
    // holding the restored document. Deleting it would delete the work.
    draftOffers = draftOffers.filter((row) => row.slotId !== slotId);
    renderDraftRecovery();
  } else if (ownSlotHasDraft) {
    await forgetDraft(slotId);
  } else {
    // Autosave could not take a copy (off, or unavailable), so the row we
    // restored from is still the only one. Keep it; just stop offering it.
    draftOffers = draftOffers.filter((row) => row.slotId !== slotId);
    renderDraftRecovery();
  }
  setStatus(
    `Restored “${name}” from the draft autosaved ${describeDraftAge(Date.now() - (meta.savedAt ?? Date.now()))} — save it to keep it`,
  );
}

/** Deletes one draft, after asking: it is the only copy of that work. */
async function deleteDraftWithConfirmation(slotId) {
  const meta = draftOffers.find((row) => row.slotId === slotId);
  if (!meta) return;
  const ok = await confirmModal({
    title: "Delete this recovered draft?",
    message: `“${meta.name}” was autosaved ${describeDraftAge(Date.now() - (meta.savedAt ?? Date.now()))}. This is the only copy of that work — deleting it cannot be undone.`,
    confirmLabel: "Delete draft",
    cancelLabel: "Keep it",
    note: "Restore it first if you are not sure.",
    icon: "warning",
  });
  if (!ok) return;
  await forgetDraft(slotId);
  setStatus(`Deleted the recovered draft of “${meta.name}”`);
}

async function forgetDraft(slotId) {
  const store = await ensureDraftStore();
  if (store) {
    try {
      await store.deleteSlot(slotId);
    } catch (err) {
      console.error("draft delete", err);
    }
  }
  draftOffers = draftOffers.filter((row) => row.slotId !== slotId);
  renderDraftRecovery();
}

/** Settings ▸ Autosave ▸ Delete saved drafts, and the switch's off path. */
async function clearAllDrafts({ confirm = true } = {}) {
  if (confirm) {
    const ok = await confirmModal({
      title: "Delete every saved draft?",
      message:
        "Drafts are the only copy of work that was never saved to a file. Deleting them cannot be undone.",
      confirmLabel: "Delete drafts",
      cancelLabel: "Keep them",
      icon: "warning",
    });
    if (!ok) return;
  }
  const store = await ensureDraftStore();
  if (store) {
    try {
      await store.clear();
    } catch (err) {
      console.error("draft clear", err);
    }
  }
  ownSlotHasDraft = false;
  stopDraftHeartbeat();
  draftOffers = [];
  renderDraftRecovery();
  setDraftStatus("");
  setStatus("Saved drafts deleted");
}

draftRecoveryBar?.addEventListener("click", (event) => {
  const restore = event.target.closest?.("button[data-draft-restore]");
  if (restore) {
    void restoreDraft(restore.dataset.draftRestore);
    return;
  }
  const remove = event.target.closest?.("button[data-draft-delete]");
  if (remove) void deleteDraftWithConfirmation(remove.dataset.draftDelete);
});
draftRecoveryDismissBtn?.addEventListener("click", () => {
  // Dismiss keeps the draft. Nothing in this bar may lose work by being closed.
  draftBarDismissed = true;
  renderDraftRecovery();
  setStatus("Recovered work is still available from File ▸ Recover unsaved work");
});

// `visibilitychange` rather than `beforeunload`: the hidden transition is the
// only one browsers reliably fire for a background-tab discard or a mobile
// app switch, which is where the tab most often dies. `pagehide` is the belt.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "hidden") draftScheduler.flush("hidden");
});
window.addEventListener("pagehide", () => draftScheduler.flush("pagehide"));

/** Boot: scan the store and offer whatever a previous session left behind. */
async function startDrafts() {
  if (!AUTOSAVE_ALLOWED_HERE) return;
  if (settings.autosave === false) {
    setDraftStatus("Autosave off", "off", "Autosave is off. Turn it back on in Settings ▸ Autosave.");
    return;
  }
  await queueDraftWork(() => refreshDraftOffers({ announce: true }));
}

const settingsPanel = document.getElementById("settingsPanel");
const themeSeg = document.getElementById("themeSeg");
const accentSwatches = document.getElementById("accentSwatches");
const accentCustom = document.getElementById("accentCustom");
const settingsReset = document.getElementById("settingsReset");
const settingsClose = document.getElementById("settingsClose");
const languageSelect = document.getElementById("languageSelect");
const authorNameInput = document.getElementById("authorName");
const authorInitialsInput = document.getElementById("authorInitials");

const DEFAULT_SETTINGS = {
  theme: "system",
  // "" means follow the browser. A person who has never touched this gets
  // their own language if we ship it, and a person who chose one keeps it even
  // on a machine whose browser disagrees (docs/124 §5).
  language: "",
  accent: "#3355c4",
  authorName: "",
  authorInitials: "",
  // On by default: the row this closes is a P0 data-safety row, and a safety
  // net nobody switches on is not one. Turning it off deletes what is stored.
  autosave: true,
  // Spelling. On by default because a plain <textarea> checks spelling and an
  // editor that does not is visibly behind one; remembered, because a user who
  // turned it off did not mean "until the next reload" (docs/114 §5.6).
  spellCheck: true,
  // Grammar. A SEPARATE switch from spelling, as in Word: the two checks are
  // independent, they mark differently, and the owner rates grammar the more
  // important of the two — so it must not be reachable only by leaving
  // spelling on.
  grammarCheck: true,
  // The language used where the document's own w:lang does not say. NOT
  // navigator.language: that would make every test non-deterministic and the
  // user could not see why the answer changed.
  spellLanguage: "en-US",
};
let settings = loadSettings();

function loadSettings() {
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(readPref("opendoc.settings") || "{}") };
  } catch {
    // Storage is already guarded by readPref; this catches a corrupt payload.
    return { ...DEFAULT_SETTINGS };
  }
}

function saveSettings() {
  writePref("opendoc.settings", JSON.stringify(settings));
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
  // The gallery is built per document, but its legibility decisions were made
  // against whatever palette was live at the time.
  refreshStylePreviews();

  for (const b of themeSeg.querySelectorAll("button")) {
    const checked = b.dataset.theme === settings.theme;
    // role=radio + aria-checked, because the group is declared a radiogroup and
    // three aria-pressed toggles do not make one (docs/104 HF-070). Roving
    // tabindex so Tab enters the group once and arrows move within it.
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(checked));
    b.tabIndex = checked ? 0 : -1;
    b.removeAttribute("aria-pressed");
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
  if (autosaveToggle) {
    autosaveToggle.checked = settings.autosave !== false;
    // A control that cannot do anything says why, rather than sitting there
    // looking operable (SKILL.md §10). In a host iframe autosave is off by
    // policy, not by preference.
    autosaveToggle.disabled = !AUTOSAVE_ALLOWED_HERE;
    autosaveToggle.title = AUTOSAVE_ALLOWED_HERE
      ? ""
      : "Autosave is off in an embedded editor — the page that embeds it owns saving.";
  }
  if (draftsClearBtn) draftsClearBtn.disabled = !AUTOSAVE_ALLOWED_HERE;
  if (spellCheckToggle) spellCheckToggle.checked = settings.spellCheck !== false;
  if (grammarCheckToggle) grammarCheckToggle.checked = settings.grammarCheck !== false;
  applyActiveAuthorToDocument();
}

autosaveToggle?.addEventListener("change", () => {
  settings.autosave = autosaveToggle.checked;
  saveSettings();
  if (settings.autosave) {
    setDraftStatus("");
    noteDraftDirty();
    void queueDraftWork(() => refreshDraftOffers());
  } else {
    // Off means off: the bytes go too, or "off" would only mean "stop adding".
    void clearAllDrafts({ confirm: false });
    setDraftStatus("Autosave off", "off", "Autosave is off. Turn it back on in Settings ▸ Autosave.");
  }
});
draftsClearBtn?.addEventListener("click", () => void clearAllDrafts());
spellCheckToggle?.addEventListener("change", () => setSpellCheckEnabled(spellCheckToggle.checked));
grammarCheckToggle?.addEventListener("change", () => setGrammarCheckEnabled(grammarCheckToggle.checked));

themeSeg.addEventListener("click", (e) => {
  const b = e.target.closest("button[data-theme]");
  if (!b) return;
  settings.theme = b.dataset.theme;
  saveSettings();
  applySettings();
});
themeSeg.addEventListener("keydown", (e) => {
  const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1
    : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1
      : 0;
  if (!step) return;
  e.preventDefault();
  const items = [...themeSeg.querySelectorAll("button[data-theme]")];
  const index = items.indexOf(document.activeElement);
  const next = items[(Math.max(0, index) + step + items.length) % items.length];
  settings.theme = next.dataset.theme;
  saveSettings();
  applySettings();
  next.focus();
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

// Settings is a MODAL now, registered like every other dialog here. It was an
// anchored popover with its own Escape handler and its own pointerdown light
// dismiss — the hand-rolled dismissal `modal.mjs` exists to end — and it was
// taller than the window, so scrolling it scrolled the document behind it. The
// owner's report: "that panel doesn't make any sense now .. see dialog
// instead". Registering it means Escape, the backdrop, the close button and
// focus restoration are one path, Tab cannot walk out into the chrome behind
// the scrim, and application shortcuts stop firing behind it.
const settingsModal = registerModal(settingsPanel, {
  initialFocus: () => settingsPanel.querySelector("#themeSeg button[aria-checked='true']"),
  fallbackFocus: () => settingsBtn,
});

function toggleSettings(open) {
  const show = open ?? !settingsModal.isOpen;
  if (show === settingsModal.isOpen) return;
  settingsBtn.setAttribute("aria-expanded", String(show));
  if (show) settingsModal.open();
  else settingsModal.close();
}
settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  // The Settings DIALOG and the Settings PANE are the same element. While the
  // File page is open that element is parented INSIDE the page, so opening the
  // dialog would have raised a half-dialog out of a pane. There is one
  // Settings, so the gear goes to wherever it currently lives: the page's own
  // pane while the page is open, the dialog otherwise.
  if (document.body.classList.contains("file-page-open")) {
    setFilePane("settings");
    renderFilePage();
    document.querySelector('#filePageBody [data-file-pane="settings"]')?.focus();
    return;
  }
  toggleSettings();
});
settingsClose.addEventListener("click", () => toggleSettings(false));
applySettings();
// After `applySettings`, so the picker is built against the settings that were
// actually loaded, and awaited nowhere: the editor draws in English and the
// chosen language relabels it when its catalogue lands.
// Locale (docs/124). After `applySettings`, so the picker is built against
// the settings that were actually loaded, and awaited nowhere: the editor
// draws in English and the chosen language relabels it when its catalogue
// lands. The counts are the one surface markup cannot carry — they are
// rendered from the engine — so re-rendering them is what this hands over.
void startLocalisation({
  select: languageSelect,
  settings,
  saveSettings,
  onLocalised: () => {
    if (doc) updateStats();
  },
});

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

// Chrome that must never survive into a modal's foreground (docs/104 HF-089):
// a pinned tracked-change card used to paint above the scrim with live
// Accept/Reject buttons, so a change could be accepted while a blocking dialog
// was supposedly in control. The z-index ladder now puts modals on top, and
// this hook makes the floating review chrome go away entirely — one place,
// rather than teaching every dialog's open path about every popover.
setModalHooks({
  firstOpen: () => {
    closeReviewPopover();
    closeReviewInlineCard();
    closeAppMenu();
  },
});

const propertiesModal = registerModal(propertiesPanel, {
  initialFocus: () => propTitle,
  fallbackFocus: () => propertiesBtn,
});

function toggleProperties(open) {
  const show = open ?? !propertiesModal.isOpen;
  if (show === propertiesModal.isOpen) return;
  if (show && doc) {
    const current = JSON.parse(doc.documentProperties());
    for (const [key, input] of PROP_FIELDS) input.value = current[key] ?? "";
    reflectDocumentMetadata();
  }
  propertiesBtn.setAttribute("aria-expanded", String(show));
  if (show) propertiesModal.open();
  else propertiesModal.close();
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

// Which control the dialog should land on for THIS opening. Layout ▸ Margins and
// Layout ▸ Columns are the same dialog reached with a different intent, and
// Word/Docs both put you on the field you asked for; landing everyone on
// Orientation would make three of the four buttons feel like the wrong button.
// Reset on every open so a deep link cannot leak into the next plain opening.
let pageSetupFocusTarget = null;

const pageSetupModal = registerModal(pageSetupMenu, {
  initialFocus: () =>
    pageSetupFocusTarget?.() ?? pageOrientationSeg.querySelector('button[aria-pressed="true"]'),
  fallbackFocus: () => pageSetupBtn,
});

function togglePageSetup(open, focusTarget = null) {
  const show = open ?? !pageSetupModal.isOpen;
  if (show === pageSetupModal.isOpen) return;
  if (show && !reflectPageSetup()) return; // no section geometry to edit
  pageSetupFocusTarget = show ? focusTarget : null;
  pageSetupBtn.setAttribute("aria-expanded", String(show));
  if (show) pageSetupModal.open();
  else pageSetupModal.close();
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
fileEl.disabled = true;
if (openBtn) openBtn.disabled = true;
// No document is open yet, so every document-scoped control starts disabled.
// (Not "no selection yet": once a document opens it always has an insertion
// point — see `openBytes` — so `selection` tracks the document, not the click.)
updateToolbar();
boot();

// ---- Chrome modes: compact vs ribbon (owner prototype) ---------------------
//
// Two mutually exclusive toolbars. The ribbon is the Word-shaped tabbed chrome;
// compact is the Docs-shaped single dense bar. The bar's layout table, its
// grouping and its fold live in `compact_toolbar.mjs` (HF-085: new logic goes
// in a module); what stays here is the mode switch and the dependency bag.
const CHROME_MODE_PREF = "opendoc.chromeMode";
let chromeMode = readPref(CHROME_MODE_PREF, "ribbon") === "compact" ? "compact" : "ribbon";

compactToolbarUi = createCompactToolbar({
  host: document.getElementById("compactToolbar"),
  editorCommands,
  onButton,
  registerPopover,
  runControls,
  paraControls,
  formatToggleCache,
  localizeShortcut: (text) => localizeShortcutText(text, EDITOR_KEYBOARD_PLATFORM),
});

/** Switches chrome. Mutually exclusive by construction — `body` carries exactly
 *  one mode class — and the caret is never disturbed, because neither chrome
 *  owns the editing surface. */
function setChromeMode(mode, { persist = true } = {}) {
  chromeMode = mode === "compact" ? "compact" : "ribbon";
  const compact = chromeMode === "compact";
  // The File PAGE is ribbon chrome: compact mode hides the whole ribbon, so
  // switching modes with it open would leave `.file-page-open` set over a page
  // nobody can see or leave. Compact mode answers File with a dropdown instead.
  if (compact) closeFilePage();
  document.body.classList.toggle("compact-mode", compact);
  document.body.classList.toggle("ribbon-mode", !compact);
  // The toolbar IS the chrome — the wrapper it used to sit in was a second
  // nested box, which is what read as a doubled outline on both sides and the
  // bottom.
  const bar = document.getElementById("compactToolbar");
  if (bar) bar.hidden = !compact;
  const cb = document.getElementById("modeCompact");
  const rb = document.getElementById("modeRibbon");
  if (cb && rb) {
    cb.setAttribute("aria-checked", String(compact));
    rb.setAttribute("aria-checked", String(!compact));
    cb.tabIndex = compact ? 0 : -1;
    rb.tabIndex = compact ? -1 : 0;
  }
  // With no document open the registry holds only `noDoc` commands, so the bar
  // would render nearly empty; it is rebuilt on `doc-loaded`. Rendering here
  // anyway keeps the adopted controls in place so the bar is never a bare strip.
  if (compact) compactToolbarUi.render();
  else compactToolbarUi.release();
  if (persist) writePref(CHROME_MODE_PREF, chromeMode);
  setStatus(compact ? "Compact toolbar" : "Ribbon toolbar", "", { timeout: 1800 });
}

{
  const cb = document.getElementById("modeCompact");
  const rb = document.getElementById("modeRibbon");
  if (cb) onButton(cb, () => setChromeMode("compact"));
  if (rb) onButton(rb, () => setChromeMode("ribbon"));
  // Arrow keys move within the radiogroup, as a radiogroup must.
  for (const [el, other, mode] of [[cb, rb, "ribbon"], [rb, cb, "compact"]]) {
    el?.addEventListener("keydown", (e) => {
      if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) return;
      e.preventDefault();
      setChromeMode(mode);
      other?.focus();
    });
  }
  setChromeMode(chromeMode, { persist: false });
}

