//! Chart **authoring**: the numbers behind a chart, its family, its title and
//! its legend.
//!
//! # Why this module exists
//!
//! `insertChart` shipped a chart with Word's sample data in it and no way to
//! change any of it. The projection was fully typed, `Operation::SetChartDefinition`
//! could already replace a chart's whole definition transactionally, the DOCX
//! writer could already regenerate the part — and the facade exposed exactly one
//! chart method. So a reader could insert a picture of three made-up series and
//! nothing else: `SKILL` §9 rule 4, "built is not reachable", in its most
//! expensive form, because the feature *looks* finished from the ribbon.
//!
//! Everything here is the missing half of that: one read, one write, and the
//! gate that says when the write is safe.
//!
//! # The interaction this serves, and where it comes from
//!
//! Stated from knowledge of the two products rather than from source — neither
//! ONLYOFFICE checkout is present in this tree, and a citation nobody can open
//! is worse than none:
//!
//! * **Word** answers "how do I put data in a chart" with *Chart Design ▸ Edit
//!   Data* (also on the chart's right-click menu), which opens a small
//!   spreadsheet grid: row 1 holds the series names, column A holds the category
//!   names, the body holds numbers, and a coloured border around the plotted
//!   range is dragged to add or remove rows and columns. Edits apply to the chart
//!   as they are made. *Change Chart Type* is a gallery, and the title and legend
//!   are Chart Elements toggles.
//! * **Google Sheets** (and a Docs chart, which opens its Sheet) answers with a
//!   side panel — Setup and Customize — whose Setup tab carries a chart-type
//!   gallery and the data range, and whose Customize tab carries the chart title
//!   and the legend position.
//!
//! The common denominator, and the thing the complaint actually asks for, is a
//! **grid of rows and columns that applies as you type**: rows are categories,
//! columns are series. That is the shape this facade speaks, and it is the shape
//! `webapp/src/chart_data.mjs` renders.
//!
//! # One mutation path
//!
//! There is exactly one write — [`WasmDocument::set_chart_data`] — and it carries
//! the family, the title, the legend, the series names, the row names and every
//! cell together. Not four setters: four setters are four undo entries for one
//! gesture and four places for the next rule to be forgotten. One
//! `SetChartDefinition` inside one transaction means the existing protection,
//! review-mode and windowed-layout gates in `apply_group` all apply for free, and
//! one undo takes the whole change back.
//!
//! # The patch is a patch, not a replacement
//!
//! `SetChartDefinition` replaces the projection, so writing one from the grid
//! alone would silently drop everything the grid does not show — a series' fill
//! and line, its data labels, the axis bounds, gridlines, tick marks, number
//! formats, the gap width. [`apply_chart_patch`] therefore starts from the
//! existing projection and overwrites only the authored fields. That is
//! `AGENTS.md`'s no-silent-data-loss rule applied to our own edit path.
//!
//! # An edited chart is DIRTY, and that is what makes its save honest
//!
//! `casual-doc-export` re-emits a retained chart part verbatim — for an imported
//! chart the source bytes are the authority and the projection is only a read
//! index over them (`docs/155` §6.1). Editing the projection alone would repaint
//! correctly, satisfy every test of the editing path, and be **thrown away on
//! save**: silent data loss in the one shape no test of the editing path would
//! catch. It also meant a chart authored here, saved and reopened, came back
//! read-only — the same "static chart" one save later.
//!
//! So the write sets [`Chart::dirty`]. A dirty projection is the authority: the
//! exporter regenerates the part from it and supersedes the retained bytes, and
//! the embedded workbook the chart names is replaced by a values-only one holding
//! exactly the grid, so Word's own *Edit Data* opens the numbers the chart shows.
//! The view says so (`replacesWorkbook`) BEFORE the first edit, because replacing
//! a producer's workbook discards whatever else it held — formulas, other sheets
//! — and that is the reader's decision to make knowingly (`docs/155` §12 Q-A).
//!
//! # When authoring is REFUSED
//!
//! Two conditions, both read off the model, both cases where regenerating the
//! part would drop something the projection does not hold: a `Partial`
//! projection may not be regenerated at all
//! ([`ChartCoverage::permits_regeneration`]), and a multi-group plot area is a
//! combo chart this grid cannot describe.
//!
//! A refused chart is still **read**, and its grid is still shown — disabled,
//! with the sentence. A control that vanishes cannot be told from a bug (`SKILL`
//! §10), and "you cannot edit this one, here is why" is an answer.
//!
//! # Complexity
//!
//! Every function here is O(series × rows) in the ONE chart named, with a bounded
//! `O(charts)` registry scan to find it. Nothing walks the document, so a
//! keystroke in a grid cell stays O(1) in document size (`docs/107` §4).

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use casual_doc_edit::{Operation, Pos, refusal};
// Own line (the parallel-import rule): the two knobs the chart write hands the
// transaction choke point.
use casual_doc_model::v1::{
    Axis, AxisKind, AxisPosition, Chart, ChartGroupKind, ChartId, ChartText, ChartTitle,
    ChartValue, DataRange, Legend, LegendPosition, MAX_CHART_DATA_POINTS, MAX_CHART_NUMBER_BYTES,
    MAX_CHART_SERIES_PER_GROUP, MAX_CHART_TEXT_BYTES, Series,
};
use casual_doc_transaction::{Coalesce, Origin};
// Own line (the parallel-import rule): the node type the authoring view names.
use casual_doc_model::NodeId;

use crate::{
    EditResult, WasmDocument, chart_colors_by_point, chart_group_for_kind, damage_of, node_id,
    node_id_msg, to_js, unpainted_chart_refusal,
};

/// The family tokens the data editor's gallery offers, in the order a gallery
/// shows them.
///
/// Word's own order for Insert ▸ Chart: Column, Bar, Line, Area, Pie, Doughnut,
/// X Y (Scatter). Every token here must be one [`chart_group_for_kind`] accepts
/// — held by `every_gallery_token_is_an_insertable_family` rather than by
/// matching two lists by eye — and [`chart_kind_token`]'s exhaustive match is
/// what makes an eighth `ChartGroupKind` variant a compile error here rather
/// than a family missing from the gallery.
pub(crate) const CHART_GALLERY: [&str; 7] = [
    "column", "bar", "line", "area", "pie", "doughnut", "scatter",
];

/// The gallery token for a group's family.
///
/// Exhaustive on purpose: a new `ChartGroupKind` variant stops this compiling,
/// which is the only way a gallery derived from the model cannot silently fall
/// behind it. `Bar` carries its direction, so it answers two tokens — the same
/// two [`chart_group_for_kind`] accepts.
///
/// O(1).
pub(crate) const fn chart_kind_token(group: ChartGroupKind) -> &'static str {
    match group {
        ChartGroupKind::Bar {
            direction: casual_doc_model::v1::BarDirection::Column,
            ..
        } => "column",
        ChartGroupKind::Bar { .. } => "bar",
        ChartGroupKind::Line { .. } => "line",
        ChartGroupKind::Area { .. } => "area",
        ChartGroupKind::Pie { .. } => "pie",
        ChartGroupKind::Doughnut { .. } => "doughnut",
        ChartGroupKind::Scatter { .. } => "scatter",
    }
}

/// The legend token for a chart's legend, with `"none"` for no legend at all.
///
/// `"none"` rather than a separate boolean because the editor offers one control
/// with six choices, and two fields for one choice is how a UI comes to show a
/// position for a legend that is not drawn.
///
/// O(1).
fn legend_token(legend: Option<&Legend>) -> &'static str {
    match legend.map(|legend| legend.position) {
        None => "none",
        Some(LegendPosition::Bottom) => "bottom",
        Some(LegendPosition::Left) => "left",
        Some(LegendPosition::Right) => "right",
        Some(LegendPosition::Top) => "top",
        Some(LegendPosition::TopRight) => "topRight",
    }
}

/// The legend position a token names, `None` for `"none"`, and an error for a
/// token no control emits.
///
/// O(1).
fn legend_position(token: &str) -> Result<Option<LegendPosition>, String> {
    let position = match token {
        "none" => return Ok(None),
        "bottom" => LegendPosition::Bottom,
        "left" => LegendPosition::Left,
        "right" => LegendPosition::Right,
        "top" => LegendPosition::Top,
        "topRight" => LegendPosition::TopRight,
        other => {
            return Err(refusal::marked(
                "chart.legend-unknown",
                &format!(
                    "A chart legend can sit at the bottom, left, right, top or top \
                     right, or be hidden; there is no {other} position."
                ),
            ));
        }
    };
    Ok(Some(position))
}

/// Whether a family plots the first column as **x values** rather than as
/// category names.
///
/// Scatter is the one family whose horizontal position is a number, which is why
/// Word's data sheet heads column A "X Values" for a scatter chart and leaves it
/// blank for every other family. The editor needs the same answer to label the
/// column and to decide whether the column is text or numbers.
///
/// O(1).
const fn plots_x_values(group: ChartGroupKind) -> bool {
    matches!(group, ChartGroupKind::Scatter { .. })
}

/// The authoring view of one chart: everything the data editor draws, in one
/// read.
///
/// One payload rather than eight getters because the editor renders the grid, the
/// gallery, the title and the legend together, and eight reads of one chart is
/// eight chances for the panel to show a half-updated chart.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChartDataView {
    /// The chart object's node id, echoed so the host can tell a stale payload
    /// from a current one after the selection moves.
    pub(crate) node: String,
    /// The chart's family, as a [`CHART_GALLERY`] token.
    pub(crate) kind: &'static str,
    /// Every family the gallery may offer. Enumerated from the engine so the UI
    /// cannot hard-code a list that drifts from what can be inserted.
    pub(crate) kinds: Vec<&'static str>,
    /// Whether the data may be changed. `false` leaves the grid readable and
    /// disabled; see [`chart_authoring_refusal`].
    pub(crate) editable: bool,
    /// The reader's sentence for why it may not, or empty.
    pub(crate) reason: String,
    /// Whether an edit will replace an embedded workbook the chart names. The
    /// editor says so before the first change, because the replacement keeps
    /// only the data the grid shows (see this module's header).
    pub(crate) replaces_workbook: bool,
    /// The stable routing code for that sentence, so a host shows it in the
    /// reader's own language, or empty.
    pub(crate) code: String,
    /// `"x"` when the first column holds x values (scatter), `"category"` when it
    /// holds category names.
    pub(crate) first_column: &'static str,
    /// The chart title, empty when there is none.
    pub(crate) title: String,
    /// The legend position token, `"none"` when the chart draws no legend.
    pub(crate) legend: &'static str,
    /// The series names, in plot order — the grid's column headers.
    pub(crate) series: Vec<String>,
    /// The category names, or the x values for a scatter chart — the grid's row
    /// headers.
    pub(crate) labels: Vec<String>,
    /// The cells, `cells[row][column]`, in each number's **verbatim lexical
    /// form**; an empty string is a blank cache entry, which is a value the model
    /// has ([`ChartValue::Blank`]) and not an absence.
    pub(crate) cells: Vec<Vec<String>>,
    /// The most series one chart may hold, so the editor can disable Add column
    /// WITH the number rather than refusing after the fact.
    pub(crate) max_series: usize,
    /// The most rows one chart may hold, for the same reason.
    pub(crate) max_rows: usize,
    /// The byte ceiling on the title and on any series or row name, so the
    /// editor's fields carry a `maxlength` rather than letting a reader type past
    /// a limit and only then be told.
    pub(crate) title_limit: usize,
}

/// What the editor sends back. Every field is authoritative: a patch names the
/// whole grid, because "row 3 changed" cannot express a row being deleted.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct ChartDataPatch {
    kind: String,
    title: String,
    legend: String,
    series: Vec<String>,
    labels: Vec<String>,
    cells: Vec<Vec<String>>,
}

/// Why this chart's data may not be authored, as a marked refusal, or `None`.
///
/// The two conditions and the reasoning behind each are in this module's header.
/// Where the chart came from is deliberately NOT one of them: an imported chart
/// is editable, and the edit marks it [`Chart::dirty`] so the save regenerates
/// it rather than copying the source bytes back over the reader's change.
///
/// O(1).
fn chart_authoring_refusal(chart: &Chart) -> Option<String> {
    if !chart.coverage.permits_regeneration() {
        return Some(refusal::marked(
            "chart.partial-coverage",
            "This chart uses features this build does not model yet, so its data \
             cannot be rewritten without losing them.",
        ));
    }
    if chart.plot_area.groups.len() != 1 {
        return Some(refusal::marked(
            "chart.many-groups",
            "This chart plots more than one chart type at once. Editing the data \
             of a combination chart is not possible yet.",
        ));
    }
    None
}

/// The values a [`DataRange`] holds, densely, in index order, as the strings the
/// grid shows.
///
/// A cache may be sparse — `point_count` can exceed `points.len()` and an index
/// may be missing — so this fills every slot up to `rows` and renders a missing
/// or blank entry as the empty string. The grid has no third state between "a
/// number" and "nothing", and inventing one would mean the editor could write
/// back a value it never showed.
///
/// Complexity: O(rows + points).
fn dense_cells(range: Option<&DataRange>, rows: usize) -> Vec<String> {
    let mut out = vec![String::new(); rows];
    let Some(range) = range else {
        return out;
    };
    for (index, value) in &range.points {
        let Ok(index) = usize::try_from(*index) else {
            continue;
        };
        if index >= rows {
            continue;
        }
        out[index] = match value {
            ChartValue::Number(text) | ChartValue::Text(text) => text.clone(),
            ChartValue::Blank => String::new(),
        };
    }
    out
}

/// How many rows the grid needs to show every value this chart holds.
///
/// The declared `point_count` and the actual point indices can disagree in a
/// producer-written cache, and the series need not agree with each other, so the
/// row count is the widest thing in the chart rather than any one series'
/// opinion. Showing fewer rows than the chart holds would hide data and then
/// delete it on the next write.
///
/// Complexity: O(series + points).
fn row_count(series: &[Series]) -> usize {
    let extent = |range: Option<&DataRange>| {
        range.map_or(0, |range| {
            let declared = usize::try_from(range.point_count).unwrap_or(0);
            let highest = range
                .points
                .iter()
                .filter_map(|(index, _)| usize::try_from(*index).ok())
                .map(|index| index + 1)
                .max()
                .unwrap_or(0);
            declared.max(highest)
        })
    };
    series
        .iter()
        .map(|series| {
            extent(Some(&series.values))
                .max(extent(series.categories.as_ref()))
                .max(extent(series.x_values.as_ref()))
        })
        .max()
        .unwrap_or(0)
}

/// The authoring view of `chart`, anchored at `object`.
///
/// Complexity: O(series × rows) for the one chart.
fn chart_view(object: NodeId, chart: &Chart) -> ChartDataView {
    // `groups.first()` rather than indexing: a projection with no group at all is
    // representable (an empty plot area), and the view of it is an empty grid
    // whose family is the default the gallery lands on, not a panic.
    let group = chart.plot_area.groups.first();
    let kind = group.map_or("column", |group| chart_kind_token(group.kind));
    let series = group.map(|group| group.series.as_slice()).unwrap_or(&[]);
    let rows = row_count(series);
    let scatter = group.is_some_and(|group| plots_x_values(group.kind));
    let labels = dense_cells(
        series.first().and_then(|first| {
            if scatter {
                first.x_values.as_ref()
            } else {
                first.categories.as_ref()
            }
        }),
        rows,
    );
    let columns: Vec<Vec<String>> = series
        .iter()
        .map(|series| dense_cells(Some(&series.values), rows))
        .collect();
    let refusal = chart_authoring_refusal(chart);
    let (reason, code) = refusal.as_deref().map_or_else(
        || (String::new(), String::new()),
        |marked| {
            let (sentence, code) = refusal::split(marked);
            // `split` keeps the `refused: ` marker on the reader's half, because
            // that marker is what tells a host the text is already a sentence.
            // This field IS the sentence — shown as the reason a control is
            // disabled, not thrown as an error — so the marker comes off here and
            // nowhere else.
            (
                sentence
                    .strip_prefix(refusal::MARKER)
                    .unwrap_or(sentence)
                    .to_owned(),
                code.unwrap_or_default().to_owned(),
            )
        },
    );
    ChartDataView {
        node: object.to_string(),
        kind,
        kinds: CHART_GALLERY.to_vec(),
        editable: refusal.is_none(),
        reason,
        replaces_workbook: chart.external_data.is_some(),
        code,
        first_column: if scatter { "x" } else { "category" },
        title: chart
            .title
            .as_ref()
            .and_then(|title| title.text.as_ref())
            .map(|text| text.text.clone())
            .unwrap_or_default(),
        legend: legend_token(chart.legend.as_ref()),
        series: series
            .iter()
            .map(|series| {
                series
                    .name
                    .as_ref()
                    .map(|name| name.text.clone())
                    .unwrap_or_default()
            })
            .collect(),
        labels,
        // Transposed to row-major, which is the shape a grid is built in and the
        // shape a row insert or delete is one splice of. The model is
        // series-major, and asking the UI to transpose would put the one piece of
        // index arithmetic that can mis-assign a column into the layer with no
        // type system.
        cells: (0..rows)
            .map(|row| {
                columns
                    .iter()
                    .map(|column| column[row].clone())
                    .collect::<Vec<String>>()
            })
            .collect(),
        max_series: MAX_CHART_SERIES_PER_GROUP,
        max_rows: MAX_CHART_DATA_POINTS,
        title_limit: MAX_CHART_TEXT_BYTES,
    }
}

/// The two axes a category family needs, and the ids a group names them by.
///
/// Byte-for-byte the pair `default_chart_projection` installs, which is asserted
/// rather than trusted (`the_axis_pair_matches_the_one_an_insert_installs`): a
/// family change has to be able to give a chart that had no axes (it was a pie)
/// the axes a column chart draws against, and two spellings of "the default axis
/// pair" is how a chart authored by changing its type comes to look different
/// from one inserted as that type.
///
/// O(1).
fn standard_axes(scatter: bool) -> (Vec<u32>, Vec<Axis>) {
    (
        vec![1, 2],
        vec![
            Axis {
                id: 1,
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
}

/// Refuses a name the model cannot hold, naming the limit and the offending text.
///
/// `MAX_CHART_TEXT_BYTES` is a BYTE ceiling, so the sentence counts bytes: a
/// message that said "characters" would be wrong by a factor of three for the
/// languages where it matters most.
///
/// Complexity: O(text length).
fn check_name(text: &str, what: &str) -> Result<(), String> {
    if text.len() <= MAX_CHART_TEXT_BYTES {
        return Ok(());
    }
    Err(refusal::marked(
        "chart.name-too-long",
        &format!(
            "{what} is {} bytes long; a chart name can hold at most {}.",
            text.len(),
            MAX_CHART_TEXT_BYTES
        ),
    ))
}

/// One cell's typed value, or the refusal that names the cell.
///
/// An empty cell is [`ChartValue::Blank`] — a state the model has, which is why
/// the grid can express "no value here" without inventing a zero. Anything else
/// has to be a finite number, and the sentence says **which** cell and what to
/// type instead, because "ValueTooLarge" aimed at a 90-cell grid is a refusal
/// that refuses nothing the reader can find.
///
/// Complexity: O(text length).
fn cell_value(text: &str, row: usize, series: &str) -> Result<ChartValue, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(ChartValue::Blank);
    }
    if trimmed.len() > MAX_CHART_NUMBER_BYTES {
        return Err(refusal::marked(
            "chart.number-too-long",
            &format!(
                "The value in row {} of “{series}” is {} characters long; a chart \
                 number can hold at most {}.",
                row + 1,
                trimmed.len(),
                MAX_CHART_NUMBER_BYTES
            ),
        ));
    }
    let value = ChartValue::Number(trimmed.to_owned());
    if value.as_f64().is_none() {
        return Err(refusal::marked(
            "chart.not-a-number",
            &format!(
                "“{trimmed}” in row {} of “{series}” is not a number. Type a number \
                 like 4.3, or clear the cell to leave a gap.",
                row + 1
            ),
        ));
    }
    // Kept in the text the reader typed, never parsed and re-formatted: `4.30`
    // and `4.3` are the same number and different documents, which is the rule
    // the chart model's own header states.
    Ok(value)
}

/// One authored [`DataRange`], preserving the number format of the range it
/// replaces.
///
/// No `c:f`: the authored data IS the cache, and a formula naming a worksheet
/// range nothing holds is a reference no reader can resolve — the same reason
/// `chart_range` omits one for an inserted chart.
///
/// Complexity: O(values).
fn authored_range(values: Vec<ChartValue>, previous: Option<&DataRange>) -> DataRange {
    DataRange {
        formula: None,
        point_count: u32::try_from(values.len()).unwrap_or(u32::MAX),
        points: values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (u32::try_from(index).unwrap_or(u32::MAX), value))
            .collect(),
        number_format: previous.and_then(|range| range.number_format.clone()),
    }
}

/// The projection `patch` describes, built on top of `chart`.
///
/// Everything the grid does not show is carried across from `chart`: each series'
/// fill, line, smoothing and data labels, the number formats, the axes and their
/// bounds, `plotVisOnly`, `dispBlanksAs` and the external-data pointer. The three
/// things that move with the FAMILY move with it — the axis pair a pie has none
/// of, `varyColors`, and whether the first column is x values — because those are
/// not decorations a reader chose, they are what the family is.
///
/// Complexity: O(series × rows), one clone of the projection.
fn apply_chart_patch(chart: &Chart, patch: &ChartDataPatch) -> Result<Chart, String> {
    let previous = chart
        .plot_area
        .groups
        .first()
        .ok_or_else(|| {
            refusal::marked(
                "chart.no-plot-area",
                "This chart has no plot area to put data in.",
            )
        })?
        .clone();
    let family_changed = chart_kind_token(previous.kind) != patch.kind;
    let kind = if family_changed {
        chart_group_for_kind(&patch.kind).ok_or_else(|| unpainted_chart_refusal(&patch.kind))?
    } else {
        previous.kind
    };

    let rows = patch.labels.len();
    let columns = patch.series.len();
    if rows == 0 || columns == 0 {
        return Err(refusal::marked(
            "chart.empty-data",
            "A chart needs at least one row of data and at least one series.",
        ));
    }
    if rows > MAX_CHART_DATA_POINTS {
        return Err(refusal::marked(
            "chart.too-many-rows",
            &format!(
                "A chart series can hold at most {MAX_CHART_DATA_POINTS} values; \
                 this would need {rows}."
            ),
        ));
    }
    if columns > MAX_CHART_SERIES_PER_GROUP {
        return Err(refusal::marked(
            "chart.too-many-series",
            &format!(
                "A chart can hold at most {MAX_CHART_SERIES_PER_GROUP} series; this \
                 would need {columns}."
            ),
        ));
    }
    check_name(&patch.title, "The chart title")?;
    for name in &patch.series {
        check_name(name, &format!("The series name “{name}”"))?;
    }

    let scatter = plots_x_values(kind);
    // The first column is numbers for a scatter chart and text for every other
    // family. A reader who switches a scatter chart to a column chart keeps the
    // numbers they typed as category NAMES, which is what Word does with the same
    // sheet — nothing is dropped by the change of family.
    let first_column: Vec<ChartValue> = if scatter {
        patch
            .labels
            .iter()
            .enumerate()
            .map(|(row, text)| cell_value(text, row, "X values"))
            .collect::<Result<_, _>>()?
    } else {
        patch
            .labels
            .iter()
            .map(|text| -> Result<ChartValue, String> {
                check_name(text, &format!("The row name “{text}”"))?;
                Ok(if text.is_empty() {
                    ChartValue::Blank
                } else {
                    ChartValue::Text(text.clone())
                })
            })
            .collect::<Result<_, _>>()?
    };

    let mut series = Vec::with_capacity(columns);
    for (column, name) in patch.series.iter().enumerate() {
        let old = previous.series.get(column);
        let values: Vec<ChartValue> = (0..rows)
            .map(|row| {
                let text = patch
                    .cells
                    .get(row)
                    .and_then(|cells| cells.get(column))
                    .map_or("", String::as_str);
                cell_value(text, row, name)
            })
            .collect::<Result<_, _>>()?;
        let labels = authored_range(
            first_column.clone(),
            old.and_then(|old| {
                if scatter {
                    old.x_values.as_ref()
                } else {
                    old.categories.as_ref()
                }
            }),
        );
        series.push(Series {
            index: u32::try_from(column).unwrap_or(u32::MAX),
            order: u32::try_from(column).unwrap_or(u32::MAX),
            name: (!name.is_empty()).then(|| ChartText {
                text: name.clone(),
                // The authored name IS the name; a retained `c:strRef/c:f` would
                // point at a cell whose contents no longer match it.
                formula: None,
            }),
            categories: (!scatter).then(|| labels.clone()),
            values: authored_range(values, old.map(|old| &old.values)),
            x_values: scatter.then_some(labels),
            // Carried across: the appearance a reader set, which the grid neither
            // shows nor has any business resetting.
            fill: old.and_then(|old| old.fill),
            line: old.and_then(|old| old.line),
            smooth: old.is_some_and(|old| old.smooth),
            data_labels: old.and_then(|old| old.data_labels),
        });
    }

    let by_point = chart_colors_by_point(kind);
    let (axis_ids, axes) = if by_point {
        // A pie draws no axis furniture, and an axis id a group names but the
        // plot area does not hold is a dangling reference the model rejects.
        (Vec::new(), Vec::new())
    } else if chart.plot_area.axes.is_empty() {
        standard_axes(scatter)
    } else {
        let mut axes = chart.plot_area.axes.clone();
        // Scatter plots both axes as values; every other family puts the
        // categories on the bottom. Without this, a column chart switched to
        // scatter kept a category axis under numeric x values and drew its ticks
        // in the wrong place.
        if let Some(bottom) = axes
            .iter_mut()
            .find(|axis| axis.position == Some(AxisPosition::Bottom))
        {
            bottom.kind = if scatter {
                AxisKind::Value
            } else {
                AxisKind::Category
            };
        }
        let ids = if previous.axis_ids.is_empty() {
            axes.iter().map(|axis| axis.id).collect()
        } else {
            previous.axis_ids.clone()
        };
        (ids, axes)
    };

    let mut next = chart.clone();
    next.title = if patch.title.trim().is_empty() {
        None
    } else {
        Some(ChartTitle {
            text: Some(ChartText {
                text: patch.title.clone(),
                formula: None,
            }),
            overlay: chart.title.as_ref().is_some_and(|title| title.overlay),
        })
    };
    // `autoTitleDeleted` is the other half of the title decision, not a separate
    // setting: a chart with no title and the flag unset asks Word to invent one
    // from the single series name, which is a title the reader did not type and
    // cannot edit here.
    next.auto_title_deleted = next.title.is_none();
    next.legend = legend_position(&patch.legend)?.map(|position| Legend {
        position,
        overlay: chart.legend.as_ref().is_some_and(|legend| legend.overlay),
    });
    next.vary_colors = by_point;
    next.plot_area.axes = axes;
    next.plot_area.groups = vec![casual_doc_model::v1::ChartGroup {
        kind,
        series,
        axis_ids,
        vary_colors: by_point,
    }];
    Ok(next)
}

#[wasm_bindgen]
impl WasmDocument {
    /// The authoring view of the chart anchored at `node`, as JSON, or `""` when
    /// `node` is not a chart object.
    ///
    /// `""` rather than an error because the data editor asks this of whatever
    /// object is selected, and "the selection is a picture" is not a failure —
    /// it is the answer that tells the panel to hide its chart section.
    ///
    /// The payload's shape is `ChartDataView`: the family and the gallery, the
    /// title and legend, the series names, the row labels, the cells as
    /// `cells[row][column]`, the model's own ceilings, and — when the data may not
    /// be changed — `editable: false` with the reader's sentence and a routing
    /// code. See this module's header for the four conditions.
    ///
    /// # Complexity
    ///
    /// O(charts) to find the projection plus O(series × rows) to build the view.
    /// Nothing walks the document.
    #[wasm_bindgen(js_name = chartData)]
    #[must_use]
    pub fn chart_data(&self, node: &str) -> String {
        // `node_id_msg`, not `node_id`: the latter's error arm builds a `JsValue`
        // eagerly, which panics on a native target — so a read that only wanted
        // to say "that is not a node id" would take the whole test process down.
        let Ok(object) = node_id_msg(node) else {
            return String::new();
        };
        let Some((_, chart)) = self
            .document
            .definitions()
            .charts
            .iter()
            .find(|(_, chart)| chart.object == object)
        else {
            return String::new();
        };
        let view = chart_view(object, chart);
        serde_json::to_string(&view).unwrap_or_default()
    }

    /// Writes the chart anchored at `node` from the data editor's payload, as one
    /// undoable action.
    ///
    /// `patch` is a `ChartDataView`-shaped JSON object carrying `kind`, `title`,
    /// `legend`, `series`, `labels` and `cells`. Every field is authoritative —
    /// the grid names the whole chart, because "row 3 changed" cannot express a
    /// deleted row — and everything the grid does not show is carried across from
    /// the existing projection rather than reset.
    ///
    /// One `SetChartDefinition` in one transaction, which is what makes the whole
    /// change one undo entry and puts it behind the protection, review-mode and
    /// windowed-layout gates every other edit goes through.
    ///
    /// # Errors
    ///
    /// When `node` is not a chart object; when this chart's data may not be
    /// authored (a partial projection or a combination chart — each with its own
    /// sentence and code);
    /// when a cell is not a number; when a name or the title is longer than the
    /// model can hold; when the grid is empty or past the model's ceilings; or
    /// when the family named is one this build does not paint.
    ///
    /// # Complexity
    ///
    /// O(charts) to find the projection, O(series × rows) to build the new one,
    /// plus the one `validate` the operation pays. Independent of document size,
    /// so typing in a cell does not walk the document (`docs/107` §4).
    #[wasm_bindgen(js_name = setChartData)]
    pub fn set_chart_data(&mut self, node: &str, patch: &str) -> Result<EditResult, JsValue> {
        let object = node_id(node)?;
        let patch: ChartDataPatch = serde_json::from_str(patch).map_err(|err| {
            to_js(refusal::marked(
                "chart.payload-unreadable",
                &format!("The chart editor sent data this build could not read: {err}"),
            ))
        })?;
        let (id, chart) = self
            .document
            .definitions()
            .charts
            .iter()
            .find(|(_, chart)| chart.object == object)
            .map(|(id, chart)| (*id, chart.clone()))
            .ok_or_else(|| {
                to_js(refusal::marked(
                    "chart.not-a-chart",
                    "That object is not a chart, so it has no data to edit.",
                ))
            })?;
        if let Some(refused) = chart_authoring_refusal(&chart) {
            return Err(to_js(refused));
        }
        let next = apply_chart_patch(&chart, &patch).map_err(to_js)?;
        if next == chart {
            // Nothing changed — a cell blurred without being retyped, or a
            // gallery click on the family already selected. Applying it anyway
            // would put an undo entry on the stack that undoes nothing, which is
            // how a reader comes to press Ctrl+Z twice to get one change back.
            return Err(to_js(refusal::marked(
                "chart.unchanged",
                "That chart already holds exactly this data.",
            )));
        }
        // Set AFTER the unchanged check, which compares content: provenance is
        // not a change a reader made, and a no-op must stay a no-op.
        let mut next = next;
        next.dirty = true;
        self.write_chart_definition(object, id, next).map_err(to_js)
    }
}

impl WasmDocument {
    /// The chart module's one write: a single [`Operation::SetChartDefinition`]
    /// through `apply_group`, the facade's atomic choke point.
    ///
    /// `apply_group` is called directly rather than through `apply_action_caret_as`
    /// for one reason: the undo label. `history_kind_for_ops` maps
    /// `SetChartDefinition` to `HistoryKind::ChartInsert`, whose label is "Insert
    /// chart" — honest for an insert and wrong for a data edit, and an undo entry
    /// has to name what it will undo. The right fix is a `HistoryKind` variant,
    /// and `lib.rs` is owned by another lane this round (recorded as owed in the
    /// report), so the label is passed to the choke point directly. This is NOT a
    /// second mutation path: `apply_group` is the same transaction envelope every
    /// other edit goes through, and `apply_action_caret_as` is a label-and-caret
    /// convenience over it.
    ///
    /// The caret rests at the chart object, as every other object edit's does
    /// (`objects.rs` passes `Pos::new(object, 0)`), so a data edit never moves the
    /// reader's place in the text.
    ///
    /// Complexity: one operation's `validate` plus the repaint the caller pays.
    fn write_chart_definition(
        &mut self,
        object: NodeId,
        id: ChartId,
        chart: Chart,
    ) -> Result<EditResult, String> {
        let ops = vec![Operation::SetChartDefinition {
            id,
            chart: Some(Box::new(chart)),
        }];
        self.typing_history = None;
        self.apply_group(&ops, "Chart data", Coalesce::New, Origin::Edit)?;
        Ok(self.finish_edit_with(Pos::new(object, 0), &damage_of(&ops)))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CHART_GALLERY, ChartDataPatch, apply_chart_patch, chart_authoring_refusal,
        chart_kind_token, chart_view, standard_axes,
    };
    use crate::{
        WasmDocument, chart_group_for_kind, default_chart_projection, tests::wasm_document,
    };
    use casual_doc_edit::Operation;
    use casual_doc_layout::block::BlockFragment;
    use casual_doc_layout::text::ChartPrimitive;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BarDirection, BarGrouping, BlockNode, Chart, ChartCoverage, ChartGroupKind, Color,
        Definitions, Document, EmbeddedKind, EmbeddedObject, EmbeddedPart, Extent, InlineNode,
        LegendPosition, Paragraph, ParagraphProperties, RgbColor,
    };

    /// An empty one-paragraph document whose identity space is `seed`, and the
    /// paragraph an insert addresses.
    ///
    /// `seed` IS the document's own `IdSpace`, which is what makes the imported /
    /// authored distinction testable here: a node built with `from_parts(seed, n)`
    /// is one the importer would have minted, and one `insert_chart` mints is not.
    fn empty_body_document(seed: u64) -> (Document, NodeId) {
        let paragraph = NodeId::from_parts(seed, 2).expect("a paragraph id");
        let document = Document::new(
            NodeId::from_parts(seed, 1).expect("a document id"),
            vec![BlockNode::Paragraph(Paragraph {
                id: paragraph,
                properties: ParagraphProperties::default().into(),
                inlines: Vec::new(),
            })],
            Definitions::default(),
        )
        .expect("a valid one-paragraph document");
        (document, paragraph)
    }

    /// A document holding one inserted chart, and that chart's object node.
    fn document_with_a_chart(seed: u64, kind: &str) -> (WasmDocument, NodeId) {
        let (document, paragraph) = empty_body_document(seed);
        let mut d = wasm_document(document);
        d.insert_chart(&paragraph.to_string(), 0, kind)
            .unwrap_or_else(|err| panic!("inserting a {kind} chart: {err:?}"));
        let object = d
            .document
            .definitions()
            .charts
            .iter()
            .map(|(_, chart)| chart.object)
            .next()
            .expect("the inserted chart's projection");
        (d, object)
    }

    /// Every filled rectangle the chart painted, as `(x, height)` in twips.
    ///
    /// A column chart's bars are `ChartPrimitive::Rect`s, so this is the painted
    /// geometry a value change has to move. Reported with the x so an assertion
    /// can name WHICH bar grew rather than only that the set of heights changed —
    /// the difference between "something repainted" and "the number I typed is in
    /// the bar I typed it into".
    fn painted_bars(d: &WasmDocument) -> Vec<(i32, i32)> {
        d.painted_layout()
            .pages
            .iter()
            .flat_map(|page| &page.placed)
            .filter_map(|placed| match &placed.fragment {
                BlockFragment::Paragraph { lines, .. } => Some(lines),
                _ => None,
            })
            .flat_map(|lines| &lines.lines)
            .flat_map(|line| &line.charts)
            .flat_map(|chart| &chart.primitives)
            .filter_map(|primitive| match primitive {
                ChartPrimitive::Rect {
                    rect,
                    fill: Some(_),
                    ..
                } => Some((rect.origin.x.raw(), rect.size.height.raw())),
                _ => None,
            })
            .collect()
    }

    /// The patch a view round-trips to unchanged — the editor's "nothing was
    /// typed yet" payload, which every test that changes one thing starts from.
    ///
    /// Built by parsing the JSON `chartData` actually returns, not by reading the
    /// struct: the field names and the row-major cell shape are the contract the
    /// host depends on, and a test that bypassed the JSON could not catch a
    /// renamed field.
    fn patch_of(d: &WasmDocument, object: NodeId) -> ChartDataPatch {
        let json = d.chart_data(&object.to_string());
        assert!(!json.is_empty(), "the chart has no authoring view");
        let view: serde_json::Value = serde_json::from_str(&json).expect("the view parses");
        let strings = |key: &str| -> Vec<String> {
            view[key]
                .as_array()
                .unwrap_or_else(|| panic!("the view has no {key}"))
                .iter()
                .map(|value| value.as_str().unwrap_or_default().to_owned())
                .collect()
        };
        ChartDataPatch {
            kind: view["kind"].as_str().expect("a kind").to_owned(),
            title: view["title"].as_str().expect("a title").to_owned(),
            legend: view["legend"].as_str().expect("a legend").to_owned(),
            series: strings("series"),
            labels: strings("labels"),
            cells: view["cells"]
                .as_array()
                .expect("cells")
                .iter()
                .map(|row| {
                    row.as_array()
                        .expect("a row")
                        .iter()
                        .map(|cell| cell.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .collect(),
        }
    }

    /// `set_chart_data`'s two halves, called without crossing the JS boundary.
    ///
    /// `to_js` cannot build a `JsValue` on a native target, so calling
    /// `set_chart_data` itself would fail with "cannot call wasm-bindgen imported
    /// functions" instead of with the reason — and a guard whose red says nothing
    /// is half a guard. This is the same gate, the same patch and the same one
    /// operation, in the same order.
    fn write(d: &mut WasmDocument, object: NodeId, patch: &ChartDataPatch) -> Result<(), String> {
        let (id, chart) = d
            .document
            .definitions()
            .charts
            .iter()
            .find(|(_, chart)| chart.object == object)
            .map(|(id, chart)| (*id, chart.clone()))
            .ok_or_else(|| "not a chart".to_owned())?;
        if let Some(refused) = chart_authoring_refusal(&chart) {
            return Err(refused);
        }
        let mut next = apply_chart_patch(&chart, patch)?;
        next.dirty = true;
        d.write_chart_definition(object, id, next).map(|_| ())
    }

    fn projection(d: &WasmDocument) -> Chart {
        d.document
            .definitions()
            .charts
            .iter()
            .map(|(_, chart)| chart.clone())
            .next()
            .expect("a projection")
    }

    /// **Typing a value changes the model AND the bar it was typed into.**
    ///
    /// The round-trip guard the whole lane turns on, and it is deliberately both
    /// halves: a guard that only reads the grid back passes while the model still
    /// holds the old number, and a guard that only reads the model passes while
    /// nothing repaints. So this asserts the number reached the projection, that
    /// re-reading the grid gives it back, and that the BAR for that row became the
    /// tallest — which can only be true if the authored number reached the
    /// painter.
    #[test]
    fn typing_a_value_changes_the_model_and_the_painted_bar() {
        let (mut d, object) = document_with_a_chart(9_100, "column");
        let before = painted_bars(&d);
        assert!(!before.is_empty(), "the inserted chart painted no bars");

        let mut patch = patch_of(&d, object);
        // Row 1 of series 1 is Word's sample `4.3`; 400 is unmistakably the
        // largest value in the chart, so the tallest bar must move to row 1.
        patch.cells[0][0] = "400".to_owned();
        write(&mut d, object, &patch).expect("the data edit applies");

        let stored = projection(&d).plot_area.groups[0].series[0].values.clone();
        assert_eq!(
            stored
                .points
                .iter()
                .find(|(index, _)| *index == 0)
                .map(|(_, value)| value.clone()),
            Some(casual_doc_model::v1::ChartValue::Number("400".to_owned())),
            "the authored value never reached the projection: {stored:?}"
        );
        assert_eq!(
            patch_of(&d, object).cells[0][0],
            "400",
            "the grid does not read back the value it wrote"
        );

        let after = painted_bars(&d);
        assert_ne!(before, after, "the chart did not repaint after a data edit");
        let tallest = after
            .iter()
            .max_by_key(|(_, height)| *height)
            .expect("a tallest bar");
        let leftmost = after
            .iter()
            .filter(|(_, height)| *height > 0)
            .min_by_key(|(x, _)| *x)
            .expect("a leftmost bar");
        assert_eq!(
            tallest.0, leftmost.0,
            "row 1 now holds 400, the largest value in the chart, so the tallest \
             bar must be the leftmost one. Painted (x, height) pairs: {after:?}"
        );
    }

    /// **Adding a row and a series grows the grid; removing them shrinks it.**
    ///
    /// The complaint is "how can i add data", so the two gestures that add data
    /// are asserted end to end rather than at the engine's door.
    #[test]
    fn rows_and_series_can_be_added_and_removed() {
        let (mut d, object) = document_with_a_chart(9_101, "column");
        let start = patch_of(&d, object);
        assert_eq!(start.labels.len(), 4, "Word's sample data is four rows");
        assert_eq!(start.series.len(), 3, "…and three series");

        let mut grown = start.clone();
        grown.labels.push("Category 5".to_owned());
        grown.series.push("Series 4".to_owned());
        grown.cells = (0..5)
            .map(|row| {
                (0..4)
                    .map(|column| (row * 4 + column + 1).to_string())
                    .collect()
            })
            .collect();
        write(&mut d, object, &grown).expect("a grown grid applies");
        let view = patch_of(&d, object);
        assert_eq!(view.labels.len(), 5, "the added row is not in the model");
        assert_eq!(view.series.len(), 4, "the added series is not in the model");
        assert_eq!(view.series[3], "Series 4", "the series name was not kept");
        assert_eq!(view.cells[4][3], "20", "the new cell's value was not kept");

        let mut shrunk = view.clone();
        shrunk.labels.truncate(2);
        shrunk.series.truncate(1);
        shrunk.cells.truncate(2);
        for row in &mut shrunk.cells {
            row.truncate(1);
        }
        write(&mut d, object, &shrunk).expect("a shrunk grid applies");
        let view = patch_of(&d, object);
        assert_eq!(
            (view.labels.len(), view.series.len()),
            (2, 1),
            "removing rows and series left them in the model: {view:?}"
        );
    }

    /// **A blank cell is a blank cache entry, not a zero.**
    ///
    /// `ChartValue::Blank` is a state the model has and `c:dispBlanksAs` decides
    /// how it draws, so an empty cell has to stay empty: writing it as `0` would
    /// plot a point the reader never entered and read back as `"0"`.
    #[test]
    fn an_empty_cell_stays_empty_rather_than_becoming_a_zero() {
        let (mut d, object) = document_with_a_chart(9_102, "line");
        let mut patch = patch_of(&d, object);
        patch.cells[1][0] = String::new();
        write(&mut d, object, &patch).expect("a cleared cell applies");
        let values = projection(&d).plot_area.groups[0].series[0].values.clone();
        assert_eq!(
            values.points.iter().find(|(index, _)| *index == 1),
            Some(&(1, casual_doc_model::v1::ChartValue::Blank)),
            "a cleared cell did not become a blank cache entry: {values:?}"
        );
        assert_eq!(
            patch_of(&d, object).cells[1][0],
            "",
            "a blank cache entry reads back as something other than an empty cell"
        );
    }

    /// **A cell that is not a number says which cell, and what to type instead.**
    ///
    /// `SKILL` §10's "say something when it refuses", against the live
    /// counter-example of an edit refused as `ValueTooLarge`: a refusal aimed at a
    /// ninety-cell grid has to name the cell.
    #[test]
    fn a_cell_that_is_not_a_number_names_the_cell_and_the_series() {
        let (mut d, object) = document_with_a_chart(9_103, "column");
        let mut patch = patch_of(&d, object);
        patch.cells[2][1] = "about nine".to_owned();
        let refused = write(&mut d, object, &patch).expect_err("a word is not a value");
        assert!(
            refused.contains("chart.not-a-number"),
            "the refusal carries no routing code: {refused}"
        );
        for part in ["about nine", "row 3", "Series 2", "4.3"] {
            assert!(
                refused.contains(part),
                "the refusal does not mention {part:?}, so the reader cannot find \
                 the cell or tell what to type: {refused}"
            );
        }
        assert_eq!(
            patch_of(&d, object).cells[2][1],
            "1.8",
            "a refused edit changed the chart anyway"
        );
    }

    /// **An imported chart is editable, and the edit makes it the authority.**
    ///
    /// The silent-data-loss gate, inverted. `casual-doc-export` re-emits a
    /// retained chart part verbatim unless the projection is dirty, so an edit
    /// that did not set the bit would repaint, pass every test of the editing
    /// path, and be discarded on save. And a chart that names an embedded
    /// workbook tells the reader, before they change it, that the workbook is
    /// replaced.
    #[test]
    fn an_imported_chart_is_editable_and_its_edit_marks_it_dirty() {
        let (document, paragraph) = empty_body_document(9_104);
        let mut d = wasm_document(document);
        d.insert_chart(&paragraph.to_string(), 0, "column")
            .expect("a column chart");
        // Make it look exactly like an import: a workbook beside it, not dirty.
        let (id, mut chart) = d
            .document
            .definitions()
            .charts
            .iter()
            .map(|(id, chart)| (*id, chart.clone()))
            .next()
            .expect("the chart");
        chart.external_data = Some(EmbeddedPart {
            relationship_id: "rId1".to_owned(),
            relationship_type: "http://example.invalid/package".to_owned(),
            part_name: "word/embeddings/book.xlsx".to_owned(),
        });
        let object = chart.object;
        d.apply_group(
            &[Operation::SetChartDefinition {
                id,
                chart: Some(Box::new(chart.clone())),
            }],
            "seed",
            casual_doc_transaction::Coalesce::New,
            casual_doc_transaction::Origin::Edit,
        )
        .expect("seeding the imported shape");
        assert!(!projection(&d).dirty, "the seed must start clean");

        let view = chart_view(object, &projection(&d));
        assert!(
            view.editable,
            "an imported chart must be editable: {}",
            view.reason
        );
        assert!(
            view.replaces_workbook,
            "the reader is not told the embedded workbook will be replaced"
        );

        let mut patch = patch_of(&d, object);
        patch.cells[0][0] = "9".to_owned();
        let json = serde_json::to_string(&serde_json::json!({
            "kind": patch.kind, "title": patch.title, "legend": patch.legend,
            "series": patch.series, "labels": patch.labels, "cells": patch.cells,
        }))
        .expect("a payload");
        d.set_chart_data(&object.to_string(), &json)
            .unwrap_or_else(|_| panic!("the facade write refused an imported chart"));
        let after = projection(&d);
        assert!(
            after.dirty,
            "an edited chart was left clean, so the save would discard it"
        );
        assert_eq!(
            after.external_data, chart.external_data,
            "the workbook pointer is what the exporter replaces in place; dropping it \
             would orphan the old workbook in the file"
        );
    }

    /// **The two refusals each say what they refused, and a workbook is not one.**
    #[test]
    fn a_partial_or_combo_chart_refuses_with_its_own_reason() {
        let space = casual_doc_model::IdSpace::new(9_105);
        let object = NodeId::from_parts(casual_doc_model::IdSpace::local(space).get(), 5)
            .expect("an edit id");
        let base = {
            let mut chart = default_chart_projection(
                object,
                chart_group_for_kind("column").expect("a column family"),
            );
            chart.object = object;
            chart
        };

        let mut partial = base.clone();
        partial.coverage = ChartCoverage::Partial;
        let mut combo = base.clone();
        combo
            .plot_area
            .groups
            .push(base.plot_area.groups[0].clone());
        let mut workbook = base.clone();
        workbook.external_data = Some(EmbeddedPart {
            relationship_id: "rId9".to_owned(),
            relationship_type: "http://example.invalid/package".to_owned(),
            part_name: "word/embeddings/book.xlsx".to_owned(),
        });

        for (chart, code, phrase) in [
            (&partial, "chart.partial-coverage", "does not model yet"),
            (&combo, "chart.many-groups", "more than one chart type"),
        ] {
            let refused =
                chart_authoring_refusal(chart).unwrap_or_else(|| panic!("{code} must refuse"));
            assert!(refused.contains(code), "no routing code: {refused}");
            assert!(
                refused.contains(phrase),
                "the sentence for {code} does not explain itself: {refused}"
            );
        }
        assert_eq!(
            chart_authoring_refusal(&workbook),
            None,
            "a workbook-backed chart is editable: the save replaces the workbook"
        );
        assert_eq!(chart_authoring_refusal(&base), None);
    }

    /// **A data edit keeps everything the grid does not show.**
    ///
    /// `SetChartDefinition` replaces the whole projection, so a patch built from
    /// the grid alone would drop a series' fill, its smoothing, its data labels
    /// and the axis bounds — `AGENTS.md`'s no-silent-data-loss rule applied to our
    /// own edit path.
    #[test]
    fn a_data_edit_keeps_the_formatting_the_grid_does_not_show() {
        let (mut d, object) = document_with_a_chart(9_106, "column");
        let (id, mut dressed) = d
            .document
            .definitions()
            .charts
            .iter()
            .map(|(id, chart)| (*id, chart.clone()))
            .next()
            .expect("the projection");
        let orange = Color::Rgb(RgbColor {
            r: 0xff,
            g: 0x88,
            b: 0x00,
        });
        dressed.plot_area.groups[0].series[1].fill = Some(orange);
        dressed.plot_area.groups[0].series[1].smooth = true;
        dressed.plot_area.axes[1].maximum = Some("12.5".to_owned());
        d.apply(Operation::SetChartDefinition {
            id,
            chart: Some(Box::new(dressed)),
        })
        .expect("dressing the chart");

        let mut patch = patch_of(&d, object);
        patch.cells[0][0] = "7.25".to_owned();
        write(&mut d, object, &patch).expect("the data edit applies");

        let after = projection(&d);
        assert_eq!(
            after.plot_area.groups[0].series[1].fill,
            Some(orange),
            "the series fill was reset by a data edit"
        );
        assert!(
            after.plot_area.groups[0].series[1].smooth,
            "the series' smoothing was reset by a data edit"
        );
        assert_eq!(
            after.plot_area.axes[1].maximum.as_deref(),
            Some("12.5"),
            "the axis bound was reset by a data edit"
        );
    }

    /// **Changing the family keeps the data, and moves the axes with it.**
    ///
    /// Word's Change Chart Type keeps the sheet. A pie has no axes at all, and an
    /// axis id a group names but the plot area does not hold is a dangling
    /// reference the model rejects — so the axes have to leave with the family and
    /// come back with it.
    #[test]
    fn changing_the_family_keeps_the_data_and_moves_the_axes() {
        let (mut d, object) = document_with_a_chart(9_107, "column");
        let data = patch_of(&d, object).cells.clone();

        let mut to_pie = patch_of(&d, object);
        to_pie.kind = "pie".to_owned();
        write(&mut d, object, &to_pie).expect("switching to a pie applies");
        let pie = projection(&d);
        assert!(
            pie.plot_area.axes.is_empty() && pie.plot_area.groups[0].axis_ids.is_empty(),
            "a pie kept axes: {:?}",
            pie.plot_area
        );
        assert!(
            pie.vary_colors && pie.plot_area.groups[0].vary_colors,
            "a pie must colour by point"
        );
        assert_eq!(
            patch_of(&d, object).cells,
            data,
            "switching family lost the data the reader typed"
        );
        assert!(
            d.document.validate().is_ok(),
            "the pie is not a valid model"
        );

        let mut back = patch_of(&d, object);
        back.kind = "line".to_owned();
        write(&mut d, object, &back).expect("switching back applies");
        let line = projection(&d);
        assert_eq!(
            (
                line.plot_area.axes.len(),
                line.plot_area.groups[0].axis_ids.len()
            ),
            (2, 2),
            "switching away from a pie did not restore the axis pair: {:?}",
            line.plot_area
        );
        assert!(
            !line.vary_colors,
            "a line chart must colour by series, not by point"
        );
        assert_eq!(
            patch_of(&d, object).cells,
            data,
            "switching family twice lost the data"
        );
        assert!(d.document.validate().is_ok(), "the line chart is invalid");
    }

    /// **The title and the legend are authorable, and clearing the title sets
    /// `autoTitleDeleted` rather than inviting Word to invent one.**
    #[test]
    fn the_title_and_the_legend_are_authorable() {
        let (mut d, object) = document_with_a_chart(9_108, "column");
        assert_eq!(patch_of(&d, object).title, "", "a new chart has no title");

        let mut patch = patch_of(&d, object);
        patch.title = "Quarterly revenue".to_owned();
        patch.legend = "right".to_owned();
        write(&mut d, object, &patch).expect("a title and legend apply");
        let chart = projection(&d);
        assert_eq!(
            chart
                .title
                .as_ref()
                .and_then(|title| title.text.as_ref())
                .map(|text| text.text.as_str()),
            Some("Quarterly revenue")
        );
        assert!(
            !chart.auto_title_deleted,
            "a chart with a title must not also say its title was deleted"
        );
        assert_eq!(
            chart.legend.map(|legend| legend.position),
            Some(LegendPosition::Right)
        );
        assert_eq!(patch_of(&d, object).title, "Quarterly revenue");

        let mut cleared = patch_of(&d, object);
        cleared.title = String::new();
        cleared.legend = "none".to_owned();
        write(&mut d, object, &cleared).expect("clearing applies");
        let chart = projection(&d);
        assert!(chart.title.is_none() && chart.auto_title_deleted);
        assert!(chart.legend.is_none(), "the legend was not removed");
        assert_eq!(patch_of(&d, object).legend, "none");
    }

    /// **One data edit is one undo step, named for what it does.**
    #[test]
    fn one_data_edit_is_one_undo_step_named_chart_data() {
        let (mut d, object) = document_with_a_chart(9_109, "column");
        let before = patch_of(&d, object).cells.clone();
        let mut patch = patch_of(&d, object);
        patch.cells[0][0] = "99".to_owned();
        patch.title = "Sales".to_owned();
        patch.legend = "top".to_owned();
        write(&mut d, object, &patch).expect("the edit applies");
        assert_eq!(
            d.undo_label(),
            "Chart data",
            "the undo entry does not name a chart data edit"
        );
        d.undo_inner().expect("one undo takes the whole edit back");
        let view = patch_of(&d, object);
        assert_eq!(view.cells, before, "one undo did not restore the data");
        assert_eq!(view.title, "", "one undo did not restore the title");
        assert_eq!(view.legend, "bottom", "one undo did not restore the legend");
    }

    /// **The view the engine produced re-applies to the same projection**, so a
    /// blur with nothing typed cannot stack an undo entry that undoes nothing.
    #[test]
    fn an_unchanged_patch_changes_nothing() {
        let (d, object) = document_with_a_chart(9_110, "column");
        let patch = patch_of(&d, object);
        let chart = projection(&d);
        let next = apply_chart_patch(&chart, &patch).expect("the identity patch builds");
        assert_eq!(
            next, chart,
            "re-applying the view the engine just produced changed the projection, \
             so every blur would stack an undo entry"
        );
    }

    /// **The grid is rows of categories and columns of series.**
    ///
    /// The one piece of index arithmetic that can silently mis-assign a column.
    /// Three series of four categories is not square, so a transposition cannot
    /// hide behind a symmetric fixture.
    #[test]
    fn the_grid_is_rows_of_categories_and_columns_of_series() {
        let (d, object) = document_with_a_chart(9_111, "column");
        let view = patch_of(&d, object);
        assert_eq!(view.cells.len(), 4, "four rows, one per category");
        assert!(
            view.cells.iter().all(|row| row.len() == 3),
            "three columns, one per series: {:?}",
            view.cells
        );
        // Word's sample data, series-major: 4.3/2.5/3.5/4.5, 2.4/4.4/1.8/2.8,
        // 2.0/2.0/3.0/5.0. Read row-major, row 1 is the three series' first
        // values.
        assert_eq!(view.cells[0], vec!["4.3", "2.4", "2.0"]);
        assert_eq!(view.cells[3], vec!["4.5", "2.8", "5.0"]);
        assert_eq!(view.labels[0], "Category 1");
        assert_eq!(view.series, vec!["Series 1", "Series 2", "Series 3"]);
    }

    /// **A scatter chart's first column is its x values, and they stay numbers.**
    #[test]
    fn a_scatter_charts_first_column_is_its_x_values() {
        let (mut d, object) = document_with_a_chart(9_112, "scatter");
        let json = d.chart_data(&object.to_string());
        let view: serde_json::Value = serde_json::from_str(&json).expect("the view parses");
        assert_eq!(view["firstColumn"], "x", "a scatter chart's column A is x");
        let mut patch = patch_of(&d, object);
        patch.labels[0] = "1.5".to_owned();
        write(&mut d, object, &patch).expect("an x value applies");
        let series = projection(&d).plot_area.groups[0].series[0].clone();
        assert_eq!(
            series
                .x_values
                .as_ref()
                .and_then(|range| range.points.first())
                .map(|(_, value)| value.clone()),
            Some(casual_doc_model::v1::ChartValue::Number("1.5".to_owned())),
            "the x value did not reach `c:xVal`: {series:?}"
        );
        assert!(
            series.categories.is_none(),
            "a scatter series must not also carry category labels"
        );

        // A word in the x column is refused with the same specificity as a cell.
        let mut bad = patch_of(&d, object);
        bad.labels[1] = "second".to_owned();
        let refused = write(&mut d, object, &bad).expect_err("a word is not an x value");
        assert!(
            refused.contains("chart.not-a-number") && refused.contains("X values"),
            "an x-column refusal must name the column: {refused}"
        );
    }

    /// **Every gallery token is a family this build can insert and paint.**
    ///
    /// The anti-drift guard the gallery needs: a token the UI offers that
    /// `chart_group_for_kind` rejects is a dead control, and a family the engine
    /// paints that the gallery omits is a capability nobody can reach.
    /// `chart_kind_token`'s exhaustive match covers the other direction at compile
    /// time.
    #[test]
    fn every_gallery_token_is_an_insertable_family() {
        for token in CHART_GALLERY {
            let group = chart_group_for_kind(token)
                .unwrap_or_else(|| panic!("the gallery offers {token}, which cannot be inserted"));
            assert_eq!(
                chart_kind_token(group),
                token,
                "{token} does not round-trip through the family table"
            );
        }
        assert_eq!(
            CHART_GALLERY.len(),
            7,
            "the gallery lost or gained a family without this guard being revisited"
        );
        assert_eq!(
            chart_kind_token(ChartGroupKind::Bar {
                direction: BarDirection::Bar,
                grouping: BarGrouping::Clustered,
                gap_width: 150,
                overlap: -27,
            }),
            "bar",
            "a horizontal bar group must not read as a column chart"
        );
    }

    /// **The axis pair a family change installs is the one an insert installs.**
    ///
    /// Two spellings of "the default axis pair" is how a chart authored by
    /// changing its type comes to look different from one inserted as that type.
    #[test]
    fn the_axis_pair_matches_the_one_an_insert_installs() {
        for (scatter, kind) in [(false, "column"), (true, "scatter")] {
            let inserted = default_chart_projection(
                NodeId::from_parts(1, 1).expect("an id"),
                chart_group_for_kind(kind).expect("a family"),
            );
            let (ids, axes) = standard_axes(scatter);
            assert_eq!(
                axes, inserted.plot_area.axes,
                "the {kind} axis pair a family change installs differs from the one \
                 an insert installs"
            );
            assert_eq!(ids, inserted.plot_area.groups[0].axis_ids);
        }
    }

    /// **An empty grid, and one past the model's ceilings, are both refused with
    /// the number in the sentence.**
    #[test]
    fn an_empty_or_oversized_grid_is_refused_with_the_limit() {
        let object = NodeId::from_parts(
            casual_doc_model::IdSpace::local(casual_doc_model::IdSpace::new(9_113)).get(),
            3,
        )
        .expect("an edit id");
        let chart = {
            let mut chart = default_chart_projection(
                object,
                chart_group_for_kind("column").expect("a column family"),
            );
            chart.object = object;
            chart
        };

        let empty = ChartDataPatch {
            kind: "column".to_owned(),
            legend: "bottom".to_owned(),
            ..ChartDataPatch::default()
        };
        let refused = apply_chart_patch(&chart, &empty).expect_err("an empty grid is no chart");
        assert!(
            refused.contains("chart.empty-data") && refused.contains("at least one"),
            "{refused}"
        );

        let wide = ChartDataPatch {
            kind: "column".to_owned(),
            legend: "bottom".to_owned(),
            labels: vec!["1".to_owned()],
            series: (0..300).map(|index| format!("S{index}")).collect(),
            cells: vec![vec!["1".to_owned(); 300]],
            ..ChartDataPatch::default()
        };
        let refused = apply_chart_patch(&chart, &wide).expect_err("300 series is too many");
        assert!(
            refused.contains("chart.too-many-series") && refused.contains("256"),
            "the refusal must name the limit: {refused}"
        );
    }

    /// **A family the gallery cannot name is refused with the families that
    /// work** — the sentence `insertChart` already writes, reused rather than
    /// reworded, so the two surfaces cannot drift.
    #[test]
    fn an_unpainted_family_is_refused_with_the_families_that_work() {
        let object = NodeId::from_parts(
            casual_doc_model::IdSpace::local(casual_doc_model::IdSpace::new(9_114)).get(),
            3,
        )
        .expect("an edit id");
        let mut chart = default_chart_projection(
            object,
            chart_group_for_kind("column").expect("a column family"),
        );
        chart.object = object;
        let patch = ChartDataPatch {
            kind: "radar".to_owned(),
            legend: "bottom".to_owned(),
            labels: vec!["a".to_owned()],
            series: vec!["S".to_owned()],
            cells: vec![vec!["1".to_owned()]],
            ..ChartDataPatch::default()
        };
        let refused = apply_chart_patch(&chart, &patch).expect_err("no radar family");
        assert!(
            refused.contains("chart.unpainted-family") && refused.contains("doughnut"),
            "{refused}"
        );
    }

    /// **`chartData` answers nothing for an object that is not a chart**, because
    /// the panel asks it of whatever is selected and "that is an OLE embedding" is
    /// an answer rather than a failure.
    #[test]
    fn a_non_chart_object_has_no_authoring_view() {
        let (document, paragraph) = empty_body_document(9_115);
        let mut d = wasm_document(document);
        let embedding = NodeId::from_parts(
            casual_doc_model::IdSpace::local(casual_doc_model::IdSpace::new(9_115)).get(),
            8,
        )
        .expect("an edit id");
        d.apply(Operation::InsertInlineObject {
            at: casual_doc_edit::Pos::new(paragraph, 0),
            node: Box::new(InlineNode::EmbeddedObject(Box::new(EmbeddedObject {
                id: embedding,
                kind: EmbeddedKind::OleObject,
                part: EmbeddedPart {
                    relationship_id: "rId4".to_owned(),
                    relationship_type: "http://example.invalid/oleObject".to_owned(),
                    part_name: "word/embeddings/thing.bin".to_owned(),
                },
                extra_parts: Vec::new(),
                preview: None,
                extent: Extent {
                    width_emu: 914_400,
                    height_emu: 914_400,
                },
                prog_id: None,
            }))),
        })
        .expect("an embedded object that is not a chart");
        assert_eq!(d.chart_data(&embedding.to_string()), "");
        assert_eq!(d.chart_data("not-a-node-id"), "");
    }

    /// **The view carries the model's own ceilings**, so the editor can disable
    /// Add row and Add series WITH the number rather than refusing afterwards.
    #[test]
    fn the_view_publishes_the_models_ceilings() {
        let (d, object) = document_with_a_chart(9_116, "column");
        let chart = projection(&d);
        let view = chart_view(object, &chart);
        assert_eq!(view.max_series, 256);
        assert_eq!(view.max_rows, 32_768);
        assert_eq!(view.title_limit, 1_024);
        assert!(view.editable && view.reason.is_empty() && view.code.is_empty());
        assert_eq!(view.kinds.len(), 7, "the gallery is published to the host");
    }
}
