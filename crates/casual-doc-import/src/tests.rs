use casual_doc_model::v1::{
    Alignment, BlockNode, Break, BreakKind, Color, InlineNode, Paragraph, RgbColor, StyleKind,
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
    import_with_sources(document, Some(styles), ImportConfig::default()).unwrap()
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
    let BlockNode::Paragraph(paragraph) = &import.document.body()[index];
    paragraph
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
fn unsupported_constructs_are_dispositioned_and_cell_text_is_flattened() {
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:sectPr/>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        </w:body></w:document>"#;
    let import = import(xml);
    // The table cell paragraph is flattened into the body (R4).
    assert_eq!(import.document.body().len(), 1);
    let InlineNode::Run(run) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected run");
    };
    assert_eq!(run.text, "cell");
    // Table/section structure is reported, ordered by feature name.
    let features: Vec<&str> = import
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(features.contains(&"sectPr"));
    assert!(features.contains(&"tbl"));
    assert!(features.windows(2).all(|pair| pair[0] < pair[1]));
    for entry in &import.report.entries {
        assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
        assert_eq!(entry.retention_outcome, RetentionOutcome::NotRetained);
    }
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
    let xml = br#"<w:document xmlns:w="urn:w"><w:body>
        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
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

    // Unmapped constructs are now preserved rather than dropped.
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "tbl")
    );
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

    let texts: Vec<String> = import
        .document
        .body()
        .iter()
        .map(|BlockNode::Paragraph(paragraph)| {
            paragraph
                .inlines
                .iter()
                .filter_map(|inline| match inline {
                    InlineNode::Run(run) => Some(run.text.as_str()),
                    _ => None,
                })
                .collect::<String>()
        })
        .filter(|text| !text.is_empty())
        .collect();

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
