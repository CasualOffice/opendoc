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

/// Retention: the theme part a save used to destroy now survives byte-for-byte.
///
/// The fixture carries `ppt/theme/theme1.xml`, which nothing reads — it is in the
/// import report as a whole unconsumed part. Before retention, writing the deck
/// dropped it, so a save turned a themed deck into an unthemed one and the loss
/// report could not say `preserved` about anything. This is that gap closed, and
/// the assertion is on the BYTES rather than on a reopen, because a reopen cannot
/// see a part no reader of ours consults.
#[test]
fn a_retained_part_survives_a_save_byte_for_byte() {
    use std::io::Read as _;

    let original = deck::deck();
    let imported = import_pptx(&original, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    let retained = crate::RetainedParts::from_package(&original).expect("the original reads");

    let theme_before = retained
        .parts
        .get("ppt/theme/theme1.xml")
        .expect("the fixture carries a theme part")
        .clone();
    assert!(
        !theme_before.is_empty(),
        "and it is not empty, or this guard is vacuous"
    );

    // Without retention the part is gone. That is the control: it is what makes the
    // retained case a difference rather than a coincidence.
    let bare = export_pptx(&imported.presentation, &BTreeMap::new()).expect("writes");
    let mut bare_archive =
        zip::ZipArchive::new(std::io::Cursor::new(bare)).expect("the output is a ZIP");
    assert!(
        bare_archive.by_name("ppt/theme/theme1.xml").is_err(),
        "export_pptx retains nothing, so the theme must be absent — otherwise the \
         retained case below proves nothing"
    );

    let written = crate::export_pptx_retaining(&imported.presentation, &BTreeMap::new(), &retained)
        .expect("the retaining write succeeds");
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(written)).expect("the output is a ZIP");
    let mut theme_after = Vec::new();
    archive
        .by_name("ppt/theme/theme1.xml")
        .expect("the theme part is carried through")
        .read_to_end(&mut theme_after)
        .expect("it reads back");
    assert_eq!(
        theme_after, theme_before,
        "a retained part must survive BYTE-for-byte; a re-serialized one is not a \
         verbatim floor and could not license a preserved claim"
    );
}

/// A regenerated part WINS over a retained one, so an edit stays visible.
///
/// This is the direction that matters and the one that is easy to get backwards:
/// if retention overrode regeneration, every change a user made would be silently
/// replaced by the original bytes — a worse failure than the loss retention fixes,
/// and one that looks like the editor doing nothing.
#[test]
fn a_regenerated_part_beats_a_retained_one() {
    use std::io::Read as _;

    let original = deck::deck();
    let imported = import_pptx(&original, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    let retained = crate::RetainedParts::from_package(&original).expect("the original reads");
    assert!(
        retained.parts.contains_key("ppt/slides/slide1.xml"),
        "the original carries the slide this writer also regenerates"
    );

    let written = crate::export_pptx_retaining(&imported.presentation, &BTreeMap::new(), &retained)
        .expect("writes");
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(written)).expect("the output is a ZIP");
    let mut slide = String::new();
    archive
        .by_name("ppt/slides/slide1.xml")
        .expect("slide 1 is written")
        .read_to_string(&mut slide)
        .expect("it reads back");

    // The regenerated form is distinguishable from the original: this writer emits
    // `p:cNvPr` ids from tree POSITION and the fixture's are authored, and it
    // writes `a:bodyPr`'s insets unconditionally where the fixture writes
    // `<a:bodyPr/>`. Either marker alone would be fragile; both together say the
    // bytes came from the model.
    assert!(
        slide.contains("lIns="),
        "the slide must be the REGENERATED one, which always states its insets: \
         {slide}"
    );
    let original_slide = String::from_utf8(retained.parts["ppt/slides/slide1.xml"].clone())
        .expect("the original slide is UTF-8");
    assert_ne!(
        slide, original_slide,
        "and it must not be the retained copy"
    );
}

/// `[Content_Types].xml` is rebuilt, never retained.
///
/// A retained copy declares the ORIGINAL's parts. This writer renames parts by
/// position, so a retained content-types part can leave a written slide with no
/// declared type — and a slide with no type opens as nothing. Worth its own guard
/// because the retention loop's skip for it is one line and reads like an
/// optimisation.
#[test]
fn the_content_types_part_is_rebuilt_rather_than_retained() {
    use std::io::Read as _;

    let original = deck::deck();
    let imported = import_pptx(&original, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    let retained = crate::RetainedParts::from_package(&original).expect("the original reads");
    let written = crate::export_pptx_retaining(&imported.presentation, &BTreeMap::new(), &retained)
        .expect("writes");

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(written)).expect("the output is a ZIP");
    let mut types = String::new();
    archive
        .by_name("[Content_Types].xml")
        .expect("the content types part is written")
        .read_to_string(&mut types)
        .expect("it reads back");

    // The fixture names its slides 1, 2 and 10; this writer names them 1, 2 and 3.
    // So a retained content-types part would declare `/ppt/slides/slide10.xml` and
    // say nothing about `slide3.xml` — the exact failure.
    assert!(
        types.contains(r#"PartName="/ppt/slides/slide3.xml""#),
        "every WRITTEN slide must be declared: {types}"
    );
    assert!(
        !types.contains(r#"PartName="/ppt/slides/slide10.xml""#),
        "and no part the writer did not produce: {types}"
    );
}

/// A retained deck reopens, so retention does not produce a package a reader
/// rejects.
#[test]
fn a_retained_deck_still_reopens() {
    let original = deck::deck();
    let imported = import_pptx(&original, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    let retained = crate::RetainedParts::from_package(&original).expect("the original reads");
    let written = crate::export_pptx_retaining(&imported.presentation, &BTreeMap::new(), &retained)
        .expect("writes");

    let reopened = import_pptx(&written, PackageLimits::default(), ImportLimits::default())
        .expect("a retained deck must still be a readable deck");
    assert_eq!(
        reopened.presentation.slides().len(),
        imported.presentation.slides().len(),
        "with the same slides"
    );
    assert_eq!(
        reopened.presentation.slide_size(),
        imported.presentation.slide_size(),
        "and the same surface"
    );
}

/// A picture keeps its orientation, its crop, its opacity — and keeps its image
/// when it is inside a group.
///
/// # Why the fixture has to be perturbed
///
/// The plain fixture's picture is unrotated, unflipped, uncropped and opaque, and
/// there is no picture inside its group. So an earlier draft of this writer passed
/// `None` for a picture's rotation and `false` for its vertical flip, and wrote a
/// nested picture with NO `a:blip` at all — and every round-trip guard stayed
/// green, because the fixture could not tell. Each of those is a silent loss: a
/// rotated logo comes back upright, a cropped one comes back showing what its
/// author cut off, a watermark at 30% comes back opaque and covers the slide, and
/// a picture in a group comes back as an empty box.
///
/// So this authors all four and asserts each one separately.
#[test]
fn a_picture_keeps_its_orientation_crop_opacity_and_its_image_in_a_group() {
    use casual_doc_model::v1::GroupChild;

    let slide_two = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slides/slide2.xml")
            .expect("the fixture carries slide2.xml")
            .1,
    )
    .expect("the slide part is UTF-8");

    // A rotated, doubly-flipped, cropped, 30%-opaque picture.
    let plain_pic = r#"<p:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
<p:spPr><a:xfrm><a:off x="9144000" y="457200"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#;
    let rich_pic = r#"<p:blipFill><a:blip r:embed="rIdImage"><a:alphaModFix amt="30000"/></a:blip><a:srcRect l="5000" t="6000" r="7000" b="8000"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
<p:spPr><a:xfrm rot="1800000" flipH="1" flipV="1"><a:off x="9144000" y="457200"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#;
    assert!(slide_two.contains(plain_pic), "the fixture's picture moved");
    let slide_two = slide_two.replace(plain_pic, rich_pic);

    // And a second picture INSIDE the group, which is the arm that had no
    // relationship context.
    let group_end = "</p:sp>\n</p:grpSp>";
    let nested = r#"</p:sp>
<p:pic>
<p:nvPicPr><p:cNvPr id="8" name="Nested Mark"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
<p:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
<p:spPr><a:xfrm><a:off x="100000" y="100000"/><a:ext cx="200000" cy="200000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
</p:pic>
</p:grpSp>"#;
    assert_eq!(
        slide_two.matches(group_end).count(),
        1,
        "one group close to anchor on"
    );
    let slide_two = slide_two.replace(group_end, nested);

    let mut parts = deck::deck_parts();
    parts
        .iter_mut()
        .find(|(name, _)| name == "ppt/slides/slide2.xml")
        .expect("the part exists")
        .1 = slide_two.into_bytes();
    let original = deck::build_pptx(&parts);

    let imported = import_pptx(&original, PackageLimits::default(), ImportLimits::default())
        .expect("the perturbed deck imports");

    // The media bytes the writer needs, taken from the original rather than
    // invented — the writer refuses to emit a blip for media it was not given.
    let retained = crate::RetainedParts::from_package(&original).expect("the original reads");
    let media: BTreeMap<String, Vec<u8>> = retained
        .parts
        .iter()
        .filter(|(name, _)| name.starts_with("ppt/media/"))
        .map(|(name, bytes)| (name.clone(), bytes.clone()))
        .collect();
    assert_eq!(media.len(), 1, "the fixture carries one image");

    let written = export_pptx(&imported.presentation, &media).expect("the deck writes");
    let reopened = import_pptx(&written, PackageLimits::default(), ImportLimits::default())
        .expect("the written deck reopens");

    let pictures_of = |deck: &casual_pres_import::ImportedPresentation| {
        let mut found = Vec::new();
        let slide = &deck.presentation.slides()[1];
        for node in &slide.shapes.children {
            match &node.content {
                GroupChild::Picture(picture) => found.push(picture.clone()),
                GroupChild::Group(group) => {
                    for child in &group.children {
                        if let GroupChild::Picture(picture) = child {
                            found.push(picture.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        found
    };

    let before = pictures_of(&imported);
    let after = pictures_of(&reopened);
    assert_eq!(before.len(), 2, "a top-level picture and a nested one");
    assert_eq!(after.len(), 2, "and both must be written");

    // The top-level picture, property by property, because a single struct
    // comparison would not say WHICH of five values was dropped.
    let (first_before, first_after) = (&before[0], &after[0]);
    assert_eq!(
        first_before.rotation,
        Some(1_800_000),
        "the fixture states a 30-degree rotation, or this row is vacuous"
    );
    assert_eq!(first_after.rotation, first_before.rotation, "a:xfrm@rot");
    assert!(first_before.flip_h && first_before.flip_v, "and both flips");
    assert_eq!(first_after.flip_h, first_before.flip_h, "@flipH");
    assert_eq!(first_after.flip_v, first_before.flip_v, "@flipV");
    // `a:srcRect` and `a:alphaModFix` are NOT read by the presentation importer —
    // both are named in its "does not read" list. So the round trip cannot carry
    // them, and pinning that is more useful than asserting nothing: the day the
    // importer learns either, this row fails and says so, and the writer already
    // handles both. Asserting `is_some()` here instead would have the writer
    // appear verified by a path that cannot reach it.
    assert_eq!(
        first_before.crop, None,
        "a:srcRect is unread on the presentation path; when the importer gains it, \
         turn this into a round-trip assertion — the writer emits it already"
    );
    assert_eq!(first_before.opacity, None, "same for a:alphaModFix");
    assert_eq!(first_after.crop, first_before.crop);
    assert_eq!(first_after.opacity, first_before.opacity);

    // The nested picture kept its image. `media` resolves through the model's own
    // table, so this can only hold if the nested writer had the relationship
    // context — which is the whole fix.
    assert_eq!(
        after[1].media, before[1].media,
        "a picture inside a group must keep its a:blip, not come back as an empty \
         box"
    );
    assert!(
        reopened
            .presentation
            .definitions()
            .media
            .get(&after[1].media)
            .is_some(),
        "and that media must resolve in the written deck's own definitions"
    );
}

/// The crop and opacity the round trip cannot reach are exercised directly.
///
/// `a:srcRect` and `a:alphaModFix` are unread by the presentation importer, so the
/// guard above can only pin that gap — which would leave this writer's two
/// branches shipped and never run, the "modelled but unverified" shape `SKILL` §9.4
/// is about. A `GroupPicture` is a plain public struct, so the writer is called
/// with one built here instead, and the gap stays the importer's rather than
/// becoming a hole in the writer.
#[test]
fn a_cropped_translucent_picture_writes_its_source_rect_and_alpha() {
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        CropRect, Definitions, Extent, GroupPicture, MediaId, MediaReference, PointEmu,
    };

    use crate::opc::Relationships;
    use crate::shapes::{ShapeContext, blip_fill_xml};

    let media_id = MediaId::new(NodeId::from_parts(1, 900).expect("non-zero"));
    let mut definitions = Definitions::default();
    definitions.media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId9".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "ppt/media/image1.png".to_owned(),
        },
    );
    let bytes: BTreeMap<String, Vec<u8>> = [("ppt/media/image1.png".to_owned(), b"PNG".to_vec())]
        .into_iter()
        .collect();

    let picture = GroupPicture {
        id: NodeId::from_parts(1, 7).expect("non-zero"),
        media: media_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        descr: None,
        crop: Some(CropRect {
            left: 5_000,
            top: 0,
            right: 7_000,
            bottom: 8_000,
        }),
        opacity: Some(30_000),
        hyperlink: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    };

    let context = ShapeContext {
        definitions: &definitions,
        media: &bytes,
        part_name: "ppt/slides/slide1.xml",
    };
    let mut rels = Relationships::default();
    let xml = blip_fill_xml(&picture, &context, &mut rels);

    assert!(
        xml.contains(r#"<a:alphaModFix amt="30000"/>"#),
        "a 30% picture must carry its alpha, or a watermark comes back opaque and \
         covers the slide: {xml}"
    );
    // Each stated edge, and NOT the zero one: an omitted edge and a zero edge are
    // the same thing, so writing `t="0"` would be a diff in every package.
    assert!(
        xml.contains(r#"<a:srcRect l="5000" r="7000" b="8000"/>"#),
        "the crop's stated edges, with the zero edge omitted: {xml}"
    );
    // The blip resolved through the model's own table rather than by guessing,
    // which is what makes a multi-picture deck work.
    assert!(
        xml.contains(r#"<a:blip r:embed="rId1""#),
        "the blip takes the id this writer minted: {xml}"
    );

    // And a picture whose bytes the caller did NOT supply gets no blip at all,
    // rather than a relationship pointing at a part the package lacks.
    let empty = BTreeMap::new();
    let no_bytes = ShapeContext {
        definitions: &definitions,
        media: &empty,
        part_name: "ppt/slides/slide1.xml",
    };
    let mut rels = Relationships::default();
    let xml = blip_fill_xml(&picture, &no_bytes, &mut rels);
    assert!(
        !xml.contains("a:blip"),
        "no bytes means no blip — a dangling relationship is a package a reader \
         refuses: {xml}"
    );
    assert!(
        rels.is_empty(),
        "and no relationship is minted for an image that was not written"
    );
}

/// A ZIP directory entry is not carried through as a zero-byte part.
///
/// Real `.pptx` files from some producers contain directory entries; OPC has no
/// directories, and a zero-byte part at `ppt/` is a package some readers reject.
/// The fixture builder writes no directory entries, so the skip in
/// `RetainedParts::from_package` is unexercised by every other guard here — which
/// is why this one crafts a container that has one.
#[test]
fn a_zip_directory_entry_is_not_retained_as_a_part() {
    use std::io::Write as _;

    use zip::write::SimpleFileOptions;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer
        .add_directory("ppt/", options)
        .expect("a directory entry is added");
    writer
        .start_file("ppt/theme/theme1.xml", options)
        .expect("a real part is added");
    writer.write_all(b"<a:theme/>").expect("its bytes");
    let container = writer.finish().expect("the container closes").into_inner();

    let retained = crate::RetainedParts::from_package(&container).expect("it reads");
    assert!(
        retained.parts.contains_key("ppt/theme/theme1.xml"),
        "the real part is retained: {:?}",
        retained.parts.keys().collect::<Vec<_>>()
    );
    assert!(
        !retained.parts.contains_key("ppt/"),
        "and the directory entry is NOT, because OPC has no directories and a \
         zero-byte part at that name is a package some readers reject: {:?}",
        retained.parts.keys().collect::<Vec<_>>()
    );
}

/// A table survives a save: the frame, the grid, the row heights, the cells'
/// properties and BOTH merge encodings.
///
/// The failure this is really about is the element NAME. A table's frame is a
/// `GroupChild::Shape` in the model, so a writer that followed the enum would
/// write a `p:sp` — the deck would still open, every geometry guard would still
/// pass, and every table in it would be gone. So the first assertion is that the
/// reopened deck has a table at all, and the rest is that it is the same one.
#[test]
fn a_written_deck_keeps_its_table_its_merges_and_its_style_guid() {
    let (first, second) = round_trip();

    let table_of = |imported: &casual_pres_import::ImportedPresentation| {
        imported
            .presentation
            .slides()
            .get(2)
            .and_then(|slide| {
                slide
                    .shapes
                    .children
                    .iter()
                    .find_map(|node| node.table.clone())
            })
            .expect("slide 10 carries a table")
    };
    let before = table_of(&first);
    let after = table_of(&second);

    // One comparison, not twenty: the model is `Eq`, so the whole table —
    // properties, grid, every row height, every cell's merge roles, margins,
    // anchor, fill, borders and text — is one assertion that cannot be satisfied
    // by a writer that got any field wrong.
    assert_eq!(
        before.properties, after.properties,
        "a:tblPr's six flags, @rtl and the braced style GUID all survive"
    );
    assert_eq!(
        before.grid, after.grid,
        "every a:gridCol@w survives, in order"
    );
    assert_eq!(
        before.rows.len(),
        after.rows.len(),
        "every a:tr is written back"
    );
    for (index, (before_row, after_row)) in before.rows.iter().zip(&after.rows).enumerate() {
        assert_eq!(
            before_row.height_emu, after_row.height_emu,
            "row {index} keeps its a:tr@h"
        );
        let before_roles: Vec<_> = before_row
            .cells
            .iter()
            .map(|cell| (cell.horizontal, cell.vertical))
            .collect();
        let after_roles: Vec<_> = after_row
            .cells
            .iter()
            .map(|cell| (cell.horizontal, cell.vertical))
            .collect();
        assert_eq!(
            before_roles, after_roles,
            "row {index}: an origin writes its @gridSpan/@rowSpan and a covered \
             cell writes its @hMerge/@vMerge — the covered cell must still be \
             WRITTEN, or the row is a column short and the next cell shifts"
        );
        let before_text: Vec<String> = before_row
            .cells
            .iter()
            .map(|cell| {
                cell.text
                    .as_ref()
                    .map(casual_pres_model::TextBody::plain_text)
                    .unwrap_or_default()
            })
            .collect();
        let after_text: Vec<String> = after_row
            .cells
            .iter()
            .map(|cell| {
                cell.text
                    .as_ref()
                    .map(casual_pres_model::TextBody::plain_text)
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(
            before_text, after_text,
            "row {index}: each cell's a:txBody comes back on the cell it was on"
        );
    }

    // The origin cell's `a:tcPr`, field by field, because this is where an edge
    // written under the wrong tag name would land.
    let before_cell = &before.rows[0].cells[0].properties;
    let after_cell = &after.rows[0].cells[0].properties;
    assert_eq!(
        (
            before_cell.margin_left_emu,
            before_cell.margin_right_emu,
            before_cell.margin_top_emu,
            before_cell.margin_bottom_emu,
            before_cell.anchor,
        ),
        (
            after_cell.margin_left_emu,
            after_cell.margin_right_emu,
            after_cell.margin_top_emu,
            after_cell.margin_bottom_emu,
            after_cell.anchor,
        ),
        "the four EMU margins and the anchor survive"
    );
    assert_eq!(
        before_cell.fill, after_cell.fill,
        "the cell's a:solidFill survives"
    );
    assert_eq!(
        (
            &before_cell.border_left,
            &before_cell.border_right,
            &before_cell.border_top,
            &before_cell.border_bottom,
        ),
        (
            &after_cell.border_left,
            &after_cell.border_right,
            &after_cell.border_top,
            &after_cell.border_bottom,
        ),
        "a:lnL and a:lnB come back on the SAME edges, and a:lnR/a:lnT stay \
         absent: the fixture's two edges have different widths and different \
         colours, so a tag written in the wrong slot cannot pass"
    );
}

/// `ppt/tableStyles.xml` is written, declared and related, so the GUID a table
/// states still joins after a save.
///
/// Three things have to agree for that: the part exists, `[Content_Types].xml`
/// declares its type, and the presentation part's relationships carry a
/// `tableStyles` entry — the part is reached by TYPE, so a part written without
/// the relationship is a part no reader finds.
#[test]
fn a_written_deck_relates_and_declares_its_table_styles_part() {
    let (first, second) = round_trip();

    let before = first.presentation.table_styles();
    let after = second.presentation.table_styles();
    assert_eq!(
        before.default_style_id, after.default_style_id,
        "a:tblStyleLst@def survives, braces included"
    );
    let ids: Vec<&str> = after.styles.iter().map(|style| style.id.as_str()).collect();
    let expected: Vec<&str> = before
        .styles
        .iter()
        .map(|style| style.id.as_str())
        .collect();
    assert_eq!(
        ids, expected,
        "every a:tblStyle entry comes back, in order — a self-closing entry must \
         not swallow the next one on the way back in either"
    );
    assert!(
        !after.styles.is_empty(),
        "the fixture carries two entries, so an empty list here would mean the \
         part was written but not read back"
    );

    // And the table still resolves to the entry the file names, which is the
    // whole point of writing the part at all.
    let table = second
        .presentation
        .slides()
        .get(2)
        .and_then(|slide| {
            slide
                .shapes
                .children
                .iter()
                .find_map(|node| node.table.clone())
        })
        .expect("the reopened deck carries the table");
    let guid = table
        .properties
        .style_id
        .as_deref()
        .expect("the table states a style GUID");
    assert_eq!(
        after.style(guid).and_then(|style| style.name.as_deref()),
        Some("Medium Style 2 - Accent 1"),
        "the GUID still joins to the entry the FILE names after a round trip, and \
         not to the part's @def — which names a different style on purpose"
    );
    assert_ne!(
        Some(guid),
        after.default_style_id.as_deref(),
        "the fixture keeps the two disagreeing, so resolving through @def cannot pass"
    );
}

/// A saturation modifier survives a save, which it could not before the shared
/// model carried one.
///
/// Asserted as a DIFFERENCE between the two halves of the round trip rather than
/// against a literal: a reader that threw the modifier away on the way in and a
/// writer that dropped it on the way out are two different bugs, and comparing the
/// reopened deck with the original catches either one while a literal would have to
/// be re-derived whenever the fixture's base colour changes.
///
/// # What this does NOT prove, measured rather than assumed
///
/// It does not exercise the writer's `a:satMod` child. An `a:schemeClr` on a slide
/// is FOLDED at import — the model has no scheme-slot variant, so the run carries
/// a concrete `StyleColor::Fixed` — and the writer emits a literal `a:srgbClr`.
/// `transform_children` is reached only through `StyleColor::Placeholder`, which in
/// a deck means an `a:phClr` inside the theme's style matrix, and the theme part is
/// RETAINED byte-for-byte rather than regenerated. So the writer's arm is covered
/// by `a_phclr_carries_its_saturation_modifier` below instead, and this guard was
/// first written claiming the transform was re-emitted here. It is not. Deleting
/// the writer's arm leaves this test GREEN, which is exactly why the other one
/// exists.
#[test]
fn a_saturation_modifier_survives_a_save() {
    let (first, second) = round_trip();

    /// The themed run colour on the deck's THIRD slide, which is the one carrying
    /// the modifier. The deck's first themed run is slide 1's title, an
    /// `a:srgbClr` with no transform at all — so a search over every slide finds
    /// a colour this guard says nothing about.
    fn themed_run_fill(
        presentation: &casual_pres_model::Presentation,
    ) -> Option<casual_doc_model::v1::StyleColor> {
        presentation
            .slides()
            .get(2)?
            .shapes
            .children
            .iter()
            .filter_map(|node| node.text.as_ref())
            .flat_map(|body| body.paragraphs.iter())
            .flat_map(|paragraph| paragraph.runs.iter())
            .find_map(|run| run.properties().and_then(|properties| properties.fill))
    }

    let before = themed_run_fill(&first.presentation).expect("the fixture carries a themed run");
    let after = themed_run_fill(&second.presentation).expect("so does the reopened deck");
    assert_eq!(
        after, before,
        "a:satMod is read, folded and re-emitted, so one save does not change the \
         colour and two saves do not compound it"
    );

    // And it is actually MODULATED, not merely stable. A reader and a writer that
    // both dropped the modifier agree with each other perfectly, so the equality
    // above passes for exactly the bug this guard exists to catch — the fixture's
    // base is a:accent5 = #5B9BD5 = (91, 155, 213), and 155% saturation about its
    // lightness of 152 gives (57, 157, 247).
    assert_eq!(
        after,
        casual_doc_model::v1::StyleColor::Fixed(casual_doc_model::v1::Rgba {
            r: 57,
            g: 157,
            b: 247,
            a: 102
        }),
        "the saturation modifier is applied, not just carried"
    );
}

/// An `a:phClr`'s saturation modifier is written back.
///
/// Directly on the writer, because nothing in a deck ROUND-TRIPS one: the only
/// `a:phClr` carriers are the theme's style matrix entries, and the theme part is
/// retained byte-for-byte rather than regenerated, so `a_saturation_modifier_survives_a_save`
/// stays green with this arm deleted. The arm is still correct and still needed —
/// the moment anything regenerates a construct holding a formal-parameter colour,
/// a dropped modifier is a silent loss — and a guard that cannot be driven red is
/// worth less than this one, which can.
#[test]
fn a_phclr_carries_its_saturation_modifier() {
    use casual_doc_model::v1::{ColorTransform, StyleColor};

    let written = crate::shapes::style_color_xml(&StyleColor::Placeholder(ColorTransform {
        sat_mod: Some(155_000),
        lum_mod: Some(110_000),
        ..ColorTransform::default()
    }));
    // Both modifiers, so the assertion cannot be satisfied by a writer that emits
    // one child and calls it a day — and the full element, so a writer that
    // emitted the children outside the `a:schemeClr` fails too.
    assert_eq!(
        written,
        r#"<a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="155000"/></a:schemeClr>"#,
        "a:phClr keeps every modifier the model carries"
    );
}
