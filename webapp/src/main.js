// OpenDoc WASM viewer — P1G-001 harness.
//
// Loads `casual-doc-wasm`, opens a local `.docx`, and paints pages on canvas: the
// browser-first surface the viewer→editor is built and fine-tuned on (docs 56/57);
// no server, deployable as static files (e.g. GitHub Pages).

// The measurement layer's five free functions. FREE, not methods on a document,
// because a unit preference belongs to the person and governs dialogs that open
// with nothing loaded — which is also why they are handed to
// `measurement_units.mjs` rather than reached for there (that module stays
// browser-free and engine-free, so its policy is answerable in node).
import { editRefusalMessage, mutationBlockedMessage, readRevision } from "./edit_errors.mjs";
import { renderAccessibilityMirror } from "./a11y_mirror.mjs";
import { createPagesPanel } from "./pages_panel.mjs";
import {
  reflectReviewSurface,
} from "./ribbon_surface.mjs";
import { createStyleAdvanced } from "./style_advanced.mjs";
import { createListAdvanced } from "./list_advanced.mjs";
import { previewPx } from "./style_picker.mjs";
import { styleDisplayName } from "./style_names.mjs";
import { applyPreviewInk } from "./style_preview.mjs";
import { createCompatibilityFindings } from "./compat_findings.mjs";
import { INSERT_REASON_KEYS, listNumberingStates, reflectEnablement } from "./control_reasons.mjs";
import { attachHostBridge } from "./host_bridge.mjs";
import { createHostSession } from "./host_session.mjs";
import { createAssistant } from "./ai_assistant.mjs";
import {
  buildPageBand,
  pageBandPitch,
} from "./page_scroll.mjs";
import { keyboardPlatform } from "./keyboard.mjs";
import { chordTitle } from "./keymap.mjs";
import { modalIsOpen, registerModal } from "./modal.mjs";
import { objectBarPosition } from "./object_guides.mjs";
// Own lines (anti-conflict): the two direct-manipulation geometry gestures.
import { positionPayload, readGroupability, rotationPlan } from "./object_arrange.mjs";
import { t } from "./i18n.mjs";
import { authoredTitle, paintDocumentState } from "./localize.mjs";
import { countLabels, pageIndicator, readerPosition } from "./status_counts.mjs";
import { localizeShortcutGlyphs, localizeShortcutText } from "./shortcut_labels.mjs";
import { installRibbonTooltips } from "./ribbon_tooltip.mjs";
import { rovingIndex, tabStopIndex } from "./ribbon_nav.mjs";
import { onButton } from "./popover_manager.mjs";
import { reflectOpenPopovers } from "./popover_manager.mjs";
import { createViewZoom, nextZoomStep } from "./view_zoom.mjs";
import { createPhoneChrome } from "./phone_chrome.mjs";
import { DEFAULT_SETTINGS } from "./settings_defaults.mjs";
import { editingModeFor, hostCapabilities, hostChrome, hostConfig } from "./capabilities.mjs";
// Own line (anti-conflict): the chart's tab, panel, data dialog and commands.
import { createCollaborationShell } from "./collab_shell.mjs";
import { createReviewTracking } from "./review_tracking.mjs";
import { sessionAccess } from "./session_access.mjs"; // the ROOM's grant, a different authority from the container's
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { sameModelPosition } from "./caret_navigation.mjs";
// One line, deliberately: main.js is on a line ratchet (`module_seams`).
import { createReviewBanners } from "./review_chrome.mjs";
import { createCommentEditState, createCommentQuotes } from "./review_comment_card.mjs";
import {
  TWIPS_PER_INCH,
  round2,
  twipsToDialogInches,
} from "./units.mjs";
import {
  documentStateBadge,
  documentTabTitle,
  isObjectSelectionStatus,
  statusClassName,
} from "./status_policy.mjs";
import { createStatusChannel } from "./status_channel.mjs";
import {
  renderExportPane,
  renderFilePageInfo,
} from "./command_menu.mjs";
import {
  showSettingsPane,
  releaseSettingsPanel,
} from "./file_pane.mjs";

/** Settings' two homes, for `revealOnSettings`. The gear is deliberately NOT
 *  routed through this: it TOGGLES, which is a real difference. */
const SETTINGS_SURFACE = { showPane: () => showSettingsPane(), openDialog: () => toggleSettings(true) };
import { createBackgroundMeasure, pageTotalLabel } from "./background_measure.mjs";
import { createTableRange } from "./table_range.mjs";
import { createRuler } from "./ruler.mjs";
import { createFontAdvanced } from "./font_advanced.mjs";
import { createParagraphAdvanced } from "./paragraph_advanced.mjs";
import { createTabStopsDialog } from "./tab_stops_dialog.mjs";
import { stampRibbonFaces } from "./ribbon_faces.mjs";
import { tableBandStates, tableContextLabel } from "./table_band.mjs";
import { createRegionCycle } from "./region_focus.mjs";
import { loadPrefObject, savePrefObject } from "./prefs.mjs";
import { BRAND } from "./brand.mjs";
import { applyRegions } from "./chrome_regions.mjs";
import { createFind } from "./editor_find.mjs";
import { createDrafts } from "./editor_drafts.mjs";
import { createVersions } from "./editor_versions.mjs";
import { createReviewDisplay } from "./review_display.mjs";
import { createNoteCommands } from "./note_commands.mjs";
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
import { createProofing } from "./editor_proofing.mjs";
import { createHeaderFooter } from "./editor_header_footer.mjs";
import { createPaint } from "./editor_paint.mjs";
import { createObjectMoves } from "./editor_object_moves.mjs";
import { createOverlayPaint } from "./editor_overlay_paint.mjs";
import { createPointer } from "./editor_pointer.mjs";
import { createSelection } from "./editor_selection.mjs";
import { createObjects } from "./editor_objects.mjs";
import { createFontColor } from "./editor_font_color.mjs";
import { createShapeChrome } from "./editor_shape_chrome.mjs";
import { createFontMenu } from "./editor_font_menu.mjs";
import { createShapeGallery } from "./editor_shape_gallery.mjs";
import { createParagraphInspector } from "./editor_paragraph_inspector.mjs";
import { createTableFormat } from "./editor_table_format.mjs";
import { createDocTools } from "./editor_doc_tools.mjs";
import { createTableProps } from "./editor_table_props.mjs";
import { createFilePage } from "./editor_file_page.mjs";
import { createRibbon } from "./editor_ribbon.mjs";
import { createReviewMargin } from "./editor_review_margin.mjs";
import { createFonts } from "./editor_fonts.mjs";
import { createCommandSurfaces } from "./editor_command_surfaces.mjs";
import { createScroll } from "./editor_scroll.mjs";
import { createDocumentSettings } from "./editor_document_settings.mjs";
import { createChromeModes } from "./editor_chrome_modes.mjs";
// editor-context:begin
/** The editor context (docs/177): the one object every extracted module is handed.
 *  Each entry is a live view of a binding this file still owns, read lazily, so a
 *  module sees the value at call time exactly as the code did before it moved. */
const ed = {
  get AUTOSAVE_ALLOWED_HERE() { return AUTOSAVE_ALLOWED_HERE; },
  get BASE_DPI() { return BASE_DPI; },
  get BREAK_IO() { return BREAK_IO; },
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
  get REVIEW_WINDOW_OVERSCAN() { return REVIEW_WINDOW_OVERSCAN; },
  get SESSION() { return SESSION; },
  get SETTINGS_SURFACE() { return SETTINGS_SURFACE; },
  get SHAPE_MENU_COLORS() { return SHAPE_MENU_COLORS; },
  get TWIPS_PER_POINT() { return TWIPS_PER_POINT; },
  get TYPING_PAUSE_MS() { return TYPING_PAUSE_MS; },
  get UNDERLINE_STYLE_LABELS() { return UNDERLINE_STYLE_LABELS; },
  get ZOOM_MAX() { return ZOOM_MAX; },
  get ZOOM_MIN() { return ZOOM_MIN; },
  get accessChrome() { return accessChrome; },
  get activatingRestore() { return activatingRestore; }, set activatingRestore(v) { activatingRestore = v; },
  get activeLink() { return activeLink; }, set activeLink(v) { activeLink = v; },
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
  get applyHighlight() { return applyHighlight; },
  get applyListFormatCommand() { return applyListFormatCommand; },
  get applyShapeFill() { return applyShapeFill; },
  get applyShapeOutline() { return applyShapeOutline; },
  get applyTextColor() { return applyTextColor; },
  get applyUnderlineStyle() { return applyUnderlineStyle; },
  get armBackgroundMeasure() { return armBackgroundMeasure; },
  get armFormatPainter() { return armFormatPainter; },
  get armOrApplyRun() { return armOrApplyRun; },
  get arrangeState() { return arrangeState; },
  get autoCorrect() { return autoCorrect; }, get autoCorrectDialog() { return autoCorrectDialog; }, get languageSelection() { return languageSelection; },
  get autosaveToggle() { return autosaveToggle; },
  get backgroundMeasure() { return backgroundMeasure; }, set backgroundMeasure(v) { backgroundMeasure = v; },
  get bandEl() { return bandEl; },
  get bandOffset() { return bandOffset; }, set bandOffset(v) { bandOffset = v; },
  get bandTopInScroller() { return bandTopInScroller; },
  get blockMutationInViewing() { return blockMutationInViewing; },
  get blockUntrackedInSuggesting() { return blockUntrackedInSuggesting; },
  get blockedFromPageSetup() { return blockedFromPageSetup; },
  get bookmarkManager() { return bookmarkManager; },
  get breakTypingSession() { return breakTypingSession; },
  get buildAccessibilityTree() { return buildAccessibilityTree; },
  get buildCommands() { return buildCommands; },
  get buildContextCommands() { return buildContextCommands; },
  get buildOutline() { return buildOutline; },
  get buildShortcutsReference() { return buildShortcutsReference; },
  get bulletGalleryMenu() { return bulletGalleryMenu; },
  get bulletListBtn() { return bulletListBtn; },
  get bulletListMenuBtn() { return bulletListMenuBtn; },
  get cancelCrop() { return cancelCrop; },
  get cancelObjectMove() { return cancelObjectMove; },
  get caretIsImplicit() { return caretIsImplicit; },
  get caretTableColumn() { return caretTableColumn; },
  get changeCaseBtn() { return changeCaseBtn; },
  get changeCaseMenu() { return changeCaseMenu; },
  get chartSurface() { return chartSurface; },
  get checkListBtn() { return checkListBtn; },
  get chromeShows() { return chromeShows; },
  get clearAllDrafts() { return clearAllDrafts; },
  get clearFindParagraphCache() { return clearFindParagraphCache; },
  get clearFormattingBtn() { return clearFormattingBtn; },
  get clearObjectStatus() { return clearObjectStatus; },
  get clearOverlays() { return clearOverlays; },
  get clearSnapGuides() { return clearSnapGuides; },
  get clientPointEvent() { return clientPointEvent; },
  get climbOutOfGroup() { return climbOutOfGroup; },
  get closeAppMenu() { return closeAppMenu; },
  get closeFilePage() { return closeFilePage; },
  get closeFlankingPanel() { return closeFlankingPanel; },
  get closeReviewInlineCard() { return closeReviewInlineCard; },
  get closeReviewPopover() { return closeReviewPopover; },
  get cmdInput() { return cmdInput; },
  get cmdList() { return cmdList; },
  get cmdPalette() { return cmdPalette; },
  get collab() { return collab; },
  get collabFeatureCommands() { return collabFeatureCommands; },
  get commentEdit() { return commentEdit; },
  get commentQuotes() { return commentQuotes; },
  get commitComposedText() { return commitComposedText; },
  get commitCrop() { return commitCrop; },
  get compactToolbarUi() { return compactToolbarUi; }, set compactToolbarUi(v) { compactToolbarUi = v; },
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
  get currentDpi() { return currentDpi; },
  get currentFontFamily() { return currentFontFamily; }, set currentFontFamily(v) { currentFontFamily = v; },
  get currentName() { return currentName; }, set currentName(v) { currentName = v; },
  get currentParagraphStyleName() { return currentParagraphStyleName; },
  get currentReviewTimestamp() { return currentReviewTimestamp; },
  get currentRevision() { return currentRevision; }, set currentRevision(v) { currentRevision = v; },
  get currentSourceFormat() { return currentSourceFormat; }, set currentSourceFormat(v) { currentSourceFormat = v; },
  get currentStoryKey() { return currentStoryKey; },
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
  get descendIntoSelectedGroup() { return descendIntoSelectedGroup; },
  get descendSelectedGroup() { return descendSelectedGroup; },
  get disarmFormatPainter() { return disarmFormatPainter; },
  get discardOwnDraft() { return discardOwnDraft; },
  get doc() { return doc; }, set doc(v) { doc = v; },
  get docTitleEl() { return docTitleEl; },
  get documentChrome() { return documentChrome; },
  get documentEpoch() { return documentEpoch; }, set documentEpoch(v) { documentEpoch = v; },
  get documentHasObjects() { return documentHasObjects; },
  get documentHasTrackedChanges() { return documentHasTrackedChanges; },
  get documentIsDirty() { return documentIsDirty; },
  get documentProtection() { return documentProtection; },
  get documentVersion() { return documentVersion; },
  get draftDocKey() { return draftDocKey; }, set draftDocKey(v) { draftDocKey = v; },
  get draftOffers() { return draftOffers; }, set draftOffers(v) { draftOffers = v; },
  get draftsClearBtn() { return draftsClearBtn; },
  get dragging() { return dragging; }, set dragging(v) { dragging = v; },
  get drawRunningBands() { return drawRunningBands; },
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
  get filePageBody() { return filePageBody; },
  get findSession() { return findSession; },
  get finishObjectMove() { return finishObjectMove; },
  get fmtButtons() { return fmtButtons; },
  get focusEditorSurface() { return focusEditorSurface; },
  get focusReviewComment() { return focusReviewComment; },
  get focusReviewRevision() { return focusReviewRevision; },
  get foldView() { return foldView; },
  get followTocEntry() { return followTocEntry; },
  get fontAdvanced() { return fontAdvanced; },
  get fontCache() { return fontCache; },
  get fontFamilyBtn() { return fontFamilyBtn; },
  get fontFamilyLabel() { return fontFamilyLabel; },
  get fontInventory() { return fontInventory; },
  get fontMenu() { return fontMenu; },
  get fontMenuEmpty() { return fontMenuEmpty; },
  get fontMenuInput() { return fontMenuInput; },
  get fontMenuList() { return fontMenuList; },
  get fontSizeSel() { return fontSizeSel; },
  get fontState() { return fontState; }, set fontState(v) { fontState = v; },
  get formatPainter() { return formatPainter; }, set formatPainter(v) { formatPainter = v; },
  get formatPainterBtn() { return formatPainterBtn; },
  get formatToggleCache() { return formatToggleCache; },
  get formattingMarks() { return formattingMarks; },
  get grammarCheckToggle() { return grammarCheckToggle; },
  get growFontBtn() { return growFontBtn; },
  get handOverDraftBeforeOpen() { return handOverDraftBeforeOpen; },
  get handleFile() { return handleFile; },
  get hasRange() { return hasRange; },
  get headerFooterSettings() { return headerFooterSettings; },
  get headerFooterSettingsBtn() { return headerFooterSettingsBtn; },
  get hexToRgb() { return hexToRgb; },
  get hideImePreedit() { return hideImePreedit; },
  get hideLinkChip() { return hideLinkChip; },
  get hidePasteOptions() { return hidePasteOptions; },
  get hideRunningMarker() { return hideRunningMarker; },
  get highlightApplyBtn() { return highlightApplyBtn; },
  get highlightBar() { return highlightBar; },
  get highlightCaret() { return highlightCaret; },
  get highlightMenu() { return highlightMenu; },
  get highlightPopover() { return highlightPopover; },
  get hostSession() { return hostSession; },
  get implicitCaretAt() { return implicitCaretAt; }, set implicitCaretAt(v) { implicitCaretAt = v; },
  get indentDecBtn() { return indentDecBtn; },
  get indentIncBtn() { return indentIncBtn; },
  get indentLeftInput() { return indentLeftInput; },
  get indentRightInput() { return indentRightInput; },
  get indentSpecialByInput() { return indentSpecialByInput; },
  get indentSpecialSel() { return indentSpecialSel; },
  get insertBookmarkBtn() { return insertBookmarkBtn; },
  get insertChartBtn() { return insertChartBtn; },
  get insertChartWithData() { return insertChartWithData; },
  get insertCommandEnabled() { return insertCommandEnabled; },
  get insertCommentBtn() { return insertCommentBtn; },
  get insertDateBtn() { return insertDateBtn; },
  get insertDropCapBtn() { return insertDropCapBtn; },
  get insertEmojiBtn() { return insertEmojiBtn; },
  get insertEvenOddVariantBtn() { return insertEvenOddVariantBtn; },
  get insertFieldAtCaret() { return insertFieldAtCaret; },
  get insertFieldBtn() { return insertFieldBtn; },
  get insertFirstPageVariantBtn() { return insertFirstPageVariantBtn; },
  get insertFooterBtn() { return insertFooterBtn; },
  get insertHeaderBtn() { return insertHeaderBtn; },
  get insertImageFromBlob() { return insertImageFromBlob; },
  get insertImageFromFile() { return insertImageFromFile; },
  get insertLineBreakAtSelection() { return insertLineBreakAtSelection; },
  get insertLinkBtn() { return insertLinkBtn; },
  get insertNote() { return insertNote; },
  get insertPageNumberBtn() { return insertPageNumberBtn; },
  get insertPictureBtn() { return insertPictureBtn; },
  get insertShapeBtn() { return insertShapeBtn; },
  get insertShapeObject() { return insertShapeObject; },
  get insertSymbolBtn() { return insertSymbolBtn; },
  get insertTableBtn() { return insertTableBtn; },
  get insertTableMenu() { return insertTableMenu; },
  get insertTextBoxBtn() { return insertTextBoxBtn; },
  get insertTextBoxObject() { return insertTextBoxObject; },
  get insideGroupSelection() { return insideGroupSelection; },
  get isInteractiveChromeTarget() { return isInteractiveChromeTarget; },
  get keyboardLevelIndex() { return keyboardLevelIndex; }, set keyboardLevelIndex(v) { keyboardLevelIndex = v; },
  get keyboardObjectMove() { return keyboardObjectMove; },
  get labelledObjectMenuButton() { return labelledObjectMenuButton; },
  get lastHighlight() { return lastHighlight; }, set lastHighlight(v) { lastHighlight = v; },
  get lastTextColor() { return lastTextColor; }, set lastTextColor(v) { lastTextColor = v; },
  get lastTypingAt() { return lastTypingAt; }, set lastTypingAt(v) { lastTypingAt = v; },
  get layoutBringForwardBtn() { return layoutBringForwardBtn; },
  get layoutColumnsBtn() { return layoutColumnsBtn; },
  get layoutGroupBtn() { return layoutGroupBtn; },
  get layoutIndentDecBtn() { return layoutIndentDecBtn; },
  get layoutIndentFieldsBtn() { return layoutIndentFieldsBtn; },
  get layoutIndentIncBtn() { return layoutIndentIncBtn; },
  get layoutMarginsBtn() { return layoutMarginsBtn; },
  get layoutOrientationBtn() { return layoutOrientationBtn; },
  get layoutPositionBtn() { return layoutPositionBtn; },
  get layoutRotateBtn() { return layoutRotateBtn; },
  get layoutSendBackwardBtn() { return layoutSendBackwardBtn; },
  get layoutSizeBtn() { return layoutSizeBtn; },
  get layoutSpacingFieldsBtn() { return layoutSpacingFieldsBtn; },
  get layoutUngroupBtn() { return layoutUngroupBtn; },
  get layoutWrapBtn() { return layoutWrapBtn; },
  get leaveSharedDocument() { return leaveSharedDocument; },
  get lineNumbersBtn() { return lineNumbersBtn; },
  get linkAt() { return linkAt; },
  get linkChip() { return linkChip; },
  get linkChipAction() { return linkChipAction; },
  get linkChipEdit() { return linkChipEdit; },
  get linkChipKind() { return linkChipKind; },
  get linkChipRemove() { return linkChipRemove; },
  get linkChipTarget() { return linkChipTarget; },
  get listAdvanced() { return listAdvanced; },
  get loadSharedStartup() { return loadSharedStartup; },
  get markDocumentSaved() { return markDocumentSaved; },
  get materializedPages() { return materializedPages; },
  get measurement() { return measurement; },
  get menuLevels() { return menuLevels; }, set menuLevels(v) { menuLevels = v; },
  get mergeCellsBtn() { return mergeCellsBtn; },
  get navByViewport() { return navByViewport; },
  get navCaret() { return navCaret; },
  get navToPosition() { return navToPosition; },
  get navigateReview() { return navigateReview; },
  get navigateToAnchor() { return navigateToAnchor; },
  get newBlankDocument() { return newBlankDocument; },
  get noteCommands() { return noteCommands; },
  get noteDocumentEdited() { return noteDocumentEdited; },
  get noteDraftDirty() { return noteDraftDirty; },
  get nudgeSelectedObject() { return nudgeSelectedObject; },
  get numberGalleryMenu() { return numberGalleryMenu; },
  get numberedListBtn() { return numberedListBtn; },
  get numberedListMenuBtn() { return numberedListMenuBtn; },
  get objectArrange() { return objectArrange; },
  get objectArrangeBtn() { return objectArrangeBtn; },
  get objectCapabilities() { return objectCapabilities; },
  get objectContextAtEvent() { return objectContextAtEvent; },
  get objectContextMenuHost() { return objectContextMenuHost; },
  get objectCropSession() { return objectCropSession; }, set objectCropSession(v) { objectCropSession = v; },
  get objectEditBlocked() { return objectEditBlocked; },
  get objectInspector() { return objectInspector; },
  get objectMoveDrag() { return objectMoveDrag; }, set objectMoveDrag(v) { objectMoveDrag = v; },
  get objectPositionBtn() { return objectPositionBtn; },
  get objectPresence() { return objectPresence; },
  get objectReference() { return objectReference; },
  get objectResize() { return objectResize; },
  get objectRotate() { return objectRotate; },
  get objectRotateBtn() { return objectRotateBtn; },
  get objectSelection() { return objectSelection; }, set objectSelection(v) { objectSelection = v; },
  get objectTextDrag() { return objectTextDrag; },
  get objectTextIo() { return objectTextIo; },
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
  get pageBandModel() { return pageBandModel; },
  get pageFromClientPoint() { return pageFromClientPoint; },
  get pageFromEvent() { return pageFromEvent; },
  get pageIndexOfWrap() { return pageIndexOfWrap; },
  get pageSetup() { return pageSetup; },
  get pageWindow() { return pageWindow; }, set pageWindow(v) { pageWindow = v; },
  get pages() { return pages; },
  get pdfCommands() { return pdfCommands; }, set pdfCommands(v) { pdfCommands = v; },
  get headerFooterBand() { return headerFooterBand; }, set headerFooterBand(v) { headerFooterBand = v; },
  get pagesEl() { return pagesEl; },
  get pagesPanel() { return pagesPanel; },
  get pagesPanelView() { return pagesPanelView; },
  get paintChecklistMarkers() { return paintChecklistMarkers; },
  get paintFormatFromGesture() { return paintFormatFromGesture; },
  get paintObjectCrop() { return paintObjectCrop; },
  get paintOverlayLayer() { return paintOverlayLayer; },
  get paintPageCanvas() { return paintPageCanvas; },
  get paintPagesInView() { return paintPagesInView; },
  get paintReviewMarkers() { return paintReviewMarkers; },
  get paintSelection() { return paintSelection; },
  get paintSnapGuides() { return paintSnapGuides; },
  get paraBordersMixed() { return paraBordersMixed; },
  get paraControls() { return paraControls; },
  get paraLineSpacing() { return paraLineSpacing; },
  get paraOptsBtn() { return paraOptsBtn; },
  get paraPanelAlign() { return paraPanelAlign; },
  get paraPanelStyle() { return paraPanelStyle; },
  get paraShade() { return paraShade; },
  get paraShadeMixed() { return paraShadeMixed; },
  get paraShadeNone() { return paraShadeNone; },
  get paraSpaceAfter() { return paraSpaceAfter; },
  get paraSpaceBefore() { return paraSpaceBefore; },
  get paragraphAdvanced() { return paragraphAdvanced; },
  get paragraphPropertiesContext() { return paragraphPropertiesContext; },
  get paragraphPropertiesPanel() { return paragraphPropertiesPanel; },
  get paragraphRevisionRects() { return paragraphRevisionRects; },
  get paste() { return paste; },
  get pasteAsText() { return pasteAsText; },
  get pasteOptionsShowing() { return pasteOptionsShowing; },
  get pasteRichRunsJson() { return pasteRichRunsJson; },
  get pasteText() { return pasteText; },
  get pendingFormat() { return pendingFormat; }, set pendingFormat(v) { pendingFormat = v; },
  get pgBreakBefore() { return pgBreakBefore; },
  get pgKeepLines() { return pgKeepLines; },
  get pgKeepNext() { return pgKeepNext; },
  get phoneChrome() { return phoneChrome; },
  get pictureReplace() { return pictureReplace; },
  get place() { return place; },
  get plainTableInfo() { return plainTableInfo; },
  get pointInsideObject() { return pointInsideObject; },
  get pointToTwip() { return pointToTwip; },
  get pointerGesture() { return pointerGesture; }, set pointerGesture(v) { pointerGesture = v; },
  get pointerHover() { return pointerHover; },
  get populateStyles() { return populateStyles; },
  get populateTableStyles() { return populateTableStyles; },
  get positionEditorTextInput() { return positionEditorTextInput; },
  get positionSelToolbar() { return positionSelToolbar; },
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
  get recentHighlights() { return recentHighlights; },
  get recentTextColors() { return recentTextColors; },
  get recordRecent() { return recordRecent; },
  get redlineOf() { return redlineOf; },
  get refBookmarkBtn() { return refBookmarkBtn; },
  get refCaptionBtn() { return refCaptionBtn; },
  get refCrossRefBtn() { return refCrossRefBtn; },
  get refEndnoteBtn() { return refEndnoteBtn; },
  get refFieldBtn() { return refFieldBtn; },
  get refFootnoteBtn() { return refFootnoteBtn; },
  get refGoToHeadingBtn() { return refGoToHeadingBtn; },
  get refTocBtn() { return refTocBtn; },
  get refUpdateCaptionsBtn() { return refUpdateCaptionsBtn; },
  get refUpdateFieldsBtn() { return refUpdateFieldsBtn; },
  get referenceCommands() { return referenceCommands; },
  get referenceObjectMenuRows() { return referenceObjectMenuRows; },
  get reflectChrome() { return reflectChrome; },
  get reflectFontFamily() { return reflectFontFamily; },
  get reflectHighlightSwatch() { return reflectHighlightSwatch; },
  get reflectListGallery() { return reflectListGallery; },
  get reflectReviewBanners() { return reflectReviewBanners; },
  get reflectShapeFormatState() { return reflectShapeFormatState; },
  get reflectShapeSwatches() { return reflectShapeSwatches; },
  get reflectShowingChangesState() { return reflectShowingChangesState; },
  get reflectTextColorSwatch() { return reflectTextColorSwatch; },
  get reflowView() { return reflowView; },
  get refreshDraftOffers() { return refreshDraftOffers; },
  get refusedObjectDrag() { return refusedObjectDrag; },
  get regionCycle() { return regionCycle; },
  get releasePageCanvas() { return releasePageCanvas; },
  get remotePresence() { return remotePresence; },
  get renderAll() { return renderAll; },
  get renderCommands() { return renderCommands; },
  get renderFilePage() { return renderFilePage; },
  get renderStylesGallery() { return renderStylesGallery; },
  get repaintPage() { return repaintPage; },
  get replaceMisspelling() { return replaceMisspelling; },
  get resetDirtyTracking() { return resetDirtyTracking; },
  get resetPointerGesture() { return resetPointerGesture; },
  get restartListBtn() { return restartListBtn; },
  get restoredFromDraft() { return restoredFromDraft; }, set restoredFromDraft(v) { restoredFromDraft = v; },
  get reviewAcceptAll() { return reviewAcceptAll; },
  get reviewAcceptAllBtn() { return reviewAcceptAllBtn; },
  get reviewAcceptBtn() { return reviewAcceptBtn; },
  get reviewAnchorIndex() { return reviewAnchorIndex; }, set reviewAnchorIndex(v) { reviewAnchorIndex = v; },
  get reviewBtn() { return reviewBtn; },
  get reviewBulkActions() { return reviewBulkActions; },
  get reviewCardCache() { return reviewCardCache; },
  get reviewClose() { return reviewClose; },
  get reviewColumnStartMemo() { return reviewColumnStartMemo; }, set reviewColumnStartMemo(v) { reviewColumnStartMemo = v; },
  get reviewCommentActions() { return reviewCommentActions; },
  get reviewCommentBtn() { return reviewCommentBtn; },
  get reviewComposerState() { return reviewComposerState; }, set reviewComposerState(v) { reviewComposerState = v; },
  get reviewContextAt() { return reviewContextAt; },
  get reviewDeleteBtn() { return reviewDeleteBtn; },
  get reviewDeleteConfirmId() { return reviewDeleteConfirmId; }, set reviewDeleteConfirmId(v) { reviewDeleteConfirmId = v; },
  get reviewDisplay() { return reviewDisplay; },
  get reviewFilter() { return reviewFilter; }, set reviewFilter(v) { reviewFilter = v; },
  get reviewFilters() { return reviewFilters; },
  get reviewGrammarCheckBtn() { return reviewGrammarCheckBtn; },
  get reviewInlineCard() { return reviewInlineCard; }, set reviewInlineCard(v) { reviewInlineCard = v; },
  get reviewLayout() { return reviewLayout; }, set reviewLayout(v) { reviewLayout = v; },
  get reviewMarginCommentBtn() { return reviewMarginCommentBtn; },
  get reviewMarginFrame() { return reviewMarginFrame; }, set reviewMarginFrame(v) { reviewMarginFrame = v; },
  get reviewMode() { return reviewMode; }, set reviewMode(v) { reviewMode = v; },
  get reviewModeButtons() { return reviewModeButtons; },
  get reviewMoveEndButton() { return reviewMoveEndButton; },
  get reviewNext() { return reviewNext; },
  get reviewNextBtn() { return reviewNextBtn; },
  get reviewPanelBtn() { return reviewPanelBtn; },
  get reviewPrevBtn() { return reviewPrevBtn; },
  get reviewPrevious() { return reviewPrevious; },
  get reviewProofLanguagesBtn() { return reviewProofLanguagesBtn; },
  get reviewRangeClientRect() { return reviewRangeClientRect; },
  get reviewRejectAll() { return reviewRejectAll; },
  get reviewRejectAllBtn() { return reviewRejectAllBtn; },
  get reviewRejectBtn() { return reviewRejectBtn; },
  get reviewResolveBtn() { return reviewResolveBtn; },
  get reviewSheetMode() { return reviewSheetMode; },
  get reviewShowChangesBtn() { return reviewShowChangesBtn; },
  get reviewSidebar() { return reviewSidebar; },
  get reviewSidebarBody() { return reviewSidebarBody; },
  get reviewSidebarHeader() { return reviewSidebarHeader; },
  get reviewSidebarPreference() { return reviewSidebarPreference; }, set reviewSidebarPreference(v) { reviewSidebarPreference = v; },
  get reviewSmartQuotesBtn() { return reviewSmartQuotesBtn; },
  get reviewSpellCheckBtn() { return reviewSpellCheckBtn; },
  get reviewTrackBtn() { return reviewTrackBtn; },
  get reviewTracking() { return reviewTracking; },
  get reviewWindowFrame() { return reviewWindowFrame; }, set reviewWindowFrame(v) { reviewWindowFrame = v; },
  get revisionRange() { return revisionRange; },
  get revisionUnreadable() { return revisionUnreadable; }, set revisionUnreadable(v) { revisionUnreadable = v; },
  get ribbonPanels() { return ribbonPanels; },
  get ribbonSurfaceEnabled() { return ribbonSurfaceEnabled; },
  get ribbonSurfaceReason() { return ribbonSurfaceReason; },
  get ribbonViewCollapsed() { return ribbonViewCollapsed; }, set ribbonViewCollapsed(v) { ribbonViewCollapsed = v; },
  get rulerView() { return rulerView; },
  get runControls() { return runControls; },
  get runEdit() { return runEdit; },
  get runNodeEdit() { return runNodeEdit; },
  get runToolbarEdit() { return runToolbarEdit; },
  get runningBandAt() { return runningBandAt; },
  get runningEditBand() { return runningEditBand; }, set runningEditBand(v) { runningEditBand = v; },
  get runningEditPage() { return runningEditPage; }, set runningEditPage(v) { runningEditPage = v; },
  get runningEditPageOrView() { return runningEditPageOrView; },
  get saveBtn() { return saveBtn; },
  get saveDocument() { return saveDocument; },
  get saveFormatEl() { return saveFormatEl; },
  get saveSettings() { return saveSettings; },
  get scaleOf() { return scaleOf; },
  get scheduleChromeRefresh() { return scheduleChromeRefresh; }, get wordCount() { return wordCount; },
  get scheduleReviewInlineCardHide() { return scheduleReviewInlineCardHide; },
  get scheduleReviewMarginRender() { return scheduleReviewMarginRender; },
  get scrollCaretIntoView() { return scrollCaretIntoView; },
  get scrollFindMatchIntoView() { return scrollFindMatchIntoView; },
  get scrollModelRectIntoView() { return scrollModelRectIntoView; },
  get scrollOverlayIntoView() { return scrollOverlayIntoView; },
  get scrollReviewSelectionIntoView() { return scrollReviewSelectionIntoView; },
  get searchTrigger() { return searchTrigger; },
  get selComment() { return selComment; },
  get selEndpoints() { return selEndpoints; },
  get selHighlightBar() { return selHighlightBar; },
  get selHighlightBtn() { return selHighlightBtn; },
  get selHighlightMenu() { return selHighlightMenu; },
  get selTextColorBar() { return selTextColorBar; },
  get selTextColorBtn() { return selTextColorBtn; },
  get selTextColorMenu() { return selTextColorMenu; },
  get selectAll() { return selectAll; },
  get selectObject() { return selectObject; },
  get selectRibbonTab() { return selectRibbonTab; },
  get selectTableContext() { return selectTableContext; },
  get selectedObjectContext() { return selectedObjectContext; },
  get selectedShapeFormat() { return selectedShapeFormat; },
  get selection() { return selection; }, set selection(v) { selection = v; },
  get selectionAutoScrollFrame() { return selectionAutoScrollFrame; }, set selectionAutoScrollFrame(v) { selectionAutoScrollFrame = v; },
  get selectionContainsClientPoint() { return selectionContainsClientPoint; },
  get selectionModelRect() { return selectionModelRect; },
  get setDocumentState() { return setDocumentState; },
  get setDraftStatus() { return setDraftStatus; },
  get setGrammarCheckEnabled() { return setGrammarCheckEnabled; },
  get setObjectWrap() { return setObjectWrap; },
  get setReviewMode() { return setReviewMode; },
  get setRibbonCollapsed() { return setRibbonCollapsed; },
  get setRunningContext() { return setRunningContext; },
  get setShowingChanges() { return setShowingChanges; },
  get setSpellCheckEnabled() { return setSpellCheckEnabled; },
  get setStatus() { return setStatus; },
  get setZoom() { return setZoom; },
  get setZoomMode() { return setZoomMode; },
  get settings() { return settings; }, set settings(v) { settings = v; },
  get settingsBtn() { return settingsBtn; },
  get shapeDrawMode() { return shapeDrawMode; },
  get shapeFillBtn() { return shapeFillBtn; },
  get shapeOutlineBtn() { return shapeOutlineBtn; },
  get sharedRestoreRefusal() { return sharedRestoreRefusal; }, get sharedHistory() { return sharedHistory; },
  get showChange() { return showChange; },
  get showContextMenu() { return showContextMenu; },
  get showDraftRecovery() { return showDraftRecovery; },
  get showImePreedit() { return showImePreedit; },
  get showReviewInlineCard() { return showReviewInlineCard; },
  get showVersionPreview() { return showVersionPreview; },
  get showingChanges() { return showingChanges; }, set showingChanges(v) { showingChanges = v; },
  get shrinkFontBtn() { return shrinkFontBtn; },
  get snapContextFor() { return snapContextFor; },
  get spacingMenuControl() { return spacingMenuControl; },
  get spellCheckToggle() { return spellCheckToggle; },
  get spellChecker() { return spellChecker; },
  get splitCellBtn() { return splitCellBtn; },
  get splitCellCancel() { return splitCellCancel; },
  get splitCellClose() { return splitCellClose; },
  get splitCellColumns() { return splitCellColumns; },
  get splitCellConfirm() { return splitCellConfirm; },
  get splitCellDialog() { return splitCellDialog; },
  get splitCellRows() { return splitCellRows; },
  get startCollaboration() { return startCollaboration; },
  get startDrafts() { return startDrafts; },
  get startObjectMove() { return startObjectMove; },
  get statusChannel() { return statusChannel; },
  get statusEl() { return statusEl; },
  get stepFontSize() { return stepFontSize; },
  get stepZoom() { return stepZoom; },
  get styleAdvanced() { return styleAdvanced; },
  get stylesMenu() { return stylesMenu; },
  get stylesMenuInput() { return stylesMenuInput; },
  get stylesPopover() { return stylesPopover; },
  get stylesTrigger() { return stylesTrigger; },
  get subBtn() { return subBtn; },
  get suggestAcrossParagraphs() { return suggestAcrossParagraphs; },
  get suggestingBannerEdit() { return suggestingBannerEdit; },
  get superBtn() { return superBtn; },
  get syncActiveReviewCommentToCaret() { return syncActiveReviewCommentToCaret; },
  get syncRibbonTabStops() { return syncRibbonTabStops; },
  get syncSelectionToCellRange() { return syncSelectionToCellRange; },
  get syncStylesGalleryActive() { return syncStylesGalleryActive; },
  get tabStopsDialog() { return tabStopsDialog; },
  get tableAlign() { return tableAlign; },
  get tableBtn() { return tableBtn; },
  get tableCaption() { return tableCaption; },
  get tableCellMargin() { return tableCellMargin; },
  get tableCellSpacing() { return tableCellSpacing; },
  get tableChrome() { return tableChrome; },
  get tableColumnWidth() { return tableColumnWidth; },
  get tableColumnWidthNote() { return tableColumnWidthNote; },
  get tableDescription() { return tableDescription; },
  get tableFixedLayout() { return tableFixedLayout; },
  get tableFmtMenu() { return tableFmtMenu; },
  get tableFormula() { return tableFormula; },
  get tableFormulaApply() { return tableFormulaApply; },
  get tableGutter() { return tableGutter; },
  get tableHeaderRow() { return tableHeaderRow; },
  get tableIndent() { return tableIndent; },
  get tablePopover() { return tablePopover; },
  get tablePropertiesBtn() { return tablePropertiesBtn; },
  get tablePropertiesContext() { return tablePropertiesContext; },
  get tablePropertiesPanel() { return tablePropertiesPanel; },
  get tableRange() { return tableRange; },
  get tableRibbon() { return tableRibbon; },
  get tableRowHeight() { return tableRowHeight; },
  get tableRowHeightRule() { return tableRowHeightRule; },
  get tableStyleBtn() { return tableStyleBtn; },
  get tableStyleMenu() { return tableStyleMenu; },
  get tableStylePopover() { return tableStylePopover; }, set tableStylePopover(v) { tableStylePopover = v; },
  get tableToolCommands() { return tableToolCommands; },
  get tableWidth() { return tableWidth; },
  get takeDraftSnapshot() { return takeDraftSnapshot; },
  get textColorApplyBtn() { return textColorApplyBtn; },
  get textColorBar() { return textColorBar; },
  get textColorCaret() { return textColorCaret; },
  get textColorInput() { return textColorInput; },
  get textColorMenu() { return textColorMenu; },
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
  get underlineColorInput() { return underlineColorInput; },
  get underlineMenu() { return underlineMenu; },
  get underlineMenuBtn() { return underlineMenuBtn; },
  get updateImePreedit() { return updateImePreedit; },
  get updateObjectContextBar() { return updateObjectContextBar; },
  get updateObjectMove() { return updateObjectMove; },
  get updateObjectSelectionState() { return updateObjectSelectionState; },
  get updatePageNumber() { return updatePageNumber; },
  get updatePageWindow() { return updatePageWindow; },
  get updateReviewControls() { return updateReviewControls; },
  get updateRunningMarker() { return updateRunningMarker; },
  get updateStats() { return updateStats; },
  get updateStyleFromSelection() { return updateStyleFromSelection; },
  get updateToolbar() { return updateToolbar; },
  get versionHistory() { return versionHistory; },
  get versionPreviewHome() { return versionPreviewHome; }, set versionPreviewHome(v) { versionPreviewHome = v; },
  get verticalGoal() { return verticalGoal; },
  get viewZoom() { return viewZoom; },
  get viewingBannerEdit() { return viewingBannerEdit; },
  get viewportEl() { return viewportEl; },
  get watermarkBtn() { return watermarkBtn; },
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

let ribbonViewCollapsed = false;
const {
  buildStylesGallery, confirmDiscardIfEdited, confirmModal, copyBtn, createStyleFromSelection,
  currentParagraphStyleName, cutBtn, pasteBtn, reflectParagraphStyle, renderStylesGallery,
  replaceBtn, ribbonBodyEl, ribbonOverflowBtn, ribbonOverflowMenu, scheduleRibbonOverflow,
  setRibbonCollapsed, stylesMenu, stylesMenuInput, stylesTrigger, syncStylesGalleryActive,
  updateRibbonOverflow, updateStyleFromSelection,
} = createRibbon(ed);
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
  reviewDisplay.reflect(); // the Display-for-review picker derives its mode from this
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
// Per-paint memo for `paragraphColumnStart`: `pageSetupSections` walks every
// paragraph, and a heavily reviewed document can carry a formatting change on
// most of them. Reset at the start of each marker paint.
let reviewColumnStartMemo = new Map();
const {
  paragraphRevisionRects, reviewRangeClientRect, revisionRange, scheduleReviewMarginRender,
  setReviewMode, updateReviewControls,
} = createReviewMargin(ed);
const reviewDisplay = createReviewDisplay(ed); // Review ▸ Display for review (RM-31)
const noteCommands = createNoteCommands(ed); // Go to, settings and convert notes (RM-24)
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
  start: startCollaboration, commands: collabFeatureCommands, loadStartup: loadSharedStartup, beforeReplace: leaveSharedDocument, restoreRefusal: sharedRestoreRefusal, sharedHistory } = createCollaborationShell({
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
/** The PDF commands (`pdf_commands.mjs`), set by the file-open module. */
let pdfCommands = null;
/** The open band's commands (`editor_header_footer.mjs`), set by that module. */
let headerFooterBand = null;
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
let fontState = { provisionedFallbackKeys: new Set(), inFlightFallbackKeys: new Set(), retired: false };
const { ensureGlyphCoverage, provisionFonts } = createFonts(ed);
const {
  materializedPages, pageIndexOfWrap, paintPageCanvas, paintPagesInView, releasePageCanvas,
  updatePageWindow, virtualPageRect,
} = createPaint(ed);
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
const {
  anchorAt, clearOverlays, currentStoryKey, paintReviewMarkers, pointInsideObject, pointToTwip,
  syncActiveReviewCommentToCaret, toggleFormCheckboxAt,
} = createSelection(ed);
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
const {
  drawSelection, paintOverlayLayer, proofing, replaceMisspelling, setGrammarCheckEnabled,
  setSpellCheckEnabled, spellChecker,
} = createProofing(ed);
const {
  OBJECT_CAPABILITY_KEYS, arrangeState, cancelCrop, chartSurface, climbOutOfGroup, commitCrop,
  cropKeyIo, descendIntoSelectedGroup, descendSelectedGroup, documentHasObjects, enterCropMode,
  insertChartWithData, keyboardObjectMove, objectArrange, objectCapabilities, objectInspector,
  objectPresence, objectReference, objectResize, objectRotate, objectTextDrag, objectTextIo,
  paintObjectCrop, reflectShapeFormatState, refusedObjectDrag, selectObject, selectedObjectContext,
  shapeDrawMode, toggleObjectInspector, traverseObjects, updateObjectContextBar,
  updateObjectSelectionState,
} = createObjects(ed);
const {
  blockedFromPageSetup, deleteSelectedObject, drawRunningBands, editRunningContent, exitRunningEdit,
  hideRunningMarker, insertNote, pageInView, runningBandAt, runningEditPageOrView, setObjectWrap,
  setRunningContext, updateRunningMarker,
} = createHeaderFooter(ed);
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
const {
  cancelObjectMove, clearSnapGuides, enterObjectEditMode, finishObjectMove, insideGroupSelection,
  nudgeSelectedObject, objectEditBlocked, openAltTextDialog, paintSnapGuides, snapContextFor,
  startObjectMove, updateObjectMove,
} = createObjectMoves(ed);
const {
  hideImePreedit, navigateToAnchor, paintChecklistMarkers, paintSelection, showImePreedit,
  updateImePreedit,
} = createOverlayPaint(ed);
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
  el.style.height = `${Math.max(h * sy, kind === "caret" ? 2 : 0)}px`; // a turned-cell caret is a bar across the line
  page.overlay.appendChild(el);
  return el;
}
const {
  copySelection, cycleReviewMode, followTocEntry, hideLinkChip, linkAt, pageFromEvent, pointerHover,
  syncSelectionToCellRange, tableChrome, tableGutter, tocEntryTargetAt, touchSelection,
} = createPointer(ed);
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

const {
  BREAK_IO, INSERT_SURFACE, LAYOUT_SURFACE, REFERENCE_SURFACE, REVIEW_SURFACE, insertCommandEnabled,
  referenceObjectMenuRows, reviewCommentActions, ribbonSurfaceEnabled, ribbonSurfaceReason,
} = createCommandSurfaces(ed);
const {
  repaintPage, scrollCaretIntoView, scrollFindMatchIntoView, scrollModelRectIntoView,
  scrollOverlayIntoView, scrollReviewSelectionIntoView, selectionModelRect,
} = createScroll(ed);
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
  formattingMarks.reflect(); wordCount.reflect(); languageSelection?.reflect(); // HF-051: "x of y words"; RM-42: the text language
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
const {
  applyUnderlineStyle, highlightPopover, recentHighlights, recentTextColors, recordRecent,
  reflectHighlightSwatch, reflectTextColorSwatch, textColorPopover,
} = createFontColor(ed);
let lastTextColor = "#000000";
let lastHighlight = "yellow";
const {
  SHAPE_MENU_COLORS, labelledObjectMenuButton, objectArrangeBtn, objectPositionBtn, objectRotateBtn,
  reflectShapeSwatches, selectedShapeFormat, shapeFillBtn, shapeOutlineBtn,
} = createShapeChrome(ed);
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
const { applyChangeCase, fontInventory, reflectListGallery, stepFontSize } = createFontMenu(ed);
const { applyListFormatCommand, openShapeGallery, spacingMenuControl } = createShapeGallery(ed);
const {
  applyHighlight, applyTextColor, reflectParagraphProperties, toggleParagraphProperties,
} = createParagraphInspector(ed);
const { tablePopover } = createTableFormat(ed);
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
const {
  accessChrome, comparePanel, documentProtection, formattingMarks, measurement, redlineOf,
  showChange, wordCount,
} = createDocTools(ed);
const {
  reflectTableProperties, toggleSplitCellDialog, toggleTableProperties,
} = createTableProps(ed);
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
const { buildCommands, closeAppMenu, renderFilePage } = createFilePage(ed);
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
const { addWordToDictionary, autoCorrect, autoCorrectDialog, languageSelection } = createTextInput(ed);
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
const { headerFooterSettings, pageSetup, propertiesUi } = createDocumentSettings(ed);
fileEl.disabled = true;
if (openBtn) openBtn.disabled = true;
// No document is open yet, so every document-scoped control starts disabled.
// (Not "no selection yet": once a document opens it always has an insertion
// point — see `openBytes` — so `selection` tracks the document, not the click.)
updateToolbar();
boot();

createChromeModes(ed);
// ---- The host contract: one session, two transports (`docs/126` phase 2) ----
// `window.opendoc` is the in-process transport, and `attachHostBridge` puts THAT SAME OBJECT
// behind `postMessage`, so the two can never answer differently. Commands, events, refusals
// and gating come from `host_contract.mjs`; the registry is the PALETTE's, so the API reaches
// what a person can, and the gate runs before dispatch. `ai` is `editor.ai` (`docs/181` §4.4).
hostSession = createHostSession({
  capabilities: HOST_CAPS,
  registry: () => editorCommands({ surface: "palette" }),
  withheldMessage: () => t("capability.notGranted"),
  revision: () => (revisionUnreadable ? null : currentRevision),
  dirty: () => documentIsDirty(),
  selection: () => (selection ? { anchor: { ...selection.anchor }, focus: { ...selection.focus }, hasRange: hasRange() } : null),
  document: () => ({ name: currentName, format: currentSourceFormat }),
  ai: createAssistant(ed, { onDecision: (detail) => hostSession?.noteAiDecision(detail) }),
});
window.opendoc = hostSession;
attachHostBridge({ session: hostSession, view: window });
