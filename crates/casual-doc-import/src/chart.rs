//! DrawingML chart-part reading: `word/charts/chartN.xml` -> the typed
//! `v1::Chart` **read projection** (`docs/155` §5, §8; ADR-050).
//!
//! # The three rules this module exists to hold
//!
//! 1. **The cache is the data.** A `c:ser` names its values twice — a `c:f`
//!    formula into an embedded workbook, and a `c:numCache`/`c:strCache` holding
//!    what that formula evaluated to when the producer last saved. The cache is
//!    what Word paints and what ONLYOFFICE's *document* editor paints
//!    (`docs/155` §3.2: their `recalculateReferences` returns immediately when
//!    there is no worksheet, and a worksheet is only ever set from their
//!    spreadsheet code). So the cache is what this reader reads.
//! 2. **`c:f` is carried verbatim and never parsed.** It is stored so a
//!    regenerated part can re-emit it unchanged, and for no other purpose. This
//!    module contains no cell reference, sheet name, range or operator handling,
//!    and must never acquire any: that is the first step of growing a spreadsheet
//!    engine inside a document editor, and `opencalc` is a separate product.
//! 3. **The embedded workbook is a pointer, never bytes, and is never opened.**
//!    `c:externalData@r:id` becomes an `EmbeddedPart` — a relationship id, a
//!    relationship type and a part name (`docs/45` invariant I4). The bytes stay
//!    in the opaque side-table, byte-preserved.
//!
//! # Declining is a first-class outcome
//!
//! `docs/155` §6.1 consequence 3: *a projection failure is never a document
//! failure.* A chart part that is malformed, over-limit, or of an out-of-scope
//! family yields **no projection** and the chart behaves exactly as it does
//! today — the part is still retained byte-for-byte and the loss is still
//! reported. Every error inside this module is therefore converted into a
//! [`ChartDecline`] rather than propagated: [`read_chart_part`] returns no
//! `Result`.
//!
//! # What "unconsumed" means here
//!
//! Every element the projection does not represent is recorded by local name in
//! [`ChartRead::unconsumed`], and its subtree is skipped so one dropped construct
//! is one finding (the same rule `theme::parse` follows). A non-empty set makes
//! the projection [`ChartCoverage::Partial`], which forbids regeneration. The
//! driver turns the set into per-construct report findings charged to the chart
//! part; this module does not touch the `Reporter`, because the per-construct
//! retention outcome for a chart construct is "the opaque side-table has the
//! bytes", a fact only the driver holds.
//!
//! # Complexity
//!
//! O(bytes in the chart part), once, at import. Bounded by the configured
//! element/depth/text ceilings and by the model's own list bounds, each of which
//! turns into a decline rather than an error.

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::v1::{
    Axis, AxisKind, AxisOrientation, AxisPosition, BarDirection, BarGrouping, Chart, ChartCoverage,
    ChartGroup, ChartGroupKind, ChartLine, ChartText, ChartTitle, ChartValue, Color,
    DataLabelPosition, DataLabels, DataRange, DisplayBlanks, EmbeddedPart, Grouping, Legend,
    LegendPosition, MAX_CHART_AXES, MAX_CHART_DATA_POINTS, MAX_CHART_FORMULA_BYTES,
    MAX_CHART_GROUPS, MAX_CHART_NUMBER_BYTES, MAX_CHART_SERIES_PER_GROUP, MAX_CHART_TEXT_BYTES,
    PlotArea, ScatterStyle, Series, ThemeColor, ThemeColorRef, TickLabelPosition, TickMark,
};
use casual_doc_model::{IdGenerator, NodeId};
// Own `use` line, kept out of the sorted block above: the repo's parallel-PR
// rule, so two lanes adding model imports do not collide in one list.
use casual_doc_model::v1::{BlockNode, ChartId, DefinitionMap, EmbeddedKind, InlineNode};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::body::EmbeddedRel;
use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, is_true, parse_rgb};

/// Ceiling on distinct unconsumed construct names recorded for one chart part.
///
/// A hostile part could name ten thousand distinct foreign elements; the
/// projection must stay bounded, and past this point the part is far outside
/// anything the projection can claim to cover anyway.
const MAX_UNCONSUMED_NAMES: usize = 256;

/// Why a chart part yielded no projection.
///
/// Each variant is a *reportable* state, not an error: the chart renders exactly
/// as it does today and its bytes are preserved either way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ChartDecline {
    /// The part is not well-formed XML, is not a `c:chartSpace`, or exceeded a
    /// configured XML ceiling.
    Malformed,
    /// The part's plot area is of a family outside tier 1 (`docs/155` §4.3):
    /// any `*3DChart`, `c:surfaceChart`, `c:stockChart`, `c:radarChart`,
    /// `c:bubbleChart` or `c:ofPieChart`. Carries the group's local name.
    OutOfScopeFamily(String),
    /// A list or string in the part exceeded a model bound (`docs/155` §8.4).
    /// Carries the stable bound name.
    OverBound(&'static str),
    /// The part parsed but carries no plot area with a tier-1 group, so there is
    /// nothing to project.
    NothingToProject,
}

/// The result of reading one chart part.
#[derive(Clone, Debug)]
pub(crate) struct ChartRead {
    /// The projection, when one was built.
    pub(crate) projection: Option<Chart>,
    /// Why no projection was built, when none was.
    pub(crate) declined: Option<ChartDecline>,
    /// Local names of constructs inside the part the projection did not consume,
    /// deduplicated and in a deterministic order. Non-empty implies
    /// [`ChartCoverage::Partial`] on a projection that was built.
    pub(crate) unconsumed: BTreeSet<String>,
}

impl ChartRead {
    /// A read that declined, carrying whatever was learned before it did.
    fn decline(reason: ChartDecline, unconsumed: BTreeSet<String>) -> Self {
        Self {
            projection: None,
            declined: Some(reason),
            unconsumed,
        }
    }
}

/// Reads one chart part into a typed projection.
///
/// `object` is the `EmbeddedObject` node this projection describes (the
/// `docs/45` I3 anchor). `external_data` resolves `c:externalData@r:id` against
/// the **chart part's own** relationships — a chart's workbook hangs off
/// `word/charts/_rels/chartN.xml.rels`, not the main document's — and is handed
/// in already resolved so this module never reaches for a package.
///
/// Never returns an error: see the module header.
pub(crate) fn read_chart_part(
    xml: &[u8],
    object: NodeId,
    chart_rels: &std::collections::BTreeMap<String, EmbeddedRel>,
    config: ImportConfig,
) -> ChartRead {
    let mut parser = Parser::new(object);
    match parser.run(xml, chart_rels, config) {
        Ok(()) => parser.finish(),
        Err(reason) => ChartRead::decline(reason, std::mem::take(&mut parser.unconsumed)),
    }
}

/// One chart part's bytes plus the relationships that part owns.
///
/// A chart's workbook, colour style and chart style hang off
/// `word/charts/_rels/chartN.xml.rels` — **not** the main document's — so the
/// driver resolves them per part and hands them in. Nothing here opens a package.
pub(crate) struct ChartPartSource {
    /// The `chartN.xml` bytes.
    pub(crate) bytes: Vec<u8>,
    /// The chart part's own relationships, by `r:id`.
    pub(crate) rels: BTreeMap<String, EmbeddedRel>,
}

/// What reading every chart part in a document produced.
pub(crate) struct ChartProjections {
    /// The projections, keyed by minted [`ChartId`].
    pub(crate) charts: DefinitionMap<ChartId, Chart>,
    /// One entry per chart part read, in document order, for the report.
    pub(crate) parts: Vec<ChartPartOutcome>,
}

/// What became of one chart part, for the compatibility report.
///
/// `Clone`/`Eq` because it travels on `Import`, which is comparable.
///
/// This is deliberately not a `Disposition`: the per-construct *retention*
/// outcome for a chart construct is "the opaque side-table holds the part's
/// bytes", and the ledger record licensing that claim is created by
/// `build_retained_parts`, after this runs. So the reader states what it
/// understood and the driver resolves the disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChartPartOutcome {
    /// The part this outcome is charged to.
    pub(crate) part_name: String,
    /// Whether a projection was built.
    pub(crate) projected: bool,
    /// Local names of the constructs inside the part the projection did not
    /// represent, deduplicated and ordered.
    pub(crate) unconsumed: BTreeSet<String>,
    /// The out-of-scope chart family that made the reader decline, when that is
    /// why it declined.
    ///
    /// Reported in its own right because the whole-part row says only that
    /// `word/charts/chart1.xml` was not modeled; naming `bar3DChart` is the
    /// difference between that and an answer to "what about my chart did you not
    /// understand?". The other decline reasons (malformed, over-bound, nothing to
    /// project) add nothing the part row does not already carry, so they are not
    /// propagated — `35`'s rule that a finding must describe a loss the reader can
    /// act on.
    pub(crate) out_of_scope_family: Option<String>,
}

/// Reads every chart part a chart node in `containers` references, in the order
/// the containers are given and document order within each.
///
/// # Every surface, not the body only
///
/// This walked the body alone while only the body parser could produce an
/// `EmbeddedObject`: the note/header/footer/comment parsers were handed an empty
/// embedded-relationship index, so a chart in a header never became a node and
/// there was nothing to project (`109` HF-266). Those parsers resolve their own
/// part's relationships now, so a chart node appears on all five running
/// surfaces, and the caller passes every container — body first, so a document
/// whose charts are all in its body allocates exactly the ids it always did.
///
/// # Complexity
///
/// O(document nodes) for the walk plus O(bytes) per chart part, once, at import.
pub(crate) fn build_charts<'b>(
    containers: impl IntoIterator<Item = &'b [BlockNode]>,
    chart_parts: &BTreeMap<String, ChartPartSource>,
    ids: &mut IdGenerator,
    config: ImportConfig,
) -> Result<ChartProjections, ImportError> {
    let mut anchors = Vec::new();
    for container in containers {
        for block in container {
            collect_chart_anchors(block, &mut anchors);
        }
    }
    let mut charts = DefinitionMap::default();
    let mut parts = Vec::new();
    for (object, part_name) in anchors {
        let Some(source) = chart_parts.get(&part_name) else {
            // The part was not admitted, or the relationship did not resolve to
            // one. The node still references it and the reference still
            // round-trips; there is simply nothing to read.
            continue;
        };
        let read = read_chart_part(&source.bytes, object, &source.rels, config);
        let out_of_scope_family = match &read.declined {
            Some(ChartDecline::OutOfScopeFamily(family)) => Some(family.clone()),
            Some(
                ChartDecline::Malformed
                | ChartDecline::OverBound(_)
                | ChartDecline::NothingToProject,
            )
            | None => None,
        };
        let projected = match read.projection {
            Some(chart) => {
                let id = ChartId::new(
                    ids.next_id()
                        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?,
                );
                charts.insert(id, chart);
                true
            }
            None => false,
        };
        parts.push(ChartPartOutcome {
            part_name,
            projected,
            unconsumed: read.unconsumed,
            out_of_scope_family,
        });
    }
    Ok(ChartProjections { charts, parts })
}

/// Collects `(node id, chart part name)` for every embedded chart object in a
/// block, in document order.
fn collect_chart_anchors(block: &BlockNode, found: &mut Vec<(NodeId, String)>) {
    match block {
        BlockNode::Paragraph(paragraph) => {
            for inline in &paragraph.inlines {
                collect_chart_anchors_in_inline(inline, found);
            }
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for nested in &cell.blocks {
                        collect_chart_anchors(nested, found);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for nested in &sdt.blocks {
                collect_chart_anchors(nested, found);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

/// The inline half of [`collect_chart_anchors`].
fn collect_chart_anchors_in_inline(inline: &InlineNode, found: &mut Vec<(NodeId, String)>) {
    match inline {
        InlineNode::EmbeddedObject(object) => {
            if object.kind == EmbeddedKind::Chart {
                found.push((object.id, object.part.part_name.clone()));
            }
        }
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                collect_chart_anchors_in_inline(child, found);
            }
        }
        InlineNode::Field(field) => {
            for child in &field.inlines {
                collect_chart_anchors_in_inline(child, found);
            }
        }
        InlineNode::Revision(revision) => {
            for child in &revision.inlines {
                collect_chart_anchors_in_inline(child, found);
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                collect_chart_anchors_in_inline(child, found);
            }
        }
        InlineNode::TextBox(text_box) => {
            for block in &text_box.blocks {
                collect_chart_anchors(block, found);
            }
        }
        _ => {}
    }
}

/// Which known scope the parser is inside. The stack of these is what lets one
/// element name mean different things in different places (`c:marker` is a
/// group-level boolean and a series-level symbol; `c:spPr` is a series fill and a
/// chart-space background) without a pile of independent booleans.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    /// `c:chartSpace`.
    ChartSpace,
    /// `c:chart`.
    Chart,
    /// `c:title`.
    Title,
    /// `c:tx` inside a title or a series.
    Text,
    /// `c:strRef`/`c:numRef`/`c:rich` text source inside a `c:tx`.
    TextSource,
    /// `c:plotArea`.
    PlotArea,
    /// A chart group (`c:barChart`, `c:lineChart`, …).
    Group,
    /// `c:ser`.
    Series,
    /// `c:cat`, `c:val`, `c:xVal` or `c:yVal`.
    DataRef(DataSlot),
    /// `c:numRef`/`c:strRef`/`c:numLit`/`c:strLit` inside a data reference.
    DataSource,
    /// `c:numCache`/`c:strCache`.
    Cache,
    /// `c:pt` inside a cache.
    Point,
    /// A `c:spPr` whose solid fill and line the projection models.
    ShapeProperties,
    /// `a:ln` inside a modeled `c:spPr`.
    Line,
    /// `a:solidFill` inside a modeled `c:spPr` or `a:ln`.
    SolidFill,
    /// An axis (`c:catAx`, `c:valAx`, `c:dateAx`).
    Axis,
    /// `c:scaling` inside an axis.
    Scaling,
    /// `c:legend`.
    Legend,
    /// `c:dLbls`.
    Labels,
    /// `c:externalData`.
    ExternalData,
    /// A container whose children are dispositioned individually but which
    /// carries nothing itself (`c:majorGridlines` with a `c:spPr` inside it).
    Transparent,
}

/// Which of a series' four data slots a `c:cat`/`c:val`/`c:xVal`/`c:yVal` fills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DataSlot {
    Categories,
    Values,
    XValues,
}

/// Where a `c:solidFill` colour lands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FillTarget {
    SeriesFill,
    SeriesLine,
}

/// The chart groups outside tier 1 (`docs/155` §4.3). A plot area holding one of
/// these declines projection as a whole: a partial projection of a 3-D or stock
/// chart would be drawn wrongly rather than not drawn, which is worse than
/// today's placeholder.
///
/// Matched by **suffix** for the 3-D families so `bar3DChart`, `line3DChart`,
/// `area3DChart`, `pie3DChart` and `surface3DChart` are covered by the rule
/// rather than by a list that the next 3-D family would escape.
fn out_of_scope_family(local: &[u8]) -> bool {
    matches!(
        local,
        b"surfaceChart" | b"stockChart" | b"radarChart" | b"bubbleChart" | b"ofPieChart"
    ) || local.ends_with(b"3DChart")
}

/// Whether a chart-vocabulary element carries no document meaning, so recording
/// it as unconsumed would describe a loss that did not happen (HF-174, and
/// `crate::noop`'s header for the general rule).
///
/// Conditional members have conditional arms, exactly as `crate::noop` requires:
/// `<c:layout/>` is "lay the plot area out automatically" and a *populated*
/// `c:layout` is a manual layout this projection does not carry.
fn chart_noop(local: &[u8], element: &BytesStart<'_>, self_closing: bool) -> bool {
    match local {
        // A pure extension container. Its contents are producer-private and
        // carry no meaning this model could claim to have lost.
        b"extLst" => true,
        // Automatic layout when empty; a manual one when not.
        b"layout" => self_closing,
        // `val="0"` is the schema default and the state the model already has, so
        // its presence says nothing. A `val="1"` does.
        b"roundedCorners" | b"date1904" | b"autoUpdate" | b"invertIfNegative" | b"bubble3D"
        | b"noMultiLvlLbl" | b"showDLblsOverMax" => {
            !is_true(attribute_value(element, b"val").as_deref())
        }
        _ => false,
    }
}

/// The accumulating chart projection.
struct Parser {
    object: NodeId,
    scopes: Vec<Scope>,
    unconsumed: BTreeSet<String>,
    /// Nesting level inside a subtree already recorded on its outermost element.
    skip_depth: u32,

    chart: ChartDraft,
    /// The title being built, when inside one.
    title: Option<ChartTitle>,
    /// Accumulated text of the `c:rich`/`c:strCache` inside a `c:tx`.
    text: String,
    /// The `c:f` of the `c:strRef` inside a `c:tx`.
    text_formula: Option<String>,
    /// The group being built, when inside one.
    group: Option<GroupDraft>,
    /// The series being built, when inside one.
    series: Option<Series>,
    /// The data range being built, and which slot it fills.
    range: Option<(DataSlot, DataRange)>,
    /// The `idx` of the open `c:pt`.
    point_index: Option<u32>,
    /// Whether the open cache is a string cache (so a value is `Text`).
    string_cache: bool,
    /// Text accumulated inside the open `c:v`/`a:t`.
    value_text: Option<String>,
    /// Where a solid fill inside the open `c:spPr` lands.
    fill_target: Option<FillTarget>,
    /// The line being built, when inside an `a:ln`.
    line: Option<ChartLine>,
    /// The axis being built, when inside one.
    axis: Option<Axis>,
    /// The legend being built, when inside one.
    legend: Option<Legend>,
    /// The data labels being built, when inside a `c:dLbls`.
    data_labels: Option<DataLabels>,
}

/// The chart-space-level fields, accumulated before the `Chart` is assembled.
#[derive(Default)]
struct ChartDraft {
    title: Option<ChartTitle>,
    auto_title_deleted: bool,
    groups: Vec<ChartGroup>,
    axes: Vec<Axis>,
    legend: Option<Legend>,
    plot_visible_only: bool,
    display_blanks_as: DisplayBlanks,
    vary_colors: bool,
    external_data: Option<EmbeddedPart>,
    /// Whether a `c:plotArea` was seen at all.
    saw_plot_area: bool,
}

/// A chart group under construction: the family's own settings are collected as
/// they arrive and resolved into a [`ChartGroupKind`] when the group closes,
/// because `c:barDir` and `c:gapWidth` are siblings rather than attributes.
struct GroupDraft {
    local: Vec<u8>,
    series: Vec<Series>,
    axis_ids: Vec<u32>,
    vary_colors: bool,
    bar_direction: BarDirection,
    bar_grouping: BarGrouping,
    grouping: Grouping,
    gap_width: u16,
    overlap: i16,
    marker: bool,
    first_slice_angle: u16,
    hole_size: u8,
    scatter_style: ScatterStyle,
}

impl GroupDraft {
    fn new(local: &[u8]) -> Self {
        Self {
            local: local.to_vec(),
            series: Vec::new(),
            axis_ids: Vec::new(),
            vary_colors: false,
            bar_direction: BarDirection::Column,
            bar_grouping: BarGrouping::Clustered,
            grouping: Grouping::Standard,
            // ECMA-376 defaults: `c:gapWidth` 150, `c:overlap` 0, `c:holeSize` 10.
            gap_width: 150,
            overlap: 0,
            marker: false,
            first_slice_angle: 0,
            hole_size: 10,
            scatter_style: ScatterStyle::LineMarker,
        }
    }

    /// Resolves the draft into a typed group, or `None` for a family this
    /// projection does not represent.
    fn resolve(self) -> Option<ChartGroup> {
        let kind = match self.local.as_slice() {
            b"barChart" => ChartGroupKind::Bar {
                direction: self.bar_direction,
                grouping: self.bar_grouping,
                gap_width: self.gap_width,
                overlap: self.overlap,
            },
            b"lineChart" => ChartGroupKind::Line {
                grouping: self.grouping,
                marker: self.marker,
            },
            b"areaChart" => ChartGroupKind::Area {
                grouping: self.grouping,
            },
            b"pieChart" => ChartGroupKind::Pie {
                first_slice_angle: self.first_slice_angle,
            },
            b"doughnutChart" => ChartGroupKind::Doughnut {
                first_slice_angle: self.first_slice_angle,
                hole_size: self.hole_size,
            },
            b"scatterChart" => ChartGroupKind::Scatter {
                style: self.scatter_style,
            },
            _ => return None,
        };
        Some(ChartGroup {
            kind,
            series: self.series,
            axis_ids: self.axis_ids,
            vary_colors: self.vary_colors,
        })
    }
}

impl Parser {
    fn new(object: NodeId) -> Self {
        Self {
            object,
            scopes: Vec::new(),
            unconsumed: BTreeSet::new(),
            skip_depth: 0,
            chart: ChartDraft::default(),
            title: None,
            text: String::new(),
            text_formula: None,
            group: None,
            series: None,
            range: None,
            point_index: None,
            string_cache: false,
            value_text: None,
            fill_target: None,
            line: None,
            axis: None,
            legend: None,
            data_labels: None,
        }
    }

    /// Streams the part. Every failure is a [`ChartDecline`], never an
    /// `ImportError`: a chart that cannot be projected is not a document that
    /// cannot be opened.
    fn run(
        &mut self,
        xml: &[u8],
        chart_rels: &std::collections::BTreeMap<String, EmbeddedRel>,
        config: ImportConfig,
    ) -> Result<(), ChartDecline> {
        let mut reader = Reader::from_reader(xml);
        let mut buffer = Vec::new();
        let mut elements = 0_u64;
        let mut depth = 0_u64;
        let mut saw_root = false;

        loop {
            let event = reader
                .read_event_into(&mut buffer)
                .map_err(|_| ChartDecline::Malformed)?;
            match &event {
                Event::Eof => break,
                Event::DocType(_) => return Err(ChartDecline::Malformed),
                Event::Start(element) => {
                    depth += 1;
                    if depth > config.max_depth {
                        return Err(ChartDecline::Malformed);
                    }
                    elements += 1;
                    if elements > config.max_elements {
                        return Err(ChartDecline::Malformed);
                    }
                    let local = element.local_name();
                    let local = local.as_ref();
                    if !saw_root {
                        if local != b"chartSpace" {
                            return Err(ChartDecline::Malformed);
                        }
                        saw_root = true;
                        self.scopes.push(Scope::ChartSpace);
                        continue;
                    }
                    if self.skip_depth > 0 {
                        self.skip_depth += 1;
                        continue;
                    }
                    match self.on_start(local, element, false, chart_rels)? {
                        Step::Push(scope) => self.scopes.push(scope),
                        Step::Leaf => self.scopes.push(Scope::Transparent),
                        Step::Skip => {
                            self.record_unconsumed(local);
                            self.skip_depth = 1;
                        }
                        Step::SilentSkip => self.skip_depth = 1,
                    }
                }
                Event::Empty(element) => {
                    elements += 1;
                    if elements > config.max_elements {
                        return Err(ChartDecline::Malformed);
                    }
                    let local = element.local_name();
                    let local = local.as_ref();
                    if !saw_root {
                        // A `<c:chartSpace/>` carries no chart at all.
                        return Err(if local == b"chartSpace" {
                            ChartDecline::NothingToProject
                        } else {
                            ChartDecline::Malformed
                        });
                    }
                    if self.skip_depth > 0 {
                        continue;
                    }
                    // An empty element opens no subtree, so no `Step` has anything
                    // to enter or skip — only `Skip`'s record stands.
                    if let Step::Skip = self.on_start(local, element, true, chart_rels)? {
                        self.record_unconsumed(local);
                    }
                }
                Event::Text(text) => {
                    if self.skip_depth == 0 && self.value_text.is_some() {
                        let raw = text.decode().map_err(|_| ChartDecline::Malformed)?;
                        let decoded = quick_xml::escape::unescape(&raw)
                            .map_err(|_| ChartDecline::Malformed)?;
                        self.push_value_text(&decoded, config)?;
                    }
                }
                // `quick-xml` reports `&amp;` and `&#x2014;` separately from the
                // surrounding text, so a cached label carrying one would otherwise
                // lose it. Same policy as the body parser's `decode_xml_reference`.
                Event::GeneralRef(reference) => {
                    if self.skip_depth == 0 && self.value_text.is_some() {
                        let decoded = crate::decode_xml_reference(reference)
                            .map_err(|_| ChartDecline::Malformed)?;
                        self.push_value_text(&decoded, config)?;
                    }
                }
                Event::End(element) => {
                    depth = depth.saturating_sub(1);
                    if self.skip_depth > 0 {
                        self.skip_depth -= 1;
                        continue;
                    }
                    let local = element.local_name();
                    let local = local.as_ref();
                    self.on_end(local)?;
                    self.scopes.pop();
                }
                _ => {}
            }
            buffer.clear();
        }
        Ok(())
    }

    /// What the traversal does with an element.
    fn on_start(
        &mut self,
        local: &[u8],
        element: &BytesStart<'_>,
        self_closing: bool,
        chart_rels: &std::collections::BTreeMap<String, EmbeddedRel>,
    ) -> Result<Step, ChartDecline> {
        if chart_noop(local, element, self_closing) {
            // `SilentSkip`, not `Leaf`: a no-op takes its subtree with it. `Leaf`
            // descends, so the children of a `c:extLst` would each be
            // dispositioned on their own and reported — findings describing losses
            // that did not happen, which is the HF-174 defect in the one place the
            // code is trying to avoid it.
            return Ok(Step::SilentSkip);
        }
        let scope = self.scopes.last().copied().unwrap_or(Scope::ChartSpace);
        match (scope, local) {
            // ---- chart space ----
            (Scope::ChartSpace, b"chart") => Ok(Step::Push(Scope::Chart)),
            (Scope::ChartSpace, b"externalData") => {
                // The workbook pointer, resolved against the CHART part's own
                // relationships. A part name and a relationship id — never bytes,
                // and nothing here opens it (`docs/155` §5.2).
                if let Some(rid) = attribute_value(element, b"id")
                    && let Some(rel) = chart_rels.get(&rid)
                {
                    self.chart.external_data = Some(EmbeddedPart {
                        relationship_id: rid,
                        relationship_type: rel.relationship_type.clone(),
                        part_name: rel.part_name.clone(),
                    });
                }
                Ok(Step::Push(Scope::ExternalData))
            }
            // ---- chart ----
            (Scope::Chart, b"title") => {
                self.title = Some(ChartTitle::default());
                Ok(Step::Push(Scope::Title))
            }
            (Scope::Chart, b"autoTitleDeleted") => {
                self.chart.auto_title_deleted =
                    is_true(attribute_value(element, b"val").as_deref());
                Ok(Step::Leaf)
            }
            (Scope::Chart, b"plotArea") => {
                self.chart.saw_plot_area = true;
                Ok(Step::Push(Scope::PlotArea))
            }
            (Scope::Chart, b"legend") => {
                self.legend = Some(Legend::default());
                Ok(Step::Push(Scope::Legend))
            }
            (Scope::Chart, b"plotVisOnly") => {
                self.chart.plot_visible_only = is_true(attribute_value(element, b"val").as_deref());
                Ok(Step::Leaf)
            }
            (Scope::Chart, b"dispBlanksAs") => {
                self.chart.display_blanks_as =
                    match attribute_value(element, b"val").as_deref().unwrap_or("gap") {
                        "zero" => DisplayBlanks::Zero,
                        "span" => DisplayBlanks::Span,
                        _ => DisplayBlanks::Gap,
                    };
                Ok(Step::Leaf)
            }
            (Scope::Chart, b"varyColors") => {
                self.chart.vary_colors = is_true(attribute_value(element, b"val").as_deref());
                Ok(Step::Leaf)
            }
            // ---- title ----
            (Scope::Title, b"tx") => {
                self.text.clear();
                self.text_formula = None;
                Ok(Step::Push(Scope::Text))
            }
            (Scope::Title, b"overlay") => {
                if let Some(title) = self.title.as_mut() {
                    title.overlay = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            // ---- cached text (`c:tx`): a `c:strRef` cache, a `c:rich` body, or
            // (for a SERIES name only) a bare literal `c:v` ----
            (Scope::Text, b"strRef" | b"rich") => Ok(Step::Push(Scope::TextSource)),
            // `CT_SerTx` is `(strRef | v)`, so a series whose name is literal text
            // rather than a workbook reference spells it as one `c:v` directly
            // under `c:tx`. This arm was missing, and the cost was two faults at
            // once: the name was dropped, AND the unconsumed `v` made the whole
            // projection `ChartCoverage::Partial` — which is the flag that forbids
            // regenerating the part, so one unread element disabled the writer for
            // an otherwise fully-understood chart. `commit_text` already routes a
            // `c:v` outside a data range to the cached-text accumulator, which is
            // where a `c:strCache`'s one-cell value lands too, so the two
            // spellings converge on one field exactly as `ChartText` intends.
            (Scope::Text, b"v") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::TextSource, b"f") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::TextSource, b"strCache") => Ok(Step::Push(Scope::TextSource)),
            (Scope::TextSource, b"ptCount") => Ok(Step::Leaf),
            (Scope::TextSource, b"pt") => Ok(Step::Push(Scope::TextSource)),
            (Scope::TextSource, b"v" | b"t") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            // A `c:rich` body's paragraph scaffolding: the text inside it is
            // collected, the formatting is out of scope by `docs/155` §4.2
            // ("cached rich text only") and so is not a loss to name.
            (
                Scope::TextSource,
                b"bodyPr" | b"lstStyle" | b"p" | b"pPr" | b"r" | b"rPr" | b"endParaRPr" | b"defRPr"
                | b"fld",
            ) => Ok(Step::Push(Scope::TextSource)),
            // ---- plot area ----
            (Scope::PlotArea, _) if out_of_scope_family(local) => Err(
                ChartDecline::OutOfScopeFamily(String::from_utf8_lossy(local).into_owned()),
            ),
            (
                Scope::PlotArea,
                b"barChart" | b"lineChart" | b"areaChart" | b"pieChart" | b"doughnutChart"
                | b"scatterChart",
            ) => {
                if self.chart.groups.len() >= MAX_CHART_GROUPS {
                    return Err(ChartDecline::OverBound("chart.plotArea.groups"));
                }
                self.group = Some(GroupDraft::new(local));
                Ok(Step::Push(Scope::Group))
            }
            (Scope::PlotArea, b"catAx" | b"valAx" | b"dateAx") => {
                if self.chart.axes.len() >= MAX_CHART_AXES {
                    return Err(ChartDecline::OverBound("chart.plotArea.axes"));
                }
                self.axis = Some(Axis {
                    kind: match local {
                        b"valAx" => AxisKind::Value,
                        b"dateAx" => AxisKind::Date,
                        _ => AxisKind::Category,
                    },
                    ..Axis::default()
                });
                Ok(Step::Push(Scope::Axis))
            }
            // ---- group ----
            (Scope::Group, b"ser") => {
                let Some(group) = self.group.as_ref() else {
                    return Ok(Step::Skip);
                };
                if group.series.len() >= MAX_CHART_SERIES_PER_GROUP {
                    return Err(ChartDecline::OverBound("chart.group.series"));
                }
                self.series = Some(Series::default());
                Ok(Step::Push(Scope::Series))
            }
            (Scope::Group, b"barDir") => {
                if let Some(group) = self.group.as_mut()
                    && attribute_value(element, b"val").as_deref() == Some("bar")
                {
                    group.bar_direction = BarDirection::Bar;
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"grouping") => {
                if let Some(group) = self.group.as_mut() {
                    let value = attribute_value(element, b"val").unwrap_or_default();
                    group.bar_grouping = match value.as_str() {
                        "stacked" => BarGrouping::Stacked,
                        "percentStacked" => BarGrouping::PercentStacked,
                        "standard" => BarGrouping::Standard,
                        _ => BarGrouping::Clustered,
                    };
                    group.grouping = match value.as_str() {
                        "stacked" => Grouping::Stacked,
                        "percentStacked" => Grouping::PercentStacked,
                        _ => Grouping::Standard,
                    };
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"varyColors") => {
                if let Some(group) = self.group.as_mut() {
                    group.vary_colors = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"gapWidth") => {
                if let Some(group) = self.group.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    group.gap_width = value.min(500) as u16;
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"overlap") => {
                if let Some(group) = self.group.as_mut()
                    && let Some(value) = signed(element)
                {
                    group.overlap = value.clamp(-100, 100) as i16;
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"firstSliceAng") => {
                if let Some(group) = self.group.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    group.first_slice_angle = value.min(360) as u16;
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"holeSize") => {
                if let Some(group) = self.group.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    group.hole_size = value.clamp(1, 90) as u8;
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"scatterStyle") => {
                if let Some(group) = self.group.as_mut() {
                    group.scatter_style =
                        match attribute_value(element, b"val").as_deref().unwrap_or("") {
                            "none" => ScatterStyle::None,
                            "line" => ScatterStyle::Line,
                            "marker" => ScatterStyle::Marker,
                            "smooth" => ScatterStyle::Smooth,
                            "smoothMarker" => ScatterStyle::SmoothMarker,
                            _ => ScatterStyle::LineMarker,
                        };
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"marker") => {
                if let Some(group) = self.group.as_mut() {
                    group.marker = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            (Scope::Group, b"axId") => {
                if let Some(group) = self.group.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    if group.axis_ids.len() >= MAX_CHART_AXES {
                        return Err(ChartDecline::OverBound("chart.group.axisIds"));
                    }
                    group.axis_ids.push(value);
                }
                Ok(Step::Leaf)
            }
            // ---- series ----
            (Scope::Series, b"idx") => {
                if let Some(series) = self.series.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    series.index = value;
                }
                Ok(Step::Leaf)
            }
            (Scope::Series, b"order") => {
                if let Some(series) = self.series.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    series.order = value;
                }
                Ok(Step::Leaf)
            }
            (Scope::Series, b"tx") => {
                self.text.clear();
                self.text_formula = None;
                Ok(Step::Push(Scope::Text))
            }
            (Scope::Series, b"cat") => {
                self.range = Some((DataSlot::Categories, DataRange::default()));
                Ok(Step::Push(Scope::DataRef(DataSlot::Categories)))
            }
            (Scope::Series, b"val" | b"yVal") => {
                self.range = Some((DataSlot::Values, DataRange::default()));
                Ok(Step::Push(Scope::DataRef(DataSlot::Values)))
            }
            (Scope::Series, b"xVal") => {
                self.range = Some((DataSlot::XValues, DataRange::default()));
                Ok(Step::Push(Scope::DataRef(DataSlot::XValues)))
            }
            (Scope::Series, b"smooth") => {
                if let Some(series) = self.series.as_mut() {
                    series.smooth = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            (Scope::Series, b"spPr") => {
                self.fill_target = Some(FillTarget::SeriesFill);
                Ok(Step::Push(Scope::ShapeProperties))
            }
            (Scope::Series, b"dLbls") => {
                self.data_labels = Some(DataLabels::default());
                Ok(Step::Push(Scope::Labels))
            }
            // ---- a series' shape properties: solid fill and line only ----
            (Scope::ShapeProperties, b"solidFill") => Ok(Step::Push(Scope::SolidFill)),
            (Scope::ShapeProperties, b"ln") => {
                let mut line = ChartLine::default();
                if let Some(width) = attr_u32(element, b"w") {
                    line.width_emu = Some(width);
                }
                self.line = Some(line);
                self.fill_target = Some(FillTarget::SeriesLine);
                Ok(Step::Push(Scope::Line))
            }
            (Scope::Line, b"solidFill") => Ok(Step::Push(Scope::SolidFill)),
            (Scope::Line, b"noFill") => {
                if let Some(line) = self.line.as_mut() {
                    line.no_fill = true;
                }
                Ok(Step::Leaf)
            }
            (Scope::SolidFill, b"srgbClr" | b"schemeClr") => {
                match chart_color(local, element) {
                    Some(color) => {
                        match self.fill_target {
                            Some(FillTarget::SeriesLine) => {
                                if let Some(line) = self.line.as_mut() {
                                    line.color = Some(color);
                                }
                            }
                            Some(FillTarget::SeriesFill) => {
                                if let Some(series) = self.series.as_mut() {
                                    series.fill = Some(color);
                                }
                            }
                            None => {}
                        }
                        // A colour transform inside the colour (`a:lumMod`,
                        // `a:alpha`, …) is a modifier this model does not carry,
                        // so descend and let each be recorded on its own.
                        Ok(Step::Push(Scope::Transparent))
                    }
                    // `a:sysClr`, `a:prstClr`, `a:hslClr`, `a:scrgbClr`, or a slot
                    // with no home: the colour is genuinely not carried.
                    None => Ok(Step::Skip),
                }
            }
            // ---- data references ----
            (Scope::DataRef(_), b"numRef" | b"strRef" | b"numLit" | b"strLit") => {
                self.string_cache = matches!(local, b"strRef" | b"strLit");
                Ok(Step::Push(Scope::DataSource))
            }
            (Scope::DataRef(_), b"multiLvlStrRef") => {
                // A multi-level category axis is a nested cache this projection
                // does not represent; naming it is the honest outcome.
                Ok(Step::Skip)
            }
            (Scope::DataSource, b"f") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::DataSource, b"numCache" | b"strCache") => {
                self.string_cache = local == b"strCache";
                Ok(Step::Push(Scope::Cache))
            }
            // A `c:numLit`/`c:strLit` carries its points directly, with no cache
            // wrapper; the points land in the same place.
            (Scope::DataSource, b"ptCount") => {
                if let Some((_, range)) = self.range.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    range.point_count = value;
                }
                Ok(Step::Leaf)
            }
            (Scope::DataSource, b"formatCode") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::DataSource, b"pt") => self.begin_point(element),
            (Scope::Cache, b"ptCount") => {
                if let Some((_, range)) = self.range.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    range.point_count = value;
                }
                Ok(Step::Leaf)
            }
            (Scope::Cache, b"formatCode") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::Cache, b"pt") => self.begin_point(element),
            (Scope::Point, b"v") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            // ---- axis ----
            (Scope::Axis, b"axId") => {
                if let Some(axis) = self.axis.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    axis.id = value;
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"scaling") => Ok(Step::Push(Scope::Scaling)),
            (Scope::Scaling, b"orientation") => {
                if let Some(axis) = self.axis.as_mut()
                    && attribute_value(element, b"val").as_deref() == Some("maxMin")
                {
                    axis.orientation = AxisOrientation::MaxMin;
                }
                Ok(Step::Leaf)
            }
            (Scope::Scaling, b"min" | b"max") => {
                // Verbatim lexical form, never an `f64` — the no-float rule
                // (`docs/155` §8.2) applies to an axis bound exactly as it does to
                // a cached value.
                if let Some(axis) = self.axis.as_mut()
                    && let Some(raw) = attribute_value(element, b"val")
                    && !raw.is_empty()
                    && raw.len() <= MAX_CHART_NUMBER_BYTES
                {
                    if local == b"min" {
                        axis.minimum = Some(raw);
                    } else {
                        axis.maximum = Some(raw);
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"delete") => {
                if let Some(axis) = self.axis.as_mut() {
                    axis.deleted = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"axPos") => {
                if let Some(axis) = self.axis.as_mut() {
                    axis.position = match attribute_value(element, b"val").as_deref() {
                        Some("b") => Some(AxisPosition::Bottom),
                        Some("l") => Some(AxisPosition::Left),
                        Some("r") => Some(AxisPosition::Right),
                        Some("t") => Some(AxisPosition::Top),
                        _ => None,
                    };
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"majorGridlines" | b"minorGridlines") => {
                if let Some(axis) = self.axis.as_mut() {
                    if local == b"majorGridlines" {
                        axis.major_gridlines = true;
                    } else {
                        axis.minor_gridlines = true;
                    }
                }
                // The gridlines' own `c:spPr` is formatting this model does not
                // carry, so descend rather than silently swallow the subtree.
                Ok(Step::Push(Scope::Transparent))
            }
            (Scope::Axis, b"majorTickMark" | b"minorTickMark") => {
                if let Some(axis) = self.axis.as_mut() {
                    let mark = match attribute_value(element, b"val").as_deref() {
                        Some("none") => TickMark::None,
                        Some("in") => TickMark::Inside,
                        Some("cross") => TickMark::Cross,
                        _ => TickMark::Outside,
                    };
                    if local == b"majorTickMark" {
                        axis.major_tick_mark = mark;
                    } else {
                        axis.minor_tick_mark = mark;
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"tickLblPos") => {
                if let Some(axis) = self.axis.as_mut() {
                    axis.tick_label_position = match attribute_value(element, b"val").as_deref() {
                        Some("high") => TickLabelPosition::High,
                        Some("low") => TickLabelPosition::Low,
                        Some("none") => TickLabelPosition::None,
                        _ => TickLabelPosition::NextTo,
                    };
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"numFmt") => {
                if let Some(axis) = self.axis.as_mut()
                    && let Some(code) = attribute_value(element, b"formatCode")
                    && !code.is_empty()
                    && code.len() <= MAX_CHART_TEXT_BYTES
                {
                    axis.number_format = Some(code);
                }
                Ok(Step::Leaf)
            }
            (Scope::Axis, b"crossAx") => {
                if let Some(axis) = self.axis.as_mut()
                    && let Some(value) = unsigned(element)
                {
                    axis.cross_axis_id = Some(value);
                }
                Ok(Step::Leaf)
            }
            // ---- legend ----
            (Scope::Legend, b"legendPos") => {
                if let Some(legend) = self.legend.as_mut() {
                    legend.position = match attribute_value(element, b"val").as_deref() {
                        Some("l") => LegendPosition::Left,
                        Some("r") => LegendPosition::Right,
                        Some("t") => LegendPosition::Top,
                        Some("tr") => LegendPosition::TopRight,
                        _ => LegendPosition::Bottom,
                    };
                }
                Ok(Step::Leaf)
            }
            (Scope::Legend, b"overlay") => {
                if let Some(legend) = self.legend.as_mut() {
                    legend.overlay = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            // ---- data labels ----
            (Scope::Labels, b"showVal" | b"showCatName" | b"showSerName" | b"showPercent") => {
                if let Some(labels) = self.data_labels.as_mut() {
                    let on = is_true(attribute_value(element, b"val").as_deref());
                    match local {
                        b"showVal" => labels.show_value = on,
                        b"showCatName" => labels.show_category_name = on,
                        b"showSerName" => labels.show_series_name = on,
                        _ => labels.show_percent = on,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Labels, b"dLblPos") => {
                if let Some(labels) = self.data_labels.as_mut() {
                    labels.position = match attribute_value(element, b"val").as_deref() {
                        Some("b") => Some(DataLabelPosition::Bottom),
                        Some("ctr") => Some(DataLabelPosition::Center),
                        Some("inBase") => Some(DataLabelPosition::InsideBase),
                        Some("inEnd") => Some(DataLabelPosition::InsideEnd),
                        Some("l") => Some(DataLabelPosition::Left),
                        Some("outEnd") => Some(DataLabelPosition::OutsideEnd),
                        Some("r") => Some(DataLabelPosition::Right),
                        Some("t") => Some(DataLabelPosition::Top),
                        Some("bestFit") => Some(DataLabelPosition::BestFit),
                        _ => None,
                    };
                }
                Ok(Step::Leaf)
            }
            // `c:showLegendKey`/`c:showBubbleSize`/`c:separator` and the label's
            // own `c:spPr`/`c:txPr`/`c:numFmt` are not carried; each is recorded.
            //
            // ---- everything else ----
            //
            // A construct the projection does not represent. Recorded by name and
            // its subtree skipped, so one dropped construct is one finding — the
            // same rule `theme::parse` follows. This is the arm that makes
            // `c:trendline`, `c:errBars`, `c:dTable`, `c:view3D`, a gradient fill
            // and every foreign element reportable without any of them being
            // listed anywhere.
            _ => Ok(Step::Skip),
        }
    }

    /// Appends decoded character data to the open text-carrying leaf.
    fn push_value_text(&mut self, decoded: &str, config: ImportConfig) -> Result<(), ChartDecline> {
        if let Some(open) = self.value_text.as_mut() {
            if open.len().saturating_add(decoded.len()) > config.max_text_bytes {
                return Err(ChartDecline::OverBound("chart.text"));
            }
            open.push_str(decoded);
        }
        Ok(())
    }

    /// Opens a `c:pt`, enforcing the per-range point ceiling.
    fn begin_point(&mut self, element: &BytesStart<'_>) -> Result<Step, ChartDecline> {
        if let Some((_, range)) = self.range.as_ref()
            && range.points.len() >= MAX_CHART_DATA_POINTS
        {
            return Err(ChartDecline::OverBound("chart.dataRange.points"));
        }
        self.point_index = attr_u32(element, b"idx");
        Ok(Step::Push(Scope::Point))
    }

    /// Closes an element: commits whatever the open scope was accumulating.
    fn on_end(&mut self, local: &[u8]) -> Result<(), ChartDecline> {
        // A text-carrying leaf closes first: its content is in `value_text`, and
        // which field it lands in depends on the element, not on the scope.
        if let Some(text) = self.value_text.take() {
            self.commit_text(local, text)?;
        }
        match local {
            b"title" => {
                if let Some(mut title) = self.title.take() {
                    if title.text.is_none() && !self.text.is_empty() {
                        title.text = Some(ChartText {
                            text: std::mem::take(&mut self.text),
                            formula: self.text_formula.take(),
                        });
                    }
                    self.chart.title = Some(title);
                }
            }
            b"tx" => {
                // A title's `c:tx` closes before `c:title` does, so the text is
                // parked on the open title; a series' lands on the series.
                let text = std::mem::take(&mut self.text);
                let formula = self.text_formula.take();
                if text.is_empty() && formula.is_none() {
                    return Ok(());
                }
                let cached = ChartText { text, formula };
                if let Some(title) = self.title.as_mut() {
                    title.text = Some(cached);
                } else if let Some(series) = self.series.as_mut() {
                    series.name = Some(cached);
                }
            }
            b"ser" => {
                if let Some(series) = self.series.take()
                    && let Some(group) = self.group.as_mut()
                {
                    group.series.push(series);
                }
            }
            b"cat" | b"val" | b"yVal" | b"xVal" => {
                if let Some((slot, range)) = self.range.take()
                    && let Some(series) = self.series.as_mut()
                {
                    match slot {
                        DataSlot::Categories => series.categories = Some(range),
                        DataSlot::Values => series.values = range,
                        DataSlot::XValues => series.x_values = Some(range),
                    }
                }
            }
            b"ln" => {
                if let Some(line) = self.line.take()
                    && line != ChartLine::default()
                    && let Some(series) = self.series.as_mut()
                {
                    series.line = Some(line);
                }
                self.fill_target = Some(FillTarget::SeriesFill);
            }
            b"spPr" => self.fill_target = None,
            b"dLbls" => {
                if let Some(labels) = self.data_labels.take()
                    && let Some(series) = self.series.as_mut()
                {
                    series.data_labels = Some(labels);
                }
            }
            b"catAx" | b"valAx" | b"dateAx" => {
                if let Some(axis) = self.axis.take() {
                    self.chart.axes.push(axis);
                }
            }
            b"legend" => {
                if let Some(legend) = self.legend.take() {
                    self.chart.legend = Some(legend);
                }
            }
            b"barChart" | b"lineChart" | b"areaChart" | b"pieChart" | b"doughnutChart"
            | b"scatterChart" => {
                if let Some(draft) = self.group.take()
                    && let Some(group) = draft.resolve()
                {
                    self.chart.groups.push(group);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Commits the text content of a closing leaf.
    fn commit_text(&mut self, local: &[u8], text: String) -> Result<(), ChartDecline> {
        match local {
            // A `c:f`: verbatim, opaque, never parsed. The one place the formula
            // string is stored, and it is stored unchanged.
            b"f" => {
                if text.len() > MAX_CHART_FORMULA_BYTES {
                    return Err(ChartDecline::OverBound("chart.dataRange.formula"));
                }
                if text.is_empty() {
                    return Ok(());
                }
                match self.range.as_mut() {
                    Some((_, range)) => range.formula = Some(text),
                    None => self.text_formula = Some(text),
                }
            }
            b"formatCode" => {
                if text.len() > MAX_CHART_TEXT_BYTES {
                    return Err(ChartDecline::OverBound("chart.dataRange.numberFormat"));
                }
                if let Some((_, range)) = self.range.as_mut()
                    && !text.is_empty()
                {
                    range.number_format = Some(text);
                }
            }
            b"v" => match self.range.as_mut() {
                Some((_, range)) => {
                    let index = self.point_index.unwrap_or(0);
                    let value = if self.string_cache {
                        if text.len() > MAX_CHART_TEXT_BYTES {
                            return Err(ChartDecline::OverBound("chart.dataRange.point.text"));
                        }
                        ChartValue::Text(text)
                    } else if text.trim().is_empty() {
                        ChartValue::Blank
                    } else {
                        if text.len() > MAX_CHART_NUMBER_BYTES {
                            return Err(ChartDecline::OverBound("chart.dataRange.point.number"));
                        }
                        // Verbatim: `4.30` stays `4.30`. Parsing here and
                        // re-rendering later is the bug this spelling exists to
                        // make impossible (`docs/155` §8.2).
                        ChartValue::Number(text)
                    };
                    range.points.push((index, value));
                }
                // A `c:v` inside a `c:tx`'s one-cell cache.
                None => self.append_cached_text(&text),
            },
            b"t" => self.append_cached_text(&text),
            _ => {}
        }
        Ok(())
    }

    /// Appends a run of cached title/series-name text.
    fn append_cached_text(&mut self, text: &str) {
        if self.text.len().saturating_add(text.len()) <= MAX_CHART_TEXT_BYTES {
            self.text.push_str(text);
        }
    }

    /// Records an unconsumed construct by local name, bounded.
    fn record_unconsumed(&mut self, local: &[u8]) {
        if self.unconsumed.len() >= MAX_UNCONSUMED_NAMES {
            return;
        }
        self.unconsumed
            .insert(String::from_utf8_lossy(local).into_owned());
    }

    /// Assembles the projection, or declines when there is nothing to project.
    fn finish(&mut self) -> ChartRead {
        let unconsumed = std::mem::take(&mut self.unconsumed);
        if !self.chart.saw_plot_area || self.chart.groups.is_empty() {
            return ChartRead::decline(ChartDecline::NothingToProject, unconsumed);
        }
        let coverage = if unconsumed.is_empty() {
            ChartCoverage::Complete
        } else {
            ChartCoverage::Partial
        };
        let draft = std::mem::take(&mut self.chart);
        ChartRead {
            projection: Some(Chart {
                object: self.object,
                coverage,
                title: draft.title,
                auto_title_deleted: draft.auto_title_deleted,
                plot_area: PlotArea {
                    groups: draft.groups,
                    axes: draft.axes,
                },
                legend: draft.legend,
                plot_visible_only: draft.plot_visible_only,
                display_blanks_as: draft.display_blanks_as,
                vary_colors: draft.vary_colors,
                external_data: draft.external_data,
                dirty: false,
            }),
            declined: None,
            unconsumed,
        }
    }
}

/// What [`Parser::on_start`] decided about an element.
enum Step {
    /// Enter a known scope.
    Push(Scope),
    /// A consumed leaf: no scope of its own, and its subtree (if any) is read in
    /// the enclosing scope.
    Leaf,
    /// Not represented: record the name and skip the subtree.
    Skip,
    /// Carries no document meaning: skip the subtree and record **nothing**.
    SilentSkip,
}

/// A DrawingML colour this projection can carry: an explicit `a:srgbClr`, or an
/// `a:schemeClr` naming one of the twelve theme slots the model holds.
///
/// Returns `None` for `a:sysClr`, `a:prstClr`, `a:hslClr`, `a:scrgbClr` and for
/// `phClr` (a style placeholder with no fixed slot), so the caller records a real
/// loss instead of inventing a colour.
fn chart_color(local: &[u8], element: &BytesStart<'_>) -> Option<Color> {
    let value = attribute_value(element, b"val")?;
    match local {
        b"srgbClr" => parse_rgb(&value).map(Color::Rgb),
        b"schemeClr" => scheme_slot(&value).map(|slot| {
            Color::Theme(ThemeColor {
                slot,
                theme_tint: None,
                theme_shade: None,
            })
        }),
        _ => None,
    }
}

/// Maps a DrawingML `a:schemeClr@val` token to the model's theme slot.
///
/// The `bg1`/`tx1`/`bg2`/`tx2` aliases resolve through the default colour map
/// (`bg1 = lt1`, `tx1 = dk1`), the same mapping `body::scheme_slot_index` uses so
/// a chart series and a shape fill cannot disagree about what `accent1` means.
fn scheme_slot(name: &str) -> Option<ThemeColorRef> {
    Some(match name {
        "dk1" | "tx1" => ThemeColorRef::Dark1,
        "lt1" | "bg1" => ThemeColorRef::Light1,
        "dk2" | "tx2" => ThemeColorRef::Dark2,
        "lt2" | "bg2" => ThemeColorRef::Light2,
        "accent1" => ThemeColorRef::Accent1,
        "accent2" => ThemeColorRef::Accent2,
        "accent3" => ThemeColorRef::Accent3,
        "accent4" => ThemeColorRef::Accent4,
        "accent5" => ThemeColorRef::Accent5,
        "accent6" => ThemeColorRef::Accent6,
        "hlink" => ThemeColorRef::Hyperlink,
        "folHlink" => ThemeColorRef::FollowedHyperlink,
        _ => return None,
    })
}

/// One named attribute read as an unsigned integer.
///
/// Three different attributes carry a number in a chart part — `@val` on almost
/// everything, `@idx` on a cache point, `@w` on a line — and reading them through
/// one "try each in turn" helper would make `<c:pt val="9" idx="0"/>` ambiguous.
/// Each call site names the attribute it means.
fn attr_u32(element: &BytesStart<'_>, name: &[u8]) -> Option<u32> {
    attribute_value(element, name).and_then(|value| value.trim().parse::<u32>().ok())
}

/// A `@val` read as an unsigned integer.
fn unsigned(element: &BytesStart<'_>) -> Option<u32> {
    attr_u32(element, b"val")
}

/// A `@val` read as a signed integer.
fn signed(element: &BytesStart<'_>) -> Option<i32> {
    attribute_value(element, b"val").and_then(|value| value.trim().parse::<i32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A node id to anchor a test projection to.
    fn anchor() -> NodeId {
        IdGenerator::new(1).next_id().expect("an id")
    }

    /// Wraps chart-group markup in a minimal `c:chartSpace`.
    fn chart_space(inner: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:plotArea><c:layout/>{inner}</c:plotArea></c:chart></c:chartSpace>"#
        )
        .into_bytes()
    }

    /// A bar group with one series whose values are cached, spelled exactly as
    /// `fixtures/generated/chart.docx` spells them.
    const CACHED_BAR: &str = r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$3</c:f><c:strCache><c:ptCount val="2"/><c:pt idx="0"><c:v>Q1</c:v></c:pt><c:pt idx="1"><c:v>Q2</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>4.30</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>"#;

    fn read(xml: &[u8]) -> ChartRead {
        read_chart_part(xml, anchor(), &BTreeMap::new(), ImportConfig::default())
    }

    fn projection(xml: &[u8]) -> Chart {
        read(xml).projection.expect("a projection")
    }

    /// The cached data table, not the formula, is what the projection carries —
    /// `docs/155` §5.2. A reader that read `c:f` and left the cache behind would
    /// have no numbers to plot at all, and the only way to get them would be to
    /// open the workbook, which is the line this programme does not cross.
    #[test]
    fn chart_projection_reads_cached_series() {
        let chart = projection(&chart_space(CACHED_BAR));
        let group = &chart.plot_area.groups[0];
        let series = &group.series[0];
        assert_eq!(series.values.point_count, 2);
        assert_eq!(
            series.values.points,
            vec![
                (0, ChartValue::Number("4.30".to_owned())),
                (1, ChartValue::Number("2.5".to_owned())),
            ],
            "the numeric cache is the data"
        );
        assert_eq!(
            series
                .categories
                .as_ref()
                .expect("cached categories")
                .points,
            vec![
                (0, ChartValue::Text("Q1".to_owned())),
                (1, ChartValue::Text("Q2".to_owned())),
            ],
            "a string cache yields text, not numbers"
        );
        assert_eq!(
            series.values.number_format.as_deref(),
            Some("General"),
            "c:formatCode belongs to the range it formats"
        );
    }

    /// A cached number keeps the producer's spelling, and `as_f64` is the only
    /// place it becomes a float.
    ///
    /// `4.30` and `4.3` are the same number and different documents (`docs/155`
    /// §8.2). This is the guard that would catch a projection that parsed a value
    /// into an `f64` and re-rendered it, which no numeric assertion can see.
    #[test]
    fn a_cached_number_keeps_its_source_spelling() {
        let chart = projection(&chart_space(CACHED_BAR));
        let (_, first) = &chart.plot_area.groups[0].series[0].values.points[0];
        assert_eq!(
            first,
            &ChartValue::Number("4.30".to_owned()),
            "the trailing zero is part of the document"
        );
        assert_eq!(
            first.as_f64(),
            Some(4.3),
            "a consumer still gets the number, through the one helper that parses"
        );
        let ChartValue::Number(lexical) = first else {
            panic!("expected a cached number");
        };
        assert_eq!(lexical, "4.30");
    }

    /// `c:f` survives verbatim and is never interpreted.
    ///
    /// The assertion is deliberately on an awkward formula: a sheet name with a
    /// space and quotes, and a range. Any reader that split on `!`, stripped
    /// quotes, normalised case or re-rendered the reference would change it.
    #[test]
    fn chart_formula_is_carried_but_never_parsed() {
        let formula = "'Sheet One'!$B$2:$B$3";
        let xml = chart_space(&CACHED_BAR.replace("Sheet1!$B$2:$B$3", formula));
        let chart = projection(&xml);
        assert_eq!(
            chart.plot_area.groups[0].series[0]
                .values
                .formula
                .as_deref(),
            Some(formula),
            "the formula is an opaque string, re-emitted exactly"
        );
    }

    /// The embedded workbook is a pointer: a relationship id, a relationship type
    /// and a part name. Never bytes, and nothing opens it (`docs/155` §5.2).
    #[test]
    fn the_embedded_workbook_is_a_typed_pointer_only() {
        let mut rels = BTreeMap::new();
        rels.insert(
            "rId3".to_owned(),
            EmbeddedRel {
                relationship_type:
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package"
                        .to_owned(),
                part_name: "word/embeddings/Microsoft_Excel_Worksheet1.xlsx".to_owned(),
            },
        );
        let xml = format!(
            r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:plotArea>{CACHED_BAR}</c:plotArea></c:chart><c:externalData r:id="rId3"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#
        );
        let read = read_chart_part(xml.as_bytes(), anchor(), &rels, ImportConfig::default());
        let chart = read.projection.expect("a projection");
        let part = chart.external_data.expect("the workbook pointer");
        assert_eq!(part.relationship_id, "rId3");
        assert_eq!(
            part.part_name,
            "word/embeddings/Microsoft_Excel_Worksheet1.xlsx"
        );
        // `EmbeddedPart` has three string fields and no byte field, so "never
        // bytes" is a property of the type rather than of this call. What this
        // asserts is that reading a chart needed no workbook at all: the
        // projection carries data while this test supplied no `.xlsx`.
        assert!(!chart.plot_area.groups[0].series[0].values.points.is_empty());
    }

    /// A combo chart and a secondary axis are the absence of a restriction, not
    /// features: two groups in one plot area, and a group naming its own axis ids
    /// (`docs/155` §4.2).
    #[test]
    fn a_combo_chart_with_a_secondary_axis_projects_as_two_groups() {
        let inner = format!(
            r#"{CACHED_BAR}<c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="1"/><c:order val="1"/><c:val><c:numRef><c:f>Sheet1!$C$2</c:f><c:numCache><c:ptCount val="1"/><c:pt idx="0"><c:v>4</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:marker val="1"/><c:axId val="1"/><c:axId val="3"/></c:lineChart><c:catAx><c:axId val="1"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:axPos val="l"/><c:majorGridlines/><c:crossAx val="1"/></c:valAx><c:valAx><c:axId val="3"/><c:axPos val="r"/><c:crossAx val="1"/></c:valAx>"#
        );
        let chart = projection(&chart_space(&inner));
        assert_eq!(
            chart.plot_area.groups.len(),
            2,
            "a combo chart is two groups"
        );
        assert!(matches!(
            chart.plot_area.groups[0].kind,
            ChartGroupKind::Bar { .. }
        ));
        assert!(matches!(
            chart.plot_area.groups[1].kind,
            ChartGroupKind::Line { marker: true, .. }
        ));
        assert_eq!(chart.plot_area.groups[1].axis_ids, vec![1, 3]);
        let axis_ids: Vec<u32> = chart.plot_area.axes.iter().map(|axis| axis.id).collect();
        assert_eq!(
            axis_ids,
            vec![1, 2, 3],
            "the secondary axis is a third entry"
        );
        assert_eq!(chart.plot_area.axes[0].kind, AxisKind::Category);
        assert_eq!(chart.plot_area.axes[1].kind, AxisKind::Value);
        assert!(chart.plot_area.axes[1].major_gridlines);
        assert_eq!(chart.plot_area.axes[2].position, Some(AxisPosition::Right));
    }

    /// An out-of-scope family yields NO projection. It is preserved and reported
    /// exactly as it is today (`docs/155` §4.3, §6.2), because a partial
    /// projection of a 3-D or stock chart would be *drawn wrongly* rather than not
    /// drawn, which is worse than the placeholder it replaces.
    #[test]
    fn out_of_scope_chart_yields_no_projection_and_reports_it() {
        for family in [
            "bar3DChart",
            "line3DChart",
            "area3DChart",
            "pie3DChart",
            "surface3DChart",
            "surfaceChart",
            "stockChart",
            "radarChart",
            "bubbleChart",
            "ofPieChart",
        ] {
            let inner = format!(r#"<c:{family}><c:ser><c:idx val="0"/></c:ser></c:{family}>"#);
            let read = read(&chart_space(&inner));
            assert!(
                read.projection.is_none(),
                "{family} must not project: a family outside tier 1 declines whole"
            );
            assert_eq!(
                read.declined,
                Some(ChartDecline::OutOfScopeFamily(family.to_owned())),
                "{family} must decline by naming itself, so the report can say which"
            );
        }
    }

    /// Every construct `docs/155` §4.3 puts out of scope produces a finding.
    ///
    /// This asserts the **guarantee** — any construct the projection does not
    /// represent is named — and not the circumstance that some fixture happened to
    /// raise three findings. It is driven by the §4.3 table rather than by a list
    /// of what the reader currently happens to skip, which is the only version of
    /// this test that can fail when a construct stops being reported.
    ///
    /// A family that graduates INTO tier 1 must be removed from this table in the
    /// same change that projects it, which is the intended coupling: the table is
    /// the out-of-scope list, so it changes when the list does.
    #[test]
    fn every_out_of_scope_construct_is_enumerated() {
        let in_series = [
            (
                r#"<c:trendline><c:trendlineType val="linear"/></c:trendline>"#,
                "trendline",
            ),
            (
                r#"<c:errBars><c:errBarType val="both"/></c:errBars>"#,
                "errBars",
            ),
            (r#"<c:dPt><c:idx val="0"/></c:dPt>"#, "dPt"),
            (r#"<c:explosion val="25"/>"#, "explosion"),
            (
                r#"<c:spPr><a:gradFill><a:gsLst/></a:gradFill></c:spPr>"#,
                "gradFill",
            ),
            (
                r#"<c:spPr><a:blipFill><a:blip/></a:blipFill></c:spPr>"#,
                "blipFill",
            ),
            (
                r#"<c:spPr><a:pattFill><a:fgClr/></a:pattFill></c:spPr>"#,
                "pattFill",
            ),
            (
                r#"<c:spPr><a:effectLst><a:outerShdw/></a:effectLst></c:spPr>"#,
                "effectLst",
            ),
        ];
        for (markup, expected) in in_series {
            let inner = CACHED_BAR.replace("</c:ser>", &format!("{markup}</c:ser>"));
            let read = read(&chart_space(&inner));
            assert!(
                read.unconsumed.contains(expected),
                "{expected} must be named as unconsumed; got {:?}",
                read.unconsumed
            );
            assert_eq!(
                read.projection.expect("a projection").coverage,
                ChartCoverage::Partial,
                "{expected} is a construct the projection does not hold, so coverage is partial"
            );
        }
        for (markup, expected) in [
            (
                r#"<c:dTable><c:showHorzBorder val="1"/></c:dTable>"#,
                "dTable",
            ),
            (r#"<c:spPr><a:solidFill/></c:spPr>"#, "spPr"),
        ] {
            let inner = format!("{CACHED_BAR}{markup}");
            let read = read(&chart_space(&inner));
            assert!(
                read.unconsumed.contains(expected),
                "{expected} must be named as unconsumed; got {:?}",
                read.unconsumed
            );
        }
        // `c:view3D` sits under `c:chart`, beside the plot area.
        let xml = format!(
            r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:view3D><c:rotX val="15"/></c:view3D><c:plotArea>{CACHED_BAR}</c:plotArea></c:chart></c:chartSpace>"#
        );
        let read = read(xml.as_bytes());
        assert!(
            read.unconsumed.contains("view3D"),
            "c:view3D must be named; got {:?}",
            read.unconsumed
        );
    }

    /// A chart whose every construct is modeled is `Complete`, which is what
    /// licenses regeneration. The counterpart of the test above: if everything were
    /// reported partial the coverage gate would be a constant.
    #[test]
    fn a_fully_modeled_chart_is_complete_coverage() {
        let read = read(&chart_space(CACHED_BAR));
        assert!(
            read.unconsumed.is_empty(),
            "nothing in this chart is outside the projection; got {:?}",
            read.unconsumed
        );
        let chart = read.projection.expect("a projection");
        assert_eq!(chart.coverage, ChartCoverage::Complete);
        assert!(
            chart.coverage.permits_regeneration(),
            "a complete projection is the only one a writer may regenerate from"
        );
    }

    /// `CT_SerTx` is `(strRef | v)`, so a series name can be literal text with no
    /// workbook reference behind it — and that spelling was unread.
    ///
    /// # Why this is two defects and not one
    ///
    /// The obvious half is that the name was dropped. The expensive half is that
    /// the unread `c:v` landed in `unconsumed`, which makes the whole projection
    /// [`ChartCoverage::Partial`], and `Partial` is the flag that FORBIDS
    /// regenerating the part. So one unread element in a chart whose every other
    /// construct was understood disabled the chart part writer for that chart
    /// (`109` HF-256) and the compatibility report claimed a loss that was the
    /// reader's, not the document's.
    ///
    /// Both halves are asserted, because a fix that read the name while still
    /// recording the element would leave the second one in place.
    #[test]
    fn a_series_name_spelled_as_a_literal_c_v_is_read_and_does_not_cost_coverage() {
        const LITERAL_NAME: &str = r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>Series 1</c:v></c:tx><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>4.3</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>"#;
        let read = read(&chart_space(LITERAL_NAME));
        assert!(
            read.unconsumed.is_empty(),
            "a literal series name is modeled, so nothing in this chart is a loss; got {:?}",
            read.unconsumed
        );
        let chart = read.projection.expect("a projection");
        assert_eq!(
            chart.coverage,
            ChartCoverage::Complete,
            "an unread `c:v` must not cost the chart its regeneration licence"
        );
        let name = chart.plot_area.groups[0].series[0]
            .name
            .as_ref()
            .expect("the literal series name must be read");
        assert_eq!(name.text, "Series 1");
        assert_eq!(
            name.formula, None,
            "a literal name has no formula to carry, and inventing one would be a \
             reference nothing resolves"
        );
    }

    /// A malformed, truncated or non-chart part declines rather than failing.
    /// `docs/155` §6.1 consequence 3: a projection failure is never a document
    /// failure. `read_chart_part` returns no `Result`, so this is a property of the
    /// signature as much as of the body — and the assertion is that each of these
    /// inputs still produces an answer.
    #[test]
    fn a_malformed_chart_part_does_not_fail_the_import() {
        // Each case names the decline it expects, and the expectations are
        // MEASURED rather than assumed: `quick-xml` is lenient in places, so
        // "obviously malformed" and "the reader returns `Malformed`" are not the
        // same set, and a test asserting merely "some decline" would have been
        // satisfied by `NothingToProject` everywhere — which is how the
        // package-level counterpart of this test stayed green under a mutation
        // that made the reader panic on a malformed part.
        for (name, xml, expected) in [
            // Genuine parse failures.
            (
                "a doctype",
                br#"<!DOCTYPE c:chartSpace><c:chartSpace/>"#.as_slice(),
                ChartDecline::Malformed,
            ),
            (
                "an unterminated tag",
                b"<c:chartSpace".as_slice(),
                ChartDecline::Malformed,
            ),
            (
                "a mismatched close tag",
                b"<c:chartSpace></wrong>".as_slice(),
                ChartDecline::Malformed,
            ),
            (
                "a part that is not a chart space at all",
                br#"<w:document xmlns:w="urn:w"/>"#.as_slice(),
                ChartDecline::Malformed,
            ),
            // Readable, but with no chart in it. A different decline because the
            // two get different report treatment: a malformed part says nothing
            // about the chart, an empty one says there was nothing to project.
            (
                "no elements at all",
                b"PK\x03\x04 this is a zip".as_slice(),
                ChartDecline::NothingToProject,
            ),
            ("nothing", b"".as_slice(), ChartDecline::NothingToProject),
            (
                "an unterminated chart space",
                b"<c:chartSpace><c:chart>".as_slice(),
                ChartDecline::NothingToProject,
            ),
            (
                "a well-formed but empty chart space",
                br#"<c:chartSpace xmlns:c="urn:c"/>"#.as_slice(),
                ChartDecline::NothingToProject,
            ),
            (
                // A `chartex` part. The root is matched by LOCAL name, so
                // `cx:chartSpace` passes that check and then declines for want of
                // a `c:plotArea` — the right outcome by a slightly indirect route.
                // Recorded rather than smoothed over: a chartex part never reaches
                // this reader anyway, because a `cx:chart` payload is not routed as
                // an `EmbeddedKind::Chart` in the first place (`docs/155` §1).
                "a chartex part",
                br#"<cx:chartSpace xmlns:cx="urn:chartex"><cx:chart/></cx:chartSpace>"#.as_slice(),
                ChartDecline::NothingToProject,
            ),
        ] {
            let read = read(xml);
            assert!(
                read.projection.is_none(),
                "{name}: a part that cannot be read must not yield a projection"
            );
            assert_eq!(
                read.declined,
                Some(expected),
                "{name}: the decline must be named, not merely present"
            );
        }
    }

    /// A no-op construct raises nothing, and its populated form still does.
    ///
    /// Both halves, as `crate::noop`'s header requires: `<c:layout/>` is automatic
    /// layout and a populated `c:layout` is a manual one this projection does not
    /// carry. Silencing the populated form would hide a real loss, which is worse
    /// than the false one the rule removes.
    #[test]
    fn a_chart_no_op_is_silent_and_its_populated_form_is_not() {
        let empty = read(&chart_space(CACHED_BAR));
        assert!(
            !empty.unconsumed.contains("layout"),
            "an empty c:layout is automatic layout, not a loss"
        );
        let manual = format!(
            r#"<c:layout><c:manualLayout><c:x val="0.1"/></c:manualLayout></c:layout>{CACHED_BAR}"#
        );
        let read = read(&chart_space(&manual));
        assert!(
            read.unconsumed.contains("layout"),
            "a manual c:layout IS a loss; got {:?}",
            read.unconsumed
        );
    }

    /// A no-op construct takes its whole subtree with it.
    ///
    /// `c:extLst` is the only chart no-op that can have children, and the first
    /// version of this reader answered a no-op with "descend" — so `c:ext` and
    /// whatever a producer put inside it were each reported as unconsumed. That is
    /// HF-174 (findings describing losses that did not happen) reappearing inside
    /// the code written to avoid it, which is why this has a guard of its own.
    #[test]
    fn a_chart_no_op_takes_its_subtree_with_it() {
        let inner = format!(
            r#"{CACHED_BAR}<c:extLst><c:ext uri="{{00000000-0000-0000-0000-000000000000}}" xmlns:c15="urn:c15"><c15:filteredSeriesTitle><c15:tx/></c15:filteredSeriesTitle></c:ext></c:extLst>"#
        );
        let read = read(&chart_space(&inner));
        assert!(
            read.unconsumed.is_empty(),
            "nothing inside a c:extLst is a loss; got {:?}",
            read.unconsumed
        );
        assert_eq!(
            read.projection.expect("a projection").coverage,
            ChartCoverage::Complete,
            "an extension list must not make a projection partial"
        );
    }

    /// A list beyond its `docs/155` §8.4 bound declines instead of building an
    /// unbounded projection — and still does not fail the import.
    #[test]
    fn an_over_bound_chart_declines_rather_than_growing() {
        let points: String = (0..MAX_CHART_DATA_POINTS + 1)
            .map(|index| format!(r#"<c:pt idx="{index}"><c:v>1</c:v></c:pt>"#))
            .collect();
        let inner = format!(
            r#"<c:barChart><c:ser><c:idx val="0"/><c:val><c:numRef><c:numCache><c:ptCount val="1"/>{points}</c:numCache></c:numRef></c:val></c:ser></c:barChart>"#
        );
        let read = read(&chart_space(&inner));
        assert!(read.projection.is_none());
        assert_eq!(
            read.declined,
            Some(ChartDecline::OverBound("chart.dataRange.points"))
        );
    }
}
