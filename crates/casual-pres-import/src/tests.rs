// SPDX-License-Identifier: Apache-2.0

//! End-to-end guards, driven from a real `.pptx` package rather than from
//! hand-built XML strings.
//!
//! Every guard here was written, then driven **red** by mutating the production
//! code to reintroduce the bug it is about, then restored and confirmed green.
//! The mutation and its verbatim failure output are recorded in the commit
//! message. A test that passes on arrival proves nothing (`SKILL` §4).
//!
//! Counts are **derived from the fixture**, never hand-written: a floor asserted
//! at a number someone typed is a guard that fails for reasons unrelated to the
//! code.

mod deck;
// Own line (anti-conflict): the object-level guards (`a:srcRect`, `a:hlinkClick`,
// a connector's end bindings), split out at the file ceiling.
mod objects;
// Own line (anti-conflict): the table guards, split out at the file ceiling.
mod table;
// Own line (anti-conflict): the theme guards, split out at the file ceiling.
mod theme;

use casual_doc_loss::{ModelOutcome, RetentionOutcome};
use casual_doc_model::v1::{GroupChild, ShapeGeometry, ShapePathCommand};
use casual_doc_package::PackageLimits;
use casual_pres_model::{
    PlaceholderKind, SlideSizeKind, TextAutoFit, TextBullet, TextRun, TextSpacing,
};

use crate::{ImportError, ImportLimits, ImportedPresentation, import_pptx};

/// Imports the fixture deck, failing the test with the real error if it refuses.
fn import_fixture() -> ImportedPresentation {
    import_pptx(
        &deck::deck(),
        PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the fixture deck imports")
}

/// Every feature identifier in the report, for the loss guards.
fn features(imported: &ImportedPresentation) -> Vec<&str> {
    imported
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect()
}

/// A slide's title text, through the shape tree's own title lookup.
fn title_text(slide: &casual_pres_model::Slide) -> String {
    slide
        .shapes
        .title()
        .and_then(|node| node.text.as_ref())
        .map(casual_pres_model::TextBody::plain_text)
        .unwrap_or_default()
}

/// A real package opens: the container, the OPC graph, the three tiers and the
/// surface all resolve, and the deck passes the model's own validation.
///
/// The slide and master counts are **derived** by counting the fixture's own
/// `p:sldId`/`p:sldMasterId` entries, so the guard cannot drift from the fixture.
#[test]
fn a_real_package_opens_with_every_tier_resolved() {
    let imported = import_fixture();
    let presentation = &imported.presentation;

    let parts = deck::deck_parts();
    let presentation_xml = parts
        .iter()
        .find(|(name, _)| name == "ppt/presentation.xml")
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .expect("the fixture has a presentation part");
    let declared_slides = presentation_xml.matches("<p:sldId ").count();
    let declared_masters = presentation_xml.matches("<p:sldMasterId ").count();
    assert!(
        declared_slides > 1 && declared_masters >= 1,
        "the fixture must declare more than one slide for the order guard to mean \
         anything; it declares {declared_slides}"
    );

    assert_eq!(presentation.slides().len(), declared_slides);
    assert_eq!(presentation.masters().len(), declared_masters);
    assert_eq!(
        presentation.slide_size().width_emu,
        12_192_000,
        "p:sldSz@cx is EMU and is carried verbatim"
    );
    assert_eq!(presentation.slide_size().height_emu, 6_858_000);
    assert_eq!(presentation.slide_size().kind, SlideSizeKind::Screen16x9);

    // Every layout resolves to a master and every slide to a layout. Derived
    // through the one-pass resolver rather than `layout_of` per slide, which is
    // the O(slides x layouts) shape the model documents against.
    assert!(
        presentation
            .resolve_layout_indices()
            .iter()
            .all(Option::is_some),
        "every slide's layout must resolve"
    );
    for layout in presentation.layouts() {
        assert!(
            presentation.master_of(layout).is_some(),
            "every layout's master must resolve"
        );
    }
}

/// Slide order comes from `p:sldIdLst`, not from the slide part names.
///
/// The fixture's parts are `slide1`, `slide2`, `slide10` and the list presents
/// them in that order, so any lexical ordering of the part names gives 1, 10, 2 —
/// a different, entirely plausible deck. This guard is the reason the fixture is
/// named that way.
#[test]
fn slide_order_comes_from_the_slide_id_list_and_not_the_part_names() {
    let imported = import_fixture();
    let order: Vec<String> = imported
        .presentation
        .slides()
        .iter()
        .map(title_text)
        .collect();
    assert_eq!(
        order,
        vec!["One".to_owned(), "Two".to_owned(), "Ten".to_owned()],
        "p:sldIdLst's order is the deck; a part-name ordering would give One, Ten, Two"
    );
}

/// A run's `a:rPr@sz` is carried in hundredths of a point, with no conversion.
///
/// `w:sz` is half-points and `a:rPr@sz` is hundredths. A value carried across
/// without conversion is wrong by fifty and still looks like a font size, so this
/// asserts the exact authored number rather than a range.
#[test]
fn a_run_keeps_its_font_size_in_hundredths_of_a_point() {
    let imported = import_fixture();
    let first = imported
        .presentation
        .slides()
        .first()
        .expect("the deck has slides");
    let title = first.shapes.title().expect("the title slot is filled");
    let body = title.text.as_ref().expect("the title carries text");
    let run = body
        .paragraphs
        .first()
        .and_then(|paragraph| paragraph.runs.first())
        .expect("the title has a run");
    let properties = run.properties().expect("the run states properties");
    assert_eq!(
        properties.size_hundredths_point,
        Some(4_400),
        "44pt is 4400 hundredths; 88 would be half-points and 220000 a scaled hundredth"
    );
    assert_eq!(properties.bold, Some(true), "b=\"1\" is xsd:boolean true");
    assert!(
        properties
            .latin
            .as_ref()
            .is_some_and(casual_pres_model::Typeface::is_theme_reference),
        "+mj-lt must stay a theme reference rather than being resolved at import"
    );
}

/// A placeholder with no `a:xfrm` keeps a zero box on the slide, and the layout
/// tier carries the real geometry — so the cascade has both ends to work with.
///
/// This is the single most load-bearing property of the importer: a title shape
/// on a real slide states only its text. If the slot were dropped, the shape
/// would have no position anywhere; if the inherited geometry were materialised
/// into the shape, a round trip would turn inheritance into authorship.
#[test]
fn an_unpositioned_placeholder_keeps_its_slot_and_inherits_from_the_layout() {
    let imported = import_fixture();
    let presentation = &imported.presentation;
    let first = presentation.slides().first().expect("the deck has slides");

    let slide_title = first
        .shapes
        .slot(PlaceholderKind::CtrTitle, 0)
        .expect("the slide fills the ctrTitle slot");
    let GroupChild::Shape(slide_shape) = &slide_title.content else {
        panic!("a p:sp maps onto GroupChild::Shape");
    };
    assert_eq!(
        (slide_shape.extent.width_emu, slide_shape.extent.height_emu),
        (0, 0),
        "the slide states no a:xfrm, so nothing may be invented for it"
    );

    let layout = presentation
        .layout_of(first)
        .expect("the slide's layout resolves");
    let layout_title = layout
        .shapes
        .slot(PlaceholderKind::CtrTitle, 0)
        .expect("the layout fills the same slot, which is what makes the cascade work");
    let GroupChild::Shape(layout_shape) = &layout_title.content else {
        panic!("a p:sp maps onto GroupChild::Shape");
    };
    assert_eq!(
        (
            layout_shape.offset.x_emu,
            layout_shape.offset.y_emu,
            layout_shape.extent.width_emu,
            layout_shape.extent.height_emu,
        ),
        (1_524_000, 1_122_363, 9_144_000, 2_387_600),
        "the layout's slot carries the EMU geometry the slide inherits"
    );

    // The pair (type, idx) is what resolves, never the type alone: the
    // title-slide layout has a `subTitle` at idx 1, and matching on type would
    // have found it for a query at idx 0.
    assert!(
        layout.shapes.slot(PlaceholderKind::SubTitle, 1).is_some(),
        "the subTitle slot is at idx 1"
    );
    assert!(
        layout.shapes.slot(PlaceholderKind::SubTitle, 0).is_none(),
        "a slot is identified by the pair, so idx 0 must not match idx 1"
    );
}

/// A `p:pic` resolves through its `a:blip@r:embed` to a registered media entry,
/// and the entry records the part the package declared.
#[test]
fn a_picture_registers_its_media_and_resolves_against_the_definitions() {
    let imported = import_fixture();
    let presentation = &imported.presentation;
    let second = presentation
        .slides()
        .get(1)
        .expect("the deck has a second slide");
    let picture = second
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Picture(picture) => Some(picture),
            _ => None,
        })
        .expect("slide 2 carries a p:pic");

    let entry = presentation
        .definitions()
        .media
        .get(&picture.media)
        .expect("the picture's media reference resolves in Definitions::media");
    assert_eq!(entry.part_name, "ppt/media/image1.png");
    assert_eq!(
        entry.media_type, "image/png",
        "the declared content type is authoritative, not the extension"
    );
    assert_eq!(
        entry.relationship_id, "rIdImage",
        "the authored r:id is kept so export can re-emit the same reference"
    );
    assert_eq!(
        (picture.extent.width_emu, picture.extent.height_emu),
        (914_400, 914_400),
        "a picture is sized by its OWN a:ext"
    );
    assert_eq!(
        picture.descr.as_deref(),
        Some("The company mark"),
        "the alt text is accessibility data and must survive"
    );
}

/// A nested `p:grpSp` keeps its child coordinate space, which is what scales its
/// children into the group box.
///
/// The fixture's group is 2743200 x 1371600 with a child space of 1371600 x
/// 685800 — exactly 2x — so a reader that substituted the box for the child
/// extent would draw every child at half size and look merely "slightly off".
#[test]
fn a_nested_group_keeps_its_child_coordinate_space() {
    let imported = import_fixture();
    let second = imported
        .presentation
        .slides()
        .get(1)
        .expect("the deck has a second slide");
    let group = second
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Group(group) => Some(group),
            _ => None,
        })
        .expect("slide 2 carries a p:grpSp");

    assert_eq!(
        (
            group.transform.extent.width_emu,
            group.transform.extent.height_emu
        ),
        (2_743_200, 1_371_600)
    );
    assert_eq!(
        (
            group.transform.child_extent.width_emu,
            group.transform.child_extent.height_emu
        ),
        (1_371_600, 685_800),
        "a:chExt must not be collapsed onto a:ext; the ratio is the child scale"
    );
    assert_eq!(group.children.len(), 2, "both nested shapes arrive");
    let geometries: Vec<ShapeGeometry> = group
        .children
        .iter()
        .filter_map(|child| match child {
            GroupChild::Shape(shape) => Some(shape.geometry),
            _ => None,
        })
        .collect();
    assert_eq!(
        geometries,
        vec![ShapeGeometry::Ellipse, ShapeGeometry::Triangle],
        "children stay in document order, which is paint order"
    );
}

/// The producer's recorded `a:normAutofit` scales are honoured verbatim on load.
///
/// The producer solved the fit and wrote the answer; honouring it is what makes
/// an untouched file render identically. Re-solving at open would replace
/// PowerPoint's own numbers with ours and change the appearance of a file nobody
/// edited. ONLYOFFICE behaves the same way and keeps re-solving as an explicit
/// edit-time operation.
#[test]
fn the_recorded_autofit_scales_are_honoured_rather_than_resolved() {
    let imported = import_fixture();
    let first = imported
        .presentation
        .slides()
        .first()
        .expect("the deck has slides");
    let title = first.shapes.title().expect("the title slot is filled");
    let body = title.text.as_ref().expect("the title carries text");
    assert_eq!(
        body.body_properties.auto_fit,
        TextAutoFit::Normal {
            font_scale: Some(92_500),
            line_space_reduction: Some(10_000),
        },
        "thousandths of a percent, as authored: 92.5% and 10%"
    );
}

/// An explicit `a:buNone` is not the same as an absent bullet.
///
/// Absent means *inherit*; `a:buNone` SUPPRESSES an inherited bullet. Collapsing
/// the two grows a bullet on a deliberately unbulleted paragraph inside a
/// bulleted body placeholder — visible, wrong, and silent.
#[test]
fn an_explicit_bullet_none_is_distinguishable_from_an_absent_bullet() {
    let imported = import_fixture();
    let second = imported
        .presentation
        .slides()
        .get(1)
        .expect("the deck has a second slide");
    let body = second
        .shapes
        .slot(PlaceholderKind::Object, 1)
        .and_then(|node| node.text.as_ref())
        .expect("slide 2's content placeholder carries text");

    let bullets: Vec<Option<TextBullet>> = body
        .paragraphs
        .iter()
        .map(|paragraph| {
            paragraph
                .properties
                .as_deref()
                .and_then(|properties| properties.bullet.clone())
        })
        .collect();

    assert!(
        matches!(
            bullets.first(),
            Some(Some(TextBullet::Character { character, font }))
                if !character.is_empty()
                    && font.as_ref().is_some_and(|font| font.name == "Wingdings")
        ),
        "a:buChar keeps its glyph AND its font — the common bullets live in \
         Wingdings and Symbol, so dropping the font renders a box; got {:?}",
        bullets.first()
    );
    assert!(
        matches!(
            bullets.get(1),
            Some(Some(TextBullet::AutoNumber {
                scheme: casual_pres_model::AutoNumberScheme::AlphaLcParenR,
                start_at: Some(3),
            }))
        ),
        "a:buAutoNum keeps its scheme and its startAt; got {:?}",
        bullets.get(1)
    );
    assert_eq!(
        bullets.get(2),
        Some(&Some(TextBullet::None)),
        "a:buNone must be Some(TextBullet::None), never None"
    );
    assert_eq!(
        bullets.get(3),
        Some(&None),
        "a paragraph that states no bullet inherits, so its bullet is None"
    );

    // The outline level is zero-based while the element names are one-based, and
    // the two must not be confused.
    let levels: Vec<u8> = body
        .paragraphs
        .iter()
        .map(casual_pres_model::TextParagraph::level)
        .collect();
    assert_eq!(
        levels,
        vec![0, 1, 2, 0],
        "an absent a:pPr@lvl is level 0 and lvl=\"1\" is the SECOND level"
    );
    assert!(
        body.list_style.level(1).is_some(),
        "a:lvl2pPr is the zero-based level 1 of the list style"
    );
    assert!(
        body.list_style.level(0).is_none(),
        "the fixture states no a:lvl1pPr, so level 0 inherits"
    );
}

/// A paragraph's spacing, indent and tab stops survive in their own units.
#[test]
fn paragraph_metrics_survive_in_their_authored_units() {
    let imported = import_fixture();
    let second = imported
        .presentation
        .slides()
        .get(1)
        .expect("the deck has a second slide");
    let body = second
        .shapes
        .slot(PlaceholderKind::Object, 1)
        .and_then(|node| node.text.as_ref())
        .expect("slide 2's content placeholder carries text");

    let first = body
        .paragraphs
        .first()
        .and_then(|paragraph| paragraph.properties.as_deref())
        .expect("the first paragraph states properties");
    assert_eq!(first.margin_left_emu, Some(228_600));
    assert_eq!(
        first.indent_emu,
        Some(-228_600),
        "a hanging indent is NEGATIVE; an unsigned read loses every bullet's hang"
    );

    let third = body
        .paragraphs
        .get(2)
        .and_then(|paragraph| paragraph.properties.as_deref())
        .expect("the third paragraph states properties");
    assert_eq!(
        third.line_spacing,
        Some(TextSpacing::Percent {
            thousandths: 150_000
        }),
        "a:spcPct is thousandths of a percent: 150% is 150000"
    );
    assert_eq!(
        third.space_before,
        Some(TextSpacing::Points { hundredths: 600 }),
        "a:spcPts is hundredths of a point: 6pt is 600"
    );
    assert_eq!(
        third.tab_stops.len(),
        1,
        "the a:tabLst entry must not be dropped"
    );
}

/// An `a:fld` keeps both its kind and its cached result.
///
/// The cache is what a reader that cannot evaluate the field displays. Dropping
/// it blanks every slide number in a deck opened by anything but PowerPoint.
#[test]
fn a_field_keeps_its_kind_and_its_cached_text() {
    let imported = import_fixture();
    let second = imported
        .presentation
        .slides()
        .get(1)
        .expect("the deck has a second slide");
    let body = second
        .shapes
        .slot(PlaceholderKind::Object, 1)
        .and_then(|node| node.text.as_ref())
        .expect("slide 2's content placeholder carries text");
    let field = body
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.runs)
        .find_map(|run| match run {
            TextRun::Field(field) => Some(field),
            _ => None,
        })
        .expect("slide 2 carries an a:fld");
    assert_eq!(field.kind, "slidenum");
    assert_eq!(
        field.text, "2",
        "the cached a:t is what a non-evaluating reader shows"
    );
    assert!(
        field.field_id.starts_with('{'),
        "the authored GUID is kept verbatim because PowerPoint matches fields by it"
    );

    // An `a:br` is its own child with its own properties, which set the height of
    // the blank line — so it is neither a run nor a newline inside one.
    let first = imported
        .presentation
        .slides()
        .first()
        .expect("the deck has slides");
    let subtitle = first
        .shapes
        .slot(PlaceholderKind::SubTitle, 1)
        .and_then(|node| node.text.as_ref())
        .expect("slide 1's subtitle carries text");
    let breaks = subtitle
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.runs)
        .filter(|run| matches!(run, TextRun::LineBreak(_)))
        .count();
    assert_eq!(breaks, 1, "the a:br arrives as its own paragraph child");
    assert!(
        subtitle.plain_text().contains('\n'),
        "a break contributes a newline to the plain text"
    );
}

/// A single-subpath `a:custGeom` keeps its authored commands, in order, with
/// `a:quadBezTo` and `a:cubicBezTo` kept distinct.
#[test]
fn a_custom_geometry_keeps_its_authored_path_commands() {
    let imported = import_fixture();
    let third = imported
        .presentation
        .slides()
        .get(2)
        .expect("the deck has a third slide");
    let freeform = third
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Shape(shape) if shape.path.is_some() => Some(shape),
            _ => None,
        })
        .expect("slide 10 carries an a:custGeom");
    let geometry = freeform.path.as_ref().expect("the path is modelled");
    assert_eq!(geometry.paths.len(), 1, "the fixture authors one a:path");
    let path = &geometry.paths[0];

    assert_eq!(
        freeform.geometry,
        ShapeGeometry::Other,
        "a custom geometry must not be mistaken for a preset"
    );
    assert_eq!(
        (path.width_emu, path.height_emu),
        (1_828_800, 1_828_800),
        "a:path@w/@h is the path's own coordinate space"
    );
    assert!(
        matches!(path.commands.first(), Some(ShapePathCommand::MoveTo { .. })),
        "a path always starts with a move; drawing from an undefined pen position \
         is how a freeform becomes a line to the origin"
    );
    assert_eq!(
        path.commands.len(),
        4,
        "moveTo, lnTo, cubicBezTo, close — four commands, none folded away"
    );
    assert!(
        matches!(path.commands.last(), Some(ShapePathCommand::Close)),
        "a:close must survive, or the shape is drawn unclosed"
    );
    assert!(
        matches!(
            path.commands.get(2),
            Some(ShapePathCommand::CubicBezTo { .. })
        ),
        "a cubic stays cubic"
    );
}

/// An unknown preset keeps its authored token rather than being rewritten to
/// `rect`, and a shape's fill, outline and rotation survive.
#[test]
fn an_unknown_preset_keeps_its_token_and_a_shape_keeps_its_appearance() {
    let imported = import_fixture();
    let presentation = &imported.presentation;

    let third = presentation.slides().get(2).expect("a third slide");
    let callout = third
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Shape(shape) if shape.preset.is_some() => Some((node, shape)),
            _ => None,
        })
        .expect("slide 10 carries an untyped preset");
    assert_eq!(callout.1.geometry, ShapeGeometry::Other);
    assert_eq!(
        callout.1.preset.as_deref(),
        Some("wedgeRoundRectCallout"),
        "the authored token is retained so export does not rewrite it to rect"
    );
    assert_eq!(
        callout.1.adjustments.len(),
        2,
        "both a:avLst guides are retained, in order"
    );
    assert!(
        callout.0.hidden,
        "a hidden shape is retained rather than dropped: the author can unhide it"
    );

    let first = presentation.slides().first().expect("a first slide");
    let bar = first
        .shapes
        .children
        .iter()
        .find_map(|node| match &node.content {
            GroupChild::Shape(shape) if shape.geometry == ShapeGeometry::RoundRectangle => {
                Some(shape)
            }
            _ => None,
        })
        .expect("slide 1 carries a roundRect");
    assert_eq!(
        bar.rotation,
        Some(1_200_000),
        "a:xfrm@rot is 1/60000 degree, so 20 degrees is 1200000"
    );
    assert!(bar.flip_h, "flipH=\"1\" is xsd:boolean true");
    let stroke = bar.stroke.as_ref().expect("the a:ln is modelled");
    assert_eq!(stroke.width_emu, 19_050, "a:ln@w is EMU, not points");
    assert_eq!(
        stroke.dash,
        Some(casual_doc_model::v1::DashStyle::Dash),
        "the dash pattern survives"
    );
    assert!(
        stroke.tail_end.is_some(),
        "a tail arrowhead survives with its size tokens"
    );
    assert!(
        bar.fill.is_some(),
        "a:solidFill resolves to a concrete fill"
    );
}

/// A slide hidden from the show is retained, and the flag's polarity is right.
///
/// `p:sld@show="0"` is the authored form and the model stores `hidden`, so the
/// polarity is inverted exactly once. A hidden slide is still in the deck, still
/// edited and still exported — dropping it would lose content the author kept.
#[test]
fn a_hidden_slide_is_retained_with_the_flag_the_right_way_round() {
    let imported = import_fixture();
    let slides = imported.presentation.slides();
    let hidden: Vec<bool> = slides.iter().map(|slide| slide.hidden).collect();
    assert_eq!(
        hidden,
        vec![false, false, true],
        "only the slide with show=\"0\" is hidden, and it is still present"
    );
    assert_eq!(
        slides.iter().filter(|slide| slide.hidden).count(),
        1,
        "the hidden slide must not be dropped from the deck"
    );
    assert_eq!(
        slides.get(2).and_then(|slide| slide.name.as_deref()),
        Some("Appendix"),
        "p:cSld@name is the author-visible slide name"
    );
}

/// The fidelity report names what the projection did not recover, and says so in
/// the shared taxonomy's terms.
///
/// The constructs asserted here are each present in the fixture and each
/// genuinely unmodelled, so a report missing any of them would be claiming a
/// fidelity this build does not have.
#[test]
fn the_report_names_every_construct_the_projection_did_not_recover() {
    let imported = import_fixture();
    let features = features(&imported);

    for expected in [
        // Animation and transitions: `docs/156` §8 makes byte-faithful retention
        // of these a Tier 2 requirement, and there is no retention side table on
        // the presentation path yet.
        "timing",
        "transition",
        // Fills and effects outside the modelled subset.
        "gradFill",
        "effectLst",
        // A CHART payload still arrives as nothing — `docs/156` §8 leaves
        // `c:chart` and SmartArt to `docs/155`/ADR-050 — and the `@uri` that
        // said which kind of payload it was goes with it. The frame itself is
        // asserted ABSENT below, because the frame now arrives in both cases.
        "chart",
        "graphicData/@uri",
        // A table style's formatting parts. The GUID joins, the appearance does
        // not, so a styled table arrives unstyled and the report says which
        // parts were dropped rather than naming the style list once.
        "wholeTbl",
        "band1H",
        // A cell's diagonal rule. Visible — a crossed-out cell that reopens
        // blank is loss — and `TableCellProperties` has four edges and no
        // diagonal.
        "lnTlToBr",
        // `a:fontRef` is READ now — the collection it names and its colour both
        // reach `ShapeStyleRef::font_ref` — and still unpainted, because nothing
        // applies a shape-scoped text default: a slide run resolves its typeface
        // through the placeholder cascade, never through the shape's own
        // `p:style`. So the finding moved from `style/@fontRef` ("the attribute
        // was dropped", now false) to `shape/fontRef` with a reason naming which
        // half went unpainted — the same feature name the DOCX reader uses for the
        // same element, so a host comparing two reports sees one name.
        "shape/fontRef",
        // `a:bgFillStyleLst` is not modelled, so a `p:bgRef` still resolves to
        // nothing. Reported on the list rather than only on the reference, because
        // the list is what is dropped.
        "bgFillStyleLst",
        // The theme's own display name, and the two scheme names that have no
        // field either. There is no PresentationML writer to re-emit the part, so
        // the name is gone rather than merely unmodelled.
        "theme/@name",
        "fontScheme/@name",
        "fmtScheme/@name",
    ] {
        assert!(
            features.contains(&expected),
            "the report must name {expected}; it named {features:?}"
        );
    }

    // Everything the theme work made resolvable. Asserted ABSENT rather than
    // merely dropped from the list above, because a list is satisfied by a report
    // that says nothing (`SKILL` §9.3) — and because each of these was a finding
    // on this fixture before the change, so a regression puts it straight back.
    for recovered in [
        // The FRAME is read now, in every case: its `p:xfrm` box, its name and
        // its hidden flag arrive whether its payload is a table, a chart or a
        // diagram. So naming it would overstate the loss, and the payload
        // assertions above are what keep that honest — a list is satisfied by a
        // report that says nothing (`SKILL` §9.3), and this is the entry that
        // would hide behind one.
        "graphicFrame",
        // Every part of an `a:tbl` this build reads. Each was a finding on this
        // fixture before the change, so a regression puts it straight back.
        "tbl",
        "tblPr",
        "tblGrid",
        "gridCol",
        "tr",
        "tc",
        "tcPr",
        "tableStyleId",
        "lnL",
        "lnB",
        // `a:satMod` is APPLIED now, on both document classes: `ColorTransform`
        // carries a saturation field and the shared `v1::fold_color_modifiers`
        // folds it as an exact HSL saturation scaling about the lightness. The
        // reader used to throw the value away on the stated ground that the model
        // had no field; that ground is gone, and
        // `the_colour_map_override_chain_gives_one_token_three_different_answers`
        // asserts the modulated colour with its arithmetic written out.
        "satMod",
        // The old feature name for an `a:fontRef`, asserted ABSENT: the attribute
        // is no longer dropped, so a report still saying so would be describing a
        // loss that no longer happens. The new name is asserted PRESENT above.
        "style/@fontRef",
        // `tableStyles.xml` is consumed, so it is no longer a whole-part loss.
        // The separate guard below asserts the entries actually arrived — this
        // one alone could be "fixed" by deleting the part.
        "ppt/tableStyles.xml",
        "tblStyle",
        // A `a:tblStyle` part that states nothing loses nothing: the fixture's
        // second entry self-closes `a:firstCol`, and reporting that would make
        // every real `tableStyles.xml` look lossier than it is.
        "firstCol",
        // `p:cNvGraphicFramePr` holds only `a:graphicFrameLocks`, an editing
        // lock rather than document content.
        "cNvGraphicFramePr",
        "txStyles",
        "defaultTextStyle",
        // The colour map is read, on the master and as an override on a layout and
        // a slide.
        "clrMap",
        "clrMapOvr",
        // And so every `a:schemeClr` in the fixture resolves: there is no theme
        // slot left for which this build has no answer.
        "schemeClr",
        // A `p:style` reference is read into `Definitions::shape_styles` with its
        // `a:phClr` argument resolved.
        "style",
        // The theme part is consumed, so it is no longer a whole-part loss. This
        // is the one entry a reader could "fix" by deleting the part, so the
        // separate guard below asserts the schemes actually arrived.
        "ppt/theme/theme1.xml",
        // The schemes themselves, and the slots and collections inside them.
        "clrScheme",
        "fontScheme",
        "fmtScheme",
        "majorFont",
        "minorFont",
        "latin",
        "accent1",
        "dk1",
        "tint",
        "shade",
        "alpha",
        "lumMod",
        "lumOff",
    ] {
        assert!(
            !features.contains(&recovered),
            "{recovered} is modelled now, so reporting it would overstate the loss: \
             {features:?}"
        );
    }

    // A `p:style` reference that RESOLVES and still cannot be painted is reported
    // with the reason, once per reference — the three reasons are the three the
    // fixture's style matrix provides, and the two references that lose nothing
    // (`a:fillRef idx="1"`, a solid entry, and `a:effectRef idx="0"`, "no effect")
    // must not appear.
    for (feature, reason) in [
        ("fmtScheme/fillStyleLst", "pattern"),
        ("fmtScheme/lnStyleLst", "unmodeled"),
        ("fmtScheme/effectStyleLst", "effect-not-rendered"),
        // `a:fontRef` is in this list and not in the one above, because the
        // REASON is the whole value of reading it: an uncaptured reference could
        // only say "the attribute was dropped". The fixture's two references are
        // `idx="minor"` with an `a:schemeClr val="lt1"` and `idx="major"` with
        // `val="dk1"`, so each names a collection the theme's font scheme
        // resolves AND a colour — both halves unpainted. Reading `@idx` with the
        // wrong grammar, or against the wrong collection, changes this string to
        // `colour-not-applied` while the feature name stays identical, which is
        // why the name alone is not the assertion.
        ("shape/fontRef", "typeface-and-colour-not-applied"),
    ] {
        let entry = imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == feature)
            .unwrap_or_else(|| panic!("the report must name {feature}; it named {features:?}"));
        assert_eq!(
            entry.location.attribute.as_deref(),
            Some(reason),
            "{feature} must say WHY it cannot be painted"
        );
        assert_eq!(
            entry.model_outcome(),
            ModelOutcome::Degraded,
            "the shape is still drawn, it just is not wearing the right appearance"
        );
    }
    // And once per REFERENCE, not once per deck: collapsing them would make a deck
    // that loses a typeface on forty shapes look like one that loses it on one.
    //
    // The expected count is DERIVED from the model's own side table rather than
    // typed, and that is not tidiness — the typed `2` here went stale the moment a
    // third `p:style` joined the fixture, which is a guard failing for the one
    // reason a guard must not: the fixture grew.
    let font_refs = imported
        .presentation
        .definitions()
        .shape_styles
        .iter()
        .filter(|(_, style)| style.font_ref.is_some())
        .count();
    assert!(
        font_refs >= 2,
        "the fixture must carry at least two a:fontRefs on different shapes, or          the per-reference count below cannot tell one finding per reference from          one per deck; it carries {font_refs}"
    );
    assert_eq!(
        imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "shape/fontRef")
            .map(|entry| entry.occurrences),
        u32::try_from(font_refs).ok(),
        "one finding per reference: {features:?}"
    );

    // Nothing may claim `preserved`: there is no presentation writer, so no
    // verbatim byte floor exists to license the claim.
    assert!(
        imported
            .report
            .entries
            .iter()
            .all(|entry| entry.retention_outcome() != RetentionOutcome::Preserved),
        "a preserved claim with no byte floor would be a false preservation claim"
    );
    assert!(
        imported.ledger.records().is_empty(),
        "the ledger is empty because nothing retains the source"
    );
    // And the report must be legal in the taxonomy's own terms, which
    // `Reporter::finish` already enforces by failing the import.
    assert_eq!(imported.report.validate(&imported.ledger), Ok(()));

    // An absent `a:xfrm` is reported, because "no transform" and "an explicit
    // zero transform" import identically and the cascade turns on the
    // difference.
    //
    // The count is asserted, not merely the presence, and it is DERIVED from the
    // imported deck rather than typed: there are two code paths that can report
    // this (a self-closing `<p:spPr/>`, and a populated `p:spPr` with no
    // `a:xfrm` in it), and a presence-only assertion is satisfied by either —
    // which is exactly how a guard ends up unable to say which path it is
    // charged to. The fixture carries both shapes on purpose.
    let unpositioned = unpositioned_shape_count(&imported);
    assert!(
        unpositioned >= 2,
        "the fixture must carry at least two unpositioned shapes for this guard to \
         discriminate between the two reporting paths; it carries {unpositioned}"
    );
    let reported = imported
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "spPr/@xfrm")
        .map_or(0, |entry| entry.occurrences);
    assert_eq!(
        u32::try_from(unpositioned).ok(),
        Some(reported),
        "every shape that states no a:xfrm must be reported exactly once; the deck \
         has {unpositioned} such shapes and the report counted {reported}"
    );
}

/// Every top-level shape or picture, across all three tiers, whose own extent is
/// zero — which is what a shape that stated no `a:xfrm` imports as.
///
/// Derived from the model so the expected count cannot be typed wrongly.
fn unpositioned_shape_count(imported: &ImportedPresentation) -> usize {
    let presentation = &imported.presentation;
    let trees = presentation
        .masters()
        .iter()
        .map(|master| &master.shapes)
        .chain(presentation.layouts().iter().map(|layout| &layout.shapes))
        .chain(presentation.slides().iter().map(|slide| &slide.shapes));
    trees
        .flat_map(|tree| tree.children.iter())
        .filter(|node| match &node.content {
            GroupChild::Shape(shape) => shape.extent.width_emu == 0 && shape.extent.height_emu == 0,
            GroupChild::Picture(picture) => {
                picture.extent.width_emu == 0 && picture.extent.height_emu == 0
            }
            _ => false,
        })
        .count()
}

/// A package whose main part is not PresentationML is refused, rather than being
/// read as a deck with no slides.
#[test]
fn a_package_that_is_not_a_presentation_is_refused() {
    let wordprocessing = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
    let bytes = deck::deck_with("[Content_Types].xml", wordprocessing);
    let error = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect_err("a WordprocessingML main part is not a presentation");
    assert!(
        matches!(error, ImportError::NotAPresentation { .. }),
        "got {error:?}"
    );
}

/// `p:sldSz` is required, not defaulted.
///
/// Every shape's geometry is expressed on that surface, so substituting 16:9
/// would silently reposition every shape in a 4:3 deck — a deck that renders
/// subtly wrong rather than being visibly refused.
#[test]
fn a_presentation_with_no_slide_size_is_refused_rather_than_defaulted() {
    let without = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>
<p:sldIdLst><p:sldId id="256" r:id="rIdSlideOne"/></p:sldIdLst>
</p:presentation>"#;
    let bytes = deck::deck_with("ppt/presentation.xml", without);
    let error = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect_err("a deck with no p:sldSz has no surface to position shapes on");
    assert!(
        matches!(error, ImportError::MissingSlideSize),
        "got {error:?}"
    );
}

/// A `p:sldId` whose `r:id` resolves to nothing fails the import, because the
/// deck's order cannot be reconstructed from anything else.
#[test]
fn a_slide_reference_that_does_not_resolve_fails_the_import() {
    let dangling = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst>
<p:sldIdLst><p:sldId id="256" r:id="rIdSlideOne"/><p:sldId id="257" r:id="rIdNoSuchSlide"/></p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
</p:presentation>"#;
    let bytes = deck::deck_with("ppt/presentation.xml", dangling);
    let error = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect_err("a dangling slide relationship cannot be ordered");
    assert!(
        matches!(
            error,
            ImportError::UnresolvedRelationship {
                ref relationship_id,
                ..
            } if relationship_id == "rIdNoSuchSlide"
        ),
        "got {error:?}"
    );
}

/// The two parts OPC fixes are required.
#[test]
fn a_package_missing_a_fixed_opc_part_is_refused() {
    for part in ["[Content_Types].xml", "_rels/.rels"] {
        let bytes = deck::deck_without(part);
        let error = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
            .expect_err("OPC fixes this part's name and requires it");
        assert!(
            matches!(error, ImportError::MissingRequiredPart { .. }),
            "removing {part} gave {error:?}"
        );
    }
}

/// A part declaring a DTD is refused unconditionally.
///
/// An external entity is an exfiltration and denial-of-service primitive and no
/// presentation needs one. Refusing the declaration outright means this does not
/// depend on the XML reader's entity policy staying as it is.
#[test]
fn a_part_declaring_a_doctype_is_refused() {
    let with_doctype = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE p:presentation [<!ENTITY exfiltrate SYSTEM "file:///etc/passwd">]>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldSz cx="12192000" cy="6858000"/>
</p:presentation>"#;
    let bytes = deck::deck_with("ppt/presentation.xml", with_doctype);
    let error = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect_err("a DTD is refused whatever it declares");
    assert!(
        matches!(error, ImportError::MalformedPartXml { .. }),
        "got {error:?}"
    );
}

/// A bound in `ImportLimits` actually bites, and the failure names the limit.
#[test]
fn an_element_budget_refuses_an_oversized_part() {
    let limits = ImportLimits {
        max_part_elements: 4,
        ..ImportLimits::default()
    };
    let error = import_pptx(&deck::deck(), PackageLimits::default(), limits)
        .expect_err("four elements cannot admit a real presentation part");
    assert!(
        matches!(
            error,
            ImportError::PartLimitExceeded {
                limit: "part_xml_elements",
                ..
            }
        ),
        "got {error:?}"
    );
}

/// Every bound is clamped to its hard ceiling, so a host cannot raise one past
/// what the build supports.
#[test]
fn import_limits_clamp_to_their_hard_ceilings() {
    let clamped = ImportLimits {
        max_slides: usize::MAX,
        max_layouts: usize::MAX,
        max_masters: usize::MAX,
        max_part_elements: u64::MAX,
        max_part_depth: u32::MAX,
        max_shapes_per_tree: usize::MAX,
        max_paragraphs_per_body: usize::MAX,
        max_runs_per_paragraph: usize::MAX,
        max_run_bytes: usize::MAX,
        max_table_rows: usize::MAX,
        max_table_columns: usize::MAX,
        max_table_styles: usize::MAX,
    }
    .clamped();
    assert_eq!(clamped.max_slides, ImportLimits::HARD_MAX_SLIDES);
    assert_eq!(clamped.max_layouts, ImportLimits::HARD_MAX_LAYOUTS);
    assert_eq!(clamped.max_masters, ImportLimits::HARD_MAX_MASTERS);
    assert_eq!(
        clamped.max_part_elements,
        ImportLimits::HARD_MAX_PART_ELEMENTS
    );
    assert_eq!(clamped.max_part_depth, ImportLimits::HARD_MAX_PART_DEPTH);
    assert_eq!(
        clamped.max_shapes_per_tree,
        ImportLimits::HARD_MAX_SHAPES_PER_TREE
    );
    assert_eq!(clamped.max_table_rows, ImportLimits::HARD_MAX_TABLE_ROWS);
    assert_eq!(
        clamped.max_table_columns,
        ImportLimits::HARD_MAX_TABLE_COLUMNS
    );
    assert_eq!(
        clamped.max_table_styles,
        ImportLimits::HARD_MAX_TABLE_STYLES
    );
    assert_eq!(
        clamped.max_paragraphs_per_body,
        ImportLimits::HARD_MAX_PARAGRAPHS_PER_BODY
    );
    assert_eq!(
        clamped.max_runs_per_paragraph,
        ImportLimits::HARD_MAX_RUNS_PER_PARAGRAPH
    );
    assert_eq!(clamped.max_run_bytes, ImportLimits::HARD_MAX_RUN_BYTES);
    // The default must be strictly inside every ceiling, or the ceiling is
    // decoration.
    let default = ImportLimits::default();
    assert!(default.max_slides < ImportLimits::HARD_MAX_SLIDES);
    assert!(default.max_part_elements < ImportLimits::HARD_MAX_PART_ELEMENTS);
    assert!(default.max_part_depth < ImportLimits::HARD_MAX_PART_DEPTH);
}

/// Importing the same bytes twice yields the same deck, including its node ids.
///
/// Determinism is a correctness property here, not a nicety: a snapshot
/// comparison, a collaboration rebase and a golden test all depend on two
/// imports of one file agreeing.
#[test]
fn importing_the_same_package_twice_is_deterministic() {
    let bytes = deck::deck();
    let first = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    let second = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture imports");
    assert_eq!(first.presentation, second.presentation);
    assert_eq!(first.report.entries, second.report.entries);
    assert_eq!(
        first.presentation.node_ids().len(),
        second.presentation.node_ids().len()
    );
}

/// The two cascade tiers arrive with their authored values, in their authored
/// units, and the tier selection is the one PowerPoint uses.
///
/// This is the half that the absence of a loss finding cannot prove: a reader that
/// consumed `p:txStyles` and threw it away reports nothing either. So every value
/// is checked, and the title and body tiers are checked to be DIFFERENT, because a
/// reader that put the same `ListStyle` in all three would satisfy any single-tier
/// assertion.
#[test]
fn the_master_text_style_tiers_and_the_default_text_style_are_read() {
    use casual_pres_model::PlaceholderKind;

    let imported = import_fixture();
    let master = &imported.presentation.masters()[0];

    // `a:defRPr@sz` is HUNDREDTHS of a point, so 4400 is 44pt. A reader that
    // treated it as `w:sz`'s half-points would land on 2200 hundredths — 22pt —
    // and a title at half size still looks like a title.
    let title = master
        .text_styles
        .title
        .level(0)
        .expect("the title tier states level 1");
    assert_eq!(
        title.alignment,
        Some(casual_pres_model::TextAlign::Center),
        "p:titleStyle's lvl1 is centred"
    );
    assert_eq!(
        title
            .default_character
            .as_ref()
            .and_then(|character| character.size_hundredths_point),
        Some(4400),
        "44pt, in hundredths of a point"
    );

    let body = master
        .text_styles
        .body
        .level(0)
        .expect("the body tier states level 1");
    assert_eq!(body.margin_left_emu, Some(228_600), "a:lvl1pPr@marL in EMU");
    assert_eq!(
        body.indent_emu,
        Some(-228_600),
        "a hanging indent is NEGATIVE"
    );
    assert_eq!(
        body.default_character
            .as_ref()
            .and_then(|character| character.size_hundredths_point),
        Some(2800),
        "28pt body text"
    );
    assert!(
        body.bullet.is_some(),
        "the body tier's authored a:buChar must survive"
    );

    // The tiers are distinct, which is what proves three readers were not one.
    assert_ne!(
        master.text_styles.title.level(0),
        master.text_styles.body.level(0),
        "a title is 44pt centred and a body is 28pt bulleted; one ListStyle in \
         both slots would pass every assertion above"
    );

    // `p:defaultTextStyle` is the LAST tier, on the presentation rather than the
    // master, and 18pt is PowerPoint's own default body size.
    assert_eq!(
        imported
            .presentation
            .default_text_style()
            .level(0)
            .and_then(|level| level.default_character.as_ref())
            .and_then(|character| character.size_hundredths_point),
        Some(1800),
        "p:defaultTextStyle's lvl1 is 18pt"
    );

    // Tier selection. The entry that matters is the LAST one: a shape in no slot
    // takes the BODY tier, not the other tier, which is the reading the element
    // names do not give you.
    for (slot, expected) in [
        (Some(PlaceholderKind::Title), &master.text_styles.title),
        (Some(PlaceholderKind::CtrTitle), &master.text_styles.title),
        (Some(PlaceholderKind::Body), &master.text_styles.body),
        (Some(PlaceholderKind::SubTitle), &master.text_styles.body),
        (Some(PlaceholderKind::Object), &master.text_styles.body),
        (Some(PlaceholderKind::Footer), &master.text_styles.other),
        (
            Some(PlaceholderKind::SlideNumber),
            &master.text_styles.other,
        ),
        (None, &master.text_styles.other),
    ] {
        assert_eq!(
            master.text_styles.tier(slot),
            expected,
            "{slot:?} resolves to the wrong tier"
        );
    }
}

/// A SELF-CLOSING `a:rPr` keeps its attributes.
///
/// `<a:rPr lang="en-US" sz="2400" i="1"/>` is the commonest run-properties form
/// PowerPoint writes — a childless element whose whole content is attributes — and
/// every one of the four character-property call sites used to answer
/// `TextCharacterProperties::default()` for it. So a stated size, weight, italic,
/// underline or spacing on a childless element was dropped, which is most of the
/// stated formatting in most real decks.
///
/// The existing size guard could not catch it: the title's `a:rPr` has an
/// `a:solidFill` and an `a:latin` child, so it takes the non-empty path. This one
/// is pinned to the subtitle, whose `a:rPr` is self-closing, and asserts the
/// fixture still carries that form — otherwise the row proves nothing.
#[test]
fn a_self_closing_run_properties_element_keeps_its_attributes() {
    assert!(
        String::from_utf8_lossy(
            &deck::deck_parts()
                .into_iter()
                .find(|(name, _)| name == "ppt/slides/slide1.xml")
                .expect("the fixture carries slide1.xml")
                .1
        )
        .contains(r#"<a:rPr lang="en-US" sz="2400" i="1"/>"#),
        "the fixture must still carry a self-closing a:rPr with attributes, or \
         this guard is vacuous"
    );

    let imported = import_fixture();
    let slide = &imported.presentation.slides()[0];
    let subtitle = slide
        .shapes
        .children
        .iter()
        .find(|child| {
            child
                .placeholder
                .is_some_and(|slot| slot.kind == casual_pres_model::PlaceholderKind::SubTitle)
        })
        .expect("the title slide fills the subTitle slot");
    let properties = subtitle
        .text
        .as_ref()
        .and_then(|body| body.paragraphs.first())
        .and_then(|paragraph| paragraph.runs.first())
        .and_then(|run| run.properties())
        .expect("the subtitle's first run states properties");

    assert_eq!(
        properties.size_hundredths_point,
        Some(2_400),
        "24pt, from a self-closing a:rPr"
    );
    assert_eq!(
        properties.italic,
        Some(true),
        "i=\"1\" from the same element"
    );
    assert_eq!(
        properties.language.as_deref(),
        Some("en-US"),
        "and @lang, which is on every a:rPr Word or PowerPoint writes"
    );
}

/// A self-closing `a:endParaRPr` keeps its attributes too — the third call site.
///
/// Worth its own row rather than folded into the `a:rPr` one: the four
/// character-property call sites share a helper and nothing else, so reverting any
/// one of them leaves the others green. `a:endParaRPr` carries the formatting of
/// the paragraph MARK, which is what gives an empty trailing paragraph its height
/// — a 14pt mark read as "no properties" collapses the blank line a deck's author
/// put there deliberately.
#[test]
fn a_self_closing_paragraph_mark_keeps_its_attributes() {
    let imported = import_fixture();
    let body = imported
        .presentation
        .slides()
        .get(1)
        .and_then(|slide| slide.shapes.slot(PlaceholderKind::Object, 1))
        .and_then(|node| node.text.as_ref())
        .expect("slide 2's content placeholder carries text");
    let last = body
        .paragraphs
        .last()
        .expect("the body ends with the empty paragraph");
    assert!(
        last.runs.is_empty(),
        "the last paragraph is the empty one, whose only content is its mark"
    );
    let mark = last
        .end_properties
        .as_deref()
        .expect("a:endParaRPr states the mark's properties");
    assert_eq!(
        mark.size_hundredths_point,
        Some(1_400),
        "14pt, from a self-closing a:endParaRPr"
    );
}

/// And a line break's `a:rPr`, which is the fourth and last call site.
///
/// `<a:br><a:rPr sz="1200"/></a:br>` is how a deck sets the height of the blank it
/// is inserting: the break carries run properties of its own, and a 12pt break in a
/// 24pt paragraph is a deliberately tighter gap. Read as "no properties" it becomes
/// a full-size line and the layout the author tuned is gone.
#[test]
fn a_line_breaks_run_properties_are_read() {
    use casual_pres_model::TextRun;

    let imported = import_fixture();
    let subtitle = imported.presentation.slides()[0]
        .shapes
        .children
        .iter()
        .find(|child| {
            child
                .placeholder
                .is_some_and(|slot| slot.kind == PlaceholderKind::SubTitle)
        })
        .and_then(|node| node.text.as_ref())
        .expect("the title slide's subtitle carries text");
    let break_properties = subtitle
        .paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.runs.iter())
        .find_map(|run| match run {
            TextRun::LineBreak(line_break) => Some(line_break),
            _ => None,
        })
        .and_then(|line_break| line_break.properties.as_deref())
        .expect("the subtitle's a:br states its own properties");
    assert_eq!(
        break_properties.size_hundredths_point,
        Some(1_200),
        "12pt, from the break's own self-closing a:rPr"
    );
}

/// The fold: a run that states nothing resolves a real font size through the
/// chain, and a run that states one wins.
///
/// This is the assertion the whole cascade exists for, driven from a real package
/// rather than a hand-built model — so it covers the importer AND the resolver, and
/// a tier that is read but never folded fails here.
#[test]
fn a_run_that_states_nothing_inherits_a_size_through_the_whole_chain() {
    let imported = import_fixture();
    let presentation = &imported.presentation;
    let slide = &presentation.slides()[1];

    // Slide 2's content placeholder: its paragraphs state `a:pPr` metrics and its
    // runs state sizes, so the cascade is exercised with real competition.
    let body = slide
        .shapes
        .slot(PlaceholderKind::Object, 1)
        .expect("slide 2 fills the content slot");
    let cascade = presentation.text_cascade(slide, body);

    // Level 0 inherits the master's BODY tier: 28pt, a 228600 EMU hanging indent
    // and a bullet, none of which the shape or the layout state.
    let level0 = cascade.resolve(0);
    assert_eq!(
        level0.character.size_hundredths_point,
        Some(2_800),
        "28pt comes from the master's p:bodyStyle, four tiers up"
    );
    assert_eq!(level0.paragraph.margin_left_emu, Some(228_600));
    assert_eq!(level0.paragraph.indent_emu, Some(-228_600));
    assert!(
        level0.paragraph.bullet.is_some(),
        "the body tier's bullet is inherited, not invented"
    );

    // Level 1 inherits the SHAPE's own `a:lstStyle`, which states level 2 only.
    // The size still comes from the tier above, because the shape's level states
    // none — which is the mixed case a single-tier test cannot produce.
    let level1 = cascade.resolve(1);
    assert_eq!(
        level1.paragraph.margin_left_emu,
        Some(742_950),
        "the shape's own a:lvl2pPr wins for its margin"
    );
    assert_eq!(
        level1
            .paragraph
            .bullet_font
            .as_ref()
            .map(|font| font.name.as_str()),
        Some("Courier New"),
        "and for its bullet font"
    );

    // A level no tier in THIS chain states resolves to nothing — and that is two
    // separate facts, both deliberate. A missing `a:lvl9pPr` does NOT fall back to
    // `a:lvl1pPr`: each level is its own definition, and substituting level 1 would
    // give a ninth-level bullet the first level's indent. And a placeholder does not
    // take `p:defaultTextStyle`, which is the only tier here that states level 1,
    // so nothing is left to inherit.
    let level8 = cascade.resolve(8);
    assert_eq!(
        level8.character.size_hundredths_point, None,
        "an undeclared level inherits nothing rather than borrowing level 1's 18pt"
    );

    // And the run's own `a:rPr` beats every inherited tier. Asserted with a size
    // the fixture does NOT contain: this paragraph's real run states `sz="2800"`,
    // the same value it inherits, so overlaying it proves nothing about direction —
    // a resolver that ignored the run entirely would pass. 3600 can only appear if
    // the overlay actually ran, and the inherited value can only survive if it did
    // not.
    let mut with_run = cascade.resolve(0);
    assert_eq!(
        with_run.character.size_hundredths_point,
        Some(2_800),
        "the inherited value before the run is overlaid"
    );
    with_run.overlay_run(&casual_pres_model::TextCharacterProperties {
        size_hundredths_point: Some(3_600),
        ..casual_pres_model::TextCharacterProperties::default()
    });
    assert_eq!(
        with_run.character.size_hundredths_point,
        Some(3_600),
        "a run that states its own size wins over every tier it inherits from"
    );
    assert_eq!(
        with_run.paragraph.margin_left_emu,
        Some(228_600),
        "and a property the run says nothing about keeps what it inherited — an \
         overlay that replaced the whole layer would have cleared this"
    );
}

/// A shape in NO placeholder slot inherits through the OTHER pair of tiers:
/// `p:otherStyle` and `p:defaultTextStyle`.
///
/// Two facts at once, and they are the two I had to correct. A non-placeholder
/// shape is not a no-type placeholder — the first takes `p:otherStyle`, the second
/// takes `p:bodyStyle` — and `p:defaultTextStyle` applies to exactly this case,
/// text "not in a placeholder" (ECMA-376 §19.2.1.8), which is why a placeholder
/// does not see it.
///
/// So the fixture's unplaced shape resolves 18pt from the deck default, NOT the
/// 28pt its `p:bodyStyle` states — and reading 28pt here is the signature of the
/// bug this guard replaced.
#[test]
fn a_shape_in_no_slot_inherits_the_other_tier_and_the_deck_default() {
    let imported = import_fixture();
    let presentation = &imported.presentation;
    let slide = &presentation.slides()[0];
    let unplaced = slide
        .shapes
        .children
        .iter()
        .find(|child| child.placeholder.is_none())
        .expect("the title slide carries a shape in no slot");

    let resolved = presentation.text_cascade(slide, unplaced).resolve(0);
    assert_eq!(
        resolved.character.size_hundredths_point,
        Some(1_800),
        "18pt from p:defaultTextStyle: the fixture's p:otherStyle states no size, \
         and 28pt here would mean the body tier had been applied to a shape that \
         is not a placeholder"
    );
}

/// Tier PRECEDENCE, on a deck perturbed so the tiers actually disagree.
///
/// # Why a perturbed deck and not the plain fixture
///
/// The plain fixture states each property at exactly one tier, so no two tiers
/// ever compete — and four separate mutations of the resolver stayed green on it:
/// reversing the whole overlay order, making the overlay clobber unstated fields,
/// applying `p:defaultTextStyle` to a placeholder, and matching the slot tiers for
/// a shape with no `p:ph`. A cascade guard with no competition is the
/// "assertion cannot tell which path it is charged to" shape `SKILL` §4 names.
///
/// So this one authors the conflict. Three tiers each state level 1, with
/// deliberately different values:
///
/// * `p:defaultTextStyle` — 18pt, right-aligned;
/// * the master's `p:bodyStyle` — 28pt, LEFT-aligned;
/// * the content shape's own `a:lstStyle` — 32pt, and no alignment at all.
///
/// Every one of the four mutations is then observable from the three assertions
/// below, which is the point.
#[test]
fn the_tiers_disagree_and_the_higher_one_wins() {
    use casual_pres_model::TextAlign;

    let master = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slideMasters/slideMaster1.xml")
            .expect("the fixture carries the master")
            .1,
    )
    .expect("the master part is UTF-8");
    // `algn="l"` on the body tier, where the shape states none: an overlay that
    // replaced the whole layer rather than filling unstated fields would lose it.
    let body_before = r#"<p:bodyStyle><a:lvl1pPr marL="228600" indent="-228600">"#;
    let body_after = r#"<p:bodyStyle><a:lvl1pPr algn="l" marL="228600" indent="-228600">"#;
    assert!(master.contains(body_before), "the body tier moved");
    let master = master.replace(body_before, body_after);

    // Two SLOTS on the master, each with a property no other tier states:
    //
    //  * `(obj, 1)`, which slide 2's content placeholder matches — so its
    //    `defTabSz` proves the master-slot tier is consulted at all;
    //  * `(obj, 0)`, which NOTHING matches — so its `rtl` proves a shape with no
    //    `p:ph` is not resolved as though it were in slot `(obj, 0)`.
    //
    // Both are written as self-closing `a:lvl1pPr` deliberately: that is the form
    // real `p:txStyles` tiers use, and reading it was the second defect this test
    // uncovered.
    let slot_shapes = r#"<p:sp>
<p:nvSpPr><p:cNvPr id="9" name="Matched Slot"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="obj" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100000" cy="100000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr defTabSz="12700"/></a:lstStyle><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="10" name="Unmatched Slot"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="obj"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100000" cy="100000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr rtl="1"/></a:lstStyle><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody>
</p:sp>
</p:spTree>"#;
    assert_eq!(master.matches("</p:spTree>").count(), 1, "one shape tree");
    let master = master.replace("</p:spTree>", slot_shapes);

    let presentation_part = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/presentation.xml")
            .expect("the fixture carries the presentation part")
            .1,
    )
    .expect("the presentation part is UTF-8");
    // `algn="r"` on the deck default, which a PLACEHOLDER must never see — and
    // `marR`, which NO other tier states anywhere. The alignment alone cannot catch
    // a deck tier wrongly applied to a placeholder, because the master tier's
    // `algn="l"` overrides it either way; `marR` is the property with no competitor,
    // so its presence is the only observable difference.
    let deck_before = r#"<p:defaultTextStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr>"#;
    let deck_after =
        r#"<p:defaultTextStyle><a:lvl1pPr algn="r" marR="99999"><a:defRPr sz="1800"/></a:lvl1pPr>"#;
    assert!(
        presentation_part.contains(deck_before),
        "the deck default moved"
    );
    let presentation_part = presentation_part.replace(deck_before, deck_after);

    let layout_two = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slideLayouts/slideLayout2.xml")
            .expect("the fixture carries slideLayout2.xml")
            .1,
    )
    .expect("the layout part is UTF-8");
    // The layout's own `(obj, 1)` slot, one tier nearer the shape than the master's.
    // `spcBef` is stated nowhere else at level 1.
    let layout_before = r#"<p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle/>"#;
    let layout_after = r#"<p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm></p:spPr>
<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:spcBef><a:spcPts val="700"/></a:spcBef></a:lvl1pPr></a:lstStyle>"#;
    assert!(
        layout_two.contains(layout_before),
        "the layout's content slot moved"
    );
    let layout_two = layout_two.replace(layout_before, layout_after);

    let slide_two = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slides/slide2.xml")
            .expect("the fixture carries slide2.xml")
            .1,
    )
    .expect("the slide part is UTF-8");
    // 32pt on the SHAPE's own tier, competing with the master tier's 28pt.
    let shape_before = r#"<a:lstStyle><a:lvl2pPr marL="742950""#;
    let shape_after =
        r#"<a:lstStyle><a:lvl1pPr><a:defRPr sz="3200"/></a:lvl1pPr><a:lvl2pPr marL="742950""#;
    assert!(slide_two.contains(shape_before), "the shape's tier moved");
    let slide_two = slide_two.replace(shape_before, shape_after);

    let mut parts = deck::deck_parts();
    for (name, body) in [
        ("ppt/slideMasters/slideMaster1.xml", master),
        ("ppt/slideLayouts/slideLayout2.xml", layout_two),
        ("ppt/presentation.xml", presentation_part),
        ("ppt/slides/slide2.xml", slide_two),
    ] {
        let slot = parts
            .iter_mut()
            .find(|(part, _)| part == name)
            .expect("the part exists");
        slot.1 = body.into_bytes();
    }
    let imported = import_pptx(
        &deck::build_pptx(&parts),
        casual_doc_package::PackageLimits::default(),
        ImportLimits::default(),
    )
    .expect("the perturbed deck imports");

    let presentation = &imported.presentation;
    let slide = &presentation.slides()[1];
    let body = slide
        .shapes
        .slot(PlaceholderKind::Object, 1)
        .expect("slide 2 fills the content slot");
    let resolved = presentation.text_cascade(slide, body).resolve(0);

    assert_eq!(
        resolved.character.size_hundredths_point,
        Some(3_200),
        "the SHAPE's 32pt beats the master tier's 28pt — reversing the overlay \
         order gives 28pt here"
    );
    assert_eq!(
        resolved.paragraph.alignment,
        Some(TextAlign::Left),
        "the master tier's left alignment survives a shape tier that states none \
         — an overlay that clobbered unstated fields clears this"
    );
    assert_ne!(
        resolved.paragraph.alignment,
        Some(TextAlign::Right),
        "and p:defaultTextStyle's right alignment must NOT reach a placeholder"
    );
    assert_eq!(
        resolved.paragraph.margin_left_emu,
        Some(228_600),
        "the body tier's margin still inherits through all of it"
    );
    assert_eq!(
        resolved.paragraph.default_tab_emu,
        Some(12_700),
        "the MASTER's matching slot tier is consulted — dropping it from the \
         cascade loses this and nothing else"
    );
    assert!(
        resolved.paragraph.space_before.is_some(),
        "and so is the LAYOUT's, one tier nearer: {:?}",
        resolved.paragraph.space_before
    );
    assert_eq!(
        resolved.paragraph.right_to_left, None,
        "the master's UNMATCHED (obj, 0) slot must not contribute to an (obj, 1) \
         placeholder"
    );
    assert_eq!(
        resolved.paragraph.margin_right_emu, None,
        "p:defaultTextStyle's marR has no competitor at any tier, so a placeholder \
         resolving it is proof the deck tier was applied where it must not be"
    );

    // The non-placeholder case, where the deck default DOES apply and the slot
    // tiers must not be consulted at all.
    let unplaced = presentation.slides()[0]
        .shapes
        .children
        .iter()
        .find(|child| child.placeholder.is_none())
        .expect("the title slide carries a shape in no slot");
    let unplaced = presentation
        .text_cascade(&presentation.slides()[0], unplaced)
        .resolve(0);
    assert_eq!(
        unplaced.paragraph.alignment,
        Some(TextAlign::Right),
        "a shape in no slot sees p:defaultTextStyle, which a placeholder does not"
    );
    assert_eq!(
        unplaced.character.size_hundredths_point,
        Some(1_800),
        "and takes its 18pt, not the body tier's 28pt"
    );
    assert_eq!(
        unplaced.paragraph.margin_right_emu,
        Some(99_999),
        "the deck tier's uncontested marR DOES reach a shape in no slot — the \
         other half of the assertion above, so neither direction is vacuous"
    );
    assert_eq!(
        unplaced.paragraph.right_to_left, None,
        "and the master's (obj, 0) slot must not be consulted for a shape that is \
         not a placeholder at all — resolving it as slot (obj, 0) is the plausible \
         shortcut, and this is what refuses it"
    );
    assert_eq!(
        unplaced.paragraph.default_tab_emu, None,
        "nor the (obj, 1) one"
    );
}

/// A self-closing `a:pPr` keeps its attributes — the paragraph's own call site.
///
/// `<a:pPr algn="ctr"/>` and `<a:pPr lvl="1"/>` are how a paragraph states one
/// thing and nothing else, and `@lvl` is the single most load-bearing attribute on
/// a slide: it selects which level of every tier applies. Dropped, every bullet
/// below the first resolves at level 0 and the whole outline flattens.
///
/// Separate from the `a:lvlNpPr` row because the two call sites are different lines
/// in different functions: reverting either leaves the other green, which is what
/// the mutation run showed.
#[test]
fn a_self_closing_paragraph_properties_element_keeps_its_attributes() {
    use casual_pres_model::TextAlign;

    assert!(
        String::from_utf8_lossy(
            &deck::deck_parts()
                .into_iter()
                .find(|(name, _)| name == "ppt/slides/slide1.xml")
                .expect("the fixture carries slide1.xml")
                .1
        )
        .contains(r#"<a:pPr algn="ctr"/>"#),
        "the fixture must still carry a self-closing a:pPr with an attribute, or \
         this guard is vacuous"
    );

    let imported = import_fixture();
    let properties = imported.presentation.slides()[0]
        .shapes
        .children
        .iter()
        .find(|child| {
            child
                .placeholder
                .is_some_and(|slot| slot.kind == PlaceholderKind::SubTitle)
        })
        .and_then(|node| node.text.as_ref())
        .and_then(|text| text.paragraphs.first())
        .and_then(|paragraph| paragraph.properties.as_deref())
        .expect("the subtitle's paragraph states properties");
    assert_eq!(
        properties.alignment,
        Some(TextAlign::Center),
        "algn=\"ctr\" from a self-closing a:pPr"
    );
}

/// A deck PowerPoint opens, opens here too: a duplicate placeholder slot is
/// reported, not a reason to refuse the whole file.
///
/// # Why this guard exists
///
/// It was found by opening a real deck in the browser viewer, which refused the
/// whole package with "shape tree ... has two title placeholders at index
/// 4294967295" — every slide lost over an ambiguity about one shape. PowerPoint
/// leaves an orphaned placeholder behind when a slide's layout is changed, and
/// writes a 32-bit `@idx` that is an identifier rather than a sequence number, so
/// this is an ordinary file rather than a malformed one.
///
/// Two perturbations in one deck, because the validator has two rules and either
/// one alone refuses the file: a second shape in the SAME `(type, idx)` slot, and
/// a second TITLE at a different index — `title` and `ctrTitle` are one slot for
/// inheritance, so they collide with each other while their pairs differ.
#[test]
fn a_duplicate_placeholder_slot_is_reported_rather_than_refusing_the_deck() {
    // `idx="4294967295"` is the value the real file carried, kept verbatim: it is
    // within `u32` and so parses, which is exactly why it reached the validator
    // rather than being rejected as an attribute.
    let doubled = deck::deck_with(
        "ppt/slides/slide1.xml",
        SLIDE_WITH_DUPLICATE_SLOTS.as_bytes(),
    );
    let imported = import_pptx(&doubled, PackageLimits::default(), ImportLimits::default())
        .expect("a deck with a duplicate slot must OPEN, not be refused whole");

    // Every slide is still there. This is the assertion that matters: the failure
    // being fixed was not a wrong shape, it was no deck at all.
    assert_eq!(imported.presentation.slides().len(), 3);

    // The FIRST shape in the slot keeps it, because `ShapeTree::slot` returns the
    // first match — so the model now says what the engine already did.
    let first = imported.presentation.slides().first().expect("slide 1");
    let slots: Vec<(&str, u32)> = first
        .shapes
        .children
        .iter()
        .filter_map(|node| node.placeholder)
        .map(|placeholder| (placeholder.kind.token(), placeholder.index))
        .collect();
    assert_eq!(
        slots,
        vec![("ctrTitle", 0), ("subTitle", 1)],
        "the first shape in each slot keeps it; the duplicates are demoted"
    );

    // Both demotions are reported, and the shapes themselves survive as plain
    // shapes rather than being dropped — a demoted placeholder loses its
    // INHERITED geometry, which is a loss worth naming and much smaller than
    // losing the deck.
    assert_eq!(
        first.shapes.children.len(),
        5,
        "nothing is dropped: two demoted placeholders plus the three original shapes"
    );
    let entry = imported
        .report
        .entries
        .iter()
        .find(|entry| entry.feature == "ph/@idx")
        .expect("the ambiguity is reported");
    assert_eq!(
        entry.occurrences, 2,
        "once per demoted shape, not once per deck"
    );
    assert_eq!(entry.model_outcome(), ModelOutcome::Degraded);
}

/// Slide 1 with two extra placeholders: a second `subTitle idx="1"` (the same slot
/// as the real one) and a second title spelled `title` against the real
/// `ctrTitle`. Both carry the `idx="4294967295"` the real file used.
const SLIDE_WITH_DUPLICATE_SLOTS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld name="Opening">
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr/><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>One</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Subtitle 2"/><p:cNvSpPr/><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>First in presentation order</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="4" name="Orphaned Subtitle"/><p:cNvSpPr/><p:nvPr><p:ph type="subTitle" idx="1"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Left behind by a layout change</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="5" name="Orphaned Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title" idx="4294967295"/></p:nvPr></p:nvSpPr>
<p:spPr/>
<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>A second title</a:t></a:r></a:p></p:txBody>
</p:sp>
<p:sp>
<p:nvSpPr><p:cNvPr id="6" name="Accent Bar"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="1524000" y="5486400"/><a:ext cx="3048000" cy="152400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
</p:sp>
</p:spTree>
</p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#;
/// `<a:noFill/>` is READ as a third state, distinct both from an absent fill and
/// from a stated one.
///
/// # Why this needs five shapes and not one
///
/// The thing under test is a three-valued reading, and a guard that looks at one
/// shape cannot tell a three-valued reading from a two-valued one that happens to
/// agree on that shape. So every combination the fixture can produce is asserted,
/// and two of them are the ones a wrong reading collapses:
///
/// * `Authored` with **no modelled value** — slide 10's `a:gradFill` callout. This
///   is the case that proves `SlidePaint` is not `fill.is_some()` spelled
///   differently: the shape is filled in the file and unfilled in the model, and a
///   consumer that inferred the state from the value would inherit a theme fill
///   over an authored gradient.
/// * `Suppressed` beside an `Inherited` outline — slide 1's `subTitle`, which
///   states `<a:noFill/>` and no `a:ln` at all. A reader that folded the two
///   properties into one flag cannot produce this.
#[test]
fn a_shape_that_states_no_fill_is_read_as_a_third_state() {
    use casual_pres_model::{ShapeTree, SlidePaint};

    let imported = import_fixture();
    let stated = |tree: &ShapeTree, name: &str| {
        tree.children
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
            .map(|node| (node.fill, node.outline))
            .unwrap_or_else(|| panic!("the fixture carries a shape named {name}"))
    };
    let slide_one = &imported.presentation.slides()[0].shapes;
    let slide_ten = &imported.presentation.slides()[2].shapes;

    assert_eq!(
        stated(slide_one, "Transparent Overlay"),
        (SlidePaint::Suppressed, SlidePaint::Suppressed),
        "`<a:noFill/>` and `<a:ln><a:noFill/></a:ln>` are both explicit nothings"
    );
    assert_eq!(
        stated(slide_one, "Subtitle 2"),
        (SlidePaint::Suppressed, SlidePaint::Inherited),
        "a suppressed fill beside an absent outline — the pair one flag cannot hold"
    );
    assert_eq!(
        stated(slide_one, "Title 1"),
        (SlidePaint::Inherited, SlidePaint::Inherited),
        "a `<p:spPr/>` states neither, so both inherit"
    );
    assert_eq!(
        stated(slide_one, "Accent Bar"),
        (SlidePaint::Authored, SlidePaint::Authored),
        "a solid fill and a dashed outline are both stated values"
    );

    // The gradient: stated, unmodellable, and therefore `Authored` with nothing in
    // the drawing to derive that from.
    assert_eq!(
        stated(slide_ten, "Callout"),
        (SlidePaint::Authored, SlidePaint::Inherited),
        "an `a:gradFill` is a stated fill even though this build cannot hold it"
    );
    let callout_fill = slide_ten
        .children
        .iter()
        .find(|node| node.name.as_deref() == Some("Callout"))
        .and_then(|node| match &node.content {
            GroupChild::Shape(shape) => shape.fill.clone(),
            _ => None,
        });
    assert!(
        callout_fill.is_none(),
        "and it carries no modelled fill, which is what makes `Authored` \
         information the value does not already have"
    );
}

/// The `a:noFill` findings that LEAVE the report, and the ones that must stay.
///
/// Both halves, because either alone is satisfiable by accident: "it is not
/// reported" passes for a reader that stopped reading the element at all, and "it
/// is reported" passes for the reader this change replaced.
///
/// The surviving count is DERIVED from the fixture's own markup — a grouped shape
/// is the one place the distinction has nowhere to live, because a `p:grpSp`'s
/// children are bare `GroupChild`s — so the guard cannot drift from the fixture and
/// cannot be satisfied by a number someone typed.
#[test]
fn a_no_fill_is_reported_only_where_the_model_cannot_carry_it() {
    use casual_pres_model::SlidePaint;

    let imported = import_fixture();
    let (grouped_fills, grouped_outlines) = grouped_no_fill_counts();
    assert!(
        grouped_fills > 0 && grouped_outlines > 0,
        "the fixture must state `a:noFill` inside a group on both a fill and an \
         outline, or the surviving half of this guard is vacuous; it states \
         {grouped_fills} and {grouped_outlines}"
    );
    let carried = imported
        .presentation
        .slides()
        .iter()
        .flat_map(|slide| slide.shapes.children.iter())
        .filter(|node| node.fill.suppresses() || node.outline.suppresses())
        .count();
    assert!(
        carried >= 2,
        "and it must state `a:noFill` on at least two TOP-LEVEL shapes, or the \
         half that must no longer report is vacuous; it carries {carried}"
    );

    let occurrences = |feature: &str| {
        imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == feature)
            .map_or(0, |entry| entry.occurrences)
    };
    assert_eq!(
        Some(occurrences("spPr/@noFill")),
        u32::try_from(grouped_fills).ok(),
        "a top-level `a:noFill` is carried on the SlideNode and is no longer a \
         loss; only a grouped one still is, and the count must be the grouped \
         count exactly"
    );
    assert_eq!(
        Some(occurrences("ln/@noFill")),
        u32::try_from(grouped_outlines).ok(),
        "same for `a:ln/a:noFill`"
    );
    // And the states really are the ones that stopped reporting, rather than the
    // report having been quietened for all of them.
    assert!(
        imported.presentation.slides()[0]
            .shapes
            .children
            .iter()
            .any(|node| node.fill == SlidePaint::Suppressed),
        "the top-level suppression must be in the model, not merely absent from \
         the report"
    );
}

/// How many `<a:noFill/>` the fixture states INSIDE a `p:grpSp`, split into fills
/// and outlines.
///
/// Scanned from the fixture's own bytes rather than counted by hand, and it asserts
/// the spelling it depends on: the fixture writes a suppressed outline as the exact
/// string `<a:ln><a:noFill/></a:ln>`, so a reformat that broke this derivation
/// fails here loudly instead of silently returning zero.
fn grouped_no_fill_counts() -> (usize, usize) {
    const OUTLINE: &str = "<a:ln><a:noFill/></a:ln>";
    const ANY: &str = "<a:noFill/>";
    let mut fills = 0;
    let mut outlines = 0;
    let mut saw_group = false;
    for (name, bytes) in deck::deck_parts() {
        let slide_side = name.starts_with("ppt/slides/slide")
            || name.starts_with("ppt/slideLayouts/slideLayout")
            || name.starts_with("ppt/slideMasters/slideMaster");
        if !slide_side || !name.ends_with(".xml") {
            continue;
        }
        let part = String::from_utf8(bytes).expect("the fixture's parts are UTF-8");
        let mut rest = part.as_str();
        while let Some(start) = rest.find("<p:grpSp>") {
            saw_group = true;
            // The fixture nests no group inside a group, so the first closing tag
            // ends this one. A nested group would make these UNDER-count, which is
            // the safe direction: the caller asserts the counts are non-zero and
            // compares them against the report, so an undercount fails rather than
            // passes.
            let body = &rest[start..];
            let end = body.find("</p:grpSp>").unwrap_or(body.len());
            let group = &body[..end];
            let group_outlines = group.matches(OUTLINE).count();
            outlines += group_outlines;
            fills += group.matches(ANY).count() - group_outlines;
            // Step past the closing tag so the loop cannot spin on it.
            rest = body[end..].strip_prefix("</p:grpSp>").unwrap_or("");
        }
    }
    assert!(
        saw_group,
        "the fixture must carry a `p:grpSp`, or this derivation is reading the \
         wrong parts"
    );
    (fills, outlines)
}

/// A table CELL's `a:noFill` is still a loss, and the report names which construct
/// lost it.
///
/// `TableCellProperties` carries an `Option<Fill>` and four `Option<ShapeStroke>`
/// edges, so "states nothing" and "states `a:noFill`" collapse there exactly as they
/// did on a shape before `SlidePaint` — and it matters for the same reason: a cell
/// that suppresses its fill must not take its table style's band fill, and one that
/// suppresses its right border must not take the style's grid line.
///
/// The names are asserted, not just the presence: `tcPr/@noFill` and `lnR/@noFill`
/// rather than one `spPr/@noFill` for both. Charging a cell's loss to `spPr` would
/// make a deck look as though it had lost a shape's fill, and charging an edge to
/// `ln` would make it indistinguishable from a shape's outline — which is what this
/// reader did until the shape half was fixed.
#[test]
fn a_cell_that_states_no_fill_is_reported_by_the_construct_that_lost_it() {
    let imported = import_fixture();
    let features = features(&imported);
    for expected in ["tcPr/@noFill", "lnR/@noFill"] {
        assert!(
            features.contains(&expected),
            "a cell's suppressed fill and suppressed edge are both still losses, \
             and the report must name each: {features:?}"
        );
    }
    // And the cell really did arrive with nothing, rather than the finding being
    // reported over a value that survived.
    let cell = imported.presentation.slides()[2]
        .shapes
        .children
        .iter()
        .find_map(|node| node.table.as_ref())
        .and_then(|table| table.rows.last())
        .and_then(|row| row.cells.first())
        .expect("the fixture's table has a bottom-left cell");
    let properties = &cell.properties;
    assert_eq!(
        properties.fill, None,
        "the suppressed fill arrives as nothing"
    );
    assert_eq!(
        properties.border_right, None,
        "and so does the suppressed right edge — which is exactly why both are \
         reported: nothing here distinguishes them from a cell that stated neither"
    );
}
