// SPDX-License-Identifier: Apache-2.0
//! An embedded object on a running surface reaches the model and survives a save
//! (`109` HF-266).
//!
//! # What was broken, measured before the fix
//!
//! The importer resolved a `c:chart r:id`, a SmartArt `dgm:relIds` and an OLE
//! `o:OLEObject r:id` through an embedded-relationship index built from
//! `document.xml.rels` alone. The note, header/footer and comment parsers were
//! each handed an **empty** index, so on those five surfaces the reference
//! resolved to nothing and the drawing was dropped: the surface's paragraph
//! imported as `inlines: []`, the part stayed in the opaque side-table with no
//! relationship pointing at it, and a semantic save wrote a header with no chart
//! in it beside an orphaned `chartN.xml` no reader opens. The loss was REPORTED
//! (`drawing`, and a whole-part `omitted` row for the chart), so it was not
//! silent — but it was a loss of something a reader could see, on every save.
//!
//! The same three parsers were handed no theme colour scheme either, so a
//! DrawingML `a:schemeClr` inside a header shape resolved against the all-zero
//! default palette: the shape's `accent1` fill imported as transparent black and
//! saved that way, with no finding at all. That half WAS silent.
//!
//! # What these guards assert
//!
//! Round trips, not a one-way import: `import → write → import`, and the object
//! must be on the same surface, referencing the same part, at the end. The
//! surfaces are enumerated rather than sampled (`SKILL` §9.3), so a surface that
//! stops working names itself.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{Import, ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, EmbeddedKind, Fill, GroupChild, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CHART_CT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CHART_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

/// The five running surfaces a chart is legal on, with the part each lives in and
/// the chart part each one's own relationships name. Distinct chart parts, so a
/// surface that resolved through ANOTHER surface's index would be caught by the
/// part-name assertion rather than passing by coincidence.
const SURFACES: [(&str, &str, &str); 5] = [
    ("header", "word/header1.xml", "word/charts/chart1.xml"),
    ("footer", "word/footer1.xml", "word/charts/chart2.xml"),
    ("footnote", "word/footnotes.xml", "word/charts/chart3.xml"),
    ("endnote", "word/endnotes.xml", "word/charts/chart4.xml"),
    ("comment", "word/comments.xml", "word/charts/chart5.xml"),
];

/// A chart every construct of which the projection represents, so each surface's
/// chart is projected completely and raises no finding of its own.
const COMPLETE_CHART: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>4.30</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:gapWidth val="150"/><c:axId val="111"/><c:axId val="222"/></c:barChart><c:catAx><c:axId val="111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222"/></c:catAx><c:valAx><c:axId val="222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111"/></c:valAx></c:plotArea><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart></c:chartSpace>"#;

/// One inline chart drawing referencing `rid` through the enclosing part's own
/// relationships.
fn chart_run(rid: &str) -> String {
    format!(
        r#"<w:r><w:drawing><wp:inline><wp:extent cx="4572000" cy="2743200"/><wp:docPr id="7"/><a:graphic><a:graphicData uri="{CHART_URI}"><c:chart r:id="{rid}"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    )
}

/// The namespace declarations every content part here needs.
fn namespaces() -> String {
    format!(
        r#"xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="{CHART_URI}" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#
    )
}

/// A relationships part declaring one chart relationship `rId1 -> target`.
fn chart_rels(target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/chart" Target="{target}"/></Relationships>"#
    )
}

/// A theme whose `accent1` is a colour nothing else in the package uses, so the
/// header shape's fill can only be that colour if the scheme reached the parser.
const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Probe"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F497D"/></a:dk2><a:lt2><a:srgbClr val="EEECE1"/></a:lt2><a:accent1><a:srgbClr val="C0FFEE"/></a:accent1><a:accent2><a:srgbClr val="C0504D"/></a:accent2><a:accent3><a:srgbClr val="9BBB59"/></a:accent3><a:accent4><a:srgbClr val="8064A2"/></a:accent4><a:accent5><a:srgbClr val="4BACC6"/></a:accent5><a:accent6><a:srgbClr val="F79646"/></a:accent6><a:hlink><a:srgbClr val="0000FF"/></a:hlink><a:folHlink><a:srgbClr val="800080"/></a:folHlink></a:clrScheme><a:fontScheme name="Probe"><a:majorFont><a:latin typeface="Calibri"/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#;

/// A floating rectangle filled with the theme's `accent1`.
const SCHEME_FILLED_SHAPE: &str = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="457200"/><wp:wrapNone/><wp:docPr id="9"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;

fn zip_named(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A package holding one chart on every running surface, each declared in that
/// surface's own `_rels`, plus a theme and (when `shape`) a scheme-filled shape in
/// the header.
fn package(shape: bool) -> Vec<u8> {
    let ns = namespaces();
    let mut overrides = String::new();
    for (part, content_type) in [
        (
            "word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        (
            "word/header1.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        ),
        (
            "word/footer1.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        ),
        (
            "word/footnotes.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        ),
        (
            "word/endnotes.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        ),
        (
            "word/comments.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        ),
        (
            "word/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
        ),
    ] {
        overrides.push_str(&format!(
            r#"<Override PartName="/{part}" ContentType="{content_type}"/>"#
        ));
    }
    for (_, _, chart) in SURFACES {
        overrides.push_str(&format!(
            r#"<Override PartName="/{chart}" ContentType="{CHART_CT}"/>"#
        ));
    }
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{overrides}</Types>"#
    );
    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body><w:p><w:commentRangeStart w:id="0"/><w:r><w:t>body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:endnoteReference w:id="1"/></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="default" r:id="rIdF"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let document_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="{REL}/header" Target="header1.xml"/><Relationship Id="rIdF" Type="{REL}/footer" Target="footer1.xml"/><Relationship Id="rIdFn" Type="{REL}/footnotes" Target="footnotes.xml"/><Relationship Id="rIdEn" Type="{REL}/endnotes" Target="endnotes.xml"/><Relationship Id="rIdCm" Type="{REL}/comments" Target="comments.xml"/><Relationship Id="rIdTh" Type="{REL}/theme" Target="theme/theme1.xml"/></Relationships>"#
    );
    let chart = chart_run("rId1");
    let header_shape = if shape { SCHEME_FILLED_SHAPE } else { "" };
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {ns}><w:p>{chart}{header_shape}</w:p></w:hdr>"#
    );
    let footer = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr {ns}><w:p>{chart}</w:p></w:ftr>"#
    );
    let footnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {ns}><w:footnote w:id="1"><w:p>{chart}</w:p></w:footnote></w:footnotes>"#
    );
    let endnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:endnotes {ns}><w:endnote w:id="1"><w:p>{chart}</w:p></w:endnote></w:endnotes>"#
    );
    let comments = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments {ns}><w:comment w:id="0" w:author="A"><w:p>{chart}</w:p></w:comment></w:comments>"#
    );
    let rels: Vec<(String, String)> = SURFACES
        .iter()
        .map(|(_, part, chart)| {
            let (directory, file) = part.rsplit_once('/').unwrap();
            let target = chart.strip_prefix("word/").unwrap();
            (format!("{directory}/_rels/{file}.rels"), chart_rels(target))
        })
        .collect();
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", document_rels.as_bytes()),
        ("word/header1.xml", header.as_bytes()),
        ("word/footer1.xml", footer.as_bytes()),
        ("word/footnotes.xml", footnotes.as_bytes()),
        ("word/endnotes.xml", endnotes.as_bytes()),
        ("word/comments.xml", comments.as_bytes()),
        ("word/theme/theme1.xml", THEME.as_bytes()),
    ];
    for (name, bytes) in &rels {
        entries.push((name.as_str(), bytes.as_bytes()));
    }
    for (_, _, chart) in SURFACES {
        entries.push((chart, COMPLETE_CHART.as_bytes()));
    }
    zip_named(&entries)
}

fn import(bytes: &[u8]) -> Import {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the package opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the package imports")
}

/// Every embedded object in `blocks`, as `(kind, part name)`.
fn embedded_in(blocks: &[BlockNode]) -> Vec<(EmbeddedKind, String)> {
    let mut found = Vec::new();
    for block in blocks {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::EmbeddedObject(object) = inline {
                    found.push((object.kind.clone(), object.part.part_name.clone()));
                }
            }
        }
    }
    found
}

/// The blocks of the one body on `surface`.
fn surface_blocks<'a>(document: &'a Document, surface: &str) -> Vec<&'a [BlockNode]> {
    let definitions = document.definitions();
    match surface {
        "header" => definitions
            .headers
            .iter()
            .map(|(_, body)| body.blocks.as_slice())
            .collect(),
        "footer" => definitions
            .footers
            .iter()
            .map(|(_, body)| body.blocks.as_slice())
            .collect(),
        "footnote" => definitions
            .footnotes
            .iter()
            .map(|(_, note)| note.blocks.as_slice())
            .collect(),
        "endnote" => definitions
            .endnotes
            .iter()
            .map(|(_, note)| note.blocks.as_slice())
            .collect(),
        "comment" => definitions
            .comments
            .iter()
            .map(|(_, comment)| comment.blocks.as_slice())
            .collect(),
        other => panic!("unknown surface {other}"),
    }
}

/// Asserts the chart is on every surface, referencing that surface's own part.
fn assert_every_surface_holds_its_chart(document: &Document, stage: &str) {
    for (surface, _, chart) in SURFACES {
        let bodies = surface_blocks(document, surface);
        assert_eq!(bodies.len(), 1, "{stage}: exactly one {surface} body");
        let objects = embedded_in(bodies[0]);
        assert_eq!(
            objects,
            vec![(EmbeddedKind::Chart, chart.to_owned())],
            "{stage}: the {surface}'s chart must reach the model referencing {chart} \
             (HF-266: the {surface} parser was handed an empty embedded index)"
        );
    }
}

fn written_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("the written package is a ZIP");
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        assert!(
            parts.insert(file.name().to_owned(), content).is_none(),
            "a part was written twice: {}",
            file.name()
        );
    }
    parts
}

/// The data-loss half of HF-266: the object reaches the model on every running
/// surface, survives the semantic save, and is on the same surface after reopen.
#[test]
fn an_embedded_object_on_every_running_surface_survives_a_save() {
    let source = package(false);
    let imported = import(&source);
    assert_every_surface_holds_its_chart(&imported.document, "import");

    // Each chart is projected — the projection walk covers every surface, not
    // only the body — and nothing was charged to a dropped drawing.
    assert_eq!(
        imported.document.definitions().charts.len(),
        SURFACES.len(),
        "one chart projection per surface"
    );
    let features: Vec<&str> = imported
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(
        !features.contains(&"drawing"),
        "no chart drawing was dropped, so none is reported: {features:?}"
    );
    for (_, _, chart) in SURFACES {
        assert!(
            !features.contains(&chart),
            "a chart part that reached the model is not an omitted part: {features:?}"
        );
    }

    let written = export_document_with_retained_parts(
        &imported.document,
        &BTreeMap::new(),
        &imported.retained_parts,
    )
    .expect("the document exports");
    let parts = written_parts(&written.bytes);
    for (_, _, chart) in SURFACES {
        assert!(parts.contains_key(chart), "{chart} is in the saved package");
    }
    // The surface references its chart through its OWN relationships: a header's
    // `c:chart r:id` resolves through `header1.xml.rels`, not the document's.
    for (surface, part, chart) in SURFACES {
        let (directory, file) = part.rsplit_once('/').unwrap();
        let rels_name = format!("{directory}/_rels/{file}.rels");
        let rels = String::from_utf8(
            parts
                .get(&rels_name)
                .unwrap_or_else(|| panic!("the saved {surface} has no {rels_name}"))
                .clone(),
        )
        .unwrap();
        let target = chart.strip_prefix("word/").unwrap();
        assert_eq!(
            rels.matches(target).count(),
            1,
            "the saved {surface} declares its chart exactly once: {rels}"
        );
    }
    let document_rels =
        String::from_utf8(parts.get("word/_rels/document.xml.rels").unwrap().clone()).unwrap();
    assert!(
        !document_rels.contains("charts/"),
        "no running-surface chart is re-added to the document's relationships as an \
         orphan: {document_rels}"
    );

    let reopened = import(&written.bytes);
    assert_every_surface_holds_its_chart(&reopened.document, "reopen");
    assert_eq!(
        reopened.document.definitions().charts.len(),
        SURFACES.len(),
        "every chart is still projected after the round trip"
    );
}

/// The silent half: a DrawingML shape on a running surface resolves its theme
/// colour against the document's theme, not against an all-zero palette.
#[test]
fn a_theme_coloured_shape_in_a_header_keeps_its_colour_through_a_save() {
    let accent1 = (0xC0, 0xFF, 0xEE);
    fn solid_fill_in(children: &[GroupChild]) -> Option<(u8, u8, u8, u8)> {
        children.iter().find_map(|child| match child {
            GroupChild::Shape(shape) => match &shape.fill {
                Some(Fill::Solid(color)) => Some((color.r, color.g, color.b, color.a)),
                _ => None,
            },
            GroupChild::Group(group) => solid_fill_in(&group.children),
            _ => None,
        })
    }
    let header_fill = |document: &Document| -> Option<(u8, u8, u8, u8)> {
        let (_, header) = document.definitions().headers.iter().next()?;
        header.blocks.iter().find_map(|block| {
            let BlockNode::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.inlines.iter().find_map(|inline| match inline {
                InlineNode::Group(group) => solid_fill_in(&group.children),
                _ => None,
            })
        })
    };
    let imported = import(&package(true));
    let fill = header_fill(&imported.document).expect("the header shape has a solid fill");
    assert_eq!(
        (fill.0, fill.1, fill.2),
        accent1,
        "the header shape's schemeClr accent1 must resolve against the theme \
         (got {fill:?}; an all-zero palette is the HF-266 sibling defect)"
    );
    let written = export_document_with_retained_parts(
        &imported.document,
        &BTreeMap::new(),
        &imported.retained_parts,
    )
    .expect("the document exports");
    let reopened = import(&written.bytes);
    let fill = header_fill(&reopened.document).expect("the header shape survives the save");
    assert_eq!((fill.0, fill.1, fill.2), accent1, "and keeps its colour");
}
