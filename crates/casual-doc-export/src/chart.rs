// SPDX-License-Identifier: Apache-2.0

//! The DrawingML chart-part **writer**: `v1::Chart` -> `word/charts/chartN.xml`
//! (`docs/155` §6.1, `109` HF-256).
//!
//! # Why this exists, and the one case it serves
//!
//! An IMPORTED chart round-trips verbatim: its bytes sit in the retained-parts
//! side table and the package writer re-emits them unchanged. That is strictly
//! better than regenerating them, because the projection is a *read index* over
//! those bytes and not a replacement for them (`v1::chart`'s module header).
//!
//! A chart the EDITOR mints has no source bytes. Before this module the package
//! writer emitted its `document.xml.rels` entry and its `c:chart r:id` and then
//! wrote no part: a live relationship pointing at nothing, which Word reads as a
//! corrupt package. So this writer runs for exactly one population — a chart
//! object whose part is **not** retained and whose projection says it is
//! [`ChartCoverage::Complete`] — and never competes with retention.
//!
//! # The coverage gate is the whole safety argument
//!
//! [`ChartCoverage::permits_regeneration`] is consulted rather than assumed. A
//! `Partial` projection is one the reader could not fully represent, so writing
//! it back would drop whatever it did not model (a trendline, a data table, a
//! gradient fill) while producing a file that *looks* fine. When the gate
//! refuses, this module writes no part and the caller reports the loss; it never
//! writes an approximation.
//!
//! # Schema order is not decoration
//!
//! Every sequence below is in ECMA-376 Part 1 order (`CT_ChartSpace`,
//! `CT_Chart`, `CT_PlotArea`, the six `CT_*Chart` groups, `CT_*Ser`,
//! `CT_CatAx`/`CT_ValAx`/`CT_DateAx`). Word validates a chart part against the
//! sequence and repairs — i.e. discards — a part whose children are out of
//! order, so an element written in the wrong place is silent data loss with a
//! valid-looking file. Each writer states its sequence in a comment beside it so
//! the next element added has somewhere to go.
//!
//! # Complexity
//!
//! O(points in the chart) for one part, once per export. Bounded by the model's
//! own ceilings (`MAX_CHART_GROUPS`, `MAX_CHART_SERIES_PER_GROUP`,
//! `MAX_CHART_DATA_POINTS`), which the importer and `check_chart` both enforce,
//! so a hostile projection cannot make this unbounded.

use std::collections::BTreeMap;
use std::io::Cursor;

use casual_doc_import::RetainedParts;
use casual_doc_model::strip_xml_forbidden;
use casual_doc_model::v1::{
    Axis, AxisKind, AxisOrientation, AxisPosition, BarDirection, BarGrouping, BlockNode, Chart,
    ChartGroup, ChartGroupKind, ChartLine, ChartText, ChartTitle, ChartValue, Color,
    DataLabelPosition, DataLabels, DataRange, DisplayBlanks, Document, EmbeddedKind,
    EmbeddedObject, Grouping, InlineNode, Legend, LegendPosition, PlotArea, ScatterStyle, Series,
    ThemeColor, ThemeColorRef, TickLabelPosition, TickMark,
};
use casual_doc_model::{NodeId, v1::EmbeddedPart};
// Own `use` lines (rustfmt `Preserve`): the group walk that reaches a chart
// inside a grouped text box. Kept separate so a parallel lane adding to the
// sorted block above does not conflict here.
use casual_doc_model::v1::GroupChild;
use casual_doc_model::v1::WordprocessingGroup;
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesText, Event};

use crate::ExportError;
use crate::chart_workbook::{self, GeneratedWorkbook};
use crate::report::{Disposition, Reporter};
use crate::semantic::{finish, new_writer, pkg, start};

/// The content type of a `word/charts/chartN.xml` part.
pub(crate) const CHART_PART_CT: &str =
    "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";

/// `c:` — the DrawingML chart namespace.
const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
/// `a:` — the DrawingML main namespace (fills, lines, rich text).
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
/// `r:` — the relationship namespace, for `c:externalData@r:id`.
const RELATIONSHIP_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A chart part this export generated from a projection, with its content type
/// and its own `_rels` companion when it has one.
///
/// Deliberately not an `ExtraPart`: that type also mints a `document.xml.rels`
/// entry, and a chart's relationship is already emitted from the node's verbatim
/// `EmbeddedPart::relationship_id` (`collect_embedded_rels`). Routing a generated
/// chart through `ExtraPart` would emit the same `Id` twice, and Word reads a
/// duplicate relationship id as a corrupt package.
pub(crate) struct GeneratedChartPart {
    /// The part name, taken verbatim from the object's `EmbeddedPart`, so the
    /// part this writes and the relationship the body references cannot disagree.
    pub(crate) part_name: String,
    /// The part's content type, for the `[Content_Types].xml` `Override`.
    pub(crate) content_type: &'static str,
    /// The serialized `c:chartSpace`.
    pub(crate) bytes: Vec<u8>,
    /// The chart part's own relationships part — `(part name, bytes)` — when the
    /// chart references something of its own. See [`chart_part_rels`].
    pub(crate) rels: Option<(String, Vec<u8>)>,
    /// The values-only workbook the part names, when one was written
    /// (`chart_workbook`).
    pub(crate) workbook: Option<GeneratedWorkbook>,
    /// Retained part names this generated output REPLACES: the chart part itself
    /// when an edit made a retained chart dirty, and the workbook replaced in
    /// place. The package writer skips them, because writing both would put two
    /// ZIP entries under one name — a package no reader agrees about.
    pub(crate) supersedes: Vec<String>,
}

/// Every chart part this export must generate rather than copy.
///
/// # Every block container, not just the body
///
/// A chart is legal in a header, a footer, a footnote, an endnote and a comment —
/// `Document::visit_chart_object_ids` walks all of them for exactly that reason,
/// so the model admits a projection there. This walked the body alone while the
/// relationship walk was body-only too; both are now per-part, so a chart the
/// editor mints on a running surface gets its part written and its own part's
/// `_rels` declares it (`109` HF-256). Walking the body alone here would have
/// left the surface's relationship pointing at nothing — the same defect one
/// container along.
///
/// Part names are package-global, so one flat list is still the right answer: the
/// caller writes each part once and `available_embedded_parts` admits it for
/// every surface that references it.
///
/// # Complexity: O(document + charts), one walk, no lookup in a loop
///
/// The projection table is inverted into an anchor -> projection map **once**
/// (O(charts)) before the walk, so the per-object step is a map probe and not a
/// scan of `Definitions::charts`. Walking the document and scanning the table per
/// object would be O(objects x charts) — the shape `SKILL` §8 forbids.
pub(crate) fn generate_chart_parts(
    document: &Document,
    retained_parts: &RetainedParts,
    reporter: &mut Reporter,
) -> Result<Vec<GeneratedChartPart>, ExportError> {
    let definitions = document.definitions();
    let charts = &definitions.charts;
    if charts.is_empty() {
        return Ok(Vec::new());
    }
    let by_anchor: BTreeMap<NodeId, &Chart> = charts
        .iter()
        .map(|(_, chart)| (chart.object, chart))
        .collect();
    let mut objects: Vec<&EmbeddedObject> = Vec::new();
    let notes = definitions
        .footnotes
        .iter()
        .chain(definitions.endnotes.iter())
        .map(|(_, note)| note.blocks.as_slice());
    let running = definitions
        .headers
        .iter()
        .chain(definitions.footers.iter())
        .map(|(_, part)| part.blocks.as_slice());
    let comments = definitions
        .comments
        .iter()
        .map(|(_, comment)| comment.blocks.as_slice());
    for blocks in std::iter::once(document.body())
        .chain(notes)
        .chain(running)
        .chain(comments)
    {
        for block in blocks {
            collect_chart_objects_in_block(block, &mut objects);
        }
    }
    let mut generated: Vec<GeneratedChartPart> = Vec::new();
    for object in objects {
        if generated
            .iter()
            .any(|part| part.part_name == object.part.part_name)
        {
            // Two chart objects naming one part. `insertChart` derives the name
            // from the object's own id so its inserts cannot collide, but a
            // model that arrives by snapshot load or a future importer can carry
            // this — and writing the part twice would put two ZIP entries under
            // one name, which is a package no reader agrees about. First writer
            // wins, deterministically, because the walk order above is fixed.
            continue;
        }
        let retained = retained_parts
            .parts
            .iter()
            .any(|part| part.part_name == object.part.part_name);
        let Some(chart) = by_anchor.get(&object.id) else {
            // A chart object with neither retained bytes nor a projection. There
            // is nothing to write from; the caller's
            // `docx.export.embedded_object.missing_part` finding already names it.
            continue;
        };
        if retained && !chart.dirty {
            // Retention wins: the source bytes are the authority and the
            // projection is only a read index over them (`docs/155` §6.1) —
            // until an edit makes the projection the authority (`Chart::dirty`).
            continue;
        }
        if !chart.coverage.permits_regeneration() {
            // The gate, not an assumption. Reported under its own id because
            // "we understood the part only partly" is a different fact for the
            // reader than "the part is missing". A dirty chart that fails it
            // keeps its retained bytes: the edit is lost, and said so, rather
            // than the part being rewritten without what it did not model.
            reporter.record_part(
                "docx.export.chart.partial_coverage_not_regenerated",
                &object.part.part_name,
                Disposition::OmittedNotRetained,
            );
            continue;
        }
        let (chart, workbook) =
            match chart_workbook::bind(chart, &object.part.part_name, retained_parts)? {
                Some((bound, workbook)) => (bound, Some(workbook)),
                None => ((*chart).clone(), None),
            };
        let mut supersedes = Vec::new();
        if retained {
            supersedes.push(object.part.part_name.clone());
        }
        if let Some(workbook) = workbook
            .as_ref()
            .filter(|workbook| workbook.replaces_retained)
        {
            // The producer's workbook may have held formulas, formatting or
            // other sheets the chart never showed; the replacement holds exactly
            // the chart's data. That is the reader's edit taking effect, and it
            // is named rather than silent.
            reporter.record_part(
                "docx.export.chart.workbook_replaced",
                &workbook.part_name,
                Disposition::DegradedNotRetained,
            );
            supersedes.push(workbook.part_name.clone());
        }
        let bytes = write_chart_part(&chart)?;
        let rels = chart_part_rels(
            &chart,
            retained_parts,
            workbook
                .as_ref()
                .map(|workbook| workbook.part_name.as_str()),
        )?
        .map(|bytes| (rels_part_name(&object.part.part_name), bytes));
        generated.push(GeneratedChartPart {
            part_name: object.part.part_name.clone(),
            content_type: CHART_PART_CT,
            bytes,
            rels,
            workbook,
            supersedes,
        });
    }
    Ok(generated)
}

/// The `_rels` companion name for a part, e.g. `word/charts/chart1.xml` ->
/// `word/charts/_rels/chart1.xml.rels`.
fn rels_part_name(part_name: &str) -> String {
    match part_name.rfind('/') {
        Some(slash) => format!(
            "{}/_rels/{}.rels",
            &part_name[..slash],
            &part_name[slash + 1..]
        ),
        None => format!("_rels/{part_name}.rels"),
    }
}

/// Collects every embedded CHART object in `block`, in document order.
///
/// Complexity: O(the block subtree).
fn collect_chart_objects_in_block<'a>(block: &'a BlockNode, out: &mut Vec<&'a EmbeddedObject>) {
    match block {
        BlockNode::Paragraph(paragraph) => {
            collect_chart_objects_in_inlines(&paragraph.inlines, out);
        }
        BlockNode::Table(table) => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        collect_chart_objects_in_block(block, out);
                    }
                }
            }
        }
        BlockNode::Sdt(sdt) => {
            for block in &sdt.blocks {
                collect_chart_objects_in_block(block, out);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

/// [`collect_chart_objects_in_block`] for an inline list, descending through
/// every container that can hold one.
fn collect_chart_objects_in_inlines<'a>(
    inlines: &'a [InlineNode],
    out: &mut Vec<&'a EmbeddedObject>,
) {
    for inline in inlines {
        match inline {
            InlineNode::EmbeddedObject(object) if object.kind == EmbeddedKind::Chart => {
                out.push(object);
            }
            InlineNode::Hyperlink(link) => collect_chart_objects_in_inlines(&link.inlines, out),
            InlineNode::Field(field) => collect_chart_objects_in_inlines(&field.inlines, out),
            InlineNode::Revision(revision) => {
                collect_chart_objects_in_inlines(&revision.inlines, out);
            }
            InlineNode::Sdt(sdt) => collect_chart_objects_in_inlines(&sdt.inlines, out),
            InlineNode::TextBox(text_box) => {
                for block in &text_box.blocks {
                    collect_chart_objects_in_block(block, out);
                }
            }
            // A grouped shape's text box holds blocks like any other container,
            // and `write_group_text_box` writes a chart inside one — so a part
            // has to be generated for it too (`109` HF-196).
            InlineNode::Group(group) => collect_chart_objects_in_group(group, out),
            _ => {}
        }
    }
}

/// [`collect_chart_objects_in_inlines`] for a DrawingML group's children.
///
/// Complexity: O(the group subtree); `MAX_GROUP_DEPTH` bounds the recursion.
fn collect_chart_objects_in_group<'a>(
    group: &'a WordprocessingGroup,
    out: &mut Vec<&'a EmbeddedObject>,
) {
    for child in &group.children {
        match child {
            GroupChild::TextBox(text_box) => {
                for block in &text_box.blocks {
                    collect_chart_objects_in_block(block, out);
                }
            }
            GroupChild::Group(nested) => collect_chart_objects_in_group(nested, out),
            GroupChild::Picture(_) | GroupChild::Shape(_) => {}
        }
    }
}

/// Serializes one projection as a `c:chartSpace` part.
///
/// `CT_ChartSpace` sequence: `date1904?`, `lang?`, `roundedCorners?`, `style?`,
/// `clrMapOvr?`, `pivotSource?`, `protection?`, **`chart`**, `spPr?`, `txPr?`,
/// `externalData?`, `printSettings?`, `userShapes?`, `extLst?`.
///
/// Complexity: O(points in the chart).
pub(crate) fn write_chart_part(chart: &Chart) -> Result<Vec<u8>, ExportError> {
    let mut w = new_writer();
    let mut space = start("c:chartSpace");
    space.push_attribute(("xmlns:c", CHART_NS));
    space.push_attribute(("xmlns:a", DRAWING_NS));
    space.push_attribute(("xmlns:r", RELATIONSHIP_NS));
    w.write_event(Event::Start(space)).map_err(pkg)?;
    write_chart(&mut w, chart)?;
    if let Some(external) = &chart.external_data {
        write_external_data(&mut w, external)?;
    }
    w.write_event(Event::End(BytesEnd::new("c:chartSpace")))
        .map_err(pkg)?;
    Ok(finish(w))
}

/// The chart part's own relationships, when it has any.
///
/// Today that is exactly one: the embedded workbook `c:externalData` names,
/// which hangs off the CHART part's rels and not the document's (`docs/155`
/// §5.2). It is emitted **only when the package will actually contain that
/// part** — a relationship to a workbook whose bytes nobody supplied is the same
/// dangling reference HF-256 is about, one level down, so the `r:id` is dropped
/// with it rather than written beside a part that is not there.
///
/// `Ok(None)` means "this chart needs no rels part", which is the common case: a
/// chart the editor minted has no workbook, because the cached data *is* the
/// data and there is no worksheet behind it.
///
/// Complexity: O(retained parts) for the containment check, once per chart.
fn chart_part_rels(
    chart: &Chart,
    retained_parts: &RetainedParts,
    generated_workbook: Option<&str>,
) -> Result<Option<Vec<u8>>, ExportError> {
    let Some(external) = &chart.external_data else {
        return Ok(None);
    };
    if generated_workbook != Some(external.part_name.as_str())
        && !retained_parts
            .parts
            .iter()
            .any(|part| part.part_name == external.part_name)
    {
        return Ok(None);
    }
    let mut w = new_writer();
    let mut rels = start("Relationships");
    rels.push_attribute((
        "xmlns",
        "http://schemas.openxmlformats.org/package/2006/relationships",
    ));
    w.write_event(Event::Start(rels)).map_err(pkg)?;
    let mut rel = start("Relationship");
    rel.push_attribute(("Id", external.relationship_id.as_str()));
    rel.push_attribute(("Type", external.relationship_type.as_str()));
    rel.push_attribute((
        "Target",
        chart_relative_target(&external.part_name).as_str(),
    ));
    w.write_event(Event::Empty(rel)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("Relationships")))
        .map_err(pkg)?;
    Ok(Some(finish(w)))
}

/// A part name as a target relative to `word/charts/`, e.g.
/// `word/embeddings/Book1.xlsx` -> `../embeddings/Book1.xlsx`.
///
/// Relationship targets resolve against the *referencing part's* directory, so a
/// chart's own rels cannot use the `word/`-relative form `document.xml.rels`
/// uses. Getting this wrong points Word at `word/charts/embeddings/...`.
fn chart_relative_target(part_name: &str) -> String {
    match part_name.strip_prefix("word/charts/") {
        Some(inside) => inside.to_owned(),
        None => match part_name.strip_prefix("word/") {
            Some(rest) => format!("../{rest}"),
            None => format!("/{part_name}"),
        },
    }
}

/// `CT_Chart` sequence: `title?`, `autoTitleDeleted?`, `pivotFmts?`, `view3D?`,
/// `floor?`, `sideWall?`, `backWall?`, **`plotArea`**, `legend?`,
/// `plotVisOnly?`, `dispBlanksAs?`, `showDLblsOverMax?`, `extLst?`.
fn write_chart(w: &mut Writer<Cursor<Vec<u8>>>, chart: &Chart) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:chart"))).map_err(pkg)?;
    if let Some(title) = &chart.title {
        write_title(w, title)?;
    }
    if chart.auto_title_deleted {
        write_val(w, "c:autoTitleDeleted", "1")?;
    }
    write_plot_area(w, &chart.plot_area)?;
    if let Some(legend) = &chart.legend {
        write_legend(w, legend)?;
    }
    // Both written unconditionally rather than only when true. Their SCHEMA
    // defaults are not the model's defaults (`c:plotVisOnly` defaults to 1 in
    // ECMA-376 and to `false` in a Rust `bool`), so omitting a false value would
    // be read back as true — a round-trip that changes the document.
    write_val(w, "c:plotVisOnly", bool_val(chart.plot_visible_only))?;
    write_val(
        w,
        "c:dispBlanksAs",
        match chart.display_blanks_as {
            DisplayBlanks::Gap => "gap",
            DisplayBlanks::Zero => "zero",
            DisplayBlanks::Span => "span",
        },
    )?;
    // `Chart::vary_colors` is deliberately NOT written: `CT_Chart` has no
    // `c:varyColors` child in ECMA-376 — the element only exists on a chart
    // GROUP, and that is where `ChartGroup::vary_colors` writes it. The chart-
    // space-level field exists because the reader accepts one at that depth from
    // a non-conformant producer; inventing a schema position to write it back
    // would make Word repair the part and discard the whole chart.
    w.write_event(Event::End(BytesEnd::new("c:chart")))
        .map_err(pkg)
}

/// `CT_Title` sequence: `tx?`, `layout?`, `overlay?`, `spPr?`, `txPr?`.
fn write_title(w: &mut Writer<Cursor<Vec<u8>>>, title: &ChartTitle) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:title"))).map_err(pkg)?;
    if let Some(text) = &title.text {
        w.write_event(Event::Start(start("c:tx"))).map_err(pkg)?;
        match &text.formula {
            // A cached reference: the formula plus the one-cell cache it
            // evaluated to, which is what the reader read and what Word repaints
            // without opening a workbook.
            Some(formula) => write_str_ref(w, formula, std::slice::from_ref(&text.text))?,
            // Literal text has no `c:v` spelling inside `CT_Tx` — that variant is
            // `CT_SerTx`, for a series name. A title's literal text is a `c:rich`
            // DrawingML body, which is also what Word writes.
            None => write_rich_text(w, &text.text)?,
        }
        w.write_event(Event::End(BytesEnd::new("c:tx")))
            .map_err(pkg)?;
    }
    write_val(w, "c:overlay", bool_val(title.overlay))?;
    w.write_event(Event::End(BytesEnd::new("c:title")))
        .map_err(pkg)
}

/// A minimal `c:rich` body: the paragraph and run structure the projection
/// flattened away is not reconstructed, because the model holds one string
/// (`ChartText::text`, `docs/155` §4.2 admits cached rich text only). Writing one
/// run is therefore the whole of what is known, not a simplification of it.
fn write_rich_text(w: &mut Writer<Cursor<Vec<u8>>>, text: &str) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:rich"))).map_err(pkg)?;
    w.write_event(Event::Empty(start("a:bodyPr")))
        .map_err(pkg)?;
    w.write_event(Event::Empty(start("a:lstStyle")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("a:p"))).map_err(pkg)?;
    w.write_event(Event::Start(start("a:r"))).map_err(pkg)?;
    write_text_element(w, "a:t", text)?;
    w.write_event(Event::End(BytesEnd::new("a:r")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:p")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("c:rich")))
        .map_err(pkg)
}

/// `CT_PlotArea` sequence: `layout?`, **one or more chart groups**, **the axes**,
/// `dTable?`, `spPr?`.
///
/// The groups come before the axes and both are lists, which is the central
/// modeling decision of `docs/155` §4.2 surviving into the writer: a combo chart
/// is more than one group and a secondary axis is one more axis a group names, so
/// neither needs a special case here.
fn write_plot_area(w: &mut Writer<Cursor<Vec<u8>>>, plot: &PlotArea) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:plotArea")))
        .map_err(pkg)?;
    // Empty = automatic layout, which is what the projection means by holding no
    // manual one (`chart_noop` reads it the same way).
    w.write_event(Event::Empty(start("c:layout")))
        .map_err(pkg)?;
    for group in &plot.groups {
        write_group(w, group)?;
    }
    for axis in &plot.axes {
        write_axis(w, axis, plot)?;
    }
    w.write_event(Event::End(BytesEnd::new("c:plotArea")))
        .map_err(pkg)
}

/// One chart group, in its family's own `CT_*Chart` sequence.
///
/// Every family the model can express has an arm, so the set this writes and the
/// set [`ChartGroupKind`] represents are the same set — which is the property the
/// no-dead-control guard checks: a family the editor can insert is a family the
/// package writer can save.
fn write_group(w: &mut Writer<Cursor<Vec<u8>>>, group: &ChartGroup) -> Result<(), ExportError> {
    let element = group_element(group.kind);
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    // The leading, family-specific children.
    match group.kind {
        // `CT_BarChart`: barDir, grouping?, varyColors?, ser*, dLbls?, gapWidth?,
        // overlap?, serLines*, axId, axId.
        ChartGroupKind::Bar {
            direction,
            grouping,
            ..
        } => {
            write_val(
                w,
                "c:barDir",
                match direction {
                    BarDirection::Column => "col",
                    BarDirection::Bar => "bar",
                },
            )?;
            write_val(
                w,
                "c:grouping",
                match grouping {
                    BarGrouping::Clustered => "clustered",
                    BarGrouping::Stacked => "stacked",
                    BarGrouping::PercentStacked => "percentStacked",
                    BarGrouping::Standard => "standard",
                },
            )?;
        }
        // `CT_LineChart`/`CT_AreaChart`: grouping, varyColors?, ser*, ...
        ChartGroupKind::Line { grouping, .. } | ChartGroupKind::Area { grouping } => {
            write_val(w, "c:grouping", grouping_str(grouping))?;
        }
        // `CT_ScatterChart`: scatterStyle, varyColors?, ser*, dLbls?, axId, axId.
        ChartGroupKind::Scatter { style } => {
            write_val(
                w,
                "c:scatterStyle",
                match style {
                    ScatterStyle::None => "none",
                    ScatterStyle::Line => "line",
                    ScatterStyle::LineMarker => "lineMarker",
                    ScatterStyle::Marker => "marker",
                    ScatterStyle::Smooth => "smooth",
                    ScatterStyle::SmoothMarker => "smoothMarker",
                },
            )?;
        }
        // `CT_PieChart`/`CT_DoughnutChart` open with varyColors?.
        ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. } => {}
    }
    write_val(w, "c:varyColors", bool_val(group.vary_colors))?;
    for series in &group.series {
        write_series(w, series, group.kind)?;
    }
    // The trailing, family-specific children, after the series in every sequence.
    match group.kind {
        ChartGroupKind::Bar {
            gap_width, overlap, ..
        } => {
            write_val(w, "c:gapWidth", &gap_width.to_string())?;
            write_val(w, "c:overlap", &overlap.to_string())?;
        }
        ChartGroupKind::Line { marker, .. } => {
            write_val(w, "c:marker", bool_val(marker))?;
        }
        ChartGroupKind::Pie { first_slice_angle } => {
            write_val(w, "c:firstSliceAng", &first_slice_angle.to_string())?;
        }
        ChartGroupKind::Doughnut {
            first_slice_angle,
            hole_size,
        } => {
            write_val(w, "c:firstSliceAng", &first_slice_angle.to_string())?;
            write_val(w, "c:holeSize", &hole_size.to_string())?;
        }
        ChartGroupKind::Area { .. } | ChartGroupKind::Scatter { .. } => {}
    }
    // `c:axId` closes every sequence that has axes. The pie families have none at
    // all, which is why nothing is written for them: `CT_PieChart` has no `axId`
    // child, and emitting one would make Word repair the part.
    if group_takes_axes(group.kind) {
        for id in &group.axis_ids {
            write_val(w, "c:axId", &id.to_string())?;
        }
    }
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)
}

/// The `c:*Chart` element name for a family.
const fn group_element(kind: ChartGroupKind) -> &'static str {
    match kind {
        ChartGroupKind::Bar { .. } => "c:barChart",
        ChartGroupKind::Line { .. } => "c:lineChart",
        ChartGroupKind::Area { .. } => "c:areaChart",
        ChartGroupKind::Pie { .. } => "c:pieChart",
        ChartGroupKind::Doughnut { .. } => "c:doughnutChart",
        ChartGroupKind::Scatter { .. } => "c:scatterChart",
    }
}

/// Whether a family's `CT_*Chart` sequence ends in `c:axId` children.
///
/// The pie families plot into no axes, so their sequences have none.
const fn group_takes_axes(kind: ChartGroupKind) -> bool {
    !matches!(
        kind,
        ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. }
    )
}

/// The `c:grouping` token shared by the line and area families.
const fn grouping_str(grouping: Grouping) -> &'static str {
    match grouping {
        Grouping::Standard => "standard",
        Grouping::Stacked => "stacked",
        Grouping::PercentStacked => "percentStacked",
    }
}

/// One `c:ser`.
///
/// The six `CT_*Ser` sequences differ, but they agree on the prefix this writes —
/// `idx`, `order`, `tx?`, `spPr?`, (family extras), `dLbls?`, the data sources,
/// `smooth?` — so one writer covers all six rather than six near-copies. The two
/// places they genuinely diverge are marked: a scatter series names `xVal`/`yVal`
/// where the others name `cat`/`val`, and `c:smooth` exists only on the line and
/// scatter sequences.
fn write_series(
    w: &mut Writer<Cursor<Vec<u8>>>,
    series: &Series,
    kind: ChartGroupKind,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:ser"))).map_err(pkg)?;
    write_val(w, "c:idx", &series.index.to_string())?;
    write_val(w, "c:order", &series.order.to_string())?;
    if let Some(name) = &series.name {
        write_series_name(w, name)?;
    }
    write_shape_properties(w, series.fill.as_ref(), series.line.as_ref())?;
    if let Some(labels) = &series.data_labels {
        write_data_labels(w, labels)?;
    }
    let scatter = matches!(kind, ChartGroupKind::Scatter { .. });
    if scatter {
        // `CT_ScatterSer`: xVal?, yVal?. The model keeps the x values in their own
        // field precisely so this is a field read and not a reinterpretation of
        // `categories`.
        if let Some(x_values) = &series.x_values {
            write_num_source(w, "c:xVal", x_values)?;
        }
        write_num_source(w, "c:yVal", &series.values)?;
    } else {
        if let Some(categories) = &series.categories {
            write_category_source(w, categories)?;
        }
        write_num_source(w, "c:val", &series.values)?;
    }
    // `c:smooth` is in `CT_LineSer` and `CT_ScatterSer` only. A bar, area or pie
    // series carrying `smooth` describes nothing those families draw, so it is
    // dropped rather than written somewhere the schema does not admit it.
    if series.smooth && (scatter || matches!(kind, ChartGroupKind::Line { .. })) {
        write_val(w, "c:smooth", "1")?;
    }
    w.write_event(Event::End(BytesEnd::new("c:ser")))
        .map_err(pkg)
}

/// `CT_SerTx` is `(strRef | v)` — unlike a title's `CT_Tx`, a series name has a
/// literal spelling, so a cached name with no formula is one `c:v`.
fn write_series_name(w: &mut Writer<Cursor<Vec<u8>>>, name: &ChartText) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:tx"))).map_err(pkg)?;
    match &name.formula {
        Some(formula) => write_str_ref(w, formula, std::slice::from_ref(&name.text))?,
        None => write_text_element(w, "c:v", &name.text)?,
    }
    w.write_event(Event::End(BytesEnd::new("c:tx")))
        .map_err(pkg)
}

/// `c:spPr` with the solid fill and the line the projection holds, or nothing
/// when it holds neither.
///
/// An empty `c:spPr` is legal but says "no explicit formatting", which is already
/// what its absence says, so it is not written: a writer that emits empty
/// elements makes every round-trip diff noisy and hides the real ones.
fn write_shape_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    fill: Option<&Color>,
    line: Option<&ChartLine>,
) -> Result<(), ExportError> {
    let fill_element = fill.filter(|color| !matches!(color, Color::Auto));
    let has_line =
        line.is_some_and(|line| line.no_fill || line.color.is_some() || line.width_emu.is_some());
    if fill_element.is_none() && !has_line {
        return Ok(());
    }
    w.write_event(Event::Start(start("c:spPr"))).map_err(pkg)?;
    if let Some(color) = fill_element {
        write_solid_fill(w, color)?;
    }
    if let Some(line) = line.filter(|_| has_line) {
        let mut element = start("a:ln");
        let width = line.width_emu.map(|emu| emu.to_string());
        if let Some(width) = &width {
            element.push_attribute(("w", width.as_str()));
        }
        if line.no_fill {
            w.write_event(Event::Start(element)).map_err(pkg)?;
            w.write_event(Event::Empty(start("a:noFill")))
                .map_err(pkg)?;
            w.write_event(Event::End(BytesEnd::new("a:ln")))
                .map_err(pkg)?;
        } else if let Some(color) = line.color.as_ref().filter(|c| !matches!(c, Color::Auto)) {
            w.write_event(Event::Start(element)).map_err(pkg)?;
            write_solid_fill(w, color)?;
            w.write_event(Event::End(BytesEnd::new("a:ln")))
                .map_err(pkg)?;
        } else {
            w.write_event(Event::Empty(element)).map_err(pkg)?;
        }
    }
    w.write_event(Event::End(BytesEnd::new("c:spPr")))
        .map_err(pkg)
}

/// `a:solidFill` wrapping one DrawingML colour.
///
/// [`Color::Auto`] is filtered out by the caller rather than mapped: DrawingML
/// has no "automatic" colour, and "automatic" means *the consumer resolves it*,
/// which is exactly what the absence of an explicit fill already says. Inventing
/// `tx1` for it would turn "let the theme decide" into "black", in the file.
fn write_solid_fill(w: &mut Writer<Cursor<Vec<u8>>>, color: &Color) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("a:solidFill")))
        .map_err(pkg)?;
    match color {
        Color::Rgb(rgb) => {
            let mut element = start("a:srgbClr");
            let value = format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b);
            element.push_attribute(("val", value.as_str()));
            w.write_event(Event::Empty(element)).map_err(pkg)?;
        }
        Color::Theme(theme) => write_scheme_color(w, theme)?,
        // Unreachable through the caller's filter; written as no child rather
        // than as a guess, so a future caller that forgets the filter produces a
        // theme-resolved fill instead of a wrong colour.
        Color::Auto => {}
    }
    w.write_event(Event::End(BytesEnd::new("a:solidFill")))
        .map_err(pkg)
}

/// `a:schemeClr` for a theme slot, with `a:tint`/`a:shade` when the model holds
/// one.
///
/// The model stores tint and shade as the WordprocessingML hex byte
/// (`w:themeTint`, `00..=FF`, "the fraction of the slot colour kept"); DrawingML
/// spells the same fraction as `ST_Percentage` in thousandths of a per cent. The
/// conversion is therefore `byte / 255 * 100000`, stated here because a reader
/// otherwise has to guess whether `7F` meant 127 or 50%.
fn write_scheme_color(
    w: &mut Writer<Cursor<Vec<u8>>>,
    theme: &ThemeColor,
) -> Result<(), ExportError> {
    let mut element = start("a:schemeClr");
    element.push_attribute(("val", scheme_token(theme.slot)));
    let tint = theme.theme_tint.map(percentage_of_byte);
    let shade = theme.theme_shade.map(percentage_of_byte);
    if tint.is_none() && shade.is_none() {
        return w.write_event(Event::Empty(element)).map_err(pkg);
    }
    w.write_event(Event::Start(element)).map_err(pkg)?;
    if let Some(tint) = tint {
        write_val(w, "a:tint", &tint)?;
    }
    if let Some(shade) = shade {
        write_val(w, "a:shade", &shade)?;
    }
    w.write_event(Event::End(BytesEnd::new("a:schemeClr")))
        .map_err(pkg)
}

/// A `w:themeTint`-style hex byte as a DrawingML `ST_Percentage`.
fn percentage_of_byte(byte: u8) -> String {
    (u32::from(byte) * 100_000 / 255).to_string()
}

/// The `a:schemeClr@val` token for a theme slot.
///
/// The reader accepts both spellings of the first four slots (`dk1`/`tx1`,
/// `lt1`/`bg1`, ...); the writer picks the `dk`/`lt` family because those are the
/// theme's own slot names, and a chart part is read against the theme rather than
/// against a text colour map.
const fn scheme_token(slot: ThemeColorRef) -> &'static str {
    match slot {
        ThemeColorRef::Dark1 => "dk1",
        ThemeColorRef::Light1 => "lt1",
        ThemeColorRef::Dark2 => "dk2",
        ThemeColorRef::Light2 => "lt2",
        ThemeColorRef::Accent1 => "accent1",
        ThemeColorRef::Accent2 => "accent2",
        ThemeColorRef::Accent3 => "accent3",
        ThemeColorRef::Accent4 => "accent4",
        ThemeColorRef::Accent5 => "accent5",
        ThemeColorRef::Accent6 => "accent6",
        ThemeColorRef::Hyperlink => "hlink",
        ThemeColorRef::FollowedHyperlink => "folHlink",
    }
}

/// `CT_DLbls` (the populated choice): `numFmt?`, `spPr?`, `txPr?`, `dLblPos?`,
/// `showLegendKey?`, `showVal?`, `showCatName?`, `showSerName?`, `showPercent?`,
/// `showBubbleSize?`, `separator?`, `showLeaderLines?`, `leaderLines?`.
///
/// Only the five flags and the position the model holds are written. The ones it
/// does not model (`showLegendKey`, `showBubbleSize`, `separator`) are left out
/// rather than written as `0`: `0` is a claim that the producer turned them off,
/// and the projection never knew either way.
fn write_data_labels(
    w: &mut Writer<Cursor<Vec<u8>>>,
    labels: &DataLabels,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:dLbls"))).map_err(pkg)?;
    if let Some(position) = labels.position {
        write_val(
            w,
            "c:dLblPos",
            match position {
                DataLabelPosition::BestFit => "bestFit",
                DataLabelPosition::Bottom => "b",
                DataLabelPosition::Center => "ctr",
                DataLabelPosition::InsideBase => "inBase",
                DataLabelPosition::InsideEnd => "inEnd",
                DataLabelPosition::Left => "l",
                DataLabelPosition::OutsideEnd => "outEnd",
                DataLabelPosition::Right => "r",
                DataLabelPosition::Top => "t",
            },
        )?;
    }
    write_val(w, "c:showVal", bool_val(labels.show_value))?;
    write_val(w, "c:showCatName", bool_val(labels.show_category_name))?;
    write_val(w, "c:showSerName", bool_val(labels.show_series_name))?;
    write_val(w, "c:showPercent", bool_val(labels.show_percent))?;
    w.write_event(Event::End(BytesEnd::new("c:dLbls")))
        .map_err(pkg)
}

/// `CT_Legend` sequence: `legendPos?`, `legendEntry*`, `layout?`, `overlay?`,
/// `spPr?`, `txPr?`.
fn write_legend(w: &mut Writer<Cursor<Vec<u8>>>, legend: &Legend) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:legend")))
        .map_err(pkg)?;
    write_val(
        w,
        "c:legendPos",
        match legend.position {
            LegendPosition::Bottom => "b",
            LegendPosition::Left => "l",
            LegendPosition::Right => "r",
            LegendPosition::Top => "t",
            LegendPosition::TopRight => "tr",
        },
    )?;
    write_val(w, "c:overlay", bool_val(legend.overlay))?;
    w.write_event(Event::End(BytesEnd::new("c:legend")))
        .map_err(pkg)
}

/// One axis, as `c:catAx`, `c:valAx` or `c:dateAx`.
///
/// Shared sequence prefix: `axId`, `scaling`, `delete?`, `axPos`,
/// `majorGridlines?`, `minorGridlines?`, `title?`, `numFmt?`, `majorTickMark?`,
/// `minorTickMark?`, `tickLblPos?`, `spPr?`, `txPr?`, **`crossAx`**. The three
/// axis types diverge only *after* `crossAx`, and this writer emits nothing
/// there, so one writer is correct for all three.
///
/// `axPos` and `crossAx` are REQUIRED by the schema while the model makes both
/// optional, so each has a stated fallback rather than being omitted — an axis
/// missing either makes Word repair the part and lose the chart.
fn write_axis(
    w: &mut Writer<Cursor<Vec<u8>>>,
    axis: &Axis,
    plot: &PlotArea,
) -> Result<(), ExportError> {
    let element = match axis.kind {
        AxisKind::Category => "c:catAx",
        AxisKind::Value => "c:valAx",
        AxisKind::Date => "c:dateAx",
    };
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    write_val(w, "c:axId", &axis.id.to_string())?;
    // `CT_Scaling` sequence: logBase?, orientation?, max?, min? — max BEFORE min,
    // which is the opposite of the order a reader expects and a common mistake.
    w.write_event(Event::Start(start("c:scaling")))
        .map_err(pkg)?;
    write_val(
        w,
        "c:orientation",
        match axis.orientation {
            AxisOrientation::MinMax => "minMax",
            AxisOrientation::MaxMin => "maxMin",
        },
    )?;
    if let Some(maximum) = &axis.maximum {
        write_val(w, "c:max", maximum)?;
    }
    if let Some(minimum) = &axis.minimum {
        write_val(w, "c:min", minimum)?;
    }
    w.write_event(Event::End(BytesEnd::new("c:scaling")))
        .map_err(pkg)?;
    write_val(w, "c:delete", bool_val(axis.deleted))?;
    // The fallback: a category axis sits at the bottom and a value axis at the
    // left, which is Word's own default pair and the only placement the rest of
    // the projection is consistent with.
    write_val(
        w,
        "c:axPos",
        match axis.position {
            Some(AxisPosition::Bottom) => "b",
            Some(AxisPosition::Left) => "l",
            Some(AxisPosition::Right) => "r",
            Some(AxisPosition::Top) => "t",
            None => match axis.kind {
                AxisKind::Value => "l",
                AxisKind::Category | AxisKind::Date => "b",
            },
        },
    )?;
    if axis.major_gridlines {
        w.write_event(Event::Empty(start("c:majorGridlines")))
            .map_err(pkg)?;
    }
    if axis.minor_gridlines {
        w.write_event(Event::Empty(start("c:minorGridlines")))
            .map_err(pkg)?;
    }
    if let Some(format) = &axis.number_format {
        let mut element = start("c:numFmt");
        let code = strip_xml_forbidden(format);
        element.push_attribute(("formatCode", &*code));
        // The code is the model's own, not inherited from a cell, which is what
        // `sourceLinked="0"` says. Omitting it lets Word prefer a linked format
        // and ignore the code beside it.
        element.push_attribute(("sourceLinked", "0"));
        w.write_event(Event::Empty(element)).map_err(pkg)?;
    }
    write_val(w, "c:majorTickMark", tick_mark_str(axis.major_tick_mark))?;
    write_val(w, "c:minorTickMark", tick_mark_str(axis.minor_tick_mark))?;
    write_val(
        w,
        "c:tickLblPos",
        match axis.tick_label_position {
            TickLabelPosition::NextTo => "nextTo",
            TickLabelPosition::High => "high",
            TickLabelPosition::Low => "low",
            TickLabelPosition::None => "none",
        },
    )?;
    // The fallback: the first OTHER axis in the plot area. A one-axis plot area
    // has nothing to cross, and `crossAx` is still required, so it names itself —
    // which is what Word writes for a lone axis and keeps the part readable
    // rather than repaired.
    let cross = axis.cross_axis_id.unwrap_or_else(|| {
        plot.axes
            .iter()
            .find(|other| other.id != axis.id)
            .map_or(axis.id, |other| other.id)
    });
    write_val(w, "c:crossAx", &cross.to_string())?;
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)
}

/// The `c:majorTickMark`/`c:minorTickMark` token.
const fn tick_mark_str(mark: TickMark) -> &'static str {
    match mark {
        TickMark::None => "none",
        TickMark::Inside => "in",
        TickMark::Outside => "out",
        TickMark::Cross => "cross",
    }
}

/// A `CT_NumDataSource` (`c:val`, `c:xVal`, `c:yVal`): `numRef` when the range
/// carries a formula, `numLit` when it does not.
///
/// The choice is the formula's presence and nothing else. A `numRef` without a
/// `c:f` is invalid, and a `numLit` is exactly how a chart with no workbook
/// behind it spells its data — which is the case for every chart this editor
/// mints, because the cache IS the data (`docs/155` §5.2).
fn write_num_source(
    w: &mut Writer<Cursor<Vec<u8>>>,
    element: &str,
    range: &DataRange,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    match &range.formula {
        Some(formula) => {
            w.write_event(Event::Start(start("c:numRef")))
                .map_err(pkg)?;
            write_text_element(w, "c:f", formula)?;
            write_num_data(w, "c:numCache", range)?;
            w.write_event(Event::End(BytesEnd::new("c:numRef")))
                .map_err(pkg)?;
        }
        None => write_num_data(w, "c:numLit", range)?,
    }
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)
}

/// A `CT_AxDataSource` (`c:cat`): the numeric or the string spelling, chosen from
/// what the cache actually holds.
///
/// A category cache of numbers must be a `numRef`/`numLit` or Word reads the
/// labels as text and loses the axis' numeric ordering; a cache of labels must be
/// a `strRef`/`strLit` or the part is invalid. Mixed or empty caches take the
/// string spelling, which can carry either.
fn write_category_source(
    w: &mut Writer<Cursor<Vec<u8>>>,
    range: &DataRange,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:cat"))).map_err(pkg)?;
    let numeric = !range.points.is_empty()
        && range
            .points
            .iter()
            .all(|(_, value)| matches!(value, ChartValue::Number(_) | ChartValue::Blank));
    match (&range.formula, numeric) {
        (Some(formula), true) => {
            w.write_event(Event::Start(start("c:numRef")))
                .map_err(pkg)?;
            write_text_element(w, "c:f", formula)?;
            write_num_data(w, "c:numCache", range)?;
            w.write_event(Event::End(BytesEnd::new("c:numRef")))
                .map_err(pkg)?;
        }
        (Some(formula), false) => {
            w.write_event(Event::Start(start("c:strRef")))
                .map_err(pkg)?;
            write_text_element(w, "c:f", formula)?;
            write_str_data(w, "c:strCache", range)?;
            w.write_event(Event::End(BytesEnd::new("c:strRef")))
                .map_err(pkg)?;
        }
        (None, true) => write_num_data(w, "c:numLit", range)?,
        (None, false) => write_str_data(w, "c:strLit", range)?,
    }
    w.write_event(Event::End(BytesEnd::new("c:cat")))
        .map_err(pkg)
}

/// `CT_NumData`/`CT_NumLit`: `formatCode?`, `ptCount?`, `pt*`.
fn write_num_data(
    w: &mut Writer<Cursor<Vec<u8>>>,
    element: &str,
    range: &DataRange,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    if let Some(format) = &range.number_format {
        write_text_element(w, "c:formatCode", format)?;
    }
    write_val(w, "c:ptCount", &range.point_count.to_string())?;
    write_points(w, range)?;
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)
}

/// `CT_StrData`/`CT_StrLit`: `ptCount?`, `pt*`.
///
/// No `formatCode`: the string shapes have no such child in ECMA-376, so a
/// category range's `number_format` is dropped here. It describes the formatting
/// of a NUMBER, so there is nothing for it to format in a label cache — this is
/// the one field the writer deliberately does not round-trip, and it is
/// unreachable from the editor because a minted chart sets no format code.
fn write_str_data(
    w: &mut Writer<Cursor<Vec<u8>>>,
    element: &str,
    range: &DataRange,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(element))).map_err(pkg)?;
    write_val(w, "c:ptCount", &range.point_count.to_string())?;
    write_points(w, range)?;
    w.write_event(Event::End(BytesEnd::new(element)))
        .map_err(pkg)
}

/// The `c:pt` children of a cache, in `idx` order.
///
/// # Two different kinds of hole, and why both are written as themselves
///
/// A cache can be sparse two ways, the model distinguishes them, so the writer
/// has to as well:
///
/// * An **absent index** — no `(idx, _)` entry at all — writes no `c:pt`.
///   `c:ptCount` still declares the length, which is how a sparse cache is
///   spelled and how Word writes a chart over a range with empty cells.
/// * An explicit [`ChartValue::Blank`] — the producer wrote a `c:pt` whose value
///   was empty — writes `<c:pt idx="N"><c:v></c:v></c:pt>`, the spelling the
///   reader derived `Blank` FROM. `CT_NumVal`'s `c:v` is `ST_Xstring` (an
///   optional string), not `xsd:double`, so an empty one is schema-valid and
///   Word reads it as a blank point.
///
/// Collapsing the second into the first would lose the difference on every write:
/// the reader cannot tell an absent index from an empty `c:v` after the fact, so
/// a declared blank would come back as a hole and the reopened model would differ
/// from the one that was written. The distinction costs one branch and buys an
/// exact round trip.
fn write_points(w: &mut Writer<Cursor<Vec<u8>>>, range: &DataRange) -> Result<(), ExportError> {
    for (index, value) in &range.points {
        let text = match value {
            ChartValue::Number(number) => number.as_str(),
            ChartValue::Text(text) => text.as_str(),
            ChartValue::Blank => "",
        };
        let mut element = start("c:pt");
        element.push_attribute(("idx", index.to_string().as_str()));
        w.write_event(Event::Start(element)).map_err(pkg)?;
        write_text_element(w, "c:v", text)?;
        w.write_event(Event::End(BytesEnd::new("c:pt")))
            .map_err(pkg)?;
    }
    Ok(())
}

/// A `c:strRef`: the formula plus a one-cell `c:strCache` holding `values`.
fn write_str_ref(
    w: &mut Writer<Cursor<Vec<u8>>>,
    formula: &str,
    values: &[String],
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:strRef")))
        .map_err(pkg)?;
    write_text_element(w, "c:f", formula)?;
    w.write_event(Event::Start(start("c:strCache")))
        .map_err(pkg)?;
    write_val(w, "c:ptCount", &values.len().to_string())?;
    for (index, value) in values.iter().enumerate() {
        let mut element = start("c:pt");
        element.push_attribute(("idx", index.to_string().as_str()));
        w.write_event(Event::Start(element)).map_err(pkg)?;
        write_text_element(w, "c:v", value)?;
        w.write_event(Event::End(BytesEnd::new("c:pt")))
            .map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new("c:strCache")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("c:strRef")))
        .map_err(pkg)
}

/// `c:externalData` — a relationship id and nothing else. The workbook's BYTES
/// are never opened and never written from here (`docs/45` I4): they live in the
/// retained-parts side table, and [`chart_part_rels`] only names them when the
/// package will contain them.
fn write_external_data(
    w: &mut Writer<Cursor<Vec<u8>>>,
    external: &EmbeddedPart,
) -> Result<(), ExportError> {
    let mut element = start("c:externalData");
    element.push_attribute(("r:id", external.relationship_id.as_str()));
    w.write_event(Event::Start(element)).map_err(pkg)?;
    // `CT_ExternalData` sequence: autoUpdate?. `0` because nothing here
    // recalculates a workbook — the cache is the rendering truth.
    write_val(w, "c:autoUpdate", "0")?;
    w.write_event(Event::End(BytesEnd::new("c:externalData")))
        .map_err(pkg)
}

/// `<name val="value"/>` — the `CT_*` "one attribute" shape most chart elements
/// take.
fn write_val(w: &mut Writer<Cursor<Vec<u8>>>, name: &str, value: &str) -> Result<(), ExportError> {
    let mut element = start(name);
    element.push_attribute(("val", value));
    w.write_event(Event::Empty(element)).map_err(pkg)
}

/// `<name>text</name>`, with characters XML cannot represent removed.
///
/// The same defence `write_run_properties` applies to body text, for the same
/// reason: one forbidden character makes the whole PART unreadable, and the blast
/// radius is the chart, not the string.
fn write_text_element(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    text: &str,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start(name))).map_err(pkg)?;
    w.write_event(Event::Text(BytesText::new(&strip_xml_forbidden(text))))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new(name))).map_err(pkg)
}

/// `"1"`/`"0"` — the `ST_Boolean` spelling Word writes.
const fn bool_val(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}
