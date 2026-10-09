//! Chart formatting read into the model (`docs/155` §19): text fonts from
//! `c:txPr` and a title's rich text, axis titles, line dashes, trendlines and
//! error bars — each parsed instead of carried, with the verbatim bytes kept
//! only when they hold something the model cannot say. Synthetic fixtures
//! only, written from the schema.

use super::*;
use casual_doc_model::v1::DashStyle;

fn read_part(xml: &str) -> ChartRead {
    let object = IdGenerator::new(1).next_id().expect("an id");
    read_chart_part(
        xml.as_bytes(),
        object,
        &BTreeMap::new(),
        ImportConfig::default(),
    )
}

fn projection(xml: &str) -> Chart {
    read_part(xml).projection.expect("a projection")
}

fn names(fragments: &[ChartXml]) -> Vec<&str> {
    fragments
        .iter()
        .map(|fragment| fragment.name.as_str())
        .collect()
}

/// A `c:chartSpace` around a chart body (title, plot area, legend, …) and
/// whatever follows `c:chart` (the chart-space `c:txPr`).
fn space(chart: &str, after_chart: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart>{chart}</c:chart>{after_chart}</c:chartSpace>"#
    )
}

/// One bar series with cached values; `extra` lands at the series' end.
fn bar_series(extra: &str) -> String {
    format!(
        r#"<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>3</c:v></c:pt></c:numLit></c:val>{extra}</c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>"#
    )
}

/// A plot area holding `group` and two plain axes; `axis_extra` lands inside
/// the value axis, after `c:axPos`.
fn plot(group: &str, axis_extra: &str) -> String {
    format!(
        r#"<c:plotArea><c:layout/>{group}<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/>{axis_extra}<c:crossAx val="1"/></c:valAx></c:plotArea>"#
    )
}

/// A `c:txPr` whose every statement the font can hold.
const COMPLETE_TXPR: &str = r#"<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1000" b="1" i="0" u="none"><a:solidFill><a:srgbClr val="1F4E79"/></a:solidFill><a:latin typeface="Arial"/></a:defRPr></a:pPr></a:p></c:txPr>"#;

/// The Arial 10 pt bold, not italic, `#1F4E79` that [`COMPLETE_TXPR`] declares.
fn complete_font() -> ChartFont {
    ChartFont {
        typeface: Some("Arial".to_owned()),
        size: Some(1000),
        bold: Some(true),
        italic: Some(false),
        color: Some(Color::Rgb(parse_rgb("1F4E79").expect("rgb"))),
    }
}

/// **Every container's text font is read, and a `c:txPr` the font fully
/// holds is not kept.**
///
/// The chart space, the legend and an axis each carry a `c:txPr`; the title
/// carries rich text whose first run overrides its paragraph defaults. The
/// three complete ones leave nothing behind (the writer regenerates them from
/// the font); the axis one rotates its labels, which the font cannot say, so
/// its bytes stay beside the parsed font.
#[test]
fn fonts_are_read_from_every_container_and_a_complete_one_is_not_kept() {
    let rotated = COMPLETE_TXPR.replace("<a:bodyPr/>", r#"<a:bodyPr rot="-2700000"/>"#);
    let title = r#"<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1400" b="1"><a:latin typeface="Georgia"/></a:defRPr></a:pPr><a:r><a:rPr sz="1600" i="1"><a:solidFill><a:schemeClr val="accent2"/></a:solidFill></a:rPr><a:t>Sales</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>"#;
    let legend = format!(
        r#"<c:legend><c:legendPos val="r"/><c:overlay val="0"/>{COMPLETE_TXPR}</c:legend>"#
    );
    let xml = space(
        &format!(
            "{title}<c:autoTitleDeleted val=\"0\"/>{}{legend}",
            plot(&bar_series(""), &rotated)
        ),
        COMPLETE_TXPR,
    );
    let read = read_part(&xml);
    let chart = read.projection.expect("a projection");
    assert_eq!(
        chart.coverage,
        ChartCoverage::Complete,
        "{:?}",
        read.unconsumed
    );

    assert_eq!(chart.font, Some(complete_font()), "chart-space c:txPr");
    assert!(
        !names(&chart.space_retained).contains(&"txPr"),
        "a c:txPr the font fully holds must not be kept: {:?}",
        names(&chart.space_retained)
    );
    let legend = chart.legend.as_ref().expect("a legend");
    assert_eq!(legend.font, Some(complete_font()), "legend c:txPr");
    assert!(legend.retained.is_empty(), "{:?}", names(&legend.retained));

    let axis = &chart.plot_area.axes[1];
    assert_eq!(axis.font, Some(complete_font()), "axis c:txPr");
    assert_eq!(
        names(&axis.retained),
        ["txPr"],
        "the rotation is not in the font, so the bytes stay"
    );
    assert!(axis.retained[0].xml.contains(r#"rot="-2700000""#));
    assert!(read.unconsumed.contains("bodyPr"), "{:?}", read.unconsumed);

    // The title: the run's size, italic and colour over the paragraph's face
    // and bold — what Word's Font box shows for the title.
    let title = chart.title.as_ref().expect("a title");
    assert_eq!(
        title.text.as_ref().map(|text| text.text.as_str()),
        Some("Sales")
    );
    assert_eq!(
        title.font,
        Some(ChartFont {
            typeface: Some("Georgia".to_owned()),
            size: Some(1600),
            bold: Some(true),
            italic: Some(true),
            color: Some(Color::Theme(ThemeColor {
                slot: ThemeColorRef::Accent2,
                theme_tint: None,
                theme_shade: None,
            })),
        })
    );
    assert!(
        title.retained.is_empty(),
        "rich text the font fully holds is regenerated: {:?}",
        names(&title.retained)
    );
}

/// **A title's rich text keeps its bytes when it says more than the font**:
/// a second run in another colour, a language tag, a luminance transform.
/// This is the shadow rule, not the old "any `a:rPr` means formatted" guess.
#[test]
fn rich_title_text_beyond_the_font_is_kept_verbatim() {
    for (tail, named) in [
        (r#"<a:r><a:rPr b="0"/><a:t> and more</a:t></a:r>"#, "rPr"),
        (r#"<a:endParaRPr lang="de-DE"/>"#, "endParaRPr"),
    ] {
        let title = format!(
            r#"<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:rPr b="1"/><a:t>Sales</a:t></a:r>{tail}</a:p></c:rich></c:tx></c:title>"#
        );
        let read = read_part(&space(&format!("{title}{}", plot(&bar_series(""), "")), ""));
        let chart = read.projection.expect("a projection");
        let title = chart.title.expect("a title");
        assert_eq!(title.font.as_ref().and_then(|font| font.bold), Some(true));
        assert_eq!(names(&title.retained), ["tx"], "{named}");
        assert!(read.unconsumed.contains(named), "{:?}", read.unconsumed);
        assert_eq!(chart.coverage, ChartCoverage::Complete);
    }
}

/// **An axis title is the axis's, with its text and font**, not a carried
/// fragment and not the chart's title.
#[test]
fn an_axis_title_is_parsed_with_its_text_and_font() {
    let axis_title = r#"<c:title><c:tx><c:rich><a:bodyPr rot="-5400000" vert="horz"/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1000" b="1"/></a:pPr><a:r><a:t>Revenue</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>"#;
    let read = read_part(&space(&plot(&bar_series(""), axis_title), ""));
    let chart = read.projection.expect("a projection");
    assert_eq!(
        chart.coverage,
        ChartCoverage::Complete,
        "{:?}",
        read.unconsumed
    );
    assert!(chart.title.is_none(), "an axis title is not the chart's");
    let axis = &chart.plot_area.axes[1];
    assert!(
        !names(&axis.retained).contains(&"title"),
        "the axis title is modelled, not carried: {:?}",
        names(&axis.retained)
    );
    let title = axis.title.as_ref().expect("the axis title");
    assert_eq!(
        title.text.as_ref().map(|text| text.text.as_str()),
        Some("Revenue")
    );
    assert_eq!(title.font.as_ref().and_then(|font| font.size), Some(1000));
    assert_eq!(title.font.as_ref().and_then(|font| font.bold), Some(true));
    // A left axis title's quarter turn is the default — what Word and the
    // writer generate — so the rich text is fully held and not kept.
    assert!(title.retained.is_empty(), "{:?}", names(&title.retained));
    // On a horizontal axis the same turn is NOT what the writer generates,
    // so there it is formatting, and kept.
    let horizontal = space(&plot(&bar_series(""), axis_title), "")
        .replace(r#"<c:axPos val="l"/>"#, r#"<c:axPos val="t"/>"#);
    let chart = projection(&horizontal);
    let title = chart.plot_area.axes[1]
        .title
        .as_ref()
        .expect("the axis title");
    assert_eq!(names(&title.retained), ["tx"], "a turned horizontal title");
    // Any other turn is the title's own formatting: kept on the TITLE.
    let tilted = axis_title.replace("-5400000", "-2700000");
    let chart = projection(&space(&plot(&bar_series(""), &tilted), ""));
    let title = chart.plot_area.axes[1]
        .title
        .as_ref()
        .expect("the axis title");
    assert_eq!(names(&title.retained), ["tx"]);
    assert!(title.retained[0].xml.contains(r#"rot="-2700000""#));
    // The category axis has none.
    assert!(chart.plot_area.axes[0].title.is_none());
}

/// **`a:prstDash` is a line's dash**, and a line the model fully holds leaves
/// no shadow; an unknown token is not guessed at, it keeps the bytes.
#[test]
fn a_line_dash_is_parsed() {
    let dashed = r#"<c:spPr><a:ln w="28575"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:prstDash val="sysDash"/></a:ln></c:spPr>"#;
    let chart = projection(&space(&plot(&bar_series(dashed), ""), ""));
    let series = &chart.plot_area.groups[0].series[0];
    let line = series.line.as_ref().expect("a line");
    assert_eq!(line.dash, Some(DashStyle::SystemDash));
    assert_eq!(line.width_emu, Some(28_575));
    assert_eq!(
        line.color,
        Some(Color::Rgb(parse_rgb("FF0000").expect("rgb")))
    );
    assert!(
        series.retained.is_empty(),
        "a fully modelled c:spPr is regenerated: {:?}",
        names(&series.retained)
    );

    let unknown = dashed.replace("sysDash", "wavy");
    let chart = projection(&space(&plot(&bar_series(&unknown), ""), ""));
    let series = &chart.plot_area.groups[0].series[0];
    assert_eq!(series.line.as_ref().and_then(|line| line.dash), None);
    assert_eq!(
        names(&series.retained),
        ["spPr"],
        "the unknown dash is kept"
    );
    assert_eq!(chart.coverage, ChartCoverage::Complete);
}

/// Three trendlines Word offers, on one bar series.
const TRENDLINES: &str = r#"<c:trendline><c:name>Linear (Sales)</c:name><c:trendlineType val="linear"/><c:forward val="1.5"/><c:intercept val="0"/><c:dispRSqr val="0"/><c:dispEq val="1"/><c:trendlineLbl><c:numFmt formatCode="0.00" sourceLinked="0"/></c:trendlineLbl></c:trendline><c:trendline><c:spPr><a:ln w="19050"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:prstDash val="sysDot"/></a:ln></c:spPr><c:trendlineType val="poly"/><c:order val="3"/><c:dispRSqr val="0"/><c:dispEq val="0"/></c:trendline><c:trendline><c:trendlineType val="movingAvg"/><c:period val="2"/><c:dispRSqr val="0"/><c:dispEq val="0"/></c:trendline>"#;

/// **Trendlines and error bars are modelled, and a chart holding only modelled
/// ones is `Complete` with nothing carried for them.**
#[test]
fn trendlines_and_error_bars_are_modelled_and_cost_nothing() {
    let fixed = r#"<c:errBars><c:errBarType val="both"/><c:errValType val="fixedVal"/><c:noEndCap val="1"/><c:val val="1.5"/></c:errBars>"#;
    let read = read_part(&space(
        &plot(&bar_series(&format!("{TRENDLINES}{fixed}")), ""),
        "",
    ));
    let chart = read.projection.expect("a projection");
    assert_eq!(
        chart.coverage,
        ChartCoverage::Complete,
        "{:?}",
        read.unconsumed
    );
    let series = &chart.plot_area.groups[0].series[0];
    assert!(
        series.retained.is_empty(),
        "modelled trendlines and error bars are not carried: {:?}",
        names(&series.retained)
    );
    assert_eq!(series.trendlines.len(), 3);
    let linear = &series.trendlines[0];
    assert_eq!(linear.kind, TrendlineKind::Linear);
    assert_eq!(linear.name.as_deref(), Some("Linear (Sales)"));
    assert_eq!(linear.forward.as_deref(), Some("1.5"));
    assert_eq!(linear.intercept.as_deref(), Some("0"));
    assert!(linear.display_equation && !linear.display_r_squared);
    // The label is not modelled: carried on the TRENDLINE, and named.
    assert_eq!(names(&linear.retained), ["trendlineLbl"]);
    assert!(read.unconsumed.contains("trendlineLbl"));
    // The label the writer generates for a shown equation is the default:
    // read back, it is not a loss and nothing is kept.
    let generated = TRENDLINES.replace(r#"formatCode="0.00""#, r#"formatCode="General""#);
    let labelled = projection(&space(&plot(&bar_series(&generated), ""), ""));
    assert!(
        labelled.plot_area.groups[0].series[0].trendlines[0]
            .retained
            .is_empty()
    );
    // But only while the label has something to show.
    let hidden = generated.replace(r#"<c:dispEq val="1"/>"#, r#"<c:dispEq val="0"/>"#);
    let unlabelled = projection(&space(&plot(&bar_series(&hidden), ""), ""));
    assert_eq!(
        names(&unlabelled.plot_area.groups[0].series[0].trendlines[0].retained),
        ["trendlineLbl"]
    );
    assert!(
        !read.unconsumed.contains("trendline"),
        "{:?}",
        read.unconsumed
    );

    let poly = &series.trendlines[1];
    assert_eq!(poly.kind, TrendlineKind::Polynomial);
    assert_eq!(poly.order, Some(3));
    let line = poly.line.as_ref().expect("the trendline's line");
    assert_eq!(line.dash, Some(DashStyle::SystemDot));
    assert_eq!(line.width_emu, Some(19_050));
    assert!(poly.retained.is_empty(), "{:?}", names(&poly.retained));

    let moving = &series.trendlines[2];
    assert_eq!(moving.kind, TrendlineKind::MovingAverage);
    assert_eq!(moving.period, Some(2));

    let bars = &series.error_bars[..];
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].direction, None, "absent errDir stays absent");
    assert_eq!(bars[0].bar_type, ErrorBarType::Both);
    assert_eq!(bars[0].value_type, ErrorValueType::FixedValue);
    assert_eq!(bars[0].value.as_deref(), Some("1.5"));
    assert!(bars[0].no_end_cap);

    // A scatter series holds one set per direction: a percentage, and a custom
    // one whose plus and minus lengths are literal data ranges.
    let scatter = r#"<c:scatterChart><c:scatterStyle val="lineMarker"/><c:ser><c:idx val="0"/><c:order val="0"/><c:errBars><c:errDir val="y"/><c:errBarType val="plus"/><c:errValType val="percentage"/><c:val val="5"/></c:errBars><c:errBars><c:errDir val="x"/><c:errBarType val="both"/><c:errValType val="cust"/><c:plus><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>0.5</c:v></c:pt><c:pt idx="1"><c:v>0.25</c:v></c:pt></c:numLit></c:plus><c:minus><c:numRef><c:f>Sheet1!$D$2:$D$3</c:f><c:numCache><c:ptCount val="2"/><c:pt idx="0"><c:v>0.1</c:v></c:pt><c:pt idx="1"><c:v>0.2</c:v></c:pt></c:numCache></c:numRef></c:minus><c:spPr><a:ln><a:prstDash val="dash"/></a:ln></c:spPr></c:errBars><c:xVal><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:xVal><c:yVal><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>3</c:v></c:pt><c:pt idx="1"><c:v>4</c:v></c:pt></c:numLit></c:yVal></c:ser><c:axId val="1"/><c:axId val="2"/></c:scatterChart>"#;
    let read = read_part(&space(&plot(scatter, ""), ""));
    let chart = read.projection.expect("a projection");
    assert_eq!(
        chart.coverage,
        ChartCoverage::Complete,
        "{:?}",
        read.unconsumed
    );
    let series = &chart.plot_area.groups[0].series[0];
    assert!(series.retained.is_empty(), "{:?}", names(&series.retained));
    let [percent, custom] = &series.error_bars[..] else {
        panic!("two sets of error bars, got {:?}", series.error_bars);
    };
    assert_eq!(percent.direction, Some(ErrorBarDirection::Y));
    assert_eq!(percent.bar_type, ErrorBarType::Plus);
    assert_eq!(percent.value_type, ErrorValueType::Percentage);
    assert_eq!(percent.value.as_deref(), Some("5"));
    assert_eq!(custom.direction, Some(ErrorBarDirection::X));
    assert_eq!(custom.value_type, ErrorValueType::Custom);
    let plus = custom.plus.as_ref().expect("plus lengths");
    assert_eq!(
        plus.points,
        vec![
            (0, ChartValue::Number("0.5".to_owned())),
            (1, ChartValue::Number("0.25".to_owned())),
        ]
    );
    let minus = custom.minus.as_ref().expect("minus lengths");
    assert_eq!(minus.formula.as_deref(), Some("Sheet1!$D$2:$D$3"));
    assert_eq!(minus.points.len(), 2);
    assert_eq!(
        custom.line.as_ref().and_then(|line| line.dash),
        Some(DashStyle::Dash)
    );
    // The series' own data was not disturbed by the error bars' ranges.
    assert_eq!(series.values.points.len(), 2);
    assert_eq!(
        series.x_values.as_ref().map(|range| range.points.len()),
        Some(2)
    );
}

/// **Past the model's bound the rest are LOST, named and `Partial`** — never
/// silently dropped, never carried in a way that would let a regenerated save
/// claim to have kept them.
#[test]
fn trendlines_and_error_bars_past_the_bound_are_lost_not_dropped() {
    let one = r#"<c:trendline><c:trendlineType val="linear"/></c:trendline>"#;
    let many = one.repeat(MAX_CHART_TRENDLINES + 1);
    let read = read_part(&space(&plot(&bar_series(&many), ""), ""));
    let chart = read.projection.expect("a projection");
    let series = &chart.plot_area.groups[0].series[0];
    assert_eq!(series.trendlines.len(), MAX_CHART_TRENDLINES);
    assert!(
        read.unconsumed.contains("trendline"),
        "{:?}",
        read.unconsumed
    );
    assert_eq!(chart.coverage, ChartCoverage::Partial);

    let bars = r#"<c:errBars><c:errBarType val="both"/><c:errValType val="stdErr"/></c:errBars>"#
        .repeat(MAX_CHART_ERROR_BARS + 1);
    let read = read_part(&space(&plot(&bar_series(&bars), ""), ""));
    let chart = read.projection.expect("a projection");
    let series = &chart.plot_area.groups[0].series[0];
    assert_eq!(series.error_bars.len(), MAX_CHART_ERROR_BARS);
    assert!(read.unconsumed.contains("errBars"), "{:?}", read.unconsumed);
    assert_eq!(chart.coverage, ChartCoverage::Partial);
}

/// **Malformed values open, unmodelled, and never reach the model.** A
/// trendline or error bars the model cannot hold is carried whole on its
/// series; a font value out of domain is left absent and its bytes kept.
#[test]
fn a_malformed_trendline_or_font_is_carried_never_thrown() {
    for (markup, why) in [
        (
            r#"<c:trendline><c:trendlineType val="poly"/><c:order val="9"/></c:trendline>"#,
            "order above 6",
        ),
        (
            r#"<c:trendline><c:trendlineType val="poly"/></c:trendline>"#,
            "a polynomial with no order",
        ),
        (
            r#"<c:trendline><c:trendlineType val="movingAvg"/><c:period val="1"/></c:trendline>"#,
            "period below 2",
        ),
        (
            r#"<c:trendline><c:trendlineType val="movingAvg"/><c:period val="256"/></c:trendline>"#,
            "period above 255",
        ),
        (
            r#"<c:trendline><c:trendlineType val="cubic"/></c:trendline>"#,
            "an unknown type",
        ),
        (
            r#"<c:trendline><c:trendlineType val="linear"/><c:intercept val="abc"/></c:trendline>"#,
            "a non-number",
        ),
        (
            r#"<c:trendline><c:dispEq val="1"/></c:trendline>"#,
            "no type",
        ),
        (
            r#"<c:errBars><c:errBarType val="both"/><c:errValType val="bogus"/></c:errBars>"#,
            "an unknown value type",
        ),
        (
            r#"<c:errBars><c:errBarType val="both"/></c:errBars>"#,
            "no value type",
        ),
        (
            r#"<c:errBars><c:errBarType val="both"/><c:errValType val="fixedVal"/><c:val val="NaN"/></c:errBars>"#,
            "a NaN length",
        ),
    ] {
        let read = read_part(&space(&plot(&bar_series(markup), ""), ""));
        let chart = read.projection.expect("a projection");
        let series = &chart.plot_area.groups[0].series[0];
        assert!(
            series.trendlines.is_empty() && series.error_bars.is_empty(),
            "{why}: must not reach the model"
        );
        assert_eq!(series.retained.len(), 1, "{why}: carried whole");
        assert_eq!(series.retained[0].xml, markup, "{why}: verbatim");
        assert_eq!(chart.coverage, ChartCoverage::Complete, "{why}");
    }

    for (txpr, why) in [
        (
            COMPLETE_TXPR.replace(r#"sz="1000""#, r#"sz="99""#),
            "size below 1 pt",
        ),
        (
            COMPLETE_TXPR.replace(r#"sz="1000""#, r#"sz="400001""#),
            "size above 4000 pt",
        ),
        (
            COMPLETE_TXPR.replace(r#"sz="1000""#, r#"sz="ten""#),
            "size not a number",
        ),
    ] {
        let read = read_part(&space(&plot(&bar_series(""), ""), &txpr));
        let chart = read.projection.expect("a projection");
        let font = chart.font.as_ref().expect("the rest of the font");
        assert_eq!(font.size, None, "{why}");
        assert_eq!(font.typeface.as_deref(), Some("Arial"), "{why}");
        assert_eq!(names(&chart.space_retained), ["txPr"], "{why}: bytes kept");
    }
    let long_face = "F".repeat(MAX_CHART_TYPEFACE_BYTES + 1);
    for (txpr, why) in [
        (
            COMPLETE_TXPR.replace("Arial", &long_face),
            "an overlong face",
        ),
        (COMPLETE_TXPR.replace("Arial", ""), "an empty face"),
    ] {
        let read = read_part(&space(&plot(&bar_series(""), ""), &txpr));
        let chart = read.projection.expect("a projection");
        let font = chart.font.as_ref().expect("the rest of the font");
        assert_eq!(font.typeface, None, "{why}");
        assert_eq!(font.size, Some(1000), "{why}");
        assert_eq!(names(&chart.space_retained), ["txPr"], "{why}: bytes kept");
    }
    let flag = COMPLETE_TXPR.replace(r#"b="1""#, r#"b="maybe""#);
    let chart = projection(&space(&plot(&bar_series(""), ""), &flag));
    assert_eq!(chart.font.as_ref().and_then(|font| font.bold), None);
    assert_eq!(names(&chart.space_retained), ["txPr"]);

    // A pie series has no place for a trendline: still a loss, as before.
    let pie = r#"<c:pieChart><c:varyColors val="1"/><c:ser><c:idx val="0"/><c:order val="0"/><c:trendline><c:trendlineType val="linear"/></c:trendline><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>1</c:v></c:pt></c:numLit></c:val></c:ser><c:firstSliceAng val="0"/></c:pieChart>"#;
    let read = read_part(&space(
        &format!("<c:plotArea><c:layout/>{pie}</c:plotArea>"),
        "",
    ));
    let chart = read.projection.expect("a projection");
    assert!(chart.plot_area.groups[0].series[0].trendlines.is_empty());
    assert_eq!(chart.coverage, ChartCoverage::Partial);
}
