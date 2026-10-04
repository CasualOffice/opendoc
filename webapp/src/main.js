// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads the `casual-doc-wasm` module, opens a user-selected `.docx` fully
// client-side, and blits each rendered page onto a canvas. This is the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import init, { open, beginVersionDiff, defaultDiffSlice, engineVersion } from "../pkg/casual_doc_wasm.js";
// The measurement layer's five free functions. FREE, not methods on a document,
// because a unit preference belongs to the person and governs dialogs that open
// with nothing loaded — which is also why they are handed to
// `measurement_units.mjs` rather than reached for there (that module stays
// browser-free and engine-free, so its policy is answerable in node).
import {
  decimalSeparatorForLanguage,
  defaultMeasurementUnit,
  formatMeasurement,
  measurementUnits,
  parseMeasurement,
} from "../pkg/casual_doc_wasm.js";
import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { embedMarker, extractMarker, htmlToRuns, htmlToStructured, runsToHtml } from "./clipboard.mjs";
import { escapeHtml } from "./text_rules.mjs";
import { bindBreaksMenu, breakSurfaceRows } from "./break_commands.mjs";
import { bindComparePanel, comparableBytes } from "./compare_documents.mjs";
import { EXPORT_COMMANDS, exportCommands } from "./export_commands.mjs";
import { editRefusalMessage, mutationBlockedMessage } from "./edit_errors.mjs";
import { renderAccessibilityMirror } from "./a11y_mirror.mjs";
import { createAboutDialog } from "./about_dialog.mjs";
import { createPagesPanel } from "./pages_panel.mjs";
import { createBookmarkManager } from "./bookmark_manager.mjs";
import { createSpacingMenu } from "./spacing_menu.mjs";
import { createTocNavigator } from "./toc_navigation.mjs";
import { createDropCapDialog } from "./drop_cap.mjs";
import { createReferenceCommands, objectMenuRows } from "./reference_commands.mjs";
import { createTocCommands } from "./toc_commands.mjs";
import {
  reflectReviewSurface,
  ribbonSurfaceEnabled as surfaceEnabled,
  ribbonSurfaceReason as surfaceReason,
} from "./ribbon_surface.mjs";
import { buildObjectContextCommands } from "./object_context_menu.mjs";
import { renderOutline, reflectOutlineActive } from "./outline_panel.mjs";
import { createHeaderFooterSettings } from "./header_footer_settings.mjs";
import { createPageSetup } from "./page_setup.mjs";
import { createGlyphPicker } from "./glyph_picker.mjs";
import { EMOJI_GROUPS, SYMBOL_GROUPS } from "./glyph_sets.mjs";
import { spellingContextCommands } from "./spell_check.mjs";
import { createProofingChrome } from "./proofing_chrome.mjs";
import { OBJECT_LABELS, escapeClimbsToGroup, groupClickAction, nextObjectIndex, traversalAnnouncement, traversalRoot } from "./object_traversal.mjs";
import { FONT_SIZE_STEPS, RECOMMENDED_STYLES, caretContexts, nextFontSizeStep, offeredStyleNames, previewPx, styleMenuGroups, styleSlug } from "./style_picker.mjs";
import { applyPreviewInk, applyStylePreview, refreshStylePreviews } from "./style_preview.mjs";
import { renderShortcutsReference, shortcutGroups } from "./shortcuts_reference.mjs";
import { printDocument } from "./print.mjs";
import { downloadBytes, populateSaveFormats, showCompatibilityFindings } from "./save_formats.mjs";
import { attachHostBridge } from "./host_bridge.mjs";
import { createHostSession } from "./host_session.mjs";
import { createCompactToolbar } from "./compact_toolbar.mjs";
import { createConfirmDialog } from "./confirm_dialog.mjs";
import { createPropertiesDialog } from "./document_metadata.mjs";
import { createNamePrompt } from "./name_dialog.mjs";
import { createVersionHistory } from "./version_panel.mjs";
import { CAPTURE_REASON } from "./version_history.mjs";
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
  importFindingCount,
  downloadNameForFormat,
  formatInfo,
} from "./format_io.mjs";
import { formatShortcut, keyboardPlatform, lineDeletionDirection, navigationDirection, navigationShortcuts, wordDeletionDirection } from "./keyboard.mjs";
import { chordCommand, shortcutForCommand } from "./keymap.mjs";
import { clampContextMenuPosition, moveMenuIndex, normalizeMenuEntries } from "./context_menu.mjs";
import {
  focusMenuIndex,
  menuItemAt,
  renderMenuLevel as renderMenuLevelRows,
} from "./menu_render.mjs";
import { modalIsOpen, registerModal, setModalHooks } from "./modal.mjs";
import { beginGripDrag, cropFromDrag, keptRect, paintCropChrome } from "./object_crop_chrome.mjs";
import { clearGuides, objectBarPosition, paintGuides, paintMovePad, paintResizeHandles } from "./object_guides.mjs";
// Own lines (anti-conflict): the two direct-manipulation geometry gestures.
import { createObjectResizeDrag, sizeLabel as resizeSizeLabel } from "./object_resize_drag.mjs";
import { createObjectRotateDrag } from "./object_rotate_drag.mjs";
import { arrangeSurfaceRows, createObjectArrangeCommands } from "./object_arrange_commands.mjs";
import { createObjectBar } from "./object_bar.mjs";
import { renderShapeMenu } from "./shape_format_menu.mjs";
import { createObjectInspector } from "./object_inspector.mjs";
import { POSITION_PRESETS, ROTATE_CHOICES, WRAP_CHOICES, Z_ORDER_CHOICES, activeWrapChoice, positionAvailability, positionPayload, readGroupability, readPosition, readTransform, rotationPlan, wrapAvailability, wrapPlan } from "./object_arrange.mjs";
import { renderArrangeMenu, renderPositionGallery, renderRotateMenu } from "./object_arrange_chrome.mjs";
import { SHAPE_PRESETS, shapeNameKey } from "./shape_catalogue.mjs";
import { galleryRowStarts, nextGalleryIndex, renderShapeGallery } from "./shape_gallery.mjs";
import { createShapeDrawMode } from "./shape_draw_mode.mjs";
import { reflectObjectSelection, reflectShapeFormat } from "./object_selection_state.mjs";
import { pageSnapTargets, snapBox } from "./object_snap.mjs";
import { activeLocale, t } from "./i18n.mjs";
import { authoredTitle, paintDocumentState } from "./localize.mjs";
import { countLabels, pageIndicator, readerPosition } from "./status_counts.mjs";
import { startLocalisation } from "./locale_boot.mjs";
import { localizeShortcutGlyphs, localizeShortcutText } from "./shortcut_labels.mjs";
import { installRibbonTooltips } from "./ribbon_tooltip.mjs";
import { createFormattingMarks } from "./formatting_marks.mjs";
import { createMeasurementUnits } from "./measurement_units.mjs";
import { createDocumentProtection } from "./document_protection.mjs";
import {
  DRAFT_EXPORT_MODES,
  DRAFT_WRITE_REASONS,
  DraftPresence,
  DraftScheduler,
  HEARTBEAT_MS,
  bindDraftFlushOnExit,
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
import { bindRadioGroup } from "./radio_group.mjs";
import { closeAllPopovers, configurePopovers, onButton, openPopover } from "./popover_manager.mjs";
import { closePopover, reflectOpenPopovers, registerPopover } from "./popover_manager.mjs";
import { HIGHLIGHT_COLORS, HIGHLIGHT_LABEL, TEXT_STANDARD_COLORS, highlightHex } from "./palettes.mjs";
import { createViewZoom, fitZoomFactor, nextZoomStep, openingZoomMode, parseZoomInput, reflectZoomMenu } from "./view_zoom.mjs";
import { createPhoneChrome } from "./phone_chrome.mjs";
import { createTouchSelection, pointerDragSelects } from "./touch_selection.mjs";
import { DEFAULT_SETTINGS } from "./settings_defaults.mjs";
import { editingModeFor, hostCapabilities, hostChrome, hostConfig, reflectReviewModeAccess } from "./capabilities.mjs";
import { openRoom, resumeKey } from "./collab_transport.mjs";
import { insertChartAtCaret } from "./chart_insert.mjs";
import { collabCommands } from "./collab_chrome.mjs";
import { groupsToOverflow } from "./ribbon_overflow.mjs";
import { smallestContaining } from "./review_anchor.mjs";
import { scrollTargetFor } from "./scroll_into_view.mjs";
import { matchWithinScope, positionComparator } from "./find_scope.mjs";
import { sessionAccess } from "./session_access.mjs"; // the ROOM's grant, a different authority from the container's
import { createReviewCommentActions } from "./review_comment_actions.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { createVerticalGoal, orderedSelectionEnds, recoverVerticalMove, sameModelPosition, selectionMatchesRange } from "./caret_navigation.mjs";
import {
  reviewCardSignature,
  reviewCommentIsReplyTo,
  reviewStackHeight,
  stackReviewCards,
} from "./review_layout.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { attachComposerKeys, autoGrowTextarea, createCommentAffordance, createReviewBanners, reviewCardButton, reviewIconButton } from "./review_chrome.mjs";
import { makeMenuHeading, makeSwatchCell, makeSwatchGrid, makeUnderlineStyleOption, renderColorMenu as renderColorMenuChrome } from "./picker_chrome.mjs";
import {
  formatReviewDate,
  reviewAuthorColor,
  reviewAuthorDisplay,
  reviewAuthorKey,
  reviewCardAriaLabel,
  reviewChangeTypeLabel,
  reviewCommentTooltip,
  reviewFormattingDescription,
  reviewRevisionTooltip,
} from "./review_labels.mjs";
import {
  byteOffsetToStringIndex,
  isWholeWordAt,
  recaseRichRuns,
  smartQuoteForTyped,
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
  documentStateBadge,
  documentTabTitle,
  isObjectSelectionStatus,
  statusClassName,
} from "./status_policy.mjs";
import { createStatusChannel } from "./status_channel.mjs";
import {
  APP_MENU_SECTIONS,
  flattenCommandTree,
  tableCommandLabel,
  tableMenuPlaceholders,
} from "./command_taxonomy.mjs";
import { fileMenuSections } from "./command_taxonomy.mjs";
import { FIELD_KINDS, fieldLabel, fieldResultText } from "./field_kinds.mjs";
import {
  createMenuBar,
  renderCommandRows,
  renderExportPane,
  renderFilePageInfo,
} from "./command_menu.mjs";
import { reviewCurrentTargetIndex } from "./review_target.mjs";
import {
  fileCategoryRowFor,
  setPaneRenderer,
  showSettingsPane,
  releaseSettingsPanel,
  renderFilePane,
  setFilePane,
} from "./file_pane.mjs";
import { revealOnSettings } from "./surface_reveal.mjs";

/** Settings' two homes, for `revealOnSettings`. The gear is deliberately NOT
 *  routed through this: it TOGGLES, which is a real difference. */
const SETTINGS_SURFACE = { showPane: () => showSettingsPane(), openDialog: () => toggleSettings(true) };
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
import { pasteLossMessage } from "./paste_loss.mjs";
import { createPointerHover } from "./pointer_hover.mjs";
import { createTableChrome } from "./table_chrome.mjs";
import { createTableGutter } from "./table_gutter.mjs";
import { createTableRange } from "./table_range.mjs";
import { bindCellFormatMenu, bindSplitCellDialog } from "./table_cell_chrome.mjs";
import { createReflowChrome } from "./reflow_chrome.mjs";
import { createFoldChrome } from "./fold_chrome.mjs";
import { createRuler } from "./ruler.mjs";
import { createTabStopsDialog } from "./tab_stops_dialog.mjs";
import { createObjectPresence } from "./object_presence.mjs";
import { stampRibbonFaces } from "./ribbon_faces.mjs";
import { bindTableBand, tableBandStates, tableContextLabel } from "./table_band.mjs";
import { tableToolCommands as buildTableToolCommands } from "./table_commands.mjs";
import { loadPrefObject, readPref, savePrefObject, writePref } from "./prefs.mjs";
import { BRAND } from "./brand.mjs";
import { applyAppearance, brandAccent, reflectAppearance } from "./appearance.mjs";
import { applyRegions } from "./chrome_regions.mjs";


/** url → Uint8Array of already-fetched font bytes (persists across documents). */
const fontCache = new Map();

const statusEl = document.getElementById("status");
// Every channel a status message can reach a person through, wired once (`109`
// UX-017). The regions and the toast are body-level, outside the footer half
// that `display: none` takes away below 620px — see the comment beside them in
// editor.html for the measurement that forced the move.
const statusChannel = createStatusChannel({
  live: document.getElementById("statusLiveRegion"),
  alert: document.getElementById("statusAlertRegion"),
  toast: document.getElementById("statusToast"),
  statusLine: statusEl,
});
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
const paraOptsBtn = document.getElementById("paraOptsBtn");
const paragraphPropertiesPanel = document.getElementById("paragraphPropertiesPanel");
const paragraphPropertiesContext = document.getElementById("paragraphPropertiesContext");
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
const tableBtn = document.getElementById("tableBtn");
const tableFmtMenu = document.getElementById("tableMenu");
// The cell-format popover's six controls are looked up where they are PASSED to
// `bindCellFormatMenu`, their only reader.
const tableAlign = document.getElementById("tableAlign");
const tableContext = document.getElementById("tableContext");
const tableRibbon = document.querySelector(".table-ribbon");
// The band's authored tooltips, captured at BOOT before any catalogue exists —
// the same fallback the Layout and References bands take. Without it, restoring a
// title after a disabled state could hand back the REASON the control was given
// (`docs/141` TBL-03), because `authoredTitle` falls back to the live `title`.
for (const control of tableRibbon.querySelectorAll("button")) control.dataset.enabledTitle = control.title;
const tablePropertiesBtn = document.getElementById("tablePropertiesBtn");
const tableStyleBtn = document.getElementById("tableStyleBtn");
const tableStyleMenu = document.getElementById("tableStyleMenu");
const tablePropertiesPanel = document.getElementById("tablePropertiesPanel");
const tablePropertiesContext = document.getElementById("tablePropertiesContext");
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
const lineNumbersBtn = document.getElementById("lineNumbersBtn");
const watermarkBtn = document.getElementById("watermarkBtn");
const insertPictureBtn = document.getElementById("insertPictureBtn");
const insertShapeBtn = document.getElementById("insertShapeBtn");
const insertChartBtn = document.getElementById("insertChartBtn");
const layoutSendBackwardBtn = document.getElementById("layoutSendBackwardBtn");
const layoutGroupBtn = document.getElementById("layoutGroupBtn");
const layoutUngroupBtn = document.getElementById("layoutUngroupBtn");
const layoutRotateBtn = document.getElementById("layoutRotateBtn");
const insertTextBoxBtn = document.getElementById("insertTextBoxBtn");
const insertLinkBtn = document.getElementById("insertLinkBtn");
const insertBookmarkBtn = document.getElementById("insertBookmarkBtn");
const insertFieldBtn = document.getElementById("insertFieldBtn");
const insertPageNumberBtn = document.getElementById("insertPageNumberBtn");
const insertDateBtn = document.getElementById("insertDateBtn");
const insertCommentBtn = document.getElementById("insertCommentBtn");
const insertDropCapBtn = document.getElementById("insertDropCapBtn");
const insertHeaderBtn = document.getElementById("insertHeaderBtn");
const insertFooterBtn = document.getElementById("insertFooterBtn");
const insertFirstPageVariantBtn = document.getElementById("insertFirstPageVariantBtn");
const insertEvenOddVariantBtn = document.getElementById("insertEvenOddVariantBtn");
const headerFooterSettingsBtn = document.getElementById("headerFooterSettingsBtn");
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
const refGoToHeadingBtn = document.getElementById("refGoToHeadingBtn");
const refBookmarkBtn = document.getElementById("refBookmarkBtn");
const refCaptionBtn = document.getElementById("refCaptionBtn");
const refCrossRefBtn = document.getElementById("refCrossRefBtn");
const refFootnoteBtn = document.getElementById("refFootnoteBtn");
const refEndnoteBtn = document.getElementById("refEndnoteBtn");
const refFieldBtn = document.getElementById("refFieldBtn");
const refUpdateCaptionsBtn = document.getElementById("refUpdateCaptionsBtn");
const refUpdateFieldsBtn = document.getElementById("refUpdateFieldsBtn");
const reviewTrackBtn = document.getElementById("reviewTrackBtn");
const reviewShowChangesBtn = document.getElementById("reviewShowChangesBtn");
const reviewPrevBtn = document.getElementById("reviewPrevBtn");
const reviewNextBtn = document.getElementById("reviewNextBtn");
const reviewAcceptBtn = document.getElementById("reviewAcceptBtn");
const reviewRejectBtn = document.getElementById("reviewRejectBtn");
const reviewAcceptAllBtn = document.getElementById("reviewAcceptAllBtn");
const reviewRejectAllBtn = document.getElementById("reviewRejectAllBtn");
const reviewCommentBtn = document.getElementById("reviewCommentBtn");
const reviewResolveBtn = document.getElementById("reviewResolveBtn");
const reviewDeleteBtn = document.getElementById("reviewDeleteBtn");
const reviewPanelBtn = document.getElementById("reviewPanelBtn");
const reviewSpellCheckBtn = document.getElementById("reviewSpellCheckBtn");
const reviewGrammarCheckBtn = document.getElementById("reviewGrammarCheckBtn");
const reviewSmartQuotesBtn = document.getElementById("reviewSmartQuotesBtn");
const reviewProofLanguagesBtn = document.getElementById("reviewProofLanguagesBtn");
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
const findBtn = document.getElementById("findBtn");
const findPanel = document.getElementById("findPanel");
const findInput = document.getElementById("findInput");
const replaceInput = document.getElementById("replaceInput");
const findStatus = document.getElementById("findStatus");
const findCase = document.getElementById("findCase");
const findWholeWord = document.getElementById("findWholeWord");
const findSelection = document.getElementById("findSelection");
let findScope = null;

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
  // Where the cached stale-caption count is earned: the References tab is the only
  // route to the button, and a tab switch is a deliberate, infrequent interaction —
  // the same budget the Outline panel already spends.
  if (name === "references") { referenceCommands.numbering.refresh(); tocCommands.fields.refresh(); }
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
  // `hidden` as well as `disabled`: a band the HOST withheld is composed away
  // (`docs/126` phase 3), and a tab at `display: none` is still in this array, so
  // without the second test the arrow keys walk onto an invisible tab, `focus()`
  // does nothing, and navigation appears to stop dead.
  const enabled = ribbonTabs.filter((tab) => !tab.disabled && !tab.hidden);
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
// Both name cards go through ONE contract (`name_dialog.mjs`): the staged-result
// dance that keeps Escape from leaving the promise pending is subtle enough that
// a second copy of it is a second place for that bug to live.
const styleNamePrompt = createNamePrompt({
  registerModal,
  fallbackFocus: () => pagesEl,
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
const confirmDialogUi = createConfirmDialog({ registerModal, fallbackFocus: () => pagesEl });
const confirmModal = (options) => confirmDialogUi.ask(options);

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

// Delayed tooltips for the icon-only ribbon controls. The behaviour, the tooltip
// element and the title-parking rule are `ribbon_tooltip.mjs`; it reads no editor
// state at all, which is why it could leave this file.
const ribbonTooltips = installRibbonTooltips([document.querySelector(".ribbon"), ribbonOverflowMenu]);
// Capturing, so a scroll anywhere disarms: a tooltip left floating over the place
// a control used to be is worse than no tooltip.
window.addEventListener("scroll", () => ribbonTooltips.disarm(), true);

undoBtn.addEventListener("click", () => runEdit(() => doc.undo()));
redoBtn.addEventListener("click", () => runEdit(() => doc.redo()));
viewOutlineBtn.addEventListener("click", () => toggleOutline());
const viewZoom = createViewZoom({ stepZoom, setZoom, setZoomMode, zoomState: () => ({ mode: zoomMode, factor: zoomFactor }) });
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
const reflectReviewBanners = createReviewBanners({ suggesting: suggestingBanner, viewing: viewingBanner, viewingText: viewingBannerText });
const reviewSidebar = document.getElementById("reviewSidebar");
const reviewSidebarBody = document.getElementById("reviewSidebarBody");
const reviewSidebarHeader = document.getElementById("reviewSidebarHeader");
const reviewMarginCommentBtn = document.getElementById("reviewMarginComment");
// The host's grant, resolved once: a `readonly` embed is Viewing ON ARRIVAL,
// not an Editing page with disabled buttons. See `capabilities.mjs`.
const HOST_CAPS = hostCapabilities();
/** Whether this CONTAINER may ever change the document — the host's grant, not
 *  the review mode: a mode is reversible and explains itself, a grant is final
 *  and is composed away silently. */
const containerCanEdit = HOST_CAPS.has("edit") || HOST_CAPS.has("comment");
const HOST_MODE = editingModeFor(HOST_CAPS);
/** The ROOM's grant — what THIS PARTICIPANT may do, as against what this container may. `session_access.mjs`. */
const SESSION = sessionAccess(hostConfig(), (key) => t(key));
/** Which chrome this container paints (`docs/126` phase 3), in both of its shapes:
 *  the container's own, and the same container with its editing chrome composed
 *  away. Resolved from the same URL the capability set came from, applied BEFORE
 *  first paint, and a different question from the capability set — a withheld
 *  region is a presentation decision, a withheld capability is a permission. */
const HOST_CHROME = hostChrome();
/** Composes the chrome for the document on screen: the reading chrome whenever it
 *  cannot be edited, the container's own otherwise.
 *
 *  Two things mean that, asked as one question: `readOnlyReason` (the engine
 *  refusing the document, or a version preview standing in for it) and Viewing
 *  MODE. The mode was left out at first, on the reading that it is "not right
 *  now" rather than "never, for you" — but a reader in Viewing mode cannot change
 *  the document by any route, and a full ribbon over it is eight bands of controls
 *  every one of which refuses; Google Docs collapses its toolbar there for that
 *  reason. The mode control is in the footer's `status` region, which reading
 *  chrome keeps, so the way back is on screen throughout.
 *
 *  Leaving restores exactly what was there, the active ribbon tab included:
 *  `applyRegions` re-selects a band only when the one on screen is stranded, and
 *  reading chrome has no band to move to, so `aria-selected` is untouched.
 *  O(regions). */
function reflectChrome() {
  applyRegions({
    body: document.body,
    root: document,
    regions: composedChrome(),
    selectBand: (band) => selectRibbonTab(band),
  });
}
/** The region set in force right now — the same object `reflectChrome` hands the
 *  stylesheet, so a gesture and a rule cannot disagree. O(1). */
function composedChrome() {
  return !readOnlyReason && reviewMode !== "viewing" ? HOST_CHROME.editing : HOST_CHROME.reading;
}
/** Whether this container paints `region` at all.
 *
 *  THE GATE, and the reason there is one: `display: none` takes a thing off the
 *  screen without taking the GESTURE away. A withheld `objects` region that only
 *  hid the outline would leave a click still selecting the image and Delete still
 *  aimed at it — "present but unreachable" (`docs/99` §9.4). */
function chromeShows(region) {
  return composedChrome().has(region);
}
/** The host contract (`docs/126` phase 2), built at the END of this file because
 *  its command registry cannot exist until everything below is declared, and
 *  declared HERE because the hooks that feed it are scattered up the file. Every
 *  hook is `hostSession?.`: a status published during boot is not reported, which
 *  is correct — no host has been handed the session yet. */
let hostSession = null;
let reviewMode = HOST_MODE;
/** Why this DOCUMENT cannot be edited at all, or "" when it can be.
 *
 *  The engine answers this (`editingUnavailableReason`), and exactly one
 *  document in the product says anything: one too large to lay out whole, which
 *  the viewer opens one page-window at a time (`docs/113` §8.3/§8.7). Unlike
 *  Viewing mode — a choice the reader can reverse — this one cannot be switched
 *  off, which is why the mode buttons are disabled and the banner loses its
 *  "Switch to editing" escape. */
let readOnlyReason = "";
reflectChrome(); // before first paint, and `readOnlyReason` had to exist first
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
// The responsive ladder — both rungs, the `phone-mode` class and the soft
// keyboard's inset — is `phone_chrome.mjs` (docs/148). Crossing a rung swaps
// the layout SHAPE, so it needs a re-render, not a repaint.
const phoneChrome = createPhoneChrome({
  view: window,
  body: document.body,
  root: document.documentElement,
  // The header's real height, which `--h-header: 63px` is not at this rung: the
  // phone's menu bar wraps, so the header is two rows at 390px and three at
  // 320px, and a German menu bar wraps where an English one does not.
  header: document.querySelector("header.bar"),
});
/** True while the review surface should render as a bottom sheet (HF-088). */
const reviewSheetMode = () => phoneChrome.reviewSheet();
phoneChrome.onReviewSheetChange(() => scheduleReviewMarginRender());

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
  // A document the engine will not let anyone edit cannot offer Editing or
  // Suggesting: the buttons are disabled WITH the reason, not left live. And the
  // banner's "Switch to editing" is withdrawn rather than disabled — an offer
  // that can never be taken is a dead control. Ahead of the `doc` guard: a
  // container's grant is knowable with no document open.
  reflectReviewModeAccess({
    buttons: reviewModeButtons,
    bannerEdit: viewingBannerEdit,
    capabilities: HOST_CAPS,
    readOnlyReason,
    withheldReason: t("capability.embedded"),
    participant: SESSION.modeAuthority,
  });
  if (!doc) return;
  let count = 0;
  try { count = (JSON.parse(doc.listRevisions()) ?? []).length; } catch { count = 0; }
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
function setReviewMode(mode, { restoreFocus = true } = {}) {
  const previous = reviewMode;
  // A read-only document has one mode. Asked for another — by a shortcut, the
  // palette, or a stale click — it stays where it is and says why.
  if (readOnlyReason && mode !== "viewing") {
    setStatus(readOnlyReason, "error");
    mode = "viewing";
  }
  reviewMode =
    mode === "suggesting" ? "suggesting" : mode === "viewing" ? "viewing" : "editing";
  // The banners announce a MODE, so they show only where a mode can change: in a
  // container granted neither `edit` nor `comment` the mode IS the container, and
  // composition is silent (`docs/126`).
  reflectReviewBanners(containerCanEdit ? reviewMode : "", readOnlyReason);
  // The banner's "Switch to editing" offer is `updateReviewControls`'s, below,
  // so the segments and the banner cannot disagree about whether editing is possible.
  reflectChrome(); // every edge — `readOnlyReason`, a preview's two, and the MODE
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
  // canvas editor owns focus. `restoreFocus: false` is for callers where the mode
  // change is not the user's gesture at all — opening a document, a demo preset,
  // entering and leaving a version preview — because those arrive while the user
  // is working somewhere else (the version TIMELINE above all, which is how a
  // preview is left again); moving focus out from under them there is not a
  // safety measure, it is losing their place.
  if (restoreFocus) focusEditorSurface();
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

/** The right-margin comment affordance (`review_chrome.mjs`). Placed against the
 *  caret's line — or a selection's FIRST line, endpoints put in document order
 *  so a backwards drag still anchors where the selection starts. A collapsed
 *  selection falls through to `caretRect`, which is what keeps the button on the
 *  page while nothing is selected: dimmed, since `review.comment` needs a
 *  range. */
const commentAffordance = createCommentAffordance({
  button: reviewMarginCommentBtn,
  viewport: viewportEl,
  rect: () => {
    if (!doc || !selection) return null;
    const [start, end] = orderedSelectionEnds(selection, doc);
    return reviewRangeClientRect(start.node, start.offset, end.node, end.offset);
  },
});

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
    commentAffordance.sync(false);
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
  // The margin's affordance belongs to the margin's EMPTY state: shown exactly
  // when the column is not. Deliberately NOT also gated on `reviewSheetMode()`
  // — at the sheet's widths the page is already wider than the window, so
  // `commentAffordanceSpot` refuses on the geometry first. See its note.
  // …and only where this container annotates at all: the margin button is
  // `review.comment`'s second face, and a preview is a picture of a document
  // rather than a place to say something about one.
  commentAffordance.sync(!show && HOST_CAPS.has("comment"), bandOffset);
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
      // Through the catalogue: the ribbon's own Add comment button already has
      // this sentence translated into all eighteen languages, and a second,
      // English-only spelling of one name is how two surfaces start to disagree.
      composer.setAttribute("aria-label", t("panelReview.addComment.label"));
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
        await runEdit(() => doc.addComment(start.node, start.offset, end.node, end.offset, text, undefined, undefined, metadata.date), { keepView: true });
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
          await runEdit(() => doc.setCommentResolved(item.data.id, resolving), { keepView: true });
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
            await runEdit(() => doc.deleteComment(item.data.id), { keepView: true });
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
                await runEdit(() => doc.updateComment(reply.id, text), { keepView: true });
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
          await runEdit(() => doc.replyToComment(item.data.id, text, undefined, undefined, metadata.date), { keepView: true });
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
    commentAffordance.ride(bandOffset);
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
// The two zoom steppers are looked up where they are USED, below: one reader
// each, and a name in the widest scope in the product for it.
const documentChrome = document.getElementById("documentChrome");
// The menu bar is the ONLY entry point now that Open has left the header, so it
// can no longer wait for a document — a fresh editor would otherwise have no
// keyboard- or pointer-reachable way to open or create one. The `noDoc` commands
// (New, Open) are the only ones enabled until a document exists; every other row
// renders disabled, which is the honest state rather than a hidden one.
if (documentChrome) documentChrome.hidden = false;
const docTitleEl = document.getElementById("docTitle");
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
/** The shared session's byte pipe, or `null` in the standalone mode (`152` §2a). */
let collab = null;
/** Arrivals paint one at a time: two must not interleave two `renderAll()`s. */
let arrivalPaint = Promise.resolve();
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
/** Active image-crop session — the Word/Docs-style direct-manipulation crop: the
 * selected image shows crop handles + a dimmed overlay of the region being cut,
 * dragged live and committed as ONE `SetImageCrop` op on Enter / click-away.
 * `{ node, box:[x,y,w,h twips], crop:{l,t,r,b fractions}, handleDrag }` or null. */
let objectCropSession = null;
/** Active floating-object move drag (docs/85 §5.3): a body drag that previews an
 * outline and commits ONE `SetAnchor` (position) on release. `null` when idle. */
let objectMoveDrag = null;
/** Current table-cell selection overlay, separate from text ranges. */
// The table CELL SELECTION (`docs/141` D-3). One rectangle, `{ anchorNode,
// focusNode }`, for every shape of table selection there is — a drag, a row, a
// column, the whole table — because three degenerate rectangles and a real one
// are one type, not two (`table_range.mjs`).
const tableRange = createTableRange({
  doc: () => doc,
  pages: () => pages,
  scaleOf,
  runEdit: (fn, options) => runEdit(fn, options),
  status: (text, kind) => setStatus(text, kind),
  t,
});
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
  // A container with no `caret` region has no keyboard surface on the document
  // either: arrows, typing and copy all funnel through this one predicate. A
  // selection nobody can see that Shift+Arrow still extends and ⌘C still copies
  // is the same "present but unreachable" failure in the opposite coat.
  if (!chromeShows("caret")) return false;
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
  statusChannel.publish(text, kind);
  // The host contract listening on the one feedback channel: a refusal a person
  // sees must also be a refusal a host can hear, or an embedding host re-issues
  // it forever (`docs/126` phase 2).
  hostSession?.noteStatus(text, kind);
  if (text && timeout > 0) {
    statusClearTimer = window.setTimeout(() => {
      statusEl.textContent = "";
      statusEl.className = "status";
      statusClearTimer = 0;
    }, timeout);
  }
}

function setDocumentState(state) {
  paintDocumentState(documentStateBadge(state), document.getElementById("documentState"), documentStateText);
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
    // The single most visible place our brand leaked into someone else's product
    // (`docs/125` §2 F4: "the host page's own browser tab title carries our
    // brand"). `brand.json` decides, and `tabTitle: "document"` drops it.
    product: BRAND.tabTitle === "document" ? "" : BRAND.name,
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
  const flat = selection ? doc.caretRect(selection.focus.node, selection.focus.offset) : [];
  const cur = flat.length ? flat[0] : 1;
  const total = pageTotalLabel(pages.length, doc.estimatedPageCount, doc.pageCountIsExact);
  statPages.textContent = pageIndicator(cur, total, reflowView.isOn() ? readerPosition(flat, pageBandModel, pages) : null);
  pagesPanelView.reflect(cur);
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
    // The engine is only now instantiated. `createMeasurementUnits` runs during
    // module evaluation -- long before this line -- so its roster is faulted in
    // rather than read at construction, and THIS is the call that fills the
    // chooser. Reaching for a free function on an uninstantiated wasm module
    // throws `Cannot read properties of undefined (reading
    // '__wbindgen_add_to_stack_pointer')`, and a throw during module evaluation
    // takes the whole editor down: nothing after it runs, so the document never
    // opens at all.
    measurement.reflect();
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
  // Last of all: a restore a killed tab left prepared, and the age window. Both
  // are boot-time work by nature (`docs/140` §7.5) and neither blocks anything
  // above — a timeline is not what the user is waiting for.
  versionHistory.resume();
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
        if (preset.review) setReviewMode(preset.review, { restoreFocus: false });
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
    // A different document, so the remembered object answer belongs to a
    // document that no longer exists.
    objectPresence.forget();
    currentSourceFormat = doc.sourceFormat;
    applyActiveAuthorToDocument();
    // The room's grant into the engine that ENFORCES it, per DOCUMENT because a new document is a new minting base.
    const grantProblem = SESSION.adopt(doc) || SESSION.problemMessage();
    // The shared session, per DOCUMENT and for the same reason the grant is: a
    // new document is a new minting base for the NodeIds an op names.
    collab?.stop();
    collab = openRoom(doc, hostConfig().room, globalThis.WebSocket, {
      identity: settings.authorName.trim() || "You",
      resumeKey: resumeKey(globalThis.sessionStorage, () => globalThis.crypto.randomUUID()),
      onOutcome: (o) => { if (o.documentChanged) arrivalPaint = arrivalPaint.then(() => paintArrival(o)); },
      onState: (s) => setStatus(t(s.key), s.name === "connected" ? "" : "warn"),
    });
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
    tableRange.clear();
    objectCropSession = null; // a new document invalidates any in-progress crop
    // Ask the engine, before anything is offered, whether this document can be
    // edited at all: the mode buttons, the banner, the chrome and every refusal
    // message all read this one answer (`docs/113` §8.7).
    readOnlyReason = String(doc.editingUnavailableReason ?? "");
    reviewSidebarPreference = null;
    activeReviewCommentId = null;
    activeReviewItemId = null;
    reviewComposerState = null;
    reviewDeleteConfirmId = null;
    // A new document invalidates every retained card and its cached geometry.
    reviewLayout = [];
    reviewCardCache.clear();
    // The mode this document opens in, through the ONE owner of mode state — which
    // sets `reviewMode`, both banners, the pressed states, the tracking flag and the
    // composed chrome, so none can drift. It used to be called only for a non-editing
    // mode, with four of those five done again by hand beside it; the case that could
    // not cover is an editable document REPLACING a read-only one, which kept the old
    // mode and, now that chrome follows it, the reading chrome. Skipped only where
    // nothing can have changed — editing to editing.
    // ...narrowed by the ROOM's grant too: a read-only guest arrives in Viewing, as a `readonly` container does.
    const openMode = readOnlyReason ? "viewing" : SESSION.openMode(HOST_MODE);
    if (openMode !== "editing" || reviewMode !== "editing") setReviewMode(openMode, { restoreFocus: false });
    breakTypingSession();
    currentName = name;
    docTitleEl.value = name;
    documentChrome.hidden = false;
    resetDirtyTracking(currentName);
    // The identity a draft records, computed from the bytes the document was
    // opened from. O(1) in document size — see `documentKey`.
    adoptDraftDocument(name, bytes);
    // And the timeline this document rejoins: `adopt` mints or finds the lineage
    // and records the import baseline (`docs/140` §7.5). See `activatingRestore`
    // for why a restore skips it.
    if (!activatingRestore) void versionHistory.adopt();
    // Ignored words and cached paragraphs belong to the document that is being
    // replaced; the personal dictionary and the fetched word lists do not, and
    // are kept. Reading the personal words is one IndexedDB round trip and
    // never blocks the open.
    spellChecker.reset();
    void spellChecker.loadPersonal();
    setDocumentState("opened");
    if (grantProblem) setStatus(grantProblem, "error"); // never swallowed: a silent narrowing looks like a working read-only mode
    if (saveBtn) saveBtn.disabled = false;
    populateSaveFormats(saveFormatEl, doc ? doc.availableExportFormats() : [], currentSourceFormat, document);
    // What the IMPORT lost, not a cleared slate. `importReportJson` was a shipped
    // engine getter with zero consumers: loss was computed on every open and
    // thrown away, while export loss was reported. SKILL §1 names reporting as
    // the condition under which verbatim retention is an advantage at all.
    showCompatibilityFindings(compatibilityStatusEl, importFindingCount(doc.importReportJson), "import");
    railOutline.disabled = railPages.disabled = false;
    reflowView.setEnabled();
    // The reader's formatting-mark preference, replayed onto the new handle: the
    // PREFERENCE is the person's and outlives the document, while the STATE lives on
    // the document because that is where the renderer reads it. Idempotent and O(1)
    // when the two already agree, which is the common case.
    formattingMarks.adopt();
    versionHistory.reflect();
    populateStyles();
    populateTableStyles();
    document.getElementById("drop").hidden = true;
    document.body.classList.add("doc-loaded");
    hostSession?.noteReady();
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
    // Fit a page too wide for its container (`view_zoom.mjs`); after `onOpened`, so a caller's zoom wins.
    zoomMode = openingZoomMode(computeFitZoom("fit-width"), { mode: zoomMode, factor: zoomFactor });
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
    referenceCommands.numbering.refresh();
    tocCommands.fields.refresh();
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
    pagesPanelView.sync();
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
  // Logical CSS px per twip at this zoom — independent of devicePixelRatio, so
  // the page box geometry (hence scroll height and hit-test scale) is stable.
  const cssPerTwip = (BASE_DPI * zoom) / TWIPS_PER_INCH;
  // The width feed (`docs/151` §6.2). AFTER the zoom, which the measure depends
  // on; BEFORE `pageCount`, which in reflow it decides. O(1) unless the
  // quantised width actually moved — `reflow_view.mjs` says why.
  const reflowing = reflowView.sync(cssPerTwip);
  const count = doc.pageCount;

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
  // `gap: 0` in reflow: the 22px pitch is the desk between two SHEETS, and a tile
  // is cut mid-paragraph, so the same gap there is a band across a sentence.
  pageBandModel = buildPageBand(sizes, cssPerTwip, { gap: reflowing ? 0 : PAGE_GAP_PX, maxScroll: MAX_SCROLL_PX });
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
  pagesEl.replaceChildren(rulerView.element, bandEl); // sits above the pages, same width
  rulerView.build();
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
    pagesPanelView.build();
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
  const best = smallestContaining(reviewAnchorIndex, anchor.node, Number(anchor.offset) || 0);
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

// ---- Proofing (docs/114, docs/146, `109` HF-035) -----------------------------
// The coordinator, the two switches, the one replacement path and the language
// packs are all `proofing_chrome.mjs`; what is here is this application's answers
// to the questions it asks. The three hooks are the whole of proofing's contact
// with the editor: `paint()` from the overlay repaint, `noteEdited()` from the
// edit choke point (O(1) — it re-arms a timer and nothing else), and
// `noteWindowChanged()` from the scroll that moves the page window.
const proofing = createProofingChrome({
  getDoc: () => doc,
  pages: () => pages,
  pageWindow: () => pageWindow,
  place,
  caret: () => (selection ? selection.focus : null),
  preference: (key) => settings[key],
  setPreference: (key, value) => {
    settings[key] = value;
    saveSettings();
  },
  toggles: () => ({ spell: spellCheckToggle, grammar: grammarCheckToggle }),
  status: (text, kind) => setStatus(text, kind),
  repaint: () => paintOverlayLayer(),
  redraw: () => drawSelection(),
  reflect: () => updateToolbar(),
  openWords: () => openWordStore({}),
  runEdit,
  registerModal,
  fallbackFocus: () => pagesEl,
  activeLocale: () => activeLocale(),
  indexedDB: globalThis.indexedDB,
  stamp: new URL(import.meta.url).search,
  origin: () => location.origin,
});
const spellChecker = proofing.checker;
const { replaceMisspelling, setSpellCheckEnabled, setGrammarCheckEnabled } = proofing;

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
  // A repaint is the only thing that can move a table boundary, so it is the only
  // place the chrome layer's geometry cache must be dropped — which is what makes a
  // pointer-move a lookup rather than an engine query. AFTER `clearOverlays`,
  // because the same call puts an in-flight drag's guide back (`docs/141` D-1).
  tableChrome.invalidate();
  // The cell range's memo has the same lifetime and the same reason: a repaint
  // is the only thing that can change which cells a pair of endpoints covers.
  tableRange.invalidate();
  paintReviewMarkers();
  paintChecklistMarkers();
  spellChecker.paint();
  // A selected object owns the visible chrome (outline + handles) in place of the
  // text caret; in "editing" mode the ordinary text caret is shown instead.
  if (objectSelection && objectSelection.mode === "selected") {
    paintObjectSelection();
  } else if (selection) {
    paintTableSelection();
    // ONE `cellRect` for the whole table chrome. It used to be asked twice — once
    // for the active-cell outline and once inside the handle query's own
    // `inTable` — and it is a page scan each time.
    const cellRect = doc.cellRect(selection.focus.node); // [page, x, y, w, h] or []
    paintActiveCell(cellRect); // under the caret/highlight
    paintSelection(selection);
    tableChrome.paintCaretColumnHandles(pages, cellRect);
    tableChrome.paintTouchPills(pages);
    touchSelection.paint(pages); // the finger's handles, same layer, same reason
  }
  // Outside the `selection` branch: a strip is armed by the POINTER, and being
  // over a table is not a reason to require a caret.
  tableGutter.paint(pages);
}

function drawSelection() {
  if (!doc) return;
  paintOverlayLayer();
  foldView.syncBodyChevron(); // after the overlay repaint, which clears it
  updateObjectSelectionState();
  updateObjectContextBar();
  updateToolbar();
  updateReviewControls();
  scheduleReviewMarginRender();
  updatePageNumber();
  if (tableRange.mirrorChanged()) scheduleChromeRefresh({ a11y: true });
  rulerView.syncToCaret();
  positionSelToolbar();
  hostSession?.noteSelection();
}

function paintTableSelection() {
  const rects = tableRange.rects();
  for (let i = 0; i + 4 < rects.length; i += 5) place(rects.slice(i, i + 5), "table-cell-selection");
}

/** Outlines the table cell the caret is in (nothing when not in a table), so the
 *  user always sees which cell they are editing. */
function paintActiveCell(flat) {
  if (flat.length >= 5) place(flat, "cell-outline");
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
  const rect = doc.objectRect(node); // [page, x, y, w, h] twips
  const outlineEl = place(rect, "object-outline");
  // The objects held alongside it (Ctrl/⌘+click), so a multi-object selection
  // is visible before Group is pressed rather than being a state only the menu
  // knows about. One rect per HELD member — a user-picked handful, on a repaint,
  // never on a pointermove — which is why this is a loop and the object chip's
  // arrange gather is not.
  for (const member of objectArrange.multiSelect()) {
    // By SUBJECT: `objectRect` is answered for the subject, and a root id
    // answers nothing at all — which is why holding only the root drew no
    // outline for the second object.
    place(doc.objectRect(member.node), "object-outline is-co-selected");
  }
  // A movable object's BODY needs `touch-action: none` the way its grips
  // already have it, or the browser starts scrolling at touch-start and the
  // move gesture is gone before any handler runs. Selected + movable only.
  const padPage = rect.length >= 5 ? pages[rect[0] - 1] : null;
  if (objectSelection.canMove && padPage?.overlay) {
    paintMovePad(padPage.overlay, rect.slice(1, 5), scaleOf(padPage), (event) =>
      startObjectMove(event, padPage, node));
  }
  // The angle comes with the frame, in ONE engine read: the grips are drawn at
  // the object's rotated corners, the cursors are turned with them, and the
  // rotation grip is pushed out along the object's own axis.
  const turned = (doc.objectFrame?.(node) ?? [])[5] ?? 0;
  const rotationDegrees = turned / 1000;
  // The outline is TURNED with the object rather than redrawn as its
  // axis-aligned bounding box: a box that is bigger than the object on every
  // side is not an outline of it, and Word, Docs and ONLYOFFICE all draw the
  // turned rectangle.
  if (rotationDegrees && outlineEl) outlineEl.style.transform = `rotate(${rotationDegrees}deg)`;
  paintResizeHandles(
    (pageNumber) => {
      const page = pages[pageNumber - 1];
      return page?.overlay ? { overlay: page.overlay, scale: scaleOf(page) } : null;
    },
    doc.objectHandles(node), // [page, cx, cy, kind] * supported handle count
    (event, pageNumber, kind) => objectResize.start(event, pages[pageNumber - 1], node, kind),
    {
      rotationDegrees,
      rotateLabel: t("object.rotate.handle"),
      rotateValueText: t("object.rotate.degrees", { degrees: Math.round(rotationDegrees) }),
      onRotateDown: objectSelection.canRotate
        ? (event, pageNumber) => objectRotate.start(event, pages[pageNumber - 1], node)
        : null,
      onRotateKey: (event) => objectRotate.onKey(event),
    },
  );
}

// ---- Image crop (direct-manipulation, Word/Docs standard) -------------------
// The chrome, the clamping rule and the crop's own prose are in
// `object_crop_chrome.mjs`. What stays here is entering, committing and
// cancelling the MODE: crop is picture-only and, like resize, is not
// representable as a tracked revision, so it is blocked in Suggesting/Viewing.
// Double-clicking the picture is the doorway (Docs' gesture); Crop is surface 2.

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

/** Paints the crop chrome — the image outline, the dimmed region being cut, the
 *  kept rectangle with its live size, and eight draggable grips. The geometry
 *  and the clamping rule live in `object_crop_chrome.mjs`; what stays here is
 *  resolving which page and which scale, which only the editor can answer. */
function paintObjectCrop() {
  const s = objectCropSession;
  const [bx, by, bw, bh] = s.box;
  const rectFlat = doc.objectRect(s.node);
  if (rectFlat.length < 5) return;
  const pageNumber = rectFlat[0];
  const page = pages[pageNumber - 1];
  if (!page?.overlay) return;
  place([pageNumber, bx, by, bw, bh], "object-outline");
  const kept = keptRect(s.box, s.crop);
  paintCropChrome(page.overlay, s.box, s.crop, scaleOf(page), {
    sizeLabel: sizeLabel(kept.w, kept.h),
    onGripDown: (event, kind) => startCropHandleDrag(event, page, kind),
  });
}

/** Begins dragging a crop handle. The kept rectangle updates live; the model is
 *  untouched until commit. */
function startCropHandleDrag(event, page, handleKind) {
  if (!objectCropSession) return;
  objectCropSession.handleDrag = {
    handleKind,
    page,
    startClientX: event.clientX,
    startClientY: event.clientY,
    startCrop: { ...objectCropSession.crop },
  };
  beginGripDrag(event, {
    onMove: updateCropHandleDrag,
    onEnd: () => {
      if (objectCropSession) objectCropSession.handleDrag = null;
    },
  });
}

/** Updates the kept rectangle from a crop-grip drag. The clamping rule is
 *  `cropFromDrag`; the repaint carries the new kept size with it. */
function updateCropHandleDrag(event) {
  const s = objectCropSession;
  if (!s || !s.handleDrag) return;
  const drag = s.handleDrag;
  const [, , bw, bh] = s.box;
  const { sx, sy } = scaleOf(drag.page);
  const dxFrac = bw > 0 ? (event.clientX - drag.startClientX) / sx / bw : 0;
  const dyFrac = bh > 0 ? (event.clientY - drag.startClientY) / sy / bh : 0;
  s.crop = cropFromDrag(drag.startCrop, drag.handleKind, dxFrac, dyFrac);
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

/** The two direct-manipulation geometry gestures. Both are modules: a grip
 *  drag that now reasons in the object's own space, and the rotation handle.
 *  What stays here is the application state they read and the gated edit path
 *  they write through. */
const gestureIo = {
  doc: () => doc,
  selection: () => objectSelection,
  scaleOf: (page) => scaleOf(page),
  runEdit: (thunk, options) => runEdit(thunk, options),
  setStatus: (text, kind) => setStatus(text, kind),
  t,
  drawSelection: () => drawSelection(),
  focusEditorSurface: () => focusEditorSurface(),
  reviewMode: () => reviewMode,
  blockMutationInViewing: () => blockMutationInViewing(),
};
const objectResize = createObjectResizeDrag({
  ...gestureIo,
  // A GETTER, not the module: `measurement` is constructed further down this
  // file, so naming it in this object literal would read it in its temporal
  // dead zone. The drag only asks during a pointer move, by which time it
  // exists.
  measure: () => measurement,
  snapContextFor: (page) => snapContextFor(page),
  paintSnapGuides: (drag, guides) => paintSnapGuides(drag, guides),
  clearSnapGuides: (drag) => clearSnapGuides(drag),
  hideLinkChip: () => hideLinkChip(),
  resetPointerGesture: () => resetPointerGesture(),
});
const objectRotate = createObjectRotateDrag(gestureIo);
const sizeLabel = (widthTwip, heightTwip) =>
  resizeSizeLabel(t, widthTwip, heightTwip, measurement);

/** Reflects the object-selection state onto `#pages` as data attributes so the
 *  host (and tests) can observe the grammar state machine without reading into
 *  the overlay. */
function updateObjectSelectionState() {
  reflectObjectSelection(pagesEl, objectSelection, OBJECT_CAPABILITY_KEYS);
  reflectShapeFormatState();
}

/** Reflects a selected shape's own fill and outline onto `#pages`. */
function reflectShapeFormatState() {
  reflectShapeFormat(pagesEl, objectSelection?.kind === "shape" ? selectedShapeFormat() : null);
}

/** The object properties panel. Its 170 lines are `object_inspector.mjs`; what
 *  stays here is the application state it reads and the gated edit path it
 *  writes through. */
const objectInspector = createObjectInspector({
  doc: () => doc,
  selection: () => objectSelection,
  runEdit: (thunk, options) => runEdit(thunk, options),
  setStatus: (text, kind) => setStatus(text, kind),
  openShapeFill: () => shapeFillBtn.click(),
  openShapeOutline: () => shapeOutlineBtn.click(),
});
const toggleObjectInspector = (open) => objectInspector.toggle(open);

/** The floating object chip. Its rendering is `object_bar.mjs`; what stays here
 *  is the application state it renders and the verbs it calls. */
const objectBar = createObjectBar({
  doc: () => doc,
  selection: () => objectSelection,
  pages: () => pages,
  scaleOf: (page) => scaleOf(page),
  viewportRect: () => viewportEl.getBoundingClientRect(),
  t,
  croppingNode: () => objectCropSession?.node ?? null,
  arrangeState: () => arrangeState(),
  ensureInspector: () => objectInspector.ensure(),
  toggleInspector: (open) => objectInspector.toggle(open),
  inspectorOpen: () => objectInspector.isOpen(),
  reflectInspector: () => objectInspector.reflect(),
  setWrap: (mode) => setObjectWrap(mode),
  openAltText: () => openAltTextDialog(),
  enterCrop: () => enterCropMode(),
  deleteObject: () => deleteSelectedObject(),
  reflectShapeSwatches: () => reflectShapeSwatches(),
  fillButton: () => shapeFillBtn,
  outlineButton: () => shapeOutlineBtn,
  positionButton: () => labelledObjectMenuButton(objectPositionBtn),
  arrangeButton: () => labelledObjectMenuButton(objectArrangeBtn),
  rotateButton: () => labelledObjectMenuButton(objectRotateBtn),
});
const updateObjectContextBar = () => objectBar.update();
const positionObjectContextBar = () => objectBar.reposition();

/** Every arrange fact about the selected object, gathered in ONE pass per
 *  repaint: its anchor kind, its wrap, its stacking and its transform.
 *
 *  `objectPosition` and `objectTransform` are each O(document) with one walk.
 *  Asking them here — once, when the selection or the layout changed — is what
 *  keeps the wrap row, the Position gallery and the Rotate menu from each
 *  asking again, which is the per-object-query-in-a-loop shape that has already
 *  cost this repository a quadratic. Nothing on a pointermove path calls this. */
function arrangeState() {
  const selection = objectSelection;
  if (!doc || !selection || selection.mode !== "selected") return null;
  const root = selection.ref.root;
  const read = readPosition(doc.objectPosition?.(root) ?? "");
  if (!read.object) return null;
  return {
    root,
    node: selection.node,
    read,
    wrap: read.floating ? doc.objectWrap(root) : "",
    transform: readTransform(doc.objectTransform?.(selection.node) ?? ""),
  };
}

/** The Arrange commands — position, stacking, grouping, rotation, text in a
 *  shape. `object_arrange_commands.mjs`; what stays here is the state they read
 *  and the gated edit path they write through. */
const objectArrange = createObjectArrangeCommands({
  doc: () => doc,
  selection: () => objectSelection,
  state: () => arrangeState(),
  runEdit: (thunk, options) => runEdit(thunk, options),
  setStatus: (text, kind) => setStatus(text, kind),
  select: (entry) => selectObject(entry.node, entry.kind, null, entry.anchored, entry),
  t,
  // Ctrl on Windows/Linux, ⌘ on a Mac — derived, never spelled, because a spec
  // that asserts a Mac glyph fails on the Linux runner (`105` UX-009).
  modifier: formatShortcut("\u2318"),
});

/** Insert ▸ Shapes arms the pointer; the next gesture on the page draws the
 *  shape. Word's sequence, and every drawing tool's. `shape_draw_mode.mjs`
 *  holds the state machine and the preview; the arithmetic is `shape_draw.mjs`.
 *
 *  O(1) per pointermove: the gesture never touches the document while it runs. */
const shapeDrawMode = createShapeDrawMode({
  pointToTwip: (page, event) => pointToTwip(page, event),
  scaleOf: (page) => scaleOf(page),
  outlineFor: (token) => SHAPE_PRESETS.find((shape) => shape.token === token)?.outline ?? null,
  insert: (token, rect) => void insertShapeObject(token, rect),
  setStatus: (text, kind) => setStatus(text, kind),
  t,
});

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
  "canRotate",
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
  // Handles nobody can see are still handles Delete is aimed at.
  if (!chromeShows("objects")) return;
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
  tableRange.clear();
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
 *  would actually land on — and memoized, because the question is asked on a
 *  per-keystroke path while `objectOrder()` is O(objects) (`109` HF-183; the
 *  whole argument, and the invalidation contract, is in `object_presence.mjs`).
 *
 *  Cost: O(1) per call, O(objects) once per edit. */
const objectPresence = createObjectPresence(() => doc.objectOrder());
function documentHasObjects() {
  if (!doc) return false;
  return objectPresence.has();
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

/** The page every running-content command acts on: the page whose band is open
 *  while editing one, otherwise the page in view. Stays here rather than moving
 *  with the variant toggles, because it reads `runningEditPage` and `pages` — the
 *  application's own state — and is handed to the module as `io.runningPage`. */
function runningEditPageOrView() {
  if (runningEditPage) {
    return pages.find((page) => page.pageNumber === runningEditPage) ?? pageInView();
  }
  return pageInView();
}

/** Whether a section-property change is forbidden right now, having already said
 *  why. The two review gates every page-setup edit shares, in one place: the
 *  modules that own those surfaces take this as `io.blockedFromMutating` rather
 *  than each carrying its own copy of the review policy. */
function blockedFromPageSetup() {
  if (blockMutationInViewing()) return true;
  if (reviewMode === "suggesting") {
    setStatus("Page-setup changes cannot be tracked yet; switch to Editing", "error");
    return true;
  }
  return false;
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
  tableRange.clear();
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

/** Changes a floating object's text-wrap mode (docs/85 §5.3), as one undoable op.
 *  Blocked fail-closed in Suggesting/Viewing mode by `runEdit`'s gate. */
function setObjectWrap(mode) {
  const state = arrangeState();
  if (!state || !wrapAvailability(state.read).available) return;
  const plan = wrapPlan(mode, state.read, state.wrap);
  // An empty plan means the object is already in that mode: a chip that
  // re-asserts the current choice must not fill the undo stack.
  if (plan.length === 0) return;
  runEdit(() => {
    let result = null;
    for (const step of plan) {
      result =
        step.op === "anchorKind"
          ? doc.setObjectAnchorKind(state.root, step.kind)
          : doc.setObjectWrap(state.root, step.mode);
    }
    return result;
  }, { gate: true });
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
// How close a dragged edge comes before the page pulls it onto a line, in CSS
// pixels — converted to twips at the CURRENT zoom, so the pull is the same
// distance under the finger at 50% as at 200%.
const SNAP_TOLERANCE_PX = 7;

/** The alignment lines `page` offers, and the snap radius in that page's twips.
 *  Called ONCE per gesture (at pointer-down), never per pointer move: it asks
 *  the engine for the page's ruler geometry, and a drag that re-asked would be
 *  paying an engine call per sample for an answer that cannot change. */
function snapContextFor(page) {
  let targets;
  try {
    const g = doc.pageRulerGeometry(page.pageNumber - 1);
    targets = pageSnapTargets({
      widthTwip: g.widthTwip,
      heightTwip: page.hTwip,
      marginStartTwip: g.marginStartTwip,
      marginEndTwip: g.marginEndTwip,
    });
    g.free?.();
  } catch {
    return null; // no geometry, no guides — the drag is simply free
  }
  const { sx } = scaleOf(page);
  return { targets, tolerance: sx > 0 ? SNAP_TOLERANCE_PX / sx : 0 };
}

/** Draws (or hides) the alignment guides for a live drag, on the drag's page. */
function paintSnapGuides(drag, guides) {
  drag.guideEls = paintGuides(drag.page.overlay, drag.guideEls, guides, scaleOf(drag.page));
}

/** Removes a gesture's guides. */
function clearSnapGuides(drag) {
  clearGuides(drag?.guideEls);
  if (drag) drag.guideEls = null;
}

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
    startW: w,
    startH: h,
    lastX: x,
    lastY: y,
    moved: false,
    preview,
    snap: snapContextFor(page),
    guideEls: null,
  };
  // The move gesture takes the pointer the way the resize gesture already does:
  // a touch drag otherwise never became a move at all (the browser had already
  // committed to scrolling by the time `updateObjectMove` ran), and a mouse drag
  // died the moment the pointer left the sheet.
  event.preventDefault();
  page.overlay?.setPointerCapture?.(event.pointerId);
}

/** Updates the move preview from the pointer delta (page-local twips), pulled
 *  onto the page's alignment lines when an edge or the centre comes close. */
function updateObjectMove(event) {
  if (!objectMoveDrag) return;
  const drag = objectMoveDrag;
  const { sx, sy } = scaleOf(drag.page);
  const dxTwip = Math.round((event.clientX - drag.startClientX) / sx);
  const dyTwip = Math.round((event.clientY - drag.startClientY) / sy);
  let x = Math.max(0, drag.startX + dxTwip);
  let y = Math.max(0, drag.startY + dyTwip);
  // Alt is the Word/Docs escape hatch: hold it and the page stops pulling, so a
  // deliberate 2mm-off-centre placement is still reachable.
  if (drag.snap && !event.altKey) {
    const snapped = snapBox({ x, y, w: drag.startW, h: drag.startH }, drag.snap.targets, drag.snap.tolerance);
    x = snapped.x;
    y = snapped.y;
    paintSnapGuides(drag, snapped.guides);
  } else if (drag.guideEls) {
    paintSnapGuides(drag, []);
  }
  drag.lastX = x;
  drag.lastY = y;
  if (Math.abs(dxTwip) + Math.abs(dyTwip) > MOVE_THRESHOLD_TWIP) drag.moved = true;
  drag.preview.style.left = `${x * sx}px`;
  drag.preview.style.top = `${y * sy}px`;
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
  clearSnapGuides(drag);
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
  clearSnapGuides(objectMoveDrag);
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
  // A container with no `caret` region paints neither. The insertion point still
  // EXISTS — the engine has to have one — it is just not shown or movable here.
  if (!chromeShows("caret")) return;
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
  if (!page?.overlay) return;
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

// -- Contents entries that were never hyperlinked -----------------------------
// `toc_navigation.mjs` carries the finding, the measured evidence and the rule;
// the navigator holds the two revision-keyed caches that keep a click O(1).
const tocNavigator = createTocNavigator({ doc: () => doc, revision: documentVersion });

/** The heading the contents entry at `node` names, or null. */
const tocEntryTargetAt = (node, offset = null) => tocNavigator.targetAt(node, offset);

/** Follows the contents entry at `node`, if it is one. Returns whether it did. */
function followTocEntry(node, offset = null) {
  const target = tocEntryTargetAt(node, offset);
  if (!target) return false;
  navigateToAnchor(target.node, 0, 0);
  setStatus(t("toc.jumpedTo", { heading: target.label }));
  return true;
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
/** The review-mode flags the pointer paths gate on, in one place: `editsBlocked`
 *  is Viewing (or read-only), `geometryBlocked` is anything but Editing, because
 *  a structural change has no tracked-change representation. */
const pointerModeState = () => ({
  editsBlocked: reviewMode === "viewing" || !!readOnlyReason,
  geometryBlocked: reviewMode !== "editing" || !!readOnlyReason,
});

// The table chrome layer (`docs/141` D-1). It owns every pointer, keyboard and
// touch gesture on a table BOUNDARY; `main.js` only routes events into it.
const tableChrome = createTableChrome({
  doc: () => doc,
  pointToTwip,
  scaleOf,
  state: pointerModeState,
  runEdit: (fn, options) => runEdit(fn, options),
  status: (text, kind) => setStatus(text, kind),
  t,
  repaint: () => drawSelection(),
});

// The table GUTTER (`docs/141` D-2). It owns the strips beside the table, the
// hover insert discs and the drag that selects several rows or columns; it reads
// the SAME memoised page chrome the boundary layer does, so a hover still makes
// no engine call.
const tableGutter = createTableGutter({
  doc: () => doc,
  chromeOf: (page) => tableChrome.chromeOf(page),
  pointToTwip,
  scaleOf,
  pageFromClientPoint,
  pages: () => pages,
  state: pointerModeState,
  range: tableRange,
  runEdit: (fn, options) => runEdit(fn, options),
  status: (text, kind) => setStatus(text, kind),
  t,
  repaint: () => drawSelection(),
  repaintOverlay: () => paintOverlayLayer(),
});

const pointerHover = createPointerHover({
  doc: () => doc,
  pages: () => pages,
  materializedPages,
  pointToTwip,
  linkAt,
  tocEntryAt: (node, offset) => !!tocEntryTargetAt(node, offset),
  pointInsideObject,
  objectCapabilities,
  tableBoundaryAt: (page, event) => tableChrome.targetKind(page, event),
  tableGutterAt: (page, event) => tableGutter.targetKind(page, event),
  state: () => ({
    formatPainting: !!formatPainter,
    ...pointerModeState(),
    inRunningStory: !!runningEditBand,
    insideObjectNode: objectSelection?.mode === "editing" ? objectSelection.node : null,
    resizeDrag: objectResize.record(),
    cropDrag: objectCropSession?.handleDrag ?? null,
    moveDrag: objectMoveDrag,
    tableDrag: tableChrome.dragKind(),
    tableStripDrag: tableGutter.dragKind(),
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
    setStatus(t("toc.jumpedTo", { heading: tocNavigator.headingAt(link.targetNode) ?? link.anchor }));
    return true;
  }

  followExternalTarget(link.url, setStatus);
  return true;
}

/** Touch selection — the long press, the two handles, the loupe and the caret
 *  drag (`105` UX-018, `148` §9 item 2). Every tunable and the whole gesture
 *  live in the module; this is all the shell owes it. */
const touchSelection = createTouchSelection({
  view: window,
  surface: pagesEl,
  mount: document.body,
  makeEl: (tag) => document.createElement(tag),
  enabled: () => !!doc && chromeShows("caret"),
  pageAt: (x, y) => pageFromClientPoint(x, y),
  scaleOf,
  anchorAt,
  caretRect: (at) => doc.caretRect(at.node, at.offset),
  wordAt: (node, offset) => doc.wordAt(node, offset),
  selection: () => selection,
  setSelection: (next) => void (selection = next),
  draw: drawSelection,
  focus: focusEditorSurface,
  cancelGesture: resetPointerGesture,
});

function onPointerDown(page, event) {
  if (event.button !== 0) return;
  // No caret region: a press neither places an insertion point nor starts a
  // drag-selection. Scrolling and zooming are not gestures on the document.
  // This precedes the shape gate deliberately — a container offered no caret is
  // the page and nothing else, so it must not begin drawing one either.
  if (!chromeShows("caret")) return;
  // An armed shape owns the gesture: this press is the first corner of the
  // rectangle being drawn, not a caret placement.
  if (shapeDrawMode.tryBeginDraw(page, event)) return;
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
    // Ctrl/⌘+click ADDS to the selection instead of replacing it — Word,
    // PowerPoint and Docs all build a multi-object selection this way, and it is
    // the only way to reach Group with more than one object. The primary
    // selection stays the object clicked first; the modifier click keeps it and
    // records the new one alongside.
    if ((event.metaKey || event.ctrlKey) && objectSelection?.mode === "selected"
        && objectSelection.ref.root !== descriptor.root) {
      objectArrange.toggleMember(descriptor);
      setStatus(t("object.addedToSelection"));
      drawSelection();
      event.preventDefault();
      return;
    }
    objectArrange.clearMembers();
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
  // A press on a table BOUNDARY is a resize, not a caret placement — and it works
  // on any table on the page, not only the one the caret is in, which is the whole
  // of `docs/141` D-1. Placed after the object path because an object drawn over a
  // table is still the thing under the pointer (the same order
  // `pointer_cursor.mjs` records), and before the caret path because the caret is
  // what this gesture replaces.
  // A press in the GUTTER beside the table selects a row or a column, or inserts
  // one at the `+` disc's boundary. BEFORE the boundary layer, because the two
  // zones overlap in a thin band and the gutter is the one the point is really
  // inside: a boundary zone reaches ±5px OUTSIDE the table's box, so without
  // this the disc on the table's bottom edge was stolen by a row resize unless
  // you aimed past the first 5px of the strip.
  if (tableGutter.tryBeginDrag(page, event)) return;
  if (tableChrome.tryBeginDrag(page, event)) return;
  // A TOUCH tap inside a table arms 24px boundary pills, because hover — the
  // affordance every branch above depends on — does not exist on touch. The first
  // `pointerType` read in the product (TBL-18).
  tableChrome.noteTouch(page, event);
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
  // …but a click INSIDE the accent fill of the row/column/table you just selected
  // keeps that selection (`docs/141` TBL-05). Every left-click cleared it, so a
  // user who selected a row and then clicked it to "confirm" lost it silently and
  // Merge cells went grey again. Google Docs keeps a cell block until you click
  // OUTSIDE it or start typing; the `contextmenu` handler already asks this exact
  // question through the same helper.
  //
  // O(1) when there is no table selection — the store returns on `!selection`
  // before it queries the engine — so the ordinary click path gains no document
  // walk (`docs/107` §4).
  if (!tableRange.containsClientPoint(event.clientX, event.clientY)) tableRange.clear();
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
    // The cell the press landed in, so a drag that leaves it becomes a CELL range
    // instead of a text range (`docs/141` D-3). One `inTable` per press, and the
    // drag re-asks only when the paragraph under the pointer changes.
    cellAnchor: doc.inTable(anchor.node) ? anchor.node : "",
    lastCellProbe: "",
    cellRange: false,
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
  if (pointerDragSelects(event)) startSelectionAutoScroll(); // never under a finger
  event.preventDefault();
}

function onPointerMove(page, event) {
  if (objectMoveDrag) {
    updateObjectMove(event);
    return;
  }
  if (objectRotate.active()) {
    objectRotate.update(event);
    return;
  }
  if (objectResize.active()) {
    objectResize.update(event);
    return;
  }
  if (tableChrome.dragging()) {
    tableChrome.moveDrag(event);
    return;
  }
  if (tableGutter.dragging()) {
    tableGutter.moveDrag(event);
    return;
  }
  if (dragging && event.buttons === 0) {
    resetPointerGesture();
    return;
  }
  if (!dragging) {
    // FIRST and unthrottled: a few dozen comparisons over the memoised page
    // chrome, no engine call, and a repaint only when the armed band changes.
    tableGutter.hover(page, event);
    pointerHover.schedule(page, event);
    return;
  }
  updateDragSelection(event);
}

function updateDragSelection(event) {
  if (!dragging || !pointerGesture || !selection || !pointerDragSelects(event)) return;
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
  // A drag that crosses a CELL boundary selects the rectangle, the way Docs and
  // Word both do; one that stays inside a cell is ordinary text selection. Every
  // other drag supersedes a row/column/table selection, which is why the clear
  // lives here rather than in the "has moved" branch above: pointer-down inside
  // the fill keeps the selection (`docs/141` TBL-05), and the moment the gesture
  // turns into a drag it is no longer a confirming click.
  if (tableRange.dragTo(pointerGesture.cellAnchor, focus.node, pointerGesture)) return syncSelectionToCellRange();
  tableRange.clear();
  selection = { anchor: selection.anchor, focus };
  drawSelection();
}

/** Follows the caret/selection to the cell rectangle the store now holds — the
 *  one line both the drag and Shift+Arrow need afterwards, so Copy and the
 *  toolbar see a range inside the selected cells rather than a stale caret. */
function syncSelectionToCellRange() {
  const range = tableRange.get();
  if (!range) return;
  selection = {
    anchor: { node: range.anchorNode, offset: 0 },
    focus: { node: range.focusNode, offset: 0 },
  };
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
  if (shapeDrawMode.finish()) return;
  if (finishObjectMove(event)) return;
  if (objectRotate.finish(event)) return;
  if (objectResize.finish(event)) return;
  if (tableChrome.finishDrag(event)) return;
  if (tableGutter.finishDrag(event)) return;
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
    } else if (!link && followTocEntry(selection?.focus?.node, selection?.focus?.offset)) {
      // A contents entry whose field carried no `\h`, so there is no authored
      // link to follow. The caret is already on the entry — this click placed it
      // — so the paragraph is known without a second hit-test.
    } else {
      showLinkChipAt(gesture.page, event);
    }
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
  if (shapeDrawMode.dragging()) {
    shapeDrawMode.update(e);
    return;
  }
  if (objectMoveDrag) {
    updateObjectMove(e);
    return;
  }
  if (objectRotate.active()) {
    objectRotate.update(e);
    return;
  }
  if (objectResize.active()) {
    objectResize.update(e);
    return;
  }
  if (tableChrome.dragging()) {
    tableChrome.moveDrag(e);
    return;
  }
  if (tableGutter.dragging()) {
    tableGutter.moveDrag(e);
    return;
  }
  if (dragging) {
    if (e.buttons === 0) resetPointerGesture();
    else updateDragSelection(e);
  }
});
pagesEl.addEventListener("pointerleave", (e) => {
  pointerHover.clear();
  // The strips are hover chrome: leaving the sheet takes them down, or they sit
  // beside a table the pointer is nowhere near. A TOUCH pointer leaves the moment
  // the finger lifts, so honouring it there would undo the tap that armed them.
  if (e.pointerType !== "touch" && tableGutter.clear()) paintOverlayLayer();
});
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
    // A picture has no flowed body to put a caret in, so "enter the object"
    // means what it means in Google Docs: double-click an image and you are
    // cropping it. The direct-manipulation crop chrome was already right; only
    // its doorway was missing, so crop was reachable solely by finding the Crop
    // button. A second double-click applies it, as the button turns into Apply.
    if (objectSelection?.canCrop && !objectSelection.canEditText) {
      enterCropMode();
      e.preventDefault();
      return;
    }
    // A shape with no text body yet: double-click GIVES it one and puts the
    // caret inside, which is exactly what Word and Docs do. The engine keeps
    // the preset, its guides, its fill and its outline — a star stays a star.
    if (objectSelection?.kind === "shape" && !objectSelection.canEditText) {
      e.preventDefault();
      void objectArrange.addText().then((added) => {
        // `addTextToShape` returns the new body's caret, and `refreshObject-
        // Capabilities` has by then re-read `canEditText`. Entering the object
        // is what makes the next keystroke go INTO the shape: while an object
        // is merely "selected" a printable key has nowhere to land.
        if (added) enterObjectEditMode(selection ? { ...selection.focus } : null);
      });
      return;
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
  touchSelection.selectWord(page, e); // the SAME routine the long press uses
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
/** Every pointer gesture the document owns, dropped. The four events below mean
 *  one thing — the pointer is gone and nothing will finish it — and were four
 *  copies of one list, so the touch machine would have been added to three of
 *  them and forgotten in the fourth. One list, named once. */
function abortPointerGestures() {
  cancelObjectMove();
  objectResize.cancel();
  objectRotate.cancel();
  tableChrome.cancelDrag();
  tableGutter.cancelDrag();
  touchSelection.cancel();
  resetPointerGesture();
}
window.addEventListener("pointercancel", abortPointerGestures);
window.addEventListener("lostpointercapture", abortPointerGestures);
window.addEventListener("blur", abortPointerGestures);
document.addEventListener("visibilitychange", () => document.hidden && abortPointerGestures());

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
/** Editing → Suggesting → Viewing → Editing, for keyboard access to all three
 *  (REVIEW-GAP-014). Google Docs cycles its three modes from one chord too; Word
 *  has no equivalent, so this is Docs' grammar rather than an invention. It is a
 *  named command rather than an inline keydown branch so that the chord is
 *  advertised on every surface (`109` UX-007). */
function cycleReviewMode() {
  setReviewMode(
    reviewMode === "editing" ? "suggesting" : reviewMode === "suggesting" ? "viewing" : "editing",
  );
}
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

/** Selects a whole row, column or table around `node` — the command twin of a
 *  click on the gutter strip. The endpoints come from the engine's own anchor
 *  list, so the menu route and the pointer route build the same rectangle
 *  (`table_range.mjs`). A mode the table cannot express — a column of a merged
 *  table — refuses with the catalogue's sentence instead of painting nothing. */
function selectTableContext(node, mode) {
  const refusal = tableRange.selectMode(node, mode);
  drawSelection();
  if (refusal) setStatus(t(refusal), "warn");
  else tableRange.announce();
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
// The table command tree lives in `table_commands.mjs`; this is the one place the
// editor's own bindings are handed to it. Every binding is read at CALL time, so
// the tree reflects the state the menu is opening over rather than boot state.
const TABLE_COMMAND_HOST = {
  doc: () => doc,
  tableSelection: () => tableRange.descriptor(),
  clearTableSelection: () => {
    tableRange.clear();
  },
  plainTableInfo,
  runEdit: (thunk, options) => runEdit(thunk, options),
  selectTableContext,
  openSplitCellDialog: () => toggleSplitCellDialog(true),
  // `focus` names a control INSIDE the popover to land on, which is how
  // `table.borderStyle` reaches the line-style pen from the Table menu and the
  // palette without a second copy of the control: the same shape `layout.indent`
  // uses to reach the indent field. Focus is taken after the click, because the
  // popover is hidden until then and `.focus()` on a hidden element is a no-op.
  openCellFormat: (focus) => {
    selectRibbonTab("table");
    tableBtn.click();
    if (focus) document.getElementById(focus)?.focus();
  },
  openTableProperties: () => toggleTableProperties(true),
  stepTableBand: (axis, sign) => tableChrome.stepCaretBand(selection?.focus?.node, pages, axis, sign),
  moveTableBand: (axis, sign) => tableGutter.moveBand(selection?.focus?.node, axis, sign),
};

const tableToolCommands = (context) => buildTableToolCommands(context, TABLE_COMMAND_HOST);

function buildContextCommands(context) {
  // Right-clicking a selected drawing/image/text box shows OBJECT commands, not
  // the paragraph-text menu (docs/85 §4.1; Word/Google Docs image menu). The
  // object was already selected by the handler that resolved this context.
  if (context.surface === "object") return buildObjectContextCommands(context, objectContextMenuHost);
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
  // A contents entry the author's field never hyperlinked. Beside Open link
  // because it IS Open link for that case, reached by the same gesture — a
  // right-click on the entry. O(1); see `toc_navigation.mjs`.
  if (!context.link && tocEntryTargetAt(selection?.focus?.node)) {
    annotate.push({
      id: "reference.goToHeading",
      label: "Go to the heading this entry names",
      group: "annotate",
      icon: "linkOpen",
      run: () => void followTocEntry(selection?.focus?.node),
    });
  }
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
      // The same capability as `insert.link` under the annotate surface's own id,
      // so it reads its chord from the one table rather than restating it.
      shortcut: shortcutForCommand("insert.link", EDITOR_KEYBOARD_PLATFORM),
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
      shortcut: shortcutForCommand("review.comment", EDITOR_KEYBOARD_PLATFORM),
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
      label: t("panelHome.increaseIndent.label"),
      group: "indent",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => adjustIndentCommand(360),
    },
    {
      id: "paragraph.indent.decrease",
      label: t("panelHome.decreaseIndent.label"),
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

  // A container that may NEVER change the document gets reading rows only: copy,
  // select all, search, and the two ways of following a link. Not the editing menu
  // greyed out — "never, for you" is composition and is silent (`docs/126`) — and
  // the owner's report was exactly this: a read-only container still offering a
  // table's structure commands and an image's properties. The review MODE is the
  // other question, and still says "not right now" with a reason.
  if (!containerCanEdit) {
    return [
      ...commands.filter((row) => row.id === "edit.copy" || row.id === "edit.selectAll"),
      pick("edit.find", { group: "clipboard" }),
      ...annotate.filter((row) => row.id === "link.open" || row.id === "reference.goToHeading"),
    ].filter(Boolean);
  }

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

  // Insert caption, on the TABLE menu as well as the picture menu: a table is
  // the second thing Word captions, and right-clicking it is how a reader
  // reaches it (`DocumentHolderExt.js:46`).
  commands.push(...referenceObjectMenuRows());

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

// The object right-click menu's ROWS are `object_context_menu.mjs`; this is the
// application state and the verbs it needs. Extracted to pay for the captions
// work under the line ratchet, and to give 156 lines of menu policy its first
// test (`tests/object_context_menu.test.mjs`).
const objectContextMenuHost = {
  reviewMode: () => reviewMode,
  readOnlyReason: () => readOnlyReason,
  wrapModes: () => WRAP_CHOICES.map((choice) => [choice.value, t(choice.key)]),
  positionPresets: () => POSITION_PRESETS.map((preset) => [preset.id, t(preset.key), preset]),
  zOrderChoices: () => Z_ORDER_CHOICES.map((choice) => [choice.value, t(choice.key)]),
  rotateChoices: () => ROTATE_CHOICES.map((choice) => [choice.value, t(choice.key)]),
  text: (key) => t(key),
  // ONE gather per menu build. `objectPosition` and `objectTransform` are each
  // O(document) with a single walk, so asking them once here — rather than once
  // per row, of which there are twenty-odd — is the difference between a menu
  // that opens in constant work and one that does not.
  arrangeState: () => {
    const state = arrangeState();
    if (!state) return null;
    const wrappable = wrapAvailability(state.read);
    const positionable = positionAvailability(state.read);
    return {
      wrappable: { available: wrappable.available, reason: wrappable.reasonKey ? t(wrappable.reasonKey) : "" },
      positionable: {
        available: positionable.available,
        reason: positionable.reasonKey ? t(positionable.reasonKey) : "",
      },
      stackable: objectArrange.zOrderAvailability(state),
      groupable: objectArrange.groupability(),
      rotatable: !!state.transform,
    };
  },
  activeWrap: () => {
    const state = arrangeState();
    return state ? activeWrapChoice(state.read, state.wrap) : null;
  },
  applyPosition: (preset) => objectArrange.applyPosition(preset),
  setZOrder: (value) => objectArrange.setZOrder(value),
  group: () => void objectArrange.group(),
  ungroup: () => void objectArrange.ungroup(),
  rotate: (value) => objectArrange.rotate(value),
  addText: () => void objectArrange.addText(),
  shapeColors: () => SHAPE_MENU_COLORS,
  objectWrap: (root) => doc.objectWrap(root),
  documentRows: () => referenceObjectMenuRows(),
  setObjectWrap,
  openAltText: () => openAltTextDialog(),
  applyShapeFill,
  applyShapeOutline,
  enterCrop: () => enterCropMode(),
  openProperties: () => toggleObjectInspector(true),
  deleteObject: () => deleteSelectedObject(),
};

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
  // No menu region, no menu — and no `preventDefault` either, so a container that
  // is a picture gives the visitor the browser's own menu instead of nothing.
  if (!chromeShows("context")) return;
  const page = pageFromEvent(event);
  if (!page || !doc) return;
  // Object hit-test takes precedence (docs/85 §3.1): right-clicking a drawing /
  // image / text box selects it as a unit and shows its OBJECT menu, not the
  // paragraph-text menu.
  // …and only where this container selects objects: without the `objects` region
  // a right-click on an image is a right-click on the TEXT under it, not a
  // gateway to alt text, crop and properties (what the owner saw in read-only).
  const objectContext = chromeShows("objects") ? objectContextAtEvent(page, event) : null;
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
  ) || tableRange.containsClientPoint(event.clientX, event.clientY);
  if (!preserveSelection) {
    selection = { anchor, focus: anchor };
    tableRange.clear();
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

// The engine seam for reflow is one setter; everything the shell owes it, and
// why each number is that number, is `reflow_chrome.mjs` (`docs/151` §6).
const reflowView = createReflowChrome({
  button: document.getElementById("viewReflowBtn"),
  viewport: viewportEl,
  getDoc: () => doc,
  unavailableReason: () => readOnlyReason,
  onChanged: () => renderAll(),
  setStatus,
});

// Folding (ADR-049): the engine calls, the command rows and the margin chevron
// are `fold_chrome.mjs`; the decisions are `fold_view.mjs`. `getPages`/`scaleOf`
// are the pair the ruler and touch selection already take, for the same reason.
const foldView = createFoldChrome({
  getDoc: () => doc,
  caretNode: () => selection?.focus?.node ?? "",
  getPages: () => pages,
  outlineOpen: () => !outlinePanel.hidden,
  scaleOf,
  onChanged: () => renderAll().then(() => scheduleChromeRefresh({ stats: true, outline: true })),
  setStatus,
});

// ---- Horizontal ruler ---------------------------------------------------------
// The strip itself lives in `ruler.mjs`. It is bound to the live `doc`,
// `selection`, `pages` and `pageBandModel` through getters rather than values,
// because all four are replaced wholesale when a document is opened or
// re-paginated, and a ruler holding a stale one draws the wrong page.
const rulerView = createRuler({
  getDoc: () => doc,
  getSelection: () => selection,
  getPages: () => pages,
  getBandModel: () => pageBandModel,
  withheldReason: () => reflowView.rulerWithheldReason(),
  runToolbarEdit,
  twipsPerInch: TWIPS_PER_INCH,
  labels: {
    tabCorner: "Tab stop type — click to change",
    firstLine: "First-line indent",
    left: "Left indent",
    right: "Right indent",
    tabGlyph: "Tab stop — click to change type, drag to move, drag off to remove",
  },
});

/** The ruler's second surface. `setTabStop`, `moveTabStop` and `removeTabStop`
 *  had exactly two call sites in the product, both inside `ruler.mjs`, so tab
 *  stops were a one-surface capability at every width (`docs/105` UX-004) — the
 *  phone rung is only where that became visible, because a ruler showing 0-3in
 *  of an 8.5in page is 24px of a 844px screen spent on a control nobody drags
 *  with a finger. Word has had this dialog for thirty years; `docs/148` §9
 *  item 7 is the row that asked for it. */
const tabStopsDialog = createTabStopsDialog({
  getDoc: () => doc,
  getSelection: () => selection,
  runToolbarEdit,
  registerModal,
  setStatus,
  fallbackFocus: () => pagesEl,
  twipsPerInch: TWIPS_PER_INCH,
  formatInches: twipsToDialogInches,
});

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
  { command: "insert.chart", buttons: [insertChartBtn], requires: "doc", activate: () => void insertChartAtCaret({ doc, caret: selection?.focus, blocked: blockMutationInViewing, suggesting: () => reviewMode === "suggesting", status: setStatus, apply: applyEditResult }) },
  { command: "insert.textbox", buttons: [insertTextBoxBtn], requires: "doc", activate: () => void insertTextBoxObject() },
  { command: "insert.link", buttons: [insertLinkBtn], requires: "range", activate: () => editSelectionLink() },
  { command: "insert.bookmark", buttons: [insertBookmarkBtn, refBookmarkBtn], requires: "doc", activate: () => openBookmarkManager() },
  { command: "insert.field", buttons: [insertFieldBtn, refFieldBtn], requires: "doc", activate: () => openFieldDialog() },
  // Page number and Date are the two field kinds the competition puts on the
  // Insert TAB rather than behind a generic field picker: Word's Insert tab has
  // Page Number in its Header & Footer group and Date & Time in its Text group,
  // and ONLYOFFICE's Insert tab has both as their own buttons (their groups 5
  // and 6). Here they were one level deeper — Insert ▸ Field, then a row in the
  // picker — which is a reachability gap, not a missing capability.
  //
  // They are the SAME commands the palette already registers per kind
  // (`insert.field.<kind>`, generated from FIELD_KINDS) running the SAME
  // `insertFieldAtCaret`, so there is no second insert path to keep in step:
  // this table supplies the enablement and the click, exactly as it does for the
  // picker's own button. The other four kinds (number of pages, time, file name,
  // author) stay in the picker, where Word and ONLYOFFICE also keep them.
  { command: "insert.field.page", buttons: [insertPageNumberBtn], requires: "doc", activate: () => insertFieldAtCaret("page") },
  { command: "insert.field.date", buttons: [insertDateBtn], requires: "doc", activate: () => insertFieldAtCaret("date") },
  { command: "insert.dropCap", buttons: [insertDropCapBtn], requires: "doc", activate: () => dropCapDialog.open() },
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
const BREAK_IO = { doc: () => doc, caret: () => selection?.focus ?? null, runEdit: (thunk, options) => runEdit(thunk, options), setStatus: (text) => setStatus(text) };
const LAYOUT_SURFACE = [
  // Page setup: one dialog, four fieldsets. Word's four buttons are four routes
  // into the same section geometry; each one opens the dialog with its own
  // fieldset focused rather than pretending to be a separate dialog.
  { command: "layout.margins", label: "Page margins", kw: "margins page setup top bottom left right gutter", buttons: () => [layoutMarginsBtn], requires: "doc", run: () => pageSetup.open(true, "margins") },
  { command: "layout.orientation", label: "Page orientation", kw: "orientation portrait landscape rotate page setup", buttons: () => [layoutOrientationBtn], requires: "doc", run: () => pageSetup.open(true, "orientation") },
  { command: "layout.size", label: "Page size", kw: "size paper a4 letter legal width height page setup", buttons: () => [layoutSizeBtn], requires: "doc", run: () => pageSetup.open(true, "size") },
  { command: "layout.columns", label: "Text columns", kw: "columns newspaper two three spacing separator page setup", buttons: () => [layoutColumnsBtn], requires: "doc", run: () => pageSetup.open(true, "columns") },
  // The button's click is the popover's own (`registerPopover`), so this row
  // declares `ownsClick` and contributes only the command id, the enablement
  // rule and the palette row — the same split INSERT_SURFACE spells `activate:
  // null` for `insert.table`. Without it the wiring below would attach a handler
  // that re-clicks the button the popover manager is already listening on.
  { command: "layout.lineNumbers", label: "Line numbers", kw: "line numbers numbering margin legal pleading count suppress", buttons: () => [lineNumbersBtn], requires: "doc", ownsClick: true, run: () => pageSetup.openLineNumbers() },
  // Watermark. NOT `ownsClick`: this one opens a modal the module registers
  // itself, so the click belongs to this table's own wiring — the popover
  // manager is not involved, and `page_setup.mjs` deliberately binds no listener
  // of its own to the button.
  { command: "layout.watermark", label: "Watermark", kw: "watermark draft confidential sample stamp diagonal background behind text", buttons: () => [watermarkBtn], requires: "doc", run: () => pageSetup.openWatermark(true) },
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
  { command: "layout.firstPageVariant", buttons: () => [insertFirstPageVariantBtn], requires: "doc", pressed: () => headerFooterSettings.variantState().firstPage, run: () => headerFooterSettings.toggleVariant("firstPage") },
  { command: "layout.evenOddVariant", buttons: () => [insertEvenOddVariantBtn], requires: "doc", pressed: () => headerFooterSettings.variantState().evenOdd, run: () => headerFooterSettings.toggleVariant("evenOdd") },
  // Header and footer settings. No `label`/`kw` for the same reason as the two
  // above: `editorCommands` declares the palette row, and a second declaration
  // here would put two rows for one command in the palette.
  { command: "layout.headerFooterSettings", buttons: () => [headerFooterSettingsBtn], requires: "doc", run: () => headerFooterSettings.open(true) },
  { command: "layout.arrange.wrap", label: "Wrap text around object", kw: "wrap text square tight through behind front object image shape arrange", buttons: () => [layoutWrapBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-inspector-wrap-select]") },
  { command: "layout.arrange.position", label: "Object position and size", kw: "position size move object image shape arrange exact geometry", buttons: () => [layoutPositionBtn], requires: "object", run: () => openObjectInspectorAt("[data-object-prop=left]") },
  ...breakSurfaceRows(BREAK_IO),
  ...arrangeSurfaceRows(objectArrange, {
    bringForward: () => layoutBringForwardBtn,
    sendBackward: () => layoutSendBackwardBtn,
    group: () => layoutGroupBtn,
    ungroup: () => layoutUngroupBtn,
    rotate: () => layoutRotateBtn,
    inLine: () => setObjectWrap("inline"),
  }),
];

const REFERENCE_SURFACE = [
  {
    command: "reference.tableOfContents",
    label: "Table of contents",
    kw: "table of contents toc outline headings index navigation",
    buttons: () => [refTocBtn],
    // Body-only, and for the engine's own reason: `insertTableOfContents`
    // refuses a caret outside the body, as Word does.
    requires: "bodyCaret",
    run: () => tocCommands.insert.open(),
  },
  // The keyboard half of following a contents entry. The pointer half is a plain
  // click on the entry, and `contextMenu: true` puts the same row on the
  // right-click menu, so the capability is reachable from three surfaces and from
  // none of them only.
  {
    command: "reference.goToHeading",
    label: "Go to the heading this entry names",
    kw: "toc table of contents entry heading navigate go to jump follow outline",
    buttons: () => [refGoToHeadingBtn],
    requires: "tocEntry",
    // NOT `contextMenu: true` — that flag puts a row on the OBJECT and table
    // right-click menus, and a contents entry is text. Its right-click row is
    // built with the other text rows, beside Open link, which is the same
    // capability for the case where the author's field did hyperlink it.
    run: () => void followTocEntry(selection?.focus?.node),
  },
  // Word's Captions group. A caption attaches to the caret's block — which is
  // also the block that holds a selected picture or table, since selecting an
  // object puts the caret at its anchor — and a cross-reference is inserted at
  // the caret. `contextMenu: true` is what puts Insert caption on the object and
  // table right-click menus, the way ONLYOFFICE does
  // (`DocumentHolderExt.js:46`), from this one declaration.
  {
    command: "reference.caption",
    // English, like every other row here: these labels are read at import, before
    // a catalogue exists, so `t()` would put the KEY in the palette. The
    // context-menu row is built on demand and does route through the catalogue.
    label: "Insert caption",
    kw: "caption figure table equation label numbering sequence seq chapter",
    buttons: () => [refCaptionBtn],
    requires: "bodyCaret",
    contextMenu: true,
    run: () => referenceCommands.caption.open(),
  },
  {
    command: "reference.crossReference",
    label: "Cross-reference",
    kw: "cross reference ref heading bookmark figure numbered item caption footnote endnote",
    buttons: () => [refCrossRefBtn],
    requires: "caret",
    run: () => referenceCommands.crossReference.open(),
  },
  // Word recomputes fields on print, on F9 and on open; ours are not recomputed
  // on a Backspace, deliberately, because renumbering there would make a keystroke
  // O(document). So the product has to OFFER the fix, or "dirty rather than
  // silently stale" is just "stale".
  {
    command: "reference.updateCaptionNumbers",
    label: "Update caption numbers",
    kw: "update caption numbers renumber stale seq figure table refresh fix",
    buttons: () => [refUpdateCaptionsBtn],
    requires: "staleCaptions",
    run: () => void referenceCommands.numbering.update(),
  },
  // Word's Update Table, and its two-radio dialog. The id stays
  // `reference.updateFields` because it is a published host-contract id; what
  // changed is that it now runs something. The two MODES are commands of their
  // own, so neither is reachable only from inside a modal — the same
  // >=2-surface rule the rest of this table follows.
  { command: "reference.updateFields", label: "Update table of contents", kw: "update fields refresh recalculate table of contents toc page numbers rebuild", buttons: () => [refUpdateFieldsBtn], requires: "tocField", run: () => tocCommands.update.open() },
  { command: "reference.updateToc.pageNumbers", label: "Update table of contents: page numbers only", kw: "toc contents update page numbers only repaginate refresh", buttons: () => [], requires: "tocField", run: () => void tocCommands.update.run("pageNumbers") },
  { command: "reference.updateToc.entire", label: "Update table of contents: entire table", kw: "toc contents update entire rebuild regenerate headings refresh", buttons: () => [], requires: "tocField", run: () => void tocCommands.update.run("entire") },
];

/** The object/table right-click rows the References surface declares — see
 *  `objectMenuRows`, which is where the rule and its citations live. */
const referenceObjectMenuRows = () =>
  objectMenuRows(REFERENCE_SURFACE, ribbonSurfaceEnabled, ribbonSurfaceReason);

/** The O(1) state the two surface rules read. `tocEntryTargetAt` returns
 *  immediately unless the document holds a TOC field and the heading index
 *  behind it is built once per revision; the two counts are caches. Nothing
 *  here walks the document, because this runs on every keystroke. */
function ribbonSurfaceState(entry) {
  return {
    hasDoc: !!doc,
    objectSelected: !!(objectSelection && objectSelection.mode === "selected"),
    hasCaret: !!selection,
    inRunningStory: !!runningEditBand,
    staleCaptions: referenceCommands.numbering.count,
    tocFields: tocCommands.fields.count,
    onTocEntry:
      entry.requires === "tocEntry" && !!selection && !!tocEntryTargetAt(selection.focus.node),
  };
}

const ribbonSurfaceEnabled = (entry) => surfaceEnabled(entry, ribbonSurfaceState(entry));
const ribbonSurfaceReason = (entry) => surfaceReason(entry, ribbonSurfaceState(entry));

/** Opens the object inspector focused on one control — Layout ▸ Wrap text lands
 *  on the wrap control, not on the panel's first field. `object_inspector.mjs`. */
const openObjectInspectorAt = (selector) => objectInspector.openAt(selector);

/** The one enablement rule for an Insert command, shared by its ribbon button,
 *  its app-menu row, and its palette entry. `context.hasRange` lets a surface
 *  that already computed the selection state (the context menu) pass it in. */
// ---- Review surface: one declaration per Review command ---------------------
// Word's Review tab — Tracking, Changes, Comments. Every command below already
// existed and was reachable ONLY from the command palette, or from buttons
// living inside the review sidebar, which exist only while that sidebar is open.
// So a user with a document full of tracked changes had no durable affordance
// for accepting one. Each row names the command its buttons run and whether the
// button is a toggle, and `review-surface.spec.mjs` asserts the ribbon's id set
// equals the Review menu's, so a command cannot reach one surface and miss the
// other.
//
// `buttons`, plural, and the same shape LAYOUT_SURFACE/REFERENCE_SURFACE
// already use — one command can have more than one face. `review.comment` has
// two: the Review band's button and the right-margin affordance beside the
// selection. They are declared together on purpose. A margin button wired up on
// its own would be a second way to open the comment composer, free to drift
// from the first in what it runs and in when it is available, which is the
// command-surface defect class this table exists to prevent (`105` UX-004).
//
// Enablement is deliberately just "a document is open": the commands
// themselves report why nothing happened (no change at the cursor, none left to
// accept), which is more use than a button that is silently dead.
const reviewCommentActions = createReviewCommentActions({ runEdit, getDoc: () => doc, commentId: () => activeReviewCommentId, announce: (m) => announceReview(m), afterChange: () => drawSelection() });
const REVIEW_SURFACE = [
  { command: "review.mode.suggesting", buttons: () => [reviewTrackBtn], run: () => setReviewMode(reviewMode === "suggesting" ? "editing" : "suggesting"), pressed: () => reviewMode === "suggesting" },
  { command: "view.showChanges", buttons: () => [reviewShowChangesBtn], run: () => toggleShowChanges(), pressed: () => showingChanges },
  { command: "review.previous", buttons: () => [reviewPrevBtn], run: () => navigateReview(-1) },
  { command: "review.next", buttons: () => [reviewNextBtn], run: () => navigateReview(1) },
  { command: "review.acceptNext", buttons: () => [reviewAcceptBtn], run: () => decideReviewAndAdvance(true) },
  { command: "review.rejectNext", buttons: () => [reviewRejectBtn], run: () => decideReviewAndAdvance(false) },
  { command: "review.acceptAll", buttons: () => [reviewAcceptAllBtn], run: () => void decideAllReviewChanges(true) },
  { command: "review.rejectAll", buttons: () => [reviewRejectAllBtn], run: () => void decideAllReviewChanges(false) },
  // Three faces, and the third is on the INSERT band: ONLYOFFICE carries Comment
  // on both Insert and Collaboration, and Word's Insert tab has a Comments group
  // of its own. Commenting was already in this editor's Insert MENU (Google
  // Docs files it at Insert ▸ Comment, which is where `command_taxonomy.mjs`
  // took it from) but had no Insert BAND button, so in the ribbon chrome — which
  // has no menu bar (docs/122) — the Insert tab could not reach it at all.
  // Declared here rather than beside the new button for the reason the comment
  // above gives: a second wiring is free to drift in what it runs and in when it
  // is available.
  { command: "review.comment", buttons: () => [reviewCommentBtn, insertCommentBtn, reviewMarginCommentBtn], requires: "range", reasonKey: "review.comment.needsRange", run: () => openReviewComposer() },
  // `requires: "comment"` — the caret is inside a commented range. Word and
  // ONLYOFFICE both target that comment rather than a sidebar selection, so a
  // reviewer never has to open a panel to resolve what they are reading.
  { command: "review.comment.resolve", buttons: () => [reviewResolveBtn], requires: "comment", run: () => void reviewCommentActions.resolve() },
  { command: "review.comment.delete", buttons: () => [reviewDeleteBtn], requires: "comment", run: () => void reviewCommentActions.remove() },
  { command: "review.toggle", buttons: () => [reviewPanelBtn], run: () => toggleReview(), pressed: () => !reviewSidebar.hidden },
  // TWO faces, one command. This table owns their DISABLED state;
  // `compare_documents.mjs` owns the clicks and the pressed states, hence
  // `ownsClick`. Listing one face only shipped the rail entry dead (`105` UX-004).
  { command: "review.compare", buttons: () => [document.getElementById("reviewCompareBtn"), document.getElementById("railCompare")].filter(Boolean), requires: "doc", reasonKey: "compare.needsDocument", ownsClick: true, run: () => comparePanel.open() },
  // Proofing. Both switches were the whole content of a `Tools` menu, which is
  // one top-level name for two toggles — and one of the two names that scrolled
  // off the end of the menu bar (`109` HF-097). Word's Review tab opens with a
  // Proofing group, so that is where they go now that the ribbon is the only
  // navigation axis in this chrome. `requires: "always"`: both are `noDoc`
  // commands — they are preferences, and switching one with no document open is
  // meaningful and already supported.
  { command: "tools.spellCheck", buttons: () => [reviewSpellCheckBtn], requires: "always", pressed: () => settings.spellCheck !== false, run: () => setSpellCheckEnabled(settings.spellCheck === false) },
  { command: "tools.grammarCheck", buttons: () => [reviewGrammarCheckBtn], requires: "always", pressed: () => settings.grammarCheck !== false, run: () => setGrammarCheckEnabled(settings.grammarCheck === false) },
  { command: "tools.smartQuotes", buttons: () => [reviewSmartQuotesBtn], requires: "always", pressed: () => smartQuotesEnabled, run: () => setSmartQuotes(!smartQuotesEnabled) },
  { command: "tools.languages", buttons: () => [reviewProofLanguagesBtn], requires: "always", run: () => proofing.openLanguages() },
  // Restrict Editing (ADR-052, ADR-059). Declared HERE rather than beside the
  // button for the reason this table gives twice over: one owner for `disabled`,
  // for the reason it carries while disabled, and for the pressed state. The
  // module owns the dialog and the engine call and nothing about the button, so
  // the two cannot disagree about whether a restriction is in force.
  //
  // `pressed` reads the ENGINE, not what the dialog last asked for — ONLYOFFICE's
  // Protect button is a state too — so a document that arrives protected shows the
  // button pressed before anyone opens anything.
  { command: "review.restrictEditing", buttons: () => [document.getElementById("reviewProtectBtn")].filter(Boolean), requires: "doc", reasonKey: "command.needsDocument", pressed: () => documentProtection.isActive(), run: () => documentProtection.open() },
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
/** Breathing room between a revealed caret and the edge it was revealed past;
 *  flush lasts one frame and the next repaint puts it back outside. */
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
  const margin = SCROLL_INTO_VIEW_MARGIN; // as the overlay path, in doc space
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
  const target = scrollTargetFor({
    marker: marker.getBoundingClientRect(),
    viewport: viewportEl.getBoundingClientRect(),
    current: viewportEl.scrollTop,
    max: Math.max(0, viewportEl.scrollHeight - viewportEl.clientHeight),
    scale: pageBandModel?.scale ?? 1,
    block,
    margin: SCROLL_INTO_VIEW_MARGIN,
  });
  if (target !== null) viewportEl.scrollTo({ top: target, behavior: "auto" });
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
// from a crash draft — or from a version in the timeline (`docs/139` §8.5 step
// 7) — has never been written out ANYWHERE. Its revision starts at the open-time
// baseline and its name matches, so both axes above would call it clean — which
// would re-arm, one level up, exactly the data loss the draft exists to prevent.
// Both restores set it for the same reason, which is why it is one flag and not
// two. Cleared by the same two places that clear the others.
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
  // Every landed edit can add or remove a floating object, so the memoized
  // yes/no answer is dropped here — the one choke point every edit passes
  // (`109` HF-183). Costs one engine call on the next question, which is the
  // price of not asking on every keystroke.
  objectPresence.forget();
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
  // And the host's, under the same constraint: one revision integer, one boolean.
  hostSession?.noteChange();
}

/** How many documents this tab has opened.
 *
 *  `currentRevision` restarts at 0 for each one, so it identifies a document's
 *  STATE but not the document — and a cache keyed on it alone silently survives
 *  an open. That is not hypothetical: the contents-entry cache, keyed on the
 *  revision, answered "this document has no table of contents" for a TOC
 *  document, because the demo opened before it had already asked at revision 0.
 *  Paired with the revision, this makes the key identify both. */
let documentEpoch = 0;

/** A token that changes whenever the document changes OR is replaced. */
function documentVersion() {
  return `${documentEpoch}:${currentRevision}`;
}

/** Re-baseline onto a freshly opened document: nothing is unsaved yet. */
function resetDirtyTracking(name) {
  documentEpoch += 1;
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
  // An explicit Save is ALWAYS a version (ADR-038 / `docs/139` §18 question 3):
  // it is the point a user recognises in a timeline, and because checkpoints are
  // content-addressed a Save whose bytes are identical to the last version costs
  // one ~300-byte row rather than a second copy of the document. This takes its
  // own snapshot in the document's own format — the download may have been a PDF,
  // and a checkpoint has to be fidelity-capable whatever was handed over.
  versionHistory.capture(CAPTURE_REASON.SAVE);
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

/** Repaints what a remote arrival changed. The engine already applied it through
 *  `ClientSession::receive`; this only paints. The caret is NOT mapped (`107` P-4). */
async function paintArrival({ dirty = [], pageCount, viewRevision }) {
  noteDocumentEdited(viewRevision);
  if (pageCount !== pages.length) await renderAll();
  else { for (const i of dirty) repaintPage(i); drawSelection(); }
  scheduleChromeRefresh({ stats: true, outline: true });
}

async function applyEditResult(res, { keepView = false } = {}) {
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
  tableRange.clear();
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
  // A review edit is not a text edit: the caret never moved, and it is usually
  // nowhere near the comment being worked on — so scrolling it into view sent
  // the reader back to wherever the caret happened to be (the top, normally).
  // ...and neither is an OBJECT edit: the user is manipulating the object, not
  // the caret, so following the caret throws them off it (`object-edit-keeps-view`).
  if (!keepView && !objectSelection) scrollCaretIntoView();
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
  if (rect.length >= 5) {
    refreshObjectCapabilities();
    return;
  }
  objectSelection = null;
  objectCropSession = null;
  updateObjectSelectionState();
  updateObjectContextBar();
}

/** Re-reads the ENGINE's capability bits for the object that is still selected.
 *
 *  They were snapshotted when the object was selected and never read again, so
 *  an edit that CHANGES what the object can do left the chip offering the old
 *  answer. `setObjectAnchorKind` is exactly that edit: an object taken in line
 *  can no longer be moved or wrapped — the flow decides where it sits — and the
 *  chip went on offering Move and a wrap mode for a node that has no anchor.
 *  The same is true of grouping, ungrouping and anything else that rewrites the
 *  node under a live selection.
 *
 *  Cost: O(objects) once per EDIT, never per keystroke and never per pointermove
 *  — `objectOrder()` is the same walk `objectPresence` already memoises, and a
 *  document with no selected object pays nothing. */
function refreshObjectCapabilities() {
  const held = objectSelection;
  if (!held || held.mode === "editing") return;
  let entries;
  try {
    entries =
      held.ref.subject === held.ref.root
        ? JSON.parse(doc.objectOrder())
        : JSON.parse(doc.objectDescendants(held.ref.root));
  } catch {
    return;
  }
  const fresh = Array.isArray(entries)
    ? entries.find((entry) => entry.node === held.node)
    : null;
  // Not in the list any more (it became a group's child, say): leave what is
  // held rather than guessing. `objectRect` already proved it is still placed.
  if (!fresh) return;
  objectSelection = {
    ...held,
    kind: fresh.kind ?? held.kind,
    anchored: fresh.anchored ?? held.anchored,
    ...objectCapabilities(fresh),
  };
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
async function runEdit(thunk, { typing = false, gate = false, keepView = false } = {}) {
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
    setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason, routeRefusal: SESSION.sentenceFor }), "error");
    return false;
  }
  await applyEditResult(res, { keepView });
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
  tableRange.clear();
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
  if (hasSel && doc) reflectOpenPopovers();
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
  // failing after the user clicks them. WITH A STATED REASON, from the same
  // catalogue entry the Table menu reads (`docs/141` TBL-03): the five
  // hand-written loops that used to live here set `disabled` and never touched
  // `title`, so a disabled Sort button still advertised "Sort rows ascending"
  // and the reader had no way to tell an unmet precondition from a broken build.
  // `title` is the only channel a disabled button has — it takes no focus and
  // fires no events — which is the argument `LAYOUT_SURFACE` above already makes.
  const inTable = hasSel && doc && doc.inTable(selection.focus.node);
  const tableInfo = inTable ? doc.tableInfo(selection.focus.node) : null;
  for (const { control, enabled, reasonKey } of tableBandStates(tableRibbon, {
    inTable,
    regular: tableInfo?.regular,
    rowHeightRule: tableInfo?.rowHeightRule,
    hasCellSelection: tableRange.mergeable(),
  })) {
    control.disabled = !enabled;
    control.title = enabled ? authoredTitle(control, EDITOR_KEYBOARD_PLATFORM) : t(reasonKey);
  }
  // The one band control with a DYNAMIC enabled title, so it is written after the
  // sweep above rather than fighting it.
  const activeTableStyle = inTable && tableInfo?.found ? (doc.tableStyleAt?.(selection.focus.node) || "") : "";
  if (activeTableStyle) tableStyleBtn.title = t("table.styleNamed", { name: activeTableStyle });
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
  // Home, View and Table name the commands their controls stand for (`109` UX-005).
// The four tables below do the same for the tabs that were already declarative;
// the argument, and why an unmatched selector is reported rather than swallowed,
// is in `ribbon_faces.mjs`.
const unstampedFaces = stampRibbonFaces(document);
if (unstampedFaces.length) console.warn("ribbon faces with no control:", unstampedFaces.join(", "));

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
        button.title = enabled ? authoredTitle(button, EDITOR_KEYBOARD_PLATFORM) : reason;
      }
      // A switch has to say which way it is set, whichever table declares it.
      if (entry.pressed) button.setAttribute("aria-pressed", String(entry.pressed()));
    }
  }
  // Review: the whole band's sweep — its preconditions, its reasons, its pressed
  // states and the ROOM's grant — in `ribbon_surface.mjs` beside the Layout and
  // References one, with the rationale that used to sit here.
  reflectReviewSurface(REVIEW_SURFACE, { hasDoc: !!doc, hasRange: !!range, hasComment: !!activeReviewCommentId, refusalFor: (c) => SESSION.refusalFor(c), authoredTitle: (b) => authoredTitle(b, EDITOR_KEYBOARD_PLATFORM) });
  // The ¶ control owns both of its halves and its popover's five checkmarks, so
  // it reflects itself rather than being swept here: its state is the ENGINE's
  // `any`, not a local flag, and nothing else on the band knows how to read it.
  formattingMarks.reflect();
  // Ribbon: undo/redo/view controls need a document; the Table tab is contextual.
  undoBtn.disabled = !doc || !doc.canUndo;
  redoBtn.disabled = !doc || !doc.canRedo;
  const undoLabel = doc?.undoLabel || "";
  const redoLabel = doc?.redoLabel || "";
  // Through the seam: the boot sweep never sees these, so they stayed English.
  const undoName = undoLabel ? t("toolbar.undoNamed", { name: undoLabel }) : t("toolbar.undo");
  const redoName = redoLabel ? t("toolbar.redoNamed", { name: redoLabel }) : t("toolbar.redo");
  for (const [button, name] of [[undoBtn, undoName], [redoBtn, redoName]]) button.setAttribute("aria-label", name);
  // Reassigned on every sync, so they outlive the boot sweep (HF-025).
  undoBtn.title = localizeShortcutText(`${undoName} (⌘Z)`, EDITOR_KEYBOARD_PLATFORM);
  redoBtn.title = localizeShortcutText(`${redoName} (⌘⇧Z)`, EDITOR_KEYBOARD_PLATFORM);
  findBtn.disabled = replaceBtn.disabled = !doc;
  // Clipboard buttons mirror the clipboard actions' own preconditions: copy/cut
  // need a range; paste needs a caret. The actions still fail closed in Viewing
  // mode, but the buttons also disable there so the affordance matches.
  copyBtn.disabled = !range;
  cutBtn.disabled = !range || reviewMode === "viewing";
  pasteBtn.disabled = !hasSel || !doc || reviewMode === "viewing";
  syncStylesGalleryActive();
  propertiesBtn.disabled = !doc;
  pageSetup.setEnabled(Boolean(doc));
  viewOutlineBtn.disabled = !doc;
  viewOutlineBtn.setAttribute("aria-pressed", String(!outlinePanel.hidden));
  reviewBtn.disabled = !doc;
  reviewBtn.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  railReview.disabled = !doc;
  railReview.setAttribute("aria-pressed", String(!reviewSidebar.hidden));
  viewZoom.setEnabled(!!doc);
  tabTable.disabled = !inTable;
  // The compact bar's Table group is contextual for the same reason this tab is.
  compactToolbarUi?.setTableContext(inTable);
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

/** One literal for one meaning: the chooser's clear row, and the palette's. */
const NO_TABLE_STYLE = "No table style";

function populateTableStyles() {
  tableStyleMenu.replaceChildren();
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "table-style-choice table-style-clear";
  clear.dataset.tableStyle = "";
  clear.textContent = NO_TABLE_STYLE;
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
  for (const button of entry.buttons()) {
    if (!button) continue;
    // `ownsClick` as on the Layout table: the module that renders the surface binds
    // the click, and a second handler would run a toggle twice — i.e. not at all.
    if (!entry.ownsClick) onButton(button, entry.run);
    button.dataset.command = entry.command;
  }
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
    // Fallback for a control with no i18n key. Captured at BOOT, before a
    // catalogue exists, so always English — hence `authoredTitle`'s preference.
    button.dataset.enabledTitle = button.title;
    if (entry.run && !entry.ownsClick) onButton(button, entry.run);
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
    if (page) touchSelection.selectWord(page, event);
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
    if (e.key !== "Escape") return;
    // An armed shape is a MODE, and a mode you cannot leave is the worst kind.
    // Ahead of the painter and of every selection handler, for the same reason
    // the painter is ahead of them: a stray Escape should put the tool down.
    if (shapeDrawMode.armedToken()) {
      e.preventDefault();
      e.stopPropagation();
      shapeDrawMode.disarm(true);
      return;
    }
    if (formatPainter) {
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
// Toolbar popovers are `popover_manager.mjs`: anchoring, one-at-a-time, light
// dismiss, Escape and focus return, for the ribbon's menus and the footer's
// alike. The manager does not know what a selection is; this is the one thing
// it asks the editor, so a menu that would act on the caret does not open when
// there is no caret to act on.
configurePopovers({ documentReady: () => !!selection || !!objectSelection });
const TWIPS_PER_POINT = 20;
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

/** (Re)renders a color picker menu, marking the active swatch and refreshing the
 *  recently-used row. Called on each open via the popover's reflect hook. The
 *  menu's SHAPE is `picker_chrome.mjs`; what is here is the two vocabularies and
 *  the session state that decide which swatch is lit. */
function renderColorMenu(kind, menu = kind === "text" ? textColorMenu : highlightMenu) {
  const isText = kind === "text";
  const active = isText ? lastTextColor.toLowerCase() : lastHighlight;
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
  underlineMenu.replaceChildren();
  underlineMenu.appendChild(makeMenuHeading("Underline style"));
  const styleGroup = document.createElement("div");
  styleGroup.setAttribute("role", "radiogroup");
  styleGroup.setAttribute("aria-label", "Underline style");
  for (const [style, label] of UNDERLINE_STYLE_LABELS) {
    styleGroup.appendChild(makeUnderlineStyleOption(style, label));
  }
  underlineMenu.appendChild(styleGroup);
  // The menu is rebuilt on every open, so the group is bound per render. It is a
  // real radio group — one of the eleven underline styles is always the current
  // one — and it now gets the same single Tab stop and arrow behaviour as every
  // other segmented control instead of eleven Tab stops and dead arrows.
  bindRadioGroup(styleGroup, {
    attr: "data-underline-style",
    onSelect: (style) => applyUnderlineStyle(style),
  }).reflect(currentUnderlineMixed ? null : currentUnderlineStyle);

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
  // The style radio group applies the pick itself (so the arrow keys work, which
  // is the whole of `109` UX-021); what a POINTER pick adds is dismissal.
  if (e.target.closest("[data-underline-style]")) {
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

/** Word's Shape Fill / Shape Outline menus. `shape_format_menu.mjs`. */
const shapeMenuHost = {
  menu: (kind) => (kind === "fill" ? shapeFillMenu : shapeOutlineMenu),
  format: () => selectedShapeFormat(),
  heading: (text) => makeMenuHeading(text),
  swatchGrid: (cells) => makeSwatchGrid(cells),
  swatchCell: (...args) => makeSwatchCell(...args),
  colors: () => TEXT_STANDARD_COLORS,
  weights: () => OUTLINE_WEIGHTS,
  emuPerPoint: EMU_PER_POINT,
};

const shapeFillPopover = registerPopover(shapeFillBtn, shapeFillMenu, () => renderShapeMenu("fill", shapeMenuHost));
const shapeOutlinePopover = registerPopover(shapeOutlineBtn, shapeOutlineMenu, () =>
  renderShapeMenu("outline", shapeMenuHost),
);

// ---- The object chip's Arrange controls ------------------------------------
// Three popovers on the chip a selected object carries: Word's Position gallery,
// its Arrange menu (stacking + Group/Ungroup) and its Rotate menu. Registered
// here rather than beside the commands because `registerPopover` and the
// `popovers` list it appends to are declared further down the file.

/** A chip button that OPENS a popover. Unlike the chip's action buttons it must
 *  NOT cancel `pointerdown`: cancelling it suppresses the compatibility
 *  `mousedown` the popover manager opens on, and the button would do nothing at
 *  all — the same trap `makeShapeBarButton` documents. */
function makeObjectMenuButton(icon, labelKey, menuId) {
  const btn = document.createElement("button");
  btn.type = "button";
  // Icon-only, like Word's own Arrange group and Docs' image chip. The chip is
  // already the densest control in the product and these three would have added
  // ~150px of text to a bar that sits OVER the object when there is no room
  // above it. The name is on the tooltip AND on the accessible name.
  btn.className = "object-bar-btn object-bar-icon";
  btn.setAttribute("aria-haspopup", "menu");
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-controls", menuId);
  const glyph = document.createElement("span");
  glyph.className = "ms";
  glyph.setAttribute("aria-hidden", "true");
  glyph.textContent = icon;
  btn.appendChild(glyph);
  btn.dataset.objectMenu = menuId;
  btn.dataset.labelKey = labelKey;
  return labelledObjectMenuButton(btn);
}

/** Re-reads a chip menu button's name from the catalogue.
 *
 *  These buttons are built ONCE, at import, when no catalogue is loaded — so a
 *  name baked in there is the KEY, in every language including English. Reading
 *  it on every bar render is also what makes a locale switch reach them. */
function labelledObjectMenuButton(btn) {
  const label = t(btn.dataset.labelKey);
  btn.title = label;
  btn.setAttribute("aria-label", label);
  return btn;
}

const objectPositionMenu = document.getElementById("objectPositionMenu");
const objectArrangeMenu = document.getElementById("objectArrangeMenu");
const objectRotateMenu = document.getElementById("objectRotateMenu");
const objectPositionBtn = makeObjectMenuButton("open_with", "object.position", "objectPositionMenu");
const objectArrangeBtn = makeObjectMenuButton("layers", "object.arrange", "objectArrangeMenu");
const objectRotateBtn = makeObjectMenuButton("rotate_right", "object.rotate", "objectRotateMenu");

const objectPositionPopover = registerPopover(objectPositionBtn, objectPositionMenu, () => {
  const state = arrangeState();
  const availability = positionAvailability(state?.read ?? { object: false });
  renderPositionGallery(objectPositionMenu, {
    presets: POSITION_PRESETS,
    t,
    activeId: null,
    reason: availability.available ? null : t(availability.reasonKey),
    onPick: (preset) => {
      closePopover(objectPositionPopover);
      objectArrange.applyPosition(preset);
    },
  });
});
const objectArrangePopover = registerPopover(objectArrangeBtn, objectArrangeMenu, () => {
  const verdict = objectArrange.groupability();
  renderArrangeMenu(objectArrangeMenu, {
    t,
    zChoices: Z_ORDER_CHOICES,
    zOrder: objectArrange.zOrderAvailability(arrangeState()),
    group: { enabled: verdict.can, reason: verdict.reason },
    ungroup: {
      enabled: objectSelection?.kind === "group",
      reason: t("object.ungroup.notAGroup"),
    },
    onZOrder: (order) => {
      closePopover(objectArrangePopover);
      objectArrange.setZOrder(order);
    },
    onGroup: () => {
      closePopover(objectArrangePopover);
      void objectArrange.group();
    },
    onUngroup: () => {
      closePopover(objectArrangePopover);
      void objectArrange.ungroup();
    },
  });
});
const objectRotatePopover = registerPopover(objectRotateBtn, objectRotateMenu, () => {
  const state = arrangeState();
  renderRotateMenu(objectRotateMenu, {
    t,
    choices: ROTATE_CHOICES,
    enabled: !!state?.transform,
    reason: t("object.rotate.unsupported"),
    onPick: (value) => {
      closePopover(objectRotatePopover);
      objectArrange.rotate(value);
    },
  });
});

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
// The ladder and the step rule are `nextFontSizeStep` in `style_picker.mjs`,
// beside `previewPx`: both are point sizes, and the epsilon and the past-the-end
// behaviour are exactly the parts that want a unit test rather than a browser.
function currentFontPt() {
  const v = Number(fontSizeSel.value);
  if (Number.isFinite(v) && v >= 1) return v;
  if (pendingFormat?.sizeHalfPoints != null) return pendingFormat.sizeHalfPoints / 2;
  return 11;
}
function stepFontSize(dir) {
  const next = nextFontSizeStep(currentFontPt(), dir);
  armOrApplyRun({ sizeHalfPoints: Math.round(next * 2) }, () =>
    runToolbarEdit((a, b, c, d) => doc.setFontSize(a, b, c, d, next)),
  );
}
onButton(growFontBtn, () => stepFontSize(1));
onButton(shrinkFontBtn, () => stepFontSize(-1));

// ---- Change case (Q5): transform selected text, preserving per-run format ----
// The cross-run re-slicing rule is `recaseRichRuns` in `text_rules.mjs`, with
// the two formatting boundaries that make it hard; what is left here is reading
// the runs out of the engine and writing them back.
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
  await pasteRichRunsJson(JSON.stringify(recaseRichRuns(runs, mode)));
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

/** The gallery cells, in keyboard order, and where each visual row starts —
 *  rebuilt on every open because the catalogue is fixed but the DOM is not. */
let shapeGalleryCells = [];
const SHAPE_GALLERY_COLUMNS = 6;

/** Renders Word's grouped shape gallery: every preset the engine models, under
 *  the headings Word files them under. `shape_gallery.mjs` owns the DOM. */
function paintShapeGallery() {
  shapeGalleryCells = renderShapeGallery(shapeGalleryMenu, {
    t,
    onPick: (token) => {
      closePopover(shapeGalleryPopover);
      // Word ARMS the pointer here rather than inserting: the next gesture on
      // the page decides where the shape goes and how big it is. A click
      // without a drag still places the default size, so the gallery works for
      // someone who has never been told about the drag.
      shapeDrawMode.arm(token);
      focusEditorSurface();
    },
  });
  shapeGalleryMenu.style.setProperty("--shape-gallery-columns", String(SHAPE_GALLERY_COLUMNS));
}

const shapeGalleryPopover = registerPopover(insertShapeBtn, shapeGalleryMenu, paintShapeGallery);

/** Opens the gallery, wherever the command came from (ribbon button or palette). */
function openShapeGallery() {
  if (!doc) return;
  if (shapeGalleryMenu.hidden) openPopover(shapeGalleryPopover);
  queueMicrotask(() => shapeGalleryCells[0]?.focus());
}

// Arrow keys walk the grid, which is what makes 22 icon cells operable without a
// pointer. The rule is `nextGalleryIndex`; this is the focus half.
shapeGalleryMenu.addEventListener("keydown", (event) => {
  const index = shapeGalleryCells.indexOf(document.activeElement);
  const next = nextGalleryIndex(
    event.key,
    index,
    galleryRowStarts(SHAPE_GALLERY_COLUMNS),
    shapeGalleryCells.length,
  );
  if (next < 0) return;
  event.preventDefault();
  shapeGalleryCells[next]?.focus();
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
// Wiring lives in `spacing_menu.mjs`; the competitive reference and the reason
// an inherited line spacing reads back as "from the style" are recorded there.
const spacingMenuControl = createSpacingMenu({
  doc: () => doc,
  focusNode: () => selection?.focus?.node ?? null,
  runToolbarEdit,
  registerPopover,
  onButton,
  t,
});

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
    closeAllPopovers();
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
document.getElementById("paragraphPropertiesClose").addEventListener("click", () =>
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
    const [r, g, bl] = hexToRgb(document.getElementById("borderColor").value);
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
/** A points field's value → twips (≥ 0); "" clears the paragraph's own value so
 *  the style decides again, which is what `-1` means to the engine. */
function applySpace(input, setter) {
  const raw = input.value.trim();
  if (raw !== "" && !Number.isFinite(Number(raw))) return;
  const twips = raw === "" ? -1 : Math.max(0, Math.round(Number(raw) * TWIPS_PER_POINT));
  runToolbarEdit((a, b, c, d) => setter(a, b, c, d, twips));
}
paraSpaceBefore.addEventListener("change", () =>
  applySpace(paraSpaceBefore, (a, b, c, d, twips) => doc.setSpaceBefore(a, b, c, d, twips)),
);
paraSpaceAfter.addEventListener("change", () =>
  applySpace(paraSpaceAfter, (a, b, c, d, twips) => doc.setSpaceAfter(a, b, c, d, twips)),
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
    // Through the SAME policy function `runEdit` uses (`docs/141` TBL-04). This
    // used to put `err.message` straight on the status line, so the facade's own
    // vocabulary reached the reader — a failed cell-border change could announce
    // "column width requires a regular table" — and none of it was localised.
    setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason, routeRefusal: SESSION.sentenceFor }), "error");
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

// The cell-format popover and the split-cell dialog are `table_cell_chrome.mjs`;
// what stays here is the editor state they need. `formatRange` is the one line
// that removed the old *"put the caret in the cell to format it"* refusal: the
// caret's own node passed twice is a one-cell range, so one path serves both.
const cellFormatMenu = bindCellFormatMenu({
  menu: tableFmtMenu,
  shade: document.getElementById("cellShade"),
  shadeNone: document.getElementById("cellShadeNone"),
  vAlign: document.getElementById("cellVAlign"),
  cellBorderColor: document.getElementById("cellBorderColor"),
  tableBorderColor: document.getElementById("tableBorderColor"),
  borderWeight: document.getElementById("borderWeight"),
  borderStyle: document.getElementById("borderStyle"),
  doc: () => doc,
  caretNode: () => (selection && doc ? selection.focus.node : ""),
  formatRange: (apply) =>
    selection && doc ? tableRange.formatRange(apply, selection.focus.node) : false,
  runNodeEdit,
  onButton,
  bindRadioGroup,
  hexToRgb,
});
const tablePopover = registerPopover(tableBtn, tableFmtMenu, () => cellFormatMenu.reflect());
bindBreaksMenu(BREAK_IO);

// ---- Four engine capabilities that had no control at all --------------------
// Each of these was reachable from the engine in one call and from the product in
// none (`SKILL` §9 rule 4). What stays here is the editor state they need; every
// decision — the competitor shape, the labels, the refusals, the command rows —
// lives with its own control.
//
// `repaint` for the marks is the page WINDOW, not the document: turning marks on
// is a repaint and never a repagination, so re-rastering what is on screen is the
// whole of the chrome's duty and it is O(window) rather than O(document).
const formattingMarks = createFormattingMarks({
  getDoc: () => doc,
  repaint: () => {
    for (let i = pageWindow.first; i <= pageWindow.last; i++) repaintPage(i);
  },
  setStatus,
});
const measurement = createMeasurementUnits({
  engine: {
    measurementUnits,
    defaultMeasurementUnit,
    decimalSeparatorForLanguage,
    parseMeasurement,
    formatMeasurement,
  },
  select: document.getElementById("measurementUnitSelect"),
  locale: () => activeLocale(),
  // The browser's tag, because it carries a REGION: the first-run default is
  // inches only for the US and Canada, and the UI locale is a language choice
  // that is usually just "en".
  region: () => navigator.language || activeLocale(),
  onChanged: () => pageSetup.reflectUnits(),
  setStatus,
  openChooser: () => {
    revealOnSettings(SETTINGS_SURFACE, document.getElementById("measurementUnitSelect"), {
      group: ".settings-section",
    });
  },
});
const documentProtection = createDocumentProtection({
  getDoc: () => doc,
  runEdit,
  registerModal,
  fallbackFocus: () => pagesEl,
  bindRadioGroup,
  setStatus,
  participantRefusal: () => SESSION.refusalFor("review.restrictEditing"),
  onChanged: () => updateToolbar(),
});
// ADR-061's three review seams are inline on purpose: `compare_documents.mjs`
// owns the ORDER of "apply the sidecar, repaint, turn the markup on, re-render the
// gutter" because that order is the decision, and this is the 93%-of-the-webapp
// module with no mount seam (`109` HF-085). `landed` doubles as the capability
// test — withheld, the panel claims no tracked changes it cannot write.
const comparePanel = bindComparePanel({ doc: () => doc, currentBytes: () => comparableBytes(doc, currentSourceFormat), engine: { begin: beginVersionDiff, slice: defaultDiffSlice }, yieldToHost: () => new Promise((resolve) => requestAnimationFrame(() => resolve())), setStatus: (text, kind) => setStatus(text, kind), allowed: () => HOST_CAPS.has("open"), refusedReason: t("capability.notGranted"), blockedReason: () => (blockMutationInViewing() ? mutationBlockedMessage({ editingUnavailableReason: readOnlyReason }) : ""), readOnlyReason: () => readOnlyReason, landed: async (res) => { await applyEditResult(res); await setShowingChanges(true); scheduleReviewMarginRender(); } });

// The band's structural controls, declared in `table_band.mjs` (`109` UX-005).
// Its Select handler used to be a second copy of `selectTableContext`.
bindTableBand({
  root: tableRibbon,
  onButton,
  getDoc: () => doc,
  getSelection: () => selection,
  runEdit,
  clearTableSelection: () => {
    tableRange.clear();
  },
  selectTableContext,
  caretTableColumn,
});

onButton(mergeCellsBtn, async () => {
  // The "select something first" branch that used to be here was UNREACHABLE —
  // the button is disabled whenever fewer than two cells are selected — and it
  // carried a second, divergent copy of the menu's own sentence. The band's
  // disabled reason is now that one sentence, from the catalogue (`docs/141`
  // TBL-03, TBL-05).
  //
  // `mergeTableCellRange` takes the RECTANGLE, not one of the three degenerate
  // shapes a mode string could name, so an arbitrary drag merges (TBL-17).
  const range = tableRange.get();
  if (!selection || !doc || !range) return;
  await runEdit(() => doc.mergeTableCellRange(range.anchorNode, range.focusNode), { gate: true });
  tableRange.clear();
  updateToolbar();
});

const splitCellDialogView = bindSplitCellDialog({
  dialog: splitCellDialog,
  rows: splitCellRows,
  columns: splitCellColumns,
  open: splitCellBtn,
  close: splitCellClose,
  cancel: splitCellCancel,
  confirm: splitCellConfirm,
  fallbackFocus: () => pagesEl,
  registerModal,
  onButton,
  runEdit: (fn, options) => runEdit(fn, options),
  doc: () => doc,
  caretNode: () => (selection && doc ? selection.focus.node : ""),
  afterSplit: () => {
    tableRange.clear();
    updateToolbar();
  },
});
const toggleSplitCellDialog = (open) => splitCellDialogView.toggle(open);

// -- Table properties inspector ----------------------------------------------
let tablePropertiesCurrent = null;
let tablePropertiesNode = null;

/** An optional inches field's value → twips, or -1 for "leave it unset". */
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
  tableAlignGroup.reflect(info.alignment);
  updateTableRowHeightField();

  tablePropertiesCurrent = {
    alignment: info.alignment,
    tableWidthTwips: optionalInchesToTwips(tableWidth.value),
    tableIndentTwips: signedInchesToTwips(tableIndent.value),
    fixedLayout: info.fixedLayout,
    headerRow: info.headerRow,
    columnWidthTwips: optionalInchesToTwips(tableColumnWidth.value),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalInchesToTwips(tableRowHeight.value),
    rowHeightRule: info.rowHeightRule || "auto",
    cellMarginTwips: optionalInchesToTwips(tableCellMargin.value),
    cellSpacingTwips: optionalInchesToTwips(tableCellSpacing.value),
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
      tableAlignGroup.focusSelected(),
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
document.getElementById("tablePropertiesClose").addEventListener("click", () => toggleTableProperties(false));
const tableAlignGroup = bindRadioGroup(tableAlign, {
  attr: "data-talign",
  onSelect: () => commitTableProperties(),
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
    alignment: tableAlignGroup.value() ?? "left",
    tableWidthTwips: optionalInchesToTwips(tableWidth.value),
    tableIndentTwips: signedInchesToTwips(tableIndent.value),
    fixedLayout: tableFixedLayout.checked,
    headerRow: tableHeaderRow.checked,
    columnWidthTwips: optionalInchesToTwips(tableColumnWidth.value),
    rowHeightTwips:
      tableRowHeightRule.value === "auto" ? -1 : optionalInchesToTwips(tableRowHeight.value),
    rowHeightRule: tableRowHeightRule.value,
    cellMarginTwips: optionalInchesToTwips(tableCellMargin.value),
    cellSpacingTwips: optionalInchesToTwips(tableCellSpacing.value),
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
// The grid picker is a dialog-opening insert, so it refuses BEFORE it opens —
// the same as Symbol, Emoji, Field and Drop cap, and for the reason
// `insert-surface.spec.mjs` states for those: a reader must never be led into
// choosing a size that cannot be applied. Registered ahead of `registerPopover`
// so it runs first on the same button, and it stops there; the refusal itself is
// still `blockMutationInViewing()`, the one choke point, not a disabled control.
insertTableBtn.addEventListener("click", (event) => {
  if (!insertTableMenu.hidden || !blockMutationInViewing()) return;
  event.preventDefault();
  event.stopImmediatePropagation();
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
  renderAccessibilityMirror(doc, selection?.focus?.node ?? "", tableRange.gridRect());
}

// ---- Outline panel (heading tree → scroll-to) -------------------------------
// The rows themselves are `outline_panel.mjs`; what is left here is the two
// pieces of editor state it needs and the caret move a row performs.
/** Rebuilds the outline list from the document's headings (no-op when hidden). */
function buildOutline() {
  if (!doc || outlinePanel.hidden) return;
  foldView.sync();
  renderOutline(outlineBody, doc.documentOutline(), {
    emptyText: t("outline.noHeadings"),
    onPick: navigateToNode,
    activeNode: selection?.focus?.node ?? "",
    ...foldView.outlineOptions(),
  });
}

/** Keeps the outline's active row synchronized with the model-backed caret. */
function reflectOutlineSelection() {
  reflectOutlineActive(outlineBody, selection?.focus?.node ?? "");
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

/** Re-anchors everything positioned against PAGE geometry. Every flanking panel
 *  changes the canvas WIDTH, which re-centres the sheet, and the review markers
 *  and fold chevron are positioned against where the sheet WAS. `toggleReview`
 *  re-rendered the markers and `toggleOutline` did not, so opening and closing
 *  the outline left comment icons inside the page instead of beside it. */
function reanchorPageOverlays() {
  scheduleReviewMarginRender();
  if (doc) drawSelection();
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
  reanchorPageOverlays();
}
railOutline.addEventListener("click", toggleOutline);
outlineClose.addEventListener("click", toggleOutline);

/** The Pages navigator. Its behaviour lives in `pages_panel.mjs` (HF-085): six
 *  closures in this file were the only description of what `view.pages` does. */
const pagesPanelView = createPagesPanel({
  panel: pagesPanel,
  body: pagesBody,
  railButton: railPages,
  closeButton: pagesClose,
  viewport: viewportEl,
  // Withheld in reflow, WITH the reason: a navigator that says "page 7" about a
  // rasterisation unit is a lie the reader cannot see through (`151` §6.4).
  withheldReason: () => reflowView.withheldReason(),
  onWithheld: (reason) => setStatus(reason),
  getDoc: () => doc,
  getSelection: () => selection,
  getPages: () => pages,
  getBandModel: () => pageBandModel,
  pageInView,
  bandTop: () => bandTopInScroller,
  onExclusive: () => {
    outlinePanel.hidden = true;
    railOutline.setAttribute("aria-pressed", "false");
    if (!reviewSidebar.hidden) toggleReview(false);
    reanchorPageOverlays();
  },
  onJumped: () => {
    updatePageWindow();
    paintPagesInView();
  },
});

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
  reanchorPageOverlays();
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
  // One reading of "does this document carry a change", not two: this had its own
  // copy of `documentHasTrackedChanges`'s parse-and-count (SKILL §8).
  if (!documentHasTrackedChanges()) {
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
    { id: "file.new", label: "New blank document", group: "File", kw: "new blank empty create start untitled document", noDoc: true, enabled: hostCapabilities().has("new"), disabledReason: t("capability.embedded"), run: () => void newBlankDocument() },
    { id: "file.open", label: "Open…", group: "File", kw: "load docx odt json txt", noDoc: true, enabled: hostCapabilities().has("open"), disabledReason: t("capability.embedded"), run: () => fileEl.click() },
    { id: "file.save", label: "Save", group: "File", kw: "export download", enabled: HOST_CAPS.has("save"), disabledReason: t("capability.notGranted"), run: () => saveDocument() },
    ...exportCommands(exportDocumentAs, HOST_CAPS.has("download"), t("capability.notGranted"), doc ? doc.availableExportFormats() : null),
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
    { id: "file.print", label: "Print", group: "File", kw: "print pages paper hard copy pdf", enabled: HOST_CAPS.has("print"), disabledReason: t("capability.notGranted"), run: () => void printDocument(doc).finally(() => { void renderAll(); scheduleChromeRefresh({ stats: true, outline: true }); }) },
    { id: "file.properties", label: "Document properties", group: "File", kw: "metadata title author", run: () => propertiesUi.toggle(true) },
    // Version history. `docs/139` §8.1's first entry point, and the primary one:
    // File is where Docs, ONLYOFFICE and Word all keep it, and a File row costs
    // no ribbon width at all. The View band carries the second durable surface.
    //
    // NEVER a dead control: with no document, with the preference off, with
    // autosave off, in a framed editor or with a store the browser refused, this
    // row is present and DISABLED WITH THE REASON — which is a different sentence
    // in each of those five cases, because the way out of each one is different.
    //
    // A WITHHELD REGION is the one case that is silent instead, and that is the
    // distinction `docs/126` draws: "never, for you" is composition and says
    // nothing, "not right now" is state and explains itself. Taking the row out
    // of the REGISTRY rather than hiding a button is what makes the composition
    // complete — the palette and the ⌘⌥⇧H chord read the registry, and neither
    // belongs to a region CSS could reach.
    ...(HOST_CHROME.editing.has("history")
      ? [
          {
            id: "file.versionHistory",
            label: t("versionHistory.command"),
            group: "File",
            kw: "version history timeline earlier previous restore revert named checkpoint past revision",
            enabled: !!doc && versionHistory.available(),
            disabledReason: versionHistory.unavailableReason(),
            run: () => void versionHistory.toggle(),
          },
        ]
      : []),
    {
      id: "edit.undo",
      label: doc?.undoLabel ? `Undo ${doc.undoLabel}` : "Undo",
      group: "Edit",
      kw: "revert",
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
      contextMenu: true,
      enabled: !!doc,
      run: () => selectAll(),
    },
    { id: "edit.find", label: "Find and replace", group: "Edit", kw: "search replace", run: () => openFind() },
    { id: "format.bold", label: "Bold", group: "Format", kw: "strong", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("bold") },
    { id: "format.italic", label: "Italic", group: "Format", kw: "emphasis", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("italic") },
    { id: "format.underline", label: "Underline", group: "Format", kw: "", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("underline") },
    { id: "format.strike", label: "Strikethrough", group: "Format", kw: "strike", enabled: !!selection, disabledReason: "Place the caret or select text", run: fmt("strike") },
    { id: "format.superscript", label: "Superscript", group: "Format", kw: "raise exponent", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => superBtn.click() },
    { id: "format.subscript", label: "Subscript", group: "Format", kw: "lower", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => subBtn.click() },
    { id: "format.clear", label: "Clear direct formatting", group: "Format", kw: "reset defaults", enabled: !!selection, disabledReason: "Place the caret or select text", run: () => clearFormattingBtn.click() },
    { id: "format.painter", label: "Format painter", group: "Format", kw: "copy formatting paint brush clone style match", enabled: !!selection, disabledReason: "Place the caret or select text to copy its formatting", run: () => armFormatPainter(false) },
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
    { id: "format.color", label: "Text color…", group: "Format", kw: "font foreground colour", enabled: !!selection, disabledReason: "Place the caret or select text", run: (anchor) => openPopover(textColorPopover, { anchor }) },
    { id: "format.highlight", label: "Highlight color…", group: "Format", kw: "marker colour", enabled: !!selection, disabledReason: "Place the caret or select text", run: (anchor) => openPopover(highlightPopover, { anchor }) },
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
    { id: "insert.lineBreak", label: "Line break", group: "Insert", kw: "soft line break newline same paragraph shift enter", enabled: !!selection && reviewMode !== "suggesting", disabledReason: reviewMode === "suggesting" ? "Line breaks cannot be tracked yet" : "Place the caret where the break belongs", run: () => void insertLineBreakAtSelection() },
    { id: "insert.link", label: "Add or edit link", group: "Insert", kw: "hyperlink url bookmark toc", enabled: insertCommandEnabled("insert.link", context), disabledReason: "Select text to add a link", run: () => editSelectionLink() },
    { id: "layout.firstPageVariant", label: t("headerFooter.firstPageSwitch", { state: t(headerFooterSettings.variantState().firstPage ? "headerFooter.stateOn" : "headerFooter.stateOff") }), group: "Layout", kw: "different first page header footer title page cover", enabled: !!doc, disabledReason: "Open a document first", run: () => headerFooterSettings.toggleVariant("firstPage") },
    { id: "layout.evenOddVariant", label: t("headerFooter.evenOddSwitch", { state: t(headerFooterSettings.variantState().evenOdd ? "headerFooter.stateOn" : "headerFooter.stateOff") }), group: "Layout", kw: "different odd even pages header footer mirrored", enabled: !!doc, disabledReason: "Open a document first", run: () => headerFooterSettings.toggleVariant("evenOdd") },
    { id: "layout.headerFooterSettings", label: t("headerFooter.settingsCommand"), group: "Layout", kw: "header footer position from top bottom distance page numbering number format start at continue link to previous", enabled: !!doc, disabledReason: "Open a document first", run: () => headerFooterSettings.open(true) },
    { id: "insert.footnote", label: "Footnote", group: "Insert", kw: "footnote note reference citation bottom of page", enabled: !!selection, disabledReason: "Place the caret where the note belongs", run: () => insertNote("footnote") },
    { id: "insert.endnote", label: "Endnote", group: "Insert", kw: "endnote note reference citation end of document", enabled: !!selection, disabledReason: "Place the caret where the note belongs", run: () => insertNote("endnote") },
    { id: "insert.header", label: "Edit header", group: "Insert", kw: "header running title page top margin", enabled: !!doc, disabledReason: "Open a document first", run: () => editRunningContent("header") },
    { id: "insert.footer", label: "Edit footer", group: "Insert", kw: "footer running page number bottom margin", enabled: !!doc, disabledReason: "Open a document first", run: () => editRunningContent("footer") },
    { id: "insert.bookmark", label: "Bookmark…", group: "Insert", kw: "bookmark manager navigate create rename delete go to", enabled: insertCommandEnabled("insert.bookmark"), run: () => openBookmarkManager() },
    { id: "insert.field", label: "Field…", group: "Insert", kw: "field placeholder page number of pages date time file name author auto update", enabled: insertCommandEnabled("insert.field"), run: () => openFieldDialog() },
    { id: "insert.dropCap", label: t("dropCap.command"), group: "Insert", kw: "initial letter dropped margin lines paragraph", enabled: insertCommandEnabled("insert.dropCap"), run: () => dropCapDialog.open() },
    { id: "insert.image", label: "Picture…", group: "Insert", kw: "image picture insert photo file png jpeg jpg gif paste", enabled: insertCommandEnabled("insert.image"), run: () => insertImageFromFile() },
    { id: "insert.shape", label: "Shape…", group: "Insert", kw: "shape drawing autoshape rectangle rounded ellipse circle triangle diamond line arrow callout", enabled: insertCommandEnabled("insert.shape"), run: () => openShapeGallery() },
    { id: "insert.chart", label: t("insert.chart"), group: "Insert", kw: "chart graph column bar line area scatter pie doughnut plot data series", enabled: !!selection, disabledReason: t("paragraph.caretRequired"), run: () => void insertChartAtCaret({ doc, caret: selection?.focus, blocked: blockMutationInViewing, suggesting: () => reviewMode === "suggesting", status: setStatus, apply: applyEditResult }) },
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
    // The outline lives in the RAIL, so a container that withheld the rail is not
    // handed a command that opens it — the palette and the chord belong to no
    // region, the hole `history` closed the same way ("no panels and nothing").
    // `view.pages` beside it, and gated the same way for the same reason. The
    // two panels are the rail's, so a HOST that withheld the rail region has
    // withheld the panels with it and must not be handed a command that opens
    // one. A PHONE is a different question and the opposite answer: it stops
    // painting the rail strip while keeping both panels, which is why this had
    // to become a command rather than staying a tile (`docs/148` §5.3a). The
    // label is the catalogue's existing `pagesPanel.pages`, so this adds no
    // English literal to a file that is at an unrouted-string ceiling and no
    // key to eighteen catalogues that already answer this one.
    ...(HOST_CHROME.editing.has("rail")
      ? [
          { id: "view.outline", label: "Toggle outline", group: "View", kw: "headings navigation", run: () => toggleOutline() },
          { id: "view.pages", label: t("pagesPanel.pages"), group: "View", kw: "pages panel thumbnails navigator go to page jump browse", enabled: !reflowView.withheldReason(), disabledReason: reflowView.withheldReason(), run: () => pagesPanelView.toggle() },
        ]
      : []),
    { id: "view.showChanges", label: "Show changes (read-only)", group: "View", kw: "tracked changes markup deletions insertions review redline", run: () => toggleShowChanges() },
    { id: "view.reflow", label: t(reflowView.isOn() ? "reflow.commandOn" : "reflow.commandOff"), group: "View", kw: "reflow pageless continuous column mobile phone reader web layout wrap width", enabled: !readOnlyReason, disabledReason: t("reflow.unavailable"), run: () => reflowView.toggle() },
    ...reflowView.commands(),
    // Fold at the caret, Collapse/Expand All and the level rungs, generated from
    // the module's one table so no two surfaces can offer different sets.
    ...foldView.commands(),
    ...collabCommands({ transport: () => collab, t }),
    // The ¶ toggle and its five switches, the measurement-unit preference, and
    // Restrict Editing. Each module generates its own rows from its own table, so
    // the palette, the menu and the control cannot offer different sets — and
    // `main.js` carries one line per capability instead of eight command literals.
    ...formattingMarks.commands(),
    ...measurement.commands(),
    ...documentProtection.commands(),
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
    { id: "view.settings", label: "Settings", group: "View", kw: "theme accent dark appearance preferences identity author name initials", noDoc: true, run: () => revealOnSettings(SETTINGS_SURFACE) },
    { id: "layout.pageSetup", label: "Page setup", group: "Layout", kw: "margins orientation paper size", run: () => pageSetup.open(true) },
    { id: "layout.paragraph", label: "Paragraph properties", group: "Layout", kw: "spacing borders shading indent", enabled: !!selection, disabledReason: "Place the caret in a paragraph", run: () => toggleParagraphProperties(true) },
    { id: "layout.tabStops", label: t("tabStops.command"), group: "Layout", kw: "tab tabs stop stops ruler decimal bar align position", enabled: !!selection && reviewMode !== "viewing", disabledReason: selection ? mutationBlockedMessage({ editingUnavailableReason: readOnlyReason }) : t("paragraph.caretRequired"), run: () => tabStopsDialog.open() },
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
    { id: "help.commands", label: "Find a command…", group: "Help", kw: "help command palette search run", noDoc: true, run: () => openCmd() },
    { id: "help.shortcuts", label: "Keyboard shortcuts", group: "Help", kw: "help shortcuts keys chords reference cheat sheet", noDoc: true, run: () => toggleShortcutsReference(true) },
    { id: "help.about", label: "About OpenDoc", group: "Help", kw: "about version licence license apache build source repository issue report credits", noDoc: true, run: () => toggleAbout(true) },
    // Resolve/Delete must have a menu home, not only a ribbon button:
    // `docs/105` command-surface parity forbids a capability reachable from one
    // surface, and `review-surface.spec.mjs` enforces it.
    { id: "review.comment.resolve", label: t("reviewComment.resolve"), group: "Review", kw: "resolve close comment thread done", enabled: !!activeReviewCommentId && HOST_CAPS.has("comment"), disabledReason: t("reviewComment.needsCaret"), run: () => void reviewCommentActions.resolve() },
    { id: "review.comment.delete", label: t("reviewComment.delete"), group: "Review", kw: "delete remove comment thread", enabled: !!activeReviewCommentId && HOST_CAPS.has("comment"), disabledReason: t("reviewComment.needsCaret"), run: () => void reviewCommentActions.remove() },
    {
      id: "review.comment",
      label: "Add comment",
      group: "Review",
      kw: "annotate note",
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
    { id: "tools.languages", label: t("proofLanguages.command"), group: "Tools", kw: "language pack proofing spelling dictionary install download offline locale manage languages", noDoc: true, run: () => proofing.openLanguages() },
    { id: "review.toggle", label: "Toggle comments & suggestions", group: "Review", kw: "sidebar review panel", run: () => toggleReview() },
    { id: "review.compare", label: t("compare.command"), group: "Review", kw: "compare diff difference changes two documents review merge", enabled: !!doc, disabledReason: t("compare.needsDocument"), run: () => comparePanel.open() },
    // ⌘⇧E worked before this row and was advertised NOWHERE — no palette row, no
    // menu row, no reference entry — because the binding was a hand-written
    // keydown branch and only descriptors carry labels (`109` UX-007). It exists
    // as a command now, so the chord is discoverable on every surface. Google Docs
    // cycles its three modes from one chord too; Word has no equivalent.
    { id: "review.mode.cycle", label: "Next review mode", group: "Review", kw: "review mode cycle switch editing suggesting viewing next", run: () => cycleReviewMode() },
    { id: "review.mode.editing", label: "Editing mode", group: "Review", kw: "review mode edit", run: () => setReviewMode("editing") },
    { id: "review.mode.suggesting", label: "Suggesting mode (track changes)", group: "Review", kw: "review mode track changes suggest", run: () => setReviewMode("suggesting") },
    { id: "review.mode.viewing", label: "Viewing mode (read-only)", group: "Review", kw: "review mode view read only", run: () => setReviewMode("viewing") },
    { id: "review.next", label: "Next comment or change", group: "Review", kw: "revision suggestion comment navigate forward", run: () => navigateReview(1) },
    { id: "review.previous", label: "Previous comment or change", group: "Review", kw: "revision suggestion comment navigate back", run: () => navigateReview(-1) },
    { id: "review.acceptAtCaret", label: "Accept change at cursor", group: "Review", kw: "revision suggestion approve current", run: () => decideReviewAtCaret(true) },
    { id: "review.rejectAtCaret", label: "Reject change at cursor", group: "Review", kw: "revision suggestion discard current", run: () => decideReviewAtCaret(false) },
    { id: "review.acceptNext", label: "Accept change and move to next", group: "Review", kw: "revision suggestion approve next advance", run: () => decideReviewAndAdvance(true) },
    { id: "review.rejectNext", label: "Reject change and move to next", group: "Review", kw: "revision suggestion discard next advance", run: () => decideReviewAndAdvance(false) },
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
  // The line-spacing presets and the two one-gesture space rows, generated from
  // the popover's own markup by `spacing_menu.mjs` so a preset added there gets
  // its command — and its ⌘1 / ⌘5 / ⌘2 chord's meaning — for free.
  cmds.push(...spacingMenuControl.commands({ enabled: !!doc && !!selection }));
  if (doc) {
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
    for (const points of FONT_SIZE_STEPS) {
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
    // Table STYLE, generated from the engine's own list — the very list the band's
    // chooser renders (`populateTableStyles`). It was the one table capability with
    // a single surface: the `#tableStyleBtn` popover and nothing else, no menu row
    // and no palette row, so applying a table style was mouse-only (`109` UX-005).
    // Generated rather than enumerated, for the same reason `style.<name>` is: a
    // style the document defines gets its command without anyone writing it down.
    const applyTableStyle = (name) =>
      runEdit(() => doc.applyTableStyle(selection.focus.node, name), { gate: true });
    cmds.push({ id: "table.style.none", label: NO_TABLE_STYLE, group: "Table", kw: "table style clear none plain remove banding", run: () => applyTableStyle("") });
    for (const name of doc.listTableStyles?.() ?? []) {
      cmds.push({
        id: `table.style.${name}`,
        label: `Table style: ${name}`,
        group: "Table",
        kw: `table style banding grid ${name}`.toLowerCase(),
        run: () => applyTableStyle(name),
      });
    }
  } else if (context.surface === "menu" || context.surface === "compact") {
    // The Table MENU must exist even when the caret is not in a table: an empty
    // popover says the editor cannot edit tables (UX-012). `command_taxonomy`
    // builds the greyed rows from the same label map the live rows use, and
    // `menu_taxonomy` fails if that map and the real command set disagree.
    // The COMPACT bar's Table dropdown is the same surface by the same argument
    // (TBL-18): in compact mode there is no ribbon, so this dropdown is where a
    // user learns tables are editable at all, and an empty one says they are not.
    cmds.push(
      // The same sentence the BAND shows for the same precondition, from the one
      // catalogue entry (`docs/141` TBL-03).
      ...tableMenuPlaceholders(doc ? t("table.reason.caretOutsideTable") : "Open a document first"),
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
      ...flattenCommandTree(buildObjectContextCommands(selectedObjectContext(), objectContextMenuHost), (entry, trail) => ({
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
  // The chord a command advertises comes from the keymap, never from a literal
  // written here (`109` UX-006/UX-007). This one line is what makes a label that
  // disagrees with its binding unrepresentable: the palette hint, the menu hint,
  // the compact bar's tooltip and the shortcut reference all read
  // `command.shortcut`, and the only thing that can set it is the table the
  // dispatcher matches against.
  for (const command of cmds) command.shortcut = shortcutForCommand(command.id, EDITOR_KEYBOARD_PLATFORM);
  // Narrowed to the ROOM's grant HERE, once, for every surface this registry
  // feeds — palette, menu bar, compact bar, context menus — rather than by a
  // clause in each governed row. A review command added later is governed by the
  // id it already has instead of shipping ungated (`SKILL` §10: fix the pattern).
  return SESSION.narrow(cmds.filter((command) => doc || command.noDoc));
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
      if (pane === "about") toggleAbout.stampVersion?.(); else if (pane === "pageSetup") pageSetup.reflect?.();
      if (pane === "properties") propertiesUi.fill();
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
      headings: true,
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
  if (!doc || !selection) return;
  if (blockMutationInViewing()) return;
  breakTypingSession();
  if (blockUntrackedInSuggesting()) return;
  const { node, offset } = selection.focus;
  let res;
  try {
    res = doc.insertField(node, offset, kind, fieldResultText(kind, { fileName: currentName, authorName: settings.authorName }));
  } catch (err) {
    console.warn("insertField ignored:", err?.message ?? err);
    setStatus("A field can’t be inserted at this position", "error");
    focusEditorSurface();
    return;
  }
  await applyEditResult(res);
  setStatus(`Inserted ${fieldLabel(kind)}`);
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

/** Word's Insert ▸ Shapes. Inserts a floating preset shape and leaves it
 *  SELECTED (not entered) — a shape has no text until you ask for some.
 *
 *  `at` is the rectangle the user DREW, in page-local twips, or null for the
 *  plain command. `insertShape` authors a fixed 2"x1" at the caret's paragraph,
 *  so the drawn geometry is applied by the resize that follows — one more
 *  undoable action, which is why the two are reported as one gesture in the
 *  status line rather than as two. */
async function insertShapeObject(geometry, at = null) {
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
  // Word leaves a new shape SELECTED, not entered — which also puts Fill and
  // Outline within reach straight away.
  const shape = newestObject(before, "shape");
  if (shape) selectObject(shape.node, shape.kind, null, shape.anchored, shape);
  if (at && shape) {
    try {
      await applyEditResult(
        doc.resizeObject(
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
      setStatus(editRefusalMessage(err, { routeRefusal: SESSION.sentenceFor }), "error");
    }
  }
  const key = shapeNameKey(geometry);
  setStatus(t("shape.added", { shape: key ? t(key) : "Shape" }));
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

const dropCapDialog = createDropCapDialog({
  registerModal, getDoc: () => doc, selectionNode: () => selection?.focus.node ?? "",
  mutationBlocked: () => blockMutationInViewing() || blockUntrackedInSuggesting(),
  apply: (mode, lines) => runEdit(() => doc.setDropCap(selection.focus.node, mode, lines), { gate: true }),
  status: setStatus, fallbackFocus: () => pagesEl,
});

// ---- References ▸ captions and cross-references (OO-005) --------------------
// Both dialogs and the caption-numbering cache are `reference_commands.mjs`; what
// is here is the application state they read. Every insert goes through the SAME
// gated, tracked, undoable path as every other edit. The caption's target is the
// caret's block — selecting a picture or table puts the caret at its anchor.
const referenceCommands = createReferenceCommands({
  registerModal, getDoc: () => doc, status: setStatus, fallbackFocus: () => pagesEl,
  mutationBlocked: () => blockMutationInViewing() || blockUntrackedInSuggesting(),
  targetNode: () => selection?.focus.node ?? "",
  caret: () => (selection ? { node: selection.focus.node, offset: selection.focus.offset } : null),
  applyEdit: (run) => runEdit(run, { gate: true }),
});

// References ▸ Table of contents. Same shape and the same cache discipline as
// the caption numbering beside it: the count the ribbon reads is refreshed on
// the deliberate interactions that change it, never on the keystroke path.
const tocCommands = createTocCommands({
  registerModal, getDoc: () => doc, status: setStatus, fallbackFocus: () => pagesEl,
  mutationBlocked: () => blockMutationInViewing() || blockUntrackedInSuggesting(),
  caretNode: () => selection?.focus.node ?? "",
  applyEdit: (run) => runEdit(run, { gate: true }),
  changed: () => updateToolbar(),
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
    if (c.enabled === false && c.disabledReason) item.title = c.disabledReason;
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
  // From the File page's PANE there is no modal for `closeCmd` to close, so
  // the command ran with the page still over the document. No-op when closed.
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
/** Runs the command `id` names, or says why it cannot run.
 *
 *  A chord that exists and does nothing is a dead control (SKILL.md §10), so a
 *  shortcut whose command is unavailable refuses OUT LOUD with the same reason
 *  the ribbon button and the menu row give — and since `109` UX-017 that reason
 *  is perceivable at phone widths and to a screen reader too. Returns false only
 *  when no such command exists in the current state, which is how a chord whose
 *  command needs a document (⌘S) still falls through to the browser when nothing
 *  is open, exactly as it did before.
 *
 *  Cost: one `editorCommands()` build per chord press. That is the same build the
 *  command palette already performs on EVERY keystroke typed into its input
 *  (`renderCommands` → `buildCommands`), so this is an accepted cost on an
 *  existing path rather than a new one — but it is not O(1): `documentHasObjects`
 *  inside the build calls `objectOrder()` and parses it, which is O(objects).
 *  That is pre-existing and worth its own row; it is not introduced here.
 */
function runCommandById(id) {
  const command = editorCommands({ surface: "palette" }).find((c) => c.id === id);
  if (!command) return false;
  if (command.enabled === false) {
    setStatus(command.disabledReason ?? "That command is not available right now", "error");
    return true;
  }
  command.run();
  return true;
}

// THE dispatcher: the one place a keyboard chord becomes a command (`109`
// UX-006). It replaces four separate `keydown` handlers that each carried their
// own hand-written if-chain, which is why the editor could advertise a chord it
// had never bound and bind one it advertised nowhere.
//
// No modal check here, and none is needed: `modal.mjs` already swallows
// application chords in the capture phase while a dialog is open, with an
// allowlist for text-editing chords and a `toggleChord` escape hatch. That is
// ONLYOFFICE's `common/main/lib/util/Shortcuts.js:96-107` suspend/resume, with
// the two refinements their version lacks.
//
// `stopImmediatePropagation` once a chord is claimed, because a chord the table
// owns must not also be read by anything else — that is what made ⌘⌥⏎ need it by
// hand, or it would both decide a tracked change and insert a paragraph. Every
// other document-level `keydown` listener in this file is Escape-only or
// popover-only, so nothing legitimate is cut off.
document.addEventListener("keydown", (e) => {
  const id = chordCommand(e, EDITOR_KEYBOARD_PLATFORM, {
    inEditor: !isInteractiveChromeTarget(e.target) && eventTargetsEditor(e),
  });
  if (!id) return;
  // The palette is the one command that toggles: its own chord has to close it
  // as well as open it.
  if (id === "help.commands" && cmdModal.isOpen) {
    e.preventDefault();
    e.stopImmediatePropagation();
    closeCmd();
    return;
  }
  if (!runCommandById(id)) return;
  e.preventDefault();
  e.stopImmediatePropagation();
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

// A find match spans a single paragraph, so match.startNode === match.endNode.
// Accept iff [match.startOffset .. match.endOffset] on that node lies within the
// ordered scope [scopeStart .. scopeEnd]. Boundary-node matches (the common
// single-paragraph case and the first/last paragraph of a multi-paragraph
// scope) resolve with zero extra WASM calls; only genuinely-interior nodes need
// the two order tests.
function matchInFindSelection(match) {
  if (!findSelection.checked) return true;
  if (!findScope) return false;
  return matchWithinScope(match, findScope, positionComparator(doc));
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
// The find bar's five buttons, looked up where they are USED: one listener each,
// and five names in the widest scope in the product for them.
document.getElementById("findPrev").addEventListener("click", () => findFromSelection(false));
document.getElementById("findNext").addEventListener("click", () => findFromSelection(true));
document.getElementById("replaceOne").addEventListener("click", replaceCurrentMatch);
document.getElementById("replaceAll").addEventListener("click", replaceAllMatches);
document.getElementById("findClose").addEventListener("click", closeFind);
findBtn.addEventListener("click", () => openFind());

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
  // …plus whatever the touch handles need. They hang above the selection's top
  // and below its bottom, and this bar is a FIXED element over the page: eight
  // pixels of gap put it through the middle of the start handle's dot and took
  // every touch aimed at it (`touch_selection.mjs`'s `clearance`).
  const gap = 8 + touchSelection.clearance();
  let y = top - th - gap;
  if (y < topBound) y = bottom + gap; // no room above → drop below the selection
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
/** Serializes the edited document through the selected registered exporter and
 *  downloads it. Saving back to the source format preserves unchanged bytes where
 *  safe; a different target uses the semantic writer. */
/** Exports the current document through the given registered exporter and
 *  downloads it. Saving back to the source format preserves unchanged bytes where
 *  safe; a different target uses the semantic writer. Shared by the Save button,
 *  the ⌘S shortcut, and the File ▸ Export-as menu entries. */
function exportDocumentAs(targetFormat, intent = "export") {
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
    const saved = downloadBytes(bytes, mimeType, downloadNameForFormat(currentName, extension), document);
    hostSession?.noteWrite(intent, { format: targetFormat, name: saved, bytes: bytes.length });
    markDocumentSaved();
    showCompatibilityFindings(compatibilityStatusEl, findings, "export");
    setStatus(
      findings === 0
        ? `Saved ${saved}`
        : `Saved ${saved} with ${findings.toLocaleString()} compatibility finding${findings === 1 ? "" : "s"}`,
    );
  } catch (err) {
    console.error(err);
    setStatus(`Save failed: ${err?.message ?? err}`, "error");
  }
}

/** Saves using the format chosen in the Save-format selector (default: source). */
function saveDocument() {
  exportDocumentAs(saveFormatEl && saveFormatEl.value ? saveFormatEl.value : currentSourceFormat, "save");
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
        tableRange.clear();
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
  tableRange.clear();
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
      const fragment = JSON.stringify({ blocks: parsed.blocks });
      const insert = (a, f) => doc.pasteStructured(a.node, a.offset, f.node, f.offset, fragment);
      if (reviewMode !== "suggesting" && (await runStructuredPaste(insert))) {
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
    const fragment = JSON.stringify(structured);
    const insert = (a, f) =>
      doc.pasteExternalStructured(a.node, a.offset, f.node, f.offset, fragment);
    if (structured && (await runStructuredPaste(insert))) return true;
  }
  const runs = htmlToRuns(parsed.body);
  if (!runs.length) return false;
  await pasteRichRunsJson(JSON.stringify(runs));
  return true;
}

/** Editing-mode paste of whole-block structure (tables and list paragraphs) at
 * the caret, as one undoable action. `insert(anchor, focus)` names the engine
 * call: `pasteStructured` for an internal OpenDoc copy, `pasteExternalStructured`
 * for foreign HTML parsed by `htmlToStructured`. Returns true when applied; false
 * when the engine declines (a range selection, or a caret that is not a top-level
 * body paragraph), so the caller falls back to the flat rich-run paste. Calls the
 * engine directly (not through `runEdit`, which swallows the decline) so the
 * fallback can see it. ONE helper rather than the two that used to sit here: they
 * differed only in which engine method they named, and two copies of a paste path
 * is where the loss report below gets wired into one and forgotten in the other. */
async function runStructuredPaste(insert) {
  if (!doc || !selection) return false;
  const { anchor, focus } = selection;
  breakTypingSession();
  let res;
  try {
    res = insert(anchor, focus);
  } catch {
    return false;
  }
  // Five families of reference cannot be duplicated inside one document, and are
  // REPORTED because `AGENTS.md` forbids silent data loss. READ before
  // `applyEditResult`, which calls `res.free()` — reading a freed wasm object
  // throws "null pointer passed to rust" and failed all four structured-paste
  // specs on the first draft. SAID after, so the paint's own status line cannot
  // overwrite it, and as `"error"`: the paste succeeded, but "your comment anchor
  // did not come across" is must-notice, and only that kind reaches the toast and
  // the assertive region (`status_policy.mjs`).
  const loss = pasteLossMessage(res.pasteLoss);
  await applyEditResult(res);
  if (loss) setStatus(loss, "error");
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
const pasteOptionsMergeBtn = document.getElementById("pasteOptionsMerge");
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
onButton(document.getElementById("pasteOptionsTextOnly"), () => void switchPasteToTextOnly());
onButton(pasteOptionsMergeBtn, () => void switchPasteToMergeFormatting());
onButton(document.getElementById("pasteOptionsClose"), hidePasteOptions);
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

document.addEventListener("keydown", async (e) => {
  if (!doc) return;
  // The canvas editor owns keystrokes only while its focus owner is active.
  // Chrome controls, popovers, and link chips keep normal browser semantics.
  if (isInteractiveChromeTarget(e.target) || !eventTargetsEditor(e)) return;

  const mod = e.metaKey || e.ctrlKey;
  const key = e.key;

  if (composingText || e.isComposing || key === "Process") return;

  // The object interaction grammar (docs/85 §4) owns the keyboard while an
  // object is selected. Escape is the two-step exit (editing → selected → text);
  // Enter/Delete act on the object; a selected object swallows text keys so a
  // stale caret is never edited.
  if ((objectResize.active() || objectRotate.active() || objectMoveDrag) && key === "Escape") {
    e.preventDefault();
    objectResize.cancel();
  objectRotate.cancel(); // Escape during a drag cancels it (docs/85 §4.2)
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
  // no modifier only: Ctrl+Space is Clear Formatting (a keymap row now, claimed
  // by the dispatcher), and a Space over a selection is a replacement, not a
  // toggle.
  if (key === " " && !e.ctrlKey && !e.metaKey && !e.altKey && !hasRange() && selection) {
    if (toggleFormCheckboxAt(selection.focus.node, selection.focus.offset)) {
      e.preventDefault();
      return;
    }
  }

  // Every ⌘/Ctrl chord this handler used to carry — clipboard, select-all,
  // history, B/I/U, the format painter, clear-formatting — is now a row in
  // `keymap.mjs` claimed by the one dispatcher, which runs before this listener
  // and stops the event. What stays here is the keyboard that is NOT a chord:
  // caret movement, Enter, Tab, Backspace, the object grammar and text input.
  if (!selection) return;

  // Navigation uses an explicit macOS/Windows keymap. It runs before the
  // generic modifier guard so the supported Ctrl/Command/Option combinations
  // reach semantic engine moves. Shift extends every navigation intent.
  const navDir = navigationDirection(e, EDITOR_KEYBOARD_PLATFORM);
  // Shift+Arrow EXTENDS a cell selection once one exists — the Docs and Word
  // behaviour — instead of collapsing it into a text range one character long.
  // With no cell selection this is not reached and Shift+Arrow is text as ever.
  if (navDir && e.shiftKey && tableRange.get() && tableRange.extendByStep(navDir)) {
    e.preventDefault();
    syncSelectionToCellRange();
    tableRange.announce();
    return;
  }
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
      const from = selection.focus.node;
      let target = null;
      try {
        // ONLY the engine call is guarded, so a throw from `navToPosition` can
        // never be mistaken for "we are at the boundary" and append a row.
        target = doc.moveTableCell(from, !e.shiftKey);
      } catch {
        // THE TABLE BOUNDARY (`docs/141` TBL-01). `moveTableCell` throws only
        // "no adjacent table cell" here — `inTable` has already answered the
        // other throw — so a forward Tab that fails is a Tab in the LAST cell,
        // and Word and Google Docs both append a row for it. The engine's own
        // caret lands in the new row's first cell (`insert_row`'s
        // `apply_action_caret`), which is exactly where the gesture means to go,
        // so nothing here moves the caret a second time.
        //
        // This used to be an empty `catch {}`: the only table gesture in the
        // product that refused with nothing at all — no row, no message, no
        // console line — which a reader cannot tell from a broken build.
        if (e.shiftKey) setStatus(t("table.atFirstCell"));
        else if (await runEdit(() => doc.insertRow(from, true), { gate: true })) setStatus(t("table.rowAppended"));
      }
      if (target) navToPosition(target, false);
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
    const quoteNode = range ? (anchor.offset <= focus.offset ? anchor.node : focus.node) : focus.node;
    const quoteOffset = range ? Math.min(anchor.offset, focus.offset) : focus.offset;
    const typed = smartQuoteForTyped(key, {
      enabled: smartQuotesEnabled,
      offset: quoteOffset,
      readPrefix: () => doc.copyText(quoteNode, 0, quoteNode, quoteOffset),
    });
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

/** Fit-to-viewport factor for the CARET's page, not page 0. Fitting page 0 on a
 *  document whose later section turns landscape sized the view to the portrait
 *  page and let every landscape page overflow sideways — a "Fit width" that does
 *  not fit. The page index comes from the ruler, so the two cannot disagree
 *  about which page the user is on. The arithmetic is `view_zoom.mjs`. */
function computeFitZoom(mode) {
  if (!doc) return zoomFactor;
  const size = doc.pageSize(Math.max(0, Math.min(rulerView.builtForPage(), pages.length - 1)));
  const factor = fitZoomFactor(mode, size, viewportEl.getBoundingClientRect(), { dpi: BASE_DPI, twipsPerInch: TWIPS_PER_INCH });
  size.free();
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
  reflectZoomMenu(zoomMenu, { mode: zoomMode, factor: zoomFactor });
  viewZoom.reflect();
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
  setZoom(nextZoomStep(zoomFactor, dir, clampZoom));
}

/** Commit the typed zoom value. The parse is `view_zoom.mjs`'s. */
function commitZoomInput({ restoreFocus = false } = {}) {
  const asked = parseZoomInput(zoomEl.value);
  if (asked.mode) setZoomMode(asked.mode, restoreFocus);
  else if (asked.factor) setZoom(asked.factor, restoreFocus);
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
for (const [id, dir] of [["zoomIn", 1], ["zoomOut", -1]]) document.getElementById(id).addEventListener("click", () => stepZoom(dir));

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
// One authority. This used to re-derive framing here, which is exactly why
// `capabilities.mjs` was not the single answer to "what may this page do".
const AUTOSAVE_ALLOWED_HERE = HOST_CAPS.has("autosave");

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
  // `publish`, not `announce`: the draft pill it pairs with is `priority 3` in
  // the footer's informational half, so it is the FIRST thing shed as the window
  // narrows — a broken promise about the user's work must not narrow away with it.
  statusChannel.publish(`Autosave unavailable: ${reason}`, "error");
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
 * The mode ladder is `exportDocumentAs`'s plus `semantic` as the last resort, so
 * a draft and a save can never come from different ladders; there is deliberately
 * no cross-format fallback, because normalized JSON would succeed where DOCX
 * failed and drop every image. `contentId` rides along — see `version_panel.mjs`.
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
      return { bytes, formatId, mode, findings, contentId: doc.contentDigest() };
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
  // The version capture rides HERE and nowhere else, with the SAME bytes: the
  // artifact is already exported and already verified at this point, so a
  // version costs one hash and one IndexedDB transaction rather than a second
  // export of the document (`docs/140` §6.2, §7.5). The decision itself —
  // `VersionCapturePolicy.shouldCapture` — is O(1) in both document size and
  // stored-version count and is all the editing path ever pays.
  versionHistory.capture(reason, snapshot);
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
    // `announce`, not `publish`: the recovery bar is its own on-screen surface at
    // every width, so a toast would be a second copy of what is already in view.
    statusChannel.announce(
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
  await queueDraftWork(() => writeDraft(DRAFT_WRITE_REASONS.RESTORED));
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
document.getElementById("draftRecoveryDismiss")?.addEventListener("click", () => {
  // Dismiss keeps the draft. Nothing in this bar may lose work by being closed.
  draftBarDismissed = true;
  renderDraftRecovery();
  setStatus("Recovered work is still available from File ▸ Recover unsaved work");
});

// Hidden, then pagehide: see `bindDraftFlushOnExit` for why those two.
bindDraftFlushOnExit(draftScheduler);

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

// Who owns the accent: the deployment's `brand.json`, or the visitor. Read once,
// before the first `applySettings()`, because a brand resolved after the first
// paint was never resolved before it.
const ACCENT_PIN = brandAccent(document.documentElement);

let settings = loadSettings();

function loadSettings() {
  return loadPrefObject("opendoc.settings", DEFAULT_SETTINGS);
}

function saveSettings() {
  savePrefObject("opendoc.settings", settings);
}

// ---- Version history (docs/139, docs/140, ADR-038/ADR-040; HF-068 / OO-004) --
//
// Everything about the timeline — the store handle, the panel, the keyboard
// contract, the retention disclosure and the restore state machine — is in
// `version_panel.mjs`. What is left here is the four seams it cannot own:
// parsing bytes, putting a document on the canvas, taking it off again, and
// reporting. Placed AFTER `settings`, because the capture interval is read from
// it at construction.
//
// The read-only enforcement is deliberately NOT a new mechanism. A preview sets
// `readOnlyReason` and `viewing` mode, which is the editor's existing fail-closed
// choke point: `blockMutationInViewing()` refuses every mutation route — typing,
// paste, toolbar, tables, review decisions, the SDK and the host bridge — and
// `editRefusalMessage` already prefers `readOnlyReason` over every other
// sentence, so a preview refuses an edit by SAYING it is a preview.
const versionNamePrompt = createNamePrompt({
  registerModal,
  fallbackFocus: () => pagesEl,
  ids: {
    dialog: "versionNameDialog",
    input: "versionNameInput",
    confirm: "versionNameConfirm",
    cancel: "versionNameCancel",
    close: "versionNameClose",
  },
});

/** The live document and the chrome state a preview borrowed, or null. */
let versionPreviewHome = null;

/**
 * Swaps the canvas onto a version preview, or (with `null`) back to the live
 * document.
 *
 * O(preview document) for the render, once per preview, and it never touches the
 * live document's model. The live wrapper is KEPT, not freed: it is the document
 * the user is editing and the only copy of their unsaved work.
 */
async function showVersionPreview(previewDoc) {
  if (previewDoc) {
    if (!versionPreviewHome) {
      versionPreviewHome = { doc, selection, reviewMode, readOnlyReason };
    }
    doc = previewDoc;
    readOnlyReason = t("versionHistory.preview.readOnly");
  } else {
    if (!versionPreviewHome) return;
    ({ doc, selection, readOnlyReason } = versionPreviewHome);
    const home = versionPreviewHome;
    versionPreviewHome = null;
    // LEAVING keeps the default focus restore, unlike entering: the control the
    // reader pressed — "Back to current" — is part of the preview banner and goes
    // away with it, so declining to move focus would drop the keyboard onto
    // `<body>` and make them Tab in from the top. Measured, both ways.
    setReviewMode(home.reviewMode);
  }
  // A different document, so every answer cached about the last one is wrong: the
  // remembered object presence, any table selection, the review card geometry,
  // and the background measure ticker (which captures `doc`, so a late tick from
  // the old one is already a no-op).
  objectPresence.forget();
  tableRange.clear();
  reviewLayout = [];
  reviewCardCache.clear();
  if (previewDoc) {
    const start = doc.firstPosition();
    selection = {
      anchor: { node: start.node, offset: start.offset },
      focus: { node: start.node, offset: start.offset },
    };
    start.free();
    setReviewMode("viewing", { restoreFocus: false });
  }
  armBackgroundMeasure();
  await renderAll();
  drawSelection();
  updateToolbar();
}

/** Set while a restore is activating, so the open path does not ALSO record an
 *  import baseline on top of the restore version that just committed — one user
 *  action, one point in the timeline. */
let activatingRestore = false;

const versionHistory = createVersionHistory({
  parse: (bytes) => open(bytes),
  showPreview: (previewDoc) => showVersionPreview(previewDoc),
  // Through the ORDINARY open path, so a restored document is indistinguishable
  // from an opened one: same admission limits, dirty tracking and loss reporting.
  activateRestored: async (bytes, name) => {
    activatingRestore = true;
    try {
      await openBytes(bytes, name);
    } finally {
      activatingRestore = false;
    }
    // Edited/Unsaved until the host saves it (`docs/139` §8.5 step 7):
    // `openBytes` re-baselines dirty tracking onto the bytes it opened, and for a
    // restore that baseline is a lie — the file on disk is still the old one.
    // Same flag as a recovered draft, because it is the same fact.
    restoredFromDraft = true;
    setDocumentState("edited");
  },
  snapshot: () => (doc ? takeDraftSnapshot() : null),
  documentInfo: () => ({
    name: currentName,
    docKey: draftDocKey,
    revision: revisionUnreadable ? null : currentRevision,
    // Only with a document, and that guard is load-bearing: `applySettings` asks
    // this for `unavailableReason` at module init, BEFORE `boot()` has awaited
    // `init()`, and `engineVersion()` on an uninstantiated wasm module throws
    // `__wbindgen_add_to_stack_pointer` of undefined and kills boot outright.
    engine: doc ? engineVersion() : "",
    actor: settings.authorName.trim(),
    hasDocument: Boolean(doc),
  }),
  settings: () => settings,
  hostAllows: () => AUTOSAVE_ALLOWED_HERE,
  capabilities: () => HOST_CAPS,
  publish: (text, kind) => statusChannel.publish(text, kind),
  confirm: (options) => confirmModal(options),
  promptName: (current) => versionNamePrompt.prompt(current),
  onOpenChange: (isOpen) => {
    // The timeline and the comments sidebar are both on the right, so they are
    // mutually exclusive for the reason Outline and Pages are: the canvas is
    // never squeezed from both sides at once.
    if (isOpen && !reviewSidebar.hidden) toggleReview(false);
  },
  showChanges: (bytes, name) => void comparePanel.compareWith(bytes, name),
});

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
  applyAppearance({
    root: document.documentElement,
    theme: settings.theme,
    accent: settings.accent,
    pin: ACCENT_PIN,
  });
  // The gallery is built per document, but its legibility decisions were made
  // against whatever palette was live at the time.
  refreshStylePreviews();

  reflectAppearance({
    themeGroup,
    swatches: accentSwatches,
    custom: accentCustom,
    settings,
    pin: ACCENT_PIN,
    reason: t("branding.themeSetByHost"),
  });
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
  // Version history rides on the autosave switch (ADR-038 / `docs/139` §18
  // question 2: one switch must not promise what the other has stopped doing),
  // so both switches and the retention numbers change what this entry says.
  versionHistory.reflect();
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
  // Version history rides this switch (ADR-038). Stored VERSIONS are left alone:
  // `clear()` above empties the draft slots only, and deleting a timeline as a
  // side effect of a switch is a destruction nobody asked for.
  versionHistory.reflect();
});
draftsClearBtn?.addEventListener("click", () => void clearAllDrafts());
spellCheckToggle?.addEventListener("change", () => setSpellCheckEnabled(spellCheckToggle.checked));
grammarCheckToggle?.addEventListener("change", () => setGrammarCheckEnabled(grammarCheckToggle.checked));

const themeGroup = bindRadioGroup(themeSeg, {
  attr: "data-theme",
  onSelect: (theme) => {
    settings.theme = theme;
    saveSettings();
    applySettings();
  },
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
  initialFocus: () => themeGroup.selected(),
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
  // The Settings DIALOG and the Settings PANE are the same element, and while
  // the File page is open it is parented INSIDE the page — opening the dialog
  // would raise a half-dialog out of a pane. One Settings, so the gear goes to
  // wherever it currently lives.
  if (showSettingsPane()) {
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
// Locale (docs/124). After `applySettings`, so the picker sees the settings
// that loaded; awaited nowhere, so the editor draws in English and relabels
// when the catalogue lands. The counts are the one surface markup cannot
// carry, so re-rendering them is what this hands over.
setPaneRenderer(() => renderFilePage());
void startLocalisation({
  select: languageSelect,
  // The footer control is a POPOVER, on the same manager the zoom presets use,
  // so the two footer controls dismiss, restore focus and stay mutually
  // exclusive by one implementation rather than two. The module owns the rest
  // of that control's DOM already, so it looks the elements up itself.
  registerPopover,
  settings,
  saveSettings,
  onLocalised: () => {
    // Every relabel re-introduces ⌘ from the catalogue (`105` UX-009, #599).
    localizeShortcutGlyphs(document.body, EDITOR_KEYBOARD_PLATFORM);
    reflowView.setEnabled(); // the width button's label is the STEP's, not markup's
    if (doc) updateStats();
    // The timeline's rows, day headings and disclosure are all script-built from
    // locale-shaped values, and the View entry's disabled reason is a sentence —
    // so this panel is one of the surfaces a relabel has to reach.
    versionHistory.reflect();
  },
});

// ---- Document properties (docProps/core.xml — title, author, subject, …) ----
// The whole dialog is `document_metadata.mjs`: the six editable fields, the
// seventeen read-only `docProps` cells beside them, and the one transaction Apply
// runs. Splitting one dialog across two files would have been worse than moving
// it whole, and the File page shows the same element as a pane.
const propertiesUi = createPropertiesDialog({ registerModal, getDoc: () => doc, runEdit });

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

// ---- Page setup, line numbers, watermark -----------------------------------
// Word's Layout ▸ Page Setup group, in `page_setup.mjs`: the geometry dialog, the
// Line Numbers popover and the Watermark dialog, which share one answer to
// "which section is the caret in". None of the three is decided here any more.
const pageSetup = createPageSetup({
  getDoc: () => doc,
  // The same inventory the Home band's font menu offers, so the watermark's Font
  // list cannot name a face no other control here can.
  fontInventory,
  selectionNode: () => selection?.focus?.node ?? "",
  selectionEndpoints: selEndpoints,
  runEdit,
  registerModal,
  registerPopover,
  // Word's *Measurement units* setting governs exactly this dialog, so the
  // preference arrives as a dependency rather than being read from a global.
  measure: measurement,
});

// ---- Header and footer settings --------------------------------------------
// The band distances, the two running-content variants and the section's page
// numbering, in `header_footer_settings.mjs`: ONLYOFFICE's own grouping of the
// four (`view/HeaderFooterTab.js` L63-87). The two variant toggles moved there
// from here, so the checkbox faces in the dialog and the button faces on the
// Insert band are one implementation rather than two that can disagree.
const headerFooterSettings = createHeaderFooterSettings({
  getDoc: () => doc,
  selectionNode: () => selection?.focus?.node ?? "",
  runningPage: runningEditPageOrView,
  runEdit,
  applyEditResult,
  blockedFromMutating: blockedFromPageSetup,
  setStatus,
  registerModal,
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
  // Which roster the bar draws. A phone gets Docs' row over two named sheets
  // (`PHONE_TOOLBAR`) instead of the desktop's thirteen groups over a fold whose
  // membership changes with the window width — see `compact_toolbar.mjs`.
  // Resolved at render time, not captured, so crossing the rung switches it.
  isPhone: () => phoneChrome.isPhone(),
});

/** Switches chrome. Mutually exclusive by construction — `body` carries exactly
 *  one mode class — and the caret is never disturbed, because neither chrome
 *  owns the editing surface. */
function setChromeMode(mode, { persist = true, announce = true } = {}) {
  chromeMode = mode === "compact" ? "compact" : "ribbon";
  // A phone has room for one navigation axis and the ribbon is not it (docs/148
  // §5). The preference is untouched, so growing back past the rung restores it.
  const compact = chromeMode === "compact" || phoneChrome.isPhone();
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
  chromeModeGroup?.reflect(chromeMode);
  // With no document open the registry holds only `noDoc` commands, so the bar
  // would render nearly empty; it is rebuilt on `doc-loaded`. Rendering here
  // anyway keeps the adopted controls in place so the bar is never a bare strip.
  if (compact) compactToolbarUi.render();
  else compactToolbarUi.release();
  if (persist) writePref(CHROME_MODE_PREF, chromeMode);
  if (announce) setStatus(compact ? "Compact toolbar" : "Ribbon toolbar", "", { timeout: 1800 });
}

const chromeModeGroup = bindRadioGroup(document.querySelector(".chrome-mode"), {
  attr: "data-chrome-mode",
  onSelect: (mode) => setChromeMode(mode),
});
setChromeMode(chromeMode, { persist: false });
// Silently: nobody asked to cross the rung, so a toast on every rotation would
// be a notification about the window rather than about the document.
phoneChrome.onPhoneChange(() => setChromeMode(chromeMode, { persist: false, announce: false }));

// ---- The host contract: one session, two transports (`docs/126` phase 2) ----
// `window.opendoc` is the in-process transport, and `attachHostBridge` puts THAT
// SAME OBJECT behind `postMessage`, so the two can never answer differently.
// Commands, events, refusals and gating all come from `host_contract.mjs`. The
// registry handed over is the PALETTE's, so the API reaches what a person can
// reach and no more; the gate runs before dispatch, because the API is a fourth
// enforcement layer and must not be the unlocked one.
hostSession = createHostSession({
  capabilities: HOST_CAPS,
  registry: () => editorCommands({ surface: "palette" }),
  withheldMessage: () => t("capability.notGranted"),
  revision: () => (revisionUnreadable ? null : currentRevision),
  dirty: () => documentIsDirty(),
  selection: () => (selection ? { anchor: { ...selection.anchor }, focus: { ...selection.focus }, hasRange: hasRange() } : null),
  document: () => ({ name: currentName, format: currentSourceFormat }),
});
window.opendoc = hostSession;
attachHostBridge({ session: hostSession, view: window });
