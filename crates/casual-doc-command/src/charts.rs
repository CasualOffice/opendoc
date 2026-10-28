// SPDX-License-Identifier: Apache-2.0

//! Chart insertion, the editor's and agents' one lowering (`docs/181` §4.2,
//! step 7).
//!
//! The editor's `insertChart` and an agent's `insert_chart` both call
//! [`insert_chart`]: an inline chart object with Word's own sample data, in
//! one of the families this build paints. Editing a chart's data stays inside
//! the editor's chart module (`casual-doc-wasm/src/chart.rs`) for now.

#![allow(clippy::cast_possible_truncation)] // point counts of a fixed sample

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Axis, AxisKind, AxisPosition, BarDirection, BarGrouping, Chart, ChartCoverage, ChartGroup,
    ChartGroupKind, ChartId, ChartText, ChartValue, DataRange, DisplayBlanks, EmbeddedKind,
    EmbeddedObject, EmbeddedPart, Extent, Grouping, InlineNode, Legend, LegendPosition, PlotArea,
    ScatterStyle, Series,
};

/// The chart families an agent or a person can insert, as `kind` tokens.
pub const KINDS: [&str; 17] = [
    "column",
    "column-stacked",
    "column-percent",
    "bar",
    "bar-stacked",
    "bar-percent",
    "line",
    "line-markers",
    "line-stacked",
    "line-percent",
    "area",
    "area-stacked",
    "area-percent",
    "scatter",
    "scatter-smooth",
    "pie",
    "doughnut",
];

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// Inserts an inline chart of family `kind` at `at`, 5" x 3" (Word's default
/// frame), with Word's sample data: the object first, then its projection,
/// one step (one undo removes both). The object id and the chart id are minted
/// in that order; the part name and relationship id derive from the object's
/// id, so two inserts (on two replicas) cannot mint the same name. O(1).
///
/// # Errors
/// [`unpainted_chart_refusal`] for a family this build does not paint, or ids
/// running out.
pub fn insert_chart(at: Pos, kind: &str, ids: MintId<'_>) -> Result<Vec<Operation>, String> {
    let group = chart_group_for_kind(kind).ok_or_else(|| unpainted_chart_refusal(kind))?;
    let object_id = next(ids)?;
    let chart_id = ChartId::new(next(ids)?);
    let extent = Extent {
        width_emu: 4_572_000,
        height_emu: 2_743_200,
    };
    let object = EmbeddedObject {
        id: object_id,
        kind: EmbeddedKind::Chart,
        part: EmbeddedPart {
            relationship_id: format!("rIdChart{object_id}"),
            relationship_type: EMBEDDED_CHART_RELATIONSHIP.to_owned(),
            part_name: format!("word/charts/chart{object_id}.xml"),
        },
        extra_parts: Vec::new(),
        // No cached preview: the projection draws, and a preview would be a
        // second, stale picture of the same data.
        preview: None,
        extent,
        prog_id: None,
    };
    let projection = default_chart_projection(object_id, group);
    Ok(vec![
        Operation::InsertInlineObject {
            at,
            node: Box::new(InlineNode::EmbeddedObject(Box::new(object))),
        },
        Operation::SetChartDefinition {
            id: chart_id,
            chart: Some(Box::new(projection)),
        },
    ])
}

/// The OOXML relationship type of an embedded chart part.
pub const EMBEDDED_CHART_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";

/// The chart family one of [`insert_chart`]'s `kind`
/// tokens names, or `None` when this build does not PAINT that family.
///
/// Deliberately keyed off what paints rather than off what the model represents.
/// `ChartGroupKind` is the whole typed surface the importer projects into, and a
/// family it can read but `casual_doc_layout::chart` cannot draw must not be
/// insertable: a chart with no drawable projection paints `[chart]`, so offering
/// one would be a gesture that inserts a label (`SKILL` §10). Today the two sets
/// coincide — all seven variants paint — and this function is where they would
/// diverge if an eighth were modeled first.
///
/// The defaults are Word's own: a column chart's 150% gap width and -27% overlap,
/// a pie's twelve-o'clock first slice, a doughnut's 75% hole.
///
/// O(1).
#[must_use]
pub fn chart_group_for_kind(kind: &str) -> Option<ChartGroupKind> {
    // Word's and ONLYOFFICE's chart-type picker: each family with its stacked
    // and 100% stacked forms, a line with markers, a smoothed scatter. Stacked
    // bars overlap fully (`c:overlap` 100), which is what stacking means for a
    // bar and what Word writes for one.
    let bar = |direction, grouping| {
        let stacked = !matches!(grouping, BarGrouping::Clustered | BarGrouping::Standard);
        ChartGroupKind::Bar {
            direction,
            grouping,
            gap_width: 150,
            overlap: if stacked { 100 } else { -27 },
        }
    };
    let group = match kind {
        "column" => bar(BarDirection::Column, BarGrouping::Clustered),
        "column-stacked" => bar(BarDirection::Column, BarGrouping::Stacked),
        "column-percent" => bar(BarDirection::Column, BarGrouping::PercentStacked),
        "bar" => bar(BarDirection::Bar, BarGrouping::Clustered),
        "bar-stacked" => bar(BarDirection::Bar, BarGrouping::Stacked),
        "bar-percent" => bar(BarDirection::Bar, BarGrouping::PercentStacked),
        "line" => ChartGroupKind::Line {
            grouping: Grouping::Standard,
            marker: false,
        },
        "line-markers" => ChartGroupKind::Line {
            grouping: Grouping::Standard,
            marker: true,
        },
        "line-stacked" => ChartGroupKind::Line {
            grouping: Grouping::Stacked,
            marker: false,
        },
        "line-percent" => ChartGroupKind::Line {
            grouping: Grouping::PercentStacked,
            marker: false,
        },
        "area" => ChartGroupKind::Area {
            grouping: Grouping::Standard,
        },
        "area-stacked" => ChartGroupKind::Area {
            grouping: Grouping::Stacked,
        },
        "area-percent" => ChartGroupKind::Area {
            grouping: Grouping::PercentStacked,
        },
        "scatter" => ChartGroupKind::Scatter {
            style: ScatterStyle::LineMarker,
        },
        "scatter-smooth" => ChartGroupKind::Scatter {
            style: ScatterStyle::SmoothMarker,
        },
        "pie" => ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        "doughnut" => ChartGroupKind::Doughnut {
            first_slice_angle: 0,
            hole_size: 75,
        },
        _ => return None,
    };
    Some(group)
}

/// The marked refusal for a chart family this build does not paint.
///
/// A sentence the reader can act on plus a stable code a host routes
/// ([`casual_doc_edit::refusal`]), rather than a bare error name that
/// `edit_errors.mjs` would replace with "that edit isn't supported for this
/// selection yet" — which is wrong twice over here: the selection is fine, and
/// the reader is never told which families they CAN have.
///
/// A function rather than an inline `format!` so it is assertable without
/// crossing the `#[wasm_bindgen]` boundary: `to_js` panics under a native test.
///
/// Complexity: O(kind length), one allocation.
#[must_use]
pub fn unpainted_chart_refusal(kind: &str) -> String {
    casual_doc_edit::refusal::marked(
        "chart.unpainted-family",
        &format!(
            "This build draws bar, column, line, area, scatter, pie and doughnut \
             charts; there is no {kind} chart to insert yet."
        ),
    )
}

/// Whether `group` is one of the pie families, which take ONE series and give
/// each category its own colour.
///
/// Not a second copy of `casual_doc_layout::chart`'s `colors_by_point`, which
/// answers a PAINT question (which colour a bar or a sector takes) and is right
/// for pie whether or not a producer wrote `c:varyColors`. This answers an
/// AUTHORING one: how many series the sample data has, whether the plot area
/// gets axes at all, and what `c:varyColors` is written as. Both read the same
/// two enum variants because that is what the OOXML families are, not because
/// one is standing in for the other.
#[must_use]
pub const fn chart_colors_by_point(group: ChartGroupKind) -> bool {
    matches!(
        group,
        ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. }
    )
}

/// One cached data range over `values`, with every index present.
#[must_use]
pub fn chart_range(values: &[&str]) -> DataRange {
    DataRange {
        // No `c:f`: the data is the cache, because there is no worksheet behind
        // an inserted chart. A formula naming a range nothing holds would be a
        // reference a spreadsheet editor could not resolve.
        formula: None,
        point_count: values.len() as u32,
        points: values
            .iter()
            .enumerate()
            .map(|(index, value)| (index as u32, ChartValue::Number((*value).to_owned())))
            .collect(),
        number_format: None,
    }
}

/// The sample projection a freshly inserted chart carries, anchored to `object`.
///
/// The numbers are Word's own Insert ▸ Chart sample data — three series of four
/// categories for the category families, one series of four for the pie families,
/// and three x/y pairs for scatter. Sample data rather than an empty chart on
/// purpose, and for the same reason Word does it: an empty plot area gives the
/// author nothing to recognise, nothing to resize against, and no way to tell a
/// working chart from a broken one.
///
/// [`ChartCoverage::Complete`] is not optimism: this projection IS the whole
/// chart, there are no source bytes it fails to represent, and that is exactly
/// the condition `ChartCoverage::permits_regeneration` asks about.
///
/// Complexity: O(1) — a fixed number of series and points.
#[must_use]
pub fn default_chart_projection(object: NodeId, group: ChartGroupKind) -> Chart {
    let scatter = matches!(group, ChartGroupKind::Scatter { .. });
    let categories = if chart_colors_by_point(group) {
        chart_range(&["8.2", "3.2", "1.4", "1.2"])
    } else {
        chart_range(&["4.3", "2.5", "3.5", "4.5"])
    };
    let category_labels = if chart_colors_by_point(group) {
        ["1st Qtr", "2nd Qtr", "3rd Qtr", "4th Qtr"]
    } else {
        ["Category 1", "Category 2", "Category 3", "Category 4"]
    };
    let label = |text: &str| ChartText {
        text: text.to_owned(),
        formula: None,
    };
    let category_names = DataRange {
        formula: None,
        point_count: category_labels.len() as u32,
        points: category_labels
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u32, ChartValue::Text((*name).to_owned())))
            .collect(),
        number_format: None,
    };
    let series = if scatter {
        vec![Series {
            index: 0,
            order: 0,
            name: Some(label("Series 1")),
            categories: None,
            values: chart_range(&["2.7", "3.2", "0.8"]),
            x_values: Some(chart_range(&["0.7", "1.8", "2.6"])),
            ..Series::default()
        }]
    } else if chart_colors_by_point(group) {
        vec![Series {
            index: 0,
            order: 0,
            name: Some(label("Series 1")),
            categories: Some(category_names),
            values: categories,
            ..Series::default()
        }]
    } else {
        [
            ("Series 1", ["4.3", "2.5", "3.5", "4.5"]),
            ("Series 2", ["2.4", "4.4", "1.8", "2.8"]),
            ("Series 3", ["2.0", "2.0", "3.0", "5.0"]),
        ]
        .iter()
        .enumerate()
        .map(|(index, (name, values))| Series {
            index: index as u32,
            order: index as u32,
            name: Some(label(name)),
            categories: Some(category_names.clone()),
            values: chart_range(&values.map(|value| value)),
            ..Series::default()
        })
        .collect()
    };
    // The pie families have no axes at all, which is why nothing declares one for
    // them: `compose_chart` draws no axis furniture it was not given, and an axis
    // id a group names but the plot area does not hold would be a dangling
    // reference the model rejects.
    let (axis_ids, axes) = if chart_colors_by_point(group) {
        (Vec::new(), Vec::new())
    } else {
        (
            vec![1, 2],
            vec![
                Axis {
                    id: 1,
                    // Scatter plots both axes as values; every other family puts
                    // the categories on the bottom.
                    kind: if scatter {
                        AxisKind::Value
                    } else {
                        AxisKind::Category
                    },
                    position: Some(AxisPosition::Bottom),
                    cross_axis_id: Some(2),
                    ..Axis::default()
                },
                Axis {
                    id: 2,
                    kind: AxisKind::Value,
                    position: Some(AxisPosition::Left),
                    major_gridlines: true,
                    cross_axis_id: Some(1),
                    ..Axis::default()
                },
            ],
        )
    };
    Chart {
        font: None,
        chart_retained: Vec::new(),
        namespaces: Vec::new(),
        space_retained: Vec::new(),
        object,
        coverage: ChartCoverage::Complete,
        // No title and `autoTitleDeleted` unset: Word shows "Chart Title" as a
        // placeholder the author replaces, and there is no title-editing gesture
        // yet, so a chart that printed those two words with no way to change them
        // would be worse than one with none.
        title: None,
        auto_title_deleted: true,
        plot_area: PlotArea {
            retained: Vec::new(),
            groups: vec![ChartGroup {
                retained: Vec::new(),
                kind: group,
                series,
                axis_ids,
                vary_colors: chart_colors_by_point(group),
            }],
            axes,
        },
        legend: Some(Legend {
            font: None,
            retained: Vec::new(),
            position: LegendPosition::Bottom,
            overlay: false,
        }),
        plot_visible_only: true,
        display_blanks_as: DisplayBlanks::Gap,
        vary_colors: chart_colors_by_point(group),
        external_data: None,
        dirty: false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_listed_kind_is_a_painted_family() {
        for kind in super::KINDS {
            assert!(super::chart_group_for_kind(kind).is_some(), "{kind}");
        }
        assert!(super::chart_group_for_kind("radar").is_none());
    }
}
