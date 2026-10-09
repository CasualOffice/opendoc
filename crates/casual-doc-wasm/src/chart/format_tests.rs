//! Guards for chart formatting (`docs/155` §19): every view field the panel
//! reads, every patch field it writes, the shadows each edit drops, the
//! combination-chart regrouping and its secondary axes, and one Word chart
//! formatted, saved and reopened.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AxisKind, AxisPosition, Chart, ChartFont, ChartGroupKind, ChartLine, ChartText, ChartTitle,
    ChartXml, Color, DashStyle, DataLabelPosition, Definitions, ErrorBarType, ErrorBars,
    ErrorValueType, FontCollection, FontScheme, RgbColor, ThemeFontEntry, Trendline, TrendlineKind,
};
use casual_doc_transaction::{Coalesce, Origin};
use serde_json::Value;

use super::format::fonts_view;
use super::tests::{document_with_a_chart, format_edit, patch_of, projection, write};
use crate::WasmDocument;
use casual_doc_edit::Operation;

/// Replaces the one chart's projection with `edit` applied to it, as an
/// import would have produced it (clean, not dirty).
fn seed(d: &mut WasmDocument, edit: impl FnOnce(&mut Chart)) {
    let (id, mut chart) = d
        .document
        .definitions()
        .charts
        .iter()
        .map(|(id, chart)| (*id, chart.clone()))
        .next()
        .expect("a chart");
    edit(&mut chart);
    d.apply_group(
        &[Operation::SetChartDefinition {
            id,
            chart: Some(Box::new(chart)),
        }],
        "seed",
        Coalesce::New,
        Origin::Edit,
    )
    .expect("seeding the chart");
}

/// The JSON `chartData` returns, parsed.
fn view(d: &WasmDocument, object: NodeId) -> Value {
    serde_json::from_str(&d.chart_data(&object.to_string())).expect("the view parses")
}

fn fragment(name: &str, xml: &str) -> ChartXml {
    ChartXml {
        name: name.to_owned(),
        xml: xml.to_owned(),
    }
}

fn names(fragments: &[ChartXml]) -> Vec<&str> {
    fragments.iter().map(|f| f.name.as_str()).collect()
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(RgbColor { r, g, b })
}

fn title(text: &str) -> ChartTitle {
    ChartTitle {
        text: Some(ChartText {
            text: text.to_owned(),
            formula: None,
        }),
        overlay: false,
        font: None,
        retained: Vec::new(),
    }
}

/// The index of the axis with `kind`.
fn axis_of(chart: &Chart, kind: AxisKind) -> usize {
    chart
        .plot_area
        .axes
        .iter()
        .position(|axis| axis.kind == kind)
        .expect("an axis of that kind")
}

/// A format edit that must be refused with `code`.
fn refused(d: &mut WasmDocument, object: NodeId, format: &str, code: &str) {
    let before = projection(d);
    let refusal = format_edit(d, object, format).expect_err(format);
    assert!(
        refusal.contains(code),
        "{format} must be refused as {code}: {refusal}"
    );
    assert_eq!(projection(d), before, "a refused edit changed the chart");
}

/// The model the edit wrote is a valid document, and the edit made the chart
/// the authority over its source bytes.
fn assert_valid_and_dirty(d: &WasmDocument) {
    d.document
        .validate()
        .expect("the edited document validates");
    assert!(projection(d).dirty, "a format edit left the chart clean");
}

/// **Every new view field reports the model, inheritance resolved.**
#[test]
fn the_view_reports_fonts_axis_titles_formats_and_series() {
    let (mut d, object) = document_with_a_chart(9_400, "column");
    seed(&mut d, |chart| {
        chart.font = Some(ChartFont {
            typeface: Some("+mj-lt".to_owned()),
            size: Some(1_200),
            ..ChartFont::default()
        });
        chart.title = Some(ChartTitle {
            font: Some(ChartFont {
                bold: Some(true),
                ..ChartFont::default()
            }),
            ..title("Sales")
        });
        chart.auto_title_deleted = false;
        chart.legend.as_mut().expect("legend").font = Some(ChartFont {
            color: Some(rgb(0x11, 0x22, 0x33)),
            italic: Some(true),
            ..ChartFont::default()
        });
        let value = axis_of(chart, AxisKind::Value);
        chart.plot_area.axes[value].title = Some(title("Revenue"));
        chart.plot_area.axes[value].number_format = Some("0.0".to_owned());
        let series = &mut chart.plot_area.groups[0].series;
        series[1].fill = Some(rgb(0xFF, 0x88, 0x00));
        series[0].values.number_format = Some("0%".to_owned());
        series[0].trendlines = vec![Trendline {
            kind: TrendlineKind::Linear,
            display_equation: true,
            ..Trendline::default()
        }];
        series[0].error_bars = vec![ErrorBars {
            value_type: ErrorValueType::Percentage,
            value: Some("5".to_owned()),
            ..ErrorBars::default()
        }];
    });
    let view = view(&d, object);
    let format = &view["format"];
    let fonts = &format["fonts"];
    // Chart-wide: the heading theme face (no font scheme → Office's), 12 pt.
    assert_eq!(fonts["chart"]["typeface"], "Calibri Light");
    assert_eq!(fonts["chart"]["size"], 12.0);
    assert_eq!(fonts["chart"]["present"], true);
    // The title inherits the chart's face and size and adds bold.
    assert_eq!(fonts["title"]["bold"], true);
    assert_eq!(fonts["title"]["size"], 12.0);
    assert_eq!(fonts["title"]["typeface"], "Calibri Light");
    assert_eq!(fonts["title"]["color"], "#595959");
    assert_eq!(fonts["legend"]["color"], "#112233");
    assert_eq!(fonts["legend"]["italic"], true);
    assert_eq!(fonts["horizontalAxis"]["size"], 12.0);
    assert_eq!(fonts["verticalAxis"]["present"], true);
    assert_eq!(format["verticalAxis"]["title"], "Revenue");
    assert_eq!(format["verticalAxis"]["numberFormat"], "0.0");
    assert_eq!(format["horizontalAxis"]["title"], "");
    assert_eq!(format["horizontalAxis"]["numberFormat"], "");
    let series = format["series"].as_array().expect("series");
    assert_eq!(series.len(), 3);
    assert_eq!(series[0]["color"], "#4472C4", "series 1 paints accent 1");
    assert_eq!(series[1]["color"], "#FF8800", "series 2's explicit fill");
    assert_eq!(series[2]["color"], "#A5A5A5", "series 3 paints accent 3");
    assert_eq!(series[1]["name"], "Series 2");
    assert_eq!(series[0]["kind"], "column");
    assert_eq!(series[0]["secondary"], false);
    assert_eq!(series[0]["hasLine"], false);
    assert_eq!(series[0]["lineWidth"], 0.0);
    assert_eq!(series[0]["numberFormat"], "0%");
    assert_eq!(series[1]["numberFormat"], "");
    assert_eq!(series[0]["trendline"]["kind"], "linear");
    assert_eq!(series[0]["trendline"]["equation"], true);
    assert_eq!(series[0]["trendline"]["rSquared"], false);
    assert_eq!(series[1]["trendline"]["kind"], "none");
    assert_eq!(series[0]["errorBars"]["kind"], "percentage");
    assert_eq!(series[0]["errorBars"]["value"], "5");
    assert_eq!(series[0]["errorBars"]["type"], "both");
    assert_eq!(series[0]["admitsTrendline"], true);
    assert_eq!(series[0]["admitsErrorBars"], true);
    assert_eq!(format["combo"], false);
    assert_eq!(format["combinable"].as_array().map(Vec::len), Some(10));
    assert_eq!(format["dashes"].as_array().map(Vec::len), Some(11));
    assert_eq!(format["numberFormats"][0], "General");
    assert_eq!(format["numberFormats"].as_array().map(Vec::len), Some(10));
}

/// **Word's defaults, and an absent element, read as such.**
#[test]
fn an_unformatted_chart_reads_words_defaults() {
    let (d, object) = document_with_a_chart(9_401, "pie");
    let view = view(&d, object);
    let fonts = &view["format"]["fonts"];
    assert_eq!(fonts["chart"]["size"], 10.0);
    assert_eq!(fonts["chart"]["typeface"], "Calibri");
    assert_eq!(fonts["title"]["size"], 14.0);
    assert_eq!(fonts["title"]["present"], false, "a new chart has no title");
    assert_eq!(fonts["legend"]["size"], 9.0);
    assert_eq!(fonts["legend"]["present"], true);
    assert_eq!(fonts["verticalAxis"]["present"], false, "a pie has no axes");
    let series = &view["format"]["series"][0];
    assert_eq!(series["admitsTrendline"], false, "a pie takes no trendline");
    assert_eq!(series["admitsErrorBars"], false);
}

/// **A theme reference resolves to the document theme's own face.**
#[test]
fn a_theme_font_reference_resolves_to_the_documents_theme() {
    let (d, _) = document_with_a_chart(9_402, "column");
    let mut chart = projection(&d);
    chart.font = Some(ChartFont {
        typeface: Some("+mn-lt".to_owned()),
        ..ChartFont::default()
    });
    chart.title = Some(ChartTitle {
        font: Some(ChartFont {
            typeface: Some("+mj-lt".to_owned()),
            ..ChartFont::default()
        }),
        ..title("T")
    });
    let entry = |face: &str| FontCollection {
        latin: ThemeFontEntry {
            typeface: face.to_owned(),
            ..ThemeFontEntry::default()
        },
        ..FontCollection::default()
    };
    let definitions = Definitions {
        font_scheme: Some(FontScheme {
            major: entry("Aptos Display"),
            minor: entry("Aptos"),
        }),
        ..Definitions::default()
    };
    let fonts = fonts_view(&chart, &definitions);
    assert_eq!(fonts.chart.typeface, "Aptos");
    assert_eq!(fonts.title.typeface, "Aptos Display");
    assert_eq!(fonts.legend.typeface, "Aptos", "the legend inherits +mn-lt");
}

/// **A line series reports its line: width, dash and the colour it paints.**
#[test]
fn a_line_series_view_reports_its_line() {
    let (mut d, object) = document_with_a_chart(9_403, "line");
    seed(&mut d, |chart| {
        chart.plot_area.groups[0].series[0].line = Some(ChartLine {
            color: Some(rgb(0x00, 0xB0, 0x50)),
            width_emu: Some(28_575),
            no_fill: false,
            dash: Some(DashStyle::SystemDash),
        });
    });
    let view = view(&d, object);
    let series = &view["format"]["series"];
    assert_eq!(series[0]["hasLine"], true);
    assert_eq!(series[0]["lineWidth"], 2.25);
    assert_eq!(series[0]["dash"], "sysDash");
    assert_eq!(series[0]["color"], "#00B050");
    assert_eq!(series[1]["lineWidth"], 1.5, "the renderer's default weight");
    assert_eq!(series[1]["dash"], "solid");
    assert_eq!(series[1]["color"], "#ED7D31");
}

/// **A font edit writes the model, drops the verbatim text formatting it
/// supersedes, keeps what it does not, and reads back.**
#[test]
fn fonts_write_the_model_and_drop_carried_text_formatting() {
    let (mut d, object) = document_with_a_chart(9_404, "column");
    seed(&mut d, |chart| {
        chart.space_retained = vec![fragment("txPr", "<c:txPr><a:bodyPr/></c:txPr>")];
        chart.title = Some(ChartTitle {
            retained: vec![
                fragment("tx", "<c:tx><c:rich><a:bodyPr/></c:rich></c:tx>"),
                fragment("spPr", "<c:spPr/>"),
                fragment("txPr", "<c:txPr><a:bodyPr/></c:txPr>"),
            ],
            ..title("Sales")
        });
        chart.auto_title_deleted = false;
        chart.legend.as_mut().expect("legend").retained =
            vec![fragment("txPr", "<c:txPr><a:bodyPr/></c:txPr>")];
        let value = axis_of(chart, AxisKind::Value);
        chart.plot_area.axes[value].retained =
            vec![fragment("txPr", "<c:txPr><a:bodyPr/></c:txPr>")];
    });
    let mut patch = patch_of(&d, object);
    patch.title = "Sales".to_owned();
    patch.format = Some(
        serde_json::from_str(
            r##"{"fonts":{
                "chart":{"typeface":"Georgia","size":11},
                "title":{"bold":true,"color":"#C00000"},
                "legend":{"italic":true},
                "verticalAxis":{"size":8}
            }}"##,
        )
        .expect("a fonts patch"),
    );
    write(&mut d, object, &patch).expect("the fonts apply");
    let chart = projection(&d);
    assert_eq!(
        chart.font,
        Some(ChartFont {
            typeface: Some("Georgia".to_owned()),
            size: Some(1_100),
            ..ChartFont::default()
        })
    );
    assert!(chart.space_retained.is_empty(), "the chart txPr survived");
    let title = chart.title.as_ref().expect("title");
    assert_eq!(
        title.font,
        Some(ChartFont {
            bold: Some(true),
            color: Some(rgb(0xC0, 0, 0)),
            ..ChartFont::default()
        })
    );
    assert_eq!(names(&title.retained), ["spPr"], "tx and txPr are shadows");
    assert_eq!(
        title.text.as_ref().map(|text| text.text.as_str()),
        Some("Sales"),
        "the title text is kept"
    );
    let legend = chart.legend.as_ref().expect("legend");
    assert_eq!(
        legend.font.as_ref().and_then(|font| font.italic),
        Some(true)
    );
    assert!(legend.retained.is_empty());
    let value = &chart.plot_area.axes[axis_of(&chart, AxisKind::Value)];
    assert_eq!(value.font.as_ref().and_then(|font| font.size), Some(800));
    assert!(value.retained.is_empty());
    assert_valid_and_dirty(&d);

    let fonts = &view(&d, object)["format"]["fonts"];
    assert_eq!(fonts["chart"]["typeface"], "Georgia");
    assert_eq!(fonts["chart"]["size"], 11.0);
    assert_eq!(fonts["title"]["bold"], true);
    assert_eq!(fonts["title"]["color"], "#C00000");
    assert_eq!(fonts["title"]["typeface"], "Georgia", "inherited");
    assert_eq!(fonts["verticalAxis"]["size"], 8.0);
    assert_eq!(fonts["horizontalAxis"]["size"], 11.0, "inherited");

    // Empty and zero inherit again; a font that declares nothing is absent.
    format_edit(
        &mut d,
        object,
        r#"{"fonts":{"chart":{"typeface":"","size":0}}}"#,
    )
    .expect("inherit");
    assert_eq!(projection(&d).font, None);
    assert_valid_and_dirty(&d);
}

/// **Font values outside the model's domains are refused, naming the rule.**
#[test]
fn font_values_outside_their_domains_are_refused() {
    let (mut d, object) = document_with_a_chart(9_405, "column");
    let long = "F".repeat(200);
    for (format, code) in [
        (
            r#"{"fonts":{"chart":{"size":5000}}}"#.to_owned(),
            "chart.font-size",
        ),
        (
            r#"{"fonts":{"chart":{"size":-2}}}"#.to_owned(),
            "chart.font-size",
        ),
        (
            r#"{"fonts":{"chart":{"size":0.004}}}"#.to_owned(),
            "chart.font-size",
        ),
        (
            r#"{"fonts":{"legend":{"color":"red"}}}"#.to_owned(),
            "chart.color",
        ),
        (
            format!(r#"{{"fonts":{{"chart":{{"typeface":"{long}"}}}}}}"#),
            "chart.font-typeface",
        ),
        (
            r#"{"fonts":{"title":{"bold":true}}}"#.to_owned(),
            "chart.no-title",
        ),
    ] {
        refused(&mut d, object, &format, code);
    }
    format_edit(&mut d, object, r#"{"fonts":{"legend":{"size":12}}}"#).expect("a legend");
    let mut patch = patch_of(&d, object);
    patch.legend = "none".to_owned();
    patch.format =
        Some(serde_json::from_str(r#"{"fonts":{"legend":{"size":12}}}"#).expect("a patch"));
    let refusal = write(&mut d, object, &patch).expect_err("no legend, no legend font");
    assert!(refusal.contains("chart.no-legend"), "{refusal}");
}

/// **An axis title and an axis number format write the model; empty removes
/// them; a retitle drops the carried rich text but keeps its formatting.**
#[test]
fn axis_titles_and_number_formats_write_the_model() {
    let (mut d, object) = document_with_a_chart(9_406, "column");
    seed(&mut d, |chart| {
        let value = axis_of(chart, AxisKind::Value);
        chart.plot_area.axes[value].retained = vec![fragment(
            "numFmt",
            r#"<c:numFmt formatCode="General" sourceLinked="1"/>"#,
        )];
    });
    format_edit(
        &mut d,
        object,
        r##"{"verticalAxis":{"title":"Revenue","numberFormat":"#,##0"},"horizontalAxis":{"title":"Quarter"}}"##,
    )
    .expect("axis titles");
    let chart = projection(&d);
    let value = &chart.plot_area.axes[axis_of(&chart, AxisKind::Value)];
    let category = &chart.plot_area.axes[axis_of(&chart, AxisKind::Category)];
    let text = |axis: &casual_doc_model::v1::Axis| {
        axis.title
            .as_ref()
            .and_then(|title| title.text.as_ref())
            .map(|text| text.text.clone())
    };
    assert_eq!(text(value).as_deref(), Some("Revenue"));
    assert_eq!(text(category).as_deref(), Some("Quarter"));
    assert_eq!(value.number_format.as_deref(), Some("#,##0"));
    assert!(value.retained.is_empty(), "the carried numFmt survived");
    assert_valid_and_dirty(&d);
    let format = &view(&d, object)["format"];
    assert_eq!(format["verticalAxis"]["title"], "Revenue");
    assert_eq!(format["verticalAxis"]["numberFormat"], "#,##0");
    assert_eq!(format["horizontalAxis"]["title"], "Quarter");

    // A carried rich title: retitling drops its words, not its fill.
    seed(&mut d, |chart| {
        let value = axis_of(chart, AxisKind::Value);
        chart.plot_area.axes[value]
            .title
            .as_mut()
            .expect("title")
            .retained = vec![
            fragment("tx", "<c:tx><c:rich><a:bodyPr/></c:rich></c:tx>"),
            fragment("spPr", "<c:spPr/>"),
        ];
    });
    format_edit(&mut d, object, r#"{"verticalAxis":{"title":"Income"}}"#).expect("retitle");
    let chart = projection(&d);
    let value = &chart.plot_area.axes[axis_of(&chart, AxisKind::Value)];
    assert_eq!(text(value).as_deref(), Some("Income"));
    assert_eq!(
        names(&value.title.as_ref().expect("title").retained),
        ["spPr"]
    );

    format_edit(
        &mut d,
        object,
        r#"{"verticalAxis":{"title":"","numberFormat":""}}"#,
    )
    .expect("clear");
    let chart = projection(&d);
    let value = &chart.plot_area.axes[axis_of(&chart, AxisKind::Value)];
    assert_eq!(
        (value.title.clone(), value.number_format.clone()),
        (None, None)
    );

    refused(
        &mut d,
        object,
        "{\"verticalAxis\":{\"numberFormat\":\"0\\u0001\"}}",
        "chart.number-format",
    );
    let long = "x".repeat(2_000);
    refused(
        &mut d,
        object,
        &format!(r#"{{"verticalAxis":{{"title":"{long}"}}}}"#),
        "chart.name-too-long",
    );
}

/// **A series colour, width and dash write the model and drop the carried
/// `c:spPr`; automatic clears the colour.**
#[test]
fn series_colour_width_and_dash_write_the_model() {
    let (mut d, object) = document_with_a_chart(9_407, "line");
    seed(&mut d, |chart| {
        chart.plot_area.groups[0].series[0].retained = vec![fragment("spPr", "<c:spPr/>")];
    });
    format_edit(
        &mut d,
        object,
        r##"{"series":[{"index":0,"color":"#00B050","lineWidth":3,"dash":"dash"}]}"##,
    )
    .expect("series formatting");
    let series = projection(&d).plot_area.groups[0].series[0].clone();
    let green = rgb(0x00, 0xB0, 0x50);
    assert_eq!(series.fill, Some(green), "markers take the colour too");
    assert_eq!(
        series.line,
        Some(ChartLine {
            color: Some(green),
            width_emu: Some(38_100),
            no_fill: false,
            dash: Some(DashStyle::Dash),
        })
    );
    assert!(series.retained.is_empty(), "the carried spPr survived");
    assert_valid_and_dirty(&d);
    let read = &view(&d, object)["format"]["series"][0];
    assert_eq!(read["color"], "#00B050");
    assert_eq!(read["lineWidth"], 3.0);
    assert_eq!(read["dash"], "dash");

    format_edit(&mut d, object, r#"{"series":[{"index":0,"color":""}]}"#).expect("auto");
    let series = projection(&d).plot_area.groups[0].series[0].clone();
    assert_eq!(series.fill, None);
    assert_eq!(series.line.and_then(|line| line.color), None);
    assert_eq!(
        series.line.and_then(|line| line.width_emu),
        Some(38_100),
        "automatic colour keeps the width"
    );

    for (format, code) in [
        (
            r#"{"series":[{"index":0,"lineWidth":0.1}]}"#,
            "chart.line-width",
        ),
        (
            r#"{"series":[{"index":0,"lineWidth":25}]}"#,
            "chart.line-width",
        ),
        (
            r#"{"series":[{"index":0,"dash":"wiggly"}]}"#,
            "chart.dash-unknown",
        ),
        (r##"{"series":[{"index":0,"color":"#12"}]}"##, "chart.color"),
        (r#"{"series":[{"index":9,"color":""}]}"#, "chart.no-series"),
    ] {
        refused(&mut d, object, format, code);
    }

    let (mut column, object) = document_with_a_chart(9_408, "column");
    refused(
        &mut column,
        object,
        r#"{"series":[{"index":0,"lineWidth":2}]}"#,
        "chart.no-line",
    );
    format_edit(
        &mut column,
        object,
        r##"{"series":[{"index":2,"color":"#7030A0"}]}"##,
    )
    .expect("a bar colour");
    let series = projection(&column).plot_area.groups[0].series[2].clone();
    assert_eq!(series.fill, Some(rgb(0x70, 0x30, 0xA0)));
    assert_eq!(series.line, None, "a bar colour is a fill, not a line");
}

/// **A series number format is its values' format code.**
#[test]
fn a_series_number_format_is_its_values_format_code() {
    let (mut d, object) = document_with_a_chart(9_409, "column");
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"numberFormat":"0.00%"}]}"#,
    )
    .expect("a format");
    assert_eq!(
        projection(&d).plot_area.groups[0].series[1]
            .values
            .number_format
            .as_deref(),
        Some("0.00%")
    );
    assert_eq!(
        view(&d, object)["format"]["series"][1]["numberFormat"],
        "0.00%"
    );
    assert_valid_and_dirty(&d);
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"numberFormat":""}]}"#,
    )
    .expect("general");
    assert_eq!(
        projection(&d).plot_area.groups[0].series[1]
            .values
            .number_format,
        None
    );
}

/// **A trendline is validated against its family and its data, written, read
/// back, and removed.**
#[test]
fn trendlines_are_validated_written_and_removed() {
    let (mut d, object) = document_with_a_chart(9_410, "column");
    seed(&mut d, |chart| {
        chart.plot_area.groups[0].series[0].retained = vec![fragment(
            "trendline",
            "<c:trendline><c:trendlineType val=\"linear\"/></c:trendline>",
        )];
    });
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":0,"trendline":{"kind":"poly","order":3,"equation":true,"rSquared":true}}]}"#,
    )
    .expect("a polynomial trendline");
    let series = projection(&d).plot_area.groups[0].series[0].clone();
    assert_eq!(series.trendlines.len(), 1);
    let line = &series.trendlines[0];
    assert_eq!(
        (
            line.kind,
            line.order,
            line.display_equation,
            line.display_r_squared
        ),
        (TrendlineKind::Polynomial, Some(3), true, true)
    );
    assert!(series.retained.is_empty(), "the carried trendline survived");
    assert_valid_and_dirty(&d);
    let read = &view(&d, object)["format"]["series"][0]["trendline"];
    assert_eq!(read["kind"], "poly");
    assert_eq!(read["order"], 3);

    // Four points: a moving average over three is fine, over five is not.
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":0,"trendline":{"kind":"movingAvg","period":3,"equation":true}}]}"#,
    )
    .expect("a moving average");
    let line = projection(&d).plot_area.groups[0].series[0].trendlines[0].clone();
    assert_eq!(
        (line.kind, line.period, line.order),
        (TrendlineKind::MovingAverage, Some(3), None)
    );
    assert!(!line.display_equation, "a moving average has no equation");
    for (format, code) in [
        (
            r#"{"series":[{"index":0,"trendline":{"kind":"movingAvg","period":5}}]}"#,
            "chart.trendline-period",
        ),
        (
            r#"{"series":[{"index":0,"trendline":{"kind":"poly","order":7}}]}"#,
            "chart.trendline-order",
        ),
        (
            r#"{"series":[{"index":0,"trendline":{"kind":"cubic"}}]}"#,
            "chart.trendline-unknown",
        ),
    ] {
        refused(&mut d, object, format, code);
    }
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":0,"trendline":{"kind":"none"}}]}"#,
    )
    .expect("removed");
    assert!(
        projection(&d).plot_area.groups[0].series[0]
            .trendlines
            .is_empty()
    );

    // Stacked: no trendline (Word greys it out), error bars still allowed.
    let (mut stacked, object) = document_with_a_chart(9_411, "column-stacked");
    refused(
        &mut stacked,
        object,
        r#"{"series":[{"index":0,"trendline":{"kind":"linear"}}]}"#,
        "chart.no-trendline",
    );
    assert_eq!(
        view(&stacked, object)["format"]["series"][0]["admitsTrendline"],
        false
    );
    format_edit(
        &mut stacked,
        object,
        r#"{"series":[{"index":0,"errorBars":{"kind":"stdErr"}}]}"#,
    )
    .expect("error bars on a stacked chart");
}

/// **Error bars are validated, written with Word's defaults, and removed.**
#[test]
fn error_bars_are_validated_written_and_removed() {
    let (mut d, object) = document_with_a_chart(9_412, "column");
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"errorBars":{"kind":"fixedVal","value":"0.5","type":"plus"}}]}"#,
    )
    .expect("fixed error bars");
    let bars = projection(&d).plot_area.groups[0].series[1]
        .error_bars
        .clone();
    assert_eq!(bars.len(), 1);
    assert_eq!(
        (
            bars[0].value_type,
            bars[0].bar_type,
            bars[0].value.as_deref(),
            bars[0].direction
        ),
        (
            ErrorValueType::FixedValue,
            ErrorBarType::Plus,
            Some("0.5"),
            None
        )
    );
    assert_valid_and_dirty(&d);
    let read = &view(&d, object)["format"]["series"][1]["errorBars"];
    assert_eq!(read["kind"], "fixedVal");
    assert_eq!(read["type"], "plus");
    assert_eq!(read["value"], "0.5");

    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"errorBars":{"kind":"percentage"}}]}"#,
    )
    .expect("percentage, defaulted");
    let bars = projection(&d).plot_area.groups[0].series[1]
        .error_bars
        .clone();
    assert_eq!(bars.len(), 1, "the value bars are replaced, not added to");
    assert_eq!(bars[0].value.as_deref(), Some("5"), "Word's 5% default");

    for (format, code) in [
        (
            r#"{"series":[{"index":1,"errorBars":{"kind":"stdDev","value":"-1"}}]}"#,
            "chart.error-bars-value",
        ),
        (
            r#"{"series":[{"index":1,"errorBars":{"kind":"fixedVal","value":"abc"}}]}"#,
            "chart.error-bars-value",
        ),
        (
            r#"{"series":[{"index":1,"errorBars":{"kind":"cust"}}]}"#,
            "chart.error-bars-custom",
        ),
        (
            r#"{"series":[{"index":1,"errorBars":{"kind":"fixedVal","type":"sideways"}}]}"#,
            "chart.error-bars-unknown",
        ),
    ] {
        refused(&mut d, object, format, code);
    }
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"errorBars":{"kind":"none"}}]}"#,
    )
    .expect("removed");
    assert!(
        projection(&d).plot_area.groups[0].series[1]
            .error_bars
            .is_empty()
    );

    let (mut pie, object) = document_with_a_chart(9_413, "pie");
    refused(
        &mut pie,
        object,
        r#"{"series":[{"index":0,"trendline":{"kind":"linear"}}]}"#,
        "chart.no-trendline",
    );
    refused(
        &mut pie,
        object,
        r#"{"series":[{"index":0,"errorBars":{"kind":"stdErr"}}]}"#,
        "chart.no-error-bars",
    );
}

/// **A series on the secondary axis as a line makes a combination chart with
/// Word's secondary axis pair, and putting it back removes the pair.**
#[test]
fn a_combo_regroups_and_creates_then_removes_the_secondary_axes() {
    let (mut d, object) = document_with_a_chart(9_414, "column");
    let grid = patch_of(&d, object);
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"kind":"line","secondary":true}]}"#,
    )
    .expect("a combination chart");
    let chart = projection(&d);
    let groups = &chart.plot_area.groups;
    assert_eq!(groups.len(), 2, "column and line: {groups:?}");
    assert!(matches!(groups[0].kind, ChartGroupKind::Bar { .. }));
    assert!(matches!(groups[1].kind, ChartGroupKind::Line { .. }));
    let name = |series: &casual_doc_model::v1::Series| {
        series
            .name
            .as_ref()
            .map(|name| name.text.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        groups[0].series.iter().map(name).collect::<Vec<_>>(),
        ["Series 1", "Series 3"]
    );
    assert_eq!(
        groups[1].series.iter().map(name).collect::<Vec<_>>(),
        ["Series 2"]
    );
    assert_eq!(
        groups[0].axis_ids,
        [1, 2],
        "the bars stay on the primary pair"
    );
    assert_eq!(chart.plot_area.axes.len(), 4, "a secondary pair was added");
    let secondary: Vec<_> = groups[1]
        .axis_ids
        .iter()
        .map(|id| {
            chart
                .plot_area
                .axes
                .iter()
                .find(|axis| axis.id == *id)
                .expect("the group's axis exists")
        })
        .collect();
    assert_eq!(secondary.len(), 2);
    let (category, value) = (secondary[0], secondary[1]);
    assert!(
        category.kind == AxisKind::Category && category.deleted,
        "Word's secondary category axis is deleted: {category:?}"
    );
    assert_eq!(category.position, Some(AxisPosition::Bottom));
    assert_eq!(value.kind, AxisKind::Value);
    assert_eq!(value.position, Some(AxisPosition::Right));
    assert_eq!(value.cross_axis_id, Some(category.id));
    assert_eq!(category.cross_axis_id, Some(value.id));
    assert!(
        value
            .retained
            .iter()
            .any(|fragment| fragment.xml == r#"<c:crosses val="max"/>"#),
        "the secondary value axis crosses at the maximum"
    );
    let mut ids: Vec<u32> = chart.plot_area.axes.iter().map(|axis| axis.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4, "axis ids are unique");
    assert_valid_and_dirty(&d);

    let read = view(&d, object);
    assert_eq!(read["format"]["combo"], true);
    assert_eq!(read["format"]["series"][1]["kind"], "line");
    assert_eq!(read["format"]["series"][1]["secondary"], true);
    assert_eq!(read["format"]["series"][0]["secondary"], false);
    assert_eq!(
        read["kind"], "column",
        "the primary type is the first group's"
    );
    assert_eq!(
        read["format"]["verticalAxis"]["gridlines"], true,
        "the panel's vertical axis is still the primary one"
    );
    assert_eq!(read["editable"], true, "a combination chart is editable");
    let after = patch_of(&d, object);
    assert_eq!(
        (after.series, after.cells),
        (grid.series, grid.cells),
        "the grid keeps its column order across the regroup"
    );

    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"kind":"column","secondary":false}]}"#,
    )
    .expect("back to one chart type");
    let chart = projection(&d);
    assert_eq!(chart.plot_area.groups.len(), 1);
    assert_eq!(
        chart.plot_area.groups[0]
            .series
            .iter()
            .map(name)
            .collect::<Vec<_>>(),
        ["Series 1", "Series 2", "Series 3"]
    );
    assert_eq!(
        chart.plot_area.axes.len(),
        2,
        "the secondary pair was removed"
    );
    assert_eq!(view(&d, object)["format"]["combo"], false);
    assert_valid_and_dirty(&d);

    // Secondary alone (same type) is a combination too: a second bar group.
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":2,"secondary":true}]}"#,
    )
    .expect("a secondary column series");
    let chart = projection(&d);
    assert_eq!(chart.plot_area.groups.len(), 2);
    assert_eq!(chart.plot_area.axes.len(), 4);
    assert_valid_and_dirty(&d);
}

/// **A combination may only mix the families Word combines.**
#[test]
fn a_combination_refuses_families_that_cannot_combine() {
    let (mut d, object) = document_with_a_chart(9_415, "column");
    for (format, code) in [
        (
            r#"{"series":[{"index":0,"kind":"pie"}]}"#,
            "chart.not-combinable",
        ),
        (
            r#"{"series":[{"index":0,"kind":"scatter"}]}"#,
            "chart.not-combinable",
        ),
        (
            r#"{"series":[{"index":0,"kind":"bar"}]}"#,
            "chart.not-combinable",
        ),
        (
            r#"{"series":[{"index":0,"kind":"radar"}]}"#,
            "chart.unpainted-family",
        ),
        (
            r#"{"series":[{"index":0,"secondary":true},{"index":1,"secondary":true},{"index":2,"secondary":true}]}"#,
            "chart.all-secondary",
        ),
    ] {
        refused(&mut d, object, format, code);
    }
    let (mut pie, object) = document_with_a_chart(9_416, "pie");
    refused(
        &mut pie,
        object,
        r#"{"series":[{"index":0,"kind":"line"}]}"#,
        "chart.not-combinable",
    );
    let (mut bar, object) = document_with_a_chart(9_417, "bar");
    refused(
        &mut bar,
        object,
        r#"{"series":[{"index":0,"kind":"column"}]}"#,
        "chart.not-combinable",
    );
    // A no-op type on a family that cannot combine is not a combination.
    format_edit(
        &mut bar,
        object,
        r##"{"series":[{"index":0,"kind":"bar","color":"#112233"}]}"##,
    )
    .expect("a bar series keeping its own type");
}

/// **A data edit on a combination chart keeps every series in its group; a
/// new series joins the last one's group; removing the only secondary series
/// removes its group and its axes.**
#[test]
fn a_data_edit_on_a_combo_keeps_the_groups() {
    let (mut d, object) = document_with_a_chart(9_418, "column");
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"kind":"line","secondary":true}]}"#,
    )
    .expect("a combination chart");

    let mut patch = patch_of(&d, object);
    patch.cells[0][1] = "42".to_owned();
    patch.series.push("Series 4".to_owned());
    for row in &mut patch.cells {
        row.push("1".to_owned());
    }
    write(&mut d, object, &patch).expect("a data edit on a combo");
    let chart = projection(&d);
    assert_eq!(chart.plot_area.groups.len(), 2, "the combination survived");
    assert_eq!(
        chart.plot_area.groups[0].series.len(),
        3,
        "series 4 joined the bars"
    );
    assert_eq!(chart.plot_area.groups[1].series.len(), 1);
    assert_eq!(
        chart.plot_area.groups[1].series[0].values.points[0].1,
        casual_doc_model::v1::ChartValue::Number("42".to_owned()),
        "the line series' value was written into the line series"
    );
    assert_eq!(chart.plot_area.axes.len(), 4);
    assert_eq!(patch_of(&d, object).cells[0][1], "42");
    assert_valid_and_dirty(&d);

    // The grid is positional — column i keeps plot position i's group, as a
    // single-group chart's column keeps its formatting — so removing the
    // columns from the line onwards removes the line: its group and the
    // secondary pair go.
    let mut patch = patch_of(&d, object);
    patch.series.truncate(1);
    for row in &mut patch.cells {
        row.truncate(1);
    }
    write(&mut d, object, &patch).expect("removing the line series");
    let chart = projection(&d);
    assert_eq!(chart.plot_area.groups.len(), 1);
    assert_eq!(
        chart.plot_area.axes.len(),
        2,
        "the orphaned secondary pair stayed"
    );
    assert_valid_and_dirty(&d);

    // A change of chart type flattens a combination into one group.
    let mut patch = patch_of(&d, object);
    patch.series.push("Series 2".to_owned());
    for row in &mut patch.cells {
        row.push("2".to_owned());
    }
    write(&mut d, object, &patch).expect("a second series");
    format_edit(
        &mut d,
        object,
        r#"{"series":[{"index":1,"kind":"area","secondary":true}]}"#,
    )
    .expect("a combination again");
    let mut patch = patch_of(&d, object);
    patch.kind = "line".to_owned();
    write(&mut d, object, &patch).expect("a family change");
    let chart = projection(&d);
    assert_eq!(chart.plot_area.groups.len(), 1);
    assert_eq!(chart.plot_area.axes.len(), 2);
    assert_valid_and_dirty(&d);
}

/// **Data labels and a palette apply to every group of a combination.**
#[test]
fn labels_and_palettes_apply_across_groups() {
    let (mut d, object) = document_with_a_chart(9_419, "column");
    format_edit(&mut d, object, r#"{"series":[{"index":1,"kind":"line"}]}"#)
        .expect("a combination chart");
    format_edit(
        &mut d,
        object,
        r#"{"dataLabels":true,"labelPosition":"outsideEnd","palette":"mono-1"}"#,
    )
    .expect("labels and a palette");
    let chart = projection(&d);
    let bars = &chart.plot_area.groups[0];
    let line = &chart.plot_area.groups[1];
    assert!(bars.series.iter().all(|series| {
        series.data_labels.and_then(|labels| labels.position) == Some(DataLabelPosition::OutsideEnd)
    }));
    let line_labels = line.series[0]
        .data_labels
        .expect("the line series is labelled too");
    assert_eq!(
        line_labels.position, None,
        "a line cannot label outside end"
    );
    let fills: Vec<_> = super::plotted(&chart)
        .iter()
        .map(|series| series.fill)
        .collect();
    assert!(
        fills.iter().all(Option::is_some),
        "every group took the palette"
    );
    assert_ne!(fills[0], fills[1]);
    assert_eq!(view(&d, object)["format"]["palette"], "mono-1");
    assert_valid_and_dirty(&d);
}

/// **A data edit leaves a horizontal bar chart's value axis a value axis.**
///
/// Word writes a horizontal bar chart's category axis on the left and its
/// value axis at the bottom. The family-change rule that retypes the bottom
/// axis (category for every family but scatter) used to run on every data
/// edit, turning that value axis into a category axis.
#[test]
fn a_data_edit_keeps_a_horizontal_bar_charts_value_axis() {
    let (mut d, object) = document_with_a_chart(9_420, "bar");
    seed(&mut d, |chart| {
        for axis in &mut chart.plot_area.axes {
            axis.position = Some(if axis.kind == AxisKind::Value {
                AxisPosition::Bottom
            } else {
                AxisPosition::Left
            });
        }
    });
    let mut patch = patch_of(&d, object);
    patch.cells[0][0] = "8".to_owned();
    write(&mut d, object, &patch).expect("a data edit");
    let chart = projection(&d);
    let bottom = chart
        .plot_area
        .axes
        .iter()
        .find(|axis| axis.position == Some(AxisPosition::Bottom))
        .expect("the bottom axis");
    assert_eq!(bottom.kind, AxisKind::Value, "the value axis was retyped");
    assert_valid_and_dirty(&d);
}

/// The one-chart package `chart` makes, as Word lays it out.
fn word_package(chart: &str) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut parts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    parts.insert(
        "[Content_Types].xml".to_owned(),
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#.to_vec(),
    );
    parts.insert(
        "_rels/.rels".to_owned(),
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
    );
    let document = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><w:body><w:p><w:r><w:drawing><wp:inline><wp:extent cx="4572000" cy="2743200"/><wp:docPr id="1" name="Chart 1"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId5"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#;
    parts.insert("word/document.xml".to_owned(), document.to_vec());
    parts.insert(
        "word/_rels/document.xml.rels".to_owned(),
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/></Relationships>"#.to_vec(),
    );
    parts.insert(
        "word/charts/chart1.xml".to_owned(),
        chart.as_bytes().to_vec(),
    );
    casual_doc_export::write_package(&casual_doc_import::RetainedSource {
        main_document: document.to_vec(),
        parts,
    })
    .expect("the package zips")
}

/// A chart exactly as Word 2016 writes a default clustered column chart, two
/// categories, one series — a copy of `casual-doc-import`'s `pub(crate)`
/// `WORD_DEFAULT_CHART`, minus the workbook pointer this package has no part
/// for.
const WORD_DEFAULT_CHART: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:c16r2="http://schemas.microsoft.com/office/drawing/2015/06/chart" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:c14="http://schemas.microsoft.com/office/drawing/2007/8/2/chart"><c:date1904 val="0"/><c:lang val="en-US"/><c:roundedCorners val="0"/><mc:AlternateContent><mc:Choice Requires="c14"><c14:style val="102"/></mc:Choice><mc:Fallback><c:style val="2"/></mc:Fallback></mc:AlternateContent><c:chart><c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Series 1</c:v></c:pt></c:strCache></c:strRef></c:tx><c:spPr><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:invertIfNegative val="0"/><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$3</c:f><c:strCache><c:ptCount val="2"/><c:pt idx="0"><c:v>Category 1</c:v></c:pt><c:pt idx="1"><c:v>Category 2</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>4.3</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val><c:extLst><c:ext uri="{C3380CC4-5D6E-409C-BE32-E72D297353CC}" xmlns:c16="http://schemas.microsoft.com/office/drawing/2014/chart"><c16:uniqueId val="{00000000-0001-0000-0000-000000000000}"/></c:ext></c:extLst></c:ser><c:dLbls><c:showLegendKey val="0"/><c:showVal val="0"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="0"/><c:showBubbleSize val="0"/></c:dLbls><c:gapWidth val="219"/><c:overlap val="-27"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:spPr><a:noFill/><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="15000"/><a:lumOff val="85000"/></a:schemeClr></a:solidFill><a:round/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr rot="-60000000" spcFirstLastPara="1" vertOverflow="ellipsis" vert="horz" wrap="square" anchor="ctr" anchorCtr="1"/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900" b="0" i="0" u="none" strike="noStrike" kern="1200" baseline="0"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="65000"/><a:lumOff val="35000"/></a:schemeClr></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr><c:crossAx val="2"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/><c:noMultiLvlLbl val="0"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines><c:spPr><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="tx1"><a:lumMod val="15000"/><a:lumOff val="85000"/></a:schemeClr></a:solidFill><a:round/></a:ln><a:effectLst/></c:spPr></c:majorGridlines><c:numFmt formatCode="General" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900"/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr><c:crossAx val="1"/><c:crosses val="autoZero"/><c:crossBetween val="between"/></c:valAx><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900"/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr></c:legend><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln><a:effectLst/></c:spPr><c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr/></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr></c:chartSpace>"#;

/// The one chart object in a document opened from bytes.
fn the_chart(d: &WasmDocument) -> NodeId {
    d.document
        .definitions()
        .charts
        .iter()
        .map(|(_, chart)| chart.object)
        .next()
        .expect("the Word chart is projected")
}

/// Saves `d` and opens the saved bytes again.
fn saved_and_reopened(d: &mut WasmDocument) -> WasmDocument {
    let bytes = d
        .export_as_inner(casual_doc_io::formats::DOCX, "preserve_when_safe")
        .expect("the save exports")
        .bytes;
    crate::open_document(&bytes).expect("the saved file reopens")
}

/// **A Word chart formatted here keeps what the writer already speaks across
/// a save and a reopen:** a series colour, a series and an axis number format,
/// and a combination with a secondary axis.
#[test]
fn a_formatted_word_chart_survives_a_save_and_a_reopen() {
    let mut d = crate::open_document(&word_package(WORD_DEFAULT_CHART)).expect("opens");
    let object = the_chart(&d);
    assert_eq!(
        view(&d, object)["editable"],
        true,
        "a Word chart is editable"
    );
    // A second series, so one can move to a secondary line.
    let mut patch = patch_of(&d, object);
    patch.series.push("Series 2".to_owned());
    for row in &mut patch.cells {
        row.push("3".to_owned());
    }
    write(&mut d, object, &patch).expect("a second series");
    format_edit(
        &mut d,
        object,
        r##"{"series":[{"index":0,"color":"#C00000","numberFormat":"0.0"},{"index":1,"kind":"line","secondary":true}],"verticalAxis":{"numberFormat":"#,##0.00"}}"##,
    )
    .expect("the format edit");

    let reopened = saved_and_reopened(&mut d);
    let object = the_chart(&reopened);
    let read = view(&reopened, object);
    let format = &read["format"];
    assert_eq!(format["series"][0]["color"], "#C00000", "{format}");
    assert_eq!(format["series"][0]["numberFormat"], "0.0");
    assert_eq!(format["verticalAxis"]["numberFormat"], "#,##0.00");
    assert_eq!(format["combo"], true, "the combination survived");
    assert_eq!(format["series"][1]["kind"], "line");
    assert_eq!(format["series"][1]["secondary"], true);
    assert_eq!(read["editable"], true, "and it reopens editable");
}

/// **The rest of the formatting survives a save and a reopen**: fonts, axis
/// titles, a dash, a trendline and error bars. Each needs the chart importer
/// and writer to speak the new model fields (the import and export lanes of
/// `docs/155` §19).
#[test]
fn fonts_titles_dashes_trendlines_and_error_bars_survive_a_save_and_a_reopen() {
    let mut d = crate::open_document(&word_package(WORD_DEFAULT_CHART)).expect("opens");
    let object = the_chart(&d);
    let mut patch = patch_of(&d, object);
    patch.title = "Sales".to_owned();
    patch.format = Some(
        serde_json::from_str(
            r##"{
                "fonts":{"title":{"bold":true,"size":16},"legend":{"color":"#1F4E79"},"chart":{"typeface":"Georgia"}},
                "verticalAxis":{"title":"Revenue"},
                "series":[{"index":0,"trendline":{"kind":"linear","equation":true},"errorBars":{"kind":"percentage","value":"10"}}]
            }"##,
        )
        .expect("a patch"),
    );
    write(&mut d, object, &patch).expect("the format edit");
    let reopened = saved_and_reopened(&mut d);
    let object = the_chart(&reopened);
    let format = &view(&reopened, object)["format"];
    assert_eq!(format["fonts"]["title"]["bold"], true);
    assert_eq!(format["fonts"]["title"]["size"], 16.0);
    assert_eq!(format["fonts"]["legend"]["color"], "#1F4E79");
    assert_eq!(format["fonts"]["chart"]["typeface"], "Georgia");
    assert_eq!(format["verticalAxis"]["title"], "Revenue");
    assert_eq!(format["series"][0]["trendline"]["kind"], "linear");
    assert_eq!(format["series"][0]["trendline"]["equation"], true);
    assert_eq!(format["series"][0]["errorBars"]["kind"], "percentage");
    assert_eq!(format["series"][0]["errorBars"]["value"], "10");
}
