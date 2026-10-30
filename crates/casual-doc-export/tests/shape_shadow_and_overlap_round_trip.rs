//! A shape's outer shadow and a float's "Allow overlap" round-trip through
//! DOCX (`docs/153` `object.shape-shadow`, `object.advanced-layout-options`).
//!
//! Bytes in → model → bytes out → model, from Word-shaped markup: Word's
//! "Offset: Bottom Right" shadow preset on a floating text box whose anchor
//! says `allowOverlap="0"`, and a text-free shape carrying a preset-colour
//! shadow beside a glow. Before the shadow was modelled the glow's and the
//! shadow's colours replaced the shape's FILL (the colour capture had no idea
//! it was inside an effect), and `allowOverlap` was written as `1` whatever
//! the source said.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::export_document;
use casual_doc_import::{ImportConfig, ImportMode, import_main_document_xml, import_package};
use casual_doc_model::v1::{
    BlockNode, Document, Fill, GroupChild, InlineNode, Rgba, ShadowAlignment, ShapeShadow,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const ANCHOR_OPEN: &str = r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{id}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="{overlap}"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{id}" name="Shape {id}"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp>"#;
const ANCHOR_CLOSE: &str = "</wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>";

/// Word's "Offset: Bottom Right" outer shadow, as Word 365 writes it.
const WORD_SHADOW: &str = r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:prstClr val="black"><a:alpha val="40000"/></a:prstClr></a:outerShdw></a:effectLst>"#;

fn anchor(id: u32, overlap: &str, body: &str) -> String {
    let open = ANCHOR_OPEN
        .replace("{id}", &id.to_string())
        .replace("{overlap}", overlap);
    format!("{open}{body}{ANCHOR_CLOSE}")
}

fn source() -> Vec<u8> {
    let text_box = anchor(
        1,
        "0",
        &format!(
            r#"<wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFF00"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>{WORD_SHADOW}</wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/>"#
        ),
    );
    let shape = anchor(
        2,
        "1",
        r#"<wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="3355C4"/></a:solidFill><a:ln><a:noFill/></a:ln><a:effectLst><a:glow rad="63500"><a:srgbClr val="FF0000"/></a:glow><a:outerShdw blurRad="101600" dist="76200" dir="5400000"><a:srgbClr val="00FF00"/></a:outerShdw></a:effectLst></wps:spPr><wps:bodyPr/>"#,
    );
    format!(
        r#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body><w:p>{text_box}{shape}</w:p></w:body></w:document>"#
    )
    .into_bytes()
}

/// `(fill, shadow, allow_overlap)` for each float in the first paragraph.
fn census(document: &Document) -> Vec<(Option<Fill>, Option<ShapeShadow>, bool)> {
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("one paragraph");
    };
    let mut out = Vec::new();
    for inline in &paragraph.inlines {
        match inline {
            InlineNode::TextBox(text_box) => out.push((
                text_box.fill.clone(),
                text_box.shadow,
                text_box.anchor.as_ref().expect("floating").allow_overlap,
            )),
            InlineNode::Group(group) => {
                let overlap = group.anchor.as_ref().expect("floating").allow_overlap;
                for child in &group.children {
                    match child {
                        GroupChild::Shape(shape) => {
                            out.push((shape.fill.clone(), shape.shadow, overlap));
                        }
                        GroupChild::TextBox(text_box) => {
                            out.push((text_box.fill.clone(), text_box.shadow, overlap));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    Rgba { r, g, b, a }
}

fn expected() -> Vec<(Option<Fill>, Option<ShapeShadow>, bool)> {
    vec![
        (
            Some(Fill::Solid(rgba(255, 255, 0, 255))),
            Some(ShapeShadow {
                // 40% opaque black: `a:alpha val="40000"`.
                color: rgba(0, 0, 0, 102),
                blur_rad_emu: 50_800,
                dist_emu: 38_100,
                dir: 2_700_000,
                alignment: Some(ShadowAlignment::TopLeft),
                rot_with_shape: Some(false),
                sx: None,
                sy: None,
                kx: None,
                ky: None,
            }),
            false,
        ),
        (
            // The ellipse keeps ITS fill: neither the glow's red nor the
            // shadow's green reaches it.
            Some(Fill::Solid(rgba(0x33, 0x55, 0xc4, 255))),
            Some(ShapeShadow {
                color: rgba(0, 255, 0, 255),
                blur_rad_emu: 101_600,
                dist_emu: 76_200,
                dir: 5_400_000,
                alignment: None,
                rot_with_shape: None,
                sx: None,
                sy: None,
                kx: None,
                ky: None,
            }),
            true,
        ),
    ]
}

#[test]
fn a_shadow_and_allow_overlap_survive_a_round_trip() {
    let imported = import_main_document_xml(&source(), ImportConfig::default()).unwrap();
    assert_eq!(
        census(&imported.document),
        expected(),
        "read from Word's markup"
    );
    // The glow is not modelled, so it is named; the shadow and the list that
    // holds it are, so neither is.
    let features: Vec<&str> = imported
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(
        features.contains(&"glow"),
        "the glow is reported: {features:?}"
    );
    assert!(
        !features
            .iter()
            .any(|f| f.contains("outerShdw") || *f == "effectLst"),
        "a modelled shadow is not a loss: {features:?}"
    );

    let export = export_document(&imported.document, &BTreeMap::new()).expect("exports");
    let mut archive = zip::ZipArchive::new(Cursor::new(export.bytes.clone())).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(
        xml.contains(r#"allowOverlap="0""#),
        "the opt-out is written: {xml}"
    );
    assert!(
        xml.contains(r#"allowOverlap="1""#),
        "the default is written: {xml}"
    );
    assert!(
        xml.contains(
            r#"<a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw>"#
        ),
        "Word's preset is written back with its alpha: {xml}"
    );
    assert!(
        xml.contains(
            r#"<a:outerShdw blurRad="101600" dist="76200" dir="5400000"><a:srgbClr val="00FF00"/>"#
        ),
        "an attribute the source omitted stays omitted: {xml}"
    );

    let mut package = DocxPackage::open(&export.bytes, PackageLimits::default()).unwrap();
    let reopened = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap();
    assert_eq!(census(&reopened.document), expected(), "read back");
}
