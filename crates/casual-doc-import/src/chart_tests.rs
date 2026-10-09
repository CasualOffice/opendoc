//! The chart reader's projection, carry and decline guards (`docs/155`
//! §§5-8, §17). A child of `chart`, in its own file so `chart.rs` stays
//! near the `SKILL` §10 size line.

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

/// A chart as Word 2016 writes one by default (synthetic, written from the
/// schema — no customer file), and the three places it used to go wrong.
pub(crate) const WORD_DEFAULT_CHART: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:c16r2="http://schemas.microsoft.com/office/drawing/2015/06/chart" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:c14="http://schemas.microsoft.com/office/drawing/2007/8/2/chart"><c:date1904 val="0"/><c:lang val="en-US"/><c:roundedCorners val="0"/><mc:AlternateContent><mc:Choice Requires="c14"><c14:style val="102"/></mc:Choice><mc:Fallback><c:style val="2"/></mc:Fallback></mc:AlternateContent><c:chart><c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Series 1</c:v></c:pt></c:strCache></c:strRef></c:tx><c:spPr><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:invertIfNegative val="0"/><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$3</c:f><c:strCache><c:ptCount val="2"/><c:pt idx="0"><c:v>Category 1</c:v></c:pt><c:pt idx="1"><c:v>Category 2</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>4.3</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val><c:extLst><c:ext uri="{C3380CC4-5D6E-409C-BE32-E72D297353CC}" xmlns:c16="http://schemas.microsoft.com/office/drawing/2014/chart"><c16:uniqueId val="{00000000-0001-0000-0000-000000000000}"/></c:ext></c:extLst></c:ser><c:dLbls><c:showLegendKey val="0"/><c:showVal val="0"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="0"/><c:showBubbleSize val="0"/></c:dLbls><c:gapWidth val="219"/><c:overlap val="-27"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:spPr><a:noFill/><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="15000"/><a:lumOff val="85000"/></a:schemeClr></a:solidFill><a:round/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr rot="-60000000" spcFirstLastPara="1" vertOverflow="ellipsis" vert="horz" wrap="square" anchor="ctr" anchorCtr="1"/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900" b="0" i="0" u="none" strike="noStrike" kern="1200" baseline="0"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="65000"/><a:lumOff val="35000"/></a:schemeClr></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr><c:crossAx val="2"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:noMultiLvlLbl val="0"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines><c:spPr><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="15000"/><a:lumOff val="85000"/></a:schemeClr></a:solidFill><a:round/></a:ln><a:effectLst/></c:spPr></c:majorGridlines><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900"/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr><c:crossAx val="1"/><c:crosses val="autoZero"/><c:crossBetween val="between"/></c:valAx><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900"/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr></c:legend><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr><c:externalData r:id="rId3"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#;

/// **A default Word chart is fully understood, and keeps its formatting.**
///
/// Before `docs/155` §17 this chart projected `Partial` on eleven constructs
/// none of which its data depends on — axis and legend `c:spPr`/`c:txPr`,
/// `c:lang`, the style's `mc:AlternateContent`, `c:crosses`, `c:lblAlgn` —
/// and `Partial` forbids regeneration, so every chart Word wrote opened
/// read-only. Each is now carried verbatim on the container it came from.
#[test]
fn a_default_word_chart_is_complete_and_carries_what_it_does_not_model() {
    let read = read(WORD_DEFAULT_CHART.as_bytes());
    assert_eq!(
        read.projection.as_ref().map(|chart| chart.coverage),
        Some(ChartCoverage::Complete),
        "a default Word chart must be fully carried: {:?}",
        read.unconsumed
    );
    // Still NAMED — none of it is drawn — just no longer a loss. The
    // text properties are modelled now (`docs/155` §19), so what is named
    // from them is what the font cannot say: the East Asian face, the
    // luminance transform on the colour.
    assert!(read.unconsumed.contains("lang") && read.unconsumed.contains("ea"));
    assert!(
        !read.unconsumed.contains("txPr"),
        "a c:txPr is modelled, not an unread construct: {:?}",
        read.unconsumed
    );
    let chart = read.projection.expect("a projection");
    // Every container's text font is read from its `c:txPr`.
    let tick_labels = chart.plot_area.axes[0].font.as_ref().expect("axis font");
    assert_eq!(tick_labels.size, Some(900));
    assert_eq!(tick_labels.bold, Some(false));
    assert_eq!(tick_labels.italic, Some(false));
    assert_eq!(tick_labels.typeface.as_deref(), Some("+mn-lt"));
    assert_eq!(
        tick_labels.color,
        Some(Color::Theme(ThemeColor {
            slot: ThemeColorRef::Dark1,
            theme_tint: None,
            theme_shade: None,
        }))
    );
    assert_eq!(
        chart.plot_area.axes[1].font.as_ref().and_then(|f| f.size),
        Some(900)
    );
    assert_eq!(
        chart
            .legend
            .as_ref()
            .and_then(|legend| legend.font.as_ref())
            .and_then(|f| f.size),
        Some(900)
    );
    // `<a:defRPr/>` declares nothing: no font, and (its `a:endParaRPr` being
    // the default one) nothing kept either.
    assert_eq!(chart.font, None, "an empty defRPr");
    assert_eq!(chart.coverage, ChartCoverage::Complete);
    fn names(fragments: &[ChartXml]) -> Vec<&str> {
        fragments
            .iter()
            .map(|fragment| fragment.name.as_str())
            .collect()
    }
    assert_eq!(
        names(&chart.space_retained),
        ["lang", "AlternateContent", "spPr"]
    );
    assert_eq!(names(&chart.plot_area.retained), ["spPr"]);
    let group = &chart.plot_area.groups[0];
    assert_eq!(names(&group.retained), ["dLbls"]);
    // The series' `c:spPr` is a SHADOW: its fill is modelled AND its bytes
    // are kept, because the model does not hold `a:effectLst`.
    assert_eq!(names(&group.series[0].retained), ["spPr"]);
    assert!(
        group.series[0].fill.is_some(),
        "the shadowed fill was not also parsed"
    );
    assert_eq!(
        names(&chart.plot_area.axes[0].retained),
        ["spPr", "txPr", "crosses", "auto", "lblAlgn", "lblOffset"]
    );
    assert_eq!(
        names(&chart.plot_area.axes[1].retained),
        ["majorGridlines", "spPr", "crosses", "crossBetween"]
    );
    assert!(chart.plot_area.axes[1].major_gridlines);
    assert_eq!(
        names(&chart.legend.expect("a legend").retained),
        ["spPr"],
        "a c:txPr the font fully holds is regenerated, not kept"
    );
    // Verbatim means verbatim: the axis text size is still in the bytes.
    assert!(
        chart.plot_area.axes[0].retained[1]
            .xml
            .contains(r#"sz="900""#)
    );
    // And the root's prefixes travel with them, so `mc:` and `c14:` resolve.
    assert!(chart.namespaces.iter().any(|(prefix, _)| prefix == "c14"));
}

/// **What cannot be put back is still a loss.** An element its container's
/// schema sequence does not admit has no position a writer could use, and
/// one naming a relationship would point at nothing in a regenerated part.
#[test]
fn an_unplaceable_or_relationship_bearing_element_is_still_unconsumed() {
    let foreign = WORD_DEFAULT_CHART.replace(
        "<c:plotVisOnly",
        "<c:notInTheSchema val=\"1\"/><c:plotVisOnly",
    );
    let first = read(foreign.as_bytes());
    assert!(
        first.unconsumed.contains("notInTheSchema"),
        "{:?}",
        first.unconsumed
    );
    assert_eq!(
        first.projection.expect("a projection").coverage,
        ChartCoverage::Partial
    );

    let shapes = WORD_DEFAULT_CHART.replace(
        "</c:chartSpace>",
        r#"<c:userShapes r:id="rId9"/></c:chartSpace>"#,
    );
    let second = read(shapes.as_bytes());
    assert!(
        second.unconsumed.contains("userShapes"),
        "{:?}",
        second.unconsumed
    );
}

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
    // `c:trendline` and `c:errBars` graduated in `docs/155` §19 and left
    // this table in the same change, as the header above requires; their
    // guards are `trendlines_and_error_bars_are_modelled_and_cost_nothing`
    // and `a_malformed_trendline_or_font_is_carried_never_thrown`.
    let in_series = [
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
        // Named (it is not modelled or drawn) but carried verbatim on the
        // series, so a regenerated save keeps it: not a loss (§17) — unless
        // the BAR series sequence has no place for it. `c:explosion` is a
        // pie-slice setting; `CT_BarSer` does not admit it, so a writer
        // could not put it back and it stays a loss, honestly.
        let placeable = expected != "explosion";
        assert_eq!(
            read.projection.as_ref().map(|chart| chart.coverage),
            Some(if placeable {
                ChartCoverage::Complete
            } else {
                ChartCoverage::Partial
            }),
            "{expected}"
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
