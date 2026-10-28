//! A turned text box round-trips through DOCX (`docs/182` G-3).
//!
//! Word writes a text box's text direction as `wps:bodyPr@vert` ("Rotate all
//! text 90°/270°"), writes `wps:bodyPr@rot` on every text box (`rot="0"`), and
//! turns the whole shape with `a:xfrm@rot`. All three are read, written back
//! and read again here, from Word-shaped markup: a box whose `bodyPr` turns its
//! text, and a lone box whose SHAPE is turned — which the model carries as a
//! group of one, because `TextBox` has no shape rotation (`GroupTextBox` does).

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::export_document;
use casual_doc_import::{ImportConfig, ImportMode, import_main_document_xml, import_package};
use casual_doc_model::v1::{
    Document, GroupChild, InlineNode, TextBoxBodyProperties, TextBoxVerticalText,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// One floating `wps:wsp` text box, as Word writes it, with `xfrm` and `body`
/// spliced in.
fn word_text_box(id: u32, xfrm: &str, body: &str) -> String {
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{id}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="2743200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{id}" name="Text Box {id}"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="914400" cy="2743200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Turned {id}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr {body} spcFirstLastPara="0" vertOverflow="overflow" horzOverflow="overflow" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" numCol="1" spcCol="0" anchor="t" anchorCtr="0"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}

fn source() -> Vec<u8> {
    format!(
        r#"<w:document xmlns:w="urn:w" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:wps="urn:wps"><w:body><w:p>{}{}{}</w:p></w:body></w:document>"#,
        // Text turned by the body alone: Word's direction plus a body rotation.
        word_text_box(1, "", r#"rot="5400000" vert="vert270""#),
        // A shape turned by its xfrm, with Word's ever-present rot="0".
        word_text_box(2, r#" rot="1800000""#, r#"rot="0" vert="horz""#),
        // East Asian vertical, retained verbatim.
        word_text_box(3, "", r#"rot="0" vert="eaVert""#),
    )
    .into_bytes()
}

/// What the three boxes carry: each body's `(rot, vert)`, and the shape
/// rotation where one is modeled.
fn census(document: &Document) -> Vec<(i32, TextBoxVerticalText, Option<i32>)> {
    let mut out = Vec::new();
    let casual_doc_model::v1::BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("one paragraph");
    };
    for inline in &paragraph.inlines {
        let entry = |body: &TextBoxBodyProperties, shape: Option<i32>| {
            (body.text_rotation, body.vertical_text, shape)
        };
        match inline {
            InlineNode::TextBox(text_box) => out.push(entry(&text_box.body_properties, None)),
            InlineNode::Group(group) => {
                for child in &group.children {
                    if let GroupChild::TextBox(text_box) = child {
                        out.push(entry(&text_box.body_properties, text_box.rotation));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

const EXPECTED: [(i32, TextBoxVerticalText, Option<i32>); 3] = [
    (5_400_000, TextBoxVerticalText::Vertical270, None),
    (0, TextBoxVerticalText::Horizontal, Some(1_800_000)),
    (0, TextBoxVerticalText::EastAsianVertical, None),
];

#[test]
fn word_text_box_rotation_and_direction_survive_a_round_trip() {
    let imported = import_main_document_xml(&source(), ImportConfig::default()).unwrap();
    assert_eq!(
        census(&imported.document),
        EXPECTED,
        "read from Word's markup"
    );

    let export = export_document(&imported.document, &BTreeMap::new()).expect("exports");
    assert!(export.report.is_empty(), "{:?}", export.report.entries);
    let mut archive = zip::ZipArchive::new(Cursor::new(export.bytes.clone())).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(
        xml.contains(r#"<wps:bodyPr rot="5400000" vert="vert270""#),
        "the body's rotation and direction are written: {xml}"
    );
    assert!(xml.contains(r#"vert="eaVert""#), "{xml}");
    assert!(
        xml.contains(r#"<a:xfrm rot="1800000">"#),
        "the shape's rotation is written: {xml}"
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
    assert_eq!(census(&reopened.document), EXPECTED, "read back");
}
