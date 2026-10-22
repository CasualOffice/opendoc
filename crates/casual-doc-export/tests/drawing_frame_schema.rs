// SPDX-License-Identifier: Apache-2.0

//! Every drawing frame the writer can emit carries what the schema REQUIRES of
//! it (`docs/109` OL-02).
//!
//! # Why this file exists
//!
//! A picture made floating in the editor saved as a schema-invalid package. The
//! three `wp:anchor` writers (picture, text box, group) wrote `@relativeHeight`
//! only when the model carried one, and an object floated by the editor carries
//! none; the side wraps wrote `@wrapText` only when the model carried a side, and
//! a fresh float carries none; and a new Tight or Through wrap was written with no
//! `wp:wrapPolygon`, which `CT_WrapTight` and `CT_WrapThrough` require. Word is
//! strict about exactly this vocabulary — a package that fails `CT_Anchor` is
//! "unreadable content" — and Word is not available to this repository, so the
//! guard has to be the schema itself.
//!
//! The defects were found one attribute at a time, which is the shape SKILL §10
//! names: fix the class. So this file does not test the three attributes that
//! were missing. It states the required attributes and children of every
//! `wp:` element the drawing writers emit (`dml-wordprocessingDrawing.xsd`,
//! ECMA-376 Part 1, 5th edition), and validates the written body against that
//! table for every wrap mode, on all three anchor paths, and for the in-line
//! frame — with the model stripped of every optional value an editor-made object
//! lacks. A writer that drops any required attribute or child, on any path,
//! reddens here.
//!
//! # What this file does NOT claim
//!
//! It is not a full XSD validator. It checks presence, order of the required
//! children, and the value space of the attributes whose types are small
//! enumerations or integers. It does not check `a:graphic`'s content model,
//! which belongs to DrawingML proper and to other guards.
//!
//! # Complexity
//!
//! `O(package bytes)` per variant: one export of a small fixture, one XML walk.
//! Test-time only.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, Drawing, InlineNode, WrapMode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// A fixture with an anchored picture, an anchored text box and an anchored
/// group: one float on each of the writer's three `wp:anchor` paths.
const FLOATS_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/wrap-text-sides.docx");

/// One `wp:` element's schema contract: the attributes it must carry, and the
/// children it must contain, in schema order. A child entry with several names
/// is a `xsd:choice` — exactly one of them must appear at that position.
struct Contract {
    element: &'static str,
    attributes: &'static [&'static str],
    children: &'static [&'static [&'static str]],
}

/// The wrap choice of `CT_Anchor` (`EG_WrapType`).
const WRAP_CHOICE: &[&str] = &[
    "wrapNone",
    "wrapSquare",
    "wrapTight",
    "wrapThrough",
    "wrapTopAndBottom",
];

/// The required parts of every `wp:` element the drawing writers emit.
///
/// `CT_Anchor`'s `distT`/`distB`/`distL`/`distR`, `simplePos` (the attribute)
/// and `hidden` are optional and so are absent from the list; its five other
/// attributes are required. Optional children (`wp:effectExtent`,
/// `wp:cNvGraphicFramePr`) are not listed: the order check skips anything not
/// in the list.
const CONTRACTS: &[Contract] = &[
    Contract {
        element: "anchor",
        attributes: &[
            "relativeHeight",
            "behindDoc",
            "locked",
            "layoutInCell",
            "allowOverlap",
        ],
        children: &[
            &["simplePos"],
            &["positionH"],
            &["positionV"],
            &["extent"],
            WRAP_CHOICE,
            &["docPr"],
            &["graphic"],
        ],
    },
    Contract {
        element: "inline",
        attributes: &[],
        children: &[&["extent"], &["docPr"], &["graphic"]],
    },
    Contract {
        element: "simplePos",
        attributes: &["x", "y"],
        children: &[],
    },
    Contract {
        element: "positionH",
        attributes: &["relativeFrom"],
        children: &[&["align", "posOffset"]],
    },
    Contract {
        element: "positionV",
        attributes: &["relativeFrom"],
        children: &[&["align", "posOffset"]],
    },
    Contract {
        element: "extent",
        attributes: &["cx", "cy"],
        children: &[],
    },
    Contract {
        element: "docPr",
        attributes: &["id", "name"],
        children: &[],
    },
    Contract {
        element: "wrapSquare",
        attributes: &["wrapText"],
        children: &[],
    },
    Contract {
        element: "wrapTight",
        attributes: &["wrapText"],
        children: &[&["wrapPolygon"]],
    },
    Contract {
        element: "wrapThrough",
        attributes: &["wrapText"],
        children: &[&["wrapPolygon"]],
    },
    Contract {
        element: "wrapPolygon",
        attributes: &[],
        // `wp:start`, then `lineTo` with minOccurs=2.
        children: &[&["start"], &["lineTo"], &["lineTo"]],
    },
    Contract {
        element: "start",
        attributes: &["x", "y"],
        children: &[],
    },
    Contract {
        element: "lineTo",
        attributes: &["x", "y"],
        children: &[],
    },
];

/// One element as the walk saw it.
struct Seen {
    local: String,
    attributes: BTreeMap<String, String>,
    children: Vec<String>,
}

/// Every `wp:`-prefixed element in `xml`, with its attributes and its child
/// local names. Asked through element and attribute NAMES, never substrings: a
/// body can say "relativeHeight" in its text.
fn wp_elements(xml: &[u8]) -> Vec<Seen> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut stack: Vec<Option<Seen>> = Vec::new();
    let mut done = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .expect("the written body is well-formed XML");
        match event {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(ref element)
            | quick_xml::events::Event::Empty(ref element) => {
                let empty = matches!(event, quick_xml::events::Event::Empty(_));
                let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
                let local = String::from_utf8_lossy(element.local_name().as_ref()).into_owned();
                if let Some(Some(parent)) = stack.last_mut() {
                    parent.children.push(local.clone());
                }
                let seen = name.starts_with("wp:").then(|| Seen {
                    local,
                    attributes: element
                        .attributes()
                        .with_checks(false)
                        .flatten()
                        .map(|attribute| {
                            (
                                String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
                                String::from_utf8_lossy(attribute.value.as_ref()).into_owned(),
                            )
                        })
                        .collect(),
                    children: Vec::new(),
                });
                if empty {
                    done.extend(seen);
                } else {
                    stack.push(seen);
                }
            }
            quick_xml::events::Event::End(_) => {
                if let Some(Some(seen)) = stack.pop() {
                    done.push(seen);
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    done
}

/// The schema violations of one element, as sentences.
fn violations(seen: &Seen) -> Vec<String> {
    let mut found = Vec::new();
    let Some(contract) = CONTRACTS.iter().find(|c| c.element == seen.local) else {
        return found;
    };
    for attribute in contract.attributes {
        if !seen.attributes.contains_key(*attribute) {
            found.push(format!("wp:{} lacks required @{attribute}", seen.local));
        }
    }
    // Required children, in order. Children not named by the contract are
    // optional ones and are skipped.
    let mut cursor = 0;
    for required in contract.children {
        let at = seen.children[cursor..]
            .iter()
            .position(|child| required.contains(&child.as_str()));
        match at {
            Some(offset) => cursor += offset + 1,
            None => found.push(format!(
                "wp:{} lacks required child {} (in order); children: {:?}",
                seen.local,
                required.join("|"),
                seen.children
            )),
        }
    }
    // Small value spaces, where a wrong value is as invalid as a missing one.
    for (attribute, value) in &seen.attributes {
        let ok = match attribute.as_str() {
            "relativeHeight" => value.parse::<u32>().is_ok(),
            "behindDoc" | "locked" | "layoutInCell" | "allowOverlap" => {
                matches!(value.as_str(), "0" | "1" | "true" | "false")
            }
            "wrapText" => matches!(value.as_str(), "bothSides" | "left" | "right" | "largest"),
            _ => true,
        };
        if !ok {
            found.push(format!("wp:{} has @{attribute}={value:?}", seen.local));
        }
    }
    found
}

/// The `word/document.xml` of a package.
fn document_xml(bytes: &[u8]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let mut part = Vec::new();
    archive
        .by_name("word/document.xml")
        .expect("a main document part")
        .read_to_end(&mut part)
        .expect("a readable part");
    part
}

/// The `word/media/*` parts, so the written anchors keep their pictures.
fn media(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a valid ZIP entry name").into_owned())
        .filter(|name| name.starts_with("word/media/"))
        .collect();
    names
        .into_iter()
        .map(|name| {
            let mut buffer = Vec::new();
            archive
                .by_name(&name)
                .expect("a listed entry")
                .read_to_end(&mut buffer)
                .expect("a readable entry");
            (name, buffer)
        })
        .collect()
}

/// What an object made floating IN THE EDITOR looks like to the writer: no
/// authored z-order, no authored side, no authored contour. These are exactly
/// the optional model values a fresh float lacks, and each one gates a required
/// attribute or child in the written frame.
fn strip_authored_extras(document: &mut Document, wrap: WrapMode, behind: bool) -> usize {
    let mut floats = 0;
    for block in document.body_mut() {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        for inline in &mut paragraph.inlines {
            let (anchor, relative_height) = match inline {
                InlineNode::AnchoredDrawing(drawing) => {
                    (&mut drawing.anchor, &mut drawing.relative_height)
                }
                InlineNode::TextBox(text_box) => match text_box.anchor.as_mut() {
                    Some(anchor) => (anchor, &mut text_box.relative_height),
                    None => continue,
                },
                InlineNode::Group(group) => match group.anchor.as_mut() {
                    Some(anchor) => (anchor, &mut group.relative_height),
                    None => continue,
                },
                _ => continue,
            };
            anchor.wrap = wrap;
            anchor.behind_doc = behind;
            anchor.wrap_text = None;
            anchor.wrap_polygon = None;
            *relative_height = None;
            floats += 1;
        }
    }
    floats
}

/// Every anchored object turned in line: a picture becomes a `Drawing`, a text
/// box and a group lose their anchor. Returns how many were converted.
fn make_everything_inline(document: &mut Document) -> usize {
    let mut converted = 0;
    for block in document.body_mut() {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        for inline in &mut paragraph.inlines {
            match inline {
                InlineNode::AnchoredDrawing(drawing) => {
                    let drawing = drawing.as_ref().clone();
                    *inline = InlineNode::Drawing(Box::new(Drawing {
                        id: drawing.id,
                        media: drawing.media,
                        // The optional extent gates a REQUIRED `wp:extent`.
                        extent: None,
                        descr: drawing.descr,
                        crop: drawing.crop,
                        opacity: drawing.opacity,
                        hyperlink: drawing.hyperlink,
                        border: drawing.border,
                        flip_h: drawing.flip_h,
                        flip_v: drawing.flip_v,
                        rotation: drawing.rotation,
                    }));
                    converted += 1;
                }
                InlineNode::TextBox(text_box) if text_box.anchor.is_some() => {
                    text_box.anchor = None;
                    text_box.relative_height = None;
                    // An optional extent in the model gates a REQUIRED
                    // `wp:extent` in the frame, so the absence is the case.
                    text_box.extent = None;
                    converted += 1;
                }
                // NOT a group. An in-line group has no `wp:inline` path in
                // either direction yet: the writer emits it as a `wp:anchor`
                // without a position, and the importer drops a `wp:inline` that
                // holds a `wpg:wgp` ("grpSp" reported). Fixing one half alone
                // would turn an invalid-but-kept group into a valid-but-lost
                // one, so both halves ship together as `docs/109` OL-07, and
                // this test gains the group then.
                _ => {}
            }
        }
    }
    converted
}

fn import(bytes: &[u8]) -> (Document, casual_doc_import::RetainedParts) {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the package is admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the package imports");
    (import.document, import.retained_parts)
}

/// The written body's `wp:` elements and every schema violation among them.
fn write_and_check(
    document: &Document,
    retained: &casual_doc_import::RetainedParts,
) -> (Vec<Seen>, Vec<String>) {
    let written = export_document_with_retained_parts(document, &media(FLOATS_DOCX), retained)
        .expect("the model is writable")
        .bytes;
    let elements = wp_elements(&document_xml(&written));
    let problems = elements.iter().flat_map(violations).collect();
    (elements, problems)
}

/// Every wrap mode, on all three anchor paths, with every optional model value
/// an editor-made float lacks: the written frames satisfy the schema.
#[test]
fn every_anchor_the_writer_emits_satisfies_the_schema_for_every_wrap() {
    let cases = [
        ("wrapNone in front", WrapMode::None, false),
        ("wrapNone behind", WrapMode::None, true),
        ("wrapSquare", WrapMode::Square, false),
        ("wrapTight", WrapMode::Tight, false),
        ("wrapThrough", WrapMode::Through, false),
        ("wrapTopAndBottom", WrapMode::TopAndBottom, false),
    ];
    for (label, wrap, behind) in cases {
        let (mut document, retained) = import(FLOATS_DOCX);
        let floats = strip_authored_extras(&mut document, wrap, behind);
        assert!(
            floats >= 3,
            "{label}: the fixture must carry a float on each of the three anchor paths \
             (picture, text box, group), or this case is vacuous; found {floats}"
        );
        let (elements, problems) = write_and_check(&document, &retained);
        let anchors = elements
            .iter()
            .filter(|seen| seen.local == "anchor")
            .count();
        assert!(
            anchors >= floats,
            "{label}: every float is written as a wp:anchor ({anchors} < {floats})"
        );
        assert!(
            problems.is_empty(),
            "{label}: the written frames violate the schema:\n{}",
            problems.join("\n")
        );
    }
}

/// The in-line frame too: a picture and a text box taken in line (a group is
/// `docs/109` OL-07; see `make_everything_inline`).
#[test]
fn every_inline_frame_the_writer_emits_satisfies_the_schema() {
    let (mut document, retained) = import(FLOATS_DOCX);
    let converted = make_everything_inline(&mut document);
    assert!(
        converted >= 2,
        "the fixture's picture and text box must both convert; got {converted}"
    );
    let (elements, problems) = write_and_check(&document, &retained);
    assert!(
        elements.iter().any(|seen| seen.local == "inline"),
        "an in-line object is written as wp:inline"
    );
    assert!(
        problems.is_empty(),
        "the written in-line frames violate the schema:\n{}",
        problems.join("\n")
    );
}

/// The validator itself can fail: an anchor missing `@relativeHeight` and a
/// square wrap missing `@wrapText` are both reported. Without this, a walk that
/// silently found no elements would pass everything above.
#[test]
fn the_contract_table_reports_a_missing_required_attribute() {
    let xml = br#"<w:document xmlns:w="w" xmlns:wp="wp" xmlns:a="a"><wp:anchor behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1" cy="1"/><wp:wrapTight><wp:wrapPolygon><wp:start x="0" y="0"/><wp:lineTo x="1" y="1"/></wp:wrapPolygon></wp:wrapTight><wp:docPr id="1" name="x"/><a:graphic/></wp:anchor></w:document>"#;
    let problems: Vec<String> = wp_elements(xml).iter().flat_map(violations).collect();
    assert!(
        problems.iter().any(|p| p.contains("@relativeHeight")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("wp:wrapTight lacks required @wrapText")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("wp:wrapPolygon lacks required child lineTo")),
        "{problems:?}"
    );
}
