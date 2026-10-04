// SPDX-License-Identifier: Apache-2.0

//! Round-trip guards: a real `.pptx` in, a written package out, reopened.

use std::collections::BTreeMap;

use casual_doc_package::PackageLimits;
use casual_pres_import::{ImportLimits, import_pptx};

use crate::export_pptx;

#[allow(
    dead_code,
    reason = "the perturbation helpers belong to the import guards"
)]
#[path = "../../casual-pres-import/src/tests/deck.rs"]
mod deck;

/// Imports the fixture, writes it, and reopens the result.
fn round_trip() -> (
    casual_pres_import::ImportedPresentation,
    casual_pres_import::ImportedPresentation,
) {
    let first = import_pptx(
        &deck::deck(),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the fixture imports");
    let written = export_pptx(&first.presentation, &BTreeMap::new()).expect("the deck writes");
    let second = import_pptx(&written, PackageLimits::default(), ImportLimits::default())
        .expect("the written deck reopens");
    (first, second)
}

/// The written package is a package a reader opens, with the deck's order intact.
///
/// Slide ORDER is the assertion that matters: the writer mints a relationship id
/// per slide and writes it into both the relationship part and `p:sldIdLst`, and
/// those two drifting apart is the failure that produces a deck which opens and
/// shows the wrong thing.
#[test]
fn a_written_deck_reopens_with_its_slides_in_order() {
    let (first, second) = round_trip();

    let before: Vec<Option<&str>> = first
        .presentation
        .slides()
        .iter()
        .map(|slide| slide.name.as_deref())
        .collect();
    let after: Vec<Option<&str>> = second
        .presentation
        .slides()
        .iter()
        .map(|slide| slide.name.as_deref())
        .collect();
    assert_eq!(
        before, after,
        "the deck's order and its slide names must survive a write"
    );
    assert_eq!(before.len(), 3, "and all three slides must be there");
}

/// The three tiers and their references survive, so inheritance still resolves
/// after a save.
#[test]
fn a_written_deck_keeps_its_tiers_and_their_references() {
    let (first, second) = round_trip();

    assert_eq!(
        second.presentation.masters().len(),
        first.presentation.masters().len(),
        "every master is written"
    );
    assert_eq!(
        second.presentation.layouts().len(),
        first.presentation.layouts().len(),
        "every layout is written"
    );
    // The references are what make a tier useful: a slide whose layout cannot be
    // resolved inherits nothing, and the model would have refused the deck.
    for slide in second.presentation.slides() {
        let layout = second
            .presentation
            .layout_of(slide)
            .expect("every written slide resolves its layout");
        assert!(
            second.presentation.master_of(layout).is_some(),
            "and every written layout resolves its master"
        );
    }
}

/// The surface, the hidden flag and the placeholder slots survive.
#[test]
fn a_written_deck_keeps_its_surface_slots_and_hidden_flag() {
    let (first, second) = round_trip();

    assert_eq!(
        second.presentation.slide_size(),
        first.presentation.slide_size(),
        "p:sldSz is authoritative and must come back exactly — a guessed surface \
         silently repositions every shape in the deck"
    );

    // `p:sld@show` has INVERTED polarity against the model's `hidden`, so a writer
    // that passed the flag straight through would hide every visible slide.
    let hidden_before: Vec<bool> = first
        .presentation
        .slides()
        .iter()
        .map(|slide| slide.hidden)
        .collect();
    let hidden_after: Vec<bool> = second
        .presentation
        .slides()
        .iter()
        .map(|slide| slide.hidden)
        .collect();
    assert_eq!(hidden_before, hidden_after, "the hidden flag's polarity");
    assert!(
        hidden_before.iter().any(|hidden| *hidden),
        "the fixture must contain a hidden slide, or the polarity check above is \
         vacuous"
    );

    // The slot PAIR is what inheritance resolves on, so both halves must survive.
    let slots_before = first.presentation.slides()[0].shapes.slots();
    let slots_after = second.presentation.slides()[0].shapes.slots();
    assert_eq!(
        slots_before.keys().collect::<Vec<_>>(),
        slots_after.keys().collect::<Vec<_>>(),
        "every (type, idx) placeholder pair survives"
    );
    assert!(!slots_before.is_empty(), "and the fixture fills some");
}

/// The text cascade's tiers survive, which is what a reopened deck needs to
/// resolve a font size at all.
#[test]
fn a_written_deck_keeps_the_text_cascade_tiers() {
    let (first, second) = round_trip();

    assert_eq!(
        second.presentation.masters()[0].text_styles,
        first.presentation.masters()[0].text_styles,
        "p:txStyles — all three tiers, nine levels each"
    );
    assert_eq!(
        second.presentation.default_text_style(),
        first.presentation.default_text_style(),
        "p:defaultTextStyle"
    );
    assert!(
        !first.presentation.masters()[0].text_styles.is_empty(),
        "the fixture must state tiers, or the comparison above is vacuous"
    );

    // And the fold still answers on the reopened deck, which is the end-to-end
    // statement: tiers that survive a save are tiers that still resolve.
    let slide = &second.presentation.slides()[1];
    let body = slide
        .shapes
        .slot(casual_pres_model::PlaceholderKind::Object, 1)
        .expect("the content slot survives");
    let resolved = second.presentation.text_cascade(slide, body).resolve(0);
    assert_eq!(
        resolved.character.size_hundredths_point,
        Some(2_800),
        "28pt still resolves through the written master's body tier"
    );
}

/// A shape's geometry, its authored preset token and its custom path survive.
#[test]
fn a_written_deck_keeps_shape_geometry_and_authored_presets() {
    use casual_doc_model::v1::GroupChild;

    let (first, second) = round_trip();

    let shapes_of = |imported: &casual_pres_import::ImportedPresentation, index: usize| {
        imported.presentation.slides()[index]
            .shapes
            .children
            .iter()
            .filter_map(|node| match &node.content {
                GroupChild::Shape(shape) => Some((
                    shape.geometry,
                    shape.preset.clone(),
                    shape.path.clone(),
                    // The adjustments are what make a preset the SHAPE it is: a
                    // `wedgeRoundRectCallout` with its two `a:gd` guides dropped is
                    // still that preset, drawn at the table's defaults — a
                    // different outline that looks deliberate.
                    shape.adjustments.clone(),
                    shape.offset,
                    shape.extent,
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    // Slide 10 carries the authored `wedgeRoundRectCallout` with two adjustments
    // AND a single-subpath `a:custGeom`. Rewriting either as `prst="rect"` is the
    // one place a save DESTROYS data rather than mis-drawing it.
    let before = shapes_of(&first, 2);
    let after = shapes_of(&second, 2);
    assert_eq!(
        before, after,
        "geometry, preset token, adjustments, path, offset, extent"
    );
    assert!(
        before
            .iter()
            .any(|(_, preset, _, _, _, _)| preset.is_some()),
        "the fixture must carry an authored preset token, or this is vacuous"
    );
    assert!(
        before
            .iter()
            .any(|(_, _, _, adjustments, _, _)| !adjustments.is_empty()),
        "and an adjustment, or the guide check above is vacuous"
    );
    assert!(
        before.iter().any(|(_, _, path, _, _, _)| path.is_some()),
        "and a custom path"
    );
}

/// Every part a VALIDATING reader needs is present, including the references this
/// engine's own importer does not happen to use.
///
/// Written against the package bytes rather than a reopen, and that distinction is
/// the point: our importer resolves a layout's master top-down through the
/// master's `p:sldLayoutIdLst`, so dropping the layout's own `slideMaster`
/// relationship changes nothing it reads — a round-trip guard cannot see it. The
/// relationship is required by `CT_SlideLayout` all the same, and PowerPoint
/// refuses a layout without one. So the assertion is on what was written.
#[test]
fn a_written_package_carries_the_parts_and_references_a_reader_requires() {
    let imported = import_pptx(
        &deck::deck(),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the fixture imports");
    let written = export_pptx(&imported.presentation, &BTreeMap::new()).expect("the deck writes");

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(written)).expect("the output is a ZIP");
    let names: Vec<String> = (0..archive.len())
        .map(|index| archive.by_index(index).expect("entry").name().to_owned())
        .collect();
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        "ppt/slideLayouts/slideLayout1.xml",
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        "ppt/slides/slide1.xml",
        "ppt/slides/_rels/slide1.xml.rels",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "the package must contain {required}; it has {names:?}"
        );
    }

    let read = |archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str| {
        use std::io::Read as _;
        let mut part = archive.by_name(name).expect("the part exists");
        let mut body = String::new();
        part.read_to_string(&mut body).expect("the part is UTF-8");
        body
    };

    // Each layout names its master, and each slide names its layout. These are the
    // references a reader walks DOWN, which our importer does not need and a
    // conforming consumer does.
    let layout_rels = read(&mut archive, "ppt/slideLayouts/_rels/slideLayout1.xml.rels");
    assert!(
        layout_rels.contains("relationships/slideMaster"),
        "a layout must name its master: {layout_rels}"
    );
    let slide_rels = read(&mut archive, "ppt/slides/_rels/slide1.xml.rels");
    assert!(
        slide_rels.contains("relationships/slideLayout"),
        "a slide must name its layout: {slide_rels}"
    );
    // And the targets are RELATIVE. An absolute target resolves for some readers
    // and not others, so the relative form is the only one written.
    assert!(
        slide_rels.contains(r#"Target="../slideLayouts/"#),
        "a target must be relative to the source part's folder: {slide_rels}"
    );

    // Every part declares a content type. A slide typed `application/xml` opens as
    // nothing, which is the failure a single `xml` default would produce.
    let types = read(&mut archive, "[Content_Types].xml");
    for part in [
        "/ppt/presentation.xml",
        "/ppt/slideMasters/slideMaster1.xml",
        "/ppt/slideLayouts/slideLayout1.xml",
        "/ppt/slides/slide1.xml",
    ] {
        assert!(
            types.contains(&format!(r#"PartName="{part}""#)),
            "{part} must have a content-type override: {types}"
        );
    }
}
