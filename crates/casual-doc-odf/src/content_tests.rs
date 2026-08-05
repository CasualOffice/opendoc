use casual_doc_model::v1::{
    Alignment, BlockNode, BreakKind, Color, Extent, HyperlinkTarget, InlineNode, NoteKind,
    NumberFormat, RgbColor, VerticalMerge,
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

const DRAW_CONTENT_HEAD: &str = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text>"#;

fn draw_content(body: &str) -> Vec<u8> {
    format!("{DRAW_CONTENT_HEAD}{body}</office:text></office:body></office:document-content>")
        .into_bytes()
}

#[test]
fn embedded_image_frame_maps_to_drawing_and_media_reference() {
    let xml = draw_content(
        r#"<text:p><draw:frame svg:width="2cm" svg:height="3cm"><draw:image xlink:href="Pictures/img.png"/><svg:title>alt text</svg:title></draw:frame></text:p>"#,
    );
    let import = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();

    assert_eq!(import.document.definitions().media.len(), 1);
    let (media_id, media) = import.document.definitions().media.iter().next().unwrap();
    assert_eq!(media.part_name, "Pictures/img.png");
    assert_eq!(media.media_type, "image/png");

    let InlineNode::Drawing(drawing) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected drawing")
    };
    assert_eq!(drawing.media, *media_id);
    assert_eq!(drawing.descr.as_deref(), Some("alt text"));
    assert_eq!(
        drawing.extent,
        Some(Extent {
            width_emu: 720_000,
            height_emu: 1_080_000,
        })
    );
}

#[test]
fn active_content_in_frame_does_not_leak_macro_source_into_descr() {
    // A macro source nested (via office:scripts/office:script) inside a captured
    // svg:desc must be dropped wholesale, not copied into the image's alt text
    // and re-emitted on export. The valid image href still maps a drawing.
    let xml = draw_content(
        r#"<text:p><draw:frame><draw:image xlink:href="Pictures/x.png"/><svg:desc><office:scripts><office:script>Shell("calc")</office:script></office:scripts></svg:desc></draw:frame></text:p>"#,
    );
    let import = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();

    let InlineNode::Drawing(drawing) = &paragraph(&import, 0).inlines[0] else {
        panic!("expected drawing")
    };
    assert_eq!(
        drawing.descr, None,
        "macro source must not survive as the image description"
    );
    assert!(import.report.entries.iter().any(|entry| {
        entry.feature == "odf.security.active-content-dropped"
            && entry.model_outcome == ModelOutcome::Degraded
    }));
}

#[test]
fn active_content_nested_image_is_not_adopted_as_frame_image() {
    // A draw:image that exists only inside active content must not be promoted to
    // the frame's modeled image; the frame is image-less and drops the drawing.
    let xml = draw_content(
        r#"<text:p><draw:frame><office:scripts><draw:image xlink:href="Pictures/handler-icon.png"/></office:scripts></draw:frame></text:p>"#,
    );
    let import = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    assert!(
        import.document.definitions().media.is_empty(),
        "an image nested in active content must not become media"
    );
    assert!(paragraph(&import, 0).inlines.is_empty());
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| { entry.feature == "odf.security.active-content-dropped" })
    );
}

#[test]
fn overlong_image_href_degrades_not_aborts() {
    // 256+ bytes exceeds the model relationship_id cap: block the drawing rather
    // than abort the whole import.
    let href = format!("Pictures/{}.png", "a".repeat(300));
    let xml = draw_content(&format!(
        r#"<text:p><draw:frame><draw:image xlink:href="{href}"/></draw:frame></text:p>"#
    ));
    let import = import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    assert!(import.document.definitions().media.is_empty());
    assert!(paragraph(&import, 0).inlines.is_empty());
}

#[test]
fn linked_and_unsafe_image_hrefs_are_blocked() {
    for href in [
        "http://evil.example/x.png",
        "HTTP://evil.example/x.png",
        "file:///etc/passwd",
        "data:image/png;base64,AAAA",
        "../secret.png",
        "Pictures/../../etc/passwd",
        "Pictures/..",
        "..",
        "/etc/passwd",
        "C:\\Windows\\x.png",
        "Pictures\\x.png",
        "",
    ] {
        let xml = draw_content(&format!(
            r#"<text:p><draw:frame><draw:image xlink:href="{href}"/></draw:frame></text:p>"#
        ));
        let import =
            import_content_xml(&xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
        import.document.validate().unwrap();
        assert!(
            import.document.definitions().media.is_empty(),
            "href {href:?} must not create media"
        );
        assert!(paragraph(&import, 0).inlines.is_empty());
        assert!(import.report.entries.iter().any(|entry| {
            entry.feature == "odf.draw.linked-image" || entry.feature == "odf.draw.image-missing"
        }));
    }
}

#[test]
fn overflow_and_malformed_paragraph_lengths_are_reported_not_panicking() {
    // `fo:text-indent="-107374182.4pt"` parses to exactly i32::MIN, whose
    // negation (needed for a hanging indent) would overflow — it must be rejected
    // and reported, never panic. A malformed `"pt"` must also be reported, not
    // silently read as a zero margin.
    let styles = r#"<style:style style:name="P1" style:family="paragraph"><style:paragraph-properties fo:text-indent="-107374182.4pt" fo:margin-left="pt"/></style:style>"#;
    let body = r#"<text:p text:style-name="P1">x</text:p>"#;
    let import = import_content_xml(
        &styled_content(styles, body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let BlockNode::Paragraph(paragraph) = &import.document.body()[0] else {
        panic!("paragraph")
    };
    assert!(
        paragraph.properties.indentation.is_none(),
        "malformed/overflowing lengths must not be captured"
    );
    assert!(import.report.entries.iter().any(|entry| {
        entry.feature == "odf.attribute.fo.text-indent"
            || entry.feature == "odf.attribute.fo.margin-left"
    }));
}

#[test]
fn transparent_and_automatic_cell_defaults_are_not_reported_degraded() {
    // `fo:background-color="transparent"` and `style:vertical-align="automatic"`
    // are the ODF defaults (no fill / no explicit alignment): recognized no-ops,
    // not degradations.
    let xml = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="ce1" style:family="table-cell"><style:table-cell-properties fo:background-color="transparent" style:vertical-align="automatic"/></style:style></office:automatic-styles><office:body><office:text><table:table><table:table-column/><table:table-row><table:table-cell table:style-name="ce1"><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    assert!(
        !import.report.entries.iter().any(|entry| {
            entry.feature == "odf.attribute.fo.background-color"
                || entry.feature == "odf.attribute.style.vertical-align"
        }),
        "ODF-default cell values must not be reported degraded"
    );
}

#[test]
fn table_cell_shading_and_valign_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::CellVerticalAlignment;
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    let cell = &mut table.rows[0].cells[0];
    cell.properties.shading.fill = Some(RgbColor {
        r: 0xff,
        g: 0xcc,
        b: 0x00,
    });
    cell.properties.vertical_alignment = Some(CellVerticalAlignment::Center);
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r##"<style:style style:name="ce_cffcc00_vm" style:family="table-cell"><style:table-cell-properties fo:background-color="#ffcc00" style:vertical-align="middle"/></style:style>"##
        ),
        "cell style missing: {content_xml}"
    );
    assert!(
        content_xml.contains(r#"<table:table-cell table:style-name="ce_cffcc00_vm">"#),
        "cell style ref missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    let cell = &table.rows[0].cells[0];
    assert_eq!(
        cell.properties.shading.fill,
        Some(RgbColor {
            r: 0xff,
            g: 0xcc,
            b: 0x00,
        })
    );
    assert_eq!(
        cell.properties.vertical_alignment,
        Some(CellVerticalAlignment::Center)
    );

    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn inheriting_table_row_style_keeps_its_own_height() {
    // A child table-row style that inherits from a same-map parent must keep its
    // own directly-set row-height (style-inheritance merge must carry it), not
    // silently drop it.
    use casual_doc_model::v1::HeightRule;
    let xml = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="ro0" style:family="table-row"/><style:style style:name="ro1" style:family="table-row" style:parent-style-name="ro0"><style:table-row-properties style:row-height="1cm"/></style:style></office:automatic-styles><office:body><office:text><table:table><table:table-column/><table:table-row table:style-name="ro1"><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    let BlockNode::Table(table) = &import.document.body()[0] else {
        panic!("table")
    };
    let height = &table.rows[0].properties.height;
    assert_eq!(height.rule, Some(HeightRule::Exact));
    assert_eq!(height.value_twips, Some(567)); // 1cm
}

#[test]
fn table_align_margins_degrades_not_aborts() {
    // `table:align="margins"` has no model carrier (the model forbids Justify on
    // tables); it must be dropped with a finding, never abort the whole import.
    let xml = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="T1" style:family="table"><style:table-properties table:align="margins"/></style:style></office:automatic-styles><office:body><office:text><table:table table:style-name="T1"><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    let BlockNode::Table(table) = &import.document.body()[0] else {
        panic!("table")
    };
    assert_eq!(table.properties.alignment, None);
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.attribute.table.align"),
        "unrepresentable table align must be reported"
    );
}

#[test]
fn table_level_alignment_and_width_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::{TableWidth, WidthType};
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    table.properties.alignment = Some(Alignment::Center);
    table.properties.width = Some(TableWidth {
        value: 720, // 36pt
        width_type: WidthType::Dxa,
    });
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<style:style style:name="tb_ac_w720" style:family="table"><style:table-properties table:align="center" style:width="36pt"/></style:style>"#
        ),
        "table style missing: {content_xml}"
    );
    assert!(
        content_xml.contains(r#"<table:table table:style-name="tb_ac_w720">"#),
        "table style ref missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    assert_eq!(table.properties.alignment, Some(Alignment::Center));
    assert_eq!(
        table.properties.width,
        Some(TableWidth {
            value: 720,
            width_type: WidthType::Dxa,
        })
    );

    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn table_row_height_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::{HeightRule, RowHeight};
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    table.rows[0].properties.height = RowHeight {
        value_twips: Some(720), // 36pt
        rule: Some(HeightRule::Exact),
    };
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<style:style style:name="roe720" style:family="table-row"><style:table-row-properties style:row-height="36pt"/></style:style>"#
        ),
        "row style missing: {content_xml}"
    );
    assert!(
        content_xml.contains(r#"<table:table-row table:style-name="roe720">"#),
        "row style ref missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    assert_eq!(
        table.rows[0].properties.height,
        RowHeight {
            value_twips: Some(720),
            rule: Some(HeightRule::Exact),
        }
    );

    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn table_cell_borders_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::BorderEdge;
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    let edge = BorderEdge {
        style: "solid".to_owned(),
        size_eighth_points: Some(4), // 0.5pt
        color: Some(RgbColor { r: 0, g: 0, b: 0 }),
        space_points: None,
    };
    let cell = &mut table.rows[0].cells[0];
    cell.properties.borders.top = Some(edge.clone());
    cell.properties.borders.start = Some(edge.clone());
    cell.properties.borders.bottom = Some(edge.clone());
    cell.properties.borders.end = Some(edge.clone());
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r##"fo:border="0.5pt solid #000000""##),
        "uniform border shorthand missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    let cell = &table.rows[0].cells[0];
    assert_eq!(cell.properties.borders.top, Some(edge.clone()));
    assert_eq!(cell.properties.borders.start, Some(edge.clone()));
    assert_eq!(cell.properties.borders.bottom, Some(edge.clone()));
    assert_eq!(cell.properties.borders.end, Some(edge));

    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn cell_border_with_zero_padding_is_emitted_not_dropped() {
    // Word writes w:space="0" on every edge (space_points = Some(0)); zero padding
    // is the ODF default, so the border must still be emitted, not dropped.
    use casual_doc_model::v1::BorderEdge;
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    let edge = BorderEdge {
        style: "solid".to_owned(),
        size_eighth_points: Some(4),
        color: Some(RgbColor { r: 0, g: 0, b: 0 }),
        space_points: Some(0),
    };
    let cell = &mut table.rows[0].cells[0];
    cell.properties.borders.top = Some(edge.clone());
    cell.properties.borders.start = Some(edge.clone());
    cell.properties.borders.bottom = Some(edge.clone());
    cell.properties.borders.end = Some(edge);
    document.validate().unwrap();
    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r##"fo:border="0.5pt solid #000000""##),
        "a zero-padding (Word-style) border must be emitted: {content_xml}"
    );
}

#[test]
fn off_grid_pt_border_width_is_rounded_not_dropped() {
    // A width not on the 1/8-pt grid (0.2pt) must round and keep the border, not
    // drop style+color along with the width.
    let xml = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="ce1" style:family="table-cell"><style:table-cell-properties fo:border="0.2pt solid #000000"/></style:style></office:automatic-styles><office:body><office:text><table:table><table:table-column/><table:table-row><table:table-cell table:style-name="ce1"><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"##;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    let BlockNode::Table(table) = &import.document.body()[0] else {
        panic!("table")
    };
    let edge = table.rows[0].cells[0]
        .properties
        .borders
        .top
        .as_ref()
        .expect("off-grid border must be kept, not dropped");
    assert_eq!(edge.style, "solid");
    assert_eq!(edge.size_eighth_points, Some(2)); // 0.2pt rounds to 0.25pt (2 eighths)
    assert_eq!(edge.color, Some(RgbColor { r: 0, g: 0, b: 0 }));
}

#[test]
fn table_column_width_over_domain_degrades_and_no_spurious_finding() {
    // An out-of-domain width (24in = 34560 twips > 31680) must be dropped with a
    // finding, never abort the whole import; a valid width is captured; and a
    // consumed table:style-name must NOT produce a spurious degraded finding.
    let xml = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="cowide" style:family="table-column"><style:table-column-properties style:column-width="24in"/></style:style><style:style style:name="conarrow" style:family="table-column"><style:table-column-properties style:column-width="2cm"/></style:style></office:automatic-styles><office:body><office:text><table:table><table:table-column table:style-name="cowide"/><table:table-column table:style-name="conarrow"/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell><table:table-cell><text:p>b</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    let BlockNode::Table(table) = &import.document.body()[0] else {
        panic!("table")
    };
    assert_eq!(
        table.grid[0].width_twips, None,
        "over-domain width must be dropped"
    );
    assert!(
        table.grid[1].width_twips.is_some(),
        "in-domain width must be captured"
    );
    assert!(
        !import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.attribute.table.style-name"),
        "a consumed table:style-name must not be reported degraded"
    );
}

#[test]
fn table_column_widths_round_trip_to_a_fixed_point() {
    let body = r#"<table:table><table:table-column table:number-columns-repeated="2"/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell><table:table-cell><text:p>b</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    assert_eq!(table.grid.len(), 2);
    table.grid[0].width_twips = Some(1440);
    table.grid[1].width_twips = Some(2880);
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<style:style style:name="co1440" style:family="table-column"><style:table-column-properties style:column-width="72pt"/></style:style>"#
        ),
        "column style missing: {content_xml}"
    );
    assert!(
        content_xml.contains(
            r#"<table:table-column table:style-name="co1440"/><table:table-column table:style-name="co2880"/>"#
        ),
        "column refs missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    assert_eq!(table.grid[0].width_twips, Some(1440));
    assert_eq!(table.grid[1].width_twips, Some(2880));

    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn date_and_time_fields_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::FieldKind;
    let body =
        r#"<text:p><text:date>2020-01-01</text:date> <text:time>12:00:00</text:time></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let kinds: Vec<FieldKind> = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.kind.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            FieldKind::Date { format: None },
            FieldKind::Time { format: None }
        ]
    );

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains("<text:date/>") && content_xml.contains("<text:time/>"),
        "date/time field elements missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn sequence_field_round_trips_to_a_fixed_point() {
    use casual_doc_model::v1::FieldKind;
    let body = r#"<text:p>Figure <text:sequence text:name="Figure">1</text:sequence></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let kinds: Vec<FieldKind> = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.kind.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![FieldKind::Seq {
            name: "Figure".to_owned()
        }]
    );

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"<text:sequence text:name="Figure"/>"#),
        "sequence field missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn table_cell_padding_imports_and_round_trips_to_a_fixed_point() {
    // Four DISTINCT `fo:padding-*` edges exercise the importer's per-edge padding
    // reader and the exporter's per-edge emission, and catch any left/right or
    // top/bottom transposition (which equal-edge values would hide).
    let xml = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="ce1" style:family="table-cell"><style:table-cell-properties fo:padding-top="5pt" fo:padding-left="2.5pt" fo:padding-bottom="7.5pt" fo:padding-right="10pt"/></style:style></office:automatic-styles><office:body><office:text><table:table><table:table-column/><table:table-row><table:table-cell table:style-name="ce1"><text:p>a</text:p></table:table-cell></table:table-row></table:table></office:text></office:body></office:document-content>"#;
    let import = import_content_xml(xml, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    import.document.validate().unwrap();
    let BlockNode::Table(table) = &import.document.body()[0] else {
        panic!("table")
    };
    let margins = &table.rows[0].cells[0].properties.margins;
    // 5pt=100, 2.5pt=50, 7.5pt=150, 10pt=200 twips.
    assert_eq!(margins.top_twips, Some(100));
    assert_eq!(margins.start_twips, Some(50)); // fo:padding-left -> start
    assert_eq!(margins.bottom_twips, Some(150));
    assert_eq!(margins.end_twips, Some(200)); // fo:padding-right -> end

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"fo:padding-top="5pt""#)
            && content_xml.contains(r#"fo:padding-left="2.5pt""#)
            && content_xml.contains(r#"fo:padding-bottom="7.5pt""#)
            && content_xml.contains(r#"fo:padding-right="10pt""#),
        "per-edge padding missing: {content_xml}"
    );
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn uniform_cell_padding_collapses_to_the_shorthand() {
    let body = r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>a</text:p></table:table-cell></table:table-row></table:table>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    let BlockNode::Table(table) = &mut document.body_mut()[0] else {
        panic!("table")
    };
    let margins = &mut table.rows[0].cells[0].properties.margins;
    margins.top_twips = Some(100); // 5pt on every edge
    margins.start_twips = Some(100);
    margins.bottom_twips = Some(100);
    margins.end_twips = Some(100);
    document.validate().unwrap();

    let first = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"fo:padding="5pt""#) && !content_xml.contains("fo:padding-top"),
        "uniform padding shorthand missing: {content_xml}"
    );
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    let BlockNode::Table(table) = &reopened.document.body()[0] else {
        panic!("table")
    };
    let margins = &table.rows[0].cells[0].properties.margins;
    assert_eq!(margins.top_twips, Some(100));
    assert_eq!(margins.end_twips, Some(100));
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn table_of_content_round_trips_to_a_fixed_point() {
    use casual_doc_model::v1::SdtControlKind;
    let body = r#"<text:p>Intro</text:p><text:table-of-content text:name="_Toc1"><text:table-of-content-source text:outline-level="10"/><text:index-body><text:p>Chapter One</text:p><text:p>Chapter Two</text:p></text:index-body></text:table-of-content><text:p>Body</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();

    // The TOC is captured as a block content control between the surrounding body
    // paragraphs; its index-body entries are NOT leaked into the body.
    let blocks = import.document.body();
    assert!(
        matches!(&blocks[0], BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Run(r) if r.text == "Intro"))),
        "intro paragraph missing"
    );
    assert!(
        matches!(&blocks[2], BlockNode::Paragraph(p) if p.inlines.iter().any(|i| matches!(i, InlineNode::Run(r) if r.text == "Body"))),
        "trailing body paragraph missing"
    );
    let BlockNode::Sdt(sdt) = &blocks[1] else {
        panic!(
            "expected a TOC content control at body[1], got {:?}",
            blocks[1]
        );
    };
    assert_eq!(
        sdt.properties.control_kind,
        Some(SdtControlKind::BuildingBlockGallery)
    );
    assert_eq!(sdt.properties.gallery.as_deref(), Some("Table of Contents"));
    assert_eq!(sdt.properties.tag.as_deref(), Some("_Toc1"));
    assert_eq!(sdt.blocks.len(), 2, "two captured TOC entries");

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<text:table-of-content text:name="_Toc1"><text:table-of-content-source text:outline-level="10"/><text:index-body>"#
        ),
        "TOC wrapper missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn table_of_content_captures_a_nested_table_without_leaking() {
    // The trickiest splice case: a table inside index-body. During table parse
    // the block-router targets cells; the finished table lands in the body draft
    // list and must be spliced into the TOC (not left in the body, not lost).
    let body = r#"<text:table-of-content text:name="T1"><text:index-body><text:p>Before</text:p><table:table><table:table-column/><table:table-row><table:table-cell><text:p>cell</text:p></table:table-cell></table:table-row></table:table><text:p>After</text:p></text:index-body></text:table-of-content>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let blocks = import.document.body();
    assert_eq!(
        blocks.len(),
        1,
        "TOC is the only body block; entries must not leak"
    );
    let BlockNode::Sdt(sdt) = &blocks[0] else {
        panic!("expected a TOC content control")
    };
    assert_eq!(
        sdt.blocks.len(),
        3,
        "before-para + table + after-para captured"
    );
    assert!(
        matches!(sdt.blocks[1], BlockNode::Table(_)),
        "nested table captured inside the TOC"
    );

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn unnamed_table_of_content_mints_a_stable_name() {
    // A TOC with no text:name still round-trips: export mints a document-unique
    // name, re-import captures it, and the second export is a byte-exact fixed
    // point.
    let body = r#"<text:table-of-content><text:table-of-content-source text:outline-level="10"/><text:index-body><text:p>Entry</text:p></text:index-body></text:table-of-content>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"text:name="Table of Contents1""#),
        "minted TOC name missing: {content_xml}"
    );
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn colliding_toc_names_are_made_unique_on_export() {
    // A tagged TOC named exactly like the mint pattern plus an unnamed sibling
    // would collide (duplicate text:name = spec-invalid). Export must make them
    // unique, and the result must still be a byte-exact fixed point.
    let body = r#"<text:table-of-content text:name="Table of Contents1"><text:index-body><text:p>A</text:p></text:index-body></text:table-of-content><text:table-of-content><text:index-body><text:p>B</text:p></text:index-body></text:table-of-content>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"text:name="Table of Contents1""#)
            && content_xml.contains(r#"text:name="Table of Contents2""#),
        "colliding TOC names not disambiguated: {content_xml}"
    );
    assert_eq!(
        content_xml
            .matches(r#"text:name="Table of Contents1""#)
            .count(),
        1,
        "the mint-pattern name must appear exactly once"
    );
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn inline_text_box_round_trips_to_a_fixed_point() {
    use casual_doc_model::v1::BlockNode;
    // The `&amp;` exercises entity (GeneralRef) capture in the box body.
    let body = r#"<text:p>Before <draw:frame xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"><draw:text-box><text:p>box &amp; text</text:p></draw:text-box></draw:frame> after</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();

    let text_box = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|inline| match inline {
            InlineNode::TextBox(text_box) => Some(text_box.clone()),
            _ => None,
        })
        .expect("inline text box");
    let box_text = match text_box.blocks.as_slice() {
        [BlockNode::Paragraph(paragraph)] => match paragraph.inlines.as_slice() {
            [InlineNode::Run(run)] => run.text.clone(),
            other => panic!("unexpected text-box body inlines: {other:?}"),
        },
        other => panic!("unexpected text-box blocks: {other:?}"),
    };
    assert_eq!(box_text, "box & text");

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<draw:frame xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"><draw:text-box>"#
        ) && content_xml.contains("</draw:text-box></draw:frame>"),
        "text box not round-tripped: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn start_form_image_nested_in_text_box_does_not_hijack_the_frame() {
    // Regression (review Finding A): a Start-form draw:image inside a text box's
    // body must be treated as box content (flattened away), NOT captured as the
    // frame's image — otherwise the frame imports as an image and the box is lost,
    // inconsistently with the self-closing image form.
    let body = r#"<text:p><draw:frame xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink"><draw:text-box><text:p>caption<draw:image xlink:href="Pictures/x.png"><svg:title/></draw:image></text:p></draw:text-box></draw:frame></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let is_text_box = paragraph(&import, 0)
        .inlines
        .iter()
        .any(|inline| matches!(inline, InlineNode::TextBox(_)));
    let is_drawing = paragraph(&import, 0)
        .inlines
        .iter()
        .any(|inline| matches!(inline, InlineNode::Drawing(_)));
    assert!(
        is_text_box,
        "frame with a boxed image must import as a text box"
    );
    assert!(!is_drawing, "the nested image must not hijack the frame");
}

#[test]
fn multi_paragraph_text_box_flattens_and_round_trips() {
    // Two body paragraphs flatten to one paragraph with a line break between
    // them, which re-exports as text:line-break and re-imports to the same char.
    let body = r#"<text:p><draw:frame xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"><draw:text-box><text:p>one</text:p><text:p>two</text:p></draw:text-box></draw:frame></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn tracked_insertion_round_trips_to_a_fixed_point() {
    use casual_doc_model::v1::{Revision, RevisionKind};
    let body = r#"<text:tracked-changes xmlns:dc="http://purl.org/dc/elements/1.1/"><text:changed-region text:id="ct1"><text:insertion><office:change-info><dc:creator>Ada</dc:creator><dc:date>2024-01-02T03:04:05</dc:date></office:change-info></text:insertion></text:changed-region></text:tracked-changes><text:p>Before <text:change-start text:change-id="ct1"/>inserted<text:change-end text:change-id="ct1"/> after</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();

    // The inserted span becomes a Revision wrapping its run, flanked by the
    // surrounding text.
    let revision = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|inline| match inline {
            InlineNode::Revision(revision) => Some(revision.clone()),
            _ => None,
        })
        .expect("insertion revision");
    assert_eq!(revision.kind, RevisionKind::Insertion);
    assert_eq!(revision.author.as_deref(), Some("Ada"));
    assert_eq!(revision.date.as_deref(), Some("2024-01-02T03:04:05"));
    assert_eq!(revision.revision_id.as_deref(), Some("ct1"));
    let Revision { inlines, .. } = &revision;
    assert!(
        matches!(inlines.as_slice(), [InlineNode::Run(run)] if run.text == "inserted"),
        "revision must wrap the inserted run: {inlines:?}"
    );

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    for expected in [
        r#"<text:tracked-changes xmlns:dc="http://purl.org/dc/elements/1.1/"><text:changed-region text:id="ct1"><text:insertion><office:change-info><dc:creator>Ada</dc:creator><dc:date>2024-01-02T03:04:05</dc:date></office:change-info></text:insertion></text:changed-region></text:tracked-changes>"#,
        r#"<text:change-start text:change-id="ct1"/>"#,
        r#"<text:change-end text:change-id="ct1"/>"#,
    ] {
        assert!(
            content_xml.contains(expected),
            "missing {expected}: {content_xml}"
        );
    }

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn insertion_of_only_an_unpaired_bookmark_degrades_not_aborts() {
    // Regression: an insertion wrapping only an unpaired bookmark-start captures a
    // non-empty draft (so the change-end empty-guard passes), but build_inlines
    // later drops the unpaired bookmark, leaving an empty Revision — which the
    // model rejects (EmptyRevision), aborting the WHOLE import. normalize must
    // drop the emptied revision first so it degrades gracefully.
    let body = r#"<text:tracked-changes xmlns:dc="http://purl.org/dc/elements/1.1/"><text:changed-region text:id="c1"><text:insertion><office:change-info><dc:creator>A</dc:creator></office:change-info></text:insertion></text:changed-region></text:tracked-changes><text:p>keep<text:change-start text:change-id="c1"/><text:bookmark-start text:name="bm"/><text:change-end text:change-id="c1"/> end</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .expect("must degrade, not error");
    import
        .document
        .validate()
        .expect("must build a valid model");
    // No revision survives (it normalized empty), but the surrounding text does.
    let has_revision = paragraph(&import, 0)
        .inlines
        .iter()
        .any(|inline| matches!(inline, InlineNode::Revision(_)));
    assert!(!has_revision, "the emptied revision must be dropped");
}

#[test]
fn comment_annotation_round_trips_to_a_fixed_point() {
    // A two-word author with an ampersand exercises PCDATA escaping and the
    // whitespace path that a naive `write_text` (which encodes spaces as
    // `<text:s/>`) would corrupt.
    let body = r#"<text:p>Before <office:annotation xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:creator>Ada &amp; Grace</dc:creator><dc:date>2024-01-02T03:04:05</dc:date><text:p>Please revise this.</text:p></office:annotation>after</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();

    // The annotation lands as a single point `CommentReference`, flanked by the
    // surrounding text runs.
    let comment_id = paragraph(&import, 0)
        .inlines
        .iter()
        .find_map(|inline| match inline {
            InlineNode::CommentReference(reference) => Some(reference.comment),
            _ => None,
        })
        .expect("comment reference");
    let comment = import
        .document
        .definitions()
        .comments
        .get(&comment_id)
        .expect("comment definition");
    assert_eq!(comment.author.as_deref(), Some("Ada & Grace"));
    assert_eq!(comment.date.as_deref(), Some("2024-01-02T03:04:05"));
    let body_text = match comment.blocks.as_slice() {
        [BlockNode::Paragraph(paragraph)] => match paragraph.inlines.as_slice() {
            [InlineNode::Run(run)] => run.text.clone(),
            other => panic!("unexpected comment body inlines: {other:?}"),
        },
        other => panic!("unexpected comment body blocks: {other:?}"),
    };
    assert_eq!(body_text, "Please revise this.");

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    for expected in [
        r#"<office:annotation xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
        // The space is preserved as literal PCDATA (not re-encoded as <text:s/>)
        // and the ampersand is escaped.
        r#"<dc:creator>Ada &amp; Grace</dc:creator>"#,
        r#"<dc:date>2024-01-02T03:04:05</dc:date>"#,
        r#"</office:annotation>"#,
    ] {
        assert!(
            content_xml.contains(expected),
            "annotation missing {expected}: {content_xml}"
        );
    }

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn reference_fields_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::FieldKind;
    let body = r#"<text:p><text:bookmark-ref text:reference-format="text" text:ref-name="mark">X</text:bookmark-ref> <text:bookmark-ref text:reference-format="page" text:ref-name="mark">3</text:bookmark-ref></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let kinds: Vec<FieldKind> = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.kind.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            FieldKind::Ref {
                bookmark: "mark".to_owned()
            },
            FieldKind::PageRef {
                bookmark: "mark".to_owned()
            },
        ]
    );

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml
            .contains(r#"<text:bookmark-ref text:reference-format="text" text:ref-name="mark"/>"#)
            && content_xml.contains(
                r#"<text:bookmark-ref text:reference-format="page" text:ref-name="mark"/>"#
            ),
        "reference field elements missing: {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn field_inside_hyperlink_degrades_not_aborts() {
    // The model forbids a Field nested in an inline wrapper, so a page-number
    // inside a hyperlink must fall through to the degrade path (imported as the
    // link's text), never abort the whole document.
    let body = r##"<text:p>See <text:a xlink:type="simple" xlink:href="http://example.com/">link <text:page-number>1</text:page-number></text:a> end</text:p>"##;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let inlines = &paragraph(&import, 0).inlines;
    assert!(
        !inlines
            .iter()
            .any(|inline| matches!(inline, InlineNode::Field(_))),
        "a field in a hyperlink must not be modeled"
    );
    assert!(
        inlines
            .iter()
            .any(|inline| matches!(inline, InlineNode::Hyperlink(_))),
        "the hyperlink itself must survive"
    );
}

#[test]
fn page_number_and_count_fields_round_trip_to_a_fixed_point() {
    use casual_doc_model::v1::FieldKind;
    let body = r#"<text:p>Page <text:page-number>1</text:page-number> of <text:page-count>5</text:page-count></text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let kinds: Vec<FieldKind> = paragraph(&import, 0)
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.kind.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec![FieldKind::Page, FieldKind::NumPages]);

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains("<text:page-number/>") && content_xml.contains("<text:page-count/>"),
        "field elements missing (cache must be dropped): {content_xml}"
    );

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn hyperlinks_and_bookmarks_survive_export_round_trip() {
    // Both are imported faithfully; a semantic export must re-emit the `text:a`
    // wrapper (external + internal targets) and the `text:bookmark-start`/`-end`
    // markers, not drop them, and reopen to a byte-exact fixed point.
    let body = r##"<text:p><text:bookmark-start text:name="mark"/>Go to <text:a xlink:type="simple" xlink:href="https://example.com/">site</text:a> then <text:a xlink:type="simple" xlink:href="#mark">back</text:a><text:bookmark-end text:name="mark"/></text:p>"##;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();

    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(
            r#"<text:a xlink:type="simple" xlink:href="https://example.com/">site</text:a>"#
        ),
        "external hyperlink must round-trip: {content_xml}"
    );
    assert!(
        content_xml.contains(r##"<text:a xlink:type="simple" xlink:href="#mark">back</text:a>"##),
        "internal hyperlink must round-trip: {content_xml}"
    );
    assert!(content_xml.contains(r#"<text:bookmark-start text:name="mark"/>"#));
    assert!(content_xml.contains(r#"<text:bookmark-end text:name="mark"/>"#));

    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(
        first.bytes, second.bytes,
        "hyperlink/bookmark export must be a fixed point"
    );
}

#[test]
fn bookmark_name_with_control_char_round_trips_via_numeric_ref() {
    // A bookmark name with a tab (a legal XML char) survives import; export emits
    // it as a numeric character reference (`&#9;`) so it round-trips byte-exactly
    // through XML attribute-value normalization, rather than being dropped.
    let body = r#"<text:p><text:bookmark text:name="a&#9;b"/>x</text:p>"#;
    let import = import_content_xml(
        &content("1.4", body),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    import.document.validate().unwrap();
    let first = write_odt(&import.document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        content_xml.contains(r#"text:name="a&#9;b""#),
        "bookmark tab must be emitted as a numeric ref: {content_xml}"
    );
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    let second = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
}

#[test]
fn export_degrades_hyperlink_with_blocked_scheme() {
    // A blocked scheme the importer would refuse must not be re-emitted as a
    // live link (a non-ODT-origin document can carry one). Export degrades it to
    // the inner text, matching the importer's allowlist.
    let import = import_content_xml(
        &content(
            "1.4",
            r#"<text:p><text:a xlink:type="simple" xlink:href="https://ok.example/">clickme</text:a></text:p>"#,
        ),
        OdfVersion::V1_4,
        OdfImportLimits::default(),
    )
    .unwrap();
    let mut document = import.document;
    if let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0]
        && let InlineNode::Hyperlink(link) = &mut paragraph.inlines[0]
        && let HyperlinkTarget::External(target) = &mut link.target
    {
        target.url = "javascript:alert(1)".to_owned();
    }
    let export = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_xml = String::from_utf8(package.read_part(crate::CONTENT_PART).unwrap()).unwrap();
    assert!(
        !content_xml.contains("javascript:") && !content_xml.contains("<text:a"),
        "blocked scheme must not survive export: {content_xml}"
    );
    assert!(
        content_xml.contains("clickme"),
        "inner text must survive: {content_xml}"
    );
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
fn out_of_range_and_invalid_item_start_values_degrade_not_abort() {
    let styles = r#"<text:list-style style:name="L"><text:list-level-style-number text:level="1" style:num-format="1"/></text:list-style>"#;
    // 40000 is in u16 range but exceeds the model domain: clamp like the style
    // path. 70000 / 0 / empty are non-representable: degrade the item, never
    // abort the whole import.
    for (value, expected) in [
        ("40000", Some(32_767_u16)),
        ("70000", None),
        ("0", None),
        ("", None),
    ] {
        let body = format!(
            r#"<text:list text:style-name="L"><text:list-item text:start-value="{value}"><text:p>x</text:p></text:list-item></text:list>"#
        );
        let import = import_content_xml(
            &styled_content(styles, &body),
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
        match expected {
            Some(start) => {
                assert_eq!(
                    instance.overrides.first().and_then(|o| o.start),
                    Some(start)
                )
            }
            None => {
                assert!(
                    instance.overrides.is_empty(),
                    "value {value:?} should not map"
                );
                assert!(
                    import
                        .report
                        .entries
                        .iter()
                        .any(|entry| entry.feature == "odf.list.item-override")
                );
            }
        }
    }
}

#[test]
fn bullet_level_item_start_value_is_dropped_and_reported() {
    let styles = r#"<text:list-style style:name="B"><text:list-level-style-bullet text:level="1" text:bullet-char="&#8226;"/></text:list-style>"#;
    let body = r#"<text:list text:style-name="B"><text:list-item text:start-value="5"><text:p>x</text:p></text:list-item></text:list>"#;
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
    assert!(instance.overrides.is_empty());
    assert!(
        import
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.list.item-override")
    );
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
fn wrong_version_document_kind_and_dtd_fail_closed_active_content_is_dropped() {
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

    // Active content inside content.xml (macros, event listeners) is not a
    // stored active-content *part*; it is dropped wholesale with a security
    // finding rather than aborting the document, so real producer output (which
    // routinely emits an empty office:scripts) still imports. Nothing survives
    // into the model, so no macro or handler code is ever re-emitted.
    let active = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:script="urn:oasis:names:tc:opendocument:xmlns:script:1.0" office:version="1.4"><office:scripts><script:event-listener/></office:scripts><office:body><office:text/></office:body></office:document-content>"#;
    let imported =
        import_content_xml(active, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert!(imported.report.entries.iter().any(|entry| {
        entry.feature == "odf.security.active-content-dropped"
            && entry.model_outcome == ModelOutcome::Degraded
    }));

    let event_listener = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:script="urn:oasis:names:tc:opendocument:xmlns:script:1.0" office:version="1.4"><office:body><office:text><script:event-listener/></office:text></office:body></office:document-content>"#;
    let imported =
        import_content_xml(event_listener, OdfVersion::V1_4, OdfImportLimits::default()).unwrap();
    assert!(imported.report.entries.iter().any(|entry| {
        entry.feature == "odf.security.active-content-dropped"
            && entry.model_outcome == ModelOutcome::Degraded
    }));

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
