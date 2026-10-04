// SPDX-License-Identifier: Apache-2.0

//! `w:wrap@wrapText` — which side(s) of a float the text may flow down — is read,
//! modelled, written back, a fixed point across a save, and **absent when the
//! source was absent**.
//!
//! # Why this file exists
//!
//! `ST_WrapText` picks which of the two side channels beside a square/tight/through
//! float remain available to the flow. The model did not carry it; the writer
//! hard-coded `wrapText="bothSides"` onto every side wrap it emitted; and the
//! importer never looked at the attribute. So an authored `left`/`right`/`largest`
//! was destroyed on open and replaced by the default on save, which left layout
//! inferring a side by comparing the float's midpoint to the paragraph's — a guess
//! that flips on a one-twip crossing and makes text jump sideways as a shape is
//! dragged past the column centre.
//!
//! No fixture could have caught any of that: a scan of all 35 readable packages
//! under `fixtures/` — 36 existed, the thirty-sixth being
//! `malformed-truncated.docx`, which is deliberately not a zip — found exactly
//! one `wrapText` anywhere, the `bothSides` in
//! `visual-containment.docx`. `fixtures/generated/wrap-text-sides.docx` was
//! authored for this file and carries all four values, the absence, a
//! `wrapTopAndBottom` the schema gives no `@wrapText`, and — because the writer
//! reaches its wrap emitter from three different anchor paths — a side-wrapped
//! anchored TEXT BOX and a side-wrapped anchored GROUP as well as pictures. The
//! owner's report is about dragging shapes and text boxes, so a picture-only
//! fixture would have left the path that matters most unexercised.
//!
//! What the fixture found, and it is the worse of the two possible answers: **the
//! drop was silent.** With the read mutated out, the compatibility report for a
//! `wrapText="left"` document has zero entries. There is no generic attribute arm
//! to catch it — `report_identity_attributes` covers only `w:p`/`w:r`/`w:tr`/
//! `w:sectPr`, every other `report_attribute` call site is explicit, and
//! `source_element_coverage.rs`'s own module doc records that attribute names are
//! not gated at all. `SKILL` §1 is explicit that direct OOXML only beats a
//! `DOCX -> Editor.bin -> DOCX` converter *if loss is detected*, so a silent drop
//! is a competitive defect and not merely a fidelity one.
//!
//! # The trap this file is written around
//!
//! The sibling `w15:collapsed` guard records asking
//! `xml_text(written).contains("collapsed")` and having it pass because a
//! *heading's title text* held the substring. Every question here is therefore
//! asked through [`side_wraps`], which walks XML **element and attribute names**.
//! It is also what pins the namespace: `@wrapText` is unqualified, so the walk
//! requires the attribute's key to be exactly `wrapText` and a `w:`/`wp:`-prefixed
//! spelling does not count. A local-name parser cannot tell two same-named
//! attributes in different namespaces apart, and this project has guessed an
//! attribute namespace wrong before.
//!
//! The pin is on the **written** side, and the limit is worth stating rather than
//! leaving ambiguous: the importer matches local names, as every attribute reader
//! on that path does, so a hypothetical `w15:wrapText` in a source would also be
//! read. Nothing writes one — the schema's attribute is unqualified and so is
//! Word's — and the fixture is what makes that a checked fact instead of a
//! remembered one. Mutating the writer to emit `wp:wrapText` reddens three tests
//! here, which is what makes the pin load-bearing rather than decorative.
//!
//! # What this file does NOT claim
//!
//! It does not claim the value is **honoured**. Nothing in layout reads it yet:
//! the exclusion arithmetic is `casual-doc-layout`'s lane and is being changed
//! there now. `DrawingAnchor::wrap_side()` is the seam that lane writes against.
//! Making the value available and round-tripped is the half that was losing data,
//! so it is the half that had to come first.
//!
//! # Complexity
//!
//! `O(package bytes)` per test: one import and one or two exports of one small
//! fixture. Test-time only; nothing here is on an edit path.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::export_document_with_retained_parts;
use casual_doc_import::{CompatibilityEntry, ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, InlineNode, WrapMode, WrapSide};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const WRAP_TEXT_SIDES_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/wrap-text-sides.docx");

/// The three wrap elements `ST_WrapText` applies to, by local name.
const SIDE_WRAPS: [&str; 3] = ["wrapSquare", "wrapTight", "wrapThrough"];

/// The attribute, unqualified. Not `w:wrapText`, not `wp:wrapText`.
const WRAP_TEXT: &[u8] = b"wrapText";

/// What the fixture's side wraps say, in document order: six authored values
/// across two wrap element names and three kinds of float, plus one absence.
///
/// Pinned so a fixture edited down to nothing cannot satisfy this file by having
/// nothing to find — the vacuous pass that makes a guard worse than no guard, and
/// exactly the state the whole corpus was in before the fixture existed.
const EXPECTED_SIDE_WRAPS: &[(&str, Option<&str>)] = &[
    ("wrapSquare", Some("bothSides")),
    ("wrapSquare", Some("left")),
    ("wrapSquare", Some("right")),
    ("wrapTight", Some("largest")),
    ("wrapSquare", None),
    // A text box and a group, not pictures. The writer reaches `write_wrap` from
    // three different anchor paths and only the picture one ran against a side
    // wrap before these two existed; the owner's report is about dragging shapes
    // and TEXT BOXES, so the uncovered path was the one that mattered most.
    ("wrapSquare", Some("right")),
    ("wrapSquare", Some("left")),
];

/// The model states the five side-wrapped PICTURES must import with, in document
/// order. The text box and the group have their own list,
/// [`EXPECTED_NON_PICTURES`], because they are different nodes written by
/// different paths.
const EXPECTED_MODEL: &[(WrapMode, Option<WrapSide>)] = &[
    (WrapMode::Square, Some(WrapSide::BothSides)),
    (WrapMode::Square, Some(WrapSide::Left)),
    (WrapMode::Square, Some(WrapSide::Right)),
    (WrapMode::Tight, Some(WrapSide::Largest)),
    (WrapMode::Square, None),
];

/// One import of a package: the model, the media it referenced, the retained
/// parts needed to write it back, and the compatibility findings.
struct Imported {
    document: Document,
    media: BTreeMap<String, Vec<u8>>,
    retained: casual_doc_import::RetainedParts,
    entries: Vec<CompatibilityEntry>,
}

fn import(bytes: &[u8]) -> Imported {
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
    Imported {
        document: import.document,
        media: binary_parts(bytes),
        retained: import.retained_parts,
        entries: import.report.entries,
    }
}

fn write_back(imported: &Imported) -> Vec<u8> {
    export_document_with_retained_parts(&imported.document, &imported.media, &imported.retained)
        .expect("the model is writable")
        .bytes
}

/// The `word/media/*` parts, so a written package keeps the pictures its anchors
/// reference. Without these the drawings would be dropped and this file would be
/// asserting things about an empty body.
fn binary_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let mut parts = BTreeMap::new();
    for name in names {
        if !name.starts_with("word/media/") {
            continue;
        }
        let mut buffer = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut buffer)
            .expect("a readable entry");
        parts.insert(name, buffer);
    }
    parts
}

/// Every side-wrap element in a package, in document order, as a pair of its
/// element local name and its unqualified `@wrapText` if it has one.
///
/// This is the function every question in this file must be asked through, and the
/// reason is the trap in the module doc: a raw-substring search for `"wrapText"`
/// or `"left"` is satisfiable by body text, and this fixture's body text names the
/// sides on purpose so a guard that reaches for a substring fails here rather than
/// in a published claim.
///
/// The attribute test is on `attribute.key.as_ref()`, the name **as written**, not
/// on `local_name()`. That is what pins the namespace: a `w15:wrapText` has the
/// same local name and must not count.
fn side_wraps(bytes: &[u8]) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip package");
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();
    for name in names {
        if !name.ends_with(".xml") {
            continue;
        }
        let mut part = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut part)
            .expect("a readable entry");
        let mut reader = quick_xml::Reader::from_reader(part.as_slice());
        let mut buffer = Vec::new();
        loop {
            let event = reader.read_event_into(&mut buffer);
            match event {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Start(element))
                | Ok(quick_xml::events::Event::Empty(element)) => {
                    let local = element.local_name();
                    let local = String::from_utf8_lossy(local.as_ref()).into_owned();
                    if !SIDE_WRAPS.contains(&local.as_str()) {
                        continue;
                    }
                    let mut value = None;
                    for attribute in element.attributes().with_checks(false).flatten() {
                        if attribute.key.as_ref() == WRAP_TEXT {
                            value = Some(
                                String::from_utf8_lossy(attribute.value.as_ref()).into_owned(),
                            );
                        }
                    }
                    found.push((local, value));
                }
                Ok(_) => {}
                Err(error) => panic!("part `{name}` must be well-formed XML: {error}"),
            }
            buffer.clear();
        }
    }
    found
}

/// Every anchored PICTURE's `(wrap, wrap_text)` pair, in body order.
///
/// Derived by walking the body rather than by indexing, so inserting a paragraph
/// into the fixture cannot silently shift an assertion onto the wrong float.
fn anchored_floats(document: &Document) -> Vec<(WrapMode, Option<WrapSide>)> {
    let mut floats = Vec::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::AnchoredDrawing(drawing) = inline {
                    floats.push((drawing.anchor.wrap, drawing.anchor.wrap_text));
                }
            }
        }
    }
    floats
}

/// Every anchored float that is NOT a picture, as a label and its
/// `(wrap, wrap_text)` pair, in body order.
///
/// A separate walk because these are separate node kinds with separate writer
/// paths, and a single list keyed only by position would hide which one lost its
/// side. An inline (non-anchored) text box or group contributes nothing: there is
/// no `wp:anchor` on it to carry the attribute.
fn anchored_non_pictures(document: &Document) -> Vec<(&'static str, WrapMode, Option<WrapSide>)> {
    let mut floats = Vec::new();
    for block in document.body() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                let found = match inline {
                    InlineNode::TextBox(text_box) => {
                        text_box.anchor.as_ref().map(|anchor| ("text box", anchor))
                    }
                    InlineNode::Group(group) => {
                        group.anchor.as_ref().map(|anchor| ("group", anchor))
                    }
                    _ => None,
                };
                if let Some((label, anchor)) = found {
                    floats.push((label, anchor.wrap, anchor.wrap_text));
                }
            }
        }
    }
    floats
}

/// The two non-picture floats' states, in document order.
const EXPECTED_NON_PICTURES: &[(&str, WrapMode, Option<WrapSide>)] = &[
    ("text box", WrapMode::Square, Some(WrapSide::Right)),
    ("group", WrapMode::Square, Some(WrapSide::Left)),
];

/// The fixture really carries the five discriminating states, on floats that
/// really import as floats.
///
/// Without this every other assertion here could be satisfied by a package with no
/// floats in it, which is the silence the corpus was already in.
#[test]
fn the_fixture_carries_every_wrap_text_state() {
    let source: Vec<(String, Option<String>)> = side_wraps(WRAP_TEXT_SIDES_DOCX);
    let expected: Vec<(String, Option<String>)> = EXPECTED_SIDE_WRAPS
        .iter()
        .map(|(element, value)| ((*element).to_owned(), value.map(str::to_owned)))
        .collect();
    assert_eq!(
        source, expected,
        "the fixture must carry four authored `@wrapText` values across two wrap \
         element names, plus one side wrap with the attribute absent"
    );

    let imported = import(WRAP_TEXT_SIDES_DOCX);
    assert_eq!(
        anchored_floats(&imported.document).len(),
        6,
        "the fixture's six anchored PICTURES must all import as floats — five side \
         wraps plus the `wrapTopAndBottom` that has no side to select"
    );
    assert_eq!(
        anchored_non_pictures(&imported.document).len(),
        2,
        "and its anchored text box and anchored group must import as floats too, or \
         two of the three writer paths go unexercised"
    );
}

/// Import reads all four values and the absence onto the right floats.
///
/// The absence is the load-bearing entry. `None` has to survive as `None` all the
/// way through the model, because the writer keys on it to decide whether to emit
/// the attribute at all; an importer that normalized it to `Some(BothSides)` would
/// make the writer invent markup the author never wrote, and nothing downstream
/// could tell the difference any more.
#[test]
fn import_reads_every_side_and_the_absence() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let floats = anchored_floats(&imported.document);
    assert_eq!(
        &floats[..EXPECTED_MODEL.len()],
        EXPECTED_MODEL,
        "the five side-wrapped floats' `(wrap, wrap_text)` pairs, in document order"
    );
    assert_eq!(
        floats.last().copied(),
        Some((WrapMode::TopAndBottom, None)),
        "and the `wrapTopAndBottom`, which the schema gives no `@wrapText`, must \
         carry no side"
    );
    assert_eq!(
        anchored_non_pictures(&imported.document),
        EXPECTED_NON_PICTURES,
        "and the anchored text box and anchored group must each read their own \
         side: these are different model nodes reached by different writer paths, \
         and a picture-only guard would not notice either losing it"
    );
}

/// `DrawingAnchor::wrap_side()` is the seam the layout lane reads, and it answers
/// Word's absent-attribute default without the caller re-deriving it.
///
/// Asserted separately from the raw field because they are two different
/// guarantees that a single change could silently merge: the field must keep the
/// absence (for the writer) and the accessor must hide it (for layout).
#[test]
fn the_accessor_resolves_the_absent_attribute_to_words_default() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let sides: Vec<WrapSide> = imported
        .document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .flat_map(|paragraph| paragraph.inlines.iter())
        .filter_map(|inline| match inline {
            InlineNode::AnchoredDrawing(drawing) => Some(drawing.anchor.wrap_side()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sides,
        vec![
            WrapSide::BothSides,
            WrapSide::Left,
            WrapSide::Right,
            WrapSide::Largest,
            // Absent in the model; `bothSides` to a consumer.
            WrapSide::BothSides,
            // A `wrapTopAndBottom` has no side channels at all. The accessor still
            // answers, because a total function is easier to use correctly than one
            // that can refuse; the doc comment is where "meaningless here" lives.
            WrapSide::BothSides,
        ],
        "every anchored picture's resolved side, in document order"
    );

    let non_pictures: Vec<WrapSide> = anchored_non_pictures(&imported.document)
        .iter()
        .map(|(_, _, side)| side.unwrap_or(WrapSide::BothSides))
        .collect();
    assert_eq!(
        non_pictures,
        vec![WrapSide::Right, WrapSide::Left],
        "and the text box's and the group's, which layout reads through the same \
         accessor on the same `DrawingAnchor`"
    );
}

/// The load-bearing invariant: an authored `@wrapText` is either written back or
/// **named** by a compatibility finding. Silence fails.
///
/// A disjunction on purpose, and it is not the weaker test — it is the invariant at
/// the altitude it actually holds at (`SKILL`: a guard asserts the guarantee, not
/// the circumstance). Before this change the report branch was not taken either,
/// which is what made the drop silent; now the round-trip branch is. A guard pinned
/// to today's mechanism would redden on a future change that removed nothing.
///
/// This is also the gate the task asked for over the attribute axis.
/// `source_element_coverage.rs` compares element *names* and says so in its own
/// module doc, so it cannot see an attribute vanish; for this attribute, this test
/// is what would.
#[test]
fn wrap_text_survives_the_save_or_is_reported() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let reported = imported.entries.iter().any(|entry| {
        entry.location.attribute.as_deref() == Some("wrapText")
            || entry.feature.ends_with("/@wrapText")
    });

    let written = write_back(&imported);
    let round_tripped = side_wraps(&written)
        .iter()
        .any(|(_, value)| value.is_some());

    assert!(
        round_tripped || reported,
        "`@wrapText` is in the source, is absent from every side wrap in the \
         written package, and no compatibility finding names it — a silent drop, \
         which the engineering priority order forbids outright. Either model it and \
         write it, or report it.\nfindings: {:?}",
        imported
            .entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect::<Vec<_>>()
    );
}

/// The save is a **fixed point**: each float's side comes back identical, in place,
/// and the absent one comes back absent.
///
/// This is the assertion `wrap_text_survives_the_save_or_is_reported` cannot make.
/// That one asks whether the attribute survived *somewhere*; a writer that emitted
/// `wrapText="bothSides"` on all five would satisfy it while having destroyed three
/// authored sides and invented a fourth.
#[test]
fn the_wrap_side_is_a_fixed_point_across_a_save() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let written = write_back(&imported);

    assert_eq!(
        side_wraps(&written),
        side_wraps(WRAP_TEXT_SIDES_DOCX),
        "the written package's side wraps must match the source's element for \
         element and attribute for attribute"
    );

    let reopened = import(&written);
    assert_eq!(
        &anchored_floats(&reopened.document)[..EXPECTED_MODEL.len()],
        EXPECTED_MODEL,
        "and reopening it must give back the same five picture states"
    );
    assert_eq!(
        anchored_non_pictures(&reopened.document),
        EXPECTED_NON_PICTURES,
        "and the same text-box and group states, which travel through the writer's \
         other two anchor paths"
    );
}

/// An absent `@wrapText` stays absent. The hard-coded `bothSides` cannot come back.
///
/// This is the test that refuses a fidelity loss shaped like fidelity. Layout reads
/// the same side either way — `wrap_side()` resolves `None` to `BothSides` — so
/// writing the default would look harmless and would even make a naive "did the
/// attribute survive" check *greener*. What it does is change the document: a
/// package whose author wrote no attribute comes back with one, and the saved file
/// stops being the file that was opened. `SKILL` §12 puts round-trip fidelity above
/// convenience, and the writer that shipped before this change wrote `bothSides`
/// unconditionally.
///
/// Stated as a count over the whole package rather than over one element, so
/// re-introducing the hard-code on `wrapTight`/`wrapThrough` alone is caught too.
#[test]
fn absent_wrap_text_is_not_written_back() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let written = write_back(&imported);

    let source_absences = side_wraps(WRAP_TEXT_SIDES_DOCX)
        .iter()
        .filter(|(_, value)| value.is_none())
        .count();
    assert_eq!(
        source_absences, 1,
        "the fixture must contain exactly one side wrap with no `@wrapText`, or \
         this test is vacuous"
    );

    let written_absences = side_wraps(&written)
        .iter()
        .filter(|(_, value)| value.is_none())
        .count();
    assert_eq!(
        written_absences,
        source_absences,
        "a side wrap whose source carried no `@wrapText` must be written with no \
         `@wrapText`. Writing Word's `bothSides` default there invents markup the \
         author never wrote; it is the exact defect this change removed, and it \
         cannot be allowed back.\nwritten side wraps: {:?}",
        side_wraps(&written)
    );
}

/// A wrap element the schema gives no `@wrapText` must not acquire one.
///
/// `wrapNone` and `wrapTopAndBottom` leave no side channels, so there is nothing
/// for the attribute to select. A writer that pushed it onto every wrap element it
/// emitted would produce markup that fails schema validation in Word while every
/// value-level test here still passed.
#[test]
fn a_wrap_with_no_side_channels_acquires_no_wrap_text() {
    let imported = import(WRAP_TEXT_SIDES_DOCX);
    let written = write_back(&imported);

    let mut offenders = Vec::new();
    let mut archive =
        zip::ZipArchive::new(Cursor::new(written.clone())).expect("the written package is a zip");
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    for name in names {
        if !name.ends_with(".xml") {
            continue;
        }
        let mut part = Vec::new();
        archive
            .by_name(&name)
            .expect("a listed entry")
            .read_to_end(&mut part)
            .expect("a readable entry");
        let mut reader = quick_xml::Reader::from_reader(part.as_slice());
        let mut buffer = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(quick_xml::events::Event::Start(element))
                | Ok(quick_xml::events::Event::Empty(element)) => {
                    let local = element.local_name();
                    let local = String::from_utf8_lossy(local.as_ref()).into_owned();
                    if local != "wrapNone" && local != "wrapTopAndBottom" {
                        continue;
                    }
                    if element
                        .attributes()
                        .with_checks(false)
                        .flatten()
                        .any(|attribute| attribute.key.as_ref() == WRAP_TEXT)
                    {
                        offenders.push(format!("{name}:{local}"));
                    }
                }
                Ok(_) => {}
                Err(error) => panic!("part `{name}` must be well-formed XML: {error}"),
            }
            buffer.clear();
        }
    }
    assert!(
        offenders.is_empty(),
        "`wrapNone`/`wrapTopAndBottom` have no side channels and `ST_WrapText` does \
         not apply to them, but the writer put `@wrapText` on: {offenders:?}"
    );
}
