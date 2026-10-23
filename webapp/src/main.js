// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads `casual-doc-wasm`, opens a local `.docx`, and paints pages on canvas: the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

import { open, beginVersionDiff, defaultDiffSlice } from "../pkg/casual_doc_wasm.js";
import { toggleTextCheckboxAt } from "./text_checkbox.mjs";
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
  fetchSharedFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { embedMarker, runsToHtml } from "./clipboard.mjs";
import { escapeHtml } from "./text_rules.mjs";
import { bindBreaksMenu, breakSurfaceRows } from "./break_commands.mjs";
import { bindComparePanel, buildRedline, comparableBytes } from "./compare_documents.mjs";
import { EXPORT_COMMANDS } from "./export_commands.mjs";
import { editRefusalMessage, mutationBlockedMessage, readRevision } from "./edit_errors.mjs";
import { renderAccessibilityMirror } from "./a11y_mirror.mjs";
import { createPagesPanel } from "./pages_panel.mjs";
import { createSpacingMenu } from "./spacing_menu.mjs";
import { createTocNavigator } from "./toc_navigation.mjs";
import { objectMenuRows } from "./reference_commands.mjs";
import {
  reflectReviewSurface,
  ribbonSurfaceEnabled as surfaceEnabled,
  ribbonSurfaceReason as surfaceReason,
} from "./ribbon_surface.mjs";
import { capabilityRefusal, createRefusedDrag, readCapabilityReasons } from "./object_refusal.mjs";
import { createKeyboardObjectMove, createObjectTextDrag } from "./object_text_move.mjs";
import { edgeScrollStep } from "./edge_scroll.mjs";
import { canChangePicture } from "./picture_replace.mjs";
import { createHeaderFooterSettings } from "./header_footer_settings.mjs";
import { createPageSetup } from "./page_setup.mjs";
import { createStyleAdvanced } from "./style_advanced.mjs";
import { createListAdvanced } from "./list_advanced.mjs";
import { wireListGallery } from "./list_gallery_wiring.mjs";
import { createProofingChrome } from "./proofing_chrome.mjs";
import { OBJECT_LABELS, escapeClimbsToGroup, groupClickAction, nextObjectIndex, traversalAnnouncement, traversalRoot } from "./object_traversal.mjs";
import { RECOMMENDED_STYLES, caretContexts, nextFontSizeStep, offeredStyleNames, previewPx, styleMenuGroups, styleSlug } from "./style_picker.mjs";
import { styleDisplayName } from "./style_names.mjs";
import { applyPreviewInk, applyStylePreview } from "./style_preview.mjs";
import { createCompatibilityFindings } from "./compat_findings.mjs";
import { INSERT_REASON_KEYS, listNumberingStates, reflectEnablement } from "./control_reasons.mjs";
import { attachHostBridge } from "./host_bridge.mjs";
import { createHostSession } from "./host_session.mjs";
import { createCompactToolbar } from "./compact_toolbar.mjs";
import { createConfirmDialog } from "./confirm_dialog.mjs";
import { createPropertiesDialog } from "./document_metadata.mjs";
import { createNamePrompt } from "./name_dialog.mjs";
import {
  PAGE_WINDOW_OVERSCAN_PX,
  buildPageBand,
  docToScroll,
  pageBandPitch,
  pageClientRect,
  pageRangeAt,
  scrollToDoc,
} from "./page_scroll.mjs";
import {
  formatInfo,
} from "./format_io.mjs";
import { formatShortcut, keyboardPlatform } from "./keyboard.mjs";
import { chordTitle } from "./keymap.mjs";
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
import { POSITION_PRESETS, ROTATE_CHOICES, Z_ORDER_CHOICES, positionAvailability, positionPayload, readGroupability, readPosition, readTransform, rotationPlan, runWrapPlan, wrapAvailability, wrapPlan } from "./object_arrange.mjs";
import { renderArrangeMenu, renderPositionGallery, renderRotateMenu } from "./object_arrange_chrome.mjs";
import { SHAPE_PRESETS } from "./shape_catalogue.mjs";
import { galleryRowStarts, nextGalleryIndex, renderShapeGallery } from "./shape_gallery.mjs";
import { createShapeDrawMode } from "./shape_draw_mode.mjs";
import { reflectObjectSelection, reflectShapeFormat } from "./object_selection_state.mjs";
import { pageSnapTargets, snapBox } from "./object_snap.mjs";
import { activeLocale, t } from "./i18n.mjs";
import { authoredTitle, paintDocumentState } from "./localize.mjs";
import { countLabels, pageIndicator, readerPosition } from "./status_counts.mjs";
import { localizeShortcutGlyphs, localizeShortcutText } from "./shortcut_labels.mjs";
import { installRibbonTooltips } from "./ribbon_tooltip.mjs";
import { createFormattingMarks } from "./formatting_marks.mjs";
import { createMeasurementUnits } from "./measurement_units.mjs";
import { createDocumentProtection } from "./document_protection.mjs";
import {
  openWordStore,
} from "./drafts.mjs";
import { rovingIndex, tabStopIndex } from "./ribbon_nav.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { closeAllPopovers, onButton, openPopover } from "./popover_manager.mjs";
import { closePopover, reflectOpenPopovers, registerPopover } from "./popover_manager.mjs";
import { HIGHLIGHT_COLORS, HIGHLIGHT_LABEL, TEXT_STANDARD_COLORS, highlightHex } from "./palettes.mjs";
import { createViewZoom, nextZoomStep } from "./view_zoom.mjs";
import { createPhoneChrome } from "./phone_chrome.mjs";
import { createTouchSelection, pointerDragSelects } from "./touch_selection.mjs";
import { DEFAULT_SETTINGS } from "./settings_defaults.mjs";
import { editingModeFor, hostCapabilities, hostChrome, hostConfig, reflectReviewModeAccess } from "./capabilities.mjs";
import { createAccessChrome } from "./access_chrome.mjs";
import { tablePropertiesPatch as tablePatch } from "./table_properties_patch.mjs";
import { insertChartAtCaret } from "./chart_insert.mjs";
// Own line (anti-conflict): the chart's tab, panel, data dialog and commands.
import { createChartSurface } from "./chart_surface.mjs";
import { createCollaborationShell } from "./collab_shell.mjs";
import { groupsToOverflow } from "./ribbon_overflow.mjs";
import { smallestContaining } from "./review_anchor.mjs";
import { createReviewTracking } from "./review_tracking.mjs";
import { scrollLeftFor, scrollTargetFor } from "./scroll_into_view.mjs";
import { sessionAccess } from "./session_access.mjs"; // the ROOM's grant, a different authority from the container's
import { createReviewCommentActions } from "./review_comment_actions.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { orderedSelectionEnds, sameModelPosition } from "./caret_navigation.mjs";
import {
  reviewCardSignature,
  reviewCommentIsReplyTo,
  reviewStackHeight,
  stackReviewCards,
} from "./review_layout.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { attachComposerKeys, autoGrowTextarea, createCommentAffordance, createReviewBanners, reviewCardButton, reviewIconButton } from "./review_chrome.mjs";
import { commentEditButton, commentEditor, commentQuoteElement, createCommentEditState, createCommentQuotes } from "./review_comment_card.mjs";
import { makeMenuHeading, makeSwatchCell, makeSwatchGrid, makeUnderlineStyleOption, renderColorMenu as renderColorMenuChrome } from "./picker_chrome.mjs";
import {
  formatReviewDate,
  reviewAuthorColor,
  reviewAuthorKey,
  reviewCardAriaLabel,
  reviewChangeTypeLabel,
  reviewCommentTooltip,
  reviewFormattingDescription,
  reviewRevisionTooltip,
} from "./review_labels.mjs";
import {
  recaseRichRuns,
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
} from "./command_taxonomy.mjs";
import { fileMenuSections } from "./command_taxonomy.mjs";
import {
  createMenuBar,
  renderCommandRows,
  renderExportPane,
  renderFilePageInfo,
} from "./command_menu.mjs";
import {
  fileCategoryRowFor,
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
  DOCUMENT_TEMPLATES,
  templateBytes,
  templateName,
  templateThumbnail,
} from "./blank_document.mjs";
import { followExternalTarget } from "./link_targets.mjs";
import { createPointerHover } from "./pointer_hover.mjs";
import { createTableChrome } from "./table_chrome.mjs";
import { createTableGutter } from "./table_gutter.mjs";
import { createTableRange } from "./table_range.mjs";
import { bindCellFormatMenu, bindSplitCellDialog } from "./table_cell_chrome.mjs";
import { createRuler } from "./ruler.mjs";
import { createFontAdvanced } from "./font_advanced.mjs";
import { createParagraphAdvanced } from "./paragraph_advanced.mjs";
import { createTabStopsDialog } from "./tab_stops_dialog.mjs";
import { createObjectPresence } from "./object_presence.mjs";
import { stampRibbonFaces } from "./ribbon_faces.mjs";
import { announceTableEdit, bindTableBand, tableBandStates, tableContextLabel } from "./table_band.mjs";
import { createTableGridPicker } from "./table_grid_picker.mjs";
import { createRegionCycle } from "./region_focus.mjs";
import { loadPrefObject, readPref, savePrefObject, writePref } from "./prefs.mjs";
import { BRAND } from "./brand.mjs";
import { applyRegions } from "./chrome_regions.mjs";
import { installViewportFocusGuard } from "./viewport_focus.mjs";
import { createFind } from "./editor_find.mjs";
import { createDrafts } from "./editor_drafts.mjs";
import { createVersions } from "./editor_versions.mjs";
import { createSettings } from "./editor_settings.mjs";
import { createPalette } from "./editor_palette.mjs";
import { createDialogs } from "./editor_dialogs.mjs";
import { createLinkDialog } from "./editor_link_dialog.mjs";
import { createPaletteUi } from "./editor_palette_ui.mjs";
import { createFileOpen } from "./editor_file_open.mjs";
import { createDirtyTracking } from "./editor_dirty.mjs";
import { createEditPipeline } from "./editor_edit_pipeline.mjs";
import { createCaretNav } from "./editor_caret_nav.mjs";
import { createReviewDecisions } from "./editor_review_decisions.mjs";
import { createSelectionToolbar } from "./editor_selection_toolbar.mjs";
import { createSave } from "./editor_save.mjs";
import { createSelectionCommands } from "./editor_selection_commands.mjs";
import { createClipboard } from "./editor_clipboard.mjs";
import { createZoom } from "./editor_zoom.mjs";
import { createTextInput } from "./editor_text_input.mjs";
import { createKeyboard } from "./editor_keyboard.mjs";
import { createFilePick } from "./editor_file_pick.mjs";
import { createContextMenu } from "./editor_context_menu.mjs";
import { createMenuRender } from "./editor_menu_render.mjs";
import { createFormatting } from "./editor_formatting.mjs";
import { createFormatPainter } from "./editor_format_painter.mjs";
import { createOutline } from "./editor_outline.mjs";
// editor-context:begin
/** The editor context (docs/177): the one object every extracted module is handed.
 *  Each entry is a live view of a binding this file still owns, read lazily, so a
 *  module sees the value at call time exactly as the code did before it moved. */
const ed = {
  get AUTOSAVE_ALLOWED_HERE() { return AUTOSAVE_ALLOWED_HERE; },
  get BASE_DPI() { return BASE_DPI; },
  get EDITOR_KEYBOARD_PLATFORM() { return EDITOR_KEYBOARD_PLATFORM; },
  get HOST_CAPS() { return HOST_CAPS; },
  get HOST_CHROME() { return HOST_CHROME; },
  get HOST_MODE() { return HOST_MODE; },
  get INSERT_SURFACE() { return INSERT_SURFACE; },
  get LAYOUT_SURFACE() { return LAYOUT_SURFACE; },
  get NO_TABLE_STYLE() { return NO_TABLE_STYLE; },
  get OBJECT_CAPABILITY_KEYS() { return OBJECT_CAPABILITY_KEYS; },
  get REFERENCE_SURFACE() { return REFERENCE_SURFACE; },
  get REVIEW_SURFACE() { return REVIEW_SURFACE; },
  get REVIEW_TEXTLESS_KINDS() { return REVIEW_TEXTLESS_KINDS; },
  get SESSION() { return SESSION; },
  get SETTINGS_SURFACE() { return SETTINGS_SURFACE; },
  get SHAPE_MENU_COLORS() { return SHAPE_MENU_COLORS; },
  get TYPING_PAUSE_MS() { return TYPING_PAUSE_MS; },
  get UNDERLINE_STYLE_LABELS() { return UNDERLINE_STYLE_LABELS; },
  get ZOOM_MAX() { return ZOOM_MAX; },
  get ZOOM_MIN() { return ZOOM_MIN; },
  get accessChrome() { return accessChrome; },
  get activatingRestore() { return activatingRestore; }, set activatingRestore(v) { activatingRestore = v; },
  get activeLink() { return activeLink; },
  get activeReviewCommentId() { return activeReviewCommentId; }, set activeReviewCommentId(v) { activeReviewCommentId = v; },
  get activeReviewItemId() { return activeReviewItemId; }, set activeReviewItemId(v) { activeReviewItemId = v; },
  get addWordToDictionary() { return addWordToDictionary; },
  get adjustIndentCommand() { return adjustIndentCommand; },
  get adoptDraftDocument() { return adoptDraftDocument; },
  get alignBtns() { return alignBtns; },
  get anchorAt() { return anchorAt; },
  get announceReview() { return announceReview; },
  get applyActiveAuthorToDocument() { return applyActiveAuthorToDocument; },
  get applyChangeCase() { return applyChangeCase; },
  get applyEditResult() { return applyEditResult; },
  get applyFontFamily() { return applyFontFamily; },
  get applyFontSize() { return applyFontSize; },
  get applyListFormatCommand() { return applyListFormatCommand; },
  get applyShapeFill() { return applyShapeFill; },
  get applyShapeOutline() { return applyShapeOutline; },
  get applyUnderlineStyle() { return applyUnderlineStyle; },
  get armBackgroundMeasure() { return armBackgroundMeasure; },
  get armFormatPainter() { return armFormatPainter; },
  get armOrApplyRun() { return armOrApplyRun; },
  get arrangeState() { return arrangeState; },
  get autoCorrect() { return autoCorrect; },
  get autosaveToggle() { return autosaveToggle; },
  get backgroundMeasure() { return backgroundMeasure; }, set backgroundMeasure(v) { backgroundMeasure = v; },
  get blockMutationInViewing() { return blockMutationInViewing; },
  get blockUntrackedInSuggesting() { return blockUntrackedInSuggesting; },
  get bookmarkManager() { return bookmarkManager; },
  get breakTypingSession() { return breakTypingSession; },
  get buildAccessibilityTree() { return buildAccessibilityTree; },
  get buildCommands() { return buildCommands; },
  get buildContextCommands() { return buildContextCommands; },
  get buildOutline() { return buildOutline; },
  get bulletGalleryMenu() { return bulletGalleryMenu; },
  get bulletListBtn() { return bulletListBtn; },
  get cancelCrop() { return cancelCrop; },
  get cancelObjectMove() { return cancelObjectMove; },
  get caretIsImplicit() { return caretIsImplicit; },
  get chartSurface() { return chartSurface; },
  get checkListBtn() { return checkListBtn; },
  get chromeShows() { return chromeShows; },
  get clearAllDrafts() { return clearAllDrafts; },
  get clearFindParagraphCache() { return clearFindParagraphCache; },
  get clearFormattingBtn() { return clearFormattingBtn; },
  get clearObjectStatus() { return clearObjectStatus; },
  get clientPointEvent() { return clientPointEvent; },
  get climbOutOfGroup() { return climbOutOfGroup; },
  get closeAppMenu() { return closeAppMenu; },
  get closeFilePage() { return closeFilePage; },
  get cmdInput() { return cmdInput; },
  get cmdList() { return cmdList; },
  get cmdPalette() { return cmdPalette; },
  get collab() { return collab; },
  get collabFeatureCommands() { return collabFeatureCommands; },
  get commentEdit() { return commentEdit; },
  get commentQuotes() { return commentQuotes; },
  get commitComposedText() { return commitComposedText; },
  get compactToolbarUi() { return compactToolbarUi; },
  get comparePanel() { return comparePanel; },
  get compatFindings() { return compatFindings; },
  get composingText() { return composingText; }, set composingText(v) { composingText = v; },
  get computeFitZoom() { return computeFitZoom; },
  get confirmDiscardIfEdited() { return confirmDiscardIfEdited; },
  get confirmModal() { return confirmModal; },
  get containerCanEdit() { return containerCanEdit; },
  get contextAt() { return contextAt; },
  get contextMenuReturnFocus() { return contextMenuReturnFocus; }, set contextMenuReturnFocus(v) { contextMenuReturnFocus = v; },
  get continueListBtn() { return continueListBtn; },
  get copySelection() { return copySelection; },
  get createStyleFromSelection() { return createStyleFromSelection; },
  get cropKeyIo() { return cropKeyIo; },
  get currentFontFamily() { return currentFontFamily; }, set currentFontFamily(v) { currentFontFamily = v; },
  get currentName() { return currentName; }, set currentName(v) { currentName = v; },
  get currentParagraphStyleName() { return currentParagraphStyleName; },
  get currentRevision() { return currentRevision; }, set currentRevision(v) { currentRevision = v; },
  get currentSourceFormat() { return currentSourceFormat; }, set currentSourceFormat(v) { currentSourceFormat = v; },
  get currentUnderlineColor() { return currentUnderlineColor; }, set currentUnderlineColor(v) { currentUnderlineColor = v; },
  get currentUnderlineMixed() { return currentUnderlineMixed; }, set currentUnderlineMixed(v) { currentUnderlineMixed = v; },
  get currentUnderlineStyle() { return currentUnderlineStyle; }, set currentUnderlineStyle(v) { currentUnderlineStyle = v; },
  get cut() { return cut; },
  get cycleReviewMode() { return cycleReviewMode; },
  get decideAllReviewChanges() { return decideAllReviewChanges; },
  get decideContextRevision() { return decideContextRevision; },
  get decideReviewAndAdvance() { return decideReviewAndAdvance; },
  get decideReviewAtCaret() { return decideReviewAtCaret; },
  get deleteSelectedObject() { return deleteSelectedObject; },
  get discardOwnDraft() { return discardOwnDraft; },
  get doc() { return doc; }, set doc(v) { doc = v; },
  get docTitleEl() { return docTitleEl; },
  get documentChrome() { return documentChrome; },
  get documentEpoch() { return documentEpoch; }, set documentEpoch(v) { documentEpoch = v; },
  get documentHasObjects() { return documentHasObjects; },
  get documentHasTrackedChanges() { return documentHasTrackedChanges; },
  get documentIsDirty() { return documentIsDirty; },
  get documentProtection() { return documentProtection; },
  get draftDocKey() { return draftDocKey; }, set draftDocKey(v) { draftDocKey = v; },
  get draftOffers() { return draftOffers; }, set draftOffers(v) { draftOffers = v; },
  get draftsClearBtn() { return draftsClearBtn; },
  get drawSelection() { return drawSelection; },
  get dropCapDialog() { return dropCapDialog; },
  get editRunningContent() { return editRunningContent; },
  get editSelectionLink() { return editSelectionLink; },
  get editorCommands() { return editorCommands; },
  get editorContextMenu() { return editorContextMenu; },
  get editorSurfaceHasFocus() { return editorSurfaceHasFocus; },
  get editorTextInputEl() { return editorTextInputEl; },
  get editorTextInputEvent() { return editorTextInputEvent; },
  get ensureGlyphCoverage() { return ensureGlyphCoverage; },
  get enterCropMode() { return enterCropMode; },
  get enterObjectEditMode() { return enterObjectEditMode; },
  get eventTargetsEditor() { return eventTargetsEditor; },
  get exitRunningEdit() { return exitRunningEdit; },
  get exportDocumentAs() { return exportDocumentAs; },
  get fileEl() { return fileEl; },
  get fmtButtons() { return fmtButtons; },
  get focusEditorSurface() { return focusEditorSurface; },
  get focusReviewComment() { return focusReviewComment; },
  get foldView() { return foldView; },
  get followTocEntry() { return followTocEntry; },
  get fontAdvanced() { return fontAdvanced; },
  get fontFamilyBtn() { return fontFamilyBtn; },
  get fontFamilyLabel() { return fontFamilyLabel; },
  get fontInventory() { return fontInventory; },
  get fontSizeSel() { return fontSizeSel; },
  get fontState() { return fontState; }, set fontState(v) { fontState = v; },
  get formatPainter() { return formatPainter; }, set formatPainter(v) { formatPainter = v; },
  get formatPainterBtn() { return formatPainterBtn; },
  get formattingMarks() { return formattingMarks; },
  get grammarCheckToggle() { return grammarCheckToggle; },
  get handOverDraftBeforeOpen() { return handOverDraftBeforeOpen; },
  get handleFile() { return handleFile; },
  get hasRange() { return hasRange; },
  get headerFooterSettings() { return headerFooterSettings; },
  get hexToRgb() { return hexToRgb; },
  get hideImePreedit() { return hideImePreedit; },
  get hideLinkChip() { return hideLinkChip; },
  get hidePasteOptions() { return hidePasteOptions; },
  get highlightPopover() { return highlightPopover; },
  get hostSession() { return hostSession; },
  get implicitCaretAt() { return implicitCaretAt; }, set implicitCaretAt(v) { implicitCaretAt = v; },
  get indentDecBtn() { return indentDecBtn; },
  get indentIncBtn() { return indentIncBtn; },
  get insertChartWithData() { return insertChartWithData; },
  get insertCommandEnabled() { return insertCommandEnabled; },
  get insertFieldAtCaret() { return insertFieldAtCaret; },
  get insertImageFromBlob() { return insertImageFromBlob; },
  get insertImageFromFile() { return insertImageFromFile; },
  get insertLineBreakAtSelection() { return insertLineBreakAtSelection; },
  get insertNote() { return insertNote; },
  get insertTableBtn() { return insertTableBtn; },
  get insertTextBoxObject() { return insertTextBoxObject; },
  get isInteractiveChromeTarget() { return isInteractiveChromeTarget; },
  get keyboardLevelIndex() { return keyboardLevelIndex; }, set keyboardLevelIndex(v) { keyboardLevelIndex = v; },
  get keyboardObjectMove() { return keyboardObjectMove; },
  get lastTypingAt() { return lastTypingAt; }, set lastTypingAt(v) { lastTypingAt = v; },
  get leaveSharedDocument() { return leaveSharedDocument; },
  get linkAt() { return linkAt; },
  get listAdvanced() { return listAdvanced; },
  get loadSharedStartup() { return loadSharedStartup; },
  get markDocumentSaved() { return markDocumentSaved; },
  get measurement() { return measurement; },
  get menuLevels() { return menuLevels; }, set menuLevels(v) { menuLevels = v; },
  get navByViewport() { return navByViewport; },
  get navCaret() { return navCaret; },
  get navToPosition() { return navToPosition; },
  get navigateReview() { return navigateReview; },
  get newBlankDocument() { return newBlankDocument; },
  get noteDocumentEdited() { return noteDocumentEdited; },
  get noteDraftDirty() { return noteDraftDirty; },
  get nudgeSelectedObject() { return nudgeSelectedObject; },
  get numberGalleryMenu() { return numberGalleryMenu; },
  get numberedListBtn() { return numberedListBtn; },
  get objectArrange() { return objectArrange; },
  get objectCapabilities() { return objectCapabilities; },
  get objectContextAtEvent() { return objectContextAtEvent; },
  get objectContextMenuHost() { return objectContextMenuHost; },
  get objectCropSession() { return objectCropSession; }, set objectCropSession(v) { objectCropSession = v; },
  get objectEditBlocked() { return objectEditBlocked; },
  get objectMoveDrag() { return objectMoveDrag; },
  get objectPresence() { return objectPresence; },
  get objectReference() { return objectReference; },
  get objectResize() { return objectResize; },
  get objectRotate() { return objectRotate; },
  get objectSelection() { return objectSelection; }, set objectSelection(v) { objectSelection = v; },
  get objectTextDrag() { return objectTextDrag; },
  get openAltTextDialog() { return openAltTextDialog; },
  get openBookmarkManager() { return openBookmarkManager; },
  get openBtn() { return openBtn; },
  get openBytes() { return openBytes; },
  get openCmd() { return openCmd; },
  get openEmojiPicker() { return openEmojiPicker; },
  get openFieldDialog() { return openFieldDialog; },
  get openFind() { return openFind; },
  get openLinkDialog() { return openLinkDialog; },
  get openReviewComposer() { return openReviewComposer; },
  get openShapeGallery() { return openShapeGallery; },
  get openSymbolPicker() { return openSymbolPicker; },
  get outlineBody() { return outlineBody; },
  get outlineClose() { return outlineClose; },
  get outlinePanel() { return outlinePanel; },
  get pageFromClientPoint() { return pageFromClientPoint; },
  get pageFromEvent() { return pageFromEvent; },
  get pageSetup() { return pageSetup; },
  get pages() { return pages; },
  get pagesEl() { return pagesEl; },
  get pagesPanel() { return pagesPanel; },
  get pagesPanelView() { return pagesPanelView; },
  get paintOverlayLayer() { return paintOverlayLayer; },
  get paintPagesInView() { return paintPagesInView; },
  get paragraphAdvanced() { return paragraphAdvanced; },
  get paste() { return paste; },
  get pasteAsText() { return pasteAsText; },
  get pasteOptionsShowing() { return pasteOptionsShowing; },
  get pasteRichRunsJson() { return pasteRichRunsJson; },
  get pasteText() { return pasteText; },
  get pendingFormat() { return pendingFormat; }, set pendingFormat(v) { pendingFormat = v; },
  get pictureReplace() { return pictureReplace; },
  get place() { return place; },
  get plainTableInfo() { return plainTableInfo; },
  get pointInsideObject() { return pointInsideObject; },
  get pointToTwip() { return pointToTwip; },
  get pointerHover() { return pointerHover; },
  get populateStyles() { return populateStyles; },
  get populateTableStyles() { return populateTableStyles; },
  get positionEditorTextInput() { return positionEditorTextInput; },
  get proofing() { return proofing; },
  get propertiesUi() { return propertiesUi; },
  get provisionFonts() { return provisionFonts; },
  get queueDraftWork() { return queueDraftWork; },
  get railOutline() { return railOutline; },
  get railPages() { return railPages; },
  get railReview() { return railReview; },
  get readOnlyReason() { return readOnlyReason; }, set readOnlyReason(v) { readOnlyReason = v; },
  get readReviewData() { return readReviewData; },
  get reanchorPageOverlays() { return reanchorPageOverlays; },
  get redlineOf() { return redlineOf; },
  get referenceCommands() { return referenceCommands; },
  get referenceObjectMenuRows() { return referenceObjectMenuRows; },
  get reflectShowingChangesState() { return reflectShowingChangesState; },
  get reflowView() { return reflowView; },
  get refreshDraftOffers() { return refreshDraftOffers; },
  get regionCycle() { return regionCycle; },
  get remotePresence() { return remotePresence; },
  get renderAll() { return renderAll; },
  get renderFilePage() { return renderFilePage; },
  get renderStylesGallery() { return renderStylesGallery; },
  get repaintPage() { return repaintPage; },
  get replaceMisspelling() { return replaceMisspelling; },
  get resetDirtyTracking() { return resetDirtyTracking; },
  get restartListBtn() { return restartListBtn; },
  get restoredFromDraft() { return restoredFromDraft; }, set restoredFromDraft(v) { restoredFromDraft = v; },
  get reviewAcceptAll() { return reviewAcceptAll; },
  get reviewBtn() { return reviewBtn; },
  get reviewCardCache() { return reviewCardCache; },
  get reviewClose() { return reviewClose; },
  get reviewCommentActions() { return reviewCommentActions; },
  get reviewComposerState() { return reviewComposerState; }, set reviewComposerState(v) { reviewComposerState = v; },
  get reviewContextAt() { return reviewContextAt; },
  get reviewDeleteConfirmId() { return reviewDeleteConfirmId; }, set reviewDeleteConfirmId(v) { reviewDeleteConfirmId = v; },
  get reviewFilter() { return reviewFilter; }, set reviewFilter(v) { reviewFilter = v; },
  get reviewFilters() { return reviewFilters; },
  get reviewInlineCard() { return reviewInlineCard; }, set reviewInlineCard(v) { reviewInlineCard = v; },
  get reviewLayout() { return reviewLayout; }, set reviewLayout(v) { reviewLayout = v; },
  get reviewMode() { return reviewMode; },
  get reviewModeButtons() { return reviewModeButtons; },
  get reviewNext() { return reviewNext; },
  get reviewPrevious() { return reviewPrevious; },
  get reviewRangeClientRect() { return reviewRangeClientRect; },
  get reviewRejectAll() { return reviewRejectAll; },
  get reviewSidebar() { return reviewSidebar; },
  get reviewSidebarPreference() { return reviewSidebarPreference; }, set reviewSidebarPreference(v) { reviewSidebarPreference = v; },
  get reviewTracking() { return reviewTracking; },
  get revisionRange() { return revisionRange; },
  get revisionUnreadable() { return revisionUnreadable; }, set revisionUnreadable(v) { revisionUnreadable = v; },
  get ribbonSurfaceEnabled() { return ribbonSurfaceEnabled; },
  get ribbonSurfaceReason() { return ribbonSurfaceReason; },
  get ribbonViewCollapsed() { return ribbonViewCollapsed; },
  get rulerView() { return rulerView; },
  get runEdit() { return runEdit; },
  get runNodeEdit() { return runNodeEdit; },
  get runToolbarEdit() { return runToolbarEdit; },
  get runningEditBand() { return runningEditBand; },
  get saveBtn() { return saveBtn; },
  get saveDocument() { return saveDocument; },
  get saveFormatEl() { return saveFormatEl; },
  get saveSettings() { return saveSettings; },
  get scaleOf() { return scaleOf; },
  get scheduleChromeRefresh() { return scheduleChromeRefresh; },
  get scheduleReviewMarginRender() { return scheduleReviewMarginRender; },
  get scrollCaretIntoView() { return scrollCaretIntoView; },
  get scrollFindMatchIntoView() { return scrollFindMatchIntoView; },
  get scrollReviewSelectionIntoView() { return scrollReviewSelectionIntoView; },
  get searchTrigger() { return searchTrigger; },
  get selComment() { return selComment; },
  get selEndpoints() { return selEndpoints; },
  get selectAll() { return selectAll; },
  get selectObject() { return selectObject; },
  get selectRibbonTab() { return selectRibbonTab; },
  get selectedObjectContext() { return selectedObjectContext; },
  get selection() { return selection; }, set selection(v) { selection = v; },
  get selectionContainsClientPoint() { return selectionContainsClientPoint; },
  get setDocumentState() { return setDocumentState; },
  get setDraftStatus() { return setDraftStatus; },
  get setGrammarCheckEnabled() { return setGrammarCheckEnabled; },
  get setObjectWrap() { return setObjectWrap; },
  get setReviewMode() { return setReviewMode; },
  get setRibbonCollapsed() { return setRibbonCollapsed; },
  get setSpellCheckEnabled() { return setSpellCheckEnabled; },
  get setStatus() { return setStatus; },
  get setZoom() { return setZoom; },
  get setZoomMode() { return setZoomMode; },
  get settings() { return settings; }, set settings(v) { settings = v; },
  get settingsBtn() { return settingsBtn; },
  get shapeDrawMode() { return shapeDrawMode; },
  get sharedRestoreRefusal() { return sharedRestoreRefusal; },
  get showChange() { return showChange; },
  get showDraftRecovery() { return showDraftRecovery; },
  get showImePreedit() { return showImePreedit; },
  get showingChanges() { return showingChanges; }, set showingChanges(v) { showingChanges = v; },
  get spacingMenuControl() { return spacingMenuControl; },
  get spellCheckToggle() { return spellCheckToggle; },
  get spellChecker() { return spellChecker; },
  get startCollaboration() { return startCollaboration; },
  get startDrafts() { return startDrafts; },
  get statusChannel() { return statusChannel; },
  get statusEl() { return statusEl; },
  get stepFontSize() { return stepFontSize; },
  get stepZoom() { return stepZoom; },
  get styleAdvanced() { return styleAdvanced; },
  get stylesMenu() { return stylesMenu; },
  get stylesMenuInput() { return stylesMenuInput; },
  get stylesTrigger() { return stylesTrigger; },
  get subBtn() { return subBtn; },
  get suggestAcrossParagraphs() { return suggestAcrossParagraphs; },
  get suggestingBannerEdit() { return suggestingBannerEdit; },
  get superBtn() { return superBtn; },
  get syncActiveReviewCommentToCaret() { return syncActiveReviewCommentToCaret; },
  get syncSelectionToCellRange() { return syncSelectionToCellRange; },
  get syncStylesGalleryActive() { return syncStylesGalleryActive; },
  get tabStopsDialog() { return tabStopsDialog; },
  get tableBtn() { return tableBtn; },
  get tableChrome() { return tableChrome; },
  get tableGutter() { return tableGutter; },
  get tableRange() { return tableRange; },
  get tableStyleBtn() { return tableStyleBtn; },
  get tableStyleMenu() { return tableStyleMenu; },
  get tableStylePopover() { return tableStylePopover; }, set tableStylePopover(v) { tableStylePopover = v; },
  get tableToolCommands() { return tableToolCommands; },
  get takeDraftSnapshot() { return takeDraftSnapshot; },
  get textColorPopover() { return textColorPopover; },
  get tocCommands() { return tocCommands; },
  get tocEntryTargetAt() { return tocEntryTargetAt; },
  get toggleAbout() { return toggleAbout; },
  get toggleChecklistCommand() { return toggleChecklistCommand; },
  get toggleFormCheckboxAt() { return toggleFormCheckboxAt; },
  get toggleFormat() { return toggleFormat; },
  get toggleObjectInspector() { return toggleObjectInspector; },
  get toggleOutline() { return toggleOutline; },
  get toggleParagraphProperties() { return toggleParagraphProperties; },
  get toggleReview() { return toggleReview; },
  get toggleShortcutsReference() { return toggleShortcutsReference; },
  get toggleShowChanges() { return toggleShowChanges; },
  get toggleSplitCellDialog() { return toggleSplitCellDialog; },
  get toggleTableProperties() { return toggleTableProperties; },
  get touchSelection() { return touchSelection; },
  get traverseObjects() { return traverseObjects; },
  get typingSession() { return typingSession; }, set typingSession(v) { typingSession = v; },
  get typingSessionActive() { return typingSessionActive; }, set typingSessionActive(v) { typingSessionActive = v; },
  get typingSessionForKey() { return typingSessionForKey; },
  get underlineMenuBtn() { return underlineMenuBtn; },
  get updateImePreedit() { return updateImePreedit; },
  get updateObjectContextBar() { return updateObjectContextBar; },
  get updateObjectSelectionState() { return updateObjectSelectionState; },
  get updatePageWindow() { return updatePageWindow; },
  get updateStats() { return updateStats; },
  get updateStyleFromSelection() { return updateStyleFromSelection; },
  get updateToolbar() { return updateToolbar; },
  get versionHistory() { return versionHistory; },
  get versionPreviewHome() { return versionPreviewHome; }, set versionPreviewHome(v) { versionPreviewHome = v; },
  get verticalGoal() { return verticalGoal; },
  get viewZoom() { return viewZoom; },
  get viewingBannerEdit() { return viewingBannerEdit; },
  get viewportEl() { return viewportEl; },
  get zoomEl() { return zoomEl; },
  get zoomFactor() { return zoomFactor; },
  get zoomMenu() { return zoomMenu; },
  get zoomMode() { return zoomMode; }, set zoomMode(v) { zoomMode = v; },
};
// editor-context:end


/** url → fetched or pending font bytes (persists across documents). */
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
const { clearFindParagraphCache, findBtn, findSession, openFind } = createFind(ed);

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
// second code path. Replace opens the same Find & Replace panel on its replace field.
pasteBtn.addEventListener("click", () => { paste(); });
cutBtn.addEventListener("click", () => { cut(); });
copyBtn.addEventListener("click", () => { copySelection(); });
replaceBtn.addEventListener("click", () => { if (!replaceBtn.disabled) openFind({ replace: true }); });

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
  setStatus(`Updated “${styleDisplayName(name)}” to match the selection`);
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
const reviewTracking = createReviewTracking(); // the document's Track Changes setting and the mode (HF-283)
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
// RC-12 / RC-11: the quoted anchor text per comment, and the comment being edited, outlive a card rebuild.
const commentQuotes = createCommentQuotes(), commentEdit = createCommentEditState();
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
  reviewTracking.apply({ doc, mode: reviewMode, byUser: restoreFocus && !hostSession?.executing, caret: selection?.focus, runEdit }); // HF-131; HF-283: a host's switch is not the reader's
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

/** The in-place editor for `comment` when it is the one being edited (RC-11), else `null`. */
function commentEditorFor(comment, kind) {
  const draft = commentEdit.draftFor(comment.id);
  if (draft === null) return null;
  const done = () => { commentEdit.end(); scheduleReviewMarginRender(); focusEditorSurface(); };
  const editor = commentEditor({ text: draft, kind, t, onInput: (text) => commentEdit.update(comment.id, text), onCancel: done,
    onSave: async (text) => {
      await runEdit(() => doc.updateComment(comment.id, text), { keepView: true });
      announceReview(t(kind === "reply" ? "reviewCard.replyUpdated" : "reviewCard.commentUpdated"));
      done();
    } });
  requestAnimationFrame(() => { if (editor.root.isConnected && document.activeElement !== editor.field) editor.focus(); });
  return editor.root;
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
    if (rect) items.push({ type: "comment", data: comment, rect, quote: commentQuotes.read(doc, comment) });
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
  if (show) closeFlankingPanel(outlinePanel, railOutline);
  if (show) closeFlankingPanel(pagesPanel, railPages);
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
      editingId: commentEdit.id(),
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
      const cancel = reviewCardButton("Cancel", () => { closeReviewPopover(); focusEditorSurface(); });
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
        focusEditorSurface();
      }, false, "Add comment");
      submit.dataset.testid = "review-comment-submit";
      // Enter submits, Shift+Enter is a newline, Esc cancels — the modern
      // Docs/Word comment interaction, identical to the reply composer (Q5).
      attachComposerKeys(textarea, {
        onSubmit: () => submit.click(),
        onCancel: () => cancel.click(),
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
          focusEditorSurface();
        }),
        ...(item.data.resolved ? [] : [commentEditButton(t, () => {
          commentEdit.begin(item.data.id, item.data.text);
          scheduleReviewMarginRender();
        })]),
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
    const quote = item.type === "comment" ? commentQuoteElement(item.quote, t) : null;
    const bodyOrEditor = item.type === "comment" && expanded ? commentEditorFor(item.data, "comment") ?? body : body;
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
            focusEditorSurface();
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
    card.append(header, ...(quote ? [quote] : []), bodyOrEditor);
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
          replyItem.append(replyHead, (expanded && commentEditorFor(reply, "reply")) || replyBody);
          // REVIEW-GAP-011: edit or delete this specific reply (not just the
          // whole thread). Only on an expanded, unresolved parent card, so the
          // controls stay out of the collapsed/resolved presentation.
          if (expanded && !item.data.resolved) {
            const replyActionRow = document.createElement("div");
            replyActionRow.className = "review-margin-reply-actions";
            const editReply = reviewCardButton(t("reviewCard.edit"), () => {
              commentEdit.begin(reply.id, reply.text);
              scheduleReviewMarginRender();
            }, false, t("reviewCard.editReply"));
            const deleteReply = reviewCardButton(
              "Delete",
              async () => {
                await runEdit(() => doc.deleteReply(reply.id));
                announceReview("Reply deleted");
                drawSelection();
                focusEditorSurface();
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
          focusEditorSurface();
        }, false, "Send reply");
        const cancel = reviewCardButton("Cancel", () => {
          clear();
          focusEditorSurface();
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
// The findings chip is a button that opens them (`compat_findings.mjs`), not a count leading nowhere.
const compatFindings = createCompatibilityFindings({ chip: document.getElementById("compatibilityStatus"), registerModal, fallbackFocus: () => pagesEl });
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
/** F6 / Shift+F6 between the window's regions (`region_focus.mjs`); early, because the registry reads it. */
const regionCycle = createRegionCycle({ root: document, focusEditor: () => focusEditorSurface(), documentOpen: () => !!doc, modalOpen: modalIsOpen });
/** The shared session's byte pipe, or `null` in the standalone mode (`152` §2a). */
let collab = null;
const { presence: remotePresence, recovery, share, chrome: shareChrome,
  start: startCollaboration, commands: collabFeatureCommands, loadStartup: loadSharedStartup, beforeReplace: leaveSharedDocument, restoreRefusal: sharedRestoreRefusal } = createCollaborationShell({
  root: document, getDocument: () => doc, getTransport: () => collab, setTransport: (pipe) => { collab = pipe; },
  getSelection: () => selection, getSettings: () => settings, getName: () => currentName, hostConfig,
  paintOverlay: () => paintOverlayLayer(), receiveAccess: (outcome) => accessChrome.received(outcome),
  arrival: (outcome) => { arrivalPaint = arrivalPaint.then(() => paintArrival(outcome)); },
  repaintRecovered: renderAll, edited: () => noteDocumentEdited(doc.revision),
  setSelection: (next) => { selection = next; }, status: setStatus, t,
});

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

const { boot, newBlankDocument, openBytes } = createFileOpen(ed);

// Provision the host-owned named families in one bounded batch/repagination,
// then fetch only the script fallbacks this document's uncovered code points
// require. Network failures do not block opening: the target-bundled
// metric-compatible faces remain available.
async function provisionFonts(name, target = doc, state = fontState) {
  if (!target || state.retired) return [];
  const warnings = [];
  statusChannel.progress(`Fetching web fonts for ${name}…`);

  const named = await Promise.allSettled(
    NAMED_WEB_FONT_FACES.map((face) => fetchSharedFontBytes(face.url, fontCache)),
  );
  const namedBytes = named
    .filter((result) => result.status === "fulfilled")
    .map((result) => result.value);
  if (namedBytes.length > 0 && !state.retired) {
    const packed = packFontBytes(namedBytes);
    target.registerFonts(packed.bytes, packed.lengths);
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

  warnings.push(...(await provisionMissingFallbacks(name, target, state)));
  return warnings;
}

let fontState = { provisionedFallbackKeys: new Set(), inFlightFallbackKeys: new Set(), retired: false };

/** Fetches and registers any script fallback fonts the document now needs but
 *  hasn't got yet (`doc.missingCoverage()` → buckets), skipping ones already
 *  provisioned. Used both on open and after an edit that introduces new glyphs
 *  (e.g. a checklist's `☐`/`☒` markers), so newly-added symbols render instead of
 *  tofu. Returns the keys that failed to load. */
async function provisionMissingFallbacks(label, target = doc, state = fontState) {
  const warnings = [];
  if (!target || state.retired) return warnings;
  const { provisionedFallbackKeys, inFlightFallbackKeys } = state;
  const missing = target.missingCoverage();
  const keys = fallbackKeysFor(missing).filter(
    (key) => !provisionedFallbackKeys.has(key) && !inFlightFallbackKeys.has(key),
  );
  if (keys.length === 0) return warnings;
  if (doc === target) statusChannel.progress(`Fetching fonts for ${label} (${keys.join(", ")})…`);
  for (const key of keys) inFlightFallbackKeys.add(key);
  for (const key of keys) {
    const { url, scripts } = SCRIPT_FALLBACK_FONTS[key];
    try {
      const bytes = await fetchSharedFontBytes(url, fontCache);
      if (!state.retired) {
        target.registerFallbackFont(bytes, scripts); // registers + re-paginates
        provisionedFallbackKeys.add(key);
      }
      // WASM holds the authoritative copy now; drop the JS cache entry so the
      // fallback bytes are not double-held for the session (see registerFonts).
      fontCache.delete(url);
    } catch (err) {
      console.warn(`font ${key} (${url}) failed:`, err);
      if (doc === target) setStatus(`Could not load the ${key} font — some text may show as ▯`, "error");
      warnings.push(key);
    } finally {
      inFlightFallbackKeys.delete(key);
    }
  }
  return warnings;
}

/** Ensures any glyphs a just-applied edit introduced (e.g. checklist checkbox
 *  markers) have a covering font, then re-renders if one was fetched. */
async function ensureGlyphCoverage(label, target = doc, state = fontState) {
  if (versionPreviewHome) return;
  const before = state.provisionedFallbackKeys.size;
  await provisionMissingFallbacks(label, target, state);
  if (doc === target && state.provisionedFallbackKeys.size !== before) {
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
  statusChannel.progress(renderingStatus); // background progress: never over the reader's message

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
  pageBandModel = buildPageBand(sizes, cssPerTwip, pageBandPitch(reflowing));
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
    // The progress above painted only if nobody held the line, so what a command
    // just told the reader survives the render ("Footnote added" did not, when the
    // font upgrade repainted). Clearing our own text hands the line back to any
    // standing condition, e.g. "no dictionary for this document's language".
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
  if (!control) return toggleTextCheckboxAt(doc, node, offset, runEdit);
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

/** Clears caret/selection chrome while retaining previews of held object gestures. */
function clearOverlays() {
  for (let i = pageWindow.first; i <= pageWindow.last; i++) {
    const overlay = pages[i]?.overlay;
    overlay?.replaceChildren(...overlay.querySelectorAll(":scope > .object-resize-preview, :scope > .object-rotate-preview"));
  }
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
  findSession.paint(); // every Find match, the current one distinct (TA-04)
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
  remotePresence.paint(place);
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
  remotePresence.changed();
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
  if ((objectSelection.canMove || objectSelection.canMoveInText) && padPage?.overlay) {
    paintMovePad(padPage.overlay, rect.slice(1, 5), scaleOf(padPage), (event) =>
      objectSelection.canMove ? startObjectMove(event, padPage, node) : objectTextDrag.arm(event, padPage, objectSelection));
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
  const rectFlat = doc.objectRect(s.node);
  if (rectFlat.length < 5) return;
  s.box = rectFlat.slice(1, 5); // the picture as placed NOW, not at enterCropMode (HF-106)
  const [bx, by, bw, bh] = s.box;
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
    onEnd: (cancelled) => {
      if (!objectCropSession) return;
      // A gesture the browser cancelled leaves the crop where it started.
      if (cancelled) objectCropSession.crop = objectCropSession.handleDrag?.startCrop ?? objectCropSession.crop;
      objectCropSession.handleDrag = null;
      if (cancelled) drawSelection();
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
/** A drag on an object that cannot move says why, once (HF-259). */
const refusedObjectDrag = createRefusedDrag({ setStatus: (text, kind) => setStatus(text, kind) });
/** Dragging, or F2 ("Move to where?"), an in-line object to a new place in the text (`109` UX-OB-02). */
const objectTextIo = {
  doc: () => doc, pageAt: pageFromClientPoint, pageByNumber: (n) => pages[n - 1], pointToTwip, scaleOf, platform: EDITOR_KEYBOARD_PLATFORM, t, viewport: () => viewportEl,
  blocked: () => objectEditBlocked(), runEdit: (thunk, options) => runEdit(thunk, options), setStatus: (text, kind) => setStatus(text, kind),
  select: (node, held) => selectObject(node, held.kind, null, false, { ...held, ref: { ...held.ref, root: node, subject: node } }),
  caret: () => selection?.focus ?? null,
  selection: () => (objectSelection?.mode === "selected" ? objectSelection : null),
  leaveObject: () => { objectSelection = null; clearObjectStatus(); drawSelection(); },
  refusal: (held) => capabilityRefusal(held, held.anchored ? "canMove" : "canMoveInText", SESSION.sentenceFor),
};
const objectTextDrag = createObjectTextDrag(objectTextIo);
const keyboardObjectMove = createKeyboardObjectMove(objectTextIo);
/** The crop session's keyboard (`object_keys.mjs`). */
const cropKeyIo = {
  get session() { return objectCropSession; },
  commit: () => commitCrop(),
  cancel: () => cancelCrop(),
  redraw: () => drawSelection(),
  nothingToMove: () => setStatus(t("object.crop.nothingToMove")),
};
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
  reflectShapeFormat(pagesEl, objectSelection?.canFill || objectSelection?.canStroke ? selectedShapeFormat() : null);
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
  t,
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
  openChartData: () => chartSurface.openData(),
  enterCrop: () => enterCropMode(),
  canChangePicture,
  changePicture: () => pictureReplace.choose(),
  deleteObject: () => deleteSelectedObject(),
  reflectShapeSwatches: () => reflectShapeSwatches(),
  fillButton: () => shapeFillBtn,
  outlineButton: () => shapeOutlineBtn,
  positionButton: () => labelledObjectMenuButton(objectPositionBtn),
  arrangeButton: () => labelledObjectMenuButton(objectArrangeBtn),
  rotateButton: () => labelledObjectMenuButton(objectRotateBtn),
});
const updateObjectContextBar = () => (objectBar.update(), chartSurface.sync());
/** The chart's Chart tab, settings panel, data dialog and command tree (`chart_surface.mjs`). */
const chartSurface = createChartSurface({
  doc: () => doc,
  selection: () => objectSelection,
  blocked: () => objectEditBlocked(),
  blockedReason: () => readOnlyReason || (reviewMode === "viewing" ? t("chart.reason.viewing") : reviewMode === "suggesting" ? t("chart.reason.suggesting") : ""),
  apply: (result) => applyEditResult(result, { keepView: true }),
  setStatus: (text, kind) => setStatus(text, kind),
  returnFocus: () => focusEditorSurface(),
  registerModal,
  showMenu: (button, commands) => { const box = button.getBoundingClientRect(); showContextMenu(box.left, box.bottom, { surface: "ribbon", commands }); },
  reflectTab: (tab, state) => reflectEnablement(tab, state, EDITOR_KEYBOARD_PLATFORM),
  selectTab: (name) => selectRibbonTab(name),
  t,
});
/** Insert ▸ Chart, as ONLYOFFICE and Word do it: the chart lands SELECTED with its data open. */
const insertChartWithData = () => chartSurface.insertAndOpen(() => insertChartAtCaret({ doc, caret: selection?.focus, blocked: blockMutationInViewing, suggesting: () => reviewMode === "suggesting", status: setStatus, apply: applyEditResult }), (chart) => selectObject(chart.node, chart.kind, null, chart.anchored, chart));
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
  "canMoveInText",
];

/** Copies the engine-declared structural capabilities before a wasm hit payload
 *  is freed. JSON object-order entries use the same camelCase field names. */
function objectCapabilities(source) {
  if (source && OBJECT_CAPABILITY_KEYS.some((key) => key in source)) {
    // The engine's reason for every `false` travels with the bits (HF-259), and so does the file's aspect lock (FID-AT-09).
    const reasons = { capabilityReasons: readCapabilityReasons(source), locksAspectRatio: source.locksAspectRatio === true };
    return Object.assign(Object.fromEntries(OBJECT_CAPABILITY_KEYS.map((key) => [key, source[key] === true])), reasons);
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
  // ONE engine action (`docs/109` OL-04): as two calls only the second
  // result's pages were repainted and the text did not reflow.
  runEdit(() => runWrapPlan(doc, state.root, plan), { gate: true });
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
  // A member of a group is deleted ALONE, as in Word; sending it to the root
  // deleted everything grouped with it (HF-214).
  const member = insideGroupSelection() ? objectSelection.node : null;
  runEdit(
    () => {
      const res = member ? doc.deleteGroupMember(member) : doc.deleteObject(root);
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

/** Nudges the selected floating object one `objectNudge` step (`object_keys.mjs`
 *  owns which key means which step), as a single `SetAnchor` op (gated in
 *  Viewing/Suggesting like a drag-move). */
function nudgeSelectedObject({ dx, dy, step }) {
  if (!doc || !objectSelection?.canMove) return;
  const subject = objectSelection.node;
  const root = objectSelection.ref.root;
  const rect = doc.objectRect(subject); // [page, x, y, w, h] twips
  if (rect.length < 5) return;
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
    moveDrag: objectMoveDrag ?? (objectTextDrag.dragging() || null),
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
    // The same gesture that selects an object drags it (a bare click commits
    // nothing): a floating one anywhere on the page, an in-line one to another
    // place in the text (UX-OB-02), and any other says why it cannot.
    if (descriptor.canMove) startObjectMove(event, page, node);
    else if (!objectTextDrag.arm(event, page, objectSelection)) refusedObjectDrag.arm(event, objectTextIo.refusal({ ...descriptor, anchored }));
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
  if (objectTextDrag.active()) return; // the window listener drives it, once per event
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

function startSelectionAutoScroll() {
  if (selectionAutoScrollFrame) return;
  const tick = () => {
    selectionAutoScrollFrame = 0;
    if (!dragging || !pointerGesture) return;
    // Both axes (`edge_scroll.mjs`, shared with the in-text object drag).
    const { dx, dy } = edgeScrollStep(viewportEl.getBoundingClientRect(), pointerGesture.lastClientX, pointerGesture.lastClientY);
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
  refusedObjectDrag.end();
  if (objectTextDrag.finish(event)) return;
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

/** The selection as clipboard HTML: the exact `copyRichRuns` JSON embedded as a leading comment (a
 * lossless internal round-trip marker) plus a visible rendering built from the same runs and the
 * block outline (headings, lists, tables: what an external app sees, TA-12). `null` if nothing. */
function selectionRichHtml() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  const runsJson = doc.copyRichRuns(anchor.node, anchor.offset, focus.node, focus.offset);
  const runs = JSON.parse(runsJson);
  const outline = JSON.parse(doc.copyBlockOutline(anchor.node, anchor.offset, focus.node, focus.offset));
  // When the selection spans block structure the flat runs flatten — a table or
  // a list — carry a structured payload for internal OpenDoc-to-OpenDoc paste
  // (`{ blocks, runs }`: the flat runs ride along so a Suggesting-mode paste, or
  // a structured paste the engine declines, still has the rich-run fallback).
  const structured = doc.copyStructured(anchor.node, anchor.offset, focus.node, focus.offset);
  if (structured) {
    const blocks = JSON.parse(structured).blocks;
    return embedMarker(JSON.stringify({ blocks, runs })) + runsToHtml(runs, outline);
  }
  if (!runs.length) return null;
  return embedMarker(runsJson) + runsToHtml(runs, outline);
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
  // Only a hit inside a page places the caret: a gap click must not jump into the nearest fragment.
  const page = pageFromEvent(e);
  if (page) onPointerDown(page, e);
});
installViewportFocusGuard({ viewport: viewportEl, pages: pagesEl, lastSheet: () => pages[pages.length - 1]?.wrap, focus: focusEditorSurface, caretToEnd: (extend) => doc && navToPosition(doc.lastPosition(), extend) });
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
  if (refusedObjectDrag.move(e)) return; // a drag of an object that cannot move says why
  if (objectTextDrag.move(e)) return; // an in-line object follows the drop caret (UX-OB-02)
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
    if (objectSelection?.kind === "chart" && chartSurface.openData(objectSelection.node)) return e.preventDefault();
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
  objectTextDrag.cancel();
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

const {
  buildContextCommands, caretTableColumn, contextAt, decideContextRevision, editorContextMenu,
  objectContextAtEvent, objectContextMenuHost, plainTableInfo, reviewContextAt, selectTableContext,
  selectionContainsClientPoint, tableToolCommands,
} = createContextMenu(ed);
// Open menus form a stack: index 0 is the root context menu, deeper indexes are
// nested submenu flyouts. `keyboardLevelIndex` marks which level the arrow keys
// currently drive; hovering a submenu opens a deeper level visually without
// stealing keyboard control until the user presses ArrowRight/Enter.
let menuLevels = [];
let keyboardLevelIndex = 0;
let contextMenuReturnFocus = null;
const { foldView, reflowView, showContextMenu } = createMenuRender(ed);
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

const styleAdvanced = createStyleAdvanced({ getDoc: () => doc, hasCaret: () => !!selection, currentStyle: currentParagraphStyleName, runToolbarEdit, onButton, openPanel: (focus) => toggleParagraphProperties(true, focus), refresh: populateStyles });
const listAdvanced = createListAdvanced({ getDoc: () => doc, getNode: () => selection?.focus.node, runNodeEdit, onButton, openPanel: (focus) => toggleParagraphProperties(true, focus), ensureGlyphCoverage });
const paragraphAdvanced = createParagraphAdvanced({ getDoc: () => doc, getSelection: () => selection, getEndpoints: selEndpoints, runToolbarEdit, onButton });
const fontAdvanced = createFontAdvanced({ getDoc: () => doc, getSelection: () => selection, getEndpoints: selEndpoints, getPendingFormat: () => pendingFormat, setPendingFormat: (value) => { pendingFormat = value; }, getReviewMode: () => reviewMode, hasRange, blockMutationInViewing, runToolbarEdit, updateToolbar, registerModal, onButton, fallbackFocus: () => pagesEl });

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
  { command: "insert.chart", buttons: [insertChartBtn], requires: "doc", activate: () => void insertChartWithData() },
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
  { command: "tools.smartQuotes", buttons: () => [reviewSmartQuotesBtn], requires: "always", pressed: () => autoCorrect.enabled("smartQuotes"), run: () => autoCorrect.toggle("smartQuotes") },
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
  // Manage access. `requires: "doc"` because there is nothing to manage access TO
  // without one; whether the button is PRESENT at all is `accessChrome.reflect`'s
  // answer and not this table's, because absence is a permission decision and
  // this table only knows about enablement. Same group as Restrict Editing, which
  // is Word's own grouping of the two authority questions.
  { command: "review.manageAccess", buttons: () => [document.getElementById("reviewManageAccessBtn")].filter(Boolean), requires: "doc", reasonKey: "command.needsDocument", run: () => accessChrome.open() },
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
  scrollMarkerIntoViewX(marker); // sideways first: it moves nothing vertically (TA-10)
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

/** Follows a marker sideways when zoom makes the page wider than the window
 *  (`docs/174` TA-10; Word and Docs do). Every scroll-into-view path ends here,
 *  the model path included once it has painted the marker. */
function scrollMarkerIntoViewX(marker) {
  if (!marker || viewportEl.scrollWidth <= viewportEl.clientWidth) return;
  const { left } = viewportEl.getBoundingClientRect();
  const max = viewportEl.scrollWidth - viewportEl.clientWidth;
  const viewport = { left, right: left + viewportEl.clientWidth };
  const target = scrollLeftFor({ marker: marker.getBoundingClientRect(), viewport, current: viewportEl.scrollLeft, max, margin: SCROLL_INTO_VIEW_MARGIN });
  if (target !== null) viewportEl.scrollLeft = target;
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
    return scrollMarkerIntoViewX(pagesEl.querySelector(".overlay .caret"));
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
    return scrollMarkerIntoViewX(pagesEl.querySelector(".overlay .highlight"));
  }
  scrollOverlayIntoView(marker, "center");
}

const {
  documentIsDirty, markDocumentSaved, noteDocumentEdited, resetDirtyTracking,
} = createDirtyTracking(ed);
let currentRevision = 0;
let revisionUnreadable = false;
// A third axis, and the only one that is not about editing: a document restored
// from a crash draft — or from a version in the timeline (`docs/139` §8.5 step
// 7) — has never been written out ANYWHERE. Its revision starts at the open-time
// baseline and its name matches, so both axes above would call it clean — which
// would re-arm, one level up, exactly the data loss the draft exists to prevent.
// Both restores set it for the same reason, which is why it is one flag and not
// two. Cleared by the same two places that clear the others.
let restoredFromDraft = false;
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
const {
  applyEditResult, blockUntrackedInSuggesting, breakTypingSession, paintArrival,
  typingSessionForKey,
} = createEditPipeline(ed);
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
async function runEdit(thunk, { typing = false, gate = false, keepView = false, keepSelection = false } = {}) {
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
  await applyEditResult(res, { keepView, keepSelection });
  return true;
}
const { navCaret, verticalGoal } = createCaretNav(ed);
const {
  NO_TABLE_STYLE, UNDERLINE_STYLE_LABELS, caretFormatState, editSelectionLink, formatToggleButtons,
  formatToggleCache, hexToRgb, populateTableStyles, reflectFontFamily, reflectUnderlineControl,
  selectionFormat, suggestAcrossParagraphs, toggleFormat,
} = createFormatting(ed);
/** The current selection endpoints as `[sNode, sOff, eNode, eOff]`, or null. */
function selEndpoints() {
  if (!selection) return null;
  const { anchor, focus } = selection;
  return [anchor.node, anchor.offset, focus.node, focus.offset];
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
    setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason, routeRefusal: SESSION.sentenceFor }), "error");
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
/** Reflects an authored font family without restricting imported documents to
 * the toolbar's starter list. The temporary option is presentation-only: the
 * renderer's physical substitution/fallback family is never written back as the
 * document's requested font. */
let currentFontFamily = "";
let currentUnderlineStyle = "single";
let currentUnderlineColor = "";
let currentUnderlineMixed = false;
/** The compact bar, assigned once its dependencies exist (bottom of the file).
 *  Declared here because `updateToolbar` runs before that point and reflecting
 *  alignment into an uninitialised `const` would be a temporal-dead-zone throw
 *  rather than a no-op. */
let compactToolbarUi = null;
/** Reflects the selection in the toolbar: active states + which controls are
 *  enabled (run controls need a text range; paragraph controls need a caret). */
function updateToolbar() { fontAdvanced.reflect();
  shareChrome.reflect();
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
  // Restart/Continue numbering, each saying WHY when unavailable — the palette's own sentences.
  const numbering = listNumberingStates({ hasCaret: hasSel && !!doc, listKind, canContinue: () => doc.canContinueList(selection.focus.node) });
  reflectEnablement(restartListBtn, numbering.restart, EDITOR_KEYBOARD_PLATFORM);
  reflectEnablement(continueListBtn, numbering.continue, EDITOR_KEYBOARD_PLATFORM);
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
    for (const button of entry.buttons) reflectEnablement(button, { enabled, reasonKey: INSERT_REASON_KEYS[entry.requires] }, EDITOR_KEYBOARD_PLATFORM);
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
  // AFTER the review sweep, because it decides whether Manage access is on screen
  // at all and that outranks whether the sweep enabled it. O(1) — every input is a
  // field read, so it costs nothing on the per-interaction path (`107` §4 B1).
  accessChrome.reflect();
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
  // The chord comes from the keymap, per platform (Replace is ⌘⇧H on a Mac), never from markup.
  for (const [button, id] of [[findBtn, "edit.find"], [replaceBtn, "edit.replace"]]) button.title = chordTitle(button.getAttribute("aria-label"), id, EDITOR_KEYBOARD_PLATFORM);
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
  reflectEnablement(tabTable, { enabled: !!inTable, reasonKey: "table.reason.caretOutsideTable" }, EDITOR_KEYBOARD_PLATFORM);
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
  for (const [value, label] of [["", "Style"], ...styles.map((s) => [s, styleDisplayName(s)])]) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = label;
    paraPanelStyle.appendChild(opt);
  }
  // The band's gallery offers the short list drawn from the same registry.
  buildStylesGallery(styles);
  scheduleRibbonOverflow();
}
let tableStylePopover;
const {
  TWIPS_PER_POINT, adjustIndentCommand, applyFontSize, armFormatPainter, armOrApplyRun,
  disarmFormatPainter, insertLineBreakAtSelection, paintFormatFromGesture, stylesPopover,
  toggleChecklistCommand,
} = createFormatPainter(ed);
// Single click captures the caret/selection's formatting and arms a one-shot
// paint; the next document click (expanded to the clicked word) or drag receives
// it, then it disarms. Double-click the brush locks it (sticky) so successive
// targets keep receiving the format until Esc or another brush click. Formatting
// is applied through the very same engine ops the toolbar's own Bold/color/…
// controls use, so painted formatting is indistinguishable from hand-applied.
let formatPainter = null; // { fmt, sticky } while armed, else null
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
  // A picture has the outline half — its border (HF-254) — read the same way.
  if (!doc || !objectSelection || (!objectSelection.canFill && !objectSelection.canStroke)) return {};
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
  if (!objectSelection?.canStroke) return; // a shape's outline or a picture's border
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
for (const [menu, popover] of [[bulletGalleryMenu, bulletGalleryPopover], [numberGalleryMenu, numberGalleryPopover]]) {
  wireListGallery(menu, popover, { canApply: () => !!selection && !!doc, closePopover, apply: applyListFormatCommand });
}

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
  paragraphAdvanced.reflect();
  listAdvanced.reflect();
  styleAdvanced.reflect();
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
/** "Who may do what", in one handle — `access_chrome.mjs`, which composes the
 *  rights dialog and the persistent access indicator over the same four
 *  authorities. One call here rather than two modules wired separately, because a
 *  permission resolved at two call sites is a permission two call sites can
 *  disagree about. Constructed after `documentProtection` because it reads that
 *  module's state, which is the DOCUMENT's own authority. */
const accessChrome = createAccessChrome({
  getDoc: () => doc,
  getTransport: () => collab,
  getGrant: () => SESSION.grant,
  getProtection: () => documentProtection.state(),
  getCapabilities: () => HOST_CAPS,
  getReadOnlyReason: () => readOnlyReason,
  registerModal,
  fallbackFocus: () => pagesEl,
  setStatus,
  t: (key, values) => t(key, values),
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
// Review ▸ Compare's seams (ADR-061, ADR-065), inline because this module has no
// mount seam (`109` HF-085); `landed` is also the capability test for "Keep".
const yieldFrame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
const redlineOf = (io) => buildRedline({ ...io, engine: { begin: beginVersionDiff, slice: defaultDiffSlice, open }, yieldToHost: yieldFrame });
/** Takes the reader to one change on a redline (ADR-065): the selection and the scroll, focus left on the control they stepped with. */
const showChange = (a) => { selection = { anchor: { node: a.node, offset: Number(a.start) || 0 }, focus: { node: a.node, offset: Number(a.end) || Number(a.start) || 0 } }; drawSelection(); if (scrollModelRectIntoView(selectionModelRect(), "center")) paintOverlayLayer(); };
const comparePanel = bindComparePanel({ doc: () => doc, currentBytes: () => comparableBytes(doc, currentSourceFormat), redline: redlineOf, showView: (view) => (view ? showVersionPreview(view, { markup: true, reason: t("compare.viewReadOnly") }) : showVersionPreview(null)), beforeView: () => versionHistory.closePreview(), yieldToHost: yieldFrame, setStatus: (text, kind) => setStatus(text, kind), allowed: () => HOST_CAPS.has("open"), refusedReason: t("capability.notGranted"), blockedReason: () => (blockMutationInViewing() ? mutationBlockedMessage({ editingUnavailableReason: readOnlyReason }) : ""), readOnlyReason: () => readOnlyReason, landed: async (res) => { await applyEditResult(res); await setShowingChanges(true); scheduleReviewMarginRender(); }, navigate: showChange });

// The band's structural controls, declared in `table_band.mjs` (`109` UX-005).
// Its Select handler used to be a second copy of `selectTableContext`.
bindTableBand({
  root: tableRibbon,
  onButton,
  getDoc: () => doc,
  getSelection: () => selection,
  runEdit,
  clearTableSelection: () => tableRange.clear(),
  selectTableContext,
  caretTableColumn,
  status: (text) => setStatus(text),
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
  await announceTableEdit(runEdit(() => doc.mergeTableCellRange(range.anchorNode, range.focusNode), { gate: true }), "merge", setStatus);
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
  tableColumnWidth.disabled = false; // a merged table sizes its grid columns too (docs/171)
  tableColumnWidth.value = twipsToDialogInches(info.columnWidthTwips);
  tableColumnWidthNote.textContent = "Sets the width of the current column.";
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

/** The form's live fields, named once so the patch builder is a pure function of
 *  its bag rather than of this file's eighteen consts. */
const TABLE_PROPERTIES_FORM = { width: tableWidth, indent: tableIndent, columnWidth: tableColumnWidth, rowHeight: tableRowHeight, rowHeightRule: tableRowHeightRule, cellMargin: tableCellMargin, cellSpacing: tableCellSpacing, fixedLayout: tableFixedLayout, headerRow: tableHeaderRow, caption: tableCaption, description: tableDescription };
/** Validation and the CHANGED-only diff live in `table_properties_patch.mjs`,
 *  where a test can reach them — `main.js` has no exports, so the diff that
 *  decides whether eleven untouched properties are re-asserted was unreachable
 *  from node. */
function tablePropertiesPatch() {
  return tablePatch(TABLE_PROPERTIES_FORM, { alignment: tableAlignGroup.value() ?? "left", current: tablePropertiesCurrent, inchesToTwips: optionalInchesToTwips, signedInchesToTwips, rowHeightRequired: "Enter a row height or choose Auto." });
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

// -- Insert table: the size grid (`table_grid_picker.mjs`) --------------------
createTableGridPicker({
  picker: document.getElementById("gridPicker"),
  label: document.getElementById("gridLabel"),
  button: insertTableBtn,
  menu: insertTableMenu,
  registerPopover,
  closePopover,
  blocked: () => blockMutationInViewing(),
  insert: async (rows, columns) => {
    if (!selection || !doc) return false;
    await runEdit(() => doc.insertTable(selection.focus.node, rows, columns), { gate: true });
    return true;
  },
  focusEditor: focusEditorSurface,
});

/** Rebuilds the off-screen accessibility mirror for the caret's part of the
 *  document. The projection lives in `a11y_mirror.mjs`; what stays here is the
 *  editor state it needs and the gated toggle a mirror checkbox runs (HF-178). */
function buildAccessibilityTree() {
  renderAccessibilityMirror(doc, selection?.focus?.node ?? "", tableRange.gridRect(), (control) => runEdit(() => doc.toggleFormCheckbox(control)));
}

const {
  buildOutline, closeFlankingPanel, reanchorPageOverlays, reflectOutlineSelection, toggleOutline,
} = createOutline(ed);
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
    closeFlankingPanel(outlinePanel, railOutline);
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

const {
  closeReviewInlineCard, decideAllReviewChanges, decideReviewAndAdvance, decideReviewAtCaret,
  focusReviewRevision, navigateReview, openReviewComposer, reviewMoveEndButton,
  scheduleReviewInlineCardHide, showReviewInlineCard, toggleReview,
} = createReviewDecisions(ed);
// Hovering (or clicking) a tracked-change marker on the canvas surfaces a
// compact card — author, one-line change summary, ✔ Accept / ✗ Reject — the
// Google-Docs suggestion affordance. A pinned card (opened by click) stays until
// an outside pointerdown or Escape; a hover card follows the pointer and hides
// shortly after it leaves both the marker and the card. Accept/Reject reuse the
// same grouped/move-aware decision path as the sidebar and context menu.
let reviewInlineCard = null;
const { cmdInput, cmdList, cmdPalette, editorCommands, searchTrigger } = createPalette(ed);
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

const {
  bookmarkManager, buildShortcutsReference, dropCapDialog, insertFieldAtCaret, insertImageFromBlob,
  insertImageFromFile, insertShapeObject, insertTextBoxObject, openBookmarkManager, openEmojiPicker,
  openFieldDialog, openSymbolPicker, pictureReplace, referenceCommands, tocCommands, toggleAbout,
  toggleShortcutsReference,
} = createDialogs(ed);
const { openLinkDialog } = createLinkDialog(ed);
const { openCmd, renderCommands } = createPaletteUi(ed);


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

const { positionSelToolbar } = createSelectionToolbar(ed);
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
const { exportDocumentAs, saveDocument } = createSave(ed);
const { navByViewport, navToPosition, selectAll } = createSelectionCommands(ed);
const {
  commitComposedText, cut, editorTextInputEvent, hidePasteOptions, paste, pasteAsText,
  pasteOptionsShowing, pasteRichRunsJson, pasteText,
} = createClipboard(ed);
const { addWordToDictionary, autoCorrect } = createTextInput(ed);
createKeyboard(ed);
const { handleFile } = createFilePick(ed);
const { clampZoom, computeFitZoom, updateZoomDisplay, zoomMenu } = createZoom(ed);
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
// ---- Settings: theme + accent + reviewer identity, persisted (OSS-customizable) ----
const settingsBtn = document.getElementById("settingsBtn");
const {
  AUTOSAVE_ALLOWED_HERE, adoptDraftDocument, autosaveToggle, clearAllDrafts, discardOwnDraft,
  draftsClearBtn, grammarCheckToggle, handOverDraftBeforeOpen, noteDraftDirty, queueDraftWork,
  refreshDraftOffers, setDraftStatus, showDraftRecovery, spellCheckToggle, startDrafts,
  takeDraftSnapshot,
} = createDrafts(ed);
let draftDocKey = "";
let draftOffers = [];

let settings = loadSettings();

function loadSettings() {
  return loadPrefObject("opendoc.settings", DEFAULT_SETTINGS);
}

function saveSettings() {
  savePrefObject("opendoc.settings", settings);
}

const { showVersionPreview, versionHistory } = createVersions(ed);
/** The live document and the chrome state a preview borrowed, or null. */
let versionPreviewHome = null;
/** Set while a restore is activating, so the open path does not ALSO record an
 *  import baseline on top of the restore version that just committed — one user
 *  action, one point in the timeline. */
let activatingRestore = false;
const { applyActiveAuthorToDocument, toggleSettings } = createSettings(ed);
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
