use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, HyperlinkTarget, InlineNode, NoteKind, NumberFormat,
    RgbColor, VerticalMerge,
};
use casual_doc_package::CancellationToken;

use crate::{
    ModelOutcome, OdfError, OdfExportLimits, OdfImportLimits, OdfPackageLimits, OdfVersion,
    OdtPackage, RetentionOutcome, import_content_xml, import_content_xml_with_cancellation,
    write_odt,
};

fn content(version: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
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
fn list_item_start_value_maps_to_numbering_override() {
    let styles = r#"<text:list-style style:name="L1"><text:list-level-style-number text:level="1" style:num-format="1"/></text:list-style>"#;
    let body = r#"<text:list text:style-name="L1"><text:list-item text:start-value="5"><text:p>item</text:p></text:list-item></text:list>"#;
    let import = import_content_xml(
        &styled_content(styles, body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let (_, instance) = import
        .document
        .definitions()
        .numbering
        .iter()
        .next()
        .expect("numbering instance");
    assert_eq!(instance.overrides.len(), 1);
    assert_eq!(instance.overrides[0].level, 0);
    assert_eq!(instance.overrides[0].start, Some(5));
    // A mapped start-value is not reported as an unrepresented item override.
    assert!(
        !import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.list.item-override")
    );

    // The override reaches the ODT output (as the list-style start) and the
    // written package reopens to a valid, render-equivalent document that
    // re-exports byte-identically.
    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(content.contains("text:start-value=\"5\""));
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn conflicting_later_start_value_is_reported() {
    // Two items in the same list with different start-values: the first maps to
    // an override, the second (a mid-list restart) cannot and is reported.
    let styles = r#"<text:list-style style:name="L1"><text:list-level-style-number text:level="1" style:num-format="1"/></text:list-style>"#;
    let body = r#"<text:list text:style-name="L1"><text:list-item text:start-value="5"><text:p>a</text:p></text:list-item><text:list-item text:start-value="9"><text:p>b</text:p></text:list-item></text:list>"#;
    let import = import_content_xml(
        &styled_content(styles, body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let (_, instance) = import
        .document
        .definitions()
        .numbering
        .iter()
        .next()
        .unwrap();
    assert_eq!(instance.overrides[0].start, Some(5));
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.list.item-override")
    );
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
fn named_style_cycles_and_missing_parents_degrade_without_losing_direct_properties() {
    let content = content(
        "1.4",
        r#"<text:p><text:span text:style-name="A">cycle</text:span><text:span text:style-name="Missing">missing</text:span></text:p>"#,
    );
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="A" style:family="text" style:parent-style-name="B"><style:text-properties fo:font-weight="bold"/></style:style><style:style style:name="B" style:family="text" style:parent-style-name="A"><style:text-properties fo:font-style="italic"/></style:style><style:style style:name="Missing" style:family="text" style:parent-style-name="Absent"><style:text-properties style:text-underline-style="solid"/></style:style></office:styles></office:document-styles>"#;
    let imported = crate::content::import_content_xml_with_styles_and_cancellation(
        &content,
        Some(styles),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    let paragraph = paragraph(&imported, 0);
    let InlineNode::Run(cycle) = &paragraph.inlines[0] else {
        panic!("cycle run")
    };
    assert_eq!(cycle.properties.bold, Some(true));
    assert_eq!(cycle.properties.italic, Some(true));
    let InlineNode::Run(missing) = &paragraph.inlines[1] else {
        panic!("missing-parent run")
    };
    assert_eq!(missing.properties.underline, Some(true));
    for feature in ["odf.style.inheritance-cycle", "odf.style.unresolved-parent"] {
        assert!(
            imported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == feature),
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
fn empty_text_body_and_defaulted_lists_have_explicit_outcomes() {
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
    for expected in ["odf.list-style.defaulted", "odf.list-style.missing-level"] {
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
fn bullet_decimal_and_nested_lists_map_to_numbering_definitions() {
    let xml = styled_content(
        r#"<text:list-style style:name="Mixed"><text:list-level-style-bullet text:level="1" text:bullet-char="•"/><text:list-level-style-number text:level="2" style:num-format="a" style:num-prefix="(" style:num-suffix=")" text:start-value="3"/></text:list-style>"#,
        r#"<text:list text:style-name="Mixed"><text:list-item><text:p>outer</text:p><text:p>continuation</text:p><text:list><text:list-item><text:p>nested</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>second</text:p></text:list-item></text:list>"#,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();
    assert!(imported.report.entries.is_empty(), "{:?}", imported.report);
    assert_eq!(imported.document.body().len(), 4);

    let outer = paragraph(&imported, 0).properties.numbering.unwrap();
    assert!(paragraph(&imported, 1).properties.numbering.is_none());
    let nested = paragraph(&imported, 2).properties.numbering.unwrap();
    let second = paragraph(&imported, 3).properties.numbering.unwrap();
    assert_eq!(outer.instance, nested.instance);
    assert_eq!(outer.instance, second.instance);
    assert_eq!(outer.level, 0);
    assert_eq!(nested.level, 1);
    assert_eq!(second.level, 0);

    let instance = imported
        .document
        .definitions()
        .numbering
        .get(&outer.instance)
        .unwrap();
    let abstract_numbering = imported
        .document
        .definitions()
        .abstract_numbering
        .get(&instance.abstract_ref)
        .unwrap();
    assert_eq!(abstract_numbering.levels.len(), 2);
    assert_eq!(
        abstract_numbering.levels[0].num_fmt,
        Some(NumberFormat::Bullet)
    );
    assert_eq!(abstract_numbering.levels[0].lvl_text.as_deref(), Some("•"));
    assert_eq!(
        abstract_numbering.levels[1].num_fmt,
        Some(NumberFormat::LowerLetter)
    );
    assert_eq!(
        abstract_numbering.levels[1].lvl_text.as_deref(),
        Some("(%2)")
    );
    assert_eq!(abstract_numbering.levels[1].start, 3);
}

#[test]
fn list_limits_and_unsupported_counter_controls_are_explicit() {
    let xml = styled_content(
        r#"<text:list-style style:name="N"><text:list-level-style-number text:level="1" style:num-format="1"/></text:list-style>"#,
        r#"<text:list text:style-name="N" text:continue-numbering="true"><text:list-item text:start-value="7"><text:p>x</text:p></text:list-item></text:list>"#,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    // Continuation across lists is still deferred; the item start-value now maps
    // to a numbering override rather than being reported as unrepresented.
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.list.continuation"),
        "missing odf.list.continuation"
    );
    assert!(
        !imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.list.item-override")
    );
    let (_, instance) = imported
        .document
        .definitions()
        .numbering
        .iter()
        .next()
        .unwrap();
    assert_eq!(instance.overrides[0].start, Some(7));

    for limits in [
        OdfImportLimits {
            max_lists: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_list_depth: 0,
            ..OdfImportLimits::default()
        },
    ] {
        assert!(matches!(
            import_content_xml(&xml, OdfVersion::V1_4, limits),
            Err(OdfError::LimitExceeded { .. })
        ));
    }
}

#[test]
fn tables_preserve_block_order_repeats_headers_and_nested_content() {
    let xml = content(
        "1.4",
        r#"<text:p>before</text:p>
<table:table>
 <table:table-column table:number-columns-repeated="2"/>
 <table:table-header-rows>
  <table:table-row><table:table-cell><text:p>head</text:p></table:table-cell><table:table-cell/></table:table-row>
 </table:table-header-rows>
 <table:table-row table:number-rows-repeated="2">
  <table:table-cell table:number-columns-repeated="2"><text:p>body</text:p></table:table-cell>
 </table:table-row>
 <table:table-row>
  <table:table-cell><table:table><table:table-row><table:table-cell><text:p>nested</text:p></table:table-cell></table:table-row></table:table></table:table-cell>
  <table:table-cell><text:p>tail</text:p></table:table-cell>
 </table:table-row>
</table:table>
<text:p>after</text:p>"#,
    );
    let first = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let second = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert_eq!(first, second);
    first.document.validate().unwrap();
    assert_eq!(first.document.body().len(), 3);
    let BlockNode::Table(table) = &first.document.body()[1] else {
        panic!("expected table")
    };
    assert_eq!(table.grid.len(), 2);
    assert_eq!(table.rows.len(), 4);
    assert!(table.rows[0].properties.header);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[0].cells[1].blocks.len(), 1);
    assert_eq!(table.rows[1].properties, table.rows[2].properties);
    assert_eq!(table.rows[1].cells.len(), table.rows[2].cells.len());
    for row in [&table.rows[1], &table.rows[2]] {
        for cell in &row.cells {
            let BlockNode::Paragraph(paragraph) = &cell.blocks[0] else {
                panic!("expected repeated paragraph")
            };
            assert!(matches!(&paragraph.inlines[0], InlineNode::Run(run) if run.text == "body"));
        }
    }
    let BlockNode::Table(nested) = &table.rows[3].cells[0].blocks[0] else {
        panic!("expected nested table")
    };
    assert_eq!(nested.rows.len(), 1);
    assert_eq!(nested.rows[0].cells.len(), 1);
}

#[test]
fn table_spans_and_covered_cells_map_to_normalized_merge_geometry() {
    let xml = content(
        "1.4",
        r#"<table:table>
 <table:table-column table:number-columns-repeated="3"/>
 <table:table-row>
  <table:table-cell table:number-columns-spanned="2" table:number-rows-spanned="2"><text:p>merged</text:p></table:table-cell>
  <table:covered-table-cell/>
  <table:table-cell><text:p>right</text:p></table:table-cell>
 </table:table-row>
 <table:table-row>
  <table:covered-table-cell table:number-columns-repeated="2"/>
  <table:table-cell><text:p>lower</text:p></table:table-cell>
 </table:table-row>
</table:table>"#,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let alternate = content(
        "1.4",
        r#"<t:table xmlns:t="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
 <t:table-column t:number-columns-repeated="3"/>
 <t:table-row>
  <t:table-cell t:number-rows-spanned="2" t:number-columns-spanned="2"><text:p>merged</text:p></t:table-cell>
  <t:covered-table-cell/>
  <t:table-cell><text:p>right</text:p></t:table-cell>
 </t:table-row>
 <t:table-row>
  <t:covered-table-cell t:number-columns-repeated="2"/>
  <t:table-cell><text:p>lower</text:p></t:table-cell>
 </t:table-row>
</t:table>"#,
    );
    assert_eq!(
        imported,
        import_content_xml(&alternate, OdfVersion::V1_4, OdfImportLimits::default(),).unwrap()
    );
    imported.document.validate().unwrap();
    let BlockNode::Table(table) = &imported.document.body()[0] else {
        panic!("expected table")
    };
    assert_eq!(table.grid.len(), 3);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[1].cells.len(), 2);
    assert_eq!(table.rows[0].cells[0].properties.grid_span, Some(2));
    assert_eq!(
        table.rows[0].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Restart)
    );
    assert_eq!(table.rows[1].cells[0].properties.grid_span, Some(2));
    assert_eq!(
        table.rows[1].cells[0].properties.vertical_merge,
        Some(VerticalMerge::Continue)
    );
}

#[test]
fn malformed_table_merge_topology_fails_closed() {
    for body in [
        r#"<table:table><table:table-row><table:covered-table-cell/></table:table-row></table:table>"#,
        r#"<table:table><table:table-row><table:table-cell table:number-columns-spanned="2"><text:p>x</text:p></table:table-cell><table:table-cell><text:p>not covered</text:p></table:table-cell></table:table-row></table:table>"#,
        r#"<table:table><table:table-row><table:table-cell table:number-rows-spanned="2"><text:p>x</text:p></table:table-cell></table:table-row></table:table>"#,
    ] {
        assert_eq!(
            import_content_xml(
                &content("1.4", body),
                OdfVersion::V1_4,
                OdfImportLimits::default(),
            )
            .unwrap_err(),
            OdfError::MalformedContent
        );
    }
}

#[test]
fn table_limits_bound_repetition_and_nesting() {
    let repeated = content(
        "1.4",
        r#"<table:table><table:table-row table:number-rows-repeated="2"><table:table-cell table:number-columns-repeated="2"/></table:table-row></table:table>"#,
    );
    for limits in [
        OdfImportLimits {
            max_table_rows: 1,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_table_cells: 3,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_tables: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_table_depth: 0,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_paragraphs: 3,
            ..OdfImportLimits::default()
        },
    ] {
        assert!(matches!(
            import_content_xml(&repeated, OdfVersion::V1_4, limits),
            Err(OdfError::LimitExceeded { .. })
        ));
    }

    let nested = content(
        "1.4",
        r#"<table:table><table:table-row><table:table-cell><table:table><table:table-row><table:table-cell/></table:table-row></table:table></table:table-cell></table:table-row></table:table>"#,
    );
    assert!(matches!(
        import_content_xml(
            &nested,
            OdfVersion::V1_4,
            OdfImportLimits {
                max_table_depth: 1,
                ..OdfImportLimits::default()
            },
        ),
        Err(OdfError::LimitExceeded { .. })
    ));

    let repeated_nested = content(
        "1.4",
        r#"<table:table><table:table-row><table:table-cell table:number-columns-repeated="2"><text:p>xx</text:p><table:table><table:table-row><table:table-cell/></table:table-row></table:table></table:table-cell></table:table-row></table:table>"#,
    );
    for limits in [
        OdfImportLimits {
            max_tables: 2,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_table_cells: 3,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_inline_nodes: 1,
            ..OdfImportLimits::default()
        },
        OdfImportLimits {
            max_text_bytes: 3,
            ..OdfImportLimits::default()
        },
    ] {
        assert!(matches!(
            import_content_xml(&repeated_nested, OdfVersion::V1_4, limits),
            Err(OdfError::LimitExceeded { .. })
        ));
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

    for body in [
        "<text:list><text:p>missing item</text:p></text:list>",
        "<text:list><text:list><text:list-item><text:p>bad nesting</text:p></text:list-item></text:list></text:list>",
    ] {
        assert_eq!(
            import_content_xml(
                &content("1.4", body),
                OdfVersion::V1_4,
                OdfImportLimits::default(),
            )
            .unwrap_err(),
            OdfError::MalformedContent
        );
    }
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

#[test]
fn footnotes_and_endnotes_map_to_typed_definitions_in_source_order() {
    let xml = content(
        "1.4",
        r#"<text:p>before<text:note text:id="fn-1" text:note-class="footnote"><text:note-citation>*</text:note-citation><text:note-body><text:p>foot body</text:p></text:note-body></text:note>middle<text:note text:note-class="endnote" text:id="en-1"><text:note-citation>i</text:note-citation><text:note-body><text:p>end body</text:p></text:note-body></text:note>after</text:p>"#,
    );
    let first = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    let second = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert_eq!(first, second);
    first.document.validate().unwrap();

    let paragraph = paragraph(&first, 0);
    assert!(matches!(&paragraph.inlines[0], InlineNode::Run(run) if run.text == "before"));
    let InlineNode::NoteReference(footnote_reference) = &paragraph.inlines[1] else {
        panic!("footnote reference")
    };
    assert_eq!(footnote_reference.kind, NoteKind::Footnote);
    assert!(matches!(&paragraph.inlines[2], InlineNode::Run(run) if run.text == "middle"));
    let InlineNode::NoteReference(endnote_reference) = &paragraph.inlines[3] else {
        panic!("endnote reference")
    };
    assert_eq!(endnote_reference.kind, NoteKind::Endnote);
    assert!(matches!(&paragraph.inlines[4], InlineNode::Run(run) if run.text == "after"));

    let footnote = first
        .document
        .definitions()
        .footnotes
        .get(&footnote_reference.note)
        .unwrap();
    assert!(
        matches!(&footnote.blocks[0], BlockNode::Paragraph(paragraph) if matches!(&paragraph.inlines[0], InlineNode::Run(run) if run.text == "foot body"))
    );
    let endnote = first
        .document
        .definitions()
        .endnotes
        .get(&endnote_reference.note)
        .unwrap();
    assert!(
        matches!(&endnote.blocks[0], BlockNode::Paragraph(paragraph) if matches!(&paragraph.inlines[0], InlineNode::Run(run) if run.text == "end body"))
    );
    assert!(first.report.entries.iter().any(|entry| {
        entry.feature == "odf.element.text.note-citation"
            && entry.occurrences == 2
            && entry.model_outcome == ModelOutcome::Degraded
    }));
}

#[test]
fn note_blocks_route_outside_the_enclosing_table_cell_and_may_nest_tables() {
    let xml = content(
        "1.4",
        r#"<table:table><table:table-row><table:table-cell><text:p>cell<text:note text:id="n" text:note-class="footnote"><text:note-body><text:p>note paragraph</text:p><table:table><table:table-row><table:table-cell><text:p>nested</text:p></table:table-cell></table:table-row></table:table></text:note-body></text:note>tail</text:p></table:table-cell></table:table-row></table:table>"#,
    );
    let imported = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();

    let BlockNode::Table(outer) = &imported.document.body()[0] else {
        panic!("outer table")
    };
    let BlockNode::Paragraph(cell_paragraph) = &outer.rows[0].cells[0].blocks[0] else {
        panic!("cell paragraph")
    };
    let InlineNode::NoteReference(reference) = &cell_paragraph.inlines[1] else {
        panic!("note reference")
    };
    assert!(matches!(&cell_paragraph.inlines[2], InlineNode::Run(run) if run.text == "tail"));
    let note = imported
        .document
        .definitions()
        .footnotes
        .get(&reference.note)
        .unwrap();
    assert_eq!(note.blocks.len(), 2);
    assert!(matches!(&note.blocks[0], BlockNode::Paragraph(_)));
    assert!(matches!(&note.blocks[1], BlockNode::Table(_)));
    assert_eq!(outer.rows[0].cells[0].blocks.len(), 1);
}

#[test]
fn malformed_or_over_limit_notes_fail_atomically() {
    for body in [
        r#"<text:p><text:note text:id="n" text:note-class="footnote"/></text:p>"#,
        r#"<text:p><text:note text:id="n" text:note-class="other"><text:note-body/></text:note></text:p>"#,
        r#"<text:p><text:note text:id="n" text:note-class="footnote"><text:note-body><text:p><text:note text:id="nested" text:note-class="footnote"><text:note-body/></text:note></text:p></text:note-body></text:note></text:p>"#,
        r#"<text:p><text:note text:id="n" text:note-class="footnote"><text:note-body/></text:note><text:note text:id="n" text:note-class="endnote"><text:note-body/></text:note></text:p>"#,
    ] {
        assert_eq!(
            import_content_xml(
                &content("1.4", body),
                OdfVersion::V1_4,
                OdfImportLimits::default(),
            )
            .unwrap_err(),
            OdfError::MalformedContent
        );
    }

    let two_notes = content(
        "1.4",
        r#"<text:p><text:note text:id="a" text:note-class="footnote"><text:note-body/></text:note><text:note text:id="b" text:note-class="endnote"><text:note-body/></text:note></text:p>"#,
    );
    assert!(matches!(
        import_content_xml(
            &two_notes,
            OdfVersion::V1_4,
            OdfImportLimits {
                max_notes: 1,
                ..OdfImportLimits::default()
            },
        ),
        Err(OdfError::LimitExceeded {
            limit: "odf_content_notes",
            observed: 2,
            allowed: 1,
        })
    ));
}
