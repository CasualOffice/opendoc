// SPDX-License-Identifier: Apache-2.0

//! **The one rule** that turns a square-family float's wrap band into a line
//! exclusion: which inline edge of the measure the exclusion sits on, and how
//! wide it is.
//!
//! Three call sites used to answer that question independently — the
//! paragraph-local marker and the cross-paragraph carry in
//! [`crate::flow`], and the page-level cross-paragraph pass in
//! [`crate::document_layout`] — and they disagreed about the same float. That
//! divergence is what this module exists to remove (`SKILL.md` §8, *prefer one
//! mechanism over two*); every side-and-width decision in the engine now goes
//! through [`band_exclusion`].
//!
//! ## What the rule is
//!
//! The authored answer is `w:wrap@wrapText` ([`WrapSides`]), not geometry. A
//! float's band is subtracted from the measure on the side the author said text
//! may **not** occupy:
//!
//! | `wrapText` | text occupies | exclusion |
//! | --- | --- | --- |
//! | `left` | the room left of the float | the right edge, `measure_end - band_start` |
//! | `right` | the room right of the float | the left edge, `band_end - measure_start` |
//! | `largest` | the wider of the two gaps | the narrower gap plus the band |
//! | `bothSides` (default) | both gaps | approximated as `largest` — see below |
//!
//! ## The one documented approximation
//!
//! `bothSides` means Word splits each intersecting line into two segments, one
//! in each gap. Our line geometry is a single measure with a left and a right
//! inset ([`crate::text::InlineFloatSpec`], applied by
//! `shape::break_lines_around_floats`), so a *hole* in the middle of a line is
//! not representable. Rather than guess, `bothSides` keeps the **wider** gap —
//! identical to `largest`. Recorded rather than hidden, per `SKILL.md` §8:
//! text in the narrower gap is lost, but the measure that survives is the
//! larger one, so a mid-column float never eats the whole line.
//!
//! This replaces a midpoint guess that compared the float's centre with the
//! paragraph's centre and then excluded from the paragraph's own edge all the
//! way to the float's *far* side. That had two consequences the owner saw: a
//! float anywhere but hard against a margin **discarded the whole gap between
//! the margin and the float** (so a drag into mid-column lost most of the
//! measure), and dragging a float across the column centre made the surrounding
//! text **jump from indented-left to indented-right** on a one-twip crossing.

use casual_doc_model::v1::DrawingAnchor;

use crate::text::InlineFloatSide;
use crate::units::Twip;

/// Which sides of a square-family float text may occupy — Word's
/// `w:wrap@wrapText` (`bothSides` / `left` / `right` / `largest`).
///
/// [`Self::BothSides`] is the default because it is Word's: when `wrapText` is
/// absent the float wraps on both sides.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "`Left`/`Right`/`Largest` are constructed by `wrap_sides` as soon as \
              `DrawingAnchor` carries `wrapText`; the arithmetic that consumes them \
              is complete and guarded by this module's own tests, so they are kept \
              here rather than added later alongside a second edit to the rule."
)]
pub(crate) enum WrapSides {
    /// `bothSides` — text in both gaps. The engine approximates this as
    /// [`Self::Largest`]; see the module docs.
    #[default]
    BothSides,
    /// `left` — text only in the room to the left of the float.
    Left,
    /// `right` — text only in the room to the right of the float.
    Right,
    /// `largest` — text only in the wider of the two gaps.
    Largest,
}

/// One float's resolved line exclusion: the inline edge it occupies and how far
/// into the measure it reaches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SideExclusion {
    /// The measure edge the exclusion is measured from.
    pub(crate) side: InlineFloatSide,
    /// Exclusion width from that edge, always at least one twip and always
    /// leaving at least one twip of measure for text.
    pub(crate) width: Twip,
}

/// The authored `w:wrap@wrapText` of one anchor.
///
/// **Temporary seam, deliberately one function wide.** `DrawingAnchor` does not
/// carry `wrapText` yet — the model, import and export half is landing
/// separately — so today every anchor reports the documented fallback,
/// [`WrapSides::BothSides`], which is Word's own default when the attribute is
/// absent. When the field lands, this body becomes the one-line map
/// `anchor.wrap_text.map_or(WrapSides::BothSides, WrapSides::from)` and nothing
/// else in the engine changes, because every exclusion already asks this
/// function rather than guessing from geometry.
///
/// O(1).
pub(crate) fn wrap_sides(_anchor: &DrawingAnchor) -> WrapSides {
    WrapSides::BothSides
}

/// Resolves one float's wrap band against one measure into the line exclusion
/// it imposes, or `None` when the band does not overlap the measure at all (an
/// object parked in the page margin, or pushed outside the column by a negative
/// `posOffset`, narrows nothing).
///
/// `band_start`/`band_end` are the float's horizontal extent **including its
/// authored `w:distL`/`w:distR` wrap distances**; `measure_start`/`measure_end`
/// are the text box the exclusion will apply to. Both pairs must be in the same
/// coordinate space — page-absolute for the page-level pass, container-relative
/// for the paragraph-local ones; the rule is translation-invariant, so either
/// works as long as the two agree.
///
/// The returned width always leaves at least one twip of measure, so a float
/// wider than its column pushes text down rather than producing a zero-width
/// line.
///
/// O(1) — pure arithmetic on four coordinates, no document access.
pub(crate) fn band_exclusion(
    band_start: Twip,
    band_end: Twip,
    measure_start: Twip,
    measure_end: Twip,
    sides: WrapSides,
) -> Option<SideExclusion> {
    let measure_width = measure_end.raw() - measure_start.raw();
    if measure_width <= 0 || band_end <= measure_start || band_start >= measure_end {
        return None;
    }
    // The two gaps the float leaves inside the measure. Either can be negative
    // when the band overhangs that edge.
    let gap_before = band_start.raw() - measure_start.raw();
    let gap_after = measure_end.raw() - band_end.raw();
    let text_before = match sides {
        WrapSides::Left => true,
        WrapSides::Right => false,
        // The tie goes to the leading gap so a centred float resolves
        // deterministically instead of depending on rounding.
        WrapSides::Largest | WrapSides::BothSides => gap_before >= gap_after,
    };
    let (side, raw_width) = if text_before {
        // Text keeps the leading gap, so the exclusion hugs the trailing edge
        // and reaches back to the float's near (leading) side.
        (InlineFloatSide::Right, measure_end.raw() - band_start.raw())
    } else {
        (InlineFloatSide::Left, band_end.raw() - measure_start.raw())
    };
    // Both branches are positive given the overlap test above; the clamp only
    // bounds a band that overhangs the far edge.
    let width = Twip(raw_width.clamp(1, (measure_width - 1).max(1)));
    Some(SideExclusion { side, width })
}

#[cfg(test)]
mod tests {
    use super::{WrapSides, band_exclusion};
    use crate::text::InlineFloatSide;
    use crate::units::Twip;

    /// A 9,000-twip measure with a 2,000-twip float band placed inside it.
    fn resolve(band_start: i32, band_end: i32, sides: WrapSides) -> (InlineFloatSide, i32) {
        let exclusion = band_exclusion(
            Twip(band_start),
            Twip(band_end),
            Twip::ZERO,
            Twip(9_000),
            sides,
        )
        .expect("band overlaps the measure");
        (exclusion.side, exclusion.width.raw())
    }

    #[test]
    fn a_flush_leading_band_excludes_the_leading_edge() {
        assert_eq!(
            resolve(0, 2_000, WrapSides::BothSides),
            (InlineFloatSide::Left, 2_000)
        );
    }

    #[test]
    fn a_flush_trailing_band_excludes_the_trailing_edge() {
        assert_eq!(
            resolve(7_000, 9_000, WrapSides::BothSides),
            (InlineFloatSide::Right, 2_000)
        );
    }

    #[test]
    fn a_mid_column_band_keeps_the_wider_gap_rather_than_the_whole_near_side() {
        // Band at 5,000..7,000: 5,000 before, 2,000 after. Text keeps the
        // leading 5,000 and the exclusion is 9,000-5,000 = 4,000 on the right.
        assert_eq!(
            resolve(5_000, 7_000, WrapSides::BothSides),
            (InlineFloatSide::Right, 4_000)
        );
        // Mirror image: 2,000 before, 5,000 after.
        assert_eq!(
            resolve(2_000, 4_000, WrapSides::BothSides),
            (InlineFloatSide::Left, 4_000)
        );
    }

    #[test]
    fn authored_sides_override_the_geometry() {
        // Same band both times; only `wrapText` differs.
        assert_eq!(
            resolve(5_000, 7_000, WrapSides::Right),
            (InlineFloatSide::Left, 7_000)
        );
        assert_eq!(
            resolve(2_000, 4_000, WrapSides::Left),
            (InlineFloatSide::Right, 7_000)
        );
    }

    #[test]
    fn a_band_outside_the_measure_excludes_nothing() {
        assert!(
            band_exclusion(
                Twip(-3_000),
                Twip(-100),
                Twip::ZERO,
                Twip(9_000),
                WrapSides::BothSides
            )
            .is_none()
        );
        assert!(
            band_exclusion(
                Twip(9_000),
                Twip(11_000),
                Twip::ZERO,
                Twip(9_000),
                WrapSides::BothSides
            )
            .is_none()
        );
    }

    #[test]
    fn a_band_wider_than_the_measure_still_leaves_a_twip() {
        let exclusion = band_exclusion(
            Twip(-500),
            Twip(9_500),
            Twip::ZERO,
            Twip(9_000),
            WrapSides::BothSides,
        )
        .expect("overlapping band");
        assert_eq!(exclusion.width, Twip(8_999));
    }
}
