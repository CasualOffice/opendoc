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

const entry = (key, bookkeeping) => Object.freeze({ key, bookkeeping });

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
  "textbox": entry("findings.feature.textBox", CONTENT),
  "docx.watermark": entry("findings.feature.watermark", CONTENT),

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
  "customXml/itemN.xml": entry("findings.feature.customXml", CONTENT),
  "customXml/itemPropsN.xml": entry("findings.feature.customXmlProperties", CONTENT),

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
});

const PREFIXES = Object.keys(FEATURE_CATALOGUE)
  .filter((id) => id.endsWith("*"))
  .sort((a, b) => b.length - a.length);

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
  const part = String(partName).replace(/^\//, "");
  const rules = [
    [/^word\/document\d*\.xml$/, "body"],
    [/^word\/header\d*\.xml$/, "header"],
    [/^word\/footer\d*\.xml$/, "footer"],
    [/^word\/footnotes\.xml$/, "footnotes"],
    [/^word\/endnotes\.xml$/, "endnotes"],
    [/^word\/comments[A-Za-z]*\.xml$/, "comments"],
    [/^word\/settings\.xml$/, "settings"],
    [/^word\/theme\//, "theme"],
    [/^word\/styles(WithEffects)?\.xml$/, "styles"],
    [/^word\/numbering\.xml$/, "numbering"],
    [/^word\/fontTable\.xml$/, "fonts"],
    [/^word\/fonts\//, "fonts"],
    [/^word\/charts\//, "chart"],
    [/^word\/diagrams\//, "diagram"],
    [/^word\/glossary\//, "glossary"],
    [/^word\/media\//, "media"],
    [/^word\/embeddings\//, "embedding"],
    [/^word\/webSettings\.xml$/, "webSettings"],
    [/^docProps\//, "properties"],
    [/^customXml\//, "customXml"],
    // OpenDocument packages.
    [/^content\.xml$/, "body"],
    [/^styles\.xml$/, "styles"],
    [/^settings\.xml$/, "settings"],
    [/^meta\.xml$/, "properties"],
    [/^Pictures\//, "media"],
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
    words = known.key;
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
]);
