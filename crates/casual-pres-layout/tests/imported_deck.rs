// SPDX-License-Identifier: Apache-2.0

//! A real `.pptx` opens, lays out, and paints — through both lanes at once.
//!
//! # Why this test exists at all
//!
//! `casual-pres-import` and `casual-pres-layout` were built on separate branches
//! against the same model, and each is guarded on its own: the importer's 22 guards
//! drive a package into a `Presentation`, and the layout crate's 12 drive a
//! hand-built `Presentation` into a display list. **Neither one touches the other.**
//! Two crates that compile against one type are not a pipeline, and "both lanes
//! landed" is exactly the "modelled is not shipped" claim `SKILL` §9.4 is about.
//!
//! So this is the seam, and it is the only test in the repository that asserts a
//! deck gets from bytes to paint items.

use casual_doc_layout::display::{PaintItem, ShapeGeometry};
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

/// Imports some fixture bytes, or panics with the import error.
fn open(bytes: &[u8]) -> ImportedPresentation {
    import_pptx(bytes, PackageLimits::default(), ImportLimits::default())
        .expect("the fixture deck imports")
}

/// The command count of the longest path on a composed slide, and how many shapes
/// painted as a plain rectangle. Both are derived from the display list the raster
/// backend would consume, not from the model.
fn path_profile(imported: &ImportedPresentation, index: usize) -> (usize, usize) {
    let canvas = lay_out_slide(&imported.presentation, index)
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
        let canvas = lay_out_slide(&imported.presentation, index)
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
    let canvas = lay_out_slide(&imported.presentation, 0).expect("the first slide lays out");
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
