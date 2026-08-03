use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, HyperlinkTarget, InlineNode, RgbColor,
};
use casual_doc_package::CancellationToken;

use crate::{
    ModelOutcome, OdfError, OdfImportLimits, OdfVersion, RetentionOutcome, import_content_xml,
    import_content_xml_with_cancellation,
};

fn content(version: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:xlink="http://www.w3.org/1999/xlink"
 office:version="{version}">
 <office:body><office:text>{body}</office:text></office:body>
</office:document-content>"#
    )
    .into_bytes()
}

fn styled_content(styles: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
 office:version="1.4">
 <office:automatic-styles>{styles}</office:automatic-styles>
 <office:body><office:text>{body}</office:text></office:body>
</office:document-content>"#
    )
    .into_bytes()
}

fn paragraph(import: &crate::OdtImport, index: usize) -> &casual_doc_model::v1::Paragraph {
    let BlockNode::Paragraph(paragraph) = &import.document.body()[index] else {
        panic!("expected paragraph")
    };
    paragraph
}

#[test]
fn core_text_constructs_map_to_valid_normalized_nodes() {
    for (version, expected) in [
        ("1.2", OdfVersion::V1_2),
        ("1.3", OdfVersion::V1_3),
        ("1.4", OdfVersion::V1_4),
    ] {
        let xml = content(version, "<text:p>versioned</text:p>");
        import_content_xml(&xml, expected, OdfImportLimits::default()).unwrap();
    }

    let xml = content(
        "1.4",
        r#"<text:p>Hello <text:span text:style-name="Emphasis">world</text:span><text:s text:c="2"/><text:tab/><text:line-break/>tail &amp; end</text:p><text:h text:outline-level="2">Heading</text:h>"#,
    );
    let import = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    assert_eq!(import.document.body().len(), 2);

    let first = paragraph(&import, 0);
    assert!(matches!(&first.inlines[0], InlineNode::Run(run) if run.text == "Hello world  "));
    assert!(matches!(&first.inlines[1], InlineNode::Tab(_)));
    assert!(matches!(&first.inlines[2], InlineNode::Break(node) if node.kind == BreakKind::Line));
    assert!(matches!(&first.inlines[3], InlineNode::Run(run) if run.text == "tail & end"));
    assert_eq!(paragraph(&import, 1).properties.outline_level, Some(1));
    assert_eq!(paragraph(&import, 1).inlines.len(), 1);

    assert!(import.report.entries.iter().any(|entry| {
        entry.feature == "odf.style.unresolved"
            && entry.occurrences == 1
            && entry.model_outcome == ModelOutcome::Degraded
    }));
}

#[test]
fn automatic_styles_map_and_nested_spans_cascade_deterministically() {
    let xml = styled_content(
        r##"<style:style style:name="P" style:family="paragraph"><style:paragraph-properties fo:text-align="center"/></style:style>
<style:style style:name="T" style:family="text"><style:text-properties fo:font-weight="bold" fo:font-style="italic" style:text-underline-style="solid" style:text-line-through-style="solid" fo:color="#1A2b3C" fo:font-size="10.5pt"/></style:style>
<style:style style:name="Off" style:family="text"><style:text-properties fo:font-weight="normal" style:text-underline-style="none"/></style:style>"##,
        r#"<text:p text:style-name="P">plain <text:span text:style-name="T">styled <text:span text:style-name="Off">off</text:span> after</text:span> end</text:p>"#,
    );
    let first = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let second = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert_eq!(first, second);
    assert!(first.report.entries.is_empty());

    let paragraph = paragraph(&first, 0);
    assert_eq!(paragraph.properties.alignment, Some(Alignment::Center));
    assert_eq!(paragraph.inlines.len(), 5);
    let InlineNode::Run(styled) = &paragraph.inlines[1] else {
        panic!("styled run")
    };
    assert_eq!(styled.properties.bold, Some(true));
    assert_eq!(styled.properties.italic, Some(true));
    assert_eq!(styled.properties.underline, Some(true));
    assert_eq!(styled.properties.strike, Some(true));
    assert_eq!(
        styled.properties.color,
        Some(Color::Rgb(RgbColor {
            r: 0x1a,
            g: 0x2b,
            b: 0x3c,
        }))
    );
    assert_eq!(styled.properties.size_half_points, Some(21));

    let InlineNode::Run(off) = &paragraph.inlines[2] else {
        panic!("explicit-off run")
    };
    assert_eq!(off.properties.bold, Some(false));
    assert_eq!(off.properties.underline, Some(false));
    assert_eq!(off.properties.italic, Some(true));
    assert_eq!(off.properties.strike, Some(true));
    assert_eq!(off.properties.color, styled.properties.color);
    assert_eq!(off.properties.size_half_points, Some(21));
    let InlineNode::Run(after) = &paragraph.inlines[3] else {
        panic!("outer style after nested span")
    };
    assert_eq!(after.properties, styled.properties);
}

#[test]
fn unsupported_automatic_style_values_are_reported_without_partial_mapping() {
    let xml = styled_content(
        r##"<style:style style:name="T" style:family="text"><style:text-properties fo:color="#ééé" fo:font-size="10.25pt" fo:letter-spacing="1pt"/></style:style>"##,
        r#"<text:p><text:span text:style-name="T">safe</text:span></text:p>"#,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let InlineNode::Run(run) = &paragraph(&imported, 0).inlines[0] else {
        panic!("run")
    };
    assert_eq!(run.properties, Default::default());
    for feature in [
        "odf.attribute.fo.color",
        "odf.attribute.fo.font-size",
        "odf.attribute.fo.letter-spacing",
    ] {
        assert!(
            imported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == feature
                    && entry.model_outcome == ModelOutcome::Degraded),
            "missing {feature}"
        );
    }
}

#[test]
fn identity_and_reports_ignore_prefix_and_attribute_order() {
    let first = br#"<o:document-content xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" o:version="1.3"><o:body><o:text><t:p t:style-name="Body">same<t:s t:c="2"/>text</t:p></o:text></o:body></o:document-content>"#;
    let reordered = br#"<office:document-content office:version="1.3" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><office:body><office:text><text:p text:style-name="Body">same<text:s text:c="2"/>text</text:p></office:text></office:body></office:document-content>"#;
    let first = import_content_xml(first, OdfVersion::V1_3, OdfImportLimits::default()).unwrap();
    let reordered =
        import_content_xml(reordered, OdfVersion::V1_3, OdfImportLimits::default()).unwrap();
    assert_eq!(first.document, reordered.document);
    assert_eq!(first.report, reordered.report);

    let linked_first = br##"<o:document-content xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:x="http://www.w3.org/1999/xlink" o:version="1.4"><o:body><o:text><t:p><t:bookmark t:name="anchor"/><t:a x:type="simple" x:href="#anchor">same</t:a></t:p></o:text></o:body></o:document-content>"##;
    let linked_reordered = br##"<office:document-content xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><office:body><office:text><text:p><text:bookmark text:name="anchor"/><text:a xlink:href="#anchor" xlink:type="simple">same</text:a></text:p></office:text></office:body></office:document-content>"##;
    let linked_first =
        import_content_xml(linked_first, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let linked_reordered = import_content_xml(
        linked_reordered,
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    assert_eq!(linked_first.document, linked_reordered.document);
    assert_eq!(linked_first.report, linked_reordered.report);
}

#[test]
fn wrong_version_document_kind_dtd_and_active_content_fail_closed() {
    let mismatch = content("1.2", "<text:p>x</text:p>");
    assert_eq!(
        import_content_xml(&mismatch, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let spreadsheet = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:version="1.4"><office:body><office:spreadsheet/></office:body></office:document-content>"#;
    assert_eq!(
        import_content_xml(spreadsheet, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::UnsupportedDocumentKind
    );

    let dtd = br#"<!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:version="1.4"><office:body><office:text/></office:body></office:document-content>"#;
    assert_eq!(
        import_content_xml(dtd, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::MalformedContent
    );

    let active = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:version="1.4"><office:scripts/><office:body><office:text/></office:body></office:document-content>"#;
    assert_eq!(
        import_content_xml(active, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::ActiveContent
    );

    let event_listener = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:script="urn:oasis:names:tc:opendocument:xmlns:script:1.0" office:version="1.4"><office:body><office:text><script:event-listener/></office:text></office:body></office:document-content>"#;
    assert_eq!(
        import_content_xml(event_listener, OdfVersion::V1_4, OdfImportLimits::default())
            .unwrap_err(),
        OdfError::ActiveContent
    );

    let undeclared_entity = content("1.4", "<text:p>&unknown;</text:p>");
    assert_eq!(
        import_content_xml(
            &undeclared_entity,
            OdfVersion::V1_4,
            OdfImportLimits::default()
        )
        .unwrap_err(),
        OdfError::MalformedContent
    );
}

#[test]
fn empty_text_body_and_deferred_containers_have_explicit_outcomes() {
    let empty = content("1.4", "");
    let imported =
        import_content_xml(&empty, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert_eq!(imported.document.body().len(), 1);
    assert!(paragraph(&imported, 0).inlines.is_empty());

    let deferred = content(
        "1.4",
        r#"<text:list><text:list-item><text:p><text:a xlink:href="https://example.invalid/">visible</text:a></text:p></text:list-item></text:list>"#,
    );
    let imported =
        import_content_xml(&deferred, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert_eq!(paragraph(&imported, 0).inlines.len(), 1);
    assert!(
        matches!(&paragraph(&imported, 0).inlines[0], InlineNode::Hyperlink(link)
        if matches!(&link.target, HyperlinkTarget::External(target) if target.url == "https://example.invalid/")
        && matches!(&link.inlines[0], InlineNode::Run(run) if run.text == "visible"))
    );
    for expected in ["odf.element.text.list", "odf.element.text.list-item"] {
        assert!(
            imported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == expected)
        );
    }
}

#[test]
fn hyperlinks_and_bookmarks_map_without_fetching_or_flattening() {
    let xml = content(
        "1.4",
        r##"<text:p><text:bookmark-start text:name="section"/>before <text:a xlink:type="simple" xlink:href="https://example.invalid/path">external</text:a> <text:a xlink:href="#section">internal</text:a><text:bookmark-end text:name="section"/><text:bookmark text:name="point"/></text:p>"##,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();
    assert_eq!(imported.document.definitions().bookmarks.len(), 2);
    let inlines = &paragraph(&imported, 0).inlines;
    assert!(matches!(inlines[0], InlineNode::BookmarkStart(_)));
    assert!(matches!(&inlines[2], InlineNode::Hyperlink(link)
        if matches!(&link.target, HyperlinkTarget::External(target) if target.url == "https://example.invalid/path")
        && matches!(&link.inlines[0], InlineNode::Run(run) if run.text == "external")));
    assert!(matches!(&inlines[4], InlineNode::Hyperlink(link)
        if matches!(&link.target, HyperlinkTarget::Internal(target) if target.anchor == "section")
        && matches!(&link.inlines[0], InlineNode::Run(run) if run.text == "internal")));
    assert!(matches!(inlines[5], InlineNode::BookmarkEnd(_)));
    assert!(matches!(inlines[6], InlineNode::BookmarkStart(_)));
    assert!(matches!(inlines[7], InlineNode::BookmarkEnd(_)));
    assert!(!imported.report.entries.iter().any(|entry| {
        matches!(
            entry.feature.as_str(),
            "odf.element.text.a"
                | "odf.attribute.xlink.href"
                | "odf.element.text.bookmark"
                | "odf.element.text.bookmark-start"
                | "odf.element.text.bookmark-end"
        )
    }));
}

#[test]
fn invalid_or_unpaired_links_and_bookmarks_degrade_explicitly() {
    let long_href = "x".repeat(2_049);
    let long_name = "n".repeat(256);
    let xml = content(
        "1.4",
        &format!(
            r#"<text:p><text:a>missing</text:a><text:a xlink:href="{long_href}">long</text:a><text:a xlink:href="javascript:alert(1)">blocked</text:a><text:a xlink:href="https://example.invalid/"/><text:bookmark-start text:name="open"/><text:bookmark-end text:name="missing"/><text:bookmark text:name="{long_name}"/>tail</text:p>"#
        ),
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();
    assert!(imported.document.definitions().bookmarks.is_empty());
    assert!(
        paragraph(&imported, 0)
            .inlines
            .iter()
            .all(|inline| matches!(inline, InlineNode::Run(_)))
    );
    for expected in [
        "odf.element.text.a",
        "odf.hyperlink.blocked-scheme",
        "odf.attribute.text.name",
        "odf.element.text.bookmark-start",
        "odf.element.text.bookmark-end",
    ] {
        assert!(
            imported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == expected),
            "missing {expected} finding"
        );
    }
    assert!(imported.report.entries.iter().any(|entry| {
        entry.feature == "odf.hyperlink.blocked-scheme"
            && entry.retention_outcome == RetentionOutcome::Blocked
    }));

    let nested = content(
        "1.4",
        r#"<text:p><text:a xlink:href="https://outer.invalid/"><text:a xlink:href="https://inner.invalid/">bad</text:a></text:a></text:p>"#,
    );
    assert_eq!(
        import_content_xml(&nested, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::MalformedContent
    );
}

#[test]
fn malformed_leaf_content_and_duplicate_bodies_are_rejected() {
    let nonempty_leaf = content("1.4", "<text:p><text:tab>bad</text:tab></text:p>");
    assert_eq!(
        import_content_xml(&nonempty_leaf, OdfVersion::V1_4, OdfImportLimits::default())
            .unwrap_err(),
        OdfError::MalformedContent
    );

    let duplicate = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:version="1.4"><office:body><office:text/></office:body><office:body/></office:document-content>"#;
    assert_eq!(
        import_content_xml(duplicate, OdfVersion::V1_4, OdfImportLimits::default()).unwrap_err(),
        OdfError::MalformedContent
    );
}

#[test]
fn content_limits_and_cancellation_are_atomic() {
    let xml = content("1.4", "<text:p>x<text:s text:c=\"4\"/></text:p>");
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert_eq!(
        import_content_xml_with_cancellation(
            &xml,
            OdfVersion::V1_4,
            OdfImportLimits::default(),
            &cancellation,
        )
        .unwrap_err(),
        OdfError::Cancelled
    );

    for limits in [
        OdfImportLimits {
            max_content_bytes: 8,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_paragraphs: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_inline_nodes: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_text_bytes: 1,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_space_repeat: 3,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_xml_depth: 2,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_xml_elements: 2,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_xml_attributes: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_xml_attribute_bytes: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_xml_name_bytes: 3,
            ..OdfImportLimits::default()
        },
    ] {
        let result = import_content_xml(&xml, OdfVersion::V1_4, limits);
        assert!(
            matches!(result, Err(OdfError::LimitExceeded { .. })),
            "expected limit failure for {limits:?}, got {result:?}"
        );
    }

    let invalid = OdfImportLimits {
        max_xml_depth: usize::MAX,
        ..OdfImportLimits::default()
    };
    assert!(matches!(
        import_content_xml(&xml, OdfVersion::V1_4, invalid),
        Err(OdfError::InvalidLimitConfiguration {
            limit: "odf_content_xml_depth",
            ..
        })
    ));

    let reported = content(
        "1.4",
        "<text:list><text:list-item><text:p>x</text:p></text:list-item></text:list>",
    );
    let import = import_content_xml(
        &reported,
        OdfVersion::V1_4,
        OdfImportLimits {
            max_report_features: 0,
            ..OdfImportLimits::default()
        },
    )
    .unwrap();
    assert_eq!(import.report.entries.len(), 1);
    assert_eq!(import.report.entries[0].feature, "odf.report.overflow");
}
