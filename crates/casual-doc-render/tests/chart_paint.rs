// SPDX-License-Identifier: Apache-2.0

//! A DrawingML chart reaches the page from a real package — package → import →
//! projection → layout → display list (`docs/155` §9 increment 5, `docs/105`
//! FID-R-08).
//!
//! # Why these assertions are at the paint tier
//!
//! A model-level assertion has passed through every real defect in this lane:
//! `docs/155` §14 landed a typed model, a reader, per-construct loss reporting and
//! a retention guard, and a user still saw the literal text `[chart]`. "Modeled"
//! is not "shipped" (`SKILL` §9.4), so every guard below reads the **composed
//! display list** and asserts what is painted: bars whose heights are in the ratio
//! of the cached values, a polyline with one vertex per cached point, and the
//! secondary axis on the opposite edge of the plot from the primary.
//!
//! `fixtures/generated/chart.docx` is the committed combo fixture: a `c:barChart`
//! (`4.30`, `2.5`, `3.5`, `4.5` against categories Q1–Q4) plus a `c:lineChart`
//! (`4`, `4`, `4`, `4`) sharing one category axis, a primary value axis on the
//! left with major gridlines, a **secondary** value axis on the right, and an
//! out-of-scope `c:trendline` that keeps the projection `Partial`.
//!
//! `crates/casual-doc-layout/src/chart.rs` holds the unit-level geometry tests;
//! this file exists because only an actual `.docx` proves the projection survives
//! the importer and arrives where composition can read it.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{DisplayList, PaintItem, ShapeGeometry};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Rect, Twip};
use casual_doc_model::v1::Document;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;

const CHART_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/chart.docx");

/// The fixture's cached bar values, in category order.
const BAR_VALUES: [f64; 4] = [4.30, 2.5, 3.5, 4.5];

/// The fixture's cached line values, in category order.
const LINE_POINTS: usize = 4;

/// The authored `wp:extent`: 5486400 x 3200400 EMU = 6in x 3.5in = 8640 x 5040 twips.
const EXTENT: (Twip, Twip) = (Twip(8_640), Twip(5_040));

fn imported() -> Document {
    let mut package =
        DocxPackage::open(CHART_DOCX, PackageLimits::default()).expect("the chart fixture opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the chart fixture imports")
    .document
}

/// The first page's display list.
fn painted(document: &Document) -> DisplayList {
    let layout = paginate_document(document, &ParleyShaper::new());
    let page = layout
        .pages
        .first()
        .expect("the fixture paginates to a page");
    compose_page(page)
}

/// Every `PaintItem::Shape` carrying a filled rectangle, in paint order.
fn filled_rects(list: &DisplayList) -> Vec<Rect> {
    list.items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Shape {
                geometry: ShapeGeometry::Rect { rect },
                fill: Some(_),
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

/// Every `PaintItem::Shape` carrying a path, as its vertex list.
fn paths(list: &DisplayList) -> Vec<Vec<(Twip, Twip)>> {
    list.items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Shape {
                geometry: ShapeGeometry::Path { commands, .. },
                ..
            } => Some(
                commands
                    .iter()
                    .map(|command| {
                        let point = command.endpoint();
                        (point.x, point.y)
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

/// Every `PaintItem::Shape` carrying a straight line, as `(from, to)`.
fn lines(list: &DisplayList) -> Vec<((Twip, Twip), (Twip, Twip))> {
    list.items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Shape {
                geometry: ShapeGeometry::Line { from, to },
                ..
            } => Some(((from.x, from.y), (to.x, to.y))),
            _ => None,
        })
        .collect()
}

/// The bar group's bars, left to right: the filled rectangles inside the chart
/// box that are taller than a legend key and narrower than half the plot.
///
/// One helper rather than the same filter written twice, so the bar-ratio guard
/// and the category-label guard cannot drift apart on what counts as a bar.
fn bar_rects(list: &DisplayList, box_rect: Rect) -> Vec<Rect> {
    let mut bars: Vec<Rect> = filled_rects(list)
        .into_iter()
        .filter(|rect| {
            rect.origin.x >= box_rect.origin.x
                && rect.right() <= box_rect.right()
                && rect.size.height > Twip(200)
                && rect.size.width.raw() < box_rect.size.width.raw() / 2
        })
        .collect();
    bars.sort_by_key(|rect| rect.origin.x.raw());
    bars
}

/// The x of every vertical axis line inside the chart box, left to right and
/// de-duplicated: one per drawn value axis, at the plot's left and right edges.
///
/// Shared by the secondary-axis guard (which counts them) and the category-label
/// guard (which uses them as the plot's horizontal bounds), so "where the plot is"
/// has one definition.
fn vertical_axis_xs(list: &DisplayList, box_rect: Rect) -> Vec<i32> {
    let mut verticals: Vec<i32> = lines(list)
        .into_iter()
        .filter(|((x0, y0), (x1, y1))| {
            x0 == x1
                && y0 != y1
                && *x0 >= box_rect.origin.x
                && *x0 <= box_rect.right()
                && *y0 >= box_rect.origin.y
                && *y1 <= box_rect.bottom()
        })
        .map(|((x, _), _)| x.raw())
        .collect();
    verticals.sort_unstable();
    verticals.dedup();
    verticals
}

/// The chart box's page rectangle — the clip composition pushes around it.
///
/// Found by its authored size rather than by position, so the assertion does not
/// pin the paragraph layout above it.
fn chart_box(list: &DisplayList) -> Rect {
    list.items
        .iter()
        .find_map(|item| match item {
            PaintItem::PushClip(rect)
                if rect.size.width == EXTENT.0 && rect.size.height == EXTENT.1 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the chart is clipped to its authored wp:extent, so the box is reserved")
}

#[test]
fn the_fixture_projects_a_two_group_chart_with_three_axes() {
    // Not the point of this file, but the precondition every other guard rests
    // on: if the projection is missing, a paint assertion below would fail for
    // the wrong reason and the message would not say so.
    let document = imported();
    let charts = &document.definitions().charts;
    assert_eq!(charts.len(), 1, "the fixture has exactly one chart");
    let chart = charts.iter().next().expect("the chart").1;
    assert_eq!(
        chart.plot_area.groups.len(),
        2,
        "bar + line is a combo chart"
    );
    assert_eq!(
        chart.plot_area.axes.len(),
        3,
        "one category axis and two value axes — the second IS the secondary axis"
    );
}

#[test]
fn a_chart_is_no_longer_painted_as_the_text_chart() {
    // The defect this increment closes, asserted directly: before it, the only
    // thing on the page for a chart was a glyph run spelling `[chart]`.
    let document = imported();
    let list = painted(&document);
    assert!(
        !filled_rects(&list).is_empty(),
        "a chart must paint filled geometry, not a text placeholder"
    );
    let box_rect = chart_box(&list);
    assert_eq!(
        (box_rect.size.width, box_rect.size.height),
        EXTENT,
        "the authored wp:extent is reserved, which the `[chart]` run never did"
    );
}

#[test]
fn the_bar_group_paints_bars_whose_heights_are_in_the_ratio_of_its_cached_values() {
    let document = imported();
    let list = painted(&document);
    let box_rect = chart_box(&list);
    let bars = bar_rects(&list, box_rect);
    assert_eq!(
        bars.len(),
        BAR_VALUES.len(),
        "one bar per cached value, got {bars:?}"
    );

    // The ratio test is the assertion that can only pass if the cache reached the
    // geometry: a bar chart drawn from anything else (indices, a constant, the
    // formula string) would not reproduce 4.30 : 2.5 : 3.5 : 4.5.
    let tallest = bars
        .iter()
        .map(|rect| rect.size.height.raw())
        .max()
        .expect("four bars");
    let largest = BAR_VALUES.iter().copied().fold(f64::MIN, f64::max);
    for (bar, value) in bars.iter().zip(BAR_VALUES) {
        let painted_ratio = f64::from(bar.size.height.raw()) / f64::from(tallest);
        let cached_ratio = value / largest;
        assert!(
            (painted_ratio - cached_ratio).abs() < 0.02,
            "bar for cached {value} painted at ratio {painted_ratio:.4}, cache says \
             {cached_ratio:.4} (bars: {bars:?})"
        );
    }

    // And they share a baseline: a bar chart whose bars float would satisfy the
    // ratios above while being wrong on the page.
    let baselines: Vec<i32> = bars.iter().map(|rect| rect.bottom().raw()).collect();
    assert!(
        baselines
            .windows(2)
            .all(|pair| (pair[0] - pair[1]).abs() <= 1),
        "every bar sits on the value axis zero, got {baselines:?}"
    );
}

#[test]
fn the_line_group_paints_a_polyline_through_every_cached_point() {
    let document = imported();
    let list = painted(&document);
    let box_rect = chart_box(&list);
    let series: Vec<Vec<(Twip, Twip)>> = paths(&list)
        .into_iter()
        .filter(|points| {
            points
                .iter()
                .all(|(x, _)| *x >= box_rect.origin.x && *x <= box_rect.right())
        })
        .collect();
    assert_eq!(
        series.len(),
        1,
        "the fixture's one line series paints one polyline, got {series:?}"
    );
    let points = &series[0];
    assert_eq!(
        points.len(),
        LINE_POINTS,
        "a vertex per cached point (c:smooth is 0 on this series, so it is not sampled)"
    );
    // The cached values are all `4`, so the polyline is flat — which is a real
    // property of this fixture's data and not an artifact of ignoring it: a
    // renderer plotting the point INDEX would rise monotonically here.
    let ys: Vec<i32> = points.iter().map(|(_, y)| y.raw()).collect();
    assert!(
        ys.windows(2).all(|pair| (pair[0] - pair[1]).abs() <= 1),
        "the cached line values are all 4, so the polyline is level; got {ys:?}"
    );
    // …and it is inside the plot, above the bar baseline, because 4 is below the
    // tallest bar's 4.5 and above zero.
    assert!(
        ys[0] > box_rect.origin.y.raw() && ys[0] < box_rect.bottom().raw(),
        "the line sits inside the chart box, got y {}",
        ys[0]
    );
}

#[test]
fn the_secondary_value_axis_is_drawn_on_the_opposite_side_of_the_plot() {
    let document = imported();
    let list = painted(&document);
    let box_rect = chart_box(&list);
    // The vertical axis lines inside the chart box: one per non-deleted value
    // axis, at the plot's left and right edges.
    let verticals = vertical_axis_xs(&list, box_rect);
    assert_eq!(
        verticals.len(),
        2,
        "the primary value axis and the secondary one are two vertical axis lines, got {verticals:?}"
    );
    let (primary, secondary) = (verticals[0], verticals[1]);
    assert!(
        secondary - primary > box_rect.size.width.raw() / 2,
        "the secondary axis is on the opposite side of the plot from the primary: \
         {primary} vs {secondary} in a {}-twip box",
        box_rect.size.width.raw()
    );
    // Both inside the authored box, so "opposite side" is not satisfied by one of
    // them escaping the chart.
    assert!(
        primary > box_rect.origin.x.raw() && secondary < box_rect.right().raw(),
        "both axis lines are inside the chart box"
    );
}

#[test]
fn the_cached_category_labels_are_painted_under_the_bars_they_label() {
    // Category labels come from `c:strCache`, so this is the string half of "the
    // cache is the data" — the numeric half is the bar-ratio guard above.
    //
    // This guard counted every glyph run inside the chart box and asserted a
    // floor of four. It could not fail: the two value axes contribute a tick
    // label per major interval, so suppressing all four category labels still
    // left well over four runs and the test stayed green. A floor that another
    // feature already satisfies is not a guard.
    //
    // What is actually guaranteed is positional: a category label sits in the
    // gutter BELOW the plot and HORIZONTALLY WITHIN it, one under each bar. A
    // value-axis tick label is beside the plot, not within it — including the zero
    // tick, whose baseline also falls below the bars, which is why the vertical
    // test alone is not enough (it found six).
    let document = imported();
    let list = painted(&document);
    let box_rect = chart_box(&list);
    let bars = bar_rects(&list, box_rect);
    assert_eq!(bars.len(), BAR_VALUES.len(), "one bar per cached category");
    let baseline = bars
        .iter()
        .map(|rect| rect.bottom().raw())
        .max()
        .expect("four bars");
    let axes = vertical_axis_xs(&list, box_rect);
    let (plot_left, plot_right) = (axes[0], axes[axes.len() - 1]);

    let mut below: Vec<Twip> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run }
                if run.origin.x.raw() > plot_left
                    && run.origin.x.raw() < plot_right
                    && run.origin.y.raw() > baseline
                    && run.origin.y <= box_rect.bottom() =>
            {
                Some(run.origin.x)
            }
            _ => None,
        })
        .collect();
    below.sort_unstable_by_key(|x| x.raw());
    assert_eq!(
        below.len(),
        BAR_VALUES.len(),
        "four cached category labels sit in the gutter below the plot, got \
         {below:?} (bars end at y {baseline}, plot spans x {plot_left}..{plot_right})"
    );

    // And each one is under its own bar: the label's left edge is within one
    // category slot of its bar's centre. A single label repeated, or four stacked
    // in one place, would satisfy the count above but not this.
    let slot = box_rect.size.width.raw() / BAR_VALUES.len() as i32;
    let mut centres: Vec<i32> = bars
        .iter()
        .map(|rect| rect.origin.x.raw() + rect.size.width.raw() / 2)
        .collect();
    centres.sort_unstable();
    for (label, centre) in below.iter().zip(&centres) {
        assert!(
            (label.raw() - centre).abs() <= slot,
            "the label at x {} belongs under the bar centred at {centre} \
             (slot {slot})",
            label.raw()
        );
    }
}

#[test]
fn reading_a_projection_does_not_change_the_document() {
    // The thing the retention guards already won (`docs/155` §6.1 consequence 2),
    // held at this tier too: composing a chart is a READ of a projection, so the
    // document it was read from must be byte-identical afterwards. If layout ever
    // normalises, caches into, or dirties the model while drawing, this reddens
    // before an export guard would.
    let document = imported();
    let before = document
        .to_json()
        .expect("the imported document serializes");
    let list = painted(&document);
    assert!(
        !filled_rects(&list).is_empty(),
        "the chart must actually have been drawn, or this guard passes vacuously"
    );
    let after = document.to_json().expect("the document still serializes");
    assert_eq!(
        before, after,
        "composing a chart must not change a single byte of the document"
    );
}
