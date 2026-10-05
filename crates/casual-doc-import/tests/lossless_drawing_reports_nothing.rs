// SPDX-License-Identifier: Apache-2.0

//! A drawing that loses nothing reports nothing — and a drawing that loses
//! something reports exactly that (`109` HF-174, HF-243).
//!
//! # Why a package, and why here
//!
//! The crate's own `tests.rs` already holds the body of HF-174's work:
//! `a_document_that_loses_nothing_reports_nothing` pins an empty report for a
//! document built of no-op markup, `the_corpus_reports_exactly_these_findings`
//! pins an exact count per committed fixture, and each conditional arm of the
//! no-op class has a both-halves unit test. Two things that file cannot reach are
//! here instead.
//!
//! **The misattribution.** HF-174's last named item is not a no-op arm at all:
//! *"`a:moveTo` inside `a:custGeom` is reported through the tracked-revision arm,
//! so a document with zero tracked changes reports five dropped moves."* The
//! WordprocessingML tracked move `w:moveTo` and the DrawingML path command
//! `a:moveTo` are the same XML local name, and this importer matches on local
//! names (deliberately — a namespace prefix is whatever the producer bound it to).
//! So a freeform shape's outline was being charged to the revision machinery. The
//! `on_start` and `on_end` arms that fixed it are ordered — the open-`a:custGeom`
//! arm is written *first*, ahead of the revision arms — and **nothing enforces an
//! ordering except a test that notices when it changes.** This file is that test.
//!
//! **The lone-picture transform.** HF-243's attribute gate found
//! `is_drawing_scaffolding` silencing `a:off`, `a:ext` and `a:prstGeom`
//! unconditionally, which is the defect FID-P-03 had already found three times in
//! that same list (`wp:effectExtent`, `a:graphicFrameLocks`, `a:picLocks`). Those
//! three now have conditional arms, and a conditional arm needs **both** halves
//! guarded or it is half a rule: silencing the populated form hides a real loss,
//! and reporting the empty form is the false-finding defect HF-174 is about. The
//! gate itself proves the loud half (that is how it declares the attribute read);
//! only a test can prove the quiet half, because "reports nothing" is invisible to
//! a gate looking for unreported losses.
//!
//! Both need a drawing inside a real package rather than a bare document part: the
//! picture arms consult `wp:extent` and a resolved `a:blip@r:embed`, so the media
//! relationship has to exist.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const ROOT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;

/// The namespace declarations every fixture below shares.
const NS: &str = concat!(
    r#" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#,
    r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#,
    r#" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing""#,
    r#" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#,
    r#" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#,
    r#" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#,
);

/// A 1x1 PNG, so `a:blip@r:embed` resolves to real bytes. A reference whose bytes
/// are missing is dropped and reported (FID-R-06), which would make every
/// assertion here a measurement of the wrong thing.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// The picture's size, written into `wp:extent` and echoed by `a:ext` exactly as
/// Word writes it.
const EXTENT_CX: i64 = 914_400;
const EXTENT_CY: i64 = 457_200;

/// An inline picture whose `pic:spPr` carries the transform and geometry the
/// model does not consume, in whichever form the caller asks for.
fn picture_document(off: (i64, i64), ext: (i64, i64), preset: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document{NS}><w:body><w:p><w:r><w:drawing>
  <wp:inline distT="0" distB="0" distL="0" distR="0">
    <wp:extent cx="{EXTENT_CX}" cy="{EXTENT_CY}"/>
    <wp:effectExtent l="0" t="0" r="0" b="0"/>
    <wp:docPr id="1"/>
    <wp:cNvGraphicFramePr><a:graphicFrameLocks/></wp:cNvGraphicFramePr>
    <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
      <pic:pic>
        <pic:nvPicPr><pic:cNvPr id="1"/><pic:cNvPicPr/></pic:nvPicPr>
        <pic:blipFill><a:blip r:embed="rId9"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>
        <pic:spPr>
          <a:xfrm><a:off x="{off_x}" y="{off_y}"/><a:ext cx="{ext_cx}" cy="{ext_cy}"/></a:xfrm>
          <a:prstGeom prst="{preset}"><a:avLst/></a:prstGeom>
        </pic:spPr>
      </pic:pic>
    </a:graphicData></a:graphic>
  </wp:inline>
</w:drawing></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#,
        off_x = off.0,
        off_y = off.1,
        ext_cx = ext.0,
        ext_cy = ext.1,
    )
}

/// A freeform shape whose outline is five `a:moveTo` path commands and nothing
/// else, in a document with **no tracked change anywhere** — no `w:ins`, no
/// `w:del`, no `w:moveFrom`, no `w:moveTo`, and no `w:moveToRangeStart`.
///
/// Five disjoint subpaths are deliberately outside the modelled straight-line
/// subset (a path with a second `a:moveTo` is disjoint), so the geometry really is
/// a loss and really is reported — as `a:custGeom`, which is the construct that
/// left the subset, and never as a move.
fn five_move_commands_and_no_revisions() -> String {
    let moves: String = (0..5)
        .map(|i| format!(r#"<a:moveTo><a:pt x="{i}00" y="{i}00"/></a:moveTo>"#))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document{NS}><w:body><w:p><w:r><w:drawing>
  <wp:inline distT="0" distB="0" distL="0" distR="0">
    <wp:extent cx="{EXTENT_CX}" cy="{EXTENT_CY}"/>
    <wp:docPr id="1"/>
    <a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
      <wps:wsp>
        <wps:cNvSpPr/>
        <wps:spPr>
          <a:xfrm><a:off x="0" y="0"/><a:ext cx="{EXTENT_CX}" cy="{EXTENT_CY}"/></a:xfrm>
          <a:custGeom><a:avLst/><a:pathLst><a:path w="1000" h="1000">{moves}</a:path></a:pathLst></a:custGeom>
        </wps:spPr>
        <wps:txbx><w:txbxContent><w:p><w:r><w:t>Outline</w:t></w:r></w:p></w:txbxContent></wps:txbx>
        <wps:bodyPr/>
      </wps:wsp>
    </a:graphicData></a:graphic>
  </wp:inline>
</w:drawing></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#
    )
}

/// Zips a document part into a package with the media its drawing references.
fn package(document: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/media/image1.png", PNG),
    ] {
        writer.start_file(name, options).expect("a zip entry");
        writer.write_all(bytes).expect("a written entry");
    }
    writer.finish().expect("a finished zip").into_inner()
}

/// One import of a document part, as a package.
struct Imported {
    document: Document,
    /// Every feature the compatibility report names, as a set.
    features: BTreeSet<String>,
}

fn import(document: &str) -> Imported {
    let bytes = package(document);
    let mut package =
        DocxPackage::open(&bytes, PackageLimits::default()).expect("the package is admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the document imports");
    Imported {
        features: import
            .report
            .entries
            .iter()
            .map(|entry| entry.feature.clone())
            .collect(),
        document: import.document,
    }
}

/// Every tracked revision anywhere in the document's body, as its kind.
///
/// Walks the model rather than asking the report, because the report and the model
/// are two separate ways for the misattribution to show: the defect charged
/// `a:moveTo` to the revision arm, and that arm both *reports* and *commits*. A
/// test that only read the report would pass while the model carried five phantom
/// moves.
fn revision_kinds(document: &Document) -> Vec<String> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<String>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    for inline in &paragraph.inlines {
                        if let InlineNode::Revision(revision) = inline {
                            out.push(format!("{:?}", revision.kind));
                        }
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(document.body(), &mut out);
    out
}

#[test]
fn a_custom_geometry_path_command_is_not_a_tracked_move() {
    let imported = import(&five_move_commands_and_no_revisions());

    // The guarantee, stated as the document states it: this file has no tracked
    // change in it, so the import has none either. Asserted against the MODEL
    // first, because that is where a phantom move would do damage a reader could
    // see — five insertions attributed to nobody, in a Review pane the author
    // never opened.
    assert!(
        revision_kinds(&imported.document).is_empty(),
        "a document with no tracked change imported with these revisions: {:?}",
        revision_kinds(&imported.document),
    );

    // And in the report, where HF-174 counted it: five findings named `moveTo`.
    for misattributed in ["moveTo", "moveFrom", "ins", "del"] {
        assert!(
            !imported.features.contains(misattributed),
            "a shape's outline was charged to the tracked-revision arm as \
             `{misattributed}`: {:?}",
            imported.features,
        );
    }

    // The loss that IS there is still named, so this is not the reporter being
    // switched off: five disjoint subpaths leave the modelled straight-line
    // subset, and `a:custGeom` says so.
    assert!(
        imported.features.contains("custGeom"),
        "the geometry left the modelled subset and nothing said so: {:?}",
        imported.features,
    );
}

#[test]
fn a_lone_pictures_identity_transform_and_rectangular_frame_report_nothing() {
    // The quiet half of the three conditional arms, in the exact form Word writes
    // on an inline picture: a zero offset, an `a:ext` echoing `wp:extent`, and the
    // rectangular preset that the picture's placement already describes.
    let imported = import(&picture_document((0, 0), (EXTENT_CX, EXTENT_CY), "rect"));
    assert!(
        matches!(
            imported.document.body().first(),
            Some(BlockNode::Paragraph(paragraph))
                if matches!(paragraph.inlines.first(), Some(InlineNode::Drawing(_)))
        ),
        "the fixture did not import as a picture: {:?}",
        imported.document.body().first(),
    );
    assert!(
        imported.features.is_empty(),
        "a picture that lost nothing reported: {:?}",
        imported.features,
    );
}

#[test]
fn a_lone_pictures_real_transform_and_shaped_frame_are_each_reported() {
    // The loud half, one arm at a time, so a single over-broad condition cannot
    // make the trio look guarded. Each case changes exactly one thing from the
    // lossless fixture above.
    for (what, document, expected) in [
        (
            "a translated image",
            picture_document((114_300, 0), (EXTENT_CX, EXTENT_CY), "rect"),
            "off",
        ),
        (
            "a frame sized unlike the drawing",
            picture_document((0, 0), (EXTENT_CX / 2, EXTENT_CY), "rect"),
            "ext",
        ),
        (
            "an ellipse-cropped image",
            picture_document((0, 0), (EXTENT_CX, EXTENT_CY), "ellipse"),
            "prstGeom",
        ),
    ] {
        let imported = import(&document);
        assert!(
            imported.features.contains(expected),
            "{what} was dropped in silence; expected `{expected}` in: {:?}",
            imported.features,
        );
    }
}
