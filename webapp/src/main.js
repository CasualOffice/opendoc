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
import {
  NAMED_WEB_FONT_FACES,
  SCRIPT_FALLBACK_FONTS,
  fallbackKeysFor,
  fetchSharedFontBytes,
  packFontBytes,
} from "./web_fonts.mjs";
import { breakSurfaceRows } from "./break_commands.mjs";
import { editRefusalMessage, mutationBlockedMessage, readRevision } from "./edit_errors.mjs";
import { renderAccessibilityMirror } from "./a11y_mirror.mjs";
import { createPagesPanel } from "./pages_panel.mjs";
import { objectMenuRows } from "./reference_commands.mjs";
import {
  reflectReviewSurface,
  ribbonSurfaceEnabled as surfaceEnabled,
  ribbonSurfaceReason as surfaceReason,
} from "./ribbon_surface.mjs";
import { createHeaderFooterSettings } from "./header_footer_settings.mjs";
import { createPageSetup } from "./page_setup.mjs";
import { createStyleAdvanced } from "./style_advanced.mjs";
import { createListAdvanced } from "./list_advanced.mjs";
import { RECOMMENDED_STYLES, caretContexts, offeredStyleNames, previewPx, styleMenuGroups, styleSlug } from "./style_picker.mjs";
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
  buildPageBand,
  docToScroll,
  pageBandPitch,
  scrollToDoc,
} from "./page_scroll.mjs";
import { keyboardPlatform } from "./keyboard.mjs";
import { chordTitle } from "./keymap.mjs";
import { modalIsOpen, registerModal, setModalHooks } from "./modal.mjs";
import { objectBarPosition } from "./object_guides.mjs";
// Own lines (anti-conflict): the two direct-manipulation geometry gestures.
import { arrangeSurfaceRows } from "./object_arrange_commands.mjs";
import { positionPayload, readGroupability, rotationPlan } from "./object_arrange.mjs";
import { t } from "./i18n.mjs";
import { authoredTitle, paintDocumentState } from "./localize.mjs";
import { countLabels, pageIndicator, readerPosition } from "./status_counts.mjs";
import { localizeShortcutGlyphs, localizeShortcutText } from "./shortcut_labels.mjs";
import { installRibbonTooltips } from "./ribbon_tooltip.mjs";
import { rovingIndex, tabStopIndex } from "./ribbon_nav.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { onButton } from "./popover_manager.mjs";
import { closePopover, reflectOpenPopovers, registerPopover } from "./popover_manager.mjs";
import { createViewZoom, nextZoomStep } from "./view_zoom.mjs";
import { createPhoneChrome } from "./phone_chrome.mjs";
import { DEFAULT_SETTINGS } from "./settings_defaults.mjs";
import { editingModeFor, hostCapabilities, hostChrome, hostConfig, reflectReviewModeAccess } from "./capabilities.mjs";
// Own line (anti-conflict): the chart's tab, panel, data dialog and commands.
import { createCollaborationShell } from "./collab_shell.mjs";
import { groupsToOverflow } from "./ribbon_overflow.mjs";
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
import { makeMenuHeading } from "./picker_chrome.mjs";
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
import { loadPrefObject, readPref, savePrefObject, writePref } from "./prefs.mjs";
import { BRAND } from "./brand.mjs";
import { applyRegions } from "./chrome_regions.mjs";
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
  get autoCorrect() { return autoCorrect; },
  get autosaveToggle() { return autosaveToggle; },
  get backgroundMeasure() { return backgroundMeasure; }, set backgroundMeasure(v) { backgroundMeasure = v; },
  get bandEl() { return bandEl; },
  get bandOffset() { return bandOffset; }, set bandOffset(v) { bandOffset = v; },
  get bandTopInScroller() { return bandTopInScroller; },
  get blockMutationInViewing() { return blockMutationInViewing; },
  get blockUntrackedInSuggesting() { return blockUntrackedInSuggesting; },
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
  get currentDpi() { return currentDpi; },
  get currentFontFamily() { return currentFontFamily; }, set currentFontFamily(v) { currentFontFamily = v; },
  get currentName() { return currentName; }, set currentName(v) { currentName = v; },
  get currentParagraphStyleName() { return currentParagraphStyleName; },
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
  get foldView() { return foldView; },
  get followTocEntry() { return followTocEntry; },
  get fontAdvanced() { return fontAdvanced; },
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
  get formattingMarks() { return formattingMarks; },
  get grammarCheckToggle() { return grammarCheckToggle; },
  get growFontBtn() { return growFontBtn; },
  get handOverDraftBeforeOpen() { return handOverDraftBeforeOpen; },
  get handleFile() { return handleFile; },
  get hasRange() { return hasRange; },
  get headerFooterSettings() { return headerFooterSettings; },
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
  get insertChartWithData() { return insertChartWithData; },
  get insertCommandEnabled() { return insertCommandEnabled; },
  get insertFieldAtCaret() { return insertFieldAtCaret; },
  get insertImageFromBlob() { return insertImageFromBlob; },
  get insertImageFromFile() { return insertImageFromFile; },
  get insertLineBreakAtSelection() { return insertLineBreakAtSelection; },
  get insertNote() { return insertNote; },
  get insertShapeBtn() { return insertShapeBtn; },
  get insertShapeObject() { return insertShapeObject; },
  get insertTableBtn() { return insertTableBtn; },
  get insertTableMenu() { return insertTableMenu; },
  get insertTextBoxObject() { return insertTextBoxObject; },
  get insideGroupSelection() { return insideGroupSelection; },
  get isInteractiveChromeTarget() { return isInteractiveChromeTarget; },
  get keyboardLevelIndex() { return keyboardLevelIndex; }, set keyboardLevelIndex(v) { keyboardLevelIndex = v; },
  get keyboardObjectMove() { return keyboardObjectMove; },
  get labelledObjectMenuButton() { return labelledObjectMenuButton; },
  get lastHighlight() { return lastHighlight; }, set lastHighlight(v) { lastHighlight = v; },
  get lastTextColor() { return lastTextColor; }, set lastTextColor(v) { lastTextColor = v; },
  get lastTypingAt() { return lastTypingAt; }, set lastTypingAt(v) { lastTypingAt = v; },
  get leaveSharedDocument() { return leaveSharedDocument; },
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
  get pagesEl() { return pagesEl; },
  get pagesPanel() { return pagesPanel; },
  get pagesPanelView() { return pagesPanelView; },
  get paintChecklistMarkers() { return paintChecklistMarkers; },
  get paintFormatFromGesture() { return paintFormatFromGesture; },
  get paintObjectCrop() { return paintObjectCrop; },
  get paintOverlayLayer() { return paintOverlayLayer; },
  get paintPagesInView() { return paintPagesInView; },
  get paintReviewMarkers() { return paintReviewMarkers; },
  get paintSelection() { return paintSelection; },
  get paintSnapGuides() { return paintSnapGuides; },
  get paraBordersMixed() { return paraBordersMixed; },
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
  get referenceCommands() { return referenceCommands; },
  get referenceObjectMenuRows() { return referenceObjectMenuRows; },
  get reflectFontFamily() { return reflectFontFamily; },
  get reflectHighlightSwatch() { return reflectHighlightSwatch; },
  get reflectListGallery() { return reflectListGallery; },
  get reflectShapeFormatState() { return reflectShapeFormatState; },
  get reflectShapeSwatches() { return reflectShapeSwatches; },
  get reflectShowingChangesState() { return reflectShowingChangesState; },
  get reflectTextColorSwatch() { return reflectTextColorSwatch; },
  get reflowView() { return reflowView; },
  get refreshDraftOffers() { return refreshDraftOffers; },
  get refusedObjectDrag() { return refusedObjectDrag; },
  get regionCycle() { return regionCycle; },
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
  get reviewAnchorIndex() { return reviewAnchorIndex; },
  get reviewBtn() { return reviewBtn; },
  get reviewCardCache() { return reviewCardCache; },
  get reviewClose() { return reviewClose; },
  get reviewColumnStartMemo() { return reviewColumnStartMemo; }, set reviewColumnStartMemo(v) { reviewColumnStartMemo = v; },
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
  get runningBandAt() { return runningBandAt; },
  get runningEditBand() { return runningEditBand; }, set runningEditBand(v) { runningEditBand = v; },
  get runningEditPage() { return runningEditPage; }, set runningEditPage(v) { runningEditPage = v; },
  get saveBtn() { return saveBtn; },
  get saveDocument() { return saveDocument; },
  get saveFormatEl() { return saveFormatEl; },
  get saveSettings() { return saveSettings; },
  get scaleOf() { return scaleOf; },
  get scheduleChromeRefresh() { return scheduleChromeRefresh; },
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
  get sharedRestoreRefusal() { return sharedRestoreRefusal; },
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
  get stylesTrigger() { return stylesTrigger; },
  get subBtn() { return subBtn; },
  get suggestAcrossParagraphs() { return suggestAcrossParagraphs; },
  get suggestingBannerEdit() { return suggestingBannerEdit; },
  get superBtn() { return superBtn; },
  get syncActiveReviewCommentToCaret() { return syncActiveReviewCommentToCaret; },
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
  el.style.height = `${h * sy}px`;
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
  showChange,
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
