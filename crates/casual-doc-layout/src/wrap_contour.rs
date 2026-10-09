// SPDX-License-Identifier: Apache-2.0

//! **Tight and through wrap follow the authored contour** (`wp:wrapPolygon`)
//! instead of the object's bounding box (`docs/109` FID-L-12, `docs/119`
//! "Tight and through wrap follow the contour").
//!
//! ## Named prior art
//!
//! This is the float area of CSS Shapes' `shape-outside: polygon(…)`: for each
//! line, the region a float takes away is the polygon's horizontal extent
//! **within that line's vertical band**, not the polygon's bounding box. The
//! decomposition is the textbook one for a polygon with straight edges: over
//! any horizontal slab, each edge that crosses it is one linear segment, so the
//! polygon's extent over the slab is reached at the slab's top or bottom edge
//! and is computed exactly from the edges, not sampled. The slabs are cut at
//! every vertex height (so a stepped contour steps exactly where it was drawn)
//! and on a regular grid of [`GRID_BANDS`] slabs over the contour's height (so a
//! sloped edge — an ellipse, a diamond — narrows the lines near its tip instead
//! of one tall slab taking the extent of its widest edge).
//!
//! The line breaker consumes each band as an ordinary side exclusion that
//! starts part-way down the paragraph (`InlineFloatSpec::top`); how a line
//! learns its own height before its geometry is final is the other half of the
//! mechanism and lives in `shape::break_lines_around_floats`.
//!
//! ## The coordinate space is Word's, not the schema's
//!
//! The schema types `wp:start`/`wp:lineTo` as `ST_Coordinate` (EMU). Word does
//! not write EMU there: it writes a **21,600 × 21,600 space spanning the
//! object's extent** (the space VML shapes used), so a full-box contour reads
//! `0,0 → 0,21600 → 21600,21600 → 21600,0` whatever the picture's size, and
//! other producers that interoperate with Word (LibreOffice's DOCX filter among
//! them) read and write it the same way. A contour is mapped onto the object's
//! laid-out rectangle by that rule.
//!
//! ## What is deliberately approximated
//!
//! - **One side per float.** The side the exclusion sits on is decided once,
//!   from the contour's whole horizontal extent, by the same
//!   [`band_exclusion`] rule a square float uses — so a contour narrows the
//!   lines beside it, and never flips the text from one side to the other
//!   between bands. A concave contour's inner notch on the far side is not
//!   used; Word fills it, this engine does not (one measure per line, the
//!   `bothSides` approximation recorded in [`crate::wrap_side`]).
//! - **`distT`/`distB` do not apply to a contour.** Word's own Layout dialog
//!   offers only Left and Right distances for Tight and Through wrap; the
//!   contour's own top and bottom bound it vertically. `distL`/`distR` widen
//!   every band.
//! - **A rotated object's contour is mapped onto its unrotated box.**
//! - **Resolution.** Within one slab the exclusion is the slab's widest
//!   extent, so a line sees the contour to within one slab's height — 1/32 of
//!   the contour, never less than [`MIN_BAND_HEIGHT`].
//! - **Bounded work.** A contour of more than [`MAX_CONTOUR_POINTS`] vertices
//!   falls back to the bounding box (the square wrap this replaced), and a
//!   contour is cut into at most [`MAX_CONTOUR_BANDS`] slabs — more cut heights
//!   than that are thinned evenly, and a slab spanning several takes the union
//!   of their extents, which over-excludes (the safe direction) and never
//!   under-excludes.

use casual_doc_model::v1::{DrawingAnchor, PointEmu, WrapMode};

use crate::text::InlineFloatSide;
use crate::units::{Rect, Twip};
use crate::wrap_side::{WrapSides, band_exclusion};

/// The side length of the square coordinate space Word writes a
/// `wp:wrapPolygon` in, mapped onto the object's extent.
pub(crate) const WORD_POLYGON_SPAN: f64 = 21_600.0;

/// The most vertices a contour may have and still be followed; a longer one
/// wraps as its bounding box. Word's own contours are tens of points.
pub(crate) const MAX_CONTOUR_POINTS: usize = 1_024;

/// How many slabs of equal height a contour's height is cut into, on top of the
/// cuts at its own vertex heights.
pub(crate) const GRID_BANDS: i64 = 32;

/// The thinnest grid slab, in twips (one point): a small object is not cut
/// finer than any line of text could use.
pub(crate) const MIN_BAND_HEIGHT: i64 = 20;

/// The most vertical bands one contour is cut into, grid and vertex cuts
/// together.
pub(crate) const MAX_CONTOUR_BANDS: usize = 64;

/// How far outside the 21,600 space a vertex is still taken at face value.
/// Word writes vertices slightly outside it (a contour hugging an edge), but a
/// coordinate hundreds of spans away is nonsense; clamping keeps the integer
/// arithmetic below far from overflow.
const MAX_POLYGON_COORDINATE: i64 = 100 * 21_600;

/// One horizontal slab of a contour: between `top` and `bottom` the contour
/// spans `start..end`. All four in the coordinate space of the rectangle the
/// contour was mapped onto.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContourBand {
    /// The band's top edge.
    pub(crate) top: Twip,
    /// The band's bottom edge (exclusive).
    pub(crate) bottom: Twip,
    /// The contour's leftmost point within the band.
    pub(crate) start: Twip,
    /// The contour's rightmost point within the band.
    pub(crate) end: Twip,
}

/// One band's line exclusion, measured from a caller's vertical origin (the top
/// of the paragraph it narrows).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BandExclusion {
    /// The measure edge the exclusion is measured from.
    pub(crate) side: InlineFloatSide,
    /// Exclusion width from that edge.
    pub(crate) width: Twip,
    /// Where the band starts below the origin (never negative).
    pub(crate) top: Twip,
    /// Where the band ends below the origin.
    pub(crate) bottom: Twip,
}

/// The contour an anchored object's text wraps to, mapped onto `object` (the
/// object's laid-out rectangle), or `None` when it wraps to its bounding box:
/// any wrap other than tight/through, no authored polygon, or a polygon that
/// cannot bound anything (see [`contour_bands`]).
///
/// O(n log n) in the polygon's vertex count, bounded by
/// [`MAX_CONTOUR_POINTS`].
pub(crate) fn anchor_contour(anchor: &DrawingAnchor, object: Rect) -> Option<Vec<ContourBand>> {
    if !matches!(anchor.wrap, WrapMode::Tight | WrapMode::Through) {
        return None;
    }
    contour_bands(anchor.wrap_polygon.as_deref()?, object)
}

/// Cuts a `wp:wrapPolygon` (Word's 21,600 space) mapped onto `object` into
/// vertical bands, each carrying the polygon's exact horizontal extent over it.
/// The polygon is closed implicitly (last vertex back to the first).
///
/// `None` — wrap to the bounding box — for fewer than three vertices, more
/// than [`MAX_CONTOUR_POINTS`], an object with no area, or a polygon with no
/// vertical extent.
///
/// O(n log n + n·b) for n vertices and b ≤ [`MAX_CONTOUR_BANDS`] bands.
pub(crate) fn contour_bands(polygon: &[PointEmu], object: Rect) -> Option<Vec<ContourBand>> {
    if polygon.len() < 3
        || polygon.len() > MAX_CONTOUR_POINTS
        || object.size.width.raw() <= 0
        || object.size.height.raw() <= 0
    {
        return None;
    }
    let map = |value: i64, origin: Twip, size: Twip| -> i64 {
        let value = value.clamp(-MAX_POLYGON_COORDINATE, MAX_POLYGON_COORDINATE);
        // Exact in f64 at these magnitudes (≤ 2.2e6 × 2.1e9 < 2^53);
        // `f64::round` rounds half away from zero, the crate's one rounding rule.
        i64::from(origin.raw())
            + (value as f64 * f64::from(size.raw()) / WORD_POLYGON_SPAN).round() as i64
    };
    let points: Vec<(i64, i64)> = polygon
        .iter()
        .map(|point| {
            (
                map(point.x_emu, object.origin.x, object.size.width),
                map(point.y_emu, object.origin.y, object.size.height),
            )
        })
        .collect();

    let mut cuts: Vec<i64> = points.iter().map(|&(_, y)| y).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let (Some(&top), Some(&bottom)) = (cuts.first(), cuts.last()) else {
        return None;
    };
    if bottom <= top {
        return None;
    }
    // The regular grid, so a sloped edge is followed within one slab.
    let step = ((bottom - top) / GRID_BANDS).max(MIN_BAND_HEIGHT);
    cuts.extend(
        (1..GRID_BANDS)
            .map(|k| top + k * step)
            .take_while(|&y| y < bottom),
    );
    cuts.sort_unstable();
    cuts.dedup();
    // At most MAX_CONTOUR_BANDS bands: keep evenly spaced cut heights, always
    // the first and the last. A band spanning several original ones takes the
    // union of their extents, computed directly below — never a guess.
    if cuts.len() - 1 > MAX_CONTOUR_BANDS {
        let last = cuts.len() - 1;
        cuts = (0..=MAX_CONTOUR_BANDS)
            .map(|band| cuts[band * last / MAX_CONTOUR_BANDS])
            .collect();
    }

    let mut extents: Vec<Option<(i64, i64)>> = vec![None; cuts.len() - 1];
    let widen = |slot: &mut Option<(i64, i64)>, x: i64| {
        *slot = Some(match *slot {
            Some((start, end)) => (start.min(x), end.max(x)),
            None => (x, x),
        });
    };
    for (index, &(x0, y0)) in points.iter().enumerate() {
        let (x1, y1) = points[(index + 1) % points.len()];
        // A horizontal edge is skipped: it lies on a cut (every vertex height is
        // one), and the interior it bounds lies on one side of it, so the slab
        // on that side already reaches its whole span through the edges that
        // meet it. Counting it would widen the slab on its OTHER side too — the
        // whole lower arm of an L would take the width of the upper one.
        if y0 == y1 {
            continue;
        }
        let (low, high) = (y0.min(y1), y0.max(y1));
        // The bands this edge crosses: from the one holding `low` to the last
        // one whose top is above `high`.
        let first = cuts.partition_point(|&cut| cut <= low).saturating_sub(1);
        let last = cuts.partition_point(|&cut| cut < high).min(extents.len());
        // An edge is linear, so its extent over a slab is reached at the ends
        // of the part of it inside the slab.
        let x_at = |y: i64| -> i64 {
            let t = (y - y0) as f64 / (y1 - y0) as f64;
            x0 + ((x1 - x0) as f64 * t).round() as i64
        };
        for (band, extent) in extents.iter_mut().enumerate().take(last).skip(first) {
            let clip_top = low.max(cuts[band]);
            let clip_bottom = high.min(cuts[band + 1]);
            widen(extent, x_at(clip_top));
            widen(extent, x_at(clip_bottom));
        }
    }

    let twip = |value: i64| Twip(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
    let mut bands: Vec<ContourBand> = Vec::with_capacity(extents.len());
    for (band, extent) in extents.into_iter().enumerate() {
        let Some((start, end)) = extent else {
            continue;
        };
        let next = ContourBand {
            top: twip(cuts[band]),
            bottom: twip(cuts[band + 1]),
            start: twip(start),
            end: twip(end),
        };
        // Adjacent bands with the same extent are one band.
        match bands.last_mut() {
            Some(previous)
                if previous.bottom == next.top
                    && previous.start == next.start
                    && previous.end == next.end =>
            {
                previous.bottom = next.bottom;
            }
            _ => bands.push(next),
        }
    }
    (!bands.is_empty()).then_some(bands)
}

/// The contour's whole horizontal extent, `distL`/`distR` included — the band
/// the exclusion SIDE is decided from, by the same rule a square float uses.
fn contour_hull(bands: &[ContourBand], dist_start: Twip, dist_end: Twip) -> (Twip, Twip) {
    let start = bands
        .iter()
        .map(|band| band.start)
        .min()
        .unwrap_or(Twip::ZERO);
    let end = bands
        .iter()
        .map(|band| band.end)
        .max()
        .unwrap_or(Twip::ZERO);
    (start - dist_start, end + dist_end)
}

/// The line exclusions a contour imposes on one measure, one per band that
/// reaches into it, each measured vertically from `origin_y`.
///
/// The side is resolved ONCE from the contour's whole extent through
/// [`band_exclusion`] — the authored `wrapText` rule every wrap shares — and
/// each band then excludes from that edge to its own far side: a band where the
/// contour is narrow narrows the line less. A band that ends above `origin_y`,
/// or does not reach into the measure from the resolved side, excludes nothing.
///
/// O(b) in the band count.
#[allow(clippy::too_many_arguments)]
pub(crate) fn contour_exclusions(
    bands: &[ContourBand],
    dist_start: Twip,
    dist_end: Twip,
    measure_start: Twip,
    measure_end: Twip,
    sides: WrapSides,
    origin_y: Twip,
) -> Vec<BandExclusion> {
    let (hull_start, hull_end) = contour_hull(bands, dist_start, dist_end);
    let Some(resolved) = band_exclusion(hull_start, hull_end, measure_start, measure_end, sides)
    else {
        return Vec::new();
    };
    let measure_width = measure_end.raw() - measure_start.raw();
    let mut out = Vec::with_capacity(bands.len());
    for band in bands {
        let bottom = band.bottom - origin_y;
        if bottom.raw() <= 0 {
            continue;
        }
        let raw_width = match resolved.side {
            InlineFloatSide::Left => (band.end + dist_end).raw() - measure_start.raw(),
            InlineFloatSide::Right => measure_end.raw() - (band.start - dist_start).raw(),
        };
        if raw_width <= 0 {
            continue;
        }
        out.push(BandExclusion {
            side: resolved.side,
            width: Twip(raw_width.clamp(1, (measure_width - 1).max(1))),
            top: (band.top - origin_y).max(Twip::ZERO),
            bottom,
        });
    }
    out
}

/// The vertical span a contour occupies: its first band's top to its last
/// band's bottom.
pub(crate) fn contour_span(bands: &[ContourBand]) -> Option<(Twip, Twip)> {
    Some((bands.first()?.top, bands.last()?.bottom))
}

#[cfg(test)]
mod tests {
    use super::{
        BandExclusion, ContourBand, MAX_CONTOUR_BANDS, MAX_CONTOUR_POINTS, contour_bands,
        contour_exclusions,
    };
    use crate::text::InlineFloatSide;
    use crate::units::{Point, Rect, Size, Twip};
    use crate::wrap_side::WrapSides;
    use casual_doc_model::v1::PointEmu;

    fn polygon(points: &[(i64, i64)]) -> Vec<PointEmu> {
        points
            .iter()
            .map(|&(x_emu, y_emu)| PointEmu { x_emu, y_emu })
            .collect()
    }

    /// A 2,160 × 2,160-twip object at the origin: one twip per ten polygon units.
    fn object() -> Rect {
        Rect::new(
            Point::new(Twip(0), Twip(0)),
            Size::new(Twip(2_160), Twip(2_160)),
        )
    }

    fn band(top: i32, bottom: i32, start: i32, end: i32) -> ContourBand {
        ContourBand {
            top: Twip(top),
            bottom: Twip(bottom),
            start: Twip(start),
            end: Twip(end),
        }
    }

    #[test]
    fn a_full_box_contour_in_words_space_is_the_objects_own_box() {
        // Word's full-box contour, whatever the object's size: 21,600 is the
        // object's far edge, not 21,600 EMU (about 34 twips).
        let full = polygon(&[(0, 0), (0, 21_600), (21_600, 21_600), (21_600, 0), (0, 0)]);
        assert_eq!(
            contour_bands(&full, object()),
            Some(vec![band(0, 2_160, 0, 2_160)])
        );
        let wide = Rect::new(
            Point::new(Twip(100), Twip(50)),
            Size::new(Twip(4_000), Twip(1_000)),
        );
        assert_eq!(
            contour_bands(&full, wide),
            Some(vec![band(50, 1_050, 100, 4_100)])
        );
    }

    /// The polygon's true horizontal extent at height `y` (twips, against
    /// [`object`]), by brute force over its edges — the reference the bands
    /// are checked against. Vertices are mapped to whole twips first, as the
    /// module maps them: this checks the banding, not the vertex rounding (a
    /// near-flat edge moves a long way in x for half a twip in y).
    fn cross_section(points: &[(i64, i64)], y: f64) -> Option<(f64, f64)> {
        let scale = 2_160.0 / 21_600.0;
        let mut extent: Option<(f64, f64)> = None;
        for (index, &(x0, y0)) in points.iter().enumerate() {
            let (x1, y1) = points[(index + 1) % points.len()];
            let (x0, y0, x1, y1) = (
                (x0 as f64 * scale).round(),
                (y0 as f64 * scale).round(),
                (x1 as f64 * scale).round(),
                (y1 as f64 * scale).round(),
            );
            if y0 == y1 || y < y0.min(y1) || y > y0.max(y1) {
                continue;
            }
            let x = x0 + (x1 - x0) * (y - y0) / (y1 - y0);
            extent = Some(extent.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
        }
        extent
    }

    /// Every band holds the polygon's whole cross-section at every height it
    /// spans (never under-excludes), and reaches no further than the widest of
    /// them (to the rounding of a twip).
    fn assert_bands_are_exact(points: &[(i64, i64)], bands: &[ContourBand]) {
        for band in bands {
            let (top, bottom) = (f64::from(band.top.raw()), f64::from(band.bottom.raw()));
            let mut widest: Option<(f64, f64)> = None;
            for step in 0..=16 {
                let y = top + (bottom - top) * f64::from(step) / 16.0;
                let Some((a, b)) = cross_section(points, y) else {
                    continue;
                };
                assert!(
                    f64::from(band.start.raw()) <= a + 1.0 && f64::from(band.end.raw()) >= b - 1.0,
                    "{band:?} misses {a}..{b} at y={y}"
                );
                widest = Some(widest.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
            }
            let (lo, hi) = widest.expect("a band lies inside the contour");
            assert!(
                f64::from(band.start.raw()) >= lo - 1.0 && f64::from(band.end.raw()) <= hi + 1.0,
                "{band:?} reaches past {lo}..{hi}"
            );
        }
    }

    #[test]
    fn a_sloped_contour_narrows_the_bands_near_its_tip_and_each_extent_is_exact() {
        // A diamond: its tip at the top centre, its widest at mid-height.
        let points = [(10_800, 0), (21_600, 10_800), (10_800, 21_600), (0, 10_800)];
        let bands = contour_bands(&polygon(&points), object()).expect("bounds something");
        assert!(bands.len() > 2, "{bands:?}");
        // The top slab is a sliver around the tip, not the diamond's full width:
        // one tall slab per vertex height would have given 0..2,160 here.
        let tip = bands[0];
        assert_eq!(tip.top, Twip(0));
        assert!(
            tip.start.raw() > 900 && tip.end.raw() < 1_260,
            "the tip band is {tip:?}"
        );
        // The bands widen to the waist and narrow again.
        let widths: Vec<i32> = bands
            .iter()
            .map(|band| (band.end - band.start).raw())
            .collect();
        let waist = widths
            .iter()
            .position(|&width| width == 2_160)
            .expect("full width at the waist");
        assert!(
            widths[..=waist].windows(2).all(|pair| pair[0] <= pair[1]),
            "{widths:?}"
        );
        assert!(
            widths[waist..].windows(2).all(|pair| pair[0] >= pair[1]),
            "{widths:?}"
        );
        assert_bands_are_exact(&points, &bands);
    }

    #[test]
    fn a_stepped_contour_keeps_each_steps_own_extent() {
        // An L: the full width for the top third, then only the left third.
        let ell = polygon(&[
            (0, 0),
            (21_600, 0),
            (21_600, 7_200),
            (7_200, 7_200),
            (7_200, 21_600),
            (0, 21_600),
        ]);
        assert_eq!(
            contour_bands(&ell, object()),
            Some(vec![band(0, 720, 0, 2_160), band(720, 2_160, 0, 720)])
        );
    }

    #[test]
    fn a_degenerate_or_unbounded_contour_falls_back_to_the_box() {
        assert_eq!(
            contour_bands(&polygon(&[(0, 0), (21_600, 0)]), object()),
            None
        );
        // Every vertex at one height: no vertical extent to follow.
        assert_eq!(
            contour_bands(&polygon(&[(0, 5), (10, 5), (20, 5)]), object()),
            None
        );
        let empty = Rect::new(Point::new(Twip(0), Twip(0)), Size::new(Twip(0), Twip(100)));
        let full = polygon(&[(0, 0), (0, 21_600), (21_600, 21_600)]);
        assert_eq!(contour_bands(&full, empty), None);
        let too_many: Vec<(i64, i64)> = (0..=MAX_CONTOUR_POINTS as i64).map(|i| (i, i)).collect();
        assert_eq!(contour_bands(&polygon(&too_many), object()), None);
    }

    #[test]
    fn a_dense_contour_is_cut_into_a_bounded_number_of_bands_that_still_cover_it() {
        // A 200-gon circle: 101 vertex heights plus the grid, more than the cap.
        let points: Vec<(i64, i64)> = (0..200)
            .map(|step| {
                let angle = f64::from(step) * std::f64::consts::TAU / 200.0;
                (
                    (10_800.0 + 10_800.0 * angle.cos()).round() as i64,
                    (10_800.0 + 10_800.0 * angle.sin()).round() as i64,
                )
            })
            .collect();
        let bands = contour_bands(&polygon(&points), object()).expect("bounds something");
        assert!(bands.len() <= MAX_CONTOUR_BANDS, "{} bands", bands.len());
        // Coverage: the bands tile the contour's whole height, no gaps.
        assert_eq!(bands.first().unwrap().top, Twip(0));
        assert_eq!(bands.last().unwrap().bottom, Twip(2_160));
        for pair in bands.windows(2) {
            assert_eq!(pair[0].bottom, pair[1].top);
        }
        // Thinning merges slabs by union: still never under-excludes.
        assert_bands_are_exact(&points, &bands);
    }

    #[test]
    fn each_band_excludes_from_the_one_resolved_side_to_its_own_far_edge() {
        // A contour flush with the measure's leading edge: wide at the top,
        // narrow below. Text sits to its right, so the exclusion is Left and each
        // band reaches only as far as the contour does there, plus distR.
        let bands = [band(0, 720, 0, 2_000), band(720, 1_440, 0, 500)];
        let excluded = contour_exclusions(
            &bands,
            Twip(0),
            Twip(100),
            Twip(0),
            Twip(9_000),
            WrapSides::BothSides,
            Twip(0),
        );
        assert_eq!(
            excluded,
            vec![
                BandExclusion {
                    side: InlineFloatSide::Left,
                    width: Twip(2_100),
                    top: Twip(0),
                    bottom: Twip(720),
                },
                BandExclusion {
                    side: InlineFloatSide::Left,
                    width: Twip(600),
                    top: Twip(720),
                    bottom: Twip(1_440),
                },
            ]
        );
    }

    #[test]
    fn bands_are_measured_from_the_paragraphs_top_and_spent_ones_drop_out() {
        let bands = [band(0, 720, 7_000, 9_000), band(720, 1_440, 8_500, 9_000)];
        // The paragraph starts 1,000 twips below the contour's top: the first
        // band is behind it, the second starts above it and is clamped to 0.
        let excluded = contour_exclusions(
            &bands,
            Twip(0),
            Twip(0),
            Twip(0),
            Twip(9_000),
            WrapSides::BothSides,
            Twip(1_000),
        );
        assert_eq!(
            excluded,
            vec![BandExclusion {
                side: InlineFloatSide::Right,
                width: Twip(500),
                top: Twip(0),
                bottom: Twip(440),
            }]
        );
    }
}
