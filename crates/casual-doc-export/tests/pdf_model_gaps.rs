//! The DOCX writer carries, or reports, the two model slots a PDF import needs
//! (`docs/182` gaps G-1 and G-2).
//!
//! - **G-1.** A non-DOCX font program is embedded as an obfuscated `.odttf`
//!   (ECMA-376 Part 1 §17.8.1) under its family's `w:font`, or reported by
//!   reason. The obfuscation is checked by an independent decoder written here,
//!   and the package is reopened by the real importer.
//! - **G-2.** DrawingML has no fill-rule attribute (`CT_Path2D`, §20.1.9.15), so
//!   an even-odd geometry is written re-wound to paint the same under either
//!   rule, or reported when no winding can.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::{CompatibilityReport, export_document};
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, BlockNode, CustomGeometry, Definitions, Document,
    DrawingAnchor, EmbeddedFontProgram, Extent, Fill, FillRule, FontDescriptor, FontProgramFormat,
    FontProgramStyle, FontSig, GeometryPoint, GroupChild, GroupShape, GroupTransform,
    HorizontalAnchor, HorizontalPosition, InlineNode, Paragraph, ParagraphProperties, PointEmu,
    Rgba, Run, RunProperties, ShapeGeometry, ShapePath, ShapePathCommand, VerticalAnchor,
    VerticalPosition, WordprocessingGroup, WrapMode,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use zip::ZipArchive;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn text_paragraph() -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(11),
            properties: RunProperties::default().into(),
            text: "x".to_owned(),
        })],
    })
}

fn part(package: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(package.to_vec())).unwrap();
    let mut file = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    Some(bytes)
}

fn text(package: &[u8], name: &str) -> String {
    String::from_utf8(part(package, name).unwrap_or_else(|| panic!("{name} is in the package")))
        .unwrap()
}

fn reopen(bytes: &[u8]) -> Document {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
    .document
}

fn features(report: &CompatibilityReport) -> Vec<(String, Option<String>)> {
    report
        .entries
        .iter()
        .map(|entry| (entry.feature.clone(), entry.location.part_name.clone()))
        .collect()
}

// --- G-1: font programs ------------------------------------------------------

/// A TrueType sfnt header followed by recognisable filler: the writer checks the
/// signature, never parses tables, so this is enough to prove the bytes travel.
fn truetype_bytes(tag: u8) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x01, 0x00, 0x00];
    bytes.extend((0..96_u8).map(|index| index ^ tag));
    bytes
}

fn program_document(fonts: Vec<FontDescriptor>, programs: Vec<EmbeddedFontProgram>) -> Document {
    Document::new(
        node(1),
        vec![text_paragraph()],
        Definitions {
            font_table: fonts,
            font_programs: programs,
            ..Definitions::default()
        },
    )
    .unwrap()
}

/// An independent ECMA-376 §17.8.1 decoder: the `w:fontKey`'s 32 hex digits,
/// as 16 bytes in string order, reversed, XORed over the first 32 bytes.
fn deobfuscate(font_key: &str, obfuscated: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = font_key
        .chars()
        .filter_map(|c| c.to_digit(16))
        .map(|d| d as u8)
        .collect();
    assert_eq!(digits.len(), 32, "a fontKey is 32 hex digits: {font_key}");
    let key: Vec<u8> = digits
        .chunks(2)
        .map(|pair| pair[0] * 16 + pair[1])
        .rev()
        .collect();
    let mut out = obfuscated.to_vec();
    for (index, byte) in out.iter_mut().take(32).enumerate() {
        *byte ^= key[index % 16];
    }
    out
}

fn attribute<'a>(xml: &'a str, element: &str, name: &str) -> &'a str {
    let at = xml
        .find(&format!("<{element} "))
        .unwrap_or_else(|| panic!("<{element}> in {xml}"));
    let rest = &xml[at..];
    let start = rest.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
    let end = start + rest[start..].find('"').unwrap();
    &rest[start..end]
}

/// G-1 carried: a TrueType program becomes `w:font/w:embedBold` with a
/// `/font` relationship to an `.odttf` that decodes back to the exact bytes,
/// the package reopens with the face where Word would look for it, and nothing
/// is reported.
#[test]
fn a_truetype_font_program_is_embedded_as_an_obfuscated_odttf() {
    let plain = truetype_bytes(0x5a);
    let doc = program_document(
        Vec::new(),
        vec![
            EmbeddedFontProgram::new(
                "Probe Sans",
                FontProgramStyle::Bold,
                FontProgramFormat::TrueType,
                "pdf/fonts/F1.ttf",
            )
            .with_subsetted(true),
        ],
    );
    let media = BTreeMap::from([("pdf/fonts/F1.ttf".to_owned(), plain.clone())]);
    let export = export_document(&doc, &media).unwrap();
    assert!(
        export.report.is_empty(),
        "a carried program reports nothing: {:?}",
        features(&export.report)
    );

    let table = text(&export.bytes, "word/fontTable.xml");
    assert!(table.contains(r#"<w:font w:name="Probe Sans">"#), "{table}");
    assert!(table.contains(r#"w:subsetted="true""#), "{table}");
    let key = attribute(&table, "w:embedBold", "w:fontKey");
    let rid = attribute(&table, "w:embedBold", "r:id");

    let rels = text(&export.bytes, "word/_rels/fontTable.xml.rels");
    assert!(
        rels.contains(&format!(r#"Id="{rid}""#)) && rels.contains("relationships/font"),
        "{rels}"
    );
    let target = attribute(&rels, "Relationship", "Target");
    assert!(target.ends_with(".odttf"), "{target}");
    let types = text(&export.bytes, "[Content_Types].xml");
    assert!(types.contains(r#"Extension="odttf""#), "{types}");

    let odttf = part(&export.bytes, &format!("word/{target}")).expect("the part is written");
    assert_ne!(odttf, plain, "the part is obfuscated, not the raw program");
    assert_eq!(&odttf[32..], &plain[32..], "only the first 32 bytes change");
    assert_eq!(
        deobfuscate(key, &odttf),
        plain,
        "an independent ECMA-376 decoder recovers the program"
    );

    let reopened = reopen(&export.bytes);
    let font = &reopened.definitions().font_table[0];
    assert_eq!(font.name, "Probe Sans");
    let face = font.embedded.bold.as_ref().expect("filed in the bold slot");
    assert_eq!(face.font_key, key);
    assert!(face.subsetted);

    let again = export_document(&doc, &media).unwrap();
    assert_eq!(again.bytes, export.bytes, "the font key is deterministic");
}

/// A program of a family the document's own font table names joins that entry
/// instead of adding a second `w:font` of the same name.
#[test]
fn a_font_program_joins_its_familys_font_table_entry() {
    let descriptor = FontDescriptor {
        name: "Probe Sans".to_owned(),
        alt_name: None,
        panose1: Some("020B0604020202020204".to_owned()),
        charset: None,
        family: None,
        pitch: None,
        sig: FontSig::default(),
        not_true_type: false,
        embedded: Default::default(),
    };
    let doc = program_document(
        vec![descriptor],
        vec![EmbeddedFontProgram::new(
            "Probe Sans",
            FontProgramStyle::Regular,
            FontProgramFormat::TrueType,
            "pdf/fonts/F1.ttf",
        )],
    );
    let media = BTreeMap::from([("pdf/fonts/F1.ttf".to_owned(), truetype_bytes(1))]);
    let export = export_document(&doc, &media).unwrap();
    let table = text(&export.bytes, "word/fontTable.xml");
    assert_eq!(
        table.matches(r#"w:name="Probe Sans""#).count(),
        1,
        "{table}"
    );
    let reopened = reopen(&export.bytes);
    let font = &reopened.definitions().font_table[0];
    assert_eq!(font.panose1.as_deref(), Some("020B0604020202020204"));
    assert!(
        font.embedded.regular.is_some(),
        "the program's face is on it"
    );
}

/// Every program the package cannot carry is reported by reason, and leaves
/// no `w:embed*`, relationship or part behind.
#[test]
fn an_uncarried_font_program_is_reported_by_reason() {
    let program =
        |style, format, part: &str| EmbeddedFontProgram::new("Probe Sans", style, format, part);
    let doc = program_document(
        Vec::new(),
        vec![
            program(
                FontProgramStyle::Regular,
                FontProgramFormat::OpenTypeCff,
                "pdf/fonts/cff.otf",
            ),
            program(
                FontProgramStyle::Bold,
                FontProgramFormat::BareCff,
                "pdf/fonts/bare.cff",
            ),
            program(
                FontProgramStyle::Italic,
                FontProgramFormat::TrueType,
                "pdf/fonts/absent.ttf",
            ),
            program(
                FontProgramStyle::BoldItalic,
                FontProgramFormat::TrueType,
                "pdf/fonts/first.ttf",
            ),
            // A second subset of the same face: one slot, so the first wins.
            program(
                FontProgramStyle::BoldItalic,
                FontProgramFormat::TrueType,
                "pdf/fonts/second.ttf",
            ),
        ],
    );
    let mut otto = b"OTTO".to_vec();
    otto.extend([0_u8; 60]);
    let media = BTreeMap::from([
        ("pdf/fonts/cff.otf".to_owned(), otto),
        (
            "pdf/fonts/bare.cff".to_owned(),
            vec![1, 0, 4, 2, 0, 0, 0, 0],
        ),
        ("pdf/fonts/first.ttf".to_owned(), truetype_bytes(2)),
        ("pdf/fonts/second.ttf".to_owned(), truetype_bytes(3)),
    ]);
    let export = export_document(&doc, &media).unwrap();
    let mut reported = features(&export.report);
    reported.sort();
    let entry = |feature: &str, part: &str| (feature.to_owned(), Some(part.to_owned()));
    assert_eq!(
        reported,
        vec![
            entry(
                "docx.export.font_program.missing_bytes",
                "pdf/fonts/absent.ttf"
            ),
            entry("docx.export.font_program.not_carried", "pdf/fonts/bare.cff"),
            entry("docx.export.font_program.not_carried", "pdf/fonts/cff.otf"),
            entry(
                "docx.export.font_program.slot_taken",
                "pdf/fonts/second.ttf"
            ),
        ]
    );
    let table = text(&export.bytes, "word/fontTable.xml");
    assert_eq!(
        table.matches("<w:embed").count(),
        1,
        "only the first subset: {table}"
    );
    assert!(table.contains("<w:embedBoldItalic "), "{table}");
}

// --- G-2: the even-odd fill rule ---------------------------------------------

fn square(x: i64, y: i64, size: i64) -> Vec<ShapePathCommand> {
    let mut commands = vec![ShapePathCommand::MoveTo {
        point: GeometryPoint::literal(x, y),
    }];
    for (cx, cy) in [(x + size, y), (x + size, y + size), (x, y + size)] {
        commands.push(ShapePathCommand::LineTo {
            point: GeometryPoint::literal(cx, cy),
        });
    }
    commands.push(ShapePathCommand::Close);
    commands
}

fn shape_document(geometry: CustomGeometry) -> Document {
    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let shape = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent,
        geometry: ShapeGeometry::Other,
        preset: None,
        adjustments: Vec::new(),
        path: Some(geometry),
        fill: Some(Fill::Solid(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(90),
        anchor: Some(DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(914_400),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(914_400),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc: false,
        }),
        relative_height: Some(1),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![shape],
    }));
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![group],
        })],
        Definitions::default(),
    )
    .unwrap()
}

fn geometry_of(document: &Document) -> CustomGeometry {
    let BlockNode::Paragraph(paragraph) = &document.body()[0] else {
        panic!("a paragraph");
    };
    let group = paragraph
        .inlines
        .iter()
        .find_map(|inline| match inline {
            InlineNode::Group(group) => Some(group),
            _ => None,
        })
        .expect("the group survives");
    let GroupChild::Shape(shape) = &group.children[0] else {
        panic!("a shape");
    };
    shape.path.clone().expect("a custom geometry")
}

/// G-2 carried: nested same-wound squares under even-odd are written with the
/// inner square reversed, so Word (or anything painting non-zero) shows the
/// hole, and the save reports nothing because nothing was lost.
#[test]
fn an_even_odd_ring_is_written_re_wound_so_its_hole_survives() {
    let mut commands = square(0, 0, 100);
    commands.extend(square(25, 25, 50));
    let geometry = CustomGeometry::single_path(ShapePath {
        width_emu: 100,
        height_emu: 100,
        ..ShapePath::new(commands)
    })
    .with_fill_rule(FillRule::EvenOdd);
    let export = export_document(&shape_document(geometry.clone()), &BTreeMap::new()).unwrap();
    assert!(
        export.report.is_empty(),
        "a re-wound ring loses nothing: {:?}",
        features(&export.report)
    );

    let written = text(&export.bytes, "word/document.xml");
    // The inner square, reversed: from its last corner back to its first.
    let inner_reversed = concat!(
        r#"<a:moveTo><a:pt x="25" y="75"/></a:moveTo>"#,
        r#"<a:lnTo><a:pt x="75" y="75"/></a:lnTo>"#,
        r#"<a:lnTo><a:pt x="75" y="25"/></a:lnTo>"#,
        r#"<a:lnTo><a:pt x="25" y="25"/></a:lnTo>"#,
    );
    assert!(written.contains(inner_reversed), "{written}");

    let reopened = geometry_of(&reopen(&export.bytes));
    assert_eq!(reopened.fill_rule, FillRule::NonZero, "DOCX states no rule");
    assert_eq!(reopened, geometry.to_non_zero().unwrap());
}

/// G-2 reported: crossing subpaths cannot be re-wound, so the geometry is
/// written as authored and the rule is named as not carried.
#[test]
fn an_even_odd_geometry_that_cannot_be_re_wound_is_reported() {
    let mut commands = square(0, 0, 100);
    commands.extend(square(50, 50, 100));
    let geometry = CustomGeometry::single_path(ShapePath::new(commands.clone()))
        .with_fill_rule(FillRule::EvenOdd);
    let export = export_document(&shape_document(geometry), &BTreeMap::new()).unwrap();
    assert_eq!(
        features(&export.report),
        vec![(
            "docx.export.custom_geometry.even_odd_fill".to_owned(),
            Some("word/document.xml".to_owned())
        )]
    );
    let reopened = geometry_of(&reopen(&export.bytes));
    assert_eq!(reopened.paths[0].commands, commands, "written as authored");
}
