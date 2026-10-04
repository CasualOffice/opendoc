// SPDX-License-Identifier: Apache-2.0

//! A real `.pptx` opens, lays out, and paints — through both lanes at once.
//!
//! # Why this test exists at all
//!
//! `casual-pres-import` and `casual-pres-layout` were built on separate branches
//! against the same model, and each is guarded on its own: the importer's guards
//! drive a package into a `Presentation`, and the layout crate's drive a
//! hand-built `Presentation` into a display list. **Neither one touches the other.**
//! Two crates that compile against one type are not a pipeline, and "both lanes
//! landed" is exactly the "modelled is not shipped" claim `SKILL` §9.4 is about.
//!
//! So this is the seam, and it is the only test in the repository that asserts a
//! deck gets from bytes to paint items — now including its GLYPHS, which the
//! hand-built guards cannot speak for: a `Presentation` assembled in Rust proves
//! nothing about the cascade a real `.pptx` authors across four parts.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::display::{PaintItem, ShapeGeometry};
use casual_doc_layout::page::AnchorContent;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_package::PackageLimits;
use casual_pres_import::{ImportLimits, ImportedPresentation, import_pptx};
use casual_pres_layout::{compose_slide, lay_out_slide};

// Shared with the importer's own guards by path, not copied: two builders of one
// fixture agree only until one is edited. This is the same inclusion the crate's
// `generate_pptx_fixture` example uses.
#[allow(
    dead_code,
    reason = "the perturbation helpers belong to the import guards"
)]
#[path = "../../casual-pres-import/src/tests/deck.rs"]
mod deck;

/// The deterministic shaper: bundled faces only, so a guard means the same thing
/// on a developer's machine and on the runner.
fn shaper() -> ParleyShaper {
    ParleyShaper::without_system_fonts()
}

/// Imports some fixture bytes, or panics with the import error.
fn open(bytes: &[u8]) -> ImportedPresentation {
    import_pptx(bytes, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture deck imports")
}

/// The command count of the longest path on a composed slide, and how many shapes
/// painted as a plain rectangle. Both are derived from the display list the raster
/// backend would consume, not from the model.
fn path_profile(imported: &ImportedPresentation, index: usize) -> (usize, usize) {
    let canvas = lay_out_slide(&imported.presentation, index, &shaper())
        .unwrap_or_else(|| panic!("slide {index} lays out"));
    let list = compose_slide(&canvas);
    let mut longest = 0;
    let mut rectangles = 0;
    for item in &list.items {
        if let PaintItem::Shape { geometry, .. } = item {
            match geometry {
                ShapeGeometry::Path { commands, .. } => longest = longest.max(commands.len()),
                ShapeGeometry::Rect { .. } => rectangles += 1,
                _ => {}
            }
        }
    }
    (longest, rectangles)
}

#[test]
fn an_imported_deck_lays_out_and_paints_every_slide() {
    let imported = open(&deck::deck());
    let count = imported.presentation.slides().len();
    assert_eq!(count, 3, "the fixture is a three-slide deck");

    for index in 0..count {
        let canvas = lay_out_slide(&imported.presentation, index, &shaper())
            .unwrap_or_else(|| panic!("slide {index} lays out"));
        assert!(
            canvas.size.width.raw() > 0 && canvas.size.height.raw() > 0,
            "slide {index} resolves the deck's `p:sldSz` surface"
        );
        let list = compose_slide(&canvas);
        assert!(
            list.items
                .iter()
                .any(|item| matches!(item, PaintItem::Shape { .. } | PaintItem::Image { .. })),
            "slide {index} composed {} items and none of them paint anything",
            list.items.len()
        );
    }
}

/// A preset no typed primitive covers resolves its real outline — and a hidden
/// shape does not paint at all. One differential test, because the two facts are
/// the same fact read twice.
///
/// Slide 10's `wedgeRoundRectCallout` carries two `a:gd` adjustments and
/// `hidden="1"`. Unhiding it is the ONLY change between the two decks here, so:
///
/// * the hidden deck must show no long path — if layout stopped filtering
///   `p:cNvPr@hidden`, a shape the author hid would appear on the slide;
/// * the unhidden deck must show one — and it can only exist if the importer kept
///   the authored token through `ShapeGeometry::Other` AND layout resolved it
///   through the generated 187-preset table and its guide evaluator, which is the
///   DOCX row 0.1 work being reused by the second document class.
///
/// Written as a difference rather than as two absolute numbers because either half
/// alone is satisfiable by accident: "no long path" passes when nothing resolves,
/// and "a long path" passes when nothing filters.
#[test]
fn an_unhidden_preset_paints_its_outline_and_a_hidden_one_paints_nothing() {
    let hidden = open(&deck::deck());
    let (hidden_longest, hidden_rects) = path_profile(&hidden, 2);

    // `hidden="1"` appears once in the fixture, on this shape, so the replacement
    // cannot silently unhide something else.
    let slide_ten = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slides/slide10.xml")
            .expect("the fixture carries slide10.xml")
            .1,
    )
    .expect("the slide part is UTF-8");
    assert_eq!(
        slide_ten.matches(r#" hidden="1""#).count(),
        1,
        "the perturbation below assumes exactly one hidden shape in this part"
    );
    let shown = open(&deck::deck_with(
        "ppt/slides/slide10.xml",
        slide_ten.replace(r#" hidden="1""#, "").as_bytes(),
    ));
    let (shown_longest, shown_rects) = path_profile(&shown, 2);

    assert!(
        shown_longest > hidden_longest,
        "unhiding the callout must add a path: hidden={hidden_longest} \
         shown={shown_longest}"
    );
    assert!(
        shown_longest >= 8,
        "the callout is a rounded rectangle with a wedge tail, so its resolved \
         outline is many commands — {shown_longest} is a bounding box, which means \
         the authored preset never reached the table ({shown_rects} rectangles)"
    );
    assert_eq!(
        shown_rects, hidden_rects,
        "and the shape that appeared is the resolved PATH, not one more rectangle"
    );
}

#[test]
fn an_imported_deck_paints_the_master_before_the_slide() {
    let imported = open(&deck::deck());
    let canvas =
        lay_out_slide(&imported.presentation, 0, &shaper()).expect("the first slide lays out");
    // The three tiers are one monotonic paint order, so a master shape can never
    // land on top of the slide content it sits behind. Asserted on the ORDER the
    // canvas carries, because `compose_slide` sorts by it and a composed list
    // cannot say which tier an item came from.
    let orders: Vec<u32> = canvas.anchors.iter().map(|anchor| anchor.z.order).collect();
    assert!(
        orders.windows(2).all(|pair| pair[0] < pair[1]),
        "paint order must be strictly increasing across the three tiers: {orders:?}"
    );
    assert!(
        orders.len() >= 2,
        "the fixture's first slide inherits at least one shape from a tier above it"
    );
}

/// The glyph runs of the one text anchor whose shape is `name`'s, in paragraph
/// then line order, paired with the paragraph each came from.
fn text_profile(
    canvas: &casual_pres_layout::SlideCanvas,
    shape: usize,
) -> Vec<(usize, i32, i32, i32, usize)> {
    let anchor = canvas
        .anchors
        .iter()
        .filter(|anchor| matches!(anchor.content, AnchorContent::TextBox { .. }))
        .nth(shape)
        .unwrap_or_else(|| panic!("the canvas has no text anchor {shape}"));
    let AnchorContent::TextBox { blocks, .. } = &anchor.content else {
        unreachable!("filtered above");
    };
    let mut profile = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let BlockFragment::Paragraph { lines, .. } = block else {
            panic!("slide text flows as paragraphs");
        };
        for line in &lines.lines {
            for run in &line.runs {
                profile.push((
                    index,
                    run.origin.x.raw(),
                    run.size.raw(),
                    line.height.raw(),
                    run.glyphs.len(),
                ));
            }
        }
    }
    profile
}

/// A real deck's body text reaches positioned glyph runs, in order, with each
/// paragraph's own alignment, size and line advance.
///
/// This is the claim the crate could not make before: `casual-pres-layout`
/// emitted no glyph at all, so "a deck reaches paint items" meant shapes only.
/// Every number below is a *difference between paragraphs of one shape*, so the
/// guard cannot be satisfied by an engine that resolves one property and ignores
/// the rest:
///
/// * three paragraphs, in the authored order, with the authored glyph counts —
///   so a reader that dropped the `a:fld`'s cached text, or shaped the
///   paragraphs in list order rather than document order, fails;
/// * sizes 28/24/20pt from three different `a:rPr@sz` values against ONE master
///   tier that says 28pt at level 0 and nothing at levels 1 and 2;
/// * the third paragraph alone states `algn="r"`, so its runs must start further
///   right than the first's;
/// * the third paragraph alone states `a:lnSpc` 150%, so its line box must be
///   taller than the second's although its text is SMALLER.
#[test]
fn an_imported_decks_body_text_reaches_positioned_glyph_runs() {
    let imported = open(&deck::deck());
    let canvas =
        lay_out_slide(&imported.presentation, 1, &shaper()).expect("the second slide lays out");
    // Text anchor 0 is the title; anchor 1 is the content placeholder.
    let profile = text_profile(&canvas, 1);

    let paragraphs: Vec<usize> = profile.iter().map(|entry| entry.0).collect();
    assert_eq!(
        paragraphs,
        vec![0, 1, 2, 2],
        "three paragraphs with glyphs, the third carrying its run and the \
         `a:fld`'s cached slide number: {profile:?}"
    );
    let glyphs: Vec<usize> = profile.iter().map(|entry| entry.4).collect();
    assert_eq!(
        glyphs,
        vec![
            "Top level".len(),
            "Second level, numbered from c".len(),
            "Third level, unbulleted".len(),
            "2".len(),
        ],
        "one glyph per authored character, the field's cache included: {profile:?}"
    );
    let sizes: Vec<i32> = profile.iter().map(|entry| entry.2).collect();
    assert_eq!(
        sizes,
        vec![560, 480, 400, 400],
        "28pt, 24pt and 20pt in twips — hundredths of a point divided by five"
    );

    let (first_x, second_height) = (profile[0].1, profile[1].3);
    let (third_x, third_height) = (profile[2].1, profile[2].3);
    assert!(
        third_x > first_x,
        "only the third paragraph states `algn=\"r\"`: first={first_x} \
         third={third_x}"
    );
    assert!(
        third_height > second_height,
        "the third paragraph's 150% `a:lnSpc` must outweigh its smaller text: \
         second={second_height} third={third_height}"
    );

    // `a:pPr@marL` indents the paragraph's whole column, and the two bulleted
    // paragraphs author different margins (228,600 and 742,950 EMU), so a reader
    // that ignored `@marL` would produce one number twice.
    let AnchorContent::TextBox { blocks, .. } = &canvas
        .anchors
        .iter()
        .filter(|anchor| matches!(anchor.content, AnchorContent::TextBox { .. }))
        .nth(1)
        .expect("the content placeholder")
        .content
    else {
        unreachable!("filtered above");
    };
    let metrics: Vec<(i32, i32)> = blocks
        .iter()
        .map(|block| match block {
            BlockFragment::Paragraph { box_metrics, .. } => (
                box_metrics.indent_start.raw(),
                box_metrics.space_before.raw(),
            ),
            other => panic!("slide text flows as paragraphs: {other:?}"),
        })
        .collect();
    assert_eq!(
        metrics,
        vec![(360, 0), (1_170, 0), (0, 120), (360, 0)],
        "`@marL` 228,600 and 742,950 EMU at 635 EMU per twip, and the third \
         paragraph's `a:spcBef` of 600 hundredths of a point as 120 twips; the \
         third states no `@marL` and the empty fourth inherits the shape's \
         level-1 360"
    );
}

/// A title's size comes from its run, through a cascade whose two tiers say
/// something else, and is then scaled by the producer's autofit record.
///
/// The competition is read out of the model first, so the guard states what it
/// is rather than asserting a bare number: the layout's `ctrTitle` placeholder
/// says 60pt and the master's `p:titleStyle` says 44pt while the run says 44pt
/// and the body records a 92.5% `fontScale`. The only answer consistent with all
/// four is 814 twips, and each wrong fold is a different nameable number — 1,200
/// for the layout tier, 880 for an ignored `fontScale`.
#[test]
fn an_imported_titles_size_resolves_through_the_run_and_the_autofit_record() {
    let imported = open(&deck::deck());
    let slide = imported
        .presentation
        .slides()
        .first()
        .expect("the deck has slides");
    let layout = imported
        .presentation
        .layout_of(slide)
        .expect("the slide's layout resolves");
    let layout_size = layout
        .shapes
        .slot(casual_pres_model::PlaceholderKind::CtrTitle, 0)
        .and_then(|node| node.text.as_ref())
        .and_then(|text| text.list_style.level(0))
        .and_then(|level| level.default_character.as_deref())
        .and_then(|character| character.size_hundredths_point);
    assert_eq!(
        layout_size,
        Some(6_000),
        "the layout tier must disagree with the run, or this proves nothing"
    );
    let master_size = imported
        .presentation
        .master_of(layout)
        .and_then(|master| master.text_styles.title.level(0))
        .and_then(|level| level.default_character.as_deref())
        .and_then(|character| character.size_hundredths_point);
    assert_eq!(master_size, Some(4_400), "and so must the master tier");

    let canvas =
        lay_out_slide(&imported.presentation, 0, &shaper()).expect("the first slide lays out");
    let title = text_profile(&canvas, 0);
    assert_eq!(title.len(), 1, "the title is one run: {title:?}");
    assert_eq!(
        title[0].2, 814,
        "44pt is 880 twips and the recorded 92,500 thousandths of a percent \
         takes it to 814; 1,200 would mean the layout tier won"
    );
}

/// A `+mj-lt` typeface RESOLVES to the family the theme names, and nothing on this
/// slide is left unresolved.
///
/// # Why this guard was inverted rather than deleted
///
/// It asserted the opposite until the theme reader landed: `+mj-lt` was carried out
/// as an unresolved property, because nothing read `a:fontScheme` and reporting the
/// token beat substituting a guess. Two lanes then landed in parallel — one shaping
/// slide text, one reading the theme — and this is the guard that caught the
/// combination: each branch was green and the merge was red, which is exactly the
/// `SKILL` §5a hazard with no textual conflict for git to find.
///
/// So the assertion now says what is true, and keeps BOTH halves of the claim: the
/// theme names `Calibri Light` (read from the fixture's own bytes, so the
/// resolution is of something real) and the slide's canvas reports nothing (so the
/// resolution happened rather than the report merely being dropped).
#[test]
fn an_imported_theme_typeface_resolves_to_the_family_the_theme_names() {
    let theme = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/theme/theme1.xml")
            .expect("the fixture carries a theme part")
            .1,
    )
    .expect("the theme part is UTF-8");
    assert!(
        theme.contains(r#"<a:majorFont><a:latin typeface="Calibri Light""#),
        "the fixture's theme must name a major font, or resolving it proves nothing"
    );

    let imported = open(&deck::deck());
    let major = casual_pres_model::Typeface {
        name: "+mj-lt".to_owned(),
        panose: None,
    };
    assert_eq!(
        imported
            .presentation
            .resolve_typeface(&major)
            .map(str::to_owned),
        Some("Calibri Light".to_owned()),
        "the major latin font is what +mj-lt names"
    );
    // The control: a concrete family resolves to itself, so the function is not
    // simply answering the major font for everything.
    let concrete = casual_pres_model::Typeface {
        name: "Georgia".to_owned(),
        panose: None,
    };
    assert_eq!(
        imported
            .presentation
            .resolve_typeface(&concrete)
            .map(str::to_owned),
        Some("Georgia".to_owned()),
        "a stated family is its own answer"
    );
    // The run still STORES the token: resolving it into the run would turn a theme
    // reference into an authored font, which survives a reopen and stops following
    // the theme.
    let canvas =
        lay_out_slide(&imported.presentation, 0, &shaper()).expect("the first slide lays out");
    let reported: Vec<&casual_pres_layout::UnresolvedProperty> = canvas
        .unresolved
        .iter()
        .map(|entry| &entry.property)
        .collect();
    assert!(
        reported.is_empty(),
        "nothing on this slide is unresolved now — every size resolves and the one \
         theme reference does too: {reported:?}"
    );
}

/// The composed display list carries the glyphs, after the shape they sit on.
///
/// The canvas is layout's own vocabulary; this is the claim that a renderer sees
/// them. Order matters as much as presence: a `PaintItem::Glyphs` emitted before
/// its shape's `PaintItem::Shape` would be painted over by the shape's fill, so
/// the text would be invisible on every filled placeholder in the deck.
#[test]
fn an_imported_deck_composes_glyph_paint_items_over_its_shapes() {
    let imported = open(&deck::deck());
    let canvas =
        lay_out_slide(&imported.presentation, 0, &shaper()).expect("the first slide lays out");
    let list = compose_slide(&canvas);

    let glyph_items: Vec<usize> = list
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, PaintItem::Glyphs { .. }))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        glyph_items.len(),
        3,
        "the title's one run and the subtitle's two lines: {} items composed",
        list.items.len()
    );
    let first_shape = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::Shape { .. }))
        .expect("the slide paints shapes too");
    assert!(
        glyph_items[0] > first_shape,
        "glyphs must compose after the shapes they sit on: \
         first shape at {first_shape}, first glyph run at {}",
        glyph_items[0]
    );
    let total: usize = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => Some(run.glyphs.len()),
            _ => None,
        })
        .sum();
    assert_eq!(
        total,
        "One".len() + "First in presentation order".len() + "second line".len(),
        "every authored character reaches a paint item"
    );
}

/// A slide's `ctrTitle` inherits from a master's `title`, because the two are ONE
/// slot for inheritance.
///
/// # Why the fixture has to be perturbed
///
/// `title` and `ctrTitle` differ only in where the layout puts the box — a centred
/// title is still the title placeholder — and `PlaceholderKind::is_title` folds
/// them. The SLOT lookup did not, so a slide's `ctrTitle` looked up
/// `(CtrTitle, 0)`, a master carries `title`, and the lookup missed: every title
/// slide inherited neither its geometry nor its text tiers from the master.
///
/// The plain fixture cannot show it, because its title LAYOUT carries a `ctrTitle`
/// with a full `a:xfrm`, so the slide matches at the layout tier and the master is
/// never consulted. This removes the layout's `ctrTitle` so the lookup must fall
/// through to the master's `title` — and without the fold it falls through to
/// nothing.
#[test]
fn a_slide_ctr_title_inherits_from_the_masters_title_slot() {
    let layout_one = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slideLayouts/slideLayout1.xml")
            .expect("the fixture carries slideLayout1.xml")
            .1,
    )
    .expect("the layout part is UTF-8");

    // Retype the layout's own title slot so it no longer matches the slide's. The
    // master's `title` then becomes the only candidate, which is the case under
    // test; retyping is safer than deleting the shape, which would also change the
    // tree's child count and the paint order.
    let before = r#"<p:ph type="ctrTitle"/>"#;
    assert_eq!(
        layout_one.matches(before).count(),
        1,
        "one ctrTitle slot on the title layout"
    );
    let layout_one = layout_one.replace(before, r#"<p:ph type="ftr" idx="9"/>"#);

    let mut parts = deck::deck_parts();
    parts
        .iter_mut()
        .find(|(name, _)| name == "ppt/slideLayouts/slideLayout1.xml")
        .expect("the part exists")
        .1 = layout_one.into_bytes();
    let imported = open(&deck::build_pptx(&parts));

    let slide = &imported.presentation.slides()[0];
    let title = slide.shapes.title().expect("slide 1 fills a title slot");
    assert!(
        title
            .placeholder
            .is_some_and(|slot| slot.kind == casual_pres_model::PlaceholderKind::CtrTitle),
        "and it is the ctrTitle form, or this guard is testing the wrong shape"
    );

    // Asserted on the LAID-OUT rect, not on `resolve_slot`. That function answers
    // "which shape fills this slot", and the slide's own shape does — with an empty
    // `p:spPr`. The geometry cascade is a separate resolution, and the only place it
    // is observable is the canvas, which is also the only place it matters.
    //
    // The geometry half of this does NOT depend on `ShapeTree::slot`'s title fold:
    // measured, and the fold can be reverted with this assertion still green,
    // because the shaping lane's own `inherited_geometry` matches the two title
    // tokens itself. The TEXT tiers are where the fold bites, and the second half of
    // this guard is that — a master `title` tier the slide's `ctrTitle` can only see
    // through the fold.
    let canvas = lay_out_slide(&imported.presentation, 0, &shaper()).expect("the slide lays out");
    let placed = canvas
        .anchors
        .iter()
        .find(|anchor| anchor.node == Some(title.id()))
        .expect("the title shape is placed");

    // The master's own title box in EMU is off (838200, 365126), ext
    // 10515600 x 1325563; at 635 EMU per twip that is (1320, 575) and
    // 16560 x 2088. The slide states no geometry at all, so this can only have come
    // from the tier above — and only through the title fold, because the layout's
    // own title slot was retyped away above.
    assert_eq!(
        (
            placed.rect.origin.x.raw(),
            placed.rect.origin.y.raw(),
            placed.rect.size.width.raw(),
            placed.rect.size.height.raw()
        ),
        (1320, 575, 16560, 2088),
        "the placed title takes the MASTER's title box"
    );
    assert_ne!(
        placed.rect.size.width.raw(),
        0,
        "and not the zero box the slide itself states"
    );

    // The text half. The master's `title` placeholder carries an `a:lstStyle` with
    // a level-1 right margin nothing else in the deck states, so the value can only
    // reach the slide's `ctrTitle` if the slot lookup folded the two tokens.
    let master = String::from_utf8(
        deck::deck_parts()
            .into_iter()
            .find(|(name, _)| name == "ppt/slideMasters/slideMaster1.xml")
            .expect("the fixture carries the master")
            .1,
    )
    .expect("the master part is UTF-8");
    let plain = r#"<p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
<p:spPr><a:xfrm><a:off x="838200" y="365126"/><a:ext cx="10515600" cy="1325563"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
<p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#;
    assert!(
        master.contains(plain),
        "the master's title slot moved; this perturbation is anchored on it"
    );
    let enriched = plain.replace(
        "<a:lstStyle/>",
        r#"<a:lstStyle><a:lvl1pPr marR="123456"/></a:lstStyle>"#,
    );
    let master = master.replace(plain, &enriched);

    parts
        .iter_mut()
        .find(|(name, _)| name == "ppt/slideMasters/slideMaster1.xml")
        .expect("the part exists")
        .1 = master.into_bytes();
    let imported = open(&deck::build_pptx(&parts));
    let slide = &imported.presentation.slides()[0];
    let title = slide.shapes.title().expect("slide 1 fills a title slot");
    let resolved = imported.presentation.text_cascade(slide, title).resolve(0);
    assert_eq!(
        resolved.paragraph.margin_right_emu,
        Some(123_456),
        "the master's TITLE slot tier reaches a slide's ctrTitle — without the fold \
         the slot lookup misses and this is None"
    );
}
