use casual_doc_model::v1::{
    Alignment, BlockNode, Break, BreakKind, Color, HyperlinkTarget, InlineNode, Paragraph,
    RgbColor, StyleKind,
};
use casual_doc_ooxml::DocxPackage;

use crate::{
    Import, ImportConfig, ImportError, ImportMode, ModelOutcome, RetentionOutcome,
    import_main_document_xml, import_package, import_with_sources,
};

fn import(xml: &[u8]) -> Import {
    import_main_document_xml(xml, ImportConfig::default()).unwrap()
}

fn import_with_styles(document: &[u8], styles: &[u8]) -> Import {
    import_with_sources(
        document,
        Some(styles),
        None,
        None,
        None,
        &[],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn import_with_numbering(document: &[u8], numbering: &[u8]) -> Import {
    import_with_sources(
        document,
        None,
        Some(numbering),
        None,
        None,
        &[],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn part_sources(xml: &[u8]) -> crate::PartSources {
    crate::PartSources {
        xml: xml.to_vec(),
        ..Default::default()
    }
}

fn import_with_notes(document: &[u8], footnotes: Option<&[u8]>, endnotes: Option<&[u8]>) -> Import {
    let footnotes = footnotes.map(part_sources);
    let endnotes = endnotes.map(part_sources);
    import_with_sources(
        document,
        None,
        None,
        footnotes.as_ref(),
        endnotes.as_ref(),
        &[],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

fn features(import: &Import) -> Vec<&str> {
    import
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect()
}

fn paragraph(import: &Import, index: usize) -> &Paragraph {
    match &import.document.body()[index] {
        BlockNode::Paragraph(paragraph) => paragraph,
        BlockNode::Table(_) => panic!("expected a paragraph at index {index}"),
    }
}

/// Per-paragraph run text in document order, recursing through table cells.
fn collect_block_texts(blocks: &[BlockNode], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let text: String = paragraph
                    .inlines
                    .iter()
                    .filter_map(|inline| match inline {
                        InlineNode::Run(run) => Some(run.text.as_str()),
                        _ => None,
                    })
                    .collect();
                out.push(text);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_block_texts(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

fn nonempty_block_texts(import: &Import) -> Vec<String> {
    let mut texts = Vec::new();
    collect_block_texts(import.document.body(), &mut texts);
    texts.retain(|text| !text.is_empty());
    texts
}

/// The first table in the body, if any.
fn first_table(import: &Import) -> Option<&casual_doc_model::v1::Table> {
    import.document.body().iter().find_map(|block| match block {
        BlockNode::Table(table) => Some(table),
        BlockNode::Paragraph(_) => None,
    })
}

#[test]
fn paragraphs_runs_and_run_properties_are_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r>
                 <w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(import.document.body().len(), 2);

    let first = paragraph(&import, 0);
    assert_eq!(first.inlines.len(), 2);
    let InlineNode::Run(bold) = &first.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(bold.text, "Hello");
    assert_eq!(bold.properties.bold, Some(true));
    let InlineNode::Run(plain) = &first.inlines[1] else {
        panic!("expected run");
    };
    assert_eq!(plain.text, " world");
    assert_eq!(plain.properties.bold, None);

    assert_eq!(paragraph(&import, 1).inlines.len(), 1);
}

#[test]
fn adjacent_equal_property_runs_are_merged() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 1);
    let InlineNode::Run(run) = &para.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "ab");
}

#[test]
fn tabs_breaks_and_color_are_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>a</w:t><w:tab/><w:t>b</w:t>
                 <w:br w:type="page"/></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 4);
    assert!(matches!(para.inlines[0], InlineNode::Run(_)));
    assert!(matches!(para.inlines[1], InlineNode::Tab(_)));
    assert!(matches!(para.inlines[2], InlineNode::Run(_)));
    assert!(matches!(
        para.inlines[3],
        InlineNode::Break(Break {
            kind: BreakKind::Page,
            ..
        })
    ));
    let InlineNode::Run(run) = &para.inlines[0] else {
        panic!();
    };
    assert_eq!(
        run.properties.color,
        Some(Color::Rgb(RgbColor { r: 255, g: 0, b: 0 }))
    );
}

#[test]
fn table_is_modeled_as_a_block_with_cell_content() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        </w:body></w:document>"#;
    let import = import(xml);
    // The table is a first-class block; its cell paragraph is nested inside it.
    assert_eq!(import.document.body().len(), 1);
    let table = first_table(&import).expect("table modeled in body");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells.len(), 1);
    let cell = &table.rows[0].cells[0];
    let BlockNode::Paragraph(cell_paragraph) = &cell.blocks[0] else {
        panic!("expected a paragraph in the cell");
    };
    let InlineNode::Run(run) = &cell_paragraph.inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "cell");
    // The modeled table is not reported as unmapped.
    assert!(!features(&import).contains(&"tbl"));
}

#[test]
fn paragraph_direct_formatting_is_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr>
                <w:jc w:val="center"/>
                <w:ind w:left="720" w:right="360"/>
                <w:spacing w:before="120" w:after="240" w:line="360" w:lineRule="auto"/>
            </w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    let props = &paragraph(&import, 0).properties;
    assert_eq!(props.alignment, Some(Alignment::Center));
    let indentation = props.indentation.unwrap();
    assert_eq!(indentation.start_twips, Some(720));
    assert_eq!(indentation.end_twips, Some(360));
    let spacing = props.spacing.unwrap();
    assert_eq!(spacing.before_twips, Some(120));
    assert_eq!(spacing.after_twips, Some(240));
    assert_eq!(spacing.line_percent, Some(150));
    // jc/ind/spacing are mapped, so they are no longer reported.
    let features: Vec<&str> = import
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(!features.contains(&"jc"));
    assert!(!features.contains(&"ind"));
    assert!(!features.contains(&"spacing"));
}

#[test]
fn unmapped_paragraph_property_children_are_still_reported() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "pStyle")
    );
    // No dangling style reference is emitted (styles are not mapped yet).
    assert_eq!(paragraph(&import, 0).properties.style_ref, None);
}

#[test]
fn styles_are_mapped_and_paragraph_style_reference_resolves() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
            <w:style w:type="paragraph" w:styleId="Heading1"><w:basedOn w:val="Normal"/>
                <w:rPr><w:b/></w:rPr></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let definitions = import.document.definitions();
    assert_eq!(definitions.styles.len(), 2);

    let style_ref = paragraph(&import, 0).properties.style_ref.unwrap();
    let heading = definitions.styles.get(&style_ref).unwrap();
    assert_eq!(heading.kind, StyleKind::Paragraph);
    assert_eq!(heading.run.as_ref().unwrap().bold, Some(true));
    let base = definitions.styles.get(&heading.based_on.unwrap()).unwrap();
    assert_eq!(base.kind, StyleKind::Paragraph);
    assert!(!features(&import).contains(&"pStyle"));
}

#[test]
fn dangling_paragraph_style_reference_is_reported_not_emitted() {
    let styles = br#"<w:styles xmlns:w="urn:w"/>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Missing"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    assert_eq!(paragraph(&import, 0).properties.style_ref, None);
    assert!(features(&import).contains(&"pStyle"));
}

#[test]
fn based_on_kind_mismatch_is_dropped_and_reported() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="H"><w:basedOn w:val="C"/></w:style>
            <w:style w:type="character" w:styleId="C"/>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:pStyle w:val="H"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let style_ref = paragraph(&import, 0).properties.style_ref.unwrap();
    assert_eq!(
        import
            .document
            .definitions()
            .styles
            .get(&style_ref)
            .unwrap()
            .based_on,
        None
    );
    assert!(features(&import).contains(&"basedOn"));
}

#[test]
fn run_style_reference_resolves() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="character" w:styleId="Strong"><w:rPr><w:b/></w:rPr></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:rStyle w:val="Strong"/></w:rPr><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert!(run.properties.style_ref.is_some());
    assert!(!features(&import).contains(&"rStyle"));
}

#[test]
fn end_to_end_with_styles_part() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let styles = br#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="Heading1"><w:rPr><w:b/></w:rPr></w:style></w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Titled</w:t></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/styles.xml", styles.as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    assert_eq!(import.document.definitions().styles.len(), 1);
    assert!(paragraph(&import, 0).properties.style_ref.is_some());
}

#[test]
fn based_on_cycle_does_not_abort_import() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/></w:style>
            <w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/></w:style>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    // Import succeeds (the document validated, so the basedOn graph is
    // acyclic) and the broken edge is reported.
    let import = import_with_styles(document, styles);
    assert_eq!(import.document.definitions().styles.len(), 2);
    import.document.to_json().unwrap();
    assert!(features(&import).contains(&"basedOn"));
}

#[test]
fn out_of_domain_run_size_degrades_instead_of_aborting() {
    for size in ["0", "70000"] {
        let xml = format!(
            "<w:document xmlns:w=\"urn:w\"><w:body><w:p><w:r><w:rPr>\
                 <w:sz w:val=\"{size}\"/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"
        );
        let import = import(xml.as_bytes());
        let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
            panic!("expected run");
        };
        assert_eq!(run.text, "x");
        assert_eq!(run.properties.size_half_points, None);
    }
}

#[test]
fn run_style_reference_to_a_paragraph_style_is_rejected() {
    let styles = br#"<w:styles xmlns:w="urn:w">
            <w:style w:type="paragraph" w:styleId="Body"/>
        </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:rPr><w:rStyle w:val="Body"/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.properties.style_ref, None);
    assert!(features(&import).contains(&"rStyle"));
}

#[test]
fn empty_body_yields_a_single_empty_paragraph() {
    let import = import(br#"<w:document xmlns:w="urn:w"><w:body/></w:document>"#);
    assert_eq!(import.document.body().len(), 1);
    assert!(paragraph(&import, 0).inlines.is_empty());
}

#[test]
fn import_is_deterministic() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let first = import(xml).document.to_json().unwrap();
    let second = import(xml).document.to_json().unwrap();
    assert_eq!(first, second);
}

#[test]
fn dtd_bearing_xml_is_rejected() {
    let xml = br#"<!DOCTYPE w:document><w:document xmlns:w="urn:w"><w:body/></w:document>"#;
    assert_eq!(
        import_main_document_xml(xml, ImportConfig::default()),
        Err(ImportError::MalformedXml)
    );
}

#[test]
fn end_to_end_from_admitted_package() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hello DOCX</w:t></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "Hello DOCX");
}

#[test]
fn character_spacing_in_rpr_is_reported_not_silently_dropped() {
    // w:spacing in rPr is character spacing (unmapped); it must be reported and
    // must NOT be treated as the paragraph spacing element.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:spacing w:val="20"/></w:rPr><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"spacing"));
    assert_eq!(paragraph(&import, 0).properties.spacing, None);
}

#[test]
fn styles_part_unmapped_constructs_are_reported() {
    let styles = br#"<w:styles xmlns:w="urn:w">
        <w:style w:type="paragraph" w:styleId="A"><w:qFormat/><w:uiPriority w:val="1"/></w:style>
    </w:styles>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import_with_styles(document, styles);
    let feats = features(&import);
    assert!(feats.contains(&"qFormat"));
    assert!(feats.contains(&"uiPriority"));
}

#[test]
fn constructs_outside_the_body_are_reported() {
    let xml = br#"<w:document xmlns:w="urn:w">
        <w:background w:color="FFFFFF"/>
        <w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body>
    </w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"background"));
}

#[test]
fn cdata_text_is_captured() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t><![CDATA[hi & bye]]></w:t></w:r></w:p></w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "hi & bye");
}

#[test]
fn nested_rpr_does_not_drop_following_formatting() {
    // A malformed nested self-closing rPr must not prematurely exit run-property
    // context and silently drop the following bold.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:rPr><w:rPr/><w:b/></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.properties.bold, Some(true));
}

#[test]
fn retention_mode_retains_source_and_marks_unmapped_preserved() {
    // The table structure is modeled; `w:tblStyle` (table-level styling) is not,
    // so it exercises the unmapped-but-preserved disposition in Retention mode.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr>
            <w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let config = ImportConfig {
        mode: ImportMode::Retention,
        ..ImportConfig::default()
    };
    let import = import_main_document_xml(xml, config).unwrap();

    // The source is retained byte-identically (tier-1 byte floor): an unedited
    // document can be reproduced verbatim.
    assert_eq!(
        import.retained_source.as_ref().unwrap().main_document,
        xml.to_vec()
    );

    // Unmapped constructs (here, table styling) are preserved rather than dropped.
    assert!(features(&import).contains(&"tblStyle"));
    assert!(!import.report.entries.is_empty());
    for entry in &import.report.entries {
        assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
        assert_eq!(entry.retention_outcome, RetentionOutcome::Preserved);
    }

    // Semantic mode retains nothing and reports not-retained.
    let semantic = import_main_document_xml(xml, ImportConfig::default()).unwrap();
    assert!(semantic.retained_source.is_none());
    assert!(
        semantic
            .report
            .entries
            .iter()
            .all(|entry| entry.retention_outcome == RetentionOutcome::NotRetained)
    );
}

#[test]
fn retention_over_the_byte_ceiling_fails_closed() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
    let config = ImportConfig {
        mode: ImportMode::Retention,
        max_text_bytes: xml.len() - 1,
        ..ImportConfig::default()
    };
    assert_eq!(
        import_main_document_xml(xml, config),
        Err(ImportError::LimitExceeded {
            limit: "retained_bytes"
        })
    );
}

#[test]
fn retention_mode_via_package_retains_all_parts_verbatim() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>hi</w:t></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let config = ImportConfig {
        mode: ImportMode::Retention,
        ..ImportConfig::default()
    };
    let import = import_package(&mut package, config).unwrap();
    let retained = import.retained_source.unwrap();
    assert_eq!(retained.main_document, document.to_vec());
    // Every admitted part is retained byte-identically.
    assert_eq!(
        retained.parts.get("word/document.xml").map(Vec::as_slice),
        Some(document.as_slice())
    );
    assert_eq!(
        retained.parts.get("[Content_Types].xml").map(Vec::as_slice),
        Some(content_types.as_slice())
    );
    assert!(retained.parts.contains_key("_rels/.rels"));
}

#[test]
fn real_producer_libreoffice_document_imports_expected_text() {
    // A real LibreOffice-produced .docx (styles, sectPr, unicode). Locks in
    // realistic-import text extraction in CI (no soffice needed); the harness
    // separately confirms this matches LibreOffice's own text.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-libreoffice.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let texts = nonempty_block_texts(&import);

    assert_eq!(
        texts,
        vec![
            "OpenDoc Fidelity Sample",
            "The quick brown fox jumps over the lazy dog.",
            "Formatting: bold, italic, underline.",
            "Unicode: Cafe, resume, naive, 日本語, العربية, emoji family.",
        ]
    );
}

#[test]
fn real_producer_table_and_lists_model_cells_and_item_text() {
    // Real LibreOffice .docx with a 2x2 table and bullet/numbered lists. The
    // table is now modeled as structure: its cell paragraphs live inside the
    // Table block (not flattened), while recursive text extraction still
    // recovers all cell and list-item content in document order.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let texts = nonempty_block_texts(&import);
    assert_eq!(
        texts,
        vec![
            "Intro paragraph.",
            "R1C1",
            "R1C2",
            "R2C1",
            "R2C2",
            "First item",
            "Second item",
            "Alpha",
            "Beta",
            "Closing paragraph.",
        ]
    );

    // The table is first-class structure in the body: a 2x2 grid of cells,
    // each cell holding its own paragraph. It is no longer reported as unmapped.
    let table = first_table(&import).expect("table modeled in body");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert!(
        !import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "tbl"),
        "modeled table must not be reported as unmapped"
    );
}

#[test]
fn numbering_reference_resolves_to_a_definition() {
    let numbering = br#"<w:numbering xmlns:w="urn:w">
        <w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/>
            <w:numFmt w:val="bullet"/></w:lvl></w:abstractNum>
        <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
    </w:numbering>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
            <w:r><w:t>item</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_numbering(document, numbering);

    let reference = paragraph(&import, 0).properties.numbering.unwrap();
    assert_eq!(reference.level, 0);
    let definitions = import.document.definitions();
    assert_eq!(definitions.abstract_numbering.len(), 1);
    assert_eq!(definitions.numbering.len(), 1);
    assert!(definitions.numbering.get(&reference.instance).is_some());
    // numFmt is unmapped level detail -> reported.
    assert!(features(&import).contains(&"numFmt"));
}

#[test]
fn dangling_numbering_reference_is_reported_not_emitted() {
    let numbering = br#"<w:numbering xmlns:w="urn:w"/>"#;
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="99"/></w:numPr></w:pPr>
            <w:r><w:t>x</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import_with_numbering(document, numbering);
    assert_eq!(paragraph(&import, 0).properties.numbering, None);
    assert!(features(&import).contains(&"numPr"));
}

#[test]
fn body_level_section_geometry_is_mapped() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>x</w:t></w:r></w:p>
        <w:sectPr>
            <w:pgSz w:w="12240" w:h="15840"/>
            <w:pgMar w:top="1440" w:bottom="1440" w:left="1800" w:right="1800"/>
            <w:cols w:num="2"/>
        </w:sectPr>
    </w:body></w:document>"#;
    let import = import(xml);
    let sections = &import.document.definitions().sections;
    assert_eq!(sections.len(), 1);
    let section = &sections[0];
    assert_eq!(section.page_size.width_twips, 12240);
    assert_eq!(section.page_size.height_twips, 15840);
    assert_eq!(section.page_margins.start_twips, 1800); // w:left -> start
    assert_eq!(section.page_margins.end_twips, 1800); // w:right -> end
    assert_eq!(section.columns.count, 2);
    // sectPr is now mapped, so it is no longer reported.
    assert!(!features(&import).contains(&"sectPr"));
}

#[test]
fn image_relationships_map_to_media_references() {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:drawing/></w:r></w:p></w:body></w:document>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", rels.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", document_rels.as_slice()),
        ("word/media/image1.png", b"PNGDATA".as_slice()),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let package_bytes = writer.finish().unwrap().into_inner();

    let mut package =
        DocxPackage::open(&package_bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let media = &import.document.definitions().media;
    assert_eq!(media.len(), 1);
    let (_, reference) = media.iter().next().unwrap();
    assert_eq!(reference.relationship_id, "rId7");
    assert_eq!(reference.media_type, "image/png");
    assert_eq!(reference.part_name, "word/media/image1.png");
}

/// Builds a minimal admitted DOCX package from a main-document body, the
/// main-document relationships, and any extra parts (e.g. a media binary).
fn build_package(document: &[u8], document_rels: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut write = |name: &str, bytes: &[u8]| {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    };
    write("[Content_Types].xml", content_types);
    write("_rels/.rels", rels);
    write("word/document.xml", document);
    write("word/_rels/document.xml.rels", document_rels);
    for (name, bytes) in extra {
        write(name, bytes);
    }
    writer.finish().unwrap().into_inner()
}

fn import_bytes(bytes: &[u8]) -> Import {
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    import_package(&mut package, ImportConfig::default()).unwrap()
}

const IMAGE_REL: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;

const DRAWING_INLINE: &str = r#"<w:drawing><wp:inline><wp:extent cx="9525" cy="19050"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#;

#[test]
fn inline_drawing_with_embed_maps_to_a_drawing_node() {
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:r>{DRAWING_INLINE}</w:r></w:p></w:body></w:document>"#
    );
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), IMAGE_REL, &media));

    let (media_id, _) = import.document.definitions().media.iter().next().unwrap();
    let inlines = &paragraph(&import, 0).inlines;
    assert_eq!(inlines.len(), 1);
    let InlineNode::Drawing(drawing) = &inlines[0] else {
        panic!("expected a drawing, got {:?}", inlines[0]);
    };
    assert_eq!(drawing.media, *media_id);
    let extent = drawing.extent.expect("extent parsed");
    assert_eq!(extent.width_emu, 9525);
    assert_eq!(extent.height_emu, 19050);
    // A resolved, fully-modeled inline drawing is mapped, not reported.
    assert!(!features(&import).contains(&"drawing"));
}

#[test]
fn drawing_with_a_dangling_embed_is_reported_and_dropped() {
    // The blip references rId9, which has no relationship: no media, no node.
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:a="urn:a"><w:body><w:p><w:r><w:drawing><a:blip r:embed="rId9"/></w:drawing></w:r></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    assert!(features(&import).contains(&"drawing"));
    assert!(
        paragraph(&import, 0)
            .inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Drawing(_)))
    );
}

const HYPERLINK_REL: &[u8] = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/docs" TargetMode="External"/></Relationships>"#;

#[test]
fn external_hyperlink_maps_to_a_hyperlink_node() {
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink r:id="rId8" w:tooltip="see docs"><w:r><w:t>docs</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let import = import_bytes(&build_package(document.as_bytes(), HYPERLINK_REL, &[]));

    let inlines = &paragraph(&import, 0).inlines;
    assert_eq!(inlines.len(), 1);
    let InlineNode::Hyperlink(link) = &inlines[0] else {
        panic!("expected a hyperlink, got {:?}", inlines[0]);
    };
    assert_eq!(
        link.target,
        HyperlinkTarget::External(casual_doc_model::v1::ExternalTarget {
            url: "https://example.com/docs".to_owned(),
        })
    );
    assert_eq!(link.tooltip.as_deref(), Some("see docs"));
    let InlineNode::Run(run) = &link.inlines[0] else {
        panic!("expected a run child");
    };
    assert_eq!(run.text, "docs");
    assert!(!features(&import).contains(&"hyperlink"));
}

#[test]
fn internal_anchor_hyperlink_maps_to_a_hyperlink_node() {
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink w:anchor="top"><w:r><w:t>top</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    assert_eq!(
        link.target,
        HyperlinkTarget::Internal(casual_doc_model::v1::InternalTarget {
            anchor: "top".to_owned(),
        })
    );
}

#[test]
fn unresolved_hyperlink_is_reported_and_its_text_flattened() {
    // r:id="rId9" has no relationship: the link is reported, but its text is
    // preserved as flat runs in the paragraph (never dropped).
    let document = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p><w:hyperlink r:id="rId9"><w:r><w:t>orphan</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
    let empty_rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
    let import = import_bytes(&build_package(document.as_bytes(), empty_rels, &[]));

    assert!(features(&import).contains(&"hyperlink"));
    let inlines = &paragraph(&import, 0).inlines;
    assert!(
        inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Hyperlink(_)))
    );
    let InlineNode::Run(run) = &inlines[0] else {
        panic!("expected the flattened run");
    };
    assert_eq!(run.text, "orphan");
}

#[test]
fn image_inside_a_hyperlink_becomes_a_drawing_child() {
    // Proves the push_segment router: a drawing inside a hyperlink is captured
    // by the link, not the paragraph.
    let document = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body><w:p><w:hyperlink r:id="rId8"><w:r>{DRAWING_INLINE}</w:r></w:hyperlink></w:p></w:body></w:document>"#
    );
    // Two relationships: the hyperlink (rId8) and the image (rId7).
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document.as_bytes(), rels, &media));

    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    assert!(matches!(link.inlines[0], InlineNode::Drawing(_)));
}

#[test]
fn real_producer_table_merges_map_grid_span_and_vertical_merge() {
    use casual_doc_model::v1::VerticalMerge;

    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();

    let table = first_table(&import).expect("table modeled in body");
    // Three grid columns (w:tblGrid), preserved in order.
    assert_eq!(
        table
            .grid
            .iter()
            .map(|column| column.width_twips)
            .collect::<Vec<_>>(),
        vec![Some(1636), Some(391), Some(436)],
    );
    assert_eq!(table.rows.len(), 3);

    // Row 1, cell 1 spans two grid columns (w:gridSpan) and carries a dxa width.
    let spanning = &table.rows[0].cells[0];
    assert_eq!(spanning.properties.grid_span, Some(2));
    assert_eq!(spanning.properties.width_twips, Some(2027));

    // Row 2 opens a vertical merge; row 3 continues it (w:vMerge).
    assert_eq!(
        table.rows[1].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(
        table.rows[2].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );

    // Cell text is recoverable from the modeled structure.
    let texts = nonempty_block_texts(&import);
    assert!(texts.contains(&"Spans two columns".to_owned()));
    assert!(texts.contains(&"Spans two rows".to_owned()));

    // The modeled table is not reported as unmapped structure.
    assert!(!features(&import).contains(&"tbl"));
}

#[test]
fn nested_tables_nest_and_percent_cell_width_is_reported() {
    // A table whose cell contains a nested table; the outer cell also carries a
    // percentage width (w:tcW type="pct"), which is not modeled and is reported.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl>
            <w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid>
            <w:tr><w:tc>
                <w:tcPr><w:tcW w:w="2500" w:type="pct"/></w:tcPr>
                <w:p><w:r><w:t>outer</w:t></w:r></w:p>
                <w:tbl><w:tr><w:tc><w:p><w:r><w:t>inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            </w:tc></w:tr>
        </w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);

    let outer = first_table(&import).expect("outer table modeled");
    assert_eq!(outer.grid.len(), 1);
    assert_eq!(outer.grid[0].width_twips, Some(5000));
    let cell = &outer.rows[0].cells[0];
    // Percentage width is not modeled (dxa only); it stays None and is reported.
    assert_eq!(cell.properties.width_twips, None);
    assert!(features(&import).contains(&"tcW"));

    // The cell holds its paragraph and then a nested table (document order).
    assert!(matches!(cell.blocks[0], BlockNode::Paragraph(_)));
    let BlockNode::Table(inner) = &cell.blocks[1] else {
        panic!("expected a nested table as the cell's second block");
    };
    assert_eq!(inner.rows.len(), 1);
    let texts = nonempty_block_texts(&import);
    assert_eq!(texts, vec!["outer".to_owned(), "inner".to_owned()]);
}

#[test]
fn tables_nested_past_depth_bound_flatten_without_data_loss() {
    // Regression (adversarial review): a table nested past MAX_TABLE_DEPTH must
    // not corrupt the enclosing table. The over-depth subtree is suppressed and
    // its paragraph text flattens into the innermost modeled cell — no cell
    // content is silently dropped, and the model stays valid and bounded.
    use casual_doc_model::v1::MAX_TABLE_DEPTH;

    let depth = MAX_TABLE_DEPTH + 1; // 33: one level past the bound
    let mut xml = String::from(r#"<w:document xmlns:w="urn:w"><w:body>"#);
    for i in 0..depth {
        xml.push_str("<w:tbl><w:tr><w:tc>");
        xml.push_str(&format!("<w:p><w:r><w:t>L{i}</w:t></w:r></w:p>"));
    }
    for _ in 0..depth {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:body></w:document>");

    // Imports to a valid, bounded model (validation would reject depth > bound).
    let import = import(xml.as_bytes());

    // Every level's text survives — the over-depth level flattens up, it is not
    // dropped and it does not overwrite the enclosing cell's own paragraph.
    let texts = nonempty_block_texts(&import);
    for i in 0..depth {
        assert!(texts.contains(&format!("L{i}")), "lost cell text L{i}");
    }
    // The over-depth table is reported (dispositioned), not silently absorbed.
    assert!(features(&import).contains(&"tbl"));
}

#[test]
fn malformed_table_inside_a_paragraph_does_not_desync_the_stack() {
    // A `<w:tbl>` nested inside a `<w:p>` is malformed (valid OOXML makes tables
    // and paragraphs block-level siblings). It must be suppressed WITHOUT
    // consuming the enclosing real table's `</w:tbl>`: the outer table and the
    // surrounding cell text must both survive. Regression for the adversarial
    // review's start/end asymmetry finding.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc>
            <w:p><w:tbl/><w:r><w:t>keep</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>
    </w:body></w:document>"#;
    let import = import(xml);

    // The real outer table is not dropped by the stray inner `</w:tbl>`.
    let table = first_table(&import).expect("outer table survives");
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].cells.len(), 1);
    // Cell text after the malformed table is preserved.
    assert!(nonempty_block_texts(&import).contains(&"keep".to_owned()));
    assert!(features(&import).contains(&"tbl"));
}

/// Recursively collects run text within an inline (into hyperlinks and fields).
fn inline_text(inline: &InlineNode, out: &mut String) {
    match inline {
        InlineNode::Run(run) => out.push_str(&run.text),
        InlineNode::Hyperlink(link) => link.inlines.iter().for_each(|c| inline_text(c, out)),
        InlineNode::Field(field) => field.inlines.iter().for_each(|c| inline_text(c, out)),
        _ => {}
    }
}

#[test]
fn simple_field_maps_instruction_and_cached_result() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:fldSimple w:instr=" PAGE "><w:r><w:t>7</w:t></w:r></w:fldSimple></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "7");
}

#[test]
fn complex_field_maps_instrtext_and_result_runs() {
    // begin -> instrText -> separate -> result -> end, spread across runs.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> REF _Ref1 \h </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>Section 2</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    assert_eq!(para.inlines.len(), 1);
    let InlineNode::Field(field) = &para.inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " REF _Ref1 \\h ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "Section 2");
}

#[test]
fn complex_field_without_separate_has_empty_result() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> TIME </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " TIME ");
    assert!(field.inlines.is_empty());
}

#[test]
fn complex_field_missing_end_is_flushed_without_loss() {
    // A begin/separate with no end (malformed): the field is flushed at paragraph
    // close so its cached text is not dropped.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> PAGE </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>3</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a flushed field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "3");
}

#[test]
fn field_inside_a_hyperlink_flattens_and_is_reported() {
    // A field is not opened inside a hyperlink (wrapper-in-wrapper): its result
    // text flattens into the hyperlink and the nesting is reported.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:hyperlink w:anchor="top">
            <w:fldSimple w:instr=" PAGE "><w:r><w:t>9</w:t></w:r></w:fldSimple>
        </w:hyperlink></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Hyperlink(link) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a hyperlink");
    };
    // No nested field: the "9" flattened into the link as a plain run.
    assert!(
        link.inlines
            .iter()
            .all(|inline| !matches!(inline, InlineNode::Field(_)))
    );
    let mut text = String::new();
    link.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "9");
    assert!(features(&import).contains(&"fldSimple"));
}

#[test]
fn complex_field_display_run_before_separate_is_not_lost() {
    // Regression (adversarial review): a display run appearing before `separate`
    // (or with a missing/duplicated delimiter) must not be silently dropped.
    // Here `separate` is omitted, so "5" would previously vanish; it is now kept
    // in the cached result.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText> PAGE </w:instrText></w:r>
            <w:r><w:t>5</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let InlineNode::Field(field) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected a field");
    };
    assert_eq!(field.instruction, " PAGE ");
    let mut text = String::new();
    field.inlines.iter().for_each(|c| inline_text(c, &mut text));
    assert_eq!(text, "5", "pre-separate display text must be preserved");
}

#[test]
fn nested_begin_keeps_cached_result_text() {
    // A spurious extra `begin` raises the field depth; the cached result "5"
    // must still be preserved (previously dropped because in_result stayed false).
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText>PAGE</w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>5</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // Whatever the exact structure, the display text "5" must survive somewhere.
    let mut text = String::new();
    for inline in &paragraph(&import, 0).inlines {
        inline_text(inline, &mut text);
    }
    assert!(text.contains('5'), "cached result text must not be lost");
    // The nested begin is reported.
    assert!(features(&import).contains(&"fldChar"));
}

#[test]
fn orphan_instr_text_is_reported_not_silently_dropped() {
    // An `instrText` with no enclosing field is field code we do not model; it
    // must be reported (dispositioned), never silently discarded.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:instrText>MERGEFIELD Name</w:instrText></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(features(&import).contains(&"instrText"));
}

// ---- text boxes and alternate content ------------------------------------

fn tb_block_text(blocks: &[BlockNode]) -> String {
    fn walk_blocks(blocks: &[BlockNode], out: &mut String) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(p) => p.inlines.iter().for_each(|i| walk_inline(i, out)),
                BlockNode::Table(t) => t
                    .rows
                    .iter()
                    .for_each(|r| r.cells.iter().for_each(|c| walk_blocks(&c.blocks, out))),
            }
        }
    }
    fn walk_inline(inline: &InlineNode, out: &mut String) {
        match inline {
            InlineNode::Run(r) => out.push_str(&r.text),
            InlineNode::Hyperlink(l) => l.inlines.iter().for_each(|c| walk_inline(c, out)),
            InlineNode::Field(f) => f.inlines.iter().for_each(|c| walk_inline(c, out)),
            InlineNode::TextBox(b) => walk_blocks(&b.blocks, out),
            _ => {}
        }
    }
    let mut out = String::new();
    walk_blocks(blocks, &mut out);
    out
}

fn find_textbox(inlines: &[InlineNode]) -> Option<&casual_doc_model::v1::TextBox> {
    inlines.iter().find_map(|inline| match inline {
        InlineNode::TextBox(text_box) => Some(text_box),
        _ => None,
    })
}

#[test]
fn drawingml_text_box_is_modeled_and_does_not_corrupt_the_paragraph() {
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p>
            <w:r><w:t>Before</w:t></w:r>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
            <w:r><w:t>After</w:t></w:r>
        </w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert_eq!(import.document.body().len(), 1);
    let para = paragraph(&import, 0);
    let outer: String = para
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(outer, "BeforeAfter");
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Boxed");
}

#[test]
fn image_beside_a_text_box_in_the_same_drawing_is_not_dropped() {
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps" xmlns:pic="urn:pic"><w:body>
        <w:p><w:r><w:drawing><wp:inline><wp:extent cx="100" cy="100"/><a:graphic><a:graphicData><wps:wsp>
            <a:blipFill><a:blip r:embed="rId7"/></a:blipFill>
            <wps:txbx><w:txbxContent><w:p><w:r><w:t>Caption</w:t></w:r></w:p></w:txbxContent></wps:txbx>
        </wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    let para = paragraph(&import, 0);
    assert!(
        para.inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Drawing(_))),
        "enclosing drawing image must survive"
    );
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Caption");
}

#[test]
fn alternate_content_selects_one_branch_and_does_not_duplicate() {
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:mc="urn:mc" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps" xmlns:v="urn:v"><w:body>
        <w:p><w:r><mc:AlternateContent>
            <mc:Choice Requires="wps"><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>
            <mc:Fallback><w:pict><v:shape><v:textbox>
                <w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent>
            </v:textbox></v:shape></w:pict></mc:Fallback>
        </mc:AlternateContent></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let para = paragraph(&import, 0);
    let boxes = para
        .inlines
        .iter()
        .filter(|i| matches!(i, InlineNode::TextBox(_)))
        .count();
    assert_eq!(
        boxes, 1,
        "alternate content must not duplicate the text box"
    );
    let text_box = find_textbox(&para.inlines).expect("text box modeled");
    assert_eq!(tb_block_text(&text_box.blocks), "Boxed");
    assert!(features(&import).contains(&"Fallback"));
}

#[test]
fn deep_tables_with_a_text_box_of_tables_import_without_hard_failure() {
    // Regression (review major): a text box restarts the table-depth budget in
    // both importer and model, so tables outside and inside a box do not sum
    // past the bound and abort the whole import.
    let mut xml = String::from(
        r#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>"#,
    );
    let outer = 20;
    for _ in 0..outer {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent>");
    let inner = 13;
    for _ in 0..inner {
        xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    xml.push_str("<w:p><w:r><w:t>deep</w:t></w:r></w:p>");
    for _ in 0..inner {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
    for _ in 0..outer {
        xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    xml.push_str("</w:body></w:document>");
    // Must import successfully (no ImportError::Model from over-summed depth).
    let import = import(xml.as_bytes());
    assert!(!import.document.body().is_empty());
}

#[test]
fn sect_pr_inside_a_text_box_is_not_a_document_section() {
    // Regression (review minor): a bare sectPr inside a text box must not push a
    // phantom SectionBoundary into the document's sections.
    let xml = br#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body>
        <w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx><w:txbxContent>
            <w:sectPr><w:pgSz w:w="100" w:h="100"/></w:sectPr>
            <w:p><w:r><w:t>x</w:t></w:r></w:p>
        </w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    assert!(
        import.document.definitions().sections.is_empty(),
        "sectPr inside a text box must not create a document section"
    );
    assert!(features(&import).contains(&"sectPr"));
}

// ---- footnotes / endnotes ------------------------------------------------

#[test]
fn footnote_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>Text</w:t></w:r><w:r><w:footnoteReference w:id="2"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:footnote w:id="-1" w:type="separator"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
        <w:footnote w:id="2"><w:p><w:r><w:t>The footnote body.</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);

    // The body run carries a note reference resolving to the footnote definition.
    let note_ref = paragraph(&import, 0).inlines.iter().find_map(|i| match i {
        InlineNode::NoteReference(n) => Some(n),
        _ => None,
    });
    let note_ref = note_ref.expect("footnote reference modeled");
    assert_eq!(note_ref.kind, casual_doc_model::v1::NoteKind::Footnote);
    // The footnote definition holds its body text (the separator note is skipped).
    assert_eq!(import.document.definitions().footnotes.len(), 1);
    let note = import
        .document
        .definitions()
        .footnotes
        .get(&note_ref.note)
        .expect("footnote definition resolves");
    assert_eq!(tb_block_text(&note.blocks), "The footnote body.");
}

#[test]
fn dangling_footnote_reference_is_reported_not_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="99"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:footnote w:id="2"><w:p><w:r><w:t>body</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    // The reference to a missing footnote id is reported, not modeled.
    assert!(
        !paragraph(&import, 0)
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::NoteReference(_)))
    );
    assert!(features(&import).contains(&"footnoteReference"));
}

#[test]
fn endnote_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:endnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let endnotes = br#"<w:endnotes xmlns:w="urn:w">
        <w:endnote w:id="1"><w:p><w:r><w:t>An endnote.</w:t></w:r></w:p></w:endnote>
    </w:endnotes>"#;
    let import = import_with_notes(document, None, Some(endnotes));
    assert_eq!(import.document.definitions().endnotes.len(), 1);
    let note_ref = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::NoteReference(n) => Some(n),
            _ => None,
        })
        .expect("endnote reference modeled");
    assert_eq!(note_ref.kind, casual_doc_model::v1::NoteKind::Endnote);
    let note = import
        .document
        .definitions()
        .endnotes
        .get(&note_ref.note)
        .unwrap();
    assert_eq!(tb_block_text(&note.blocks), "An endnote.");
}

#[test]
fn footnote_containing_a_text_box_preserves_its_content() {
    // Regression (review major): closing a note must unwind text-box frames and
    // finish the restored paragraph, so a note's text box content is not dropped.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnotes =
        br#"<w:footnotes xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps">
        <w:footnote w:id="1"><w:p>
            <w:r><w:t>note </w:t></w:r>
            <w:r><w:drawing><wp:inline><a:graphic><a:graphicData><wps:wsp><wps:txbx>
                <w:txbxContent><w:p><w:r><w:t>boxed</w:t></w:r></w:p></w:txbxContent>
            </wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>
        </w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    let note = import
        .document
        .definitions()
        .footnotes
        .iter()
        .next()
        .map(|(_, n)| n)
        .expect("footnote");
    // Both the note run text and the text box text survive.
    assert_eq!(tb_block_text(&note.blocks), "note boxed");
}

#[test]
fn stray_body_inside_a_notes_part_is_reported_not_silently_dropped() {
    // Regression (review minor): a <w:body> in a notes part must not model then
    // discard content; it is reported instead.
    let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p/></w:body></w:document>"#;
    let footnotes = br#"<w:footnotes xmlns:w="urn:w">
        <w:body><w:p><w:r><w:t>orphan</w:t></w:r></w:p></w:body>
        <w:footnote w:id="1"><w:p><w:r><w:t>real</w:t></w:r></w:p></w:footnote>
    </w:footnotes>"#;
    let import = import_with_notes(document, Some(footnotes), None);
    // Only the real footnote is modeled; the stray body is reported.
    assert_eq!(import.document.definitions().footnotes.len(), 1);
    assert!(features(&import).contains(&"body"));
}

// ---- headers / footers ---------------------------------------------------

fn import_with_header_footer(
    document: &[u8],
    headers: &[(&str, &[u8])],
    footers: &[(&str, &[u8])],
) -> Import {
    let headers: Vec<(String, crate::PartSources)> = headers
        .iter()
        .map(|(id, xml)| ((*id).to_owned(), part_sources(xml)))
        .collect();
    let footers: Vec<(String, crate::PartSources)> = footers
        .iter()
        .map(|(id, xml)| ((*id).to_owned(), part_sources(xml)))
        .collect();
    import_with_sources(
        document,
        None,
        None,
        None,
        None,
        &headers,
        &footers,
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap()
}

#[test]
fn header_reference_and_body_are_modeled() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/>
            <w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>Page header</w:t></w:r></w:p></w:hdr>"#;
    let import = import_with_header_footer(document, &[("rId2", header)], &[]);

    // The header part is modeled and the section references it by kind.
    assert_eq!(import.document.definitions().headers.len(), 1);
    let section = &import.document.definitions().sections[0];
    assert_eq!(section.headers.len(), 1);
    assert_eq!(
        section.headers[0].kind,
        casual_doc_model::v1::HeaderFooterKind::Default
    );
    let hf = import
        .document
        .definitions()
        .headers
        .get(&section.headers[0].reference)
        .expect("header definition resolves");
    assert_eq!(tb_block_text(&hf.blocks), "Page header");
}

#[test]
fn footer_reference_of_first_kind_is_modeled() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:footerReference w:type="first" r:id="rId3"/></w:sectPr>
    </w:body></w:document>"#;
    let footer = br#"<w:ftr xmlns:w="urn:w"><w:p><w:r><w:t>Footer</w:t></w:r></w:p></w:ftr>"#;
    let import = import_with_header_footer(document, &[], &[("rId3", footer)]);
    let section = &import.document.definitions().sections[0];
    assert_eq!(section.footers.len(), 1);
    assert_eq!(
        section.footers[0].kind,
        casual_doc_model::v1::HeaderFooterKind::First
    );
    let hf = import
        .document
        .definitions()
        .footers
        .get(&section.footers[0].reference)
        .unwrap();
    assert_eq!(tb_block_text(&hf.blocks), "Footer");
}

#[test]
fn header_reference_to_a_missing_part_is_reported() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId9"/></w:sectPr>
    </w:body></w:document>"#;
    // No header part with rId9 is supplied.
    let import = import_with_header_footer(document, &[], &[]);
    let section = &import.document.definitions().sections[0];
    assert!(section.headers.is_empty());
    assert!(features(&import).contains(&"headerReference"));
}

#[test]
fn real_producer_header_footer_fixture_models_both() {
    // End-to-end on a real LibreOffice .docx: its sectPr references a header and
    // a footer part, both of which are modeled with their content.
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
    let mut package = DocxPackage::open(bytes, casual_doc_ooxml::PackageLimits::default()).unwrap();
    let import = import_package(&mut package, ImportConfig::default()).unwrap();
    let defs = import.document.definitions();
    assert_eq!(defs.headers.len(), 1, "header part modeled");
    assert_eq!(defs.footers.len(), 1, "footer part modeled");
    // The body's section references them.
    let section = defs.sections.last().expect("a section boundary");
    assert!(!section.headers.is_empty(), "section references a header");
    assert!(!section.footers.is_empty(), "section references a footer");
}

#[test]
fn stray_sect_pr_and_body_inside_a_header_part_are_reported() {
    // Regression (review): a stray w:sectPr / w:body inside a header part must be
    // reported (not build a discarded phantom section or set in_body silently).
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w">
        <w:p><w:r><w:t>H</w:t></w:r></w:p>
        <w:sectPr><w:pgSz w:w="100" w:h="100"/></w:sectPr>
        <w:body><w:p><w:r><w:t>orphan</w:t></w:r></w:p></w:body>
    </w:hdr>"#;
    let import = import_with_header_footer(document, &[("rId2", header)], &[]);
    // The document has exactly one real section (the body's); the header's stray
    // sectPr did not add a phantom one.
    assert_eq!(import.document.definitions().sections.len(), 1);
    // Both stray constructs are reported.
    assert!(features(&import).contains(&"sectPr"));
    assert!(features(&import).contains(&"body"));
}

// ---- legacy VML pictures -------------------------------------------------

#[test]
fn vml_pict_image_becomes_a_drawing() {
    // A legacy VML picture (w:pict > v:imagedata@r:id) resolves through the same
    // media table as a DrawingML picture and becomes a Drawing.
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body>
        <w:p><w:r><w:pict><v:shape style="width:10pt"><v:imagedata r:id="rId5"/></v:shape></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    let drawing = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|i| match i {
            InlineNode::Drawing(d) => Some(d),
            _ => None,
        })
        .expect("VML image modeled as a Drawing");
    // VML sizes in CSS, not EMU, so no extent is captured.
    assert!(drawing.extent.is_none());
}

#[test]
fn vml_pict_with_unresolved_image_is_reported() {
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:v="urn:v"><w:body>
        <w:p><w:r><w:pict><v:shape><v:imagedata r:id="rId9"/></v:shape></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
    let import = import_bytes(&build_package(document, rels, &[]));
    assert!(
        !paragraph(&import, 0)
            .inlines
            .iter()
            .any(|i| matches!(i, InlineNode::Drawing(_)))
    );
    assert!(features(&import).contains(&"pict"));
}

// ---- media / hyperlinks inside extra parts -------------------------------

fn image_source(rid: &str, part: &str) -> crate::MediaSource {
    crate::MediaSource {
        relationship_id: rid.to_owned(),
        media_type: "image/png".to_owned(),
        part_name: part.to_owned(),
    }
}

#[test]
fn image_inside_a_footnote_is_modeled_via_the_notes_part_relationships() {
    // A footnote body with a DrawingML picture whose r:embed resolves through the
    // footnotes part's OWN image relationship.
    let document = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:footnoteReference w:id="1"/></w:r></w:p>
    </w:body></w:document>"#;
    let footnote_xml = format!(
        r#"<w:footnotes xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic">
            <w:footnote w:id="1"><w:p><w:r>{DRAWING_INLINE}</w:r></w:p></w:footnote>
        </w:footnotes>"#
    );
    let footnotes = crate::PartSources {
        xml: footnote_xml.into_bytes(),
        images: vec![image_source("rId7", "word/media/image1.png")],
        hyperlinks: std::collections::BTreeMap::new(),
    };
    let import = import_with_sources(
        document,
        None,
        None,
        Some(&footnotes),
        None,
        &[],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();

    // The footnote's block content contains a Drawing referencing shared media.
    let note = import
        .document
        .definitions()
        .footnotes
        .iter()
        .next()
        .map(|(_, n)| n)
        .unwrap();
    let has_drawing = note.blocks.iter().any(|b| matches!(
        b, BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Drawing(_)))
    ));
    assert!(has_drawing, "image inside footnote modeled as a Drawing");
    assert_eq!(
        import.document.definitions().media.len(),
        1,
        "note-part image in the media table"
    );
}

#[test]
fn external_hyperlink_inside_a_header_is_modeled_via_the_header_part_relationships() {
    let document = br#"<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body>
        <w:p><w:r><w:t>Body.</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr>
    </w:body></w:document>"#;
    let header = br#"<w:hdr xmlns:w="urn:w" xmlns:r="urn:r">
        <w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>site</w:t></w:r></w:hyperlink></w:p>
    </w:hdr>"#;
    let mut hyperlinks = std::collections::BTreeMap::new();
    hyperlinks.insert("rIdLink".to_owned(), "https://example.com/".to_owned());
    let header_part = crate::PartSources {
        xml: header.to_vec(),
        images: Vec::new(),
        hyperlinks,
    };
    let import = import_with_sources(
        document,
        None,
        None,
        None,
        None,
        &[("rId2".to_owned(), header_part)],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        ImportConfig::default(),
    )
    .unwrap();

    let hf = import
        .document
        .definitions()
        .headers
        .iter()
        .next()
        .map(|(_, h)| h)
        .unwrap();
    let has_link = hf.blocks.iter().any(|b| matches!(
        b, BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Hyperlink(_)))
    ));
    assert!(has_link, "external hyperlink inside a header is modeled");
}

#[test]
fn two_relationships_to_the_same_image_get_distinct_media_ids() {
    // Backward-compat (review): media is allocated per relationship (no dedup),
    // so a document referencing one image part from two relationships yields two
    // media entries, exactly as before extra-part media aggregation.
    let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body>
        <w:p><w:r><w:pict><v:imagedata r:id="rId5"/></w:pict></w:r>
             <w:r><w:pict><v:imagedata r:id="rId8"/></w:pict></w:r></w:p>
    </w:body></w:document>"#;
    let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let media = [("word/media/image1.png", b"PNGDATA".as_slice())];
    let import = import_bytes(&build_package(document, rels, &media));
    assert_eq!(
        import.document.definitions().media.len(),
        2,
        "one media entry per relationship (no dedup)"
    );
}

// ---- ruby (phonetic guides) ----------------------------------------------

#[test]
fn ruby_keeps_base_text_in_order_and_reports_the_annotation() {
    // Audit fix: the base reads in document order; the annotation (phonetic
    // guide) is dropped and reported, not merged before the base.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:t>A</w:t></w:r>
             <w:r><w:ruby><w:rubyPr/>
                 <w:rt><w:r><w:t>anno</w:t></w:r></w:rt>
                 <w:rubyBase><w:r><w:t>base</w:t></w:r></w:rubyBase>
             </w:ruby></w:r>
             <w:r><w:t>B</w:t></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    // The paragraph reads "A" + "base" + "B" — annotation is not present/merged.
    let text: String = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "AbaseB");
    assert!(
        !text.contains("anno"),
        "annotation must not be modeled as base text"
    );
    assert!(
        features(&import).contains(&"rt"),
        "ruby annotation is reported"
    );
}

#[test]
fn nested_ruby_annotation_does_not_leak_base_text() {
    // Regression (review): a nested w:rt (valid OOXML) must not clear the outer
    // annotation early via a bool; a counter keeps all annotation text dropped.
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:p><w:r><w:ruby>
            <w:rt>
                <w:r><w:ruby>
                    <w:rt><w:r><w:t>inner</w:t></w:r></w:rt>
                    <w:rubyBase><w:r><w:t>ib</w:t></w:r></w:rubyBase>
                </w:ruby></w:r>
                <w:r><w:t>outer-anno</w:t></w:r>
            </w:rt>
            <w:rubyBase><w:r><w:t>BASE</w:t></w:r></w:rubyBase>
        </w:ruby></w:r></w:p>
    </w:body></w:document>"#;
    let import = import(xml);
    let text: String = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|i| match i {
            InlineNode::Run(r) => Some(r.text.as_str()),
            _ => None,
        })
        .collect();
    // Only the outermost base is kept; no annotation fragment leaks in.
    assert_eq!(text, "BASE");
    assert!(!text.contains("anno") && !text.contains("inner") && !text.contains("ib"));
}
