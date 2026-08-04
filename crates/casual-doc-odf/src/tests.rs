use std::io::{Cursor, Write};

use casual_doc_model::v1::{Alignment, BlockNode, Color, InlineNode, RgbColor};
use casual_doc_package::CancellationToken;
use zip::CompressionMethod;
use zip::write::{FullFileOptions, ZipWriter};

use crate::{
    CONTENT_PART, MANIFEST_PART, MIMETYPE_PART, ODT_MIME, OdfError, OdfImportLimits,
    OdfPackageLimits, OdfVersion, OdtPackage, STYLES_PART,
};

const CONTENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
 xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 office:version="1.4"><office:body><office:text><text:p>Hello ODT</text:p></office:text></office:body></office:document-content>"#;

#[derive(Clone)]
struct Entry {
    name: &'static str,
    bytes: Vec<u8>,
    compression: CompressionMethod,
    local_extra: bool,
}

fn manifest(version: &str, root_mime: &str, content_extra: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="{version}">
  <m:file-entry m:media-type="{root_mime}" m:full-path="/" m:version="{version}"/>
  <m:file-entry m:full-path="content.xml" m:media-type="text/xml">{content_extra}</m:file-entry>
</m:manifest>"#
    )
    .into_bytes()
}

fn minimal_entries(version: &str) -> Vec<Entry> {
    vec![
        Entry {
            name: MIMETYPE_PART,
            bytes: ODT_MIME.as_bytes().to_vec(),
            compression: CompressionMethod::Stored,
            local_extra: false,
        },
        Entry {
            name: MANIFEST_PART,
            bytes: manifest(version, ODT_MIME, ""),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: CONTENT_PART,
            bytes: CONTENT.to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]
}

fn package(entries: &[Entry]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let mut options = FullFileOptions::default().compression_method(entry.compression);
        if entry.local_extra {
            options.add_extra_data(0xcafe, b"odf", false).unwrap();
        }
        writer.start_file(entry.name, options).unwrap();
        writer.write_all(&entry.bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn odf_1_2_through_1_4_are_admitted_deterministically() {
    for (version, expected) in [
        ("1.2", OdfVersion::V1_2),
        ("1.3", OdfVersion::V1_3),
        ("1.4", OdfVersion::V1_4),
    ] {
        let bytes = package(&minimal_entries(version));
        let mut odt = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
        assert_eq!(odt.version(), expected);
        assert_eq!(odt.version().as_str(), version);
        assert_eq!(odt.read_part(CONTENT_PART).unwrap(), CONTENT);
        assert_eq!(
            odt.manifest_entries()
                .iter()
                .map(|entry| entry.full_path.as_str())
                .collect::<Vec<_>>(),
            vec!["/", CONTENT_PART]
        );
        assert!(!odt.has_signatures());
        if expected == OdfVersion::V1_4 {
            let imported = odt.import_document(OdfImportLimits::default()).unwrap();
            assert_eq!(imported.document.body().len(), 1);
        }
    }
}

#[test]
fn named_styles_part_and_parent_chain_apply_to_content() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p text:style-name="PChild"><text:span text:style-name="TChild">named</text:span></text:p></office:text></office:body></office:document-content>"#;
    let styles = br##"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="PBase" style:family="paragraph"><style:paragraph-properties fo:text-align="end"/></style:style><style:style style:name="PChild" style:family="paragraph" style:parent-style-name="PBase"/><style:style style:name="TBase" style:family="text"><style:text-properties fo:font-weight="bold" fo:color="#123456"/></style:style><style:style style:name="TMid" style:family="text" style:parent-style-name="TBase"><style:text-properties fo:font-style="italic"/></style:style><style:style style:name="TChild" style:family="text" style:parent-style-name="TMid"><style:text-properties style:text-underline-style="none"/></style:style></office:styles></office:document-styles>"##;
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="styles.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let entries = vec![
        Entry {
            name: MIMETYPE_PART,
            bytes: ODT_MIME.as_bytes().to_vec(),
            compression: CompressionMethod::Stored,
            local_extra: false,
        },
        Entry {
            name: MANIFEST_PART,
            bytes: manifest,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: CONTENT_PART,
            bytes: content.to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: STYLES_PART,
            bytes: styles.to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ];
    let bytes = package(&entries);
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    assert!(imported.report.entries.is_empty());
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    assert_eq!(paragraph.properties.alignment, Some(Alignment::End));
    let InlineNode::Run(run) = &paragraph.inlines[0] else {
        panic!("run")
    };
    assert_eq!(run.properties.bold, Some(true));
    assert_eq!(run.properties.italic, Some(true));
    assert_eq!(run.properties.underline, Some(false));
    assert_eq!(
        run.properties.color,
        Some(Color::Rgb(RgbColor {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        }))
    );

    let error = package
        .import_document(OdfImportLimits {
            max_styles_bytes: 8,
            ..OdfImportLimits::default()
        })
        .unwrap_err();
    assert!(matches!(
        error,
        OdfError::LimitExceeded {
            limit: "odf_styles_bytes",
            ..
        }
    ));
}

const STYLES_PREFIX: &str = r#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="21cm" fo:page-height="29.7cm" fo:margin-top="2cm" fo:margin-bottom="2cm" fo:margin-left="2cm" fo:margin-right="2cm" style:print-orientation="portrait"/></style:page-layout></office:automatic-styles>"#;

/// Wraps a `<office:master-styles>` body in a full styles.xml document with a
/// bound page-layout so the importer creates a section for the header/footer.
fn styles_with_master(master_body: &str) -> Vec<u8> {
    format!("{STYLES_PREFIX}{master_body}</office:document-styles>").into_bytes()
}

/// Builds an ODT package from the shared content plus a styles.xml part.
fn package_with_styles(styles: Vec<u8>) -> Vec<u8> {
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="styles.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    package(&[
        Entry {
            name: MIMETYPE_PART,
            bytes: ODT_MIME.as_bytes().to_vec(),
            compression: CompressionMethod::Stored,
            local_extra: false,
        },
        Entry {
            name: MANIFEST_PART,
            bytes: manifest,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: CONTENT_PART,
            bytes: CONTENT.to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
        Entry {
            name: STYLES_PART,
            bytes: styles,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ])
}

#[test]
fn master_page_header_and_footer_are_imported_deterministically() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Header text</text:p></style:header><style:footer><text:p>Footer<text:tab/>Right</text:p></style:footer></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();

    imported.document.validate().unwrap();
    // The page-layout in styles.xml produces its own pre-existing style-catalog
    // findings; plain header/footer content must add none of its own.
    assert!(
        !imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature.starts_with("odf.master-page")),
        "plain header/footer content should not degrade: {:?}",
        imported.report.entries
    );

    let section = &imported.document.definitions().sections[0];
    assert_eq!(section.headers.len(), 1);
    assert_eq!(section.footers.len(), 1);
    let header_ref = section.headers[0];
    let footer_ref = section.footers[0];
    assert_eq!(
        header_ref.kind,
        casual_doc_model::v1::HeaderFooterKind::Default
    );
    assert_eq!(
        footer_ref.kind,
        casual_doc_model::v1::HeaderFooterKind::Default
    );

    let header = imported
        .document
        .definitions()
        .headers
        .get(&header_ref.reference)
        .expect("header definition");
    let BlockNode::Paragraph(paragraph) = &header.blocks[0] else {
        panic!("header paragraph")
    };
    let InlineNode::Run(run) = &paragraph.inlines[0] else {
        panic!("header run")
    };
    assert_eq!(run.text, "Header text");

    let footer = imported
        .document
        .definitions()
        .footers
        .get(&footer_ref.reference)
        .expect("footer definition");
    let BlockNode::Paragraph(paragraph) = &footer.blocks[0] else {
        panic!("footer paragraph")
    };
    assert!(matches!(paragraph.inlines[1], InlineNode::Tab(_)));

    // Import is deterministic for identical bytes.
    let again = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    assert_eq!(imported, again);
}

#[test]
fn even_page_header_maps_to_even_kind() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Odd</text:p></style:header><style:header-left><text:p>Even</text:p></style:header-left></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();

    let section = &imported.document.definitions().sections[0];
    assert_eq!(section.headers.len(), 2);
    let kinds: Vec<_> = section
        .headers
        .iter()
        .map(|reference| reference.kind)
        .collect();
    assert!(kinds.contains(&casual_doc_model::v1::HeaderFooterKind::Default));
    assert!(kinds.contains(&casual_doc_model::v1::HeaderFooterKind::Even));
    assert_ne!(section.headers[0].reference, section.headers[1].reference);
    assert!(
        imported
            .document
            .definitions()
            .settings
            .even_and_odd_headers
    );
}

#[test]
fn unsupported_header_content_is_reported_not_dropped_silently() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p><text:span text:style-name="Bold">Title</text:span></text:p></style:header><style:header-first><text:p>First</text:p></style:header-first></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();

    // The span text survives, but its formatting and the first-page region are
    // explicit findings.
    let header = &imported.document.definitions().sections[0].headers[0];
    let header = imported
        .document
        .definitions()
        .headers
        .get(&header.reference)
        .unwrap();
    let BlockNode::Paragraph(paragraph) = &header.blocks[0] else {
        panic!("paragraph")
    };
    let InlineNode::Run(run) = &paragraph.inlines[0] else {
        panic!("run")
    };
    assert_eq!(run.text, "Title");
    assert!(run.properties.bold.is_none());

    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.master-page.run-formatting")
    );
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.master-page.first-page-region")
    );
}

#[test]
fn oversized_header_content_fails_closed() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>a</text:p><text:p>b</text:p></style:header></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let error = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits {
            max_paragraphs: 1,
            ..OdfImportLimits::default()
        })
        .unwrap_err();
    assert!(matches!(
        error,
        OdfError::LimitExceeded {
            limit: "odf_content_paragraphs",
            ..
        }
    ));
}

#[test]
fn master_page_round_trips_as_a_byte_and_semantic_fixed_point() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Header</text:p></style:header><style:header-left><text:p>Even</text:p></style:header-left><style:footer><text:p>Page<text:tab/>1</text:p></style:footer></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let document = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap()
        .document;

    let first = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    // Deterministic output for identical input.
    let second = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
    // Header/footer content flows to styles.xml, not content.xml.
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(package.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(
        styles_out
            .contains(r#"<style:master-page style:name="Standard" style:page-layout-name="pm1">"#)
    );
    assert!(styles_out.contains("<style:header><text:p>Header</text:p></style:header>"));
    assert!(styles_out.contains("<style:header-left><text:p>Even</text:p></style:header-left>"));
    assert!(styles_out.contains("<style:footer><text:p>Page<text:tab/>1</text:p></style:footer>"));
    let content_out = String::from_utf8(package.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!content_out.contains("Header"));

    // Semantic round trip: reopening the written bytes reproduces the model.
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, document);
    // Byte fixed point: re-exporting the reopened model is identical.
    let reexported =
        crate::write_odt(&reopened.document, crate::OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, first.bytes);
}

#[test]
fn geometry_only_styles_have_no_master_page() {
    let bytes = package_with_styles(styles_with_master(""));
    let document = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap()
        .document;
    let export = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(package.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(!styles_out.contains("master-page"));
    assert!(!styles_out.contains("xmlns:text"));
}

#[test]
fn header_trailing_text_is_charged_against_inline_budget() {
    // "a" <tab> "b" is three inline nodes; the trailing "b" must be counted like
    // any other run, so a budget of 2 fails closed (regression: it used to slip).
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>a<text:tab/>b</text:p></style:header></style:master-page></office:master-styles>"#,
    );
    let bytes = package_with_styles(styles);
    let error = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits {
            max_inline_nodes: 2,
            ..OdfImportLimits::default()
        })
        .unwrap_err();
    assert!(matches!(
        error,
        OdfError::LimitExceeded {
            limit: "odf_content_inline_nodes",
            ..
        }
    ));
}

#[test]
fn out_of_domain_page_geometry_is_clamped_and_reported() {
    // A 50in x 60in page exceeds the model domain (max 31,680 twips = 22in). It
    // must clamp and stay valid whether or not a header is present (regression:
    // the header path aborted and the header-less path returned an invalid model).
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="50in" fo:page-height="60in" fo:margin-top="2cm" fo:margin-bottom="2cm" fo:margin-left="2cm" fo:margin-right="2cm" style:print-orientation="portrait"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>H</text:p></style:header></style:master-page></office:master-styles></office:document-styles>"#.to_vec();
    let imported = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();
    let section = &imported.document.definitions().sections[0];
    assert_eq!(section.page_size.width_twips, 31_680);
    assert_eq!(section.page_size.height_twips, 31_680);
    assert_eq!(section.headers.len(), 1);
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.page-layout.out-of-range")
    );
}

#[test]
fn empty_header_region_is_dropped_on_import_and_export() {
    let styles = styles_with_master(
        r#"<office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header></style:header><style:footer><text:p>F</text:p></style:footer></style:master-page></office:master-styles>"#,
    );
    let document = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap()
        .document;
    let section = &document.definitions().sections[0];
    assert!(
        section.headers.is_empty(),
        "empty header must not become a def"
    );
    assert_eq!(section.footers.len(), 1);

    let export = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    let mut package = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(package.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(!styles_out.contains("<style:header>"));
    assert!(styles_out.contains("<style:footer><text:p>F</text:p></style:footer>"));
}

#[test]
fn document_default_styles_map_to_document_defaults() {
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:default-style style:family="paragraph"><style:paragraph-properties fo:text-align="center"/></style:default-style><style:default-style style:family="text"><style:text-properties fo:font-weight="bold"/></style:default-style></office:styles></office:document-styles>"#.to_vec();
    let imported = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();
    let defaults = imported
        .document
        .definitions()
        .document_defaults
        .as_ref()
        .expect("document defaults");
    assert_eq!(
        defaults.paragraph.as_ref().unwrap().alignment,
        Some(Alignment::Center)
    );
    assert_eq!(defaults.run.as_ref().unwrap().bold, Some(true));

    // Export emits an office:styles default-style block and round-trips.
    let document = imported.document;
    let first = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    let second = crate::write_odt(&document, crate::OdfExportLimits::default()).unwrap();
    assert_eq!(first.bytes, second.bytes);
    let mut package = OdtPackage::open(&first.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(package.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(
        "<office:styles><style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:text-align=\"center\"/></style:default-style>"
    ));
    assert!(styles_out.contains(
        "<style:default-style style:family=\"text\"><style:text-properties fo:font-weight=\"bold\"/></style:default-style></office:styles>"
    ));
    let reopened = package.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, document);
    let reexported =
        crate::write_odt(&reopened.document, crate::OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, first.bytes);
}

#[test]
fn mimetype_must_be_first_stored_exact_and_without_extra_data() {
    let mut not_first = minimal_entries("1.4");
    not_first.swap(0, 1);
    assert_eq!(
        OdtPackage::open(&package(&not_first), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeNotFirst
    );

    let mut compressed = minimal_entries("1.4");
    compressed[0].compression = CompressionMethod::Deflated;
    assert_eq!(
        OdtPackage::open(&package(&compressed), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeCompressed
    );

    let mut extra = minimal_entries("1.4");
    extra[0].local_extra = true;
    assert_eq!(
        OdtPackage::open(&package(&extra), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MimetypeExtraField
    );

    let mut wrong = minimal_entries("1.4");
    wrong[0].bytes = b"application/vnd.oasis.opendocument.spreadsheet".to_vec();
    assert_eq!(
        OdtPackage::open(&package(&wrong), OdfPackageLimits::default()).unwrap_err(),
        OdfError::InvalidMimetype
    );
}

#[test]
fn manifest_root_coverage_duplicates_and_versions_fail_closed() {
    let mut wrong_root = minimal_entries("1.4");
    wrong_root[1].bytes = manifest("1.4", "application/octet-stream", "");
    assert_eq!(
        OdtPackage::open(&package(&wrong_root), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut missing_content = minimal_entries("1.4");
    missing_content[1].bytes = format!(
        r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}"/></manifest:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&missing_content), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut duplicate = minimal_entries("1.4");
    duplicate[1].bytes = manifest(
        "1.4",
        ODT_MIME,
        r#"</m:file-entry><m:file-entry m:full-path="content.xml" m:media-type="text/xml">"#,
    );
    assert_eq!(
        OdtPackage::open(&package(&duplicate), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let unsupported = package(&minimal_entries("1.5"));
    assert_eq!(
        OdtPackage::open(&unsupported, OdfPackageLimits::default()).unwrap_err(),
        OdfError::UnsupportedVersion
    );

    let mut unsafe_directory = minimal_entries("1.4");
    unsafe_directory[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="../" m:media-type=""/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&unsafe_directory), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );

    let mut encoded_traversal = minimal_entries("1.4");
    encoded_traversal[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="%2e%2e/" m:media-type=""/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&encoded_traversal), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ManifestMismatch
    );
}

#[test]
fn encryption_dtd_and_active_content_are_refused() {
    let mut encrypted = minimal_entries("1.4");
    encrypted[1].bytes = manifest("1.4", ODT_MIME, "<m:encryption-data/>");
    assert_eq!(
        OdtPackage::open(&package(&encrypted), OdfPackageLimits::default()).unwrap_err(),
        OdfError::EncryptedDocument
    );

    let mut dtd = minimal_entries("1.4");
    dtd[1].bytes = format!(
        r#"<!DOCTYPE manifest [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&dtd), OdfPackageLimits::default()).unwrap_err(),
        OdfError::MalformedManifest
    );

    let mut active = minimal_entries("1.4");
    active.push(Entry {
        name: "Basic/Standard/Module1.xml",
        bytes: b"macro".to_vec(),
        compression: CompressionMethod::Stored,
        local_extra: false,
    });
    active[1].bytes = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="Basic/Standard/Module1.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    assert_eq!(
        OdtPackage::open(&package(&active), OdfPackageLimits::default()).unwrap_err(),
        OdfError::ActiveContent
    );
}

#[test]
fn signatures_are_presence_facts_not_validity_claims() {
    let mut entries = minimal_entries("1.4");
    entries.push(Entry {
        name: "META-INF/documentsignatures.xml",
        bytes: b"<signatures/>".to_vec(),
        compression: CompressionMethod::Deflated,
        local_extra: false,
    });
    let bytes = package(&entries);
    let odt = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    assert!(odt.has_signatures());
}

#[test]
fn limits_and_cancellation_are_enforced_atomically() {
    let bytes = package(&minimal_entries("1.4"));
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert_eq!(
        OdtPackage::open_with_cancellation(&bytes, OdfPackageLimits::default(), &cancellation,)
            .unwrap_err(),
        OdfError::Cancelled
    );

    let limits = OdfPackageLimits {
        max_manifest_bytes: 8,
        ..OdfPackageLimits::default()
    };
    assert!(matches!(
        OdtPackage::open(&bytes, limits),
        Err(OdfError::LimitExceeded {
            limit: "odf_manifest_bytes",
            ..
        })
    ));

    let invalid = OdfPackageLimits {
        max_xml_depth: usize::MAX,
        ..OdfPackageLimits::default()
    };
    assert!(matches!(
        OdtPackage::open(&bytes, invalid),
        Err(OdfError::InvalidLimitConfiguration {
            limit: "odf_xml_depth",
            ..
        })
    ));
}
