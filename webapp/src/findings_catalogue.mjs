// What each compatibility finding IS, in words a reader shares (`109` FID-AT-05).
//
// The engine names a finding by the vocabulary it found it in: an element's
// local name (`formProt`), an element and attribute (`cNvPr/@name`), a package
// part (`customXml/item1.xml`) or a class id the engine minted
// (`docx.rsid`). That id is STABLE — support, the corpus harness and every test
// match on it — so it stays on the row, shown secondary. What a reader is shown
// first is what the thing is, in the words Word's own interface uses where it
// has one ("Lock aspect ratio", "Decimal symbol used in calculations").
//
// Three things are decided here and nothing else:
//
//   1. the words for a known feature (`FEATURE_CATALOGUE`);
//   2. whether it is WORD'S OWN BOOKKEEPING — a record Word keeps about the file
//      rather than content anybody would miss (`bookkeeping: true`), which the
//      dialog lists in a collapsed sub-group and keeps out of the headline
//      count. It is a DECLARED list, entry by entry, and
//      `findings_catalogue.test.mjs` pins it: a classification nobody wrote down
//      is a classification nobody can review;
//   3. where a finding is, in words (`partArea`), and what to say about an id
//      nobody has catalogued — the vocabulary is open-ended, so the FALLBACK is
//      held to the same standard as an entry: it says which part and what kind
//      of thing, and never prints a bare XML name as if it were a sentence.
//
// What is NOT decided here is what happened to a finding. Lost, kept or
// approximated is the engine's `retentionOutcome`/`modelOutcome`, folded by
// `compat_findings.mjs`'s `findingKind`, and no entry below says "kept" or
// "lost" — so a word here can never contradict the group it is listed under,
// whichever outcome the engine reports for that id after an edited save.
//
// Pure: no DOM and no `t()`. It returns catalogue KEYS, so it is testable in
// node and the caller translates at render time, in the active locale.
//
// Complexity: a lookup is O(1) for an exact or numbered id and O(P) in the
// number of prefix entries (eight) otherwise; a report is aggregated per
// feature by the engine, so a render describes tens of entries, not one per
// occurrence.

/** A record Word keeps about the file, not content a reader would miss. */
const BOOKKEEPING = true;
/** Content, formatting or a setting with an effect — counted in the headline. */
const CONTENT = false;

// `stylesKey`, when given, is the words for the same id found IN THE STYLES
// part: a `w:tblCellMar` in `styles.xml` is a table style's margins, while the
// same element in the body is one table's own (`109` HF-406). The element name
// cannot tell the two apart; the part it was found in can.
const entry = (key, bookkeeping, stylesKey = null) => Object.freeze({ key, bookkeeping, stylesKey });

/**
 * Every catalogued feature id, and the words for it.
 *
 * Three id shapes, matched in this order by `catalogueEntry`:
 *   - exact: the id as the engine reports it;
 *   - numbered: a part whose file name ends in a number before its extension,
 *     written with `N` in place of the number (`customXml/itemN.xml` answers
 *     `customXml/item1.xml` and `customXml/item12.xml`);
 *   - prefix: an id ending in `*` answers every id that starts with the rest
 *     (`word/embeddings/*`), the longest prefix winning.
 *
 * Where several ids share one key it is because they are one thing to a reader:
 * `p/@paraId` and `tr/@paraId` are both identifiers Word assigns.
 */
export const FEATURE_CATALOGUE = Object.freeze({
  // ---- Word's own bookkeeping ------------------------------------------------
  // Revision-save ids: one class entry per document, whatever its count.
  "docx.rsid": entry("findings.feature.rsid", BOOKKEEPING),
  "docId": entry("findings.feature.docId", BOOKKEEPING),
  "docProps/thumbnail.*": entry("findings.feature.thumbnail", BOOKKEEPING),
  "savePreviewPicture": entry("findings.feature.savePreviewPicture", BOOKKEEPING),
  "word/webSettings.xml": entry("findings.feature.webSettings", BOOKKEEPING),
  // Word 2010's compatibility copy of `styles.xml`; Word 2013 and later stopped
  // writing it, and `styles.xml` is the one this editor reads.
  "word/stylesWithEffects.xml": entry("findings.feature.stylesWithEffects", BOOKKEEPING),
  "HyperlinksChanged": entry("findings.feature.hyperlinksChanged", BOOKKEEPING),
  // After an edited save the two derived parts are left behind and named
  // (`105` FID-R-05): they describe the document as it was opened.
  "docx.export.stale.thumbnail": entry("findings.feature.staleThumbnail", BOOKKEEPING),
  "docx.export.stale.styles_with_effects": entry("findings.feature.staleStylesWithEffects", BOOKKEEPING),
  // `docProps/app.xml`'s page, word and character counts, left out of an edited
  // save that still carries the opened file's values (`109` FID-AT-04).
  "docx.export.stale.statistics": entry("findings.feature.staleStatistics", BOOKKEEPING),
  // Word 2013+'s per-paragraph and per-row identities (`w14:paraId`,
  // `w14:textId`) — the `rsid` family's sibling, one per paragraph, and the
  // reason a modern document would otherwise open with hundreds of findings.
  "p/@paraId": entry("findings.feature.paraId", BOOKKEEPING),
  "p/@textId": entry("findings.feature.textId", BOOKKEEPING),
  "tr/@paraId": entry("findings.feature.rowId", BOOKKEEPING),
  "tr/@textId": entry("findings.feature.textId", BOOKKEEPING),

  // ---- Drawings and pictures -------------------------------------------------
  "cNvPr/@name": entry("findings.feature.objectName", CONTENT),
  "docPr/@name": entry("findings.feature.objectName", CONTENT),
  "graphicFrameLocks": entry("findings.feature.objectLocks", CONTENT),
  "picLocks": entry("findings.feature.pictureLocks", CONTENT),
  "drawing": entry("findings.feature.drawing", CONTENT),
  "shape": entry("findings.feature.legacyShape", CONTENT),
  "shapetype": entry("findings.feature.legacyShapeType", CONTENT),
  "textpath": entry("findings.feature.legacyWordArt", CONTENT),
  "path": entry("findings.feature.legacyShapePath", CONTENT),
  "fill": entry("findings.feature.shapeFill", CONTENT),
  // The rest of the legacy VML vocabulary a `w:pict` carries (`109` HF-394).
  // Each is a local name, which is how the engine reports it; where a name is
  // ALSO a WordprocessingML or package name the words cover both readings
  // (`image` is a picture relationship too, `lock` a content control's lock,
  // `shadow` a run's text shadow), so a word can never misname what it lists.
  "textbox": entry("findings.feature.legacyTextBox", CONTENT),
  "rect": entry("findings.feature.legacyRect", CONTENT),
  "roundrect": entry("findings.feature.legacyRoundRect", CONTENT),
  "oval": entry("findings.feature.legacyOval", CONTENT),
  "line": entry("findings.feature.legacyLine", CONTENT),
  "polyline": entry("findings.feature.legacyPolyline", CONTENT),
  "curve": entry("findings.feature.legacyCurve", CONTENT),
  "arc": entry("findings.feature.legacyArc", CONTENT),
  "image": entry("findings.feature.picture", CONTENT),
  "imagedata": entry("findings.feature.legacyPicture", CONTENT),
  "group": entry("findings.feature.legacyGroup", CONTENT),
  "stroke": entry("findings.feature.legacyStroke", CONTENT),
  "wrap": entry("findings.feature.legacyWrap", CONTENT),
  "shadow": entry("findings.feature.legacyShadow", CONTENT),
  "extrusion": entry("findings.feature.legacyEffect3d", CONTENT),
  "skew": entry("findings.feature.legacySkew", CONTENT),
  "lock": entry("findings.feature.editingLock", CONTENT),
  "callout": entry("findings.feature.legacyCallout", CONTENT),
  "formulas": entry("findings.feature.legacyShapeFormulas", CONTENT),
  "f": entry("findings.feature.legacyShapeFormulas", CONTENT),
  "handles": entry("findings.feature.legacyShapeHandles", CONTENT),
  "h": entry("findings.feature.legacyShapeHandles", CONTENT),
  // A VML shape a save had to write as DrawingML, because it was edited
  // (`docs/08` ADR-072).
  "docx.export.vml.regenerated": entry("findings.feature.vmlRegenerated", CONTENT),
  "docx.watermark": entry("findings.feature.watermark", CONTENT),

  // ---- Styles and tables -----------------------------------------------------
  // A `w:style` whose `@w:type` names no kind this editor has. (An OMITTED type
  // is a paragraph style and is applied — HF-403.)
  "style": entry("findings.feature.styleUnknownType", CONTENT),
  // A second definition of a style id that disagrees with the first, by the kind
  // of the copy left out. A copy that only adds to the first is folded in and
  // raises nothing (HF-401, `casual-doc-import` `styles/duplicates.rs`).
  "docx.style.duplicate.paragraph": entry("findings.feature.styleDuplicateParagraph", CONTENT),
  "docx.style.duplicate.character": entry("findings.feature.styleDuplicateCharacter", CONTENT),
  "docx.style.duplicate.table": entry("findings.feature.styleDuplicateTable", CONTENT),
  "docx.style.duplicate.numbering": entry("findings.feature.styleDuplicateNumbering", CONTENT),
  // Table geometry in a unit or spelling the engine could not read.
  "tblCellMar": entry("findings.feature.tableCellMargins", CONTENT, "findings.feature.tableStyleCellMargins"),
  "tcMar": entry("findings.feature.cellMargins", CONTENT, "findings.feature.tableStyleCellMargins"),
  "tblInd": entry("findings.feature.tableIndent", CONTENT, "findings.feature.tableStyleIndent"),
  "tblW": entry("findings.feature.tableWidth", CONTENT, "findings.feature.tableStyleWidth"),

  // ---- Charts and embedded files -------------------------------------------
  "chart.trendline": entry("findings.feature.chartTrendline", CONTENT),
  "chart.*": entry("findings.feature.chartDetail", CONTENT),
  "word/charts/colorsN.xml": entry("findings.feature.chartColors", CONTENT),
  "word/charts/styleN.xml": entry("findings.feature.chartStyle", CONTENT),
  "word/embeddings/*": entry("findings.feature.embeddedFile", CONTENT),

  // ---- The theme -------------------------------------------------------------
  "theme/@name": entry("findings.feature.themeName", CONTENT),
  "fontScheme/@name": entry("findings.feature.themeFontsName", CONTENT),
  "objectDefaults": entry("findings.feature.themeObjectDefaults", CONTENT),
  "extraClrSchemeLst": entry("findings.feature.themeExtraColors", CONTENT),

  // ---- Document settings -----------------------------------------------------
  "decimalSymbol": entry("findings.feature.decimalSymbol", CONTENT),
  "listSeparator": entry("findings.feature.listSeparator", CONTENT),
  "defaultImageDpi": entry("findings.feature.defaultImageDpi", CONTENT),
  "doNotAutoCompressPictures": entry("findings.feature.doNotCompressImages", CONTENT),
  "mathPr": entry("findings.feature.equationOptions", CONTENT),
  "shapeDefaults": entry("findings.feature.shapeDefaults", CONTENT),
  // A retained `m:mathPr` or `w:shapeDefaults` fragment that failed the
  // writer's well-formedness check and was left out rather than spliced in
  // (`109` FID-AT-10). The report's location names which one.
  "docx.export.settings.fragment_refused": entry("findings.feature.settingsFragmentRefused", CONTENT),
  "hdrShapeDefaults": entry("findings.feature.headerShapeDefaults", CONTENT),
  "useFELayout": entry("findings.feature.eastAsianLayout", CONTENT),
  "formProt": entry("findings.feature.formProtection", CONTENT),
  "clrSchemeMapping": entry("findings.feature.colorMapping", CONTENT),
  "characterSpacingControl": entry("findings.feature.characterSpacing", CONTENT),
  "chartTrackingRefBased": entry("findings.feature.chartTracking", CONTENT),
  "attachedTemplate": entry("findings.feature.attachedTemplate", CONTENT),
  "docVars": entry("findings.feature.documentVariables", CONTENT),
  "mailMerge": entry("findings.feature.mailMerge", CONTENT),
  // The password material on the two protection elements, reported attribute
  // by attribute (sixteen names); to a reader it is one thing each.
  "documentProtection/@*": entry("findings.feature.restrictEditingPassword", CONTENT),
  "writeProtection/@*": entry("findings.feature.modifyPassword", CONTENT),

  // ---- Data stored with the document ---------------------------------------
  // A store the engine could not name is reported by its part name.
  "customXml/itemN.xml": entry("findings.feature.customXml", CONTENT),
  "customXml/itemPropsN.xml": entry("findings.feature.customXmlProperties", CONTENT),
  // A store the engine recognises by its root element is reported under a class
  // id, with its part name as the location (`109` FID-AT-18,
  // `casual-doc-import` `opaque::custom_xml_kind`). Kept verbatim either way;
  // what changes is whether a reader would miss it.
  //
  // Bookkeeping: an item's identity record (an id and the schemas it uses — it
  // never holds content), a bibliography store with no sources (Word writes one
  // the moment the References citation style is touched; it holds only the
  // style), and the two records a SharePoint library stamps on every file.
  "docx.customXml.storeIdentity": entry("findings.feature.customXmlStoreIdentity", BOOKKEEPING),
  "docx.customXml.bibliography.empty": entry("findings.feature.bibliographyStyleOnly", BOOKKEEPING),
  "docx.customXml.sharepoint.contentType": entry("findings.feature.sharePointContentType", BOOKKEEPING),
  "docx.customXml.sharepoint.forms": entry("findings.feature.sharePointForms", BOOKKEEPING),
  // Content: sources a reader entered, metadata values a library holds, the
  // cover page's own fields.
  "docx.customXml.bibliography": entry("findings.feature.bibliographySources", CONTENT),
  "docx.customXml.sharepoint.properties": entry("findings.feature.sharePointProperties", CONTENT),
  "docx.customXml.coverPage": entry("findings.feature.coverPageProperties", CONTENT),
  // Microsoft Purview's sensitivity-label record.
  "docMetadata/LabelInfo.xml": entry("findings.feature.sensitivityLabel", CONTENT),

  // ---- Fonts and media the engine could not use ------------------------------
  "docx.font.embedded.*": entry("findings.feature.embeddedFontUnusable", CONTENT),
  "docx.media.unreadable-part": entry("findings.feature.mediaUnreadable", CONTENT),

  // ---- What a save could not write (the export report) -----------------------
  "docx.export.background": entry("findings.feature.pageColor", CONTENT),
  "docx.export.media.missing_bytes": entry("findings.feature.pictureDataMissing", CONTENT),
  "docx.export.header.unreferenced_dropped": entry("findings.feature.unusedHeader", CONTENT),
  "docx.export.footer.unreferenced_dropped": entry("findings.feature.unusedFooter", CONTENT),
  "docx.export.embedded_object.missing_part": entry("findings.feature.embeddedObjectMissing", CONTENT),
  "docx.export.chart.partial_coverage_not_regenerated": entry("findings.feature.chartNotRewritten", CONTENT),
  "docx.export.chart.workbook_replaced": entry("findings.feature.chartWorkbookRewritten", CONTENT),
  "docx.export.chart.fragment_dropped": entry("findings.feature.chartDetailNotWritten", CONTENT),
  "docx.export.watermark.shared_header_conflict": entry("findings.feature.watermarkSharedHeader", CONTENT),
  "docx.export.watermark.missing_picture_bytes": entry("findings.feature.watermarkPictureMissing", CONTENT),
  "docx.export.embedded_font.missing_bytes": entry("findings.feature.embeddedFontMissing", CONTENT),
  "docx.export.retained_parts": entry("findings.feature.retainedParts", CONTENT),
  "source_envelope": entry("findings.feature.sourceMismatch", CONTENT),
  "docx.export.report.overflow": entry("findings.feature.overflow", CONTENT),
  "(overflow)": entry("findings.feature.overflow", CONTENT),

  // ---- A PDF's reconstruction (`docs/182` P2, the `docs/131` §16.1 families) --
  // What the import found on the pages; which group a row sits in is the
  // engine's model outcome (`compat_findings.mjs`'s PDF kinds), never these words.
  "pdf.import.active_content_blocked": entry("findings.feature.pdfActiveContent", CONTENT),
  "pdf.import.annotation_omitted": entry("findings.feature.pdfAnnotations", CONTENT),
  "pdf.import.attachment_omitted": entry("findings.feature.pdfAttachments", CONTENT),
  "pdf.import.clip_ignored": entry("findings.feature.pdfClip", CONTENT),
  "pdf.import.encrypted_unsupported": entry("findings.feature.pdfEncrypted", CONTENT),
  "pdf.import.font_embedded": entry("findings.feature.pdfFontEmbedded", CONTENT),
  "pdf.import.font_substituted": entry("findings.feature.pdfFontSubstituted", CONTENT),
  "pdf.import.form_omitted": entry("findings.feature.pdfForms", CONTENT),
  "pdf.import.image_clip_ignored": entry("findings.feature.pdfImageClip", CONTENT),
  "pdf.import.image_reencoded": entry("findings.feature.pdfImageReencoded", CONTENT),
  "pdf.import.image_undecodable": entry("findings.feature.pdfImageUndecodable", CONTENT),
  "pdf.import.invisible_text": entry("findings.feature.pdfInvisibleText", CONTENT),
  "pdf.import.native_text_invalid": entry("findings.feature.pdfTextUnidentified", CONTENT),
  "pdf.import.optional_content_flattened": entry("findings.feature.pdfLayers", CONTENT),
  "pdf.import.page_size_clamped": entry("findings.feature.pdfPageSize", CONTENT),
  "pdf.import.paragraph_boundary_inferred": entry("findings.feature.pdfParagraphs", CONTENT),
  "pdf.import.pattern_flattened": entry("findings.feature.pdfPatterns", CONTENT),
  "pdf.import.reading_order_inferred": entry("findings.feature.pdfReadingOrder", CONTENT),
  "pdf.import.tag_structure_ignored": entry("findings.feature.pdfTags", CONTENT),
  "pdf.import.text_render_mode_degraded": entry("findings.feature.pdfTextEffects", CONTENT),
  "pdf.import.text_rotated": entry("findings.feature.pdfRotatedText", CONTENT),
  "pdf.import.transparency_flattened": entry("findings.feature.pdfTransparency", CONTENT),
  "pdf.import.type3_glyphs_substituted": entry("findings.feature.pdfType3", CONTENT),
  "pdf.import.vector_split": entry("findings.feature.pdfVectorSplit", CONTENT),
  // Reflow (P3), links and notes, and OCR (P5).
  "pdf.import.native_ocr_conflict": entry("findings.feature.pdfOcrConflict", CONTENT),
  "pdf.import.ocr_applied": entry("findings.feature.pdfOcrApplied", CONTENT),
  "pdf.import.ocr_limit_reached": entry("findings.feature.pdfOcrLimit", CONTENT),
  "pdf.import.ocr_low_confidence": entry("findings.feature.pdfOcrLowConfidence", CONTENT),
  "pdf.import.ocr_scan_covered": entry("findings.feature.pdfOcrScanCovered", CONTENT),
  "pdf.import.ocr_unavailable": entry("findings.feature.pdfOcrUnavailable", CONTENT),
  "pdf.import.artifact_omitted": entry("findings.feature.pdfArtifacts", CONTENT),
  "pdf.import.bookmark_inferred": entry("findings.feature.pdfBookmarks", CONTENT),
  "pdf.import.columns_inferred": entry("findings.feature.pdfColumns", CONTENT),
  "pdf.import.comment_inferred": entry("findings.feature.pdfComments", CONTENT),
  "pdf.import.dehyphenation_inferred": entry("findings.feature.pdfDehyphenation", CONTENT),
  "pdf.import.footnote_inferred": entry("findings.feature.pdfFootnotes", CONTENT),
  "pdf.import.header_footer_inferred": entry("findings.feature.pdfHeaderFooter", CONTENT),
  "pdf.import.heading_inferred": entry("findings.feature.pdfHeadings", CONTENT),
  "pdf.import.hyperlink_inferred": entry("findings.feature.pdfHyperlinks", CONTENT),
  "pdf.import.link_unresolved": entry("findings.feature.pdfLinksUnresolved", CONTENT),
  "pdf.import.list_inferred": entry("findings.feature.pdfLists", CONTENT),
  "pdf.import.style_inferred": entry("findings.feature.pdfStyles", CONTENT),
  "pdf.import.table_inferred": entry("findings.feature.pdfTables", CONTENT),
  "pdf.import.table_topology_degraded": entry("findings.feature.pdfTablesSimplified", CONTENT),
  "pdf.import.vector_omitted": entry("findings.feature.pdfVectorsOmitted", CONTENT),
  "pdf.import.footnote_marker_renumbered": entry("findings.feature.pdfFootnoteMarkers", CONTENT),
  "pdf.import.justification_inferred": entry("findings.feature.pdfJustification", CONTENT),
  "pdf.import.page_break_inferred": entry("findings.feature.pdfPageBreaks", CONTENT),
  "pdf.import.decoration_inferred": entry("findings.feature.pdfDecorations", CONTENT),
  "pdf.import.float_inferred": entry("findings.feature.pdfFloats", CONTENT),
  "pdf.import.scan_replaced_by_text": entry("findings.feature.pdfScanReplaced", CONTENT),
  "pdf.import.image_text_boxes": entry("findings.feature.pdfImageTextBoxes", CONTENT),
  "pdf.import.vector_placed": entry("findings.feature.pdfVectorsPlaced", CONTENT),
  // The default open, a flowing document (`docs/182` §4), and the characters
  // made fit for editing (§18.8).
  "pdf.import.layout_inferred": entry("findings.feature.pdfLayoutInferred", CONTENT),
  "pdf.import.exact_save_fixed_layout_only": entry("findings.feature.pdfExactSaveFixedOnly", CONTENT),
  "pdf.import.flowing_unavailable": entry("findings.feature.pdfFlowingUnavailable", CONTENT),
  "pdf.import.ligature_decomposed": entry("findings.feature.pdfLigatures", CONTENT),
  "pdf.import.symbol_font_mapped": entry("findings.feature.pdfSymbolFonts", CONTENT),
  "pdf.import.marks_composed": entry("findings.feature.pdfMarksComposed", CONTENT),
  "pdf.import.quote_from_glyph_name": entry("findings.feature.pdfQuotes", CONTENT),
  "pdf.import.table_continued": entry("findings.feature.pdfTableContinued", CONTENT),
  // What saving a PDF-origin document as PDF did (`docs/182` §19, the export
  // report): kept the original, added an update to it, or regenerated it.
  "pdf.export.incremental_update": entry("findings.feature.pdfIncrementalUpdate", CONTENT),
  "pdf.export.page_carried_over": entry("findings.feature.pdfPageCarriedOver", CONTENT),
  "pdf.export.earlier_revision_retained": entry("findings.feature.pdfEarlierRevision", CONTENT),
  "pdf.export.signature_covers_earlier_revision": entry("findings.feature.pdfSignatureEarlier", CONTENT),
  "pdf.export.certification_invalidated": entry("findings.feature.pdfCertificationBroken", CONTENT),
  "pdf.export.original_written_back": entry("findings.feature.pdfOriginalWrittenBack", CONTENT),
  "pdf.export.preserve_fallback.*": entry("findings.feature.pdfPreserveFallback", CONTENT),
});

const PREFIXES = Object.keys(FEATURE_CATALOGUE)
  .filter((id) => id.endsWith("*"))
  .sort((a, b) => b.length - a.length);

/** The catalogued PART names, by their lower-case spelling. OPC part names are
 *  case-insensitive, and a producer that writes `customXML/item1.xml` names
 *  the same part as `customXml/item1.xml` — which fell through to the generic
 *  "A separate part of the file" until this matched it. Element and attribute
 *  ids are case-sensitive XML names and are never matched this way. */
const PART_IDS = new Map(
  Object.keys(FEATURE_CATALOGUE)
    .filter((id) => isPartName(id))
    .map((id) => [id.toLowerCase(), id]),
);

/** Whether an id is a package part name: a path with a file extension and no
 *  attribute step. */
function isPartName(id) {
  return /\/[^/@]+\.[A-Za-z0-9*]+$/.test(id) && !id.includes("@");
}

/** `customXml/item12.xml` -> `customXml/itemN.xml`: the number a part name
 *  carries before its extension is an index, not a different feature. */
function numbered(feature) {
  return feature.replace(/\d+(?=\.[A-Za-z0-9]+$)/, "N");
}

/**
 * The catalogue entry for a feature id, or `null` when nobody has written words
 * for it yet — which the caller answers with the fallback, never with a blank.
 *
 * @param {string} feature
 * @returns {{id: string, key: string, bookkeeping: boolean} | null}
 */
export function catalogueEntry(feature) {
  const id = String(feature ?? "");
  if (Object.hasOwn(FEATURE_CATALOGUE, id)) return { id, ...FEATURE_CATALOGUE[id] };
  const indexed = numbered(id);
  if (indexed !== id && Object.hasOwn(FEATURE_CATALOGUE, indexed)) {
    return { id: indexed, ...FEATURE_CATALOGUE[indexed] };
  }
  if (isPartName(id)) {
    const known = PART_IDS.get(id.toLowerCase()) ?? PART_IDS.get(indexed.toLowerCase());
    if (known) return { id: known, ...FEATURE_CATALOGUE[known] };
  }
  const prefix = PREFIXES.find((candidate) => id.startsWith(candidate.slice(0, -1)));
  return prefix ? { id: prefix, ...FEATURE_CATALOGUE[prefix] } : null;
}

/** Whether a report entry is Word's own bookkeeping. Only a catalogued entry can
 *  be: an id nobody has looked at is never presumed harmless. */
export function isBookkeeping(reportEntry) {
  return catalogueEntry(reportEntry?.feature)?.bookkeeping === true;
}

/**
 * Which area of the file a part name is, as a `findings.where.*` key suffix.
 *
 * DOCX part names first, then the OpenDocument ones, and `null` for no part. A
 * part this table does not know still gets a where — "in the file part …",
 * naming it — so a reader is never left without one.
 *
 * @param {string|null|undefined} partName
 * @returns {string|null}
 */
export function partArea(partName) {
  if (!partName) return null;
  // Case-insensitive, as OPC part names are (`customXML/item1.xml`).
  const part = String(partName).replace(/^\//, "");
  // A PDF finding's location is the first page it was seen on ("page 3"), not
  // a package part (`casual-doc-io`'s PDF adapter).
  if (/^page \d+$/.test(part)) return "pdfPage";
  const rules = [
    [/^word\/document\d*\.xml$/i, "body"],
    [/^word\/header\d*\.xml$/i, "header"],
    [/^word\/footer\d*\.xml$/i, "footer"],
    [/^word\/footnotes\.xml$/i, "footnotes"],
    [/^word\/endnotes\.xml$/i, "endnotes"],
    [/^word\/comments[A-Za-z]*\.xml$/i, "comments"],
    [/^word\/settings\.xml$/i, "settings"],
    [/^word\/theme\//i, "theme"],
    [/^word\/styles(WithEffects)?\.xml$/i, "styles"],
    [/^word\/numbering\.xml$/i, "numbering"],
    [/^word\/fontTable\.xml$/i, "fonts"],
    [/^word\/fonts\//i, "fonts"],
    [/^word\/charts\//i, "chart"],
    [/^word\/diagrams\//i, "diagram"],
    [/^word\/glossary\//i, "glossary"],
    [/^word\/media\//i, "media"],
    [/^word\/embeddings\//i, "embedding"],
    [/^word\/webSettings\.xml$/i, "webSettings"],
    [/^docProps\//i, "properties"],
    [/^customXml\//i, "customXml"],
    [/^docMetadata\//i, "properties"],
    // OpenDocument packages.
    [/^content\.xml$/i, "body"],
    [/^styles\.xml$/i, "styles"],
    [/^settings\.xml$/i, "settings"],
    [/^meta\.xml$/i, "properties"],
    [/^Pictures\//i, "media"],
  ];
  for (const [pattern, area] of rules) if (pattern.test(part)) return area;
  return "part";
}

/** The file format a class id belongs to, for a fallback with no part. */
function formatArea(feature) {
  const head = feature.slice(0, feature.indexOf("."));
  const formats = {
    docx: "formatWord",
    dotx: "formatWord",
    odt: "formatOdf",
    odf: "formatOdf",
    rtf: "formatRtf",
    markdown: "formatMarkdown",
    html: "formatHtml",
    pdf: "formatPdf",
    text: "formatText",
  };
  return formats[head] ?? null;
}

/**
 * What kind of thing a finding names, read from the shape of its id: an
 * attribute (`cNvPr/@name`), a class the engine minted (`docx.rsid`,
 * `odf.style.unresolved`, `(overflow)`), a whole package part
 * (`customXml/item1.xml`), or an element (`formProt`).
 */
export function findingShape(reportEntry) {
  const feature = String(reportEntry?.feature ?? "");
  if (feature.includes("/@")) return "attribute";
  if (!feature.includes("/") && (feature.includes(".") || feature.startsWith("("))) return "class";
  if (feature.includes("/")) return "part";
  return reportEntry?.location?.attributeName ? "attribute" : "element";
}

/**
 * Everything the dialog needs to say about one report entry, as catalogue keys.
 *
 * `words` is the entry's own words, or the fallback for its shape — and for an
 * element or attribute in the settings or the document properties, the noun
 * those parts actually hold, because "content or formatting" is the wrong noun
 * for a setting. `where` is `null` when the row would only repeat itself: a
 * whole-part finding already names its part, and a catalogued class needs no
 * format named beside words that say what it is.
 *
 * @param {object} reportEntry one `{feature, occurrences, location, ...}`.
 * @returns {{feature: string, known: boolean, bookkeeping: boolean,
 *   words: string, where: {key: string, params?: object} | null}}
 */
export function describeFinding(reportEntry) {
  const feature = String(reportEntry?.feature ?? "");
  const known = catalogueEntry(feature);
  const shape = findingShape(reportEntry);
  const partName = reportEntry?.location?.partName ?? null;
  const area = partArea(partName);

  let where = null;
  if (area && partName !== feature) {
    where = area === "part"
      ? { key: "findings.where.part", params: { part: partName } }
      : area === "pdfPage"
        ? { key: "findings.where.pdfPage", params: { page: Number(partName.slice(5)) } }
        : { key: `findings.where.${area}` };
  } else if (!known && !partName && shape === "class") {
    const format = formatArea(feature);
    if (format) where = { key: `findings.where.${format}` };
  } else if (!known && shape === "part" && area && area !== "part") {
    // An unknown whole part still says which area of the file it belongs to.
    where = { key: `findings.where.${area}` };
  }

  let words;
  if (known) {
    words = area === "styles" && known.stylesKey ? known.stylesKey : known.key;
  } else if ((shape === "element" || shape === "attribute") && area === "settings") {
    words = "findings.unknown.setting";
  } else if ((shape === "element" || shape === "attribute") && area === "properties") {
    words = "findings.unknown.property";
  } else {
    words = `findings.unknown.${shape}`;
  }
  return { feature, known: known !== null, bookkeeping: known?.bookkeeping === true, words, where };
}

/** Every catalogue key this module can hand a caller — the entries, the
 *  fallbacks and the wheres — so a test can hold each one to every locale. */
export function catalogueKeys() {
  const keys = new Set(Object.values(FEATURE_CATALOGUE).map((value) => value.key));
  for (const value of Object.values(FEATURE_CATALOGUE)) if (value.stylesKey) keys.add(value.stylesKey);
  for (const shape of ["element", "attribute", "part", "class", "setting", "property"]) {
    keys.add(`findings.unknown.${shape}`);
  }
  for (const area of WHERE_AREAS) keys.add(`findings.where.${area}`);
  return [...keys].sort();
}

/** Every area `partArea` and `formatArea` can answer. */
const WHERE_AREAS = Object.freeze([
  "body",
  "header",
  "footer",
  "footnotes",
  "endnotes",
  "comments",
  "settings",
  "theme",
  "styles",
  "numbering",
  "fonts",
  "chart",
  "diagram",
  "glossary",
  "media",
  "embedding",
  "webSettings",
  "properties",
  "customXml",
  "part",
  "formatWord",
  "formatOdf",
  "formatRtf",
  "formatMarkdown",
  "formatHtml",
  "formatPdf",
  "formatText",
  "pdfPage",
]);
