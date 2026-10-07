//! A chart the EDITOR minted survives a save (`109` HF-256).
//!
//! # What was broken, measured
//!
//! Every `word/charts/...` write in `casual-doc-export` was inside a
//! `#[cfg(test)]` module. Import kept an imported chart's source bytes in the
//! retained-parts side table and the exporter re-emitted those bytes, so an
//! imported chart round-tripped. A chart `insertChart` minted had no source
//! bytes and no generator, so the package carried the drawing's `c:chart r:id`
//! and the matching `document.xml.rels` entry and **no part at all** — a live
//! relationship pointing at nothing, which Word reports as an unreadable file.
//!
//! # What these guards assert, and what they deliberately do not
//!
//! The guarantee is a *round trip through the package*, not a string match on
//! XML: every case here writes the package, reopens it with the importer, and
//! compares the **reconstructed projection** with the one that went in. That is
//! the assertion that cannot pass while an element is in the wrong place, named
//! wrongly, or missing — Word repairs an out-of-order chart part by discarding
//! it, and so does the reader.
//!
//! Three facts are checked outside the projection because the projection cannot
//! see them: the part is in the ZIP, `[Content_Types].xml` declares it (without
//! the `Override` the part reads as generic `application/xml`), and the chart
//! relationship is emitted exactly once rather than once per writer.

use std::collections::BTreeMap;

use casual_doc_export::{export_document, write_document, write_document_with_retained_parts};
use casual_doc_import::{ImportConfig, ImportMode, RetainedPart, RetainedParts, import_package};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Axis, AxisKind, AxisPosition, BarDirection, BarGrouping, BlockNode, Chart, ChartCoverage,
    ChartGroup, ChartGroupKind, ChartId, ChartLine, ChartText, ChartTitle, ChartValue, Color,
    DataLabelPosition, DataLabels, DataRange, Definitions, DisplayBlanks, EmbeddedKind,
    EmbeddedObject, EmbeddedPart, Extent, Grouping, InlineNode, Legend, LegendPosition, Paragraph,
    ParagraphProperties, PlotArea, RgbColor, ScatterStyle, Series, ThemeColor, ThemeColorRef,
    TickLabelPosition, TickMark,
};
// Own `use` line: `Document` is the v1 one, not the crate-root re-export.
use casual_doc_model::v1::Document;
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CHART_PART: &str = "word/charts/chart7.xml";
const CHART_CT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const PACKAGE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

fn id(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).expect("a test node id")
}

/// The object the projection anchors to: the same shape `insertChart` mints,
/// including the part name and relationship id it derives from the object's own
/// identity.
fn chart_object(object: NodeId) -> EmbeddedObject {
    EmbeddedObject {
        id: object,
        kind: EmbeddedKind::Chart,
        part: EmbeddedPart {
            relationship_id: "rIdChart7".to_owned(),
            relationship_type: CHART_REL_TYPE.to_owned(),
            part_name: CHART_PART.to_owned(),
        },
        extra_parts: Vec::new(),
        preview: None,
        extent: Extent {
            width_emu: 4_572_000,
            height_emu: 2_743_200,
        },
        prog_id: None,
    }
}

/// A document holding one chart object in a body paragraph plus `projection` in
/// `Definitions::charts`, exactly as `insertChart`'s two operations leave it.
fn document_with(projection: Chart) -> Document {
    let object = projection.object;
    let mut definitions = Definitions::default();
    definitions.charts.insert(ChartId::new(id(4)), projection);
    Document::new(
        id(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: id(2),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::EmbeddedObject(Box::new(chart_object(object)))],
        })],
        definitions,
    )
    .expect("a chart object plus its projection is a valid document")
}

/// Four cached numbers with every index present — the `numLit` shape a chart
/// with no workbook behind it takes.
fn numbers(values: &[&str]) -> DataRange {
    DataRange {
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

/// Four cached labels — the `strLit` shape.
fn labels(values: &[&str]) -> DataRange {
    DataRange {
        formula: None,
        point_count: values.len() as u32,
        points: values
            .iter()
            .enumerate()
            .map(|(index, value)| (index as u32, ChartValue::Text((*value).to_owned())))
            .collect(),
        number_format: None,
    }
}

/// A category axis and a value axis, which is the pair every non-pie family
/// plots against.
fn axes() -> Vec<Axis> {
    vec![
        Axis {
            id: 1,
            kind: AxisKind::Category,
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
    ]
}

/// A projection of `group` with one labelled, cached series.
fn projection(object: NodeId, group: ChartGroupKind) -> Chart {
    let pie = matches!(
        group,
        ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. }
    );
    let scatter = matches!(group, ChartGroupKind::Scatter { .. });
    Chart {
        object,
        coverage: ChartCoverage::Complete,
        title: None,
        auto_title_deleted: true,
        plot_area: PlotArea {
            groups: vec![ChartGroup {
                kind: group,
                series: vec![Series {
                    index: 0,
                    order: 0,
                    name: Some(ChartText {
                        text: "Series 1".to_owned(),
                        formula: None,
                    }),
                    categories: if scatter {
                        None
                    } else {
                        Some(labels(&["1st Qtr", "2nd Qtr", "3rd Qtr", "4th Qtr"]))
                    },
                    values: numbers(&["4.3", "2.5", "3.50", "4.5"]),
                    x_values: if scatter {
                        Some(numbers(&["0.7", "1.8", "2.6", "3.1"]))
                    } else {
                        None
                    },
                    ..Series::default()
                }],
                axis_ids: if pie { Vec::new() } else { vec![1, 2] },
                vary_colors: pie,
            }],
            axes: if pie { Vec::new() } else { axes() },
        },
        legend: Some(Legend {
            position: LegendPosition::Bottom,
            overlay: false,
        }),
        plot_visible_only: true,
        display_blanks_as: DisplayBlanks::Gap,
        vary_colors: false,
        external_data: None,
        dirty: false,
    }
}

/// Writes `document` as a package and reads it back as a model.
fn round_trip(document: &Document) -> casual_doc_import::Import {
    let written = write_document(document, &BTreeMap::new()).expect("the document writes");
    reopen(&written)
}

/// Reads a written package back as a model.
fn reopen(written: &[u8]) -> casual_doc_import::Import {
    let mut package =
        DocxPackage::open(written, PackageLimits::default()).expect("the written package opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        },
    )
    .expect("the written package reopens")
}

/// The reopened document's one chart projection.
fn reopened_projection(import: &casual_doc_import::Import) -> Chart {
    let charts: Vec<&Chart> = import
        .document
        .definitions()
        .charts
        .iter()
        .map(|(_, chart)| chart)
        .collect();
    assert_eq!(
        charts.len(),
        1,
        "the reopened document must hold exactly one chart projection"
    );
    charts[0].clone()
}

/// The reopened document's one chart object.
fn reopened_object(import: &casual_doc_import::Import) -> EmbeddedObject {
    import
        .document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(&paragraph.inlines),
            _ => None,
        })
        .flatten()
        .find_map(|inline| match inline {
            InlineNode::EmbeddedObject(object) => Some((**object).clone()),
            _ => None,
        })
        .expect("the written chart reopens as an embedded object")
}

/// `chart` with the workbook binding the writer adds taken back off: the `c:f`
/// formulas into `Sheet1` and the `c:externalData` pointer.
///
/// The writer binds every chart it regenerates to a values-only workbook so
/// Word's *Edit Data* works (`chart_workbook`), so a written-and-reopened chart
/// carries those references and the one that went in did not. Everything ELSE
/// must come back unchanged, which is what comparing through this asserts —
/// and [`assert_bound_to_a_workbook`] asserts the binding itself, so taking it
/// off here cannot hide its absence.
fn unbound(mut chart: Chart) -> Chart {
    chart.external_data = None;
    for group in &mut chart.plot_area.groups {
        for series in &mut group.series {
            series.values.formula = None;
            if let Some(range) = series.categories.as_mut() {
                range.formula = None;
            }
            if let Some(range) = series.x_values.as_mut() {
                range.formula = None;
            }
            if let Some(name) = series.name.as_mut() {
                name.formula = None;
            }
        }
    }
    chart
}

/// The reopened chart names a workbook the package contains, and its first
/// series reads its numbers from column B of that workbook's `Sheet1`.
fn assert_bound_to_a_workbook(written: &[u8], after: &Chart, rows: usize) {
    let external = after
        .external_data
        .as_ref()
        .expect("a written chart must name its embedded workbook, or Word's Edit Data fails");
    let mut package =
        DocxPackage::open(written, PackageLimits::default()).expect("the package opens");
    let bytes = package
        .read_part(&external.part_name)
        .unwrap_or_else(|_| panic!("the workbook {} is not in the package", external.part_name));
    assert!(
        bytes.starts_with(b"PK"),
        "the workbook part is not a ZIP package"
    );
    assert_eq!(
        package.content_type(&external.part_name),
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    );
    let series = &after.plot_area.groups[0].series[0];
    assert_eq!(
        series.values.formula.as_deref(),
        Some(format!("Sheet1!$B$2:$B${}", rows + 1).as_str()),
        "the series must read its values from the workbook's column B"
    );
}

/// The cells of `Sheet1` in a workbook, as `(reference, text)` in sheet order.
fn workbook_cells(workbook: &[u8]) -> Vec<(String, String)> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(workbook)).expect("the workbook is a ZIP");
    let mut sheet = String::new();
    std::io::Read::read_to_string(
        &mut archive
            .by_name("xl/worksheets/sheet1.xml")
            .expect("the workbook has a sheet"),
        &mut sheet,
    )
    .expect("the sheet is UTF-8");
    let mut cells = Vec::new();
    for chunk in sheet.split("<c r=\"").skip(1) {
        let reference = chunk.split('"').next().unwrap_or_default().to_owned();
        let text = chunk
            .split_once("<v>")
            .or_else(|| chunk.split_once("<t xml:space=\"preserve\">"))
            .and_then(|(_, rest)| rest.split('<').next())
            .unwrap_or_default()
            .to_owned();
        cells.push((reference, text));
    }
    cells
}

/// Every chart family the model can express, written and read back.
///
/// Parameterised rather than six near-identical tests because the thing under
/// test is the same: each `CT_*Chart` sequence differs, and the projection that
/// comes back out is what proves the sequence was right. The list is exhaustive
/// over `ChartGroupKind` — a seventh variant stops compiling here, which is the
/// point.
#[test]
fn every_modeled_chart_family_writes_a_part_that_reads_back_as_the_same_chart() {
    let families = [
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
        ChartGroupKind::Bar {
            direction: BarDirection::Bar,
            grouping: BarGrouping::Stacked,
            gap_width: 182,
            overlap: 100,
        },
        ChartGroupKind::Line {
            grouping: Grouping::Standard,
            marker: true,
        },
        ChartGroupKind::Area {
            grouping: Grouping::Stacked,
        },
        ChartGroupKind::Pie {
            first_slice_angle: 90,
        },
        ChartGroupKind::Doughnut {
            first_slice_angle: 0,
            hole_size: 75,
        },
        ChartGroupKind::Scatter {
            style: ScatterStyle::LineMarker,
        },
    ];
    for group in families {
        let before = projection(id(3), group);
        let document = document_with(before.clone());
        let written = write_document(&document, &BTreeMap::new()).expect("the document writes");
        let import = reopen(&written);
        let after = reopened_projection(&import);
        assert_bound_to_a_workbook(&written, &after, 4);
        assert_eq!(
            unbound(after.clone()),
            before,
            "a {group:?} chart did not survive the package round trip"
        );
        // A chart this editor wrote must come back fully modelled, or reopening
        // the file would make it read-only — a static chart one save later.
        assert_eq!(after.coverage, ChartCoverage::Complete, "{group:?}");
        assert_eq!(
            reopened_object(&import).kind,
            EmbeddedKind::Chart,
            "the {group:?} chart's drawing must reopen as a chart object"
        );
    }
}

/// The three package-level facts the projection cannot see.
#[test]
fn the_generated_chart_part_is_in_the_package_declared_and_referenced_once() {
    let document = document_with(projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    ));
    let written = write_document(&document, &BTreeMap::new()).expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");

    // 1. The part exists. Before HF-256 this read was the failure: the
    //    relationship was written and the part was not.
    let part = package
        .read_part(CHART_PART)
        .unwrap_or_else(|_| panic!("{CHART_PART} must be in the package"));
    let xml = String::from_utf8(part).expect("the chart part is UTF-8");
    assert!(
        xml.starts_with("<c:chartSpace"),
        "the generated part must be a chart space, got: {}",
        &xml[..xml.len().min(80)]
    );

    // 2. The content type is declared. Without the `Override` the part falls
    //    back to the `.xml` `Default` (`application/xml`) and Word refuses the
    //    package.
    assert_eq!(
        package.content_type(CHART_PART),
        Some(CHART_CT),
        "the generated chart part must declare the chart content type"
    );

    // 3. One relationship, with the node's own verbatim id — not one from the
    //    node plus one from the part generator.
    let rels = String::from_utf8(
        package
            .read_part("word/_rels/document.xml.rels")
            .expect("the document rels part reads"),
    )
    .expect("the rels part is UTF-8");
    assert_eq!(
        rels.matches("charts/chart7.xml").count(),
        1,
        "the chart relationship must be emitted exactly once: {rels}"
    );
    assert!(
        rels.contains(r#"Id="rIdChart7""#),
        "the relationship must carry the node's own id: {rels}"
    );
    assert!(
        rels.contains(CHART_REL_TYPE),
        "the relationship must carry the chart relationship type: {rels}"
    );
}

/// Formatting, labels, a title, axis bounds and a scaled axis all reach the file
/// and come back.
///
/// Separate from the family sweep because the family sweep varies the group and
/// holds everything else fixed; this varies everything else. Together they cover
/// the two axes of the writer, and neither alone would catch a `c:spPr` written
/// after `c:cat` (which the schema forbids and Word repairs away).
#[test]
fn a_fully_dressed_chart_round_trips_its_formatting_labels_and_axis_bounds() {
    let mut before = projection(
        id(3),
        ChartGroupKind::Line {
            grouping: Grouping::Standard,
            marker: false,
        },
    );
    before.title = Some(ChartTitle {
        text: Some(ChartText {
            text: "Quarterly revenue".to_owned(),
            formula: None,
        }),
        overlay: true,
    });
    before.auto_title_deleted = false;
    before.plot_visible_only = false;
    before.display_blanks_as = DisplayBlanks::Span;
    before.legend = Some(Legend {
        position: LegendPosition::TopRight,
        overlay: true,
    });
    let series = &mut before.plot_area.groups[0].series[0];
    series.fill = Some(Color::Rgb(RgbColor {
        r: 0x33,
        g: 0x55,
        b: 0xC4,
    }));
    series.line = Some(ChartLine {
        color: Some(Color::Theme(ThemeColor {
            slot: ThemeColorRef::Accent3,
            theme_tint: None,
            theme_shade: None,
        })),
        width_emu: Some(28_575),
        no_fill: false,
    });
    series.smooth = true;
    series.data_labels = Some(DataLabels {
        show_value: true,
        show_category_name: false,
        show_series_name: true,
        show_percent: false,
        position: Some(DataLabelPosition::OutsideEnd),
    });
    series.values.number_format = Some("#,##0.00".to_owned());
    before.plot_area.axes[1].minimum = Some("0".to_owned());
    before.plot_area.axes[1].maximum = Some("5.0".to_owned());
    before.plot_area.axes[1].number_format = Some("0.0%".to_owned());
    before.plot_area.axes[1].major_tick_mark = TickMark::Cross;
    before.plot_area.axes[1].minor_tick_mark = TickMark::Inside;
    before.plot_area.axes[1].tick_label_position = TickLabelPosition::Low;
    before.plot_area.axes[0].deleted = true;

    let import = round_trip(&document_with(before.clone()));
    assert_eq!(
        unbound(reopened_projection(&import)),
        before,
        "a dressed chart lost something on the way through the package"
    );
}

/// A cached number keeps the spelling it was written with.
///
/// `3.50` and `3.5` are the same number and different documents (`docs/155`
/// §8.2), which is why the model holds a cached value as text. A writer that
/// formatted through an `f64` would pass every structural assertion above and
/// fail this one.
#[test]
fn a_cached_number_keeps_its_lexical_form_through_the_writer() {
    let document = document_with(projection(
        id(3),
        ChartGroupKind::Area {
            grouping: Grouping::Standard,
        },
    ));
    let written = write_document(&document, &BTreeMap::new()).expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    let xml = String::from_utf8(package.read_part(CHART_PART).expect("the chart part reads"))
        .expect("the chart part is UTF-8");
    assert!(
        xml.contains("<c:v>3.50</c:v>"),
        "the cached value lost its source spelling: {xml}"
    );
}

/// A sparse cache keeps BOTH kinds of hole: an absent index stays absent, and a
/// declared [`ChartValue::Blank`] stays a declared blank.
///
/// These are different documents, and the reader cannot tell them apart after
/// the fact, so a writer that collapsed one into the other would lose the
/// difference on every write while every structural assertion still passed.
/// `c:ptCount` must survive independently of how many points were written,
/// because that is what makes a sparse cache readable at all.
#[test]
fn a_sparse_cache_keeps_its_absent_indices_and_its_declared_blanks() {
    let mut before = projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    );
    let points = vec![
        (0, ChartValue::Number("1".to_owned())),
        // Index 1 is ABSENT: no entry at all.
        (2, ChartValue::Blank),
        (3, ChartValue::Number("3".to_owned())),
    ];
    before.plot_area.groups[0].series[0].values = DataRange {
        formula: None,
        point_count: 6,
        points: points.clone(),
        number_format: None,
    };
    let import = round_trip(&document_with(before.clone()));
    let after = reopened_projection(&import);
    let values = &after.plot_area.groups[0].series[0].values;
    assert_eq!(
        values.point_count, 6,
        "the declared length must survive independently of the points written"
    );
    assert_eq!(
        values.points, points,
        "a sparse cache must come back with its own indices, absences and blanks"
    );
}

/// A range that names a formula writes the REF shape, not the literal one, and
/// the formula is carried verbatim.
#[test]
fn a_range_with_a_formula_writes_a_ref_and_carries_the_formula_verbatim() {
    let mut before = projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    );
    {
        let series = &mut before.plot_area.groups[0].series[0];
        series.values.formula = Some("Sheet1!$B$2:$B$5".to_owned());
        series.categories.as_mut().expect("categories").formula =
            Some("Sheet1!$A$2:$A$5".to_owned());
        series.name = Some(ChartText {
            text: "Revenue".to_owned(),
            formula: Some("Sheet1!$B$1".to_owned()),
        });
    }
    let written = write_document(&document_with(before.clone()), &BTreeMap::new())
        .expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    let xml = String::from_utf8(package.read_part(CHART_PART).expect("the chart part reads"))
        .expect("the chart part is UTF-8");
    assert!(
        xml.contains("<c:numRef><c:f>Sheet1!$B$2:$B$5</c:f>"),
        "a numeric range with a formula must be a numRef: {xml}"
    );
    assert!(
        xml.contains("<c:strRef><c:f>Sheet1!$A$2:$A$5</c:f>"),
        "a label range with a formula must be a strRef: {xml}"
    );
    assert!(
        !xml.contains("<c:numLit>") && !xml.contains("<c:strLit>"),
        "nothing should be written as a literal once every range has a formula: {xml}"
    );
    let import = reopen(&written);
    assert_eq!(
        reopened_projection(&import),
        before,
        "the verbatim formulas must come back unchanged"
    );
}

/// A `Partial` projection is NOT regenerated, and the refusal is reported.
///
/// This is the coverage gate, which is the whole safety argument of the writer:
/// a projection the reader could only partly represent would lose whatever it did
/// not model if it were written back, and the file would look fine. The guard
/// asserts both halves — no part, and a finding that says why — because a silent
/// refusal is the same defect in the other direction.
#[test]
fn a_partial_projection_is_not_regenerated_and_the_refusal_is_reported() {
    let mut projection = projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    );
    projection.coverage = ChartCoverage::Partial;
    assert!(
        !projection.coverage.permits_regeneration(),
        "the gate under test must actually refuse"
    );
    let export = export_document(&document_with(projection), &BTreeMap::new())
        .expect("the document still writes");
    let mut package =
        DocxPackage::open(&export.bytes, PackageLimits::default()).expect("the package opens");
    assert!(
        package.read_part(CHART_PART).is_err(),
        "a partial projection must not be written back as a part"
    );
    let ids: Vec<&str> = export
        .report
        .entries
        .iter()
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(
        ids.contains(&"docx.export.chart.partial_coverage_not_regenerated"),
        "the refusal must be reported, not silent: {ids:?}"
    );
}

/// An imported chart is still copied, never regenerated.
///
/// The writer must not compete with retention: the source bytes are the
/// authority and the projection is a read index over them, so a chart whose part
/// IS retained comes out byte-identical even though a projection for it exists
/// and would be writable. Without this the new generator would quietly replace
/// every imported chart with a lossy rewrite of itself.
#[test]
fn a_retained_chart_part_is_copied_verbatim_and_not_regenerated() {
    let content_types = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#;
    let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="304800"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
    let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;
    // A chart the reader CAN fully project (one bar group, one cached series), so
    // the retained-wins rule is tested where it actually bites: coverage is
    // `Complete`, so without the retention check the generator would rewrite it.
    let chart = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>4.30</c:v></c:pt></c:numLit></c:val></c:ser><c:gapWidth val="150"/><c:overlap val="-27"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx></c:plotArea></c:chart></c:chartSpace>"#;
    let source = zip_package(&[
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", doc_rels.as_slice()),
        ("word/charts/chart1.xml", chart.as_slice()),
    ]);
    let mut source_package =
        DocxPackage::open(&source, PackageLimits::default()).expect("the source package opens");
    let import = import_package(
        &mut source_package,
        ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        },
    )
    .expect("the source package imports");
    assert!(
        !import.document.definitions().charts.is_empty(),
        "the fixture must project, or this test proves nothing about the race"
    );
    let written = write_document_with_retained_parts(
        &import.document,
        &BTreeMap::new(),
        &import.retained_parts,
    )
    .expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    assert_eq!(
        package
            .read_part("word/charts/chart1.xml")
            .expect("the retained chart part reads"),
        chart.to_vec(),
        "a retained chart part must come back byte-identical, not regenerated"
    );
}

/// The chart part's OWN `_rels` is written when — and only when — the workbook it
/// names is in the package.
///
/// A chart's embedded workbook hangs off `word/charts/_rels/chartN.xml.rels`, not
/// the document's, so the writer has to emit that part itself. The second half is
/// the point: naming a workbook the package does not contain is HF-256 one level
/// down, so the `r:id` is dropped with it.
#[test]
fn the_chart_parts_own_rels_is_written_only_when_the_workbook_is_there() {
    let workbook = EmbeddedPart {
        relationship_id: "rId1".to_owned(),
        relationship_type: PACKAGE_REL_TYPE.to_owned(),
        part_name: "word/embeddings/Book1.xlsx".to_owned(),
    };
    let mut with_workbook = projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    );
    with_workbook.external_data = Some(workbook.clone());
    // A formula into a workbook this export did not write, so the writer cannot
    // bind its own values-only one (`chart_workbook::bind` declines) and the
    // pointer is left exactly as the model has it.
    let mut foreign = with_workbook.clone();
    foreign.plot_area.groups[0].series[0].values.formula = Some("Data!$C$2:$C$5".to_owned());

    // No retained workbook part: no rels part, and no `c:externalData` pointing
    // at one.
    let written = write_document(&document_with(foreign.clone()), &BTreeMap::new())
        .expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    assert!(
        package
            .read_part("word/charts/_rels/chart7.xml.rels")
            .is_err(),
        "no rels part may be written for a workbook the package does not contain"
    );

    // The workbook IS in the package: the rels part is written, and its target is
    // relative to `word/charts/` — `../embeddings/Book1.xlsx`, not the
    // `word/`-relative form `document.xml.rels` uses. Getting that wrong points
    // Word at `word/charts/embeddings/Book1.xlsx`, which is nowhere.
    let retained = RetainedParts {
        parts: vec![RetainedPart {
            part_name: workbook.part_name.clone(),
            content_type: Some(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_owned(),
            ),
            bytes: b"not a real workbook, and never opened".to_vec(),
            rels: None,
        }],
        relationships: Vec::new(),
    };
    let written =
        write_document_with_retained_parts(&document_with(foreign), &BTreeMap::new(), &retained)
            .expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    let rels = String::from_utf8(
        package
            .read_part("word/charts/_rels/chart7.xml.rels")
            .expect("the chart's own rels part must be written"),
    )
    .expect("the rels part is UTF-8");
    assert!(
        rels.contains(r#"Target="../embeddings/Book1.xlsx""#),
        "the workbook target must be relative to the chart part: {rels}"
    );
    assert!(
        rels.contains(r#"Id="rId1""#) && rels.contains(PACKAGE_REL_TYPE),
        "the workbook relationship must keep its id and type: {rels}"
    );
    let chart_xml = String::from_utf8(package.read_part(CHART_PART).expect("the chart part reads"))
        .expect("the chart part is UTF-8");
    assert!(
        chart_xml.contains(r#"<c:externalData r:id="rId1">"#),
        "the chart must name the workbook it now has: {chart_xml}"
    );
}

/// The ECMA-376 child sequence of every complex type this writer emits.
///
/// `(parent local name, children in schema order)`. A chart part whose children
/// are out of sequence is invalid, and Word's repair for an invalid chart part is
/// to DISCARD the chart — so an element in the wrong place is silent data loss
/// behind a file that opens. The reader is scope-based and order-insensitive, so
/// the round-trip guards above cannot see this class at all; that is exactly why
/// this one exists rather than being assumed from them.
///
/// `c:cat`/`c:val`/`c:xVal`/`c:yVal` carry a CHOICE rather than a sequence, so
/// their rows list the alternatives: there is no order to get wrong inside one,
/// and what the row checks is that the child is one the schema admits there at
/// all — a `c:strLit` under `c:val` is invalid however it is ordered.
const SCHEMA_ORDER: &[(&str, &[&str])] = &[
    // CT_ChartSpace
    (
        "chartSpace",
        &[
            "date1904",
            "lang",
            "roundedCorners",
            "style",
            "clrMapOvr",
            "pivotSource",
            "protection",
            "chart",
            "spPr",
            "txPr",
            "externalData",
            "printSettings",
            "userShapes",
            "extLst",
        ],
    ),
    // CT_Chart
    (
        "chart",
        &[
            "title",
            "autoTitleDeleted",
            "pivotFmts",
            "view3D",
            "floor",
            "sideWall",
            "backWall",
            "plotArea",
            "legend",
            "plotVisOnly",
            "dispBlanksAs",
            "showDLblsOverMax",
            "extLst",
        ],
    ),
    // CT_Title
    ("title", &["tx", "layout", "overlay", "spPr", "txPr"]),
    // CT_Tx (rich choice) / CT_TextBody, and the one run it holds.
    ("rich", &["bodyPr", "lstStyle", "p"]),
    ("p", &["pPr", "r", "endParaRPr"]),
    ("r", &["rPr", "t"]),
    // CT_PlotArea — groups, then axes, then the tail.
    (
        "plotArea",
        &[
            "layout",
            "barChart",
            "lineChart",
            "areaChart",
            "pieChart",
            "doughnutChart",
            "scatterChart",
            "catAx",
            "valAx",
            "dateAx",
            "dTable",
            "spPr",
        ],
    ),
    // CT_BarChart
    (
        "barChart",
        &[
            "barDir",
            "grouping",
            "varyColors",
            "ser",
            "dLbls",
            "gapWidth",
            "overlap",
            "serLines",
            "axId",
        ],
    ),
    // CT_LineChart
    (
        "lineChart",
        &[
            "grouping",
            "varyColors",
            "ser",
            "dLbls",
            "dropLines",
            "hiLowLines",
            "upDownBars",
            "marker",
            "smooth",
            "axId",
        ],
    ),
    // CT_AreaChart
    (
        "areaChart",
        &[
            "grouping",
            "varyColors",
            "ser",
            "dLbls",
            "dropLines",
            "axId",
        ],
    ),
    // CT_PieChart
    ("pieChart", &["varyColors", "ser", "dLbls", "firstSliceAng"]),
    // CT_DoughnutChart
    (
        "doughnutChart",
        &["varyColors", "ser", "dLbls", "firstSliceAng", "holeSize"],
    ),
    // CT_ScatterChart
    (
        "scatterChart",
        &["scatterStyle", "varyColors", "ser", "dLbls", "axId"],
    ),
    // The union of CT_BarSer / CT_LineSer / CT_AreaSer / CT_PieSer /
    // CT_ScatterSer. They agree on the relative position of everything this
    // writer emits, which is why one writer is correct for all of them — and
    // this row is where that claim is checked rather than asserted in a comment.
    (
        "ser",
        &[
            "idx",
            "order",
            "tx",
            "spPr",
            "marker",
            "explosion",
            "invertIfNegative",
            "pictureOptions",
            "dPt",
            "dLbls",
            "trendline",
            "errBars",
            "cat",
            "xVal",
            "val",
            "yVal",
            "shape",
            "smooth",
            "extLst",
        ],
    ),
    // `c:tx` is two complex types under one name — `CT_SerTx` on a series
    // (`strRef | v`) and `CT_Tx` on a title (`strRef | rich`) — so this row is
    // their union. Both are CHOICES of one child, so there is no order inside a
    // `c:tx` to get wrong; what this row checks is that the child is one of the
    // three the schema admits at all.
    ("tx", &["strRef", "rich", "v"]),
    // CT_AxDataSource (`c:cat`) and CT_NumDataSource (`c:val`/`c:xVal`/`c:yVal`)
    // — choices, listed so an alternative the schema does NOT admit there is
    // caught: a `c:strLit` under `c:val` is invalid at any position.
    (
        "cat",
        &["multiLvlStrRef", "numRef", "numLit", "strRef", "strLit"],
    ),
    ("val", &["numRef", "numLit"]),
    ("xVal", &["numRef", "numLit"]),
    ("yVal", &["numRef", "numLit"]),
    // CT_StrRef / CT_NumRef
    ("strRef", &["f", "strCache", "extLst"]),
    ("numRef", &["f", "numCache", "extLst"]),
    // CT_StrData / CT_StrLit
    ("strCache", &["ptCount", "pt", "extLst"]),
    ("strLit", &["ptCount", "pt", "extLst"]),
    // CT_NumData / CT_NumLit
    ("numCache", &["formatCode", "ptCount", "pt", "extLst"]),
    ("numLit", &["formatCode", "ptCount", "pt", "extLst"]),
    // CT_NumVal / CT_StrVal
    ("pt", &["v"]),
    // CT_DLbls (the populated choice)
    (
        "dLbls",
        &[
            "numFmt",
            "spPr",
            "txPr",
            "dLblPos",
            "showLegendKey",
            "showVal",
            "showCatName",
            "showSerName",
            "showPercent",
            "showBubbleSize",
            "separator",
            "showLeaderLines",
            "leaderLines",
        ],
    ),
    // CT_Legend
    (
        "legend",
        &[
            "legendPos",
            "legendEntry",
            "layout",
            "overlay",
            "spPr",
            "txPr",
        ],
    ),
    // CT_CatAx / CT_ValAx / CT_DateAx share this prefix; the writer emits
    // nothing after `crossAx`, where they diverge.
    (
        "catAx",
        &[
            "axId",
            "scaling",
            "delete",
            "axPos",
            "majorGridlines",
            "minorGridlines",
            "title",
            "numFmt",
            "majorTickMark",
            "minorTickMark",
            "tickLblPos",
            "spPr",
            "txPr",
            "crossAx",
        ],
    ),
    (
        "valAx",
        &[
            "axId",
            "scaling",
            "delete",
            "axPos",
            "majorGridlines",
            "minorGridlines",
            "title",
            "numFmt",
            "majorTickMark",
            "minorTickMark",
            "tickLblPos",
            "spPr",
            "txPr",
            "crossAx",
        ],
    ),
    // CT_Scaling — `max` BEFORE `min`, which is the opposite of what a reader
    // expects and the mistake this row exists to catch.
    ("scaling", &["logBase", "orientation", "max", "min"]),
    // CT_ShapeProperties, trimmed to what this writer can emit.
    (
        "spPr",
        &[
            "xfrm",
            "custGeom",
            "prstGeom",
            "noFill",
            "solidFill",
            "gradFill",
            "ln",
        ],
    ),
    ("ln", &["noFill", "solidFill", "gradFill", "prstDash"]),
    (
        "solidFill",
        &[
            "scrgbClr",
            "srgbClr",
            "hslClr",
            "sysClr",
            "schemeClr",
            "prstClr",
        ],
    ),
    ("schemeClr", &["tint", "shade", "alpha", "lumMod", "lumOff"]),
    // CT_ExternalData
    ("externalData", &["autoUpdate"]),
];

/// Every element the writer emits sits at or after its previous sibling's
/// position in its parent's ECMA-376 sequence.
///
/// Walks the part rather than string-matching it, so the check covers whatever
/// the writer emitted and not a list of cases somebody remembered. An element
/// whose parent is not in [`SCHEMA_ORDER`] is a gap in the table, and the walk
/// says so rather than passing: a schema guard that silently skips what it does
/// not know is the vacuous-guard failure mode.
fn assert_schema_order(xml: &[u8]) {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    // (parent local name, the highest sequence position seen so far under it).
    let mut stack: Vec<(String, usize)> = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .expect("the generated part is well-formed XML");
        let (name, opens) = match &event {
            Event::Eof => break,
            Event::Start(element) => (element.local_name(), true),
            Event::Empty(element) => (element.local_name(), false),
            Event::End(_) => {
                stack.pop();
                buffer.clear();
                continue;
            }
            _ => {
                buffer.clear();
                continue;
            }
        };
        let name = String::from_utf8(name.as_ref().to_vec()).expect("an ASCII local name");
        if let Some((parent, highest)) = stack.last_mut() {
            let sequence = SCHEMA_ORDER
                .iter()
                .find(|(ct, _)| ct == parent)
                .unwrap_or_else(|| {
                    panic!(
                        "the writer emitted a child of <{parent}>, which SCHEMA_ORDER does not \
                         describe; add its ECMA-376 sequence rather than leaving the guard blind"
                    )
                })
                .1;
            let position = sequence.iter().position(|child| *child == name);
            let position = position
                .unwrap_or_else(|| panic!("<{name}> is not a child <{parent}> admits in ECMA-376"));
            assert!(
                position >= *highest,
                "<{name}> is out of sequence inside <{parent}>: it belongs at position \
                 {position} and a sibling at {highest} was already written. Word repairs an \
                 out-of-order chart part by discarding the chart."
            );
            *highest = position;
        }
        if opens {
            stack.push((name, 0));
        }
    }
}

/// Every family's part is in ECMA-376 child order.
///
/// Deliberately separate from the round-trip sweep and deliberately NOT derived
/// from it: the reader does not care about order, so a part the reader
/// reconstructs perfectly can still be one Word discards. This is the only guard
/// in the file that can fail on an ordering mistake.
#[test]
fn every_generated_chart_part_is_in_ecma_376_child_order() {
    let families = [
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
        ChartGroupKind::Line {
            grouping: Grouping::Standard,
            marker: true,
        },
        ChartGroupKind::Area {
            grouping: Grouping::Stacked,
        },
        ChartGroupKind::Pie {
            first_slice_angle: 90,
        },
        ChartGroupKind::Doughnut {
            first_slice_angle: 0,
            hole_size: 75,
        },
        ChartGroupKind::Scatter {
            style: ScatterStyle::LineMarker,
        },
    ];
    for group in families {
        // Dressed, so the optional children are present and therefore ordered:
        // a guard over the bare projection would only ever see five elements.
        let mut chart = projection(id(3), group);
        chart.title = Some(ChartTitle {
            text: Some(ChartText {
                text: "Revenue".to_owned(),
                formula: None,
            }),
            overlay: false,
        });
        chart.auto_title_deleted = false;
        let series = &mut chart.plot_area.groups[0].series[0];
        series.fill = Some(Color::Rgb(RgbColor {
            r: 0x33,
            g: 0x55,
            b: 0xC4,
        }));
        series.line = Some(ChartLine {
            color: Some(Color::Theme(ThemeColor {
                slot: ThemeColorRef::Accent1,
                theme_tint: Some(0x7F),
                theme_shade: None,
            })),
            width_emu: Some(12_700),
            no_fill: false,
        });
        series.data_labels = Some(DataLabels {
            show_value: true,
            show_category_name: true,
            show_series_name: false,
            show_percent: false,
            position: Some(DataLabelPosition::Center),
        });
        series.values.number_format = Some("0.00".to_owned());
        series.values.formula = Some("Sheet1!$B$2:$B$5".to_owned());
        if let Some(categories) = series.categories.as_mut() {
            categories.formula = Some("Sheet1!$A$2:$A$5".to_owned());
        }
        for axis in &mut chart.plot_area.axes {
            axis.minimum = Some("0".to_owned());
            axis.maximum = Some("9".to_owned());
            axis.number_format = Some("General".to_owned());
            axis.minor_gridlines = true;
        }
        chart.external_data = None;

        let written =
            write_document(&document_with(chart), &BTreeMap::new()).expect("the document writes");
        let mut package =
            DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
        let part = package.read_part(CHART_PART).expect("the chart part reads");
        assert_schema_order(&part);
    }
}

/// Builds a minimal ZIP package from `(name, bytes)` pairs.
fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    for (name, bytes) in parts {
        writer.start_file(*name, options).expect("a zip entry");
        writer.write_all(bytes).expect("zip bytes");
    }
    writer.finish().expect("the zip finishes").into_inner()
}

/// The `r:id` the DRAWING carries resolves, through OPC, to the generated part.
///
/// # Why this exists beside the round-trip guards
///
/// Every guard above that reads the projection back proves the chain works
/// *through the importer*, and the importer is one implementation of the
/// resolution. This walks it the way a consumer does and asserts each link
/// separately, because HF-256 was a break in exactly one link — the last one —
/// and a single reader that happens to tolerate a difference would hide it:
///
/// 1. `word/document.xml` carries a `c:chart` with an `r:id`.
/// 2. `word/_rels/document.xml.rels` declares a relationship with that `Id`,
///    and the package resolves its `Target` to a part name.
/// 3. That part is readable and is the chart part the model named.
/// 4. `[Content_Types].xml` declares it as a chart part rather than letting it
///    fall through the `.xml` default to `application/xml`.
///
/// The link numbers are in the failure messages so a break says which one.
#[test]
fn the_drawings_relationship_id_resolves_through_opc_to_the_generated_chart_part() {
    let document = document_with(projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    ));
    let written = write_document(&document, &BTreeMap::new()).expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");

    // Link 1: the body's own reference, read out of the written XML rather than
    // assumed from the model.
    let body = package
        .read_part("word/document.xml")
        .expect("the main document part reads");
    let referenced = chart_reference_ids(&body);
    assert_eq!(
        referenced.len(),
        1,
        "link 1: the body must carry exactly one `c:chart r:id`, got {referenced:?}"
    );
    let rel_id = &referenced[0];

    // Link 2: OPC resolution of that id, by the package reader's own rules.
    let resolved = package
        .main_document_relationships()
        .iter()
        .find(|rel| &rel.id == rel_id)
        .unwrap_or_else(|| {
            panic!(
                "link 2: the body references {rel_id}, which `document.xml.rels` does not \
                 declare — a dangling relationship is exactly what HF-256 was"
            )
        })
        .clone();
    assert_eq!(
        resolved.relationship_type, CHART_REL_TYPE,
        "link 2: the relationship must be a chart relationship"
    );
    let part_name = resolved
        .resolved_part
        .clone()
        .expect("link 2: the chart relationship's target must resolve inside the package");
    assert_eq!(
        part_name, CHART_PART,
        "link 2: the relationship must resolve to the part name the model named, so the \
         part this export wrote and the part the body points at cannot disagree"
    );

    // Link 3: the part is there and is a chart space.
    let part = package
        .read_part(&part_name)
        .unwrap_or_else(|_| panic!("link 3: {part_name} must be in the package"));
    assert!(
        String::from_utf8_lossy(&part).starts_with("<c:chartSpace"),
        "link 3: the resolved part must be a chart space"
    );

    // Link 4: the content type, resolved through the package's own override
    // handling rather than by grepping `[Content_Types].xml`.
    assert_eq!(
        package.content_type(&part_name),
        Some(CHART_CT),
        "link 4: without the Override the part falls through the `.xml` Default to \
         application/xml and Word refuses the package"
    );

    // Link 5: the chart's own `_rels` names exactly one thing, the values-only
    // workbook the writer bound it to, and that relationship resolves to a part
    // the package contains — the same chain one level down, which is where
    // Word's Edit Data goes.
    let own = package
        .part_relationships(&part_name)
        .expect("the chart part's relationships resolve");
    assert_eq!(
        own.len(),
        1,
        "link 5: one workbook relationship, got {own:?}"
    );
    assert_eq!(own[0].relationship_type, PACKAGE_REL_TYPE, "link 5");
    let workbook = own[0]
        .resolved_part
        .clone()
        .expect("link 5: the workbook relationship must resolve inside the package");
    assert!(
        package.read_part(&workbook).is_ok(),
        "link 5: {workbook} is named and not in the package"
    );
}

/// Every `r:id` the body's `c:chart` elements carry, in document order.
///
/// Reads the written XML rather than the model: the question is what the FILE
/// says, and a helper that consulted the model could not tell the two apart.
fn chart_reference_ids(body: &[u8]) -> Vec<String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(body);
    let mut buffer = Vec::new();
    let mut ids = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .expect("the written body is well-formed XML");
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element)
                if element.local_name().as_ref() == b"chart" =>
            {
                for attribute in element.attributes() {
                    let attribute = attribute.expect("a well-formed attribute");
                    if attribute.key.as_ref() == b"r:id" {
                        ids.push(
                            String::from_utf8(attribute.value.to_vec()).expect("an ASCII r:id"),
                        );
                    }
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    ids
}

/// A chart whose part the package will NOT contain leaves no reference to it —
/// neither a relationship nor a `c:chart r:id` (`109` HF-256, the second half).
///
/// # The half that was still broken, measured
///
/// `a_partial_projection_is_not_regenerated_and_the_refusal_is_reported` asserts
/// that no part is written and that the refusal is reported, and it passed —
/// while the package was still corrupt. The relationship was emitted
/// unconditionally from the node's own `EmbeddedPart` and the body still carried
/// the `c:chart r:id`, so the saved file advertised a chart part that was not
/// there: the exact failure HF-256 names, in a narrower population, and the
/// export reported a clean save of a file Word reports as unreadable.
///
/// What a correct save does instead was already settled for pictures by
/// `available_media` (FID-R-06): drop the body reference with the part and name
/// the loss. The chart is gone either way; the choice is between a corrupt
/// package that hides that and a valid one that reports it.
///
/// Both routes to the state are driven, because a fix that only looked at
/// coverage would leave the other:
///
/// * a `Partial` projection — the coverage gate refuses to regenerate it;
/// * a chart object with no projection at all — an imported chart whose part the
///   reader declined, exported without retention, so there is nothing to
///   generate from.
#[test]
fn a_chart_part_the_package_will_not_contain_leaves_no_reference_behind() {
    let bar = ChartGroupKind::Bar {
        direction: BarDirection::Column,
        grouping: BarGrouping::Clustered,
        gap_width: 150,
        overlap: -27,
    };
    let mut partial = projection(id(3), bar);
    partial.coverage = ChartCoverage::Partial;
    assert!(
        !partial.coverage.permits_regeneration(),
        "the coverage gate under test must actually refuse"
    );
    // The second route: the same chart object, with no entry in
    // `Definitions::charts` at all.
    let unprojected = Document::new(
        id(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: id(2),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::EmbeddedObject(Box::new(chart_object(id(3))))],
        })],
        Definitions::default(),
    )
    .expect("a chart object with no projection is a valid document");
    assert!(
        unprojected.definitions().charts.is_empty(),
        "the second route is a chart OBJECT with no projection behind it"
    );

    for (case, document) in [
        ("a partial projection", document_with(partial)),
        ("an unprojected chart object", unprojected),
    ] {
        let export =
            export_document(&document, &BTreeMap::new()).expect("the document still writes");
        let mut package =
            DocxPackage::open(&export.bytes, PackageLimits::default()).expect("the package opens");

        assert!(
            package.read_part(CHART_PART).is_err(),
            "{case}: no part can be written, and this case is only interesting because of that"
        );
        // The relationship. `main_document_relationships` is the package's own
        // resolution, so this is a reader's view and not a substring search.
        assert!(
            !package
                .main_document_relationships()
                .iter()
                .any(|rel| rel.relationship_type == CHART_REL_TYPE),
            "{case}: a relationship whose target is not in the package is the corruption \
             HF-256 is about"
        );
        // The body reference.
        assert_eq!(
            chart_reference_ids(
                &package
                    .read_part("word/document.xml")
                    .expect("the main document part reads")
            ),
            Vec::<String>::new(),
            "{case}: the body must not carry a `c:chart r:id` whose relationship was dropped"
        );
        // And the loss is named rather than silent, which is the other half of
        // the same requirement (`AGENTS.md`: no silent data loss).
        let ids: Vec<&str> = export
            .report
            .entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect();
        assert!(
            ids.contains(&"docx.export.embedded_object.missing_part"),
            "{case}: dropping the object must be reported: {ids:?}"
        );
        // The reopened document agrees: the object is gone, not half-written. A
        // package whose chart reference survived while its part did not would
        // reopen with an object pointing at nothing.
        let import = reopen(&export.bytes);
        assert!(
            !import.document.body().iter().any(|block| matches!(
                block,
                BlockNode::Paragraph(paragraph)
                    if paragraph
                        .inlines
                        .iter()
                        .any(|inline| matches!(inline, InlineNode::EmbeddedObject(_)))
            )),
            "{case}: the reopened document must hold no embedded object at all"
        );
    }
}

/// An ORPHAN projection — one whose anchor was removed — writes no part and
/// leaves a package that reopens (`109` HF-257, the export consequence).
///
/// HF-257 made `Document::validate` tolerate a projection whose anchor is gone,
/// on the argument that a projection is a derived read index and a stale index
/// is not corruption. That argument is only sound if nothing downstream acts on
/// the orphan, so this is the half of it that lives in the exporter: the chart
/// part writer keys off the objects it finds in the BODY, never off
/// `Definitions::charts`, so an orphan contributes no part, no relationship and
/// no content-type override.
///
/// The failure this catches is the obvious implementation of the writer —
/// iterate the projection table — which would write `word/charts/chart7.xml`
/// with nothing in the document referencing it, growing the file on every save
/// after a deletion.
#[test]
fn an_orphan_projection_writes_no_part_and_the_package_still_reopens() {
    let mut document = document_with(projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    ));
    // A second paragraph, so removing the chart's does not empty the body — an
    // empty body is a different refusal (`EmptyDocumentBody`) and would mask the
    // one under test. It did exactly that on the first run of this guard.
    document.body_mut().push(BlockNode::Paragraph(Paragraph {
        id: id(9),
        properties: ParagraphProperties::default().into(),
        inlines: Vec::new(),
    }));
    // The positional removal HF-257 is about: the paragraph holding the chart
    // goes, and nothing names the object, so the sidecar entry stays behind.
    document
        .body_mut()
        .retain(|block| !matches!(block, BlockNode::Paragraph(paragraph) if paragraph.id == id(2)));
    assert_eq!(
        document.definitions().charts.len(),
        1,
        "the orphan is the state under test: if the removal evicted it this proves nothing"
    );
    document
        .validate()
        .expect("HF-257: an orphan projection must not invalidate the document");

    let export =
        export_document(&document, &BTreeMap::new()).expect("a document with an orphan writes");
    // Not `mut`: the directory scan below reads metadata, so no part is read.
    let package =
        DocxPackage::open(&export.bytes, PackageLimits::default()).expect("the package opens");
    // Scanned by DIRECTORY, not by the one name the model happened to hold: a
    // writer keyed off the projection table would mint its own name, and a guard
    // that only looked for `chart7.xml` would pass over the part it wrote. What
    // must be true is that the package carries no chart part AT ALL.
    let chart_parts: Vec<&str> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.as_str())
        .filter(|name| name.starts_with("word/charts/"))
        .collect();
    assert_eq!(
        chart_parts,
        Vec::<&str>::new(),
        "an orphan projection must not produce a part nothing references"
    );
    // Not `None`: `content_type` falls back to the `.xml` extension Default for
    // any name, present or not, so the assertable claim is that no chart
    // OVERRIDE was declared for a part that is not there.
    assert_ne!(
        package.content_type(CHART_PART),
        Some(CHART_CT),
        "nor a content-type override for a part that is not there"
    );
    assert!(
        !package
            .main_document_relationships()
            .iter()
            .any(|rel| rel.relationship_type == CHART_REL_TYPE),
        "nor a chart relationship"
    );
    let import = reopen(&export.bytes);
    assert!(
        import.document.definitions().charts.is_empty(),
        "the orphan does not survive a save: it is a stale read index, and reopening \
         rebuilds the index from the parts that are actually there"
    );
}

/// **The workbook holds exactly the chart's data, in Word's layout.**
///
/// Row 1 the series names, column A the categories, one column per series, each
/// number in the same lexical form the cache holds — `3.50`, not `3.5`. A
/// workbook that disagreed with the cache would be the divergence `docs/155`
/// §5.3 exists to prevent: the page showing one number and Edit Data another.
#[test]
fn the_embedded_workbook_holds_the_chart_data_in_words_layout() {
    let chart = projection(
        id(3),
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
    );
    let written =
        write_document(&document_with(chart), &BTreeMap::new()).expect("the document writes");
    let after = reopened_projection(&reopen(&written));
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    let workbook = package
        .read_part(&after.external_data.expect("a workbook").part_name)
        .expect("the workbook reads");
    assert_eq!(
        workbook_cells(&workbook),
        [
            ("B1", "Series 1"),
            ("A2", "1st Qtr"),
            ("B2", "4.3"),
            ("A3", "2nd Qtr"),
            ("B3", "2.5"),
            ("A4", "3rd Qtr"),
            ("B4", "3.50"),
            ("A5", "4th Qtr"),
            ("B5", "4.5"),
        ]
        .map(|(r, t)| (r.to_owned(), t.to_owned()))
        .to_vec()
    );
}

/// **An EDITED imported chart is regenerated, and its source bytes are not
/// written beside the regeneration.**
///
/// The other half of `a_retained_chart_part_is_copied_verbatim_and_not_regenerated`.
/// Retention wins until an edit makes the projection the authority
/// (`Chart::dirty`); after that, copying the source bytes back would throw the
/// reader's edit away on save. And both may not be written: two ZIP entries
/// under one name, or two `Override`s for one part, is a package no two readers
/// agree about. The workbook the chart named is replaced in place, and the
/// report says so.
#[test]
fn an_edited_imported_chart_is_regenerated_and_supersedes_its_source_bytes() {
    let content_types = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="xlsx" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#;
    let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="304800"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
    let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#;
    let chart = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>4.30</c:v></c:pt></c:numLit></c:val></c:ser><c:gapWidth val="150"/><c:overlap val="-27"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx></c:plotArea></c:chart><c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#;
    let chart_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet.xlsx"/></Relationships>"#;
    let old_workbook = b"PK-the-producer's-workbook-with-a-secret-second-sheet";
    let source = zip_package(&[
        ("[Content_Types].xml", content_types.as_slice()),
        ("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.as_slice()),
        ("word/document.xml", document.as_slice()),
        ("word/_rels/document.xml.rels", doc_rels.as_slice()),
        ("word/charts/chart1.xml", chart.as_slice()),
        ("word/charts/_rels/chart1.xml.rels", chart_rels.as_slice()),
        ("word/embeddings/Microsoft_Excel_Worksheet.xlsx", old_workbook.as_slice()),
    ]);
    let mut source_package =
        DocxPackage::open(&source, PackageLimits::default()).expect("the source package opens");
    let mut import = import_package(
        &mut source_package,
        ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        },
    )
    .expect("the source package imports");
    let (chart_id, mut edited) = import
        .document
        .definitions()
        .charts
        .iter()
        .map(|(id, chart)| (*id, chart.clone()))
        .next()
        .expect("the fixture projects");
    assert_eq!(
        edited.coverage,
        ChartCoverage::Complete,
        "the fixture must be editable"
    );
    assert!(
        edited.external_data.is_some(),
        "the fixture must name its workbook"
    );
    // What `setChartData` does: change the data, drop the source formulas, and
    // mark the projection the authority.
    edited.plot_area.groups[0].series[0].values = numbers(&["9.75"]);
    edited.dirty = true;
    let mut definitions = import.document.definitions().clone();
    definitions.charts.insert(chart_id, edited);
    import.document = Document::new(
        import.document.id(),
        import.document.body().to_vec(),
        definitions,
    )
    .expect("the edited document is valid");

    let export = casual_doc_export::export_document_with_retained_parts(
        &import.document,
        &BTreeMap::new(),
        &import.retained_parts,
    )
    .expect("the edited document writes");
    let written = export.bytes;

    // One entry per name in the ZIP, and one Override per part in the manifest.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&written)).expect("a ZIP");
    let mut names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).expect("an entry").name().to_owned())
        .collect();
    let total = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), total, "a part was written twice: {names:?}");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    let manifest = String::from_utf8(package.read_part("[Content_Types].xml").expect("manifest"))
        .expect("UTF-8");
    assert_eq!(
        manifest
            .matches(r#"PartName="/word/charts/chart1.xml""#)
            .count(),
        1,
        "the chart part is declared twice: {manifest}"
    );

    // The chart part is the regeneration, carrying the reader's number…
    let chart_xml = String::from_utf8(package.read_part("word/charts/chart1.xml").expect("chart"))
        .expect("UTF-8");
    assert_ne!(
        chart_xml.as_bytes(),
        chart.as_slice(),
        "the source bytes won over the edit"
    );
    assert!(
        chart_xml.contains("<c:v>9.75</c:v>"),
        "the edit is not in the part: {chart_xml}"
    );
    assert!(
        chart_xml.contains("Sheet1!$B$2:$B$2"),
        "the series is not bound to the workbook"
    );
    // …and the workbook it names, at the SAME name, holds that number and
    // nothing of the producer's.
    let workbook = package
        .read_part("word/embeddings/Microsoft_Excel_Worksheet.xlsx")
        .expect("the workbook is still at its name");
    assert_ne!(
        workbook.as_slice(),
        old_workbook.as_slice(),
        "the stale workbook survived"
    );
    assert!(workbook_cells(&workbook).contains(&("B2".to_owned(), "9.75".to_owned())));
    assert!(
        export
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "docx.export.chart.workbook_replaced"),
        "replacing the producer's workbook must be reported, not silent"
    );

    // And the file reopens with the edit in it.
    let reopened = reopened_projection(&reopen(&written));
    assert_eq!(
        reopened.plot_area.groups[0].series[0].values.points,
        vec![(0, ChartValue::Number("9.75".to_owned()))]
    );
}
