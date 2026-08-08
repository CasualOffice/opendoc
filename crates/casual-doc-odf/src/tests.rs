use std::io::{Cursor, Write};

use casual_doc_model::v1::{
    Alignment, BlockNode, Color, DashStyle, Fill, GradientKind, GradientStop, GroupChild,
    HorizontalAnchor, HorizontalPosition, InlineNode, RgbColor, ShapeGeometry, ShapeStroke,
    StyleKind, VerticalAnchor, VerticalPosition, WrapMode,
};
use casual_doc_package::CancellationToken;
use zip::CompressionMethod;
use zip::write::{FullFileOptions, ZipWriter};

use crate::{
    CONTENT_PART, MANIFEST_PART, MIMETYPE_PART, ODT_MIME, OdfError, OdfExportLimits,
    OdfImportLimits, OdfPackageLimits, OdfVersion, OdtPackage, RetainedPart, STYLES_PART,
    write_odt, write_odt_with_retained_parts,
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
fn named_character_style_round_trips_as_referenced_identity() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p><text:span text:style-name="Emphasis">styled</text:span></text:p></office:text></office:body></office:document-content>"#;
    let styles = br##"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="Emphasis" style:family="text"><style:text-properties fo:font-weight="bold" fo:color="#112233"/></style:style></office:styles></office:document-styles>"##;
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

    // The named style is preserved as a Character identity referenced by the run.
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Run(run) = &paragraph.inlines[0] else {
        panic!("run")
    };
    let style_ref = run.properties.style_ref.expect("run style ref");
    assert_eq!(run.properties.bold, None);
    let style = imported
        .document
        .definitions()
        .styles
        .get(&style_ref)
        .expect("style def");
    assert_eq!(style.kind, StyleKind::Character);
    assert_eq!(style.name.as_deref(), Some("Emphasis"));

    // Export re-emits the named style in styles.xml and references it by name in
    // content.xml — no automatic `T_` run style is minted for a purely named run.
    let export = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(
        r##"<style:style style:family="text" style:name="Emphasis"><style:text-properties fo:font-weight="bold" fo:color="#112233"/></style:style>"##
    ));
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<text:span text:style-name="Emphasis">styled</text:span>"#));
    assert!(!content_out.contains(r#"text:style-name="T_"#));

    // Semantic round trip + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, export.bytes);
}

/// A named character style whose retained name lands in the `T_…` namespace that
/// automatic run styles are minted into must not be re-emitted under that name, or
/// it would collide with an automatic bold run's `T_b1_in_un_sn_cn_zn` style and
/// collapse two distinct styles on re-import (an invalid duplicate `style:name`,
/// and a broken fixed point). The named style is re-minted to a `Char{n}` name.
#[test]
fn named_style_name_in_run_style_namespace_does_not_collide() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="AutoBold" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles><office:body><office:text><text:p><text:span text:style-name="T_b1_in_un_sn_cn_zn">named</text:span><text:span text:style-name="AutoBold">bold</text:span></text:p></office:text></office:body></office:document-content>"#;
    let styles = br##"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="T_b1_in_un_sn_cn_zn" style:family="text"><style:text-properties fo:font-style="italic"/></style:style></office:styles></office:document-styles>"##;
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
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    // The two spans start as distinct styles: a referenced italic Character style
    // and a direct bold run.
    let assert_distinct = |document: &casual_doc_model::v1::Document| {
        let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Run(named) = &paragraph.inlines[0] else {
            panic!("named run")
        };
        let style = document
            .definitions()
            .styles
            .get(&named.properties.style_ref.expect("named run style ref"))
            .expect("named style def");
        assert_eq!(style.run.as_ref().and_then(|run| run.italic), Some(true));
        assert_eq!(named.properties.bold, None);
        let InlineNode::Run(bold) = &paragraph.inlines[1] else {
            panic!("bold run")
        };
        assert_eq!(bold.properties.bold, Some(true));
        assert_eq!(bold.properties.style_ref, None);
    };
    assert_distinct(&imported.document);

    // Export must not name the Character style `T_b1_in_un_sn_cn_zn`; that name is
    // reserved for the automatic bold run style also present in this document.
    let export = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(!styles_out.contains(r#"style:name="T_b1_in_un_sn_cn_zn""#));

    // Re-import keeps both identities distinct, and re-export is a byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_distinct(&reopened.document);
    let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, export.bytes);
}

/// Builds an ODT package from raw `content.xml` and `styles.xml` bytes with a
/// manifest listing both, for the named-style round-trip tests.
fn package_content_and_styles(content: &[u8], styles: &[u8]) -> Vec<u8> {
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
    ])
}

/// A named paragraph style round-trips as a referenced `Style` identity: the
/// paragraph carries a `style_ref` and the style's properties live on the
/// definition (emitted once in styles.xml), a byte + semantic fixed point.
#[test]
fn named_paragraph_style_round_trips_as_referenced_identity() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p text:style-name="Quote">quoted</text:p></office:text></office:body></office:document-content>"#;
    let styles = br##"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="Quote" style:family="paragraph"><style:paragraph-properties fo:text-align="end"/></style:style></office:styles></office:document-styles>"##;
    let bytes = package_content_and_styles(content, styles);
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    assert!(imported.report.entries.is_empty());

    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    assert_eq!(paragraph.properties.alignment, None);
    let style = imported
        .document
        .definitions()
        .styles
        .get(&paragraph.properties.style_ref.expect("paragraph style ref"))
        .expect("paragraph style def");
    assert_eq!(style.kind, StyleKind::Paragraph);
    assert_eq!(style.name.as_deref(), Some("Quote"));

    let export = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(
        r#"<style:style style:family="paragraph" style:name="Quote"><style:paragraph-properties fo:text-align="end"/></style:style>"#
    ));
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<text:p text:style-name="Quote">quoted</text:p>"#));

    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, export.bytes);
}

/// A named paragraph style whose retained name matches the automatic paragraph
/// style scheme (`P_center`) must be re-minted so it cannot collide with a
/// direct-center paragraph's automatic `P_center` style — an invalid duplicate
/// `style:name` and a broken fixed point otherwise.
#[test]
fn named_paragraph_style_name_in_automatic_namespace_does_not_collide() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="AutoCenter" style:family="paragraph"><style:paragraph-properties fo:text-align="center"/></style:style></office:automatic-styles><office:body><office:text><text:p text:style-name="P_center">named</text:p><text:p text:style-name="AutoCenter">centered</text:p></office:text></office:body></office:document-content>"#;
    let styles = br##"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:style style:name="P_center" style:family="paragraph"><style:paragraph-properties fo:text-align="end"/></style:style></office:styles></office:document-styles>"##;
    let bytes = package_content_and_styles(content, styles);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    // The two paragraphs start distinct: a referenced end-aligned paragraph style
    // and a direct center-aligned paragraph.
    let assert_distinct = |document: &casual_doc_model::v1::Document| {
        let BlockNode::Paragraph(named) = &document.body()[0] else {
            panic!("named paragraph")
        };
        let style = document
            .definitions()
            .styles
            .get(&named.properties.style_ref.expect("paragraph style ref"))
            .expect("paragraph style def");
        assert_eq!(
            style
                .paragraph
                .as_ref()
                .and_then(|properties| properties.alignment),
            Some(Alignment::End)
        );
        assert_eq!(named.properties.alignment, None);
        let BlockNode::Paragraph(direct) = &document.body()[1] else {
            panic!("direct paragraph")
        };
        assert_eq!(direct.properties.alignment, Some(Alignment::Center));
        assert_eq!(direct.properties.style_ref, None);
    };
    assert_distinct(&imported.document);

    let export = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(!styles_out.contains(r#"style:name="P_center""#));

    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_distinct(&reopened.document);
    let reexported = write_odt(&reopened.document, OdfExportLimits::default()).unwrap();
    assert_eq!(reexported.bytes, export.bytes);
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
    // The named paragraph style is likewise a referenced `Style` identity; its
    // inheritance-resolved alignment lives on the definition, not on the paragraph.
    assert_eq!(paragraph.properties.alignment, None);
    let paragraph_style = imported
        .document
        .definitions()
        .styles
        .get(&paragraph.properties.style_ref.expect("paragraph style ref"))
        .expect("paragraph style def");
    assert_eq!(paragraph_style.kind, StyleKind::Paragraph);
    assert_eq!(
        paragraph_style
            .paragraph
            .as_ref()
            .and_then(|properties| properties.alignment),
        Some(Alignment::End)
    );
    let InlineNode::Run(run) = &paragraph.inlines[0] else {
        panic!("run")
    };
    // The named character style is preserved as a `Style` identity referenced by
    // the run; its inheritance-resolved properties live on that definition rather
    // than being flattened onto the run.
    assert_eq!(run.properties.bold, None);
    assert_eq!(run.properties.italic, None);
    assert_eq!(run.properties.underline, None);
    assert_eq!(run.properties.color, None);
    let style = imported
        .document
        .definitions()
        .styles
        .get(&run.properties.style_ref.expect("run style ref"))
        .expect("style def");
    let style_run = style.run.as_ref().expect("style run props");
    assert_eq!(style_run.bold, Some(true));
    assert_eq!(style_run.italic, Some(true));
    assert_eq!(style_run.underline, Some(false));
    assert_eq!(
        style_run.color,
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
fn paragraph_default_text_properties_become_run_defaults() {
    // LibreOffice/OpenOffice put the document base font in the paragraph-family
    // default-style's text-properties; those must reach the run defaults.
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:default-style style:family="paragraph"><style:text-properties fo:font-weight="bold" fo:font-size="12pt"/></style:default-style></office:styles></office:document-styles>"#.to_vec();
    let imported = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();
    let run = imported
        .document
        .definitions()
        .document_defaults
        .as_ref()
        .and_then(|defaults| defaults.run.as_ref())
        .expect("run defaults from paragraph default");
    assert_eq!(run.bold, Some(true));
    assert_eq!(run.size_half_points, Some(24));
}

#[test]
fn duplicate_default_style_is_reported_first_wins() {
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:styles><style:default-style style:family="text"><style:text-properties fo:font-weight="bold"/></style:default-style><style:default-style style:family="text"><style:text-properties fo:font-weight="normal"/></style:default-style></office:styles></office:document-styles>"#.to_vec();
    let imported = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    // First default wins (bold=true), and the conflict is disclosed.
    assert_eq!(
        imported
            .document
            .definitions()
            .document_defaults
            .as_ref()
            .unwrap()
            .run
            .as_ref()
            .unwrap()
            .bold,
        Some(true)
    );
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.style.default-style.shadowed")
    );
}

#[test]
fn default_style_in_automatic_styles_is_rejected() {
    // ODF forbids default-style in automatic-styles; it must not inject defaults.
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:default-style style:family="text"><style:text-properties fo:font-weight="bold"/></style:default-style></office:automatic-styles></office:document-styles>"#.to_vec();
    let imported = OdtPackage::open(&package_with_styles(styles), OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    assert!(imported.document.definitions().document_defaults.is_none());
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.style.default-style.placement")
    );
}

fn image_content(href: &str) -> Vec<u8> {
    format!(
        r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p><draw:frame><draw:image xlink:href="{href}"/></draw:frame></text:p></office:text></office:body></office:document-content>"#
    )
    .into_bytes()
}

fn image_package(content: Vec<u8>, image_entries: &str, extra: &[Entry]) -> Vec<u8> {
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/>{image_entries}</m:manifest>"#
    )
    .into_bytes();
    let mut entries = vec![
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ];
    entries.extend_from_slice(extra);
    package(&entries)
}

#[test]
fn part_path_normalization_folds_escape_case() {
    use crate::package::normalized_part_path;
    assert_eq!(
        normalized_part_path("Pictures/a%2bb.png"),
        normalized_part_path("Pictures/a%2Bb.png")
    );
    // Non-escape content (including a lone %) is preserved.
    assert_eq!(normalized_part_path("Pictures/50%.png"), "Pictures/50%.png");
    assert_eq!(normalized_part_path("Pictures/x.png"), "Pictures/x.png");
}

#[test]
fn manifest_media_type_is_authoritative_for_images() {
    // The href extension (.dat) would infer octet-stream; the manifest declares
    // image/png, which must win.
    let bytes = image_package(
        image_content("Pictures/pic.dat"),
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();
    let (_, media) = imported.document.definitions().media.iter().next().unwrap();
    assert_eq!(media.part_name, "Pictures/pic.dat");
    assert_eq!(media.media_type, "image/png");
    assert!(
        !imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.draw.image-missing-part")
    );
}

#[test]
fn preserving_writer_emits_draw_frame_and_repackages_bytes() {
    let bytes = image_package(
        image_content("Pictures/pic.dat"),
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let document = imported.document;

    // Preserving writer: draw:frame + repackaged image bytes.
    let preserved =
        write_odt_with_retained_parts(&document, &retained, OdfExportLimits::default()).unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    let content = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content.contains("<draw:frame"));
    assert!(content.contains("xlink:href=\"Pictures/pic.dat\""));
    assert_eq!(out.read_part("Pictures/pic.dat").unwrap(), b"\x89PNG\r\n");

    // Plain semantic writer drops the image (no draw:frame, no picture part).
    let semantic = write_odt(&document, OdfExportLimits::default()).unwrap();
    let mut plain = OdtPackage::open(&semantic.bytes, OdfPackageLimits::default()).unwrap();
    let plain_content = String::from_utf8(plain.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!plain_content.contains("draw:frame"));
    assert!(plain.read_part("Pictures/pic.dat").is_err());
}

/// A floating (anchored) `draw:frame` — `text:anchor-type` + `svg:x`/`svg:y`
/// offsets + `draw:z-index` — imports to an `AnchoredDrawing` and re-exports through
/// the preserving path to a byte-exact fixed point.
#[test]
fn floating_anchored_image_round_trips() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p><draw:frame text:anchor-type="page" draw:z-index="2" svg:x="5cm" svg:y="3cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();

    // The frame becomes an anchored drawing: page/page references, offset placement
    // (5cm/3cm in EMU), the ODF-default Square wrap, and the z-index.
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    assert_eq!(anchored.extent.width_emu, 4 * 360_000);
    assert_eq!(anchored.extent.height_emu, 2 * 360_000);
    assert_eq!(
        anchored.anchor.horizontal.relative_from,
        HorizontalAnchor::Page
    );
    assert_eq!(anchored.anchor.vertical.relative_from, VerticalAnchor::Page);
    assert_eq!(
        anchored.anchor.horizontal.position,
        HorizontalPosition::Offset(5 * 360_000)
    );
    assert_eq!(
        anchored.anchor.vertical.position,
        VerticalPosition::Offset(3 * 360_000)
    );
    assert_eq!(anchored.anchor.wrap, WrapMode::Square);
    assert!(!anchored.anchor.behind_doc);
    assert_eq!(anchored.relative_height, Some(2));

    // Preserving export re-emits the positioned frame (canonical `cm` form) and
    // repackages the bytes. The default Square wrap carries no graphic style.
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(
        r#"<draw:frame text:anchor-type="page" draw:z-index="2" svg:x="5.0000cm" svg:y="3.0000cm" svg:width="4.0000cm" svg:height="2.0000cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame>"#
    ));
    assert!(!content_out.contains("style:wrap"));

    // Semantic + byte fixed point: re-import equals the model, re-export identical.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A floating frame whose graphic style pins `style:horizontal-rel="page-content"`
/// derives a PRECISE horizontal reference (Margin) rather than the anchor-type
/// default (Page); export re-emits the `style:horizontal-rel` so the reference
/// round-trips to a byte-exact fixed point.
#[test]
fn floating_anchor_horizontal_rel_round_trips() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="fr1" style:family="graphic"><style:graphic-properties style:horizontal-rel="page-content"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:frame draw:style-name="fr1" text:anchor-type="page" svg:x="5cm" svg:y="3cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    // `style:horizontal-rel="page-content"` pins the horizontal reference to Margin;
    // the absent vertical-rel keeps the page anchor-type's Page derivation.
    assert_eq!(
        anchored.anchor.horizontal.relative_from,
        HorizontalAnchor::Margin
    );
    assert_eq!(anchored.anchor.vertical.relative_from, VerticalAnchor::Page);

    // Export re-emits the precise reference through a graphic automatic style, and
    // does NOT emit a `style:vertical-rel` (Page is the page anchor-type default).
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"style:horizontal-rel="page-content""#));
    assert!(!content_out.contains("style:vertical-rel"));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A floating frame with a negative `svg:x` clamps the offset to zero but must
/// REPORT the drop (not lose it silently); the result still round-trips.
/// A floating frame whose graphic style carries a behind-text (run-through) wrap
/// plus text-exclusion distances round-trips those through a `style:family="graphic"`
/// automatic style, to a byte-exact fixed point.
#[test]
fn floating_anchor_wrap_and_distances_round_trip() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="fr1" style:family="graphic"><style:graphic-properties style:wrap="run-through" style:run-through="background" fo:margin-top="0.2cm" fo:margin-bottom="0.2cm" fo:margin-left="0.3cm" fo:margin-right="0.3cm"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:frame draw:style-name="fr1" text:anchor-type="page" svg:x="5cm" svg:y="3cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    // run-through + background → float behind text.
    assert_eq!(anchored.anchor.wrap, WrapMode::None);
    assert!(anchored.anchor.behind_doc);
    assert_eq!(anchored.anchor.wrap_distances.top_emu, 72_000); // 0.2cm
    assert_eq!(anchored.anchor.wrap_distances.bottom_emu, 72_000);
    assert_eq!(anchored.anchor.wrap_distances.start_emu, 108_000); // 0.3cm (left)
    assert_eq!(anchored.anchor.wrap_distances.end_emu, 108_000); // (right)

    // Export re-emits a graphic automatic style carrying the wrap + distances, and
    // the frame references it by name.
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(
        r#"<style:graphic-properties style:wrap="run-through" style:run-through="background" fo:margin-top="0.2000cm" fo:margin-bottom="0.2000cm" fo:margin-left="0.3000cm" fo:margin-right="0.3000cm"/>"#
    ));
    assert!(content_out.contains(r#"style:family="graphic""#));
    assert!(content_out.contains("draw:style-name="));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A child graphic style with `style:parent-style-name` keeps its OWN wrap (child
/// over parent), and a negative `fo:margin-*` is dropped WITH a finding — the two
/// review-found silent losses in the graphic-style path.
#[test]
fn floating_anchor_graphic_style_inheritance_and_bad_margin_report() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="grParent" style:family="graphic"><style:graphic-properties style:wrap="none"/></style:style><style:style style:name="fr1" style:family="graphic" style:parent-style-name="grParent"><style:graphic-properties style:wrap="run-through" style:run-through="background" fo:margin-top="-0.5cm"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:frame draw:style-name="fr1" text:anchor-type="page" svg:x="1cm" svg:y="1cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    // The child's run-through wins over the parent's none.
    assert_eq!(anchored.anchor.wrap, WrapMode::None);
    assert!(anchored.anchor.behind_doc);
    // The negative margin is clamped to zero AND reported.
    assert_eq!(anchored.anchor.wrap_distances.top_emu, 0);
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.style.graphic-margin-dropped")
    );
}

#[test]
fn floating_anchor_negative_offset_is_clamped_with_a_finding() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p><draw:frame text:anchor-type="page" svg:x="-2cm" svg:y="3cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.draw.anchor-offset-clamped")
    );
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    // The negative x is clamped to 0; the valid y survives.
    assert_eq!(
        anchored.anchor.horizontal.position,
        HorizontalPosition::Offset(0)
    );
    assert_eq!(
        anchored.anchor.vertical.position,
        VerticalPosition::Offset(3 * 360_000)
    );
}

#[test]
fn preserving_export_folds_multiline_alt_text() {
    // A valid alt text with a newline must not abort the preserving export; it is
    // folded to a single-line svg:title.
    let content = "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.4\"><office:body><office:text><text:p><draw:frame><draw:image xlink:href=\"Pictures/pic.dat\"/><svg:title>line one\nline two</svg:title></draw:frame></text:p></office:text></office:body></office:document-content>".as_bytes().to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    let out_content = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(out_content.contains("<svg:title>line one line two</svg:title>"));
}

#[test]
fn only_referenced_retained_parts_are_repackaged() {
    let bytes = image_package(
        image_content("Pictures/pic.dat"),
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let mut retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    // An unreferenced retained part must not be repackaged (no orphan output).
    retained.parts.insert(
        "Pictures/orphan.png".to_owned(),
        RetainedPart {
            media_type: "image/png".to_owned(),
            bytes: vec![1, 2, 3],
        },
    );
    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    assert!(out.read_part("Pictures/pic.dat").is_ok());
    assert!(out.read_part("Pictures/orphan.png").is_err());
}

#[test]
fn unknown_parts_are_retained_repackaged_and_fixed_point() {
    // A non-semantic part (a thumbnail) survives a preserving export even with no
    // media, and round-trips byte-identically.
    let bytes = image_package(
        CONTENT.to_vec(),
        r#"<m:file-entry m:full-path="Thumbnails/thumbnail.png" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Thumbnails/thumbnail.png",
            bytes: b"THUMB".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    assert!(retained.parts.is_empty());
    assert!(retained.unknown.contains_key("Thumbnails/thumbnail.png"));

    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    assert_eq!(out.read_part("Thumbnails/thumbnail.png").unwrap(), b"THUMB");
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexported =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexported.bytes, preserved.bytes);
}

#[test]
fn hand_built_retained_parts_are_sanitized_on_export() {
    // A host-supplied retained set with a duplicate key and an unsafe traversal
    // path must not yield a duplicate or non-admissible package.
    let bytes = image_package(
        image_content("Pictures/pic.dat"),
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let mut retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    retained.unknown.insert(
        "Pictures/pic.dat".to_owned(),
        RetainedPart {
            media_type: "image/png".to_owned(),
            bytes: vec![9],
        },
    );
    retained.unknown.insert(
        "../evil.png".to_owned(),
        RetainedPart {
            media_type: "image/png".to_owned(),
            bytes: vec![9],
        },
    );
    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    assert_eq!(out.read_part("Pictures/pic.dat").unwrap(), b"\x89PNG\r\n");
    assert!(out.read_part("../evil.png").is_err());
}

#[test]
fn reserved_name_image_is_not_retained_or_repackaged() {
    // A crafted href pointing at a regenerated semantic part must not be retained
    // (repackaging it would emit a duplicate ZIP entry).
    let bytes = image_package(image_content("content.xml"), "", &[]);
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    assert!(retained.is_empty());
    // Preserving export still produces an admissible package (no duplicate part).
    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
}

#[test]
fn missing_image_part_is_reported() {
    // The draw:image references a part not present in the package/manifest.
    let bytes = image_package(image_content("Pictures/missing.png"), "", &[]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    imported.document.validate().unwrap();
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.draw.image-missing-part")
    );
}

fn fixture_odt() -> Vec<u8> {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p>Body</text:p><text:p><draw:frame svg:width="2cm" svg:height="2cm"><draw:image xlink:href="Pictures/img.png"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let styles = br#"<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:styles><style:default-style style:family="paragraph"><style:text-properties fo:font-weight="bold"/></style:default-style></office:styles><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="21cm" fo:page-height="29.7cm" fo:margin-top="2cm" fo:margin-bottom="2cm" fo:margin-left="2cm" fo:margin-right="2cm" style:print-orientation="portrait"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Head</text:p></style:header><style:footer><text:p>Foot</text:p></style:footer></style:master-page></office:master-styles></office:document-styles>"#.to_vec();
    let meta = br#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="1.4"><office:meta><dc:title>Fixture</dc:title></office:meta></office:document-meta>"#.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/><m:file-entry m:full-path="styles.xml" m:media-type="text/xml"/><m:file-entry m:full-path="meta.xml" m:media-type="text/xml"/><m:file-entry m:full-path="Pictures/img.png" m:media-type="image/png"/><m:file-entry m:full-path="Thumbnails/thumbnail.png" m:media-type="image/png"/><m:file-entry m:full-path="settings.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let text = |name: &'static str, bytes: Vec<u8>| Entry {
        name,
        bytes,
        compression: CompressionMethod::Deflated,
        local_extra: false,
    };
    package(&[
        Entry {
            name: MIMETYPE_PART,
            bytes: ODT_MIME.as_bytes().to_vec(),
            compression: CompressionMethod::Stored,
            local_extra: false,
        },
        text(MANIFEST_PART, manifest),
        text(CONTENT_PART, content),
        text(STYLES_PART, styles),
        text("meta.xml", meta),
        text("Pictures/img.png", b"\x89PNG\r\nIMG".to_vec()),
        text("Thumbnails/thumbnail.png", b"THUMB".to_vec()),
        text("settings.xml", b"<x/>".to_vec()),
    ])
}

#[test]
fn realistic_multipart_odt_round_trips_and_preserves() {
    // One fixture exercises the whole stack: defaults + page-layout +
    // master-page header/footer + metadata + embedded image + unknown parts.
    let bytes = fixture_odt();
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();

    let definitions = imported.document.definitions();
    assert_eq!(definitions.sections[0].page_size.width_twips, 11_906);
    assert_eq!(definitions.sections[0].headers.len(), 1);
    assert_eq!(definitions.sections[0].footers.len(), 1);
    assert_eq!(
        definitions
            .document_defaults
            .as_ref()
            .and_then(|d| d.run.as_ref())
            .and_then(|r| r.bold),
        Some(true)
    );
    assert_eq!(definitions.media.len(), 1);
    assert_eq!(
        imported
            .document
            .properties()
            .and_then(|p| p.core.title.as_deref()),
        Some("Fixture")
    );

    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    assert!(retained.parts.contains_key("Pictures/img.png"));
    assert!(retained.unknown.contains_key("Thumbnails/thumbnail.png"));
    assert!(retained.unknown.contains_key("settings.xml"));

    // Preserving export keeps geometry, header/footer, defaults, the image, and
    // the unknown parts; the reopened document matches and re-exports identically.
    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    assert_eq!(
        out.read_part("Pictures/img.png").unwrap(),
        b"\x89PNG\r\nIMG"
    );
    assert_eq!(out.read_part("Thumbnails/thumbnail.png").unwrap(), b"THUMB");
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    assert_eq!(reopened.document.definitions().media.len(), 1);
    assert_eq!(reopened.document.definitions().sections[0].headers.len(), 1);

    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexported =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexported.bytes, preserved.bytes);
}

#[test]
fn real_libreoffice_odt_admits_imports_and_preserves() {
    // Authentic LibreOffice output (sample.docx -> .odt): rich content.xml/
    // styles.xml plus manifest.rdf, settings.xml, Configurations2/, Thumbnails,
    // and a packaged image. The bounded adapter must admit it, import a subset
    // (findings, not failure), and preserve/round-trip real producer bytes.
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/corpus/libreoffice-sample.odt"
    ));
    let mut package = OdtPackage::open(bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();
    assert!(!imported.document.body().is_empty());

    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    // The opaque non-semantic parts are carried; Configurations2/ (a directory
    // entry) and META-INF are never retained.
    assert!(retained.unknown.contains_key("settings.xml"));
    assert!(
        !retained
            .unknown
            .keys()
            .any(|name| name.starts_with("META-INF/"))
    );

    let preserved =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&preserved.bytes, OdfPackageLimits::default()).unwrap();
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    reopened.document.validate().unwrap();
    // The opaque settings part survived the semantic edit into the output.
    assert!(out.read_part("settings.xml").is_ok());
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexported =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexported.bytes, preserved.bytes);
}

/// Admit a real-producer ODT, import a subset without failing, and prove the
/// preserve-when-safe export reaches a byte-exact fixed point.
///
/// The very first export may pick fresh canonical node ids and normalise
/// producer-specific style names / citations away, so it need not equal a later
/// export. What must hold — and what this asserts — is that once the document is
/// in our canonical form, the round trip is stable: exporting the reopened
/// document and the twice-reopened document produce identical bytes. That is the
/// interop guarantee against silently lossy or non-deterministic round trips.
fn assert_real_odt_round_trips(bytes: &[u8]) {
    let mut package = OdtPackage::open(bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    imported.document.validate().unwrap();
    assert!(!imported.document.body().is_empty());
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    // Producer-private parts are carried opaquely; META-INF is never retained.
    assert!(
        !retained
            .unknown
            .keys()
            .any(|name| name.starts_with("META-INF/"))
    );

    let export_once = |bytes: &[u8]| {
        let mut package = OdtPackage::open(bytes, OdfPackageLimits::default()).unwrap();
        let imported = package.import_document(OdfImportLimits::default()).unwrap();
        imported.document.validate().unwrap();
        let retained = package
            .retained_media_parts(&imported.document, OdfImportLimits::default())
            .unwrap();
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap()
            .bytes
    };

    let first =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap()
            .bytes;
    let second = export_once(&first);
    let third = export_once(&second);
    assert_eq!(second, third, "canonical ODT export must be a fixed point");
}

macro_rules! real_odt_interop_test {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let bytes = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/corpus/",
                $file
            ));
            assert_real_odt_round_trips(bytes);
        }
    };
}

// Authentic LibreOffice conversions of the real-producer DOCX corpus, each
// exercising a different construct set (rich text, table merges, footnotes,
// hyperlinks) against the bounded ODT adapter.
real_odt_interop_test!(
    real_libreoffice_rich_odt_round_trips,
    "real-producer-rich.odt"
);
real_odt_interop_test!(
    real_libreoffice_table_merges_odt_round_trips,
    "real-producer-table-merges.odt"
);
real_odt_interop_test!(
    real_libreoffice_footnotes_odt_round_trips,
    "real-producer-footnotes.odt"
);
real_odt_interop_test!(
    real_libreoffice_hyperlinks_odt_round_trips,
    "real-producer-hyperlinks.odt"
);
real_odt_interop_test!(
    real_libreoffice_header_footer_odt_round_trips,
    "real-producer-header-footer.odt"
);
real_odt_interop_test!(
    real_libreoffice_table_list_odt_round_trips,
    "real-producer-table-list.odt"
);
real_odt_interop_test!(
    real_libreoffice_roundtrip_odt_round_trips,
    "real-producer-libreoffice.odt"
);
real_odt_interop_test!(
    real_libreoffice_rich_metadata_odt_round_trips,
    "synthetic-rich-metadata.odt"
);

/// The real-producer ODT corpus, for the timing harness below.
const ODT_CORPUS: &[(&str, &[u8])] = &[
    (
        "libreoffice-sample",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/libreoffice-sample.odt"
        )),
    ),
    (
        "rich",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-rich.odt"
        )),
    ),
    (
        "table-merges",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-table-merges.odt"
        )),
    ),
    (
        "table-list",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-table-list.odt"
        )),
    ),
    (
        "footnotes",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-footnotes.odt"
        )),
    ),
    (
        "hyperlinks",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-hyperlinks.odt"
        )),
    ),
    (
        "header-footer",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/corpus/real-producer-header-footer.odt"
        )),
    ),
];

/// Lightweight, dependency-free timing harness over the real-producer ODT
/// corpus. `#[ignore]` by default (timings are informational, not a CI gate);
/// run with `cargo test -p casual-doc-odf odt_corpus_import_export_timing --
/// --ignored --nocapture` to print per-fixture import + preserving-export
/// medians. A deliberately generous per-run ceiling still trips on a
/// catastrophic (e.g. accidentally quadratic) regression without being flaky.
#[test]
#[ignore = "timing harness; run explicitly with --ignored --nocapture"]
#[allow(clippy::print_stdout)] // intentional: this harness reports timings to stdout
fn odt_corpus_import_export_timing() {
    use std::time::Instant;

    const ITERS: u32 = 25;
    const CEILING: std::time::Duration = std::time::Duration::from_secs(2);

    for (name, bytes) in ODT_CORPUS {
        let mut import_samples = Vec::with_capacity(ITERS as usize);
        let mut export_samples = Vec::with_capacity(ITERS as usize);
        for _ in 0..ITERS {
            let t0 = Instant::now();
            let mut package = OdtPackage::open(bytes, OdfPackageLimits::default()).unwrap();
            let imported = package.import_document(OdfImportLimits::default()).unwrap();
            let retained = package
                .retained_media_parts(&imported.document, OdfImportLimits::default())
                .unwrap();
            let import_dt = t0.elapsed();

            let t1 = Instant::now();
            let _ = write_odt_with_retained_parts(
                &imported.document,
                &retained,
                OdfExportLimits::default(),
            )
            .unwrap();
            let export_dt = t1.elapsed();

            assert!(
                import_dt < CEILING && export_dt < CEILING,
                "{name}: import {import_dt:?} / export {export_dt:?} exceeded {CEILING:?} ceiling"
            );
            import_samples.push(import_dt);
            export_samples.push(export_dt);
        }
        import_samples.sort_unstable();
        export_samples.sort_unstable();
        let median = |v: &[std::time::Duration]| v[v.len() / 2];
        println!(
            "{name:>18}: import median {:>8.1?}  export median {:>8.1?}  ({} bytes)",
            median(&import_samples),
            median(&export_samples),
            bytes.len(),
        );
    }
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

/// A floating `draw:rect` with a solid fill and outline imports to a single-child
/// `WordprocessingGroup` (a `GroupShape::Rectangle`) and re-exports through the
/// preserving path to a byte-exact fixed point.
#[test]
fn standalone_rectangle_shape_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc" draw:stroke="solid" svg:stroke-width="0.05cm" svg:stroke-color="#112233"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    // The rectangle becomes a group-of-one holding a filled/outlined GroupShape.
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    assert_eq!(group.extent.width_emu, 5 * 360_000);
    assert_eq!(group.extent.height_emu, 3 * 360_000);
    assert_eq!(
        group.anchor.as_ref().unwrap().horizontal.relative_from,
        HorizontalAnchor::Column
    );
    assert_eq!(
        group.anchor.as_ref().unwrap().horizontal.position,
        HorizontalPosition::Offset(2 * 360_000)
    );
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        panic!("one shape child")
    };
    assert_eq!(shape.geometry, ShapeGeometry::Rectangle);
    let Some(Fill::Solid(fill)) = shape.fill else {
        panic!("solid fill")
    };
    assert_eq!((fill.r, fill.g, fill.b), (0x33, 0x66, 0xcc));
    let stroke = shape.stroke.as_ref().expect("stroke");
    assert_eq!(
        (stroke.color.r, stroke.color.g, stroke.color.b),
        (0x11, 0x22, 0x33)
    );
    assert_eq!(stroke.width_emu, 18_000); // 0.05cm

    // The preserving writer re-emits the positioned draw:rect + its graphic style.
    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<draw:rect text:anchor-type="paragraph" draw:style-name="#));
    assert!(content_out.contains(r##"draw:fill="solid" draw:fill-color="#3366cc""##));
    assert!(content_out.contains(r##"svg:stroke-color="#112233""##));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);

    // The plain semantic path has no draw namespaces, so the shape degrades (no
    // draw:rect emitted) rather than producing namespace-invalid XML.
    let semantic = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut plain = OdtPackage::open(&semantic.bytes, OdfPackageLimits::default()).unwrap();
    let plain_content = String::from_utf8(plain.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!plain_content.contains("draw:rect"));

    // A translucent shape fill (a < 255, only reachable from a non-ODF-origin
    // model) loses its alpha on export — `draw:fill-color` is RGB only — but the
    // drop is reported, not silent.
    let mut document = imported.document;
    let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &mut paragraph.inlines[0] else {
        panic!("group")
    };
    let GroupChild::Shape(shape) = &mut group.children[0] else {
        panic!("shape")
    };
    shape.fill = Some(Fill::Solid(casual_doc_model::v1::Rgba {
        r: 0x33,
        g: 0x66,
        b: 0xcc,
        a: 0x80,
    }));
    let export =
        write_odt_with_retained_parts(&document, &retained, OdfExportLimits::default()).unwrap();
    assert!(
        export
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odt.export.shape_fill_opacity")
    );
}

/// A floating `draw:rect` whose graphic style references a linear two-color
/// `<draw:gradient>` imports to `Fill::Gradient` (two stops + `Linear` angle) and
/// re-exports through the preserving path — the gradient definition landing in
/// `office:styles` (styles.xml) — as a byte-exact fixed point.
#[test]
fn standalone_shape_linear_gradient_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="gradient" draw:fill-gradient-name="G1"/></style:style><draw:gradient draw:name="G1" draw:style="linear" draw:start-color="#ff0000" draw:end-color="#0000ff" draw:start-intensity="100%" draw:end-intensity="100%" draw:angle="300" draw:border="0%"/></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        panic!("one shape child")
    };
    // Two endpoint stops (0..100000) and a linear angle: ODF `draw:angle="300"`
    // (1/10 degree) maps to the model's 60000ths (300 * 6000 = 1_800_000).
    let Some(Fill::Gradient { stops, kind }) = &shape.fill else {
        panic!("gradient fill")
    };
    assert_eq!(
        stops,
        &vec![
            GradientStop {
                position: 0,
                color: casual_doc_model::v1::Rgba {
                    r: 0xff,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            },
            GradientStop {
                position: 100_000,
                color: casual_doc_model::v1::Rgba {
                    r: 0,
                    g: 0,
                    b: 0xff,
                    a: 255,
                },
            },
        ]
    );
    assert_eq!(*kind, GradientKind::Linear { angle: 1_800_000 });

    // The preserving writer re-emits the gradient fill on the graphic style and the
    // `<draw:gradient>` definition into styles.xml (`office:styles`).
    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"draw:fill="gradient" draw:fill-gradient-name="grad"#));
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(r#"<draw:gradient draw:name="grad"#));
    assert!(styles_out.contains(r#"draw:style="linear""#));
    assert!(styles_out.contains(r##"draw:start-color="#ff0000""##));
    assert!(styles_out.contains(r##"draw:end-color="#0000ff""##));
    assert!(styles_out.contains(r#"draw:angle="300""#));
    assert!(
        styles_out.contains(r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0""#)
    );

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);

    // The plain semantic path has no draw namespaces, so the shape degrades and no
    // orphan `<draw:gradient>` definition is emitted.
    let semantic = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut plain = OdtPackage::open(&semantic.bytes, OdfPackageLimits::default()).unwrap();
    let plain_content = String::from_utf8(plain.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!plain_content.contains("draw:rect"));
    assert!(!plain_content.contains("draw:gradient"));
    if let Ok(part) = plain.read_part(STYLES_PART) {
        assert!(!String::from_utf8(part).unwrap().contains("draw:gradient"));
    }
}

/// A floating `draw:rect` whose graphic style references a `<draw:stroke-dash>` via
/// `draw:stroke="dash"` imports to `ShapeStroke::dash = Some(DashStyle::Dash)` (the
/// source def matches the `Dash` canonical geometry) and re-exports through the
/// preserving path — the synthesized dash definition landing in `office:styles`
/// (styles.xml) — as a byte-exact fixed point. Also asserts the model→canonical
/// def→re-import→model closure for every non-solid preset (so `export2 == export3`
/// holds for each), and that `Solid`/`None` carry no dash definition.
#[test]
fn standalone_shape_dashed_stroke_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:stroke="dash" draw:stroke-dash="D1" svg:stroke-width="0.05cm" svg:stroke-color="#112233"/></style:style><draw:stroke-dash draw:name="D1" draw:style="rect" draw:dots1="1" draw:dots1-length="0.4cm" draw:distance="0.3cm"/></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    // The source dash geometry matches the `Dash` canonical, so it imports to that
    // preset, carrying the outline color/width alongside.
    let shape = dashed_shape(&imported.document);
    let stroke = shape.stroke.as_ref().expect("stroke");
    assert_eq!(stroke.dash, Some(DashStyle::Dash));
    assert_eq!(
        (stroke.color.r, stroke.color.g, stroke.color.b),
        (0x11, 0x22, 0x33)
    );
    assert_eq!(stroke.width_emu, 18_000); // 0.05cm

    // The preserving writer re-emits `draw:stroke="dash"` referencing a synthesized
    // `<draw:stroke-dash>` definition that lands in styles.xml (`office:styles`).
    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"draw:stroke="dash" draw:stroke-dash="dash"#));
    assert!(content_out.contains(r##"svg:stroke-color="#112233""##));
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(r#"<draw:stroke-dash draw:name="dash"#));
    assert!(styles_out.contains(r#"draw:style="rect""#));
    assert!(styles_out.contains(r#"draw:dots1="1""#));
    assert!(styles_out.contains(r#"draw:dots1-length="0.4000cm""#));
    assert!(styles_out.contains(r#"draw:distance="0.3000cm""#));
    assert!(
        styles_out.contains(r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0""#)
    );

    // Semantic + byte fixed point for the imported `Dash`.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);

    // The model→canonical def→re-import→model closure for EVERY non-solid preset: set
    // the shape's dash to each variant, export, re-import, and confirm it recovers the
    // SAME preset and that a second export is byte-identical (export2 == export3).
    for variant in [
        DashStyle::Dot,
        DashStyle::Dash,
        DashStyle::LargeDash,
        DashStyle::DashDot,
        DashStyle::LargeDashDot,
        DashStyle::LargeDashDotDot,
        DashStyle::SystemDash,
        DashStyle::SystemDot,
        DashStyle::SystemDashDot,
        DashStyle::SystemDashDotDot,
    ] {
        let mut document = imported.document.clone();
        set_shape_dash(&mut document, Some(variant));
        let export2 =
            write_odt_with_retained_parts(&document, &retained, OdfExportLimits::default())
                .unwrap();
        let mut pkg = OdtPackage::open(&export2.bytes, OdfPackageLimits::default()).unwrap();
        let styles = String::from_utf8(pkg.read_part(STYLES_PART).unwrap()).unwrap();
        assert!(
            styles.contains("<draw:stroke-dash draw:name=\"dash"),
            "variant {variant:?} should emit a stroke-dash def"
        );
        let reimported = pkg.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(
            dashed_shape(&reimported.document)
                .stroke
                .as_ref()
                .unwrap()
                .dash,
            Some(variant),
            "variant {variant:?} must re-import to the same preset"
        );
        let export3 = write_odt_with_retained_parts(
            &reimported.document,
            &retained,
            OdfExportLimits::default(),
        )
        .unwrap();
        assert_eq!(
            export2.bytes, export3.bytes,
            "variant {variant:?} export must be a fixed point"
        );
    }

    // `Solid` and `None` both carry NO dash definition — a solid outline, byte-identical
    // to each other and free of any orphan `<draw:stroke-dash>`.
    let mut solid_dash = imported.document.clone();
    set_shape_dash(&mut solid_dash, Some(DashStyle::Solid));
    let mut no_dash = imported.document.clone();
    set_shape_dash(&mut no_dash, None);
    let solid_export =
        write_odt_with_retained_parts(&solid_dash, &retained, OdfExportLimits::default()).unwrap();
    let none_export =
        write_odt_with_retained_parts(&no_dash, &retained, OdfExportLimits::default()).unwrap();
    assert_eq!(solid_export.bytes, none_export.bytes);
    let mut solid_pkg = OdtPackage::open(&solid_export.bytes, OdfPackageLimits::default()).unwrap();
    let solid_content = String::from_utf8(solid_pkg.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(solid_content.contains(r#"draw:stroke="solid""#));
    assert!(!solid_content.contains("draw:stroke-dash"));
    if let Ok(part) = solid_pkg.read_part(STYLES_PART) {
        assert!(
            !String::from_utf8(part)
                .unwrap()
                .contains("draw:stroke-dash")
        );
    }

    // The plain semantic path has no draw namespaces, so the shape degrades and no
    // orphan `<draw:stroke-dash>` definition is emitted.
    let semantic = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut plain = OdtPackage::open(&semantic.bytes, OdfPackageLimits::default()).unwrap();
    let plain_content = String::from_utf8(plain.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!plain_content.contains("draw:rect"));
    assert!(!plain_content.contains("draw:stroke-dash"));
}

/// Two shapes with an identical stroke color+width but DIFFERENT dash patterns must
/// synthesize DISTINCT `gr<hash>` graphic-style names. `OdtGraphicStyle::name()`
/// builds its hash from only the present fields, so the dash field must contribute to
/// the key — otherwise the two styles collide on one name, emitting duplicate
/// `style:name` definitions (invalid ODF) that the importer rejects with
/// `MalformedContent`, breaking `export2 == export3`. This is the common real-world
/// case: a diagram with same-colored lines, some solid and some dashed.
#[test]
fn shapes_differing_only_in_dash_get_distinct_style_names() {
    // One dashed rect and one SOLID-stroke rect, same color (#112233) and width (0.05cm).
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gd" style:family="graphic"><style:graphic-properties draw:stroke="dash" draw:stroke-dash="D1" svg:stroke-width="0.05cm" svg:stroke-color="#112233"/></style:style><style:style style:name="gs" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-width="0.05cm" svg:stroke-color="#112233"/></style:style><draw:stroke-dash draw:name="D1" draw:style="rect" draw:dots1="1" draw:dots1-length="0.4cm" draw:distance="0.3cm"/></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gd" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/><draw:rect draw:style-name="gs" text:anchor-type="paragraph" svg:x="9cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();

    // The dashed rect and the solid rect must reference two DIFFERENT graphic styles.
    let names: Vec<&str> = content_out
        .match_indices("draw:style-name=\"")
        .map(|(i, m)| {
            let rest = &content_out[i + m.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .filter(|n| n.starts_with("gr"))
        .collect();
    assert_eq!(names.len(), 2, "two shape styles referenced");
    assert_ne!(
        names[0], names[1],
        "a dashed and a solid shape must not collide on one style name"
    );

    // The re-import must SUCCEED (the collision previously emitted duplicate
    // `style:name` defs that the importer rejected with `MalformedContent`).
    let reopened = out
        .import_document(OdfImportLimits::default())
        .expect("re-import must not fail on duplicate style names");
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// Returns the single `GroupShape` child of the first paragraph's first inline group.
fn dashed_shape(document: &casual_doc_model::v1::Document) -> &casual_doc_model::v1::GroupShape {
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        panic!("one shape child")
    };
    shape
}

/// Sets the dash preset on the single shape child of the first inline group.
fn set_shape_dash(document: &mut casual_doc_model::v1::Document, dash: Option<DashStyle>) {
    let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &mut paragraph.inlines[0] else {
        panic!("group")
    };
    let [GroupChild::Shape(shape)] = group.children.as_mut_slice() else {
        panic!("one shape child")
    };
    let stroke = shape.stroke.get_or_insert(ShapeStroke {
        color: casual_doc_model::v1::Rgba {
            r: 0x11,
            g: 0x22,
            b: 0x33,
            a: 255,
        },
        width_emu: 18_000,
        dash: None,
        head_end: None,
        tail_end: None,
    });
    stroke.dash = dash;
}

/// A floating `draw:rect` with a radial `<draw:gradient>` imports to a
/// `Fill::Gradient` with `GradientKind::Radial` (no angle) and re-exports as
/// `draw:style="radial"`, a byte-exact fixed point.
#[test]
fn standalone_shape_radial_gradient_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="gradient" draw:fill-gradient-name="G2"/></style:style><draw:gradient draw:name="G2" draw:style="radial" draw:start-color="#112233" draw:end-color="#aabbcc" draw:angle="0"/></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="page" svg:x="1cm" svg:y="1cm" svg:width="4cm" svg:height="4cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();

    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        panic!("one shape child")
    };
    let Some(Fill::Gradient { stops, kind }) = &shape.fill else {
        panic!("gradient fill")
    };
    assert_eq!(stops.len(), 2);
    assert_eq!(*kind, GradientKind::Radial);

    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let styles_out = String::from_utf8(out.read_part(STYLES_PART).unwrap()).unwrap();
    assert!(styles_out.contains(r#"draw:style="radial""#));
    assert!(styles_out.contains(r##"draw:start-color="#112233""##));

    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A shape's synthesized `gr<hash>` graphic-style name must depend only on the
/// style's own output-affecting fields, not on unrelated `OdtGraphicStyle` fields
/// left at their default. Concretely: a solid-fill rectangle emits the SAME
/// `draw:style-name` whether or not the document also contains a gradient shape
/// (which populates the `fill_gradient` field on its OWN style). This guards the
/// regression where `name()` hashed the whole `{self:?}` Debug string, so adding
/// the `fill_gradient` field churned every graphic style's name across releases.
#[test]
fn solid_fill_style_name_is_independent_of_sibling_gradient() {
    // Extracts the `style:name` of the `graphic` style whose properties carry the
    // solid `#3366cc` fill, from an exported content.xml/styles.xml pair.
    fn solid_style_name(part: &str) -> String {
        let marker = r##"draw:fill="solid" draw:fill-color="#3366cc""##;
        let at = part.find(marker).expect("solid graphic-properties present");
        let before = &part[..at];
        let key = "style:name=\"";
        let name_at = before.rfind(key).expect("style:name before properties") + key.len();
        let rest = &part[name_at..];
        let end = rest.find('"').expect("closing quote");
        rest[..end].to_string()
    }

    fn export_content(content: &[u8]) -> String {
        let manifest = format!(
            r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
        )
        .into_bytes();
        let bytes = package(&[
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
        ]);
        let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
            .unwrap()
            .import_document(OdfImportLimits::default())
            .unwrap();
        let retained = crate::OdfRetainedParts::default();
        let export = write_odt_with_retained_parts(
            &imported.document,
            &retained,
            OdfExportLimits::default(),
        )
        .unwrap();
        let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
        String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap()
    }

    // A lone solid-fill rectangle.
    let solid_only = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##;

    // The same solid-fill rectangle plus a sibling gradient rectangle. The gradient
    // populates `fill_gradient` on ITS style only; the solid style must be unaffected.
    let solid_plus_gradient = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc"/></style:style><style:style style:name="gr2" style:family="graphic"><style:graphic-properties draw:fill="gradient" draw:fill-gradient-name="G1"/></style:style><draw:gradient draw:name="G1" draw:style="linear" draw:start-color="#ff0000" draw:end-color="#0000ff" draw:angle="300"/></office:automatic-styles><office:body><office:text><text:p><draw:rect draw:style-name="gr1" text:anchor-type="paragraph" svg:x="2cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/><draw:rect draw:style-name="gr2" text:anchor-type="paragraph" svg:x="9cm" svg:y="1cm" svg:width="5cm" svg:height="3cm"/></text:p></office:text></office:body></office:document-content>"##;

    let name_alone = solid_style_name(&export_content(solid_only));
    let name_with_gradient = solid_style_name(&export_content(solid_plus_gradient));
    assert_eq!(
        name_alone, name_with_gradient,
        "a sibling gradient must not change the solid style's synthesized name"
    );
}

/// A floating `draw:ellipse` imports to an `Ellipse` `GroupShape` and re-exports
/// as `draw:ellipse` (not `draw:rect`), a byte-exact fixed point.
#[test]
fn standalone_ellipse_shape_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#22aa44"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:ellipse draw:style-name="gr1" text:anchor-type="page" svg:x="1cm" svg:y="1cm" svg:width="6cm" svg:height="4cm"/></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    let [GroupChild::Shape(shape)] = group.children.as_slice() else {
        panic!("one shape child")
    };
    assert_eq!(shape.geometry, ShapeGeometry::Ellipse);

    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<draw:ellipse text:anchor-type="page""#));
    assert!(!content_out.contains("draw:rect"));

    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A floating `draw:line` imports to a `Line` `GroupShape` whose bounding box +
/// flip pair encode the endpoints, and re-exports to the same endpoints — verified
/// for all four diagonal directions, each a byte-exact fixed point.
#[test]
fn standalone_line_shape_round_trips_all_directions() {
    // (x1, y1, x2, y2) in cm, and the expected (flip_h, flip_v).
    let cases = [
        ("2", "1", "6", "4", false, false), // top-left → bottom-right
        ("6", "1", "2", "4", true, false),  // top-right → bottom-left
        ("2", "4", "6", "1", false, true),  // bottom-left → top-right
        ("6", "4", "2", "1", true, true),   // bottom-right → top-left
    ];
    for (x1, y1, x2, y2, exp_flip_h, exp_flip_v) in cases {
        let content = format!(
            r##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-width="0.05cm" svg:stroke-color="#000000"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:line draw:style-name="gr1" text:anchor-type="paragraph" svg:x1="{x1}cm" svg:y1="{y1}cm" svg:x2="{x2}cm" svg:y2="{y2}cm"/></text:p></office:text></office:body></office:document-content>"##
        )
        .into_bytes();
        let manifest = format!(
            r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
        )
        .into_bytes();
        let bytes = package(&[
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
                bytes: content,
                compression: CompressionMethod::Deflated,
                local_extra: false,
            },
        ]);
        let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
            .unwrap()
            .import_document(OdfImportLimits::default())
            .unwrap();
        let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
            panic!("paragraph")
        };
        let InlineNode::Group(group) = &paragraph.inlines[0] else {
            panic!("group ({x1},{y1})-({x2},{y2})")
        };
        // Bounding box is the min corner + |delta|, direction-independent.
        assert_eq!(group.extent.width_emu, 4 * 360_000);
        assert_eq!(group.extent.height_emu, 3 * 360_000);
        let [GroupChild::Shape(shape)] = group.children.as_slice() else {
            panic!("one shape child")
        };
        assert_eq!(shape.geometry, ShapeGeometry::Line);
        assert_eq!(
            shape.flip_h, exp_flip_h,
            "flip_h for ({x1},{y1})-({x2},{y2})"
        );
        assert_eq!(
            shape.flip_v, exp_flip_v,
            "flip_v for ({x1},{y1})-({x2},{y2})"
        );

        // Export re-emits draw:line with the ORIGINAL endpoints (canonical cm form).
        let retained = crate::OdfRetainedParts::default();
        let export = write_odt_with_retained_parts(
            &imported.document,
            &retained,
            OdfExportLimits::default(),
        )
        .unwrap();
        let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
        let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
        assert!(content_out.contains(&format!(
            r#"svg:x1="{x1}.0000cm" svg:y1="{y1}.0000cm" svg:x2="{x2}.0000cm" svg:y2="{y2}.0000cm""#
        )));
        assert!(content_out.contains("<draw:line "));

        // Semantic + byte fixed point.
        let reopened = out.import_document(OdfImportLimits::default()).unwrap();
        assert_eq!(reopened.document, imported.document);
        let reexport = write_odt_with_retained_parts(
            &reopened.document,
            &retained,
            OdfExportLimits::default(),
        )
        .unwrap();
        assert_eq!(reexport.bytes, export.bytes);
    }
}

/// A `draw:g` with multiple shape children (rect + ellipse + line) imports to a
/// multi-child `WordprocessingGroup` and re-exports as a `draw:g` preserving each
/// child's absolute position and order, a byte-exact fixed point.
#[test]
fn multi_child_shape_group_round_trips() {
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc"/></style:style><style:style style:name="gr2" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-width="0.05cm" svg:stroke-color="#000000"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:g text:anchor-type="page" draw:z-index="3"><draw:rect draw:style-name="gr1" svg:x="2cm" svg:y="2cm" svg:width="4cm" svg:height="3cm"/><draw:ellipse draw:style-name="gr1" svg:x="7cm" svg:y="2cm" svg:width="3cm" svg:height="3cm"/><draw:line draw:style-name="gr2" svg:x1="2cm" svg:y1="6cm" svg:x2="10cm" svg:y2="6cm"/></draw:g></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    // Three children in document (paint) order; union bbox is (2,2)..(10,6).
    assert_eq!(group.children.len(), 3);
    assert_eq!(group.extent.width_emu, 8 * 360_000); // 10cm - 2cm
    assert_eq!(group.extent.height_emu, 4 * 360_000); // 6cm - 2cm
    let GroupChild::Shape(rect) = &group.children[0] else {
        panic!("rect")
    };
    assert_eq!(rect.geometry, ShapeGeometry::Rectangle);
    assert_eq!(rect.offset.x_emu, 0); // 2cm - 2cm
    assert_eq!(rect.offset.y_emu, 0);
    let GroupChild::Shape(ellipse) = &group.children[1] else {
        panic!("ellipse")
    };
    assert_eq!(ellipse.geometry, ShapeGeometry::Ellipse);
    assert_eq!(ellipse.offset.x_emu, 5 * 360_000); // 7cm - 2cm
    let GroupChild::Shape(line) = &group.children[2] else {
        panic!("line")
    };
    assert_eq!(line.geometry, ShapeGeometry::Line);

    // Export re-emits draw:g with the three children at their absolute positions.
    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<draw:g text:anchor-type="page" draw:z-index="3">"#));
    assert!(content_out.contains(r#"<draw:rect draw:style-name="#));
    assert!(content_out.contains(r#"<draw:ellipse draw:style-name="#));
    assert!(content_out.contains(r#"<draw:line draw:style-name="#));
    // Children carry no anchor-type/z-index (the draw:g owns those).
    assert!(!content_out.contains(r#"<draw:rect text:anchor-type"#));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A `draw:g` containing a leaf shape AND a NESTED `draw:g` (itself holding an
/// ellipse + line) imports to a `WordprocessingGroup` whose second child is a
/// `GroupChild::Group` (anchor `None`, positioned by a translation-only transform),
/// and re-exports the nested `<draw:g>` — WITHOUT `text:anchor-type` — reproducing
/// every descendant's absolute coordinate, a byte-exact fixed point.
#[test]
fn nested_shape_group_round_trips() {
    const CM: i64 = 360_000;
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc"/></style:style><style:style style:name="gr2" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-width="0.05cm" svg:stroke-color="#000000"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:g text:anchor-type="page" draw:z-index="3"><draw:rect draw:style-name="gr1" svg:x="2cm" svg:y="2cm" svg:width="2cm" svg:height="2cm"/><draw:g><draw:ellipse draw:style-name="gr1" svg:x="10cm" svg:y="10cm" svg:width="2cm" svg:height="2cm"/><draw:line draw:style-name="gr2" svg:x1="10cm" svg:y1="15cm" svg:x2="14cm" svg:y2="15cm"/></draw:g></draw:g></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    // Outer union bbox over ALL descendants is (2,2)..(14,15) → extent 12cm x 13cm.
    assert_eq!(group.children.len(), 2);
    assert_eq!(group.extent.width_emu, 12 * CM);
    assert_eq!(group.extent.height_emu, 13 * CM);
    let GroupChild::Shape(rect) = &group.children[0] else {
        panic!("rect")
    };
    assert_eq!(rect.geometry, ShapeGeometry::Rectangle);
    assert_eq!(rect.offset.x_emu, 0); // 2cm - outer min 2cm
    assert_eq!(rect.offset.y_emu, 0);
    // The second child is a NESTED group, positioned by a translation-only transform.
    let GroupChild::Group(nested) = &group.children[1] else {
        panic!("nested group")
    };
    assert!(nested.anchor.is_none());
    assert!(nested.relative_height.is_none());
    // Nested bbox is (10,10)..(14,15); its offset within the outer child space is
    // nmin - outer_min = (8cm, 8cm).
    assert_eq!(nested.transform.offset.x_emu, 8 * CM);
    assert_eq!(nested.transform.offset.y_emu, 8 * CM);
    assert_eq!(nested.extent.width_emu, 4 * CM);
    assert_eq!(nested.extent.height_emu, 5 * CM);
    assert_eq!(nested.transform.child_offset.x_emu, 0);
    assert_eq!(nested.transform.child_extent.width_emu, 4 * CM);
    assert_eq!(nested.children.len(), 2);
    let GroupChild::Shape(ellipse) = &nested.children[0] else {
        panic!("ellipse")
    };
    assert_eq!(ellipse.geometry, ShapeGeometry::Ellipse);
    assert_eq!(ellipse.offset.x_emu, 0); // 10cm - nested min 10cm
    assert_eq!(ellipse.offset.y_emu, 0);
    let GroupChild::Shape(line) = &nested.children[1] else {
        panic!("line")
    };
    assert_eq!(line.geometry, ShapeGeometry::Line);
    assert_eq!(line.offset.x_emu, 0); // 10cm - 10cm
    assert_eq!(line.offset.y_emu, 5 * CM); // 15cm - 10cm

    // Export re-emits the outer draw:g plus a bare nested draw:g (no anchor-type/z).
    let retained = crate::OdfRetainedParts::default();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<draw:g text:anchor-type="page" draw:z-index="3">"#));
    // Exactly two draw:g opens: the anchored outer one and one bare nested one.
    assert_eq!(content_out.matches("<draw:g").count(), 2);
    assert_eq!(content_out.matches("</draw:g>").count(), 2);
    assert!(content_out.contains("<draw:g>")); // the nested group carries no attributes
    assert!(content_out.contains(r#"<draw:ellipse draw:style-name="#));
    assert!(content_out.contains(r#"<draw:line draw:style-name="#));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A `draw:g` containing a `draw:rect` AND a `draw:frame > draw:image` (an embedded
/// picture) imports to a `WordprocessingGroup` whose children are a `GroupChild::Shape`
/// and a `GroupChild::Picture` (media resolving in `definitions().media`, the picture's
/// bbox extending the group box), and re-exports through the PRESERVING path as a
/// `draw:g` carrying both the box and the positioned frame — repackaging the image
/// bytes — a byte-exact fixed point. The PLAIN path (no retained media) degrades the
/// whole group and writes no orphan media.
#[test]
fn group_picture_child_round_trips() {
    const CM: i64 = 360_000;
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#3366cc"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:g text:anchor-type="page" draw:z-index="3"><draw:rect draw:style-name="gr1" svg:x="2cm" svg:y="2cm" svg:width="4cm" svg:height="3cm"/><draw:frame svg:x="7cm" svg:y="2cm" svg:width="2cm" svg:height="2cm"><draw:image xlink:href="Pictures/logo.png"/></draw:frame></draw:g></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let image_bytes = b"\x89PNG\r\n\x1a\nLOGO".to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/logo.png" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/logo.png",
            bytes: image_bytes.clone(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();

    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::Group(group) = &paragraph.inlines[0] else {
        panic!("group")
    };
    // Two children: the rect shape and the embedded picture. The union bbox spans the
    // rect (2,2)..(6,5) AND the picture (7,2)..(9,4) → (2,2)..(9,5), extent 7cm x 3cm.
    assert_eq!(group.children.len(), 2);
    assert_eq!(group.extent.width_emu, 7 * CM);
    assert_eq!(group.extent.height_emu, 3 * CM);
    let GroupChild::Shape(rect) = &group.children[0] else {
        panic!("rect")
    };
    assert_eq!(rect.geometry, ShapeGeometry::Rectangle);
    assert_eq!(rect.offset.x_emu, 0);
    let GroupChild::Picture(picture) = &group.children[1] else {
        panic!("picture")
    };
    // The picture keeps its OWN extent (2cm) and sits at 7cm-2cm = 5cm within the box.
    assert_eq!(picture.offset.x_emu, 5 * CM);
    assert_eq!(picture.offset.y_emu, 0);
    assert_eq!(picture.extent.width_emu, 2 * CM);
    assert_eq!(picture.extent.height_emu, 2 * CM);
    // Its media resolves to the retained package part.
    let media = imported
        .document
        .definitions()
        .media
        .get(&picture.media)
        .expect("group picture media reference");
    assert_eq!(media.part_name, "Pictures/logo.png");

    // Preserving export re-emits the draw:g with the box AND the positioned frame, and
    // repackages the image bytes under the same part name.
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"<draw:g text:anchor-type="page" draw:z-index="3">"#));
    assert!(content_out.contains(r#"<draw:rect draw:style-name="#));
    assert!(content_out.contains(
        r#"<draw:frame svg:x="7.0000cm" svg:y="2.0000cm" svg:width="2.0000cm" svg:height="2.0000cm"><draw:image xlink:href="Pictures/logo.png"/></draw:frame>"#
    ));
    // The image bytes are repackaged verbatim (no orphan; the part round-trips).
    assert_eq!(out.read_part("Pictures/logo.png").unwrap(), image_bytes);

    // Semantic + byte fixed point: re-import equals the model, re-export identical.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);

    // The PLAIN path has no retained media (and no drawing namespaces), so the whole
    // group degrades — no `draw:g`, no orphaned `draw:image` — rather than emitting a
    // frame whose part was never packaged.
    let plain = write_odt(&imported.document, OdfExportLimits::default()).unwrap();
    let mut plain_pkg = OdtPackage::open(&plain.bytes, OdfPackageLimits::default()).unwrap();
    let plain_content = String::from_utf8(plain_pkg.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(!plain_content.contains("<draw:g"));
    assert!(!plain_content.contains("draw:image"));
    assert!(plain_pkg.read_part("Pictures/logo.png").is_err());
}

/// A `draw:g` whose children are each within the EMU domain but whose union
/// bounding box exceeds it must drop the group WITH a finding — not abort the whole
/// import — so unrelated content (a normal paragraph) survives.
#[test]
fn oversized_group_bbox_drops_group_not_document() {
    // Two rects at 0 and 7.5e7 cm, each width 7.5e7 cm (< MAX_EMU individually); the
    // union spans ~1.5e8 cm > MAX_EMU.
    let content = br##"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" office:version="1.4"><office:body><office:text><text:p>Normal text</text:p><text:p><draw:g text:anchor-type="page"><draw:rect svg:x="0cm" svg:y="0cm" svg:width="75000000cm" svg:height="1cm"/><draw:rect svg:x="75000000cm" svg:y="0cm" svg:width="75000000cm" svg:height="1cm"/></draw:g></text:p></office:text></office:body></office:document-content>"##.to_vec();
    let manifest = format!(
        r#"<m:manifest xmlns:m="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" m:version="1.4"><m:file-entry m:full-path="/" m:media-type="{ODT_MIME}" m:version="1.4"/><m:file-entry m:full-path="content.xml" m:media-type="text/xml"/></m:manifest>"#
    )
    .into_bytes();
    let bytes = package(&[
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
            bytes: content,
            compression: CompressionMethod::Deflated,
            local_extra: false,
        },
    ]);
    // Import must SUCCEED (not abort), dropping only the group.
    let imported = OdtPackage::open(&bytes, OdfPackageLimits::default())
        .unwrap()
        .import_document(OdfImportLimits::default())
        .unwrap();
    assert!(
        imported
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odf.draw.group-oversized")
    );
    // The normal paragraph survives; the group produced no inline in its paragraph.
    let BlockNode::Paragraph(first) = &imported.document.body()[0] else {
        panic!("first paragraph")
    };
    let InlineNode::Run(run) = &first.inlines[0] else {
        panic!("run")
    };
    assert_eq!(run.text, "Normal text");
    let BlockNode::Paragraph(second) = &imported.document.body()[1] else {
        panic!("second paragraph")
    };
    assert!(second.inlines.is_empty());
}

/// A floating image whose graphic style uses alignment positioning
/// (`style:horizontal-pos="center"`, `style:vertical-pos="top"`) imports to
/// `HorizontalPosition::Align`/`VerticalPosition::Align` and re-exports the same
/// alignment, a byte-exact fixed point.
#[test]
fn floating_anchor_alignment_positioning_round_trips() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" office:version="1.4"><office:automatic-styles><style:style style:name="fr1" style:family="graphic"><style:graphic-properties style:horizontal-pos="center" style:vertical-pos="top"/></style:style></office:automatic-styles><office:body><office:text><text:p><draw:frame draw:style-name="fr1" text:anchor-type="page" svg:x="0cm" svg:y="0cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let BlockNode::Paragraph(paragraph) = &imported.document.body()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    assert_eq!(
        anchored.anchor.horizontal.position,
        HorizontalPosition::Align(casual_doc_model::v1::HorizontalAlign::Center)
    );
    assert_eq!(
        anchored.anchor.vertical.position,
        VerticalPosition::Align(casual_doc_model::v1::VerticalAlign::Top)
    );

    // Export re-emits the alignment on the graphic style, with svg:x/y = 0.
    let retained = package
        .retained_media_parts(&imported.document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&imported.document, &retained, OdfExportLimits::default())
            .unwrap();
    let mut out = OdtPackage::open(&export.bytes, OdfPackageLimits::default()).unwrap();
    let content_out = String::from_utf8(out.read_part(CONTENT_PART).unwrap()).unwrap();
    assert!(content_out.contains(r#"style:horizontal-pos="center" style:vertical-pos="top""#));

    // Semantic + byte fixed point.
    let reopened = out.import_document(OdfImportLimits::default()).unwrap();
    assert_eq!(reopened.document, imported.document);
    let retained2 = out
        .retained_media_parts(&reopened.document, OdfImportLimits::default())
        .unwrap();
    let reexport =
        write_odt_with_retained_parts(&reopened.document, &retained2, OdfExportLimits::default())
            .unwrap();
    assert_eq!(reexport.bytes, export.bytes);
}

/// A vertical inside/outside alignment (only reachable from a cross-format model,
/// since ODF has no `style:vertical-pos` inside/outside) collapses to top/bottom on
/// export and MUST be reported, not lost silently.
#[test]
fn floating_anchor_vertical_inside_align_is_reported() {
    let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p><draw:frame text:anchor-type="page" svg:x="1cm" svg:y="1cm" svg:width="4cm" svg:height="2cm"><draw:image xlink:href="Pictures/pic.dat"/></draw:frame></text:p></office:text></office:body></office:document-content>"#.to_vec();
    let bytes = image_package(
        content,
        r#"<m:file-entry m:full-path="Pictures/pic.dat" m:media-type="image/png"/>"#,
        &[Entry {
            name: "Pictures/pic.dat",
            bytes: b"\x89PNG\r\n".to_vec(),
            compression: CompressionMethod::Deflated,
            local_extra: false,
        }],
    );
    let mut package = OdtPackage::open(&bytes, OdfPackageLimits::default()).unwrap();
    let imported = package.import_document(OdfImportLimits::default()).unwrap();
    let mut document = imported.document;
    // Force a vertical Inside alignment (a DOCX-origin state ODF cannot express).
    let BlockNode::Paragraph(paragraph) = &mut document.body_mut()[0] else {
        panic!("paragraph")
    };
    let InlineNode::AnchoredDrawing(anchored) = &mut paragraph.inlines[0] else {
        panic!("anchored drawing")
    };
    anchored.anchor.vertical.position =
        VerticalPosition::Align(casual_doc_model::v1::VerticalAlign::Inside);
    let retained = package
        .retained_media_parts(&document, OdfImportLimits::default())
        .unwrap();
    let export =
        write_odt_with_retained_parts(&document, &retained, OdfExportLimits::default()).unwrap();
    assert!(
        export
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "odt.export.anchor_vertical_align")
    );
}
