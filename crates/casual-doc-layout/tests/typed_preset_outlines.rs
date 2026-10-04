// SPDX-License-Identifier: Apache-2.0

//! Pins the eighteen typed preset geometries that used to carry hand-written
//! vertices and are now resolved from the committed ECMA-376 table.
//!
//! # What this replaced, and why it is safe
//!
//! `preset_polygon` hand-coded a vertex list per typed geometry, shadowing the same
//! preset's entry in `shape_preset_table.txt` — two implementations of one rule,
//! which `SKILL` §8 says diverge. They were measured against each other before
//! either was deleted: for **all eighteen**, the hand-written vertices and the
//! specification's own produce the **identical set of points**. Seven looked like
//! they differed by as much as the full width of the box until the comparison
//! stopped treating a closed path as a sequence — they begin at a different vertex
//! and wind the other way, which is the same shape.
//!
//! So the collapse deleted a duplicate rather than changing what is drawn. This
//! guard is what remains of that measurement: it cannot compare against vertices
//! that no longer exist, so instead it pins each token's presence and shape in the
//! table. A preset silently dropping out of the table, or growing a different
//! number of segments, fails here instead of quietly painting a bounding rectangle.

use casual_doc_layout::shape_preset::{preset_is_closed, preset_outline};
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::v1::ShapeGeometry;

/// A deliberately non-square box, so a shape that ignored one axis would show up.
fn rect() -> Rect {
    Rect::new(
        Point::new(Twip(0), Twip(0)),
        Size::new(Twip(1000), Twip(500)),
    )
}

/// The four typed geometries that stay as primitives, because a path cannot carry
/// what they do: a true ellipse rather than a bezier approximation, a rounded
/// rectangle with its own radius, a line with head and tail arrow decorations, and
/// the plain rectangle.
const PRIMITIVES: [ShapeGeometry; 4] = [
    ShapeGeometry::Rectangle,
    ShapeGeometry::RoundRectangle,
    ShapeGeometry::Ellipse,
    ShapeGeometry::Line,
];

#[test]
fn every_typed_polygonal_preset_resolves_from_the_table() {
    let mut resolved = 0_usize;
    let mut unresolved: Vec<&str> = Vec::new();
    for geometry in ShapeGeometry::TYPED {
        if PRIMITIVES.contains(&geometry) || geometry == ShapeGeometry::Other {
            continue;
        }
        let token = geometry
            .preset_token()
            .expect("a typed geometry names its own preset token");
        match preset_outline(token, &[], rect()) {
            Some(commands) => {
                assert!(
                    commands.len() >= 3,
                    "{token} resolved to {} commands, which cannot enclose an area",
                    commands.len()
                );
                assert!(
                    preset_is_closed(token),
                    "{token} is a filled shape and must close"
                );
                resolved += 1;
            }
            None => unresolved.push(token),
        }
    }
    // Derived from the model, not written down: `TYPED` is the twenty-two typed
    // geometries and already excludes `Other`, so the polygonal count is simply
    // those minus the four kept as primitives. (It is NOT `- 1` for `Other`; that
    // off-by-one is what this guard's first run caught.)
    let expected = ShapeGeometry::TYPED.len() - PRIMITIVES.len();
    assert_eq!(
        unresolved,
        Vec::<&str>::new(),
        "these typed presets no longer resolve from the table, so they would paint a \
         bounding rectangle: {unresolved:?}"
    );
    assert_eq!(resolved, expected, "expected {expected} polygonal presets");
    assert_eq!(
        resolved, 18,
        "the measured count when the collapse was made"
    );
}

#[test]
fn each_typed_presets_outline_stays_inside_its_box() {
    // The collapse moved these from vertices clamped by hand to the specification's
    // own guide arithmetic. A preset whose guides resolved wrongly would most likely
    // show up as a point outside the shape's own rectangle, which nothing downstream
    // clips — so it is asserted here rather than assumed.
    let rect = rect();
    for geometry in ShapeGeometry::TYPED {
        if PRIMITIVES.contains(&geometry) || geometry == ShapeGeometry::Other {
            continue;
        }
        let token = geometry.preset_token().expect("a token");
        let commands = preset_outline(token, &[], rect).expect("resolves");
        for command in &commands {
            for point in command.points() {
                assert!(
                    point.x >= rect.origin.x
                        && point.x <= rect.right()
                        && point.y >= rect.origin.y
                        && point.y <= rect.bottom(),
                    "{token} puts {point:?} outside its own box {rect:?}"
                );
            }
        }
    }
}

#[test]
fn the_four_primitives_are_deliberately_not_routed_through_the_table() {
    // Each of these IS in the table, so the exclusion is a choice and not an
    // accident — and this guard is what makes it a stated choice. Painting them as
    // paths would cost a true ellipse its exactness and a line its arrow ends.
    for geometry in PRIMITIVES {
        let token = geometry.preset_token().expect("a token");
        assert!(
            preset_outline(token, &[], rect()).is_some(),
            "{token} is in the table; it is excluded by choice, not absence"
        );
    }
}
