use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const NOTE_REFERENCES_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/></Types>"#;
const VISUAL_CONTAINMENT_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const CELL_HIT_ROUTING_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const CELL_HIT_ROUTING_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLogo" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/cell-logo.png"/></Relationships>"#;
const ROOT_RELATIONSHIPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const NOTE_REFERENCES_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdFootnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/><Relationship Id="rIdEndnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/></Relationships>"#;
const WRAP_TEXT_SIDES_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdWrapSide" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/wrap-side.png"/></Relationships>"#;
const VISUAL_CONTAINMENT_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdVisualFloat" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/visual-float.png"/></Relationships>"#;
const PAGINATION_FIDELITY_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#;
const PAGINATION_FIDELITY_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdFooter" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>"#;
/// The footer part of `pagination-fidelity.docx`. One paragraph on an **exact**
/// 300-twip line, so the reserved footer band is a fixed 300 twips no matter
/// which fonts are installed — the pagination scenarios the fixture
/// discriminates then differ only in the body.
const PAGINATION_FIDELITY_FOOTER: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:spacing w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="18"/></w:rPr><w:t>Pagination fidelity fixture footer</w:t></w:r></w:p></w:ftr>"#;
const WATERMARK_CONTENT_TYPES: &[u8] = br##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"##;
const WATERMARK_DOCUMENT_RELS: &[u8] = br##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"##;
/// The body of `watermark.docx`: one paragraph and a `w:sectPr` that references
/// the header the watermark shape lives in.
const WATERMARK_DOCUMENT: &[u8] = br##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>Body text behind a DRAFT watermark.</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdHeader"/><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"##;
/// The header part of `watermark.docx`, shaped exactly as Word writes Design ▸
/// Watermark: a floating `v:shape` whose `id` carries the
/// `PowerPlusWaterMarkObject` prefix Word re-identifies its own watermark by, the
/// plain-text WordArt shapetype (`#_x0000_t136`), `rotation:315` for the diagonal
/// layout, `<v:fill opacity=".5"/>` for "Semitransparent", and the stamped words
/// in `v:textpath@string` — an ATTRIBUTE, which is why an importer that reads only
/// element text loses them. Import must LIFT this shape onto the section
/// (`casual-doc-import/src/watermark.rs`) instead of leaving it a header float.
const WATERMARK_HEADER: &[u8] = br##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w10="urn:schemas-microsoft-com:office:word"><w:p><w:r><w:rPr><w:noProof/></w:rPr><w:pict><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@11,21600e"><v:path textpathok="t"/><v:textpath on="t" fitshape="t"/></v:shapetype><v:shape id="PowerPlusWaterMarkObject357476642" o:spid="_x0000_s2049" type="#_x0000_t136" style="position:absolute;margin-left:0;margin-top:0;width:527.85pt;height:131.95pt;rotation:315;z-index:-251658752;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin" o:allowincell="f" fillcolor="#c0c0c0" stroked="f"><v:fill opacity=".5"/><v:textpath style="font-family:&quot;Calibri&quot;;font-size:1pt" string="DRAFT"/></v:shape></w:pict></w:r></w:p></w:hdr>"##;
/// `chart.docx` content types. The two chart-part overrides and the workbook's
/// `Default Extension="xlsx"` are what let retention re-emit each part with the
/// content type the producer declared rather than a guessed one.
const CHART_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="xlsx" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/><Override PartName="/word/charts/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chartColorStyle+xml"/><Override PartName="/word/charts/style1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chartStyle+xml"/></Types>"#;
/// `chart.docx` document relationships: only the chart is reachable from the
/// body. `colors1.xml`, `style1.xml` and the workbook hang off the chart's own
/// `_rels`, which is how Word writes them and why the closure matters.
const CHART_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;
/// `chart.docx` body: a sentence, then an inline `a:graphicFrame` whose
/// `a:graphicData@uri` is the chart namespace and whose payload is `c:chart`,
/// then a sentence after it. The surrounding text is not decoration — it is how a
/// layout guard can tell a chart that reserved its `wp:extent` box from one that
/// reflowed the paragraph as a five-character word (`155` §1).
const CHART_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:t>Revenue by quarter, with the target as a line.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="5486400" cy="3200400"/><wp:docPr id="1" name="Chart 1"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId4"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:p><w:r><w:t>Figure 1.</w:t></w:r></w:p></w:body></w:document>"#;
/// `chart.docx` chart part. See `chart_entries`' doc comment for what each piece
/// of this is here to discriminate.
const CHART_PART: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Revenue by quarter</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Actual</c:v></c:pt></c:strCache></c:strRef></c:tx><c:spPr><a:solidFill><a:srgbClr val="4472C4"/></a:solidFill></c:spPr><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$5</c:f><c:strCache><c:ptCount val="4"/><c:pt idx="0"><c:v>Q1</c:v></c:pt><c:pt idx="1"><c:v>Q2</c:v></c:pt><c:pt idx="2"><c:v>Q3</c:v></c:pt><c:pt idx="3"><c:v>Q4</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="4"/><c:pt idx="0"><c:v>4.30</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt><c:pt idx="2"><c:v>3.5</c:v></c:pt><c:pt idx="3"><c:v>4.5</c:v></c:pt></c:numCache></c:numRef></c:val><c:trendline><c:trendlineType val="linear"/></c:trendline></c:ser><c:gapWidth val="150"/><c:overlap val="-27"/><c:axId val="111111111"/><c:axId val="222222222"/></c:barChart><c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/><c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:strRef><c:f>Sheet1!$C$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Target</c:v></c:pt></c:strCache></c:strRef></c:tx><c:val><c:numRef><c:f>Sheet1!$C$2:$C$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="4"/><c:pt idx="0"><c:v>4</c:v></c:pt><c:pt idx="1"><c:v>4</c:v></c:pt><c:pt idx="2"><c:v>4</c:v></c:pt><c:pt idx="3"><c:v>4</c:v></c:pt></c:numCache></c:numRef></c:val><c:smooth val="0"/></c:ser><c:marker val="1"/><c:axId val="111111111"/><c:axId val="333333333"/></c:lineChart><c:catAx><c:axId val="111111111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222222222"/></c:catAx><c:valAx><c:axId val="222222222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/><c:numFmt formatCode="General" sourceLinked="1"/><c:crossAx val="111111111"/></c:valAx><c:valAx><c:axId val="333333333"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="111111111"/></c:valAx></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart><c:externalData r:id="rId3"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#;
/// `chart.docx` chart-part relationships — the closure retention has to carry.
const CHART_PART_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartColorStyle" Target="colors1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartStyle" Target="style1.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet1.xlsx"/></Relationships>"#;
/// `chart.docx` colour style, abridged to one method element. `155` §4.3 puts this
/// part out of scope and preserved, so its size is irrelevant and its survival is
/// not.
const CHART_COLORS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cs:colorStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" meth="cycle" id="10"><a:schemeClr val="accent1"/><a:schemeClr val="accent2"/></cs:colorStyle>"#;
/// `chart.docx` chart style, abridged the same way and out of scope the same way.
const CHART_STYLE: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cs:chartStyle xmlns:cs="http://schemas.microsoft.com/office/drawing/2012/chartStyle" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" id="201"><cs:axisTitle><cs:lnRef idx="0"/></cs:axisTitle></cs:chartStyle>"#;
/// `chart.docx` embedded workbook: an opaque blob with a ZIP local-file
/// signature, **not** a valid `.xlsx`.
///
/// This is the fixture's most deliberate choice. `155` §5 decides that we read the
/// cached data table and never open the workbook, because opening it is how a
/// document product acquires a spreadsheet engine. A fixture whose workbook we
/// could parse would quietly invite exactly the thing the design forbids, and a
/// test written against it would pass for the wrong reason. What must hold of these
/// bytes is only that they come back identical.
const CHART_WORKBOOK: &[u8] = b"PK\x03\x04opendoc-fixture-opaque-workbook";
/// `collapsed-headings.docx` — the only fixture in the repository that carries
/// `w15:collapsed`, Word's persisted "this heading is folded" state.
///
/// # Why it exists
///
/// Word writes a collapsed heading as a `CT_OnOff` element in the heading
/// paragraph's `w:pPr`, in the `w15` namespace
/// (`http://schemas.microsoft.com/office/word/2012/wordml`): *"When a collapsed
/// element is added to a paragraph (pPr) of a particular heading level and its
/// value is true/on/1, immediately subsequent paragraphs with a higher heading
/// level number appear collapsed when the document is opened"*
/// (Microsoft Open Specifications, MS-DOCX). Before this fixture **no `.docx` in
/// `fixtures/` carried the element at all** — 38 packages, every XML part, zero
/// hits — so the loss-coverage gate
/// (`casual-doc-export/tests/source_element_coverage.rs`) had never had the
/// opportunity to say whether the drop is reported or silent. `docs/154` §3.4 and
/// ADR-049 both name adding it as the cheapest item in the folding lane, because it
/// converts an unknown into either a report or a red gate.
///
/// # What it discriminates
///
/// Four headings, each a different state of the same `CT_OnOff` axis, so a reader
/// that collapses the tri-state cannot pass:
///
/// 1. `w15:collapsed w:val="1"` — folded, the ordinary case.
/// 2. `w15:collapsed` with **no** `w:val` — `CT_OnOff` defaults to on when the
///    attribute is absent (ECMA-376 §17.17.4), so this is also folded. A reader
///    that requires `w:val="1"` loses it.
/// 3. `w15:collapsed w:val="0"` — an **explicit** unfolded, which is a different
///    statement from the element being absent: it cancels whatever a style chain
///    contributed. This is the same tri-state trap `ParagraphProperties::
///    contextual_spacing` documents at length, and it is the one a plain `bool`
///    cannot represent.
/// 4. No `w15:collapsed` at all — absent, inherit.
///
/// Each heading is followed by a body paragraph, so the subtree a fold would hide
/// is real content rather than an empty heading, and the headings carry
/// `w:outlineLvl` directly rather than through a `w:pStyle`: heading level is what
/// folding keys on, the fixture must not also depend on a styles part resolving,
/// and `w:outlineLvl` is already modelled so it cannot confuse the loss gate with a
/// second new name.
///
/// The `w15` prefix is declared on the root exactly as Word declares it, under
/// `mc:Ignorable="w15"` — also as Word declares it, and deliberately, because that
/// directive is a licence for an old consumer to ignore the element and the fixture
/// should prove our reader does not take it. `body.rs` acts only on
/// `mc:AlternateContent` branches and documents that `mc:Ignorable` is plumbing
/// rather than a skip instruction; this fixture is what makes that checkable.
///
/// Note the namespace: it is `w15:collapsed`, **not** the bare `w:collapsed` this
/// project's own earlier notes guessed.
const COLLAPSED_HEADINGS_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w15"><w:body><w:p><w:pPr><w:outlineLvl w:val="0"/><w15:collapsed w:val="1"/></w:pPr><w:r><w:t>Folded heading, explicit on</w:t></w:r></w:p><w:p><w:r><w:t>Body under the folded heading.</w:t></w:r></w:p><w:p><w:pPr><w:outlineLvl w:val="1"/><w15:collapsed/></w:pPr><w:r><w:t>Folded subheading, implied on</w:t></w:r></w:p><w:p><w:r><w:t>Body under the implied-on subheading.</w:t></w:r></w:p><w:p><w:pPr><w:outlineLvl w:val="0"/><w15:collapsed w:val="0"/></w:pPr><w:r><w:t>Unfolded heading, explicit off</w:t></w:r></w:p><w:p><w:r><w:t>Body under the explicitly unfolded heading.</w:t></w:r></w:p><w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Heading with no collapsed state</w:t></w:r></w:p><w:p><w:r><w:t>Body under the heading that says nothing.</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:body></w:document>"#;
/// `wrap-text-sides.docx` — the only fixture in the repository that carries a
/// non-default `w:wrap@wrapText`, the attribute that says which side(s) of a
/// float the text may flow down.
///
/// # Why it exists
///
/// `ST_WrapText` has four values — `bothSides`, `left`, `right`, `largest` — and
/// on a square/tight/through wrap it selects which of the two side channels
/// beside the float remain available to the flow. Before this fixture **no
/// `.docx` in `fixtures/` carried a non-default value**: a scan of all 35
/// readable packages — 36 existed, the thirty-sixth being
/// `malformed-truncated.docx`, which is deliberately not a zip — over every XML
/// part, found exactly one `wrapText` anywhere, the `bothSides` in
/// `visual-containment.docx`. So nothing in the suite could tell
/// a reader that honours the attribute from one that ignores it, and nothing
/// could tell a writer that round-trips it from the one we actually had, which
/// hard-coded `bothSides` onto every side wrap it emitted.
///
/// # What it discriminates
///
/// Six floats, each a different state of the same axis, so a reader that
/// collapses it cannot pass:
///
/// 1. `wrapSquare wrapText="bothSides"` — the default, stated explicitly.
/// 2. `wrapSquare wrapText="left"` — text only in the channel left of the float.
/// 3. `wrapSquare wrapText="right"` — the mirror, which a reader that confuses
///    the attribute with an alignment will swap.
/// 4. `wrapTight wrapText="largest"` — on a *second* element name, so a reader
///    that only looks at `wrapSquare` loses it, and with the value whose meaning
///    cannot be reached by comparing midpoints.
/// 5. `wrapSquare` with **no** `@wrapText` — absent, which Word reads as
///    `bothSides` but which is a different document from case 1. A writer that
///    materializes the default here produces a package the author never wrote;
///    `absent_wrap_text_is_not_written_back` is the guard that refuses it.
/// 6. `wrapTopAndBottom` — an element the schema gives no `@wrapText`, because
///    it leaves no side channels. A reader that hunts the attribute by name
///    anywhere under the anchor would invent a side here.
/// 7. `wrapSquare wrapText="right"` on an anchored DrawingML **text box**
///    (`wps:wsp` + `wps:txbx`), not a picture.
/// 8. `wrapSquare wrapText="left"` on an anchored **group** (`wpg:wgp`).
///
/// 7 and 8 exist because the writer reaches `write_wrap` from three different
/// anchor paths — a picture, a text box, and a group through
/// `write_wrap_after_extent` — and only one of them ran against a side wrap
/// before. The owner's report is about dragging *shapes and text boxes*
/// specifically, so leaving the text-box writer path unexercised would be the
/// "modelled but not covered" shape this repository keeps paying for. The same
/// three paths are also how the attribute reaches a float that is not a picture
/// at all, which is most of what a user drags.
///
/// Floats 2, 3 and 4 sit a full inch in from the column edge rather than flush
/// to a margin. That is the geometry the attribute exists for and the geometry
/// the old midpoint guess got wrong: with no `@wrapText` to read, layout
/// compared the float's midpoint to the paragraph's and flipped which channel it
/// kept on a one-twip crossing, so text jumped sideways as the shape was
/// dragged past the column centre.
///
/// Note the namespace: `@wrapText` is **unqualified**, on a `wp:`-prefixed
/// element. It is not `w:wrapText` and not `wp:wrapText`; this project has
/// guessed an attribute namespace wrong before (`w15:collapsed` was noted as
/// `w:collapsed`), and a local-name parser cannot tell two same-named attributes
/// in different namespaces apart. This fixture is what pins the spelling to a
/// package instead of to a comment.
const WRAP_TEXT_SIDES_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><w:body><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="1" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>left</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Both sides"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text beside the float that says bothSides explicitly.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="2" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare wrapText="left"/><wp:docPr id="2" name="Left only"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="2" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text that may use only the channel to the left of this mid-measure float.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="3" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare wrapText="right"/><wp:docPr id="3" name="Right only"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="3" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text that may use only the channel to the right of this mid-measure float.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="4" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapTight wrapText="largest"/><wp:docPr id="4" name="Largest only"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="4" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text that may use whichever channel beside this tight-wrapped float is wider.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="5" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>left</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapSquare/><wp:docPr id="5" name="No wrapText at all"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="5" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text beside the float whose author wrote no wrapText attribute at all.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="6" simplePos="0" distT="0" distB="0" distL="0" distR="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapTopAndBottom/><wp:docPr id="6" name="No side channels"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="6" name="wrap-side.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdWrapSide"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text pushed above and below a float that has no side channels to select.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="7" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1828800" cy="914400"/><wp:wrapSquare wrapText="right"/><wp:docPr id="7" name="Right-only text box"/><a:graphic><a:graphicData><wps:wsp><wps:cNvPr id="7" name="Text Box 7"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>A text box, not a picture, whose author chose the right channel.</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text beside the right-only text box.</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="8" simplePos="0" distT="0" distB="0" distL="91440" distR="91440"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1828800" cy="914400"/><wp:wrapSquare wrapText="left"/><wp:docPr id="8" name="Left-only group"/><a:graphic><a:graphicData uri="urn:wpg"><wpg:wgp><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="914400"/><a:chOff x="0" y="0"/><a:chExt cx="1828800" cy="914400"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:cNvPr id="9" name="Rectangle 9"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp></wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:t>Text beside the left-only group.</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="567" w:footer="567"/></w:sectPr></w:body></w:document>"#;
const DOCUMENT: &[u8] = br#"<?xml version="1.0"?><w:document/>"#;
const MIXED_UNICODE_DOCUMENT: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    "<w:body><w:p><w:r><w:t xml:space=\"preserve\">",
    "Cafe\u{0301} | \u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940} | ",
    "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} | ",
    "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}",
    "</w:t></w:r></w:p></w:body></w:document>",
);
const NOTE_REFERENCES_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Text with footnote</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r></w:p><w:p><w:r><w:t>Text with endnote</w:t></w:r><w:r><w:endnoteReference w:id="2"/></w:r></w:p></w:body></w:document>"#;
const NOTE_REFERENCES_FOOTNOTES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id="0" w:type="continuationSeparator"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:t>Generated footnote body.</w:t></w:r></w:p></w:footnote></w:footnotes>"#;
const NOTE_REFERENCES_ENDNOTES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?><w:endnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:endnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:id="0" w:type="continuationSeparator"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote><w:endnote w:id="2"><w:p><w:r><w:t>Generated endnote body.</w:t></w:r></w:p></w:endnote></w:endnotes>"#;
const UNKNOWN_SAFE_PART: &[u8] =
    br#"<custom xmlns="urn:opendoc:fixture"><value>preserve-me</value></custom>"#;
const VISUAL_FLOAT_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xb8, 0xe3, 0xe6, 0xf6,
    0x1f, 0x00, 0x05, 0xd2, 0x02, 0x68, 0x3b, 0x3a, 0xb3, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    fs::create_dir_all(&output)?;

    fs::write(output.join("minimal-valid.docx"), minimal_package())?;

    fs::write(
        output.join("mixed-unicode.docx"),
        package(&entries_with_document(MIXED_UNICODE_DOCUMENT.as_bytes()))?,
    )?;

    fs::write(
        output.join("note-references.docx"),
        package(&note_reference_entries())?,
    )?;

    fs::write(
        output.join("emoji-containers.docx"),
        package(&emoji_containers_entries())?,
    )?;

    fs::write(
        output.join("visual-containment.docx"),
        package(&visual_containment_entries())?,
    )?;

    fs::write(
        output.join("watermark.docx"),
        package(&watermark_entries())?,
    )?;

    fs::write(
        output.join("pagination-fidelity.docx"),
        package(&pagination_fidelity_entries())?,
    )?;

    fs::write(
        output.join("cell-hit-routing.docx"),
        package(&cell_hit_routing_entries())?,
    )?;

    fs::write(
        output.join("tab-hit-offsets.docx"),
        package(&entries_with_document(&tab_hit_offsets_document()))?,
    )?;

    fs::write(
        output.join("custom-geometry.docx"),
        package(&entries_with_document(&custom_geometry_document()))?,
    )?;

    fs::write(
        output.join("floating-table.docx"),
        package(&entries_with_document(&floating_table_document()))?,
    )?;

    fs::write(output.join("chart.docx"), package(&chart_entries())?)?;

    fs::write(
        output.join("collapsed-headings.docx"),
        package(&entries_with_document(COLLAPSED_HEADINGS_DOCUMENT))?,
    )?;

    fs::write(
        output.join("wrap-text-sides.docx"),
        package(&wrap_text_sides_entries())?,
    )?;

    fs::write(
        output.join("themed-shape.docx"),
        package(&themed_shape_entries())?,
    )?;

    let mut unknown_safe = minimal_entries();
    unknown_safe.push((
        "customXml/item1.xml".to_owned(),
        UNKNOWN_SAFE_PART.to_vec(),
        CompressionMethod::Deflated,
    ));
    fs::write(
        output.join("unknown-safe-part.docx"),
        package(&unknown_safe)?,
    )?;

    let mut traversal = minimal_entries();
    traversal.push((
        "../outside.xml".to_owned(),
        b"unsafe".to_vec(),
        CompressionMethod::Stored,
    ));
    fs::write(output.join("path-traversal.docx"), package(&traversal)?)?;

    let mut expansion = required_stored_entries();
    expansion[2] = (
        "word/document.xml".to_owned(),
        vec![b'A'; 64 * 1024],
        CompressionMethod::Deflated,
    );
    fs::write(output.join("high-expansion.docx"), package(&expansion)?)?;

    let mut duplicate_entries = minimal_entries();
    duplicate_entries.push((
        "word/documenx.xml".to_owned(),
        b"duplicate".to_vec(),
        CompressionMethod::Stored,
    ));
    let mut duplicate = package(&duplicate_entries)?;
    patch_fourth_name_as_document(&mut duplicate)?;
    fs::write(output.join("duplicate-part.docx"), duplicate)?;

    fs::write(
        output.join("malformed-truncated.docx"),
        b"PK\x03\x04truncated",
    )?;
    Ok(())
}

fn minimal_package() -> Vec<u8> {
    package(&minimal_entries()).expect("fixture ZIP generation should succeed")
}

fn minimal_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    entries_with_document(DOCUMENT)
}

fn entries_with_document(document: &[u8]) -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            document.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

/// `emoji-containers.docx` — the same five emoji in four different block
/// containers: a body paragraph, a table cell, the running header, and a
/// footnote body.
///
/// The uniform-flow rule says every block container shapes through the same
/// pipeline as the body, so one coverage fix serves them all; this fixture is
/// what makes that claim checkable rather than assumed. The table cell is here
/// because that is where the defect was reported.
///
/// The five are deliberately the five hard shapes, not five pictures: a lone
/// scalar, a ZWJ sequence, a scalar plus a skin-tone modifier, an
/// emoji-presentation sequence (base + U+FE0F), and a regional-indicator pair.
/// Each is ONE user-perceived character and must shape, measure and caret as
/// one.
const EMOJI_TEXT: &str = concat!(
    "\u{1f600}",
    "\u{1f469}\u{200d}\u{1f4bb}",
    "\u{1f44d}\u{1f3fd}",
    "\u{2764}\u{fe0f}",
    "\u{1f1ec}\u{1f1e7}",
);
const EMOJI_CONTAINERS_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/></Types>"#;
const EMOJI_CONTAINERS_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/><Relationship Id="rIdFootnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>"#;

fn emoji_containers_document() -> Vec<u8> {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"",
            " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
            "<w:body>",
            "<w:p><w:r><w:t xml:space=\"preserve\">Body {emoji}</w:t></w:r>",
            "<w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>",
            "<w:tbl><w:tblPr><w:tblW w:w=\"9000\" w:type=\"dxa\"/></w:tblPr>",
            "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"9000\" w:type=\"dxa\"/></w:tcPr>",
            "<w:p><w:r><w:t xml:space=\"preserve\">Cell {emoji}</w:t></w:r></w:p>",
            "</w:tc></w:tr></w:tbl>",
            "<w:p/>",
            "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdHeader\"/>",
            "<w:pgSz w:w=\"11906\" w:h=\"16838\"/>",
            "<w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"",
            " w:header=\"720\" w:footer=\"720\"/>",
            "</w:sectPr></w:body></w:document>",
        ),
        emoji = EMOJI_TEXT,
    )
    .into_bytes()
}

fn emoji_containers_header() -> Vec<u8> {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<w:hdr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<w:p><w:r><w:t xml:space=\"preserve\">Header {emoji}</w:t></w:r></w:p></w:hdr>",
        ),
        emoji = EMOJI_TEXT,
    )
    .into_bytes()
}

fn emoji_containers_footnotes() -> Vec<u8> {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<w:footnotes xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<w:footnote w:id=\"-1\" w:type=\"separator\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>",
            "<w:footnote w:id=\"0\" w:type=\"continuationSeparator\"><w:p><w:r>",
            "<w:continuationSeparator/></w:r></w:p></w:footnote>",
            "<w:footnote w:id=\"1\"><w:p><w:r><w:t xml:space=\"preserve\">Note {emoji}</w:t>",
            "</w:r></w:p></w:footnote></w:footnotes>",
        ),
        emoji = EMOJI_TEXT,
    )
    .into_bytes()
}

fn emoji_containers_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            emoji_containers_document(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            EMOJI_CONTAINERS_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            EMOJI_CONTAINERS_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/header1.xml".to_owned(),
            emoji_containers_header(),
            CompressionMethod::Deflated,
        ),
        (
            "word/footnotes.xml".to_owned(),
            emoji_containers_footnotes(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn note_reference_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            NOTE_REFERENCES_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            NOTE_REFERENCES_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            NOTE_REFERENCES_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/footnotes.xml".to_owned(),
            NOTE_REFERENCES_FOOTNOTES.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/endnotes.xml".to_owned(),
            NOTE_REFERENCES_ENDNOTES.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

/// `chart.docx` — one realistic DrawingML chart with its whole relationship
/// closure, built for `docs/155`.
///
/// Every other chart XML in this repository is a two-element stub
/// (`<c:chartSpace><c:chart/></c:chartSpace>`), which is enough to prove that a
/// *reference* round-trips and not enough to prove anything about a chart. This
/// fixture is deliberately the shape `155` §4 and §6 argue about, so the claims in
/// that document are checkable rather than asserted:
///
/// * **Tier 1, and combo by construction.** A `c:barChart` and a `c:lineChart` in
///   one `c:plotArea` — which is what a combo chart *is* (`155` §4.2) — with two
///   cached series, cached categories, a title, a legend, and three axes. The
///   third axis (`axId 333333333`, `axPos="r"`) is a **secondary axis**, named by
///   the line group and not by the bar group, so a model that puts the chart type
///   on the chart rather than on the group cannot represent this file.
/// * **Out of scope, so the report has something true to say.** `c:trendline` on
///   the first series is a `155` §4.3 construct: it must never be silently
///   dropped and it must never be claimed as modeled.
/// * **`4.30`.** The first cached value is spelled with a trailing zero on
///   purpose. `4.30` and `4.3` are the same number and different documents, so a
///   projection that parses a cached value into an `f64` and reformats it cannot
///   rewrite this part byte-faithfully — which is the whole reason `155` §8.2
///   stores a cached number in its verbatim lexical form. A fixture is the only
///   thing that makes that mistake fail rather than merely be argued about.
/// * **A closure, not a part.** `chart1.xml` has its own `_rels` reaching
///   `colors1.xml`, `style1.xml` and an embedded workbook under
///   `word/embeddings/`. Retention enumerates parts rather than walking the
///   relationship graph (`casual-doc-import/src/lib.rs:512-644`), so a fixture
///   with a real closure is what proves the closure survives — the guarantee
///   `155` §6.1 rests on.
///
/// The workbook payload is a short opaque blob, not a valid `.xlsx`. That is
/// deliberate and it is the point: `155` §5 says we never open it, so a fixture
/// whose workbook we could parse would be testing something this design promises
/// not to do. What must hold is that its bytes come back unchanged.
/// `themed-shape.docx` content types, including the theme part's `Override`.
///
/// Word writes that override, so the fixture carries it. It is NOT what makes the part
/// reachable, though, and the comment here used to claim it was: removing the override
/// and regenerating leaves the theme resolving perfectly, because discovery is by
/// RELATIONSHIP TYPE. Breaking the relationship type is what makes the part vanish.
/// Recorded because the wrong version of this note would have sent the next reader
/// looking in the wrong place.
const THEMED_SHAPE_CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#;

/// `themed-shape.docx` document relationships: the theme, reachable from the body
/// part exactly as Word writes it.
const THEMED_SHAPE_DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/></Relationships>"#;

/// `themed-shape.docx` theme part.
///
/// The format scheme is deliberately NOT uniform, because a theme whose entries were
/// all alike could not tell a correct resolution from an index that landed on the
/// wrong neighbour. Each entry is here to discriminate one thing:
///
/// * **fill 1** — `a:solidFill` of `a:phClr`: what Word's Shape Styles gallery
///   writes, and the case that must take the shape's own colour.
/// * **fill 2** — `a:gradFill`, the entry the gallery writes for most of its
///   swatches. Its FIRST stop is `a:phClr` carrying an `a:tint` and its second is a
///   colour the theme fixes, so the fixture proves per-stop placeholder semantics
///   AND that the deferred transform is applied. The default Office theme's three
///   stops are all `phClr` differing only by transform, so a build that dropped the
///   transform would paint that gradient as one flat colour while looking
///   deliberate — this entry is the smallest thing that catches it.
/// * **fill 3** — `a:pattFill`. Modeled, and painted by nothing: it must leave the
///   shape UNFILLED, not take its foreground colour.
/// * **fill 4** — a colour the theme fixes itself, which must ignore the shape's own
///   colour, and which sits after the pattern so the fixture proves an unpaintable
///   entry holds its index instead of shifting this one.
/// * **line 1** resolves; **line 2** is an `a:ln` whose fill is a gradient, which
///   `ShapeStroke` cannot hold, so it must leave the shape UNSTROKED rather than
///   take the gradient's first stop.
/// * **effect 1** is an EMPTY `a:effectLst` and **effect 2** carries a real
///   `a:outerShdw`. That pairing is the whole point of resolving `a:effectRef` at
///   all: the default Office theme's first two effect styles are empty, so a build
///   that reported on the index alone would raise a false loss on most Word
///   documents.
const THEMED_SHAPE_THEME: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Fixture"><a:themeElements><a:clrScheme name="Fixture"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Fixture"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Fixture"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="40000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:pattFill prst="pct25"><a:fgClr><a:schemeClr val="phClr"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill><a:solidFill><a:srgbClr val="C0FFEE"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="dash"/></a:ln><a:ln w="12700"><a:gradFill><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"/></a:gs><a:gs pos="100000"><a:srgbClr val="808080"/></a:gs></a:gsLst><a:lin ang="0" scaled="0"/></a:gradFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

/// `themed-shape.docx` body: four anchored shapes, none of which declares an `spPr`
/// fill or outline, so the theme is the only possible source of their appearance.
///
/// * **Shape 1** references the `a:phClr` entries and names `accent2`, so a correct
///   resolution paints orange, not the accent1 a wrong palette index would pick. Its
///   `a:effectRef idx="0"` is "no effect" and must raise no finding.
/// * **Shape 2** references the GRADIENT entry and the gradient OUTLINE entry: it
///   must come out with a real two-stop gradient fill and no stroke at all. Its
///   `a:effectRef idx="1"` points at the empty effect style, which is also no
///   finding — that pair is what distinguishes a real effect loss from an index.
/// * **Shape 3** references entry 4, whose colour the theme fixes, so the shape's own
///   `schemeClr` must be ignored — and entry 4 is only reachable if the pattern
///   before it held its index.
/// * **Shape 4** references the PATTERN entry and `a:effectRef idx="2"`, the effect
///   style that carries a real `a:outerShdw`. It must come out unfilled, and both
///   losses must be reported.
const THEMED_SHAPE_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body><w:p><w:r><w:t>Themed shapes.</w:t></w:r><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="10" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="1" name="Placeholder styled"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvPr id="2" name="Shape 1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="1"><a:schemeClr val="accent2"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent2"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent6"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="11" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>2743200</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="3" name="Gradient styled"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvPr id="4" name="Shape 2"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="2"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="1"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="12" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>4572000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="5" name="Theme-fixed styled"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvPr id="6" name="Shape 3"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="1"><a:schemeClr val="accent3"/></a:lnRef><a:fillRef idx="4"><a:schemeClr val="accent3"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent3"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r><w:r><w:drawing><wp:anchor behindDoc="0" relativeHeight="13" simplePos="0"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>2743200</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="914400"/><wp:wrapNone/><wp:docPr id="7" name="Pattern styled"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvPr id="8" name="Shape 4"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="1"><a:schemeClr val="accent4"/></a:lnRef><a:fillRef idx="3"><a:schemeClr val="accent4"/></a:fillRef><a:effectRef idx="2"><a:schemeClr val="accent4"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p></w:body></w:document>"#;

fn themed_shape_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            THEMED_SHAPE_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            THEMED_SHAPE_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            THEMED_SHAPE_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/theme/theme1.xml".to_owned(),
            THEMED_SHAPE_THEME.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn chart_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            CHART_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            CHART_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            CHART_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/charts/chart1.xml".to_owned(),
            CHART_PART.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/charts/_rels/chart1.xml.rels".to_owned(),
            CHART_PART_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/charts/colors1.xml".to_owned(),
            CHART_COLORS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/charts/style1.xml".to_owned(),
            CHART_STYLE.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/embeddings/Microsoft_Excel_Worksheet1.xlsx".to_owned(),
            CHART_WORKBOOK.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

/// `wrap-text-sides.docx`: six anchored pictures that between them cover every
/// `ST_WrapText` value, the absent attribute, and a wrap element the schema gives
/// no `@wrapText`. One PNG is shared by all six — the picture is scaffolding; the
/// anchor is the subject.
fn wrap_text_sides_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            WRAP_TEXT_SIDES_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            VISUAL_CONTAINMENT_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            WRAP_TEXT_SIDES_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/media/wrap-side.png".to_owned(),
            VISUAL_FLOAT_PNG.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn visual_containment_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            visual_containment_document(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            VISUAL_CONTAINMENT_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            VISUAL_CONTAINMENT_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/media/visual-float.png".to_owned(),
            VISUAL_FLOAT_PNG.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn visual_containment_document() -> Vec<u8> {
    let drop_cap_body = "Drop-cap body text must begin beside the full initial, continue without clipping it, and return to the full measure after the initial ends. ".repeat(3);
    let float_anchor_text = "This paragraph starts beside a tall left-anchored picture. Every line whose vertical band crosses that picture must use the narrowed measure. ".to_owned();
    let float_following_text = "This following paragraph is intentionally still inside the picture band, so page-level exclusion must continue across the paragraph boundary before restoring the full measure below it. ".repeat(3);
    let split_row_text = "The first table row is deliberately long enough to split over page boundaries. Its cell content must remain inside each emitted row fragment and must never paint over either successor row. ".repeat(18);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
 xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
 xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
 xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
 xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
 <w:body>
  <w:p>
   <w:pPr>
    <w:keepNext/>
    <w:framePr w:dropCap="drop" w:lines="3" w:wrap="around"
     w:hAnchor="text" w:vAnchor="text" w:xAlign="left" w:yAlign="top"
     w:hSpace="90" w:vSpace="0"/>
   </w:pPr>
   <w:r><w:rPr><w:sz w:val="117"/></w:rPr><w:t>D</w:t></w:r>
  </w:p>
  <w:p><w:r><w:t>{drop_cap_body}</w:t></w:r></w:p>
  <w:p>
   <w:r>
    <w:drawing>
     <wp:anchor behindDoc="0" relativeHeight="1" simplePos="0"
      distT="0" distB="0" distL="0" distR="91440">
      <wp:simplePos x="0" y="0"/>
      <wp:positionH relativeFrom="margin"><wp:align>left</wp:align></wp:positionH>
      <wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>
      <wp:extent cx="1600200" cy="2000250"/>
      <wp:wrapSquare wrapText="bothSides"/>
      <wp:docPr id="1" name="Visual containment float"
       descr="Generated visual containment fixture"/>
      <a:graphic>
       <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
        <pic:pic>
         <pic:nvPicPr><pic:cNvPr id="1" name="visual-float.png"/><pic:cNvPicPr/></pic:nvPicPr>
         <pic:blipFill><a:blip r:embed="rIdVisualFloat"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
         <pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1600200" cy="2000250"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>
        </pic:pic>
       </a:graphicData>
      </a:graphic>
     </wp:anchor>
    </w:drawing>
   </w:r>
   <w:r><w:t>{float_anchor_text}</w:t></w:r>
  </w:p>
  <w:p><w:r><w:t>{float_following_text}</w:t></w:r></w:p>
  <w:tbl>
   <w:tblPr>
    <w:tblW w:w="5000" w:type="pct"/>
    <w:tblBorders>
     <w:top w:val="single" w:sz="8" w:color="000000"/>
     <w:left w:val="single" w:sz="8" w:color="000000"/>
     <w:bottom w:val="single" w:sz="8" w:color="000000"/>
     <w:right w:val="single" w:sz="8" w:color="000000"/>
     <w:insideH w:val="single" w:sz="8" w:color="000000"/>
    </w:tblBorders>
    <w:tblCellMar>
     <w:top w:w="120" w:type="dxa"/><w:left w:w="120" w:type="dxa"/>
     <w:bottom w:w="120" w:type="dxa"/><w:right w:w="120" w:type="dxa"/>
    </w:tblCellMar>
   </w:tblPr>
   <w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr>
    <w:p><w:r><w:t>{split_row_text}</w:t></w:r></w:p>
   </w:tc></w:tr>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/><w:shd w:val="clear" w:fill="DDEEFF"/></w:tcPr>
    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>SUCCESSOR ROW ONE</w:t></w:r></w:p>
   </w:tc></w:tr>
   <w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/><w:shd w:val="clear" w:fill="FFEEDD"/></w:tcPr>
    <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>SUCCESSOR ROW TWO</w:t></w:r></w:p>
   </w:tc></w:tr>
  </w:tbl>
  <w:sectPr>
   <w:pgSz w:w="7200" w:h="7200"/>
   <w:pgMar w:top="600" w:right="600" w:bottom="600" w:left="600" w:header="300" w:footer="300"/>
  </w:sectPr>
 </w:body>
</w:document>"#,
    )
    .into_bytes()
}

/// `watermark.docx`: a body whose `w:sectPr` references a header carrying Word's
/// own watermark shape. The importer's job is to lift it onto the section, so the
/// fixture proves the whole package path — content types, the header
/// relationship, and the header part — not just the XML mapping.
fn watermark_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            WATERMARK_DOCUMENT.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            WATERMARK_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            WATERMARK_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/header1.xml".to_owned(),
            WATERMARK_HEADER.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

fn pagination_fidelity_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            pagination_fidelity_document(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            PAGINATION_FIDELITY_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            PAGINATION_FIDELITY_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/footer1.xml".to_owned(),
            PAGINATION_FIDELITY_FOOTER.to_vec(),
            CompressionMethod::Deflated,
        ),
    ]
}

/// A page-count fidelity probe, shaped after a real A4 form that regressed from
/// 5 pages to 7 (`crates/casual-doc-render/tests/pagination_fidelity.rs`).
///
/// Every construct here is load-bearing:
///
/// - **`<w:docGrid w:linePitch="299"/>` with no `w:type`.** Word writes exactly
///   this into essentially every Latin `sectPr` and does not snap lines to it.
///   Reading the omitted type as an active line grid rounds each 240-twip body
///   line up to 299 and adds a page.
/// - **A footer part and no header part.** `w:pgMar/@w:header` is still written
///   (737) and is larger than the 567-twip top margin, so a body-top rule that
///   reserves a band for a header that does not exist silently loses 170 twips a
///   page. The footer's own band is real and *must* still be reserved.
/// - **A `continuous` second section.** Per ECMA-376 the *following* section's
///   start type decides whether a section break paginates, so the final section
///   is the `continuous` one: the two sections must share a page.
/// - **A long run of table rows.** Atomic fragments that cannot be split at a
///   line boundary, so a small height error shows up as a whole displaced row.
///
/// Paragraph lines are `w:lineRule="atLeast" w:line="240"` at 9 pt: `atLeast`
/// is still rounded up by an active grid (so the grid defect stays visible) but
/// pins the height at 240 for any installed font (so the page count does not
/// drift with font metrics). Table rows and the exact-spaced footer are
/// grid-immune by design, which keeps the scenarios differing only where they
/// should.
///
/// The counts are chosen so a single page-count assertion discriminates both
/// defects. Body area = `16838 - 567 - (737 + 300)` = **15234** twips = 63
/// lines a page, and the body is exactly **126** lines — two full pages. Reserve
/// a phantom header band and the area drops to 15064 = 62 lines, so 126 lines
/// need three pages. Snap the lines to the 299 grid and the 106 paragraphs grow
/// by 59 twips each, which also needs three pages.
fn pagination_fidelity_document() -> Vec<u8> {
    // Body geometry: A4 (11906 x 16838), top margin 567, bottom margin 278,
    // header/footer distance 737, footer band 300. 2 + 20 + 104 = 126 lines.
    let paragraph = |text: &str| {
        format!(
            "<w:p><w:pPr><w:spacing w:line=\"240\" w:lineRule=\"atLeast\"/>\
             <w:rPr><w:sz w:val=\"18\"/></w:rPr></w:pPr>\
             <w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"
        )
    };
    let section_properties = |extra: &str| {
        format!(
            "{extra}<w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
             <w:pgMar w:top=\"567\" w:right=\"709\" w:bottom=\"278\" w:left=\"709\" \
             w:header=\"737\" w:footer=\"737\" w:gutter=\"0\"/>\
             <w:cols w:space=\"720\"/><w:docGrid w:linePitch=\"299\"/>"
        )
    };

    let mut body = String::new();
    // Section one carries the only footer reference and is closed by a section
    // break whose successor is CONTINUOUS, so section two keeps flowing on the
    // same page instead of starting a new one.
    body.push_str(&paragraph("Section one, line one."));
    body.push_str(&format!(
        "<w:p><w:pPr><w:spacing w:line=\"240\" w:lineRule=\"atLeast\"/><w:sectPr>{}</w:sectPr></w:pPr>\
         <w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr><w:t>Section one, line two.</w:t></w:r></w:p>",
        section_properties("<w:footerReference w:type=\"default\" r:id=\"rIdFooter\"/>")
    ));

    // Section two: twenty atomic table rows, then a long run of body lines. The
    // second section declares no footer reference and must inherit the first's.
    body.push_str(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"10488\"/></w:tblGrid>",
    );
    for row in 1..=20 {
        body.push_str(&format!(
            "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"10488\" w:type=\"dxa\"/></w:tcPr>\
             <w:p><w:pPr><w:spacing w:line=\"240\" w:lineRule=\"atLeast\"/></w:pPr>\
             <w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr><w:t>Table row {row}.</w:t></w:r></w:p>\
             </w:tc></w:tr>"
        ));
    }
    body.push_str("</w:tbl>");
    for line in 1..=104 {
        body.push_str(&paragraph(&format!("Section two, body line {line}.")));
    }
    body.push_str(&format!(
        "<w:sectPr>{}</w:sectPr>",
        section_properties("<w:type w:val=\"continuous\"/>")
    ));

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
         <w:body>{body}</w:body></w:document>"
    )
    .into_bytes()
}

fn cell_hit_routing_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "word/document.xml".to_owned(),
            cell_hit_routing_document(),
            CompressionMethod::Deflated,
        ),
        (
            "[Content_Types].xml".to_owned(),
            CELL_HIT_ROUTING_CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            CELL_HIT_ROUTING_DOCUMENT_RELS.to_vec(),
            CompressionMethod::Deflated,
        ),
        (
            "word/media/cell-logo.png".to_owned(),
            VISUAL_FLOAT_PNG.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

/// A caret-routing probe for table cells whose clickable area carries **no
/// text**, on a page the first viewport never shows.
///
/// Shaped after the owner's loan agreement, where clicking a form's empty value
/// box and typing put the text into the LABEL cell beside it. Two rows, each
/// deliberately taller than its content so most of every cell is blank:
///
/// - **Row 1 — `LOGO | TEXT`.** The left cell holds one inline picture 600 twips
///   tall; the right cell holds several lines of text. Below the picture, a
///   *right-cell* text line is vertically nearer to the pointer than the left
///   cell's own line is, so a nearest-line search leaves the cell that was
///   clicked.
/// - **Row 2 — `LABEL | (empty)`.** Both cells are bottom-aligned in a 1200-twip
///   row, so the top 960 twips of the empty value cell are blank and its single
///   caret slot does not cover them. The empty cell paints nothing, so a search
///   that skips text-free lines cannot see it at all.
///
/// The table sits on **page 2**, reached through an explicit page break: every
/// browser spec in this repository operates inside the first viewport, which is
/// precisely why this survived. Line heights are `w:lineRule="exact"` so the
/// geometry the assertions rest on does not move with the installed fonts.
fn cell_hit_routing_document() -> Vec<u8> {
    let line =
        "<w:spacing w:line=\"240\" w:lineRule=\"exact\"/><w:rPr><w:sz w:val=\"18\"/></w:rPr>";
    let paragraph = |text: &str| {
        format!(
            "<w:p><w:pPr>{line}</w:pPr><w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr>\
             <w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
        )
    };
    // A 1000x600 twip picture: 635 EMU per twip.
    let picture_paragraph = format!(
        "<w:p><w:pPr>{line}</w:pPr><w:r><w:drawing>\
         <wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"635000\" cy=\"381000\"/>\
         <wp:docPr id=\"1\" name=\"Cell logo\" descr=\"Generated cell-hit-routing logo\"/>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"1\" name=\"cell-logo.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"rIdLogo\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"635000\" cy=\"381000\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\
         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    );
    let filler = "The first page exists only to push the table onto page two, where no browser specification in this repository has ever clicked. ".repeat(6);
    let beside_the_logo = "Legal documents you can trust since two thousand and four, in a cell wide enough to wrap over several lines beside the picture. ".repeat(2);

    let mut body = String::new();
    body.push_str(&paragraph(&filler));
    body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
    body.push_str(&paragraph("PAGE TWO TABLE"));
    body.push_str(
        "<w:tbl><w:tblPr><w:tblW w:w=\"6000\" w:type=\"dxa\"/>\
         <w:tblBorders>\
         <w:top w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         <w:left w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         <w:bottom w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         <w:right w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         <w:insideH w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         <w:insideV w:val=\"single\" w:sz=\"8\" w:color=\"000000\"/>\
         </w:tblBorders></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>",
    );
    // Row 1: a picture-only cell beside a wordy one, in a 2400-twip row.
    body.push_str(&format!(
        "<w:tr><w:trPr><w:trHeight w:val=\"2400\" w:hRule=\"atLeast\"/></w:trPr>\
         <w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{picture_paragraph}</w:tc>\
         <w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc></w:tr>",
        paragraph(&beside_the_logo)
    ));
    // Row 2: the form shape — a label beside an EMPTY value cell, both pinned
    // to the bottom of a 1200-twip row.
    body.push_str(&format!(
        "<w:tr><w:trPr><w:trHeight w:val=\"1200\" w:hRule=\"atLeast\"/></w:trPr>\
         <w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/>\
         <w:vAlign w:val=\"bottom\"/></w:tcPr>{}</w:tc>\
         <w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/>\
         <w:vAlign w:val=\"bottom\"/></w:tcPr><w:p><w:pPr>{line}</w:pPr></w:p></w:tc></w:tr>",
        paragraph("Interest rate")
    ));
    body.push_str("</w:tbl>");
    body.push_str(&paragraph("AFTER THE TABLE"));
    body.push_str(
        "<w:sectPr><w:pgSz w:w=\"7200\" w:h=\"7200\"/>\
         <w:pgMar w:top=\"600\" w:right=\"600\" w:bottom=\"600\" w:left=\"600\" \
         w:header=\"300\" w:footer=\"300\"/></w:sectPr>",
    );

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <w:body>{body}</w:body></w:document>"
    )
    .into_bytes()
}

/// A caret-offset probe for **tab-indented** text, including a tab-indented
/// paragraph that soft-wraps inside a table cell.
///
/// Shaped after the footer block of the owner's loan agreement, whose last page
/// carries `\t\t\tThe Voice of the Tax Agent community. Since 1992` in a table
/// cell. Clicking between the `h` and the `e` of `the` drew the caret exactly
/// there and then typed three characters earlier, just after the `f` of `of`.
///
/// The mechanism is byte accounting, not geometry: a `w:tab` is one byte (`\t`)
/// of the paragraph's model text, and the layout's tab layer threaded its caret
/// byte cursor across tabs as if they were zero-width. Every glyph after a tab
/// therefore carried a cluster one byte short per preceding tab. Because the
/// caret is painted at the same stop the hit resolved to, the caret looked
/// right and only the insertion was wrong — the editor showing one thing and
/// doing another.
///
/// The fixture holds the three shapes that make the drift observable:
///
/// - **A body paragraph with three leading tabs**, so the drift is three bytes
///   and cannot be mistaken for an off-by-one at a boundary.
/// - **A tab-indented paragraph in a narrow table cell that soft-wraps**, so the
///   second visual line is reached through the wrap repair as well as the tab
///   cursor.
/// - **A paragraph with a tab between two runs**, so the drift appears in the
///   middle of a line rather than only at its start.
/// - **A paragraph with a tab *and* a `PAGE` field**, which routes through the
///   separate fielded-line assembly — a second byte cursor, with the same bug,
///   that a fixture without a field never reaches.
///
/// Line heights are `w:lineRule="exact"` so assertions do not move with the
/// installed fonts, and every x the tests use is taken from `caret_rect` rather
/// than from a hard-coded column.
fn tab_hit_offsets_document() -> Vec<u8> {
    let line =
        "<w:spacing w:line=\"240\" w:lineRule=\"exact\"/><w:rPr><w:sz w:val=\"18\"/></w:rPr>";
    let run = |text: &str| {
        format!(
            "<w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr>\
             <w:t xml:space=\"preserve\">{text}</w:t></w:r>"
        )
    };
    let tab = "<w:r><w:rPr><w:sz w:val=\"18\"/></w:rPr><w:tab/></w:r>";

    let mut body = String::new();
    // 1. Three leading tabs before ordinary body text.
    body.push_str(&format!(
        "<w:p><w:pPr>{line}</w:pPr>{tab}{tab}{tab}{}</w:p>",
        run("The Voice of the Tax Agent community. Since 1992")
    ));
    // 2. The same shape inside a narrow cell, wide enough that the text wraps.
    body.push_str(
        "<w:tbl><w:tblPr><w:tblW w:w=\"4000\" w:type=\"dxa\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid>",
    );
    body.push_str(&format!(
        "<w:tr><w:tc><w:tcPr><w:tcW w:w=\"4000\" w:type=\"dxa\"/></w:tcPr>\
         <w:p><w:pPr>{line}</w:pPr>{tab}{tab}{}</w:p></w:tc></w:tr>",
        run("The Voice of the Tax Agent community. Since 1992 and for many years after that.")
    ));
    body.push_str("</w:tbl>");
    // 3. A tab BETWEEN two runs, so the drift starts mid-line.
    body.push_str(&format!(
        "<w:p><w:pPr>{line}</w:pPr>{}{tab}{}</w:p>",
        run("Name"),
        run("The Voice of the Tax Agent")
    ));
    // 4. A tab in a paragraph that also carries a FIELD, which routes the line
    //    through the separate fielded-line assembly rather than the tab layer —
    //    a second byte cursor that has to agree with the first.
    body.push_str(&format!(
        "<w:p><w:pPr>{line}</w:pPr>{}{tab}{}\
         <w:fldSimple w:instr=\" PAGE \">{}</w:fldSimple></w:p>",
        run("Ref"),
        run("The Voice of the Tax Agent"),
        run("1")
    ));
    body.push_str(
        "<w:sectPr><w:pgSz w:w=\"7200\" w:h=\"7200\"/>\
         <w:pgMar w:top=\"600\" w:right=\"600\" w:bottom=\"600\" w:left=\"600\" \
         w:header=\"300\" w:footer=\"300\"/></w:sectPr>",
    );

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}</w:body></w:document>"
    )
    .into_bytes()
}

/// `custom-geometry.docx` — four anchored `wps:wsp` shapes whose geometry is
/// `a:custGeom`, covering both sides of the modeled subset (docs/119).
///
/// 1. **open rule** — a byte-for-byte reproduction of the five identical shapes
///    in the owner's loan agreement: `@w` set, `@h` omitted, one `a:moveTo` and
///    one `a:lnTo`, no `a:close`, no fill, a 1.5 pt teal stroke, in a box
///    524.45 pt × 0.1 pt. This is the shape `118` row 4 was filed against.
/// 2. **closed triangle** — three vertices plus `a:close`, with BOTH `@w` and
///    `@h`, and a fill. Discriminates `closed` and the two-axis scale.
/// 3. **curve** — an `a:cubicBezTo`, outside the subset.
/// 4. **guide coordinate** — `<a:pt x="wd2" y="t"/>`, a coordinate that is a
///    guide name rather than an integer, also outside the subset.
///
/// 3 and 4 must keep painting their bounding rectangle and keep reporting the
/// loss; a fixture with only the supported cases could not tell a correct
/// importer from one that flattens curves into straight lines.
/// `floating-table.docx` — a **positioned** table (`w:tblPr/w:tblpPr`) followed
/// by prose that must wrap beside it (`docs/109` row 64 / `105` FID-L-07).
///
/// The `w:tblpPr` is the one the owner's own corpus carries verbatim
/// (`demo.docx`, the only `w:tblpPr` in any document available to this
/// repository): anchored to the text, nudged `w:tblpY="1"` below the flow
/// position, with a 187-twip right and 72-twip bottom wrap gap and
/// `w:tblOverlap="never"`. The grid is Word's own 1818 + 1620 twips, so the
/// placed table is exactly 3 438 twips wide.
///
/// The page is US Letter with 1-inch margins (`w:pgSz`/`w:pgMar` written
/// explicitly, so the fixture's geometry does not depend on a default), and the
/// body is one short paragraph, the table, then enough prose to produce lines
/// both beside and below it.
fn floating_table_document() -> Vec<u8> {
    let cell = |width: i32, text: &str| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/></w:tcPr>\
             <w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"
        )
    };
    let row = |a: &str, b: &str| format!("<w:tr>{}{}</w:tr>", cell(1_818, a), cell(1_620, b));
    let prose = "the body text beside a positioned table must wrap around it ".repeat(24);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>\
         <w:p><w:r><w:t>Tables</w:t></w:r></w:p>\
         <w:tbl><w:tblPr>\
         <w:tblpPr w:rightFromText=\"187\" w:bottomFromText=\"72\" \
         w:vertAnchor=\"text\" w:tblpY=\"1\"/>\
         <w:tblOverlap w:val=\"never\"/>\
         <w:tblW w:w=\"0\" w:type=\"auto\"/>\
         </w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"1818\"/><w:gridCol w:w=\"1620\"/></w:tblGrid>\
         {}{}\
         </w:tbl>\
         <w:p><w:r><w:t xml:space=\"preserve\">{prose}</w:t></w:r></w:p>\
         <w:sectPr>\
         <w:pgSz w:w=\"12240\" w:h=\"15840\"/>\
         <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" \
         w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\
         </w:sectPr>\
         </w:body></w:document>",
        row("ITEM", "COST"),
        row("Widget", "12"),
    )
    .into_bytes()
}

fn custom_geometry_document() -> Vec<u8> {
    // One anchored shape carrying `geometry` as its `wps:spPr` geometry child.
    let shape = |name: &str, cx: i64, cy: i64, geometry: &str, paint: &str| {
        format!(
            "<w:p><w:r><w:drawing>\
             <wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" \
             relativeHeight=\"1\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" \
             allowOverlap=\"1\">\
             <wp:simplePos x=\"0\" y=\"0\"/>\
             <wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH>\
             <wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV>\
             <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
             <wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>\
             <wp:wrapNone/><wp:docPr id=\"1\" name=\"{name}\"/>\
             <a:graphic><a:graphicData \
             uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
             <wps:wsp><wps:cNvSpPr/><wps:spPr>\
             <a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
             {geometry}{paint}\
             </wps:spPr><wps:bodyPr/></wps:wsp>\
             </a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"
        )
    };
    // The empty containers Word writes on every `a:custGeom`. They state that
    // adjust handles, guides and connection sites are ABSENT, so an importer
    // that treated them as losses would be wrong (`118` §4).
    let empties = "<a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>";
    let teal_stroke = "<a:ln w=\"19050\"><a:solidFill><a:srgbClr val=\"29D19E\"/></a:solidFill>\
                       <a:prstDash val=\"solid\"/></a:ln>";
    let filled = "<a:solidFill><a:srgbClr val=\"c9d7f0\"/></a:solidFill>\
                  <a:ln w=\"12700\"><a:solidFill><a:srgbClr val=\"2a4b8d\"/></a:solidFill></a:ln>";

    let mut body = String::new();
    body.push_str("<w:p><w:r><w:t>Custom geometry fixture.</w:t></w:r></w:p>");
    body.push_str(&shape(
        "Freeform: open rule",
        6_660_515,
        1270,
        &format!(
            "<a:custGeom>{empties}<a:rect l=\"l\" t=\"t\" r=\"r\" b=\"b\"/><a:pathLst>\
             <a:path w=\"6660515\">\
             <a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo>\
             <a:lnTo><a:pt x=\"6660057\" y=\"0\"/></a:lnTo>\
             </a:path></a:pathLst></a:custGeom>"
        ),
        teal_stroke,
    ));
    body.push_str(&shape(
        "Freeform: closed triangle",
        914_400,
        914_400,
        &format!(
            "<a:custGeom>{empties}<a:pathLst>\
             <a:path w=\"100\" h=\"100\">\
             <a:moveTo><a:pt x=\"50\" y=\"0\"/></a:moveTo>\
             <a:lnTo><a:pt x=\"100\" y=\"100\"/></a:lnTo>\
             <a:lnTo><a:pt x=\"0\" y=\"100\"/></a:lnTo>\
             <a:close/></a:path></a:pathLst></a:custGeom>"
        ),
        filled,
    ));
    body.push_str(&shape(
        "Freeform: curve",
        914_400,
        914_400,
        &format!(
            "<a:custGeom>{empties}<a:pathLst>\
             <a:path w=\"100\" h=\"100\">\
             <a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo>\
             <a:cubicBezTo><a:pt x=\"30\" y=\"80\"/><a:pt x=\"70\" y=\"80\"/>\
             <a:pt x=\"100\" y=\"0\"/></a:cubicBezTo>\
             </a:path></a:pathLst></a:custGeom>"
        ),
        filled,
    ));
    body.push_str(&shape(
        "Freeform: guide coordinate",
        914_400,
        914_400,
        &format!(
            "<a:custGeom>{empties}<a:pathLst><a:path>\
             <a:moveTo><a:pt x=\"wd2\" y=\"t\"/></a:moveTo>\
             <a:lnTo><a:pt x=\"r\" y=\"b\"/></a:lnTo>\
             </a:path></a:pathLst></a:custGeom>"
        ),
        filled,
    ));
    body.push_str(
        "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/>\
         <w:pgMar w:top=\"720\" w:right=\"720\" w:bottom=\"720\" w:left=\"720\" \
         w:header=\"360\" w:footer=\"360\"/></w:sectPr>",
    );

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
         <w:body>{body}</w:body></w:document>"
    )
    .into_bytes()
}

fn required_stored_entries() -> Vec<(String, Vec<u8>, CompressionMethod)> {
    vec![
        (
            "[Content_Types].xml".to_owned(),
            CONTENT_TYPES.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "_rels/.rels".to_owned(),
            ROOT_RELATIONSHIPS.to_vec(),
            CompressionMethod::Stored,
        ),
        (
            "word/document.xml".to_owned(),
            DOCUMENT.to_vec(),
            CompressionMethod::Stored,
        ),
    ]
}

fn package(
    entries: &[(String, Vec<u8>, CompressionMethod)],
) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, compression) in entries {
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(*compression),
        )?;
        writer.write_all(bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn patch_fourth_name_as_document(bytes: &mut [u8]) -> Result<(), &'static str> {
    const DOCUMENT_PART: &[u8] = b"word/document.xml";
    let central = central_record_positions(bytes)?;
    let duplicate_central = *central.get(3).ok_or("missing fourth central record")?;
    let duplicate_local =
        usize::try_from(read_u32(bytes, duplicate_central + 42)?).map_err(|_| "large offset")?;
    let central_name = duplicate_central + 46;
    let local_name = duplicate_local + 30;
    bytes
        .get_mut(central_name..central_name + DOCUMENT_PART.len())
        .ok_or("central name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    bytes
        .get_mut(local_name..local_name + DOCUMENT_PART.len())
        .ok_or("local name outside fixture")?
        .copy_from_slice(DOCUMENT_PART);
    Ok(())
}

fn central_record_positions(bytes: &[u8]) -> Result<Vec<usize>, &'static str> {
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or("missing EOCD")?;
    let mut cursor = usize::try_from(read_u32(bytes, eocd + 16)?).map_err(|_| "large directory")?;
    let entries = usize::from(read_u16(bytes, eocd + 10)?);
    let mut positions = Vec::new();
    for _ in 0..entries {
        positions.push(cursor);
        let name = usize::from(read_u16(bytes, cursor + 28)?);
        let extra = usize::from(read_u16(bytes, cursor + 30)?);
        let comment = usize::from(read_u16(bytes, cursor + 32)?);
        cursor += 46 + name + extra + comment;
    }
    Ok(positions)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let value = bytes.get(offset..offset + 2).ok_or("short fixture")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value = bytes.get(offset..offset + 4).ok_or("short fixture")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
