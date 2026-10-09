// SPDX-License-Identifier: Apache-2.0

//! Chart **formatting** writers (`docs/155` §19): text fonts as `c:txPr` and
//! rich-text run properties, line dashes, trendlines and error bars.
//!
//! A child of `chart` so it shares the part writer's private vocabulary
//! (`Carry`, `Drops`, the value and colour writers) without widening it; split
//! out to keep `chart.rs` under the repository's file-size ceiling (`SKILL`
//! §10). Every writer here states its ECMA-376 sequence beside it, for the
//! same reason the parent does.
//!
//! Complexity: O(the elements written), once per regenerated chart.

use std::io::Cursor;

use casual_doc_model::strip_xml_forbidden;
use casual_doc_model::v1::{ChartContainer, chart_child_rank};
use casual_doc_model::v1::{ChartFont, ChartGroupKind, ChartLine, Color, DashStyle};
use casual_doc_model::v1::{ErrorBarDirection, ErrorBarType, ErrorBars, ErrorValueType};
use casual_doc_model::v1::{Trendline, TrendlineKind};
use quick_xml::Writer;
use quick_xml::events::{BytesEnd, Event};

use super::{
    Carry, Drops, bool_val, raw, write_num_source, write_shape_properties, write_solid_fill,
    write_text_element, write_val,
};
use crate::ExportError;
use crate::semantic::{pkg, start};

/// `a:bodyPr`, rotated a quarter turn counter-clockwise when `vertical` — the
/// `rot="-5400000" vert="horz"` pair Word writes on a vertical axis title.
pub(super) fn write_body_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    vertical: bool,
) -> Result<(), ExportError> {
    let mut body = start("a:bodyPr");
    if vertical {
        body.push_attribute(("rot", "-5400000"));
        body.push_attribute(("vert", "horz"));
    }
    w.write_event(Event::Empty(body)).map_err(pkg)
}

/// A container's `c:txPr`: the carried one when the importer kept it (it holds
/// what [`ChartFont`] does not model — East Asian and complex-script faces,
/// underline, kerning, a rotated body), else one generated from `font`, else
/// nothing.
///
/// The carried copy wins over the model's font because the facade drops it the
/// moment the reader edits the font; while it is here it agrees with the model
/// and says more. So: carried → carried; a font and nothing carried →
/// generated; neither → no element (an empty `c:txPr` says what its absence
/// already says).
///
/// Generated shape (`CT_TextBody`): `a:bodyPr`, `a:lstStyle`, one `a:p` whose
/// `a:pPr/a:defRPr` holds the font and whose `a:endParaRPr` closes it, as Word
/// writes every chart `c:txPr`.
pub(super) fn write_text_properties(
    w: &mut Writer<Cursor<Vec<u8>>>,
    carry: &Carry<'_>,
    font: Option<&ChartFont>,
    vertical: bool,
) -> Result<(), ExportError> {
    if let Some(fragment) = carry.shadow("txPr") {
        return raw(w, &fragment.xml);
    }
    let Some(font) = font.filter(|font| !font.is_empty()) else {
        return Ok(());
    };
    w.write_event(Event::Start(start("c:txPr"))).map_err(pkg)?;
    write_body_properties(w, vertical)?;
    w.write_event(Event::Empty(start("a:lstStyle")))
        .map_err(pkg)?;
    w.write_event(Event::Start(start("a:p"))).map_err(pkg)?;
    w.write_event(Event::Start(start("a:pPr"))).map_err(pkg)?;
    write_run_font(w, "a:defRPr", font)?;
    w.write_event(Event::End(BytesEnd::new("a:pPr")))
        .map_err(pkg)?;
    let mut end = start("a:endParaRPr");
    end.push_attribute(("lang", "en-US"));
    w.write_event(Event::Empty(end)).map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("a:p")))
        .map_err(pkg)?;
    w.write_event(Event::End(BytesEnd::new("c:txPr")))
        .map_err(pkg)
}

/// One `CT_TextCharacterProperties` element (`a:defRPr` or `a:rPr`) from a
/// [`ChartFont`].
///
/// Attributes `sz` (hundredths of a point), `b`, `i`; children in the
/// schema's sequence — `ln?`, the fill choice, the effect choice,
/// `highlight?`, `uLnTx|uLn`, `uFillTx|uFill`, **`latin?`**, `ea?`, `cs?`,
/// `sym?`, `hlinkClick?`… — so the colour's `a:solidFill` precedes
/// `a:latin`. The colour goes through the same writer as a series fill, so a
/// theme colour stays an `a:schemeClr` and [`Color::Auto`] writes no fill
/// (the consumer resolves it, which is what "automatic" means).
pub(super) fn write_run_font(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    font: &ChartFont,
) -> Result<(), ExportError> {
    let mut element = start(name);
    let size = font.size.map(|size| size.to_string());
    if let Some(size) = &size {
        element.push_attribute(("sz", size.as_str()));
    }
    if let Some(bold) = font.bold {
        element.push_attribute(("b", bool_val(bold)));
    }
    if let Some(italic) = font.italic {
        element.push_attribute(("i", bool_val(italic)));
    }
    let color = font
        .color
        .as_ref()
        .filter(|color| !matches!(color, Color::Auto));
    let typeface = font
        .typeface
        .as_deref()
        .map(strip_xml_forbidden)
        .filter(|face| !face.is_empty());
    if color.is_none() && typeface.is_none() {
        return w.write_event(Event::Empty(element)).map_err(pkg);
    }
    w.write_event(Event::Start(element)).map_err(pkg)?;
    if let Some(color) = color {
        write_solid_fill(w, color)?;
    }
    if let Some(typeface) = &typeface {
        let mut latin = start("a:latin");
        latin.push_attribute(("typeface", &**typeface));
        w.write_event(Event::Empty(latin)).map_err(pkg)?;
    }
    w.write_event(Event::End(BytesEnd::new(name))).map_err(pkg)
}

/// Writes one typed series element ([`write_trendline`], [`write_error_bars`]).
pub(super) type ElementWriter<T> =
    fn(&mut Writer<Cursor<Vec<u8>>>, &T, &mut Drops) -> Result<(), ExportError>;

/// The typed `c:trendline`s or `c:errBars` of one series, at the slot the
/// caller has reached — or, when the model holds none, the ones a reader that
/// did not type them carried verbatim.
///
/// # Which families admit them, and how many
///
/// The family's own `CT_*Ser` sequence decides: `CT_PieSer` (pie and
/// doughnut) has neither element, so a pie series' trendline has nowhere to
/// go and is dropped and reported by element rather than written somewhere
/// Word would discard the chart for. `c:errBars` is `maxOccurs="1"` in
/// `CT_BarSer` and `CT_LineSer` and `2` in `CT_AreaSer` and `CT_ScatterSer`
/// (one per direction); a set beyond that is dropped and reported the same
/// way. `c:trendline` is unbounded where it is admitted.
///
/// # Typed supersedes carried
///
/// Writing both the typed list and a carried copy of the same element would
/// duplicate it (and break `maxOccurs` for error bars), so a carried fragment
/// of the name is written only while the typed list is empty, and is dropped
/// and counted otherwise.
///
/// Complexity: O(items + carried fragments).
pub(super) fn write_series_extras<T>(
    w: &mut Writer<Cursor<Vec<u8>>>,
    carry: &Carry<'_>,
    kind: ChartGroupKind,
    name: &'static str,
    items: &[T],
    dropped: &mut Drops,
    write: ElementWriter<T>,
) -> Result<(), ExportError> {
    if items.is_empty() {
        for fragment in carry.carried(name) {
            raw(w, &fragment.xml)?;
        }
        return Ok(());
    }
    dropped.fragments += carry.carried(name).count();
    if chart_child_rank(ChartContainer::Series(kind), name).is_none() {
        dropped.element(name);
        return Ok(());
    }
    let limit = if name == "errBars" {
        match kind {
            ChartGroupKind::Bar { .. } | ChartGroupKind::Line { .. } => 1,
            _ => 2,
        }
    } else {
        usize::MAX
    };
    for item in items.iter().take(limit) {
        write(w, item, dropped)?;
    }
    if items.len() > limit {
        dropped.element(name);
    }
    Ok(())
}

/// One `c:trendline`.
///
/// `CT_Trendline` sequence: `name?`, `spPr?`, `trendlineType`, `order?`,
/// `period?`, `forward?`, `backward?`, `intercept?`, `dispRSqr?`, `dispEq?`,
/// `trendlineLbl?`, `extLst?`.
///
/// `dispRSqr` and `dispEq` are always written with an explicit value: the
/// schema default of a `CT_Boolean` is TRUE, so an absent `val` and an absent
/// element mean different things, and Word itself always writes both. Word
/// shows the R² or the equation only through a `c:trendlineLbl`; when either
/// is on and none was carried, a minimal one (`General`, not source-linked) is
/// written so the label actually appears.
pub(super) fn write_trendline(
    w: &mut Writer<Cursor<Vec<u8>>>,
    trendline: &Trendline,
    dropped: &mut Drops,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:trendline")))
        .map_err(pkg)?;
    let mut carry = Carry::new(
        ChartContainer::Trendline,
        &trendline.retained,
        &["spPr", "trendlineLbl"],
        &mut dropped.fragments,
    );
    carry.before(w, "name")?;
    if let Some(name) = &trendline.name {
        write_text_element(w, "c:name", name)?;
    }
    carry.before(w, "spPr")?;
    match carry.shadow("spPr") {
        Some(fragment) => raw(w, &fragment.xml)?,
        None => write_shape_properties(w, None, trendline.line.as_ref())?,
    }
    carry.before(w, "trendlineType")?;
    write_val(
        w,
        "c:trendlineType",
        match trendline.kind {
            TrendlineKind::Linear => "linear",
            TrendlineKind::Exponential => "exp",
            TrendlineKind::Logarithmic => "log",
            TrendlineKind::Polynomial => "poly",
            TrendlineKind::Power => "power",
            TrendlineKind::MovingAverage => "movingAvg",
        },
    )?;
    if let Some(order) = trendline.order {
        write_val(w, "c:order", &order.to_string())?;
    }
    if let Some(period) = trendline.period {
        write_val(w, "c:period", &period.to_string())?;
    }
    for (element, value) in [
        ("c:forward", &trendline.forward),
        ("c:backward", &trendline.backward),
        ("c:intercept", &trendline.intercept),
    ] {
        if let Some(value) = value {
            write_val(w, element, &strip_xml_forbidden(value))?;
        }
    }
    carry.before(w, "dispRSqr")?;
    write_val(w, "c:dispRSqr", bool_val(trendline.display_r_squared))?;
    write_val(w, "c:dispEq", bool_val(trendline.display_equation))?;
    carry.before(w, "trendlineLbl")?;
    // Present by the MODEL: a label with nothing to display is not written,
    // whatever was carried, the gridlines rule.
    if trendline.display_r_squared || trendline.display_equation {
        match carry.shadow("trendlineLbl") {
            Some(fragment) => raw(w, &fragment.xml)?,
            None => {
                // `CT_TrendlineLbl`: layout?, tx?, numFmt?, spPr?, txPr?.
                w.write_event(Event::Start(start("c:trendlineLbl")))
                    .map_err(pkg)?;
                let mut format = start("c:numFmt");
                format.push_attribute(("formatCode", "General"));
                format.push_attribute(("sourceLinked", "0"));
                w.write_event(Event::Empty(format)).map_err(pkg)?;
                w.write_event(Event::End(BytesEnd::new("c:trendlineLbl")))
                    .map_err(pkg)?;
            }
        }
    }
    carry.rest(w)?;
    w.write_event(Event::End(BytesEnd::new("c:trendline")))
        .map_err(pkg)
}

/// One `c:errBars`.
///
/// `CT_ErrBars` sequence: `errDir?`, `errBarType`, `errValType`, `noEndCap?`,
/// `plus?`, `minus?`, `val?`, `spPr?`, `extLst?`.
///
/// Custom lengths (`c:plus`/`c:minus`, `CT_NumDataSource`) go through the
/// series' own range writer: a `c:numLit` for a range with no formula and a
/// `c:numRef` with its cache for one that names cells. The values-only
/// workbook (`chart_workbook`) does not hold error-bar columns, so it never
/// gives these a formula — and it declines to bind a chart whose error bars
/// already name one, because that formula points into a workbook it would
/// replace.
///
/// `noEndCap` is written explicitly for the same `CT_Boolean`-defaults-true
/// reason as a trendline's flags.
pub(super) fn write_error_bars(
    w: &mut Writer<Cursor<Vec<u8>>>,
    bars: &ErrorBars,
    dropped: &mut Drops,
) -> Result<(), ExportError> {
    w.write_event(Event::Start(start("c:errBars")))
        .map_err(pkg)?;
    let mut carry = Carry::new(
        ChartContainer::ErrorBars,
        &bars.retained,
        &["spPr"],
        &mut dropped.fragments,
    );
    if let Some(direction) = bars.direction {
        write_val(
            w,
            "c:errDir",
            match direction {
                ErrorBarDirection::X => "x",
                ErrorBarDirection::Y => "y",
            },
        )?;
    }
    write_val(
        w,
        "c:errBarType",
        match bars.bar_type {
            ErrorBarType::Both => "both",
            ErrorBarType::Minus => "minus",
            ErrorBarType::Plus => "plus",
        },
    )?;
    write_val(
        w,
        "c:errValType",
        match bars.value_type {
            ErrorValueType::Custom => "cust",
            ErrorValueType::FixedValue => "fixedVal",
            ErrorValueType::Percentage => "percentage",
            ErrorValueType::StandardDeviation => "stdDev",
            ErrorValueType::StandardError => "stdErr",
        },
    )?;
    write_val(w, "c:noEndCap", bool_val(bars.no_end_cap))?;
    carry.before(w, "plus")?;
    if let Some(plus) = &bars.plus {
        write_num_source(w, "c:plus", plus)?;
    }
    carry.before(w, "minus")?;
    if let Some(minus) = &bars.minus {
        write_num_source(w, "c:minus", minus)?;
    }
    carry.before(w, "val")?;
    if let Some(value) = &bars.value {
        write_val(w, "c:val", &strip_xml_forbidden(value))?;
    }
    carry.before(w, "spPr")?;
    match carry.shadow("spPr") {
        Some(fragment) => raw(w, &fragment.xml)?,
        None => write_shape_properties(w, None, bars.line.as_ref())?,
    }
    carry.rest(w)?;
    w.write_event(Event::End(BytesEnd::new("c:errBars")))
        .map_err(pkg)
}

/// One `a:ln`.
///
/// `CT_LineProperties` sequence: the fill choice (`noFill` | `solidFill` |
/// `gradFill` | `pattFill`), the dash choice (`prstDash` | `custDash`), the
/// join choice (`round` | `bevel` | `miter`), `headEnd?`, `tailEnd?`,
/// `extLst?` — so `a:prstDash` follows the fill and nothing else the model
/// holds comes after it. The same writer serves a series, a trendline and an
/// error-bar line.
pub(super) fn write_line(
    w: &mut Writer<Cursor<Vec<u8>>>,
    line: &ChartLine,
) -> Result<(), ExportError> {
    let mut element = start("a:ln");
    let width = line.width_emu.map(|emu| emu.to_string());
    if let Some(width) = &width {
        element.push_attribute(("w", width.as_str()));
    }
    let color = line
        .color
        .as_ref()
        .filter(|color| !line.no_fill && !matches!(color, Color::Auto));
    if !line.no_fill && color.is_none() && line.dash.is_none() {
        return w.write_event(Event::Empty(element)).map_err(pkg);
    }
    w.write_event(Event::Start(element)).map_err(pkg)?;
    if line.no_fill {
        w.write_event(Event::Empty(start("a:noFill")))
            .map_err(pkg)?;
    } else if let Some(color) = color {
        write_solid_fill(w, color)?;
    }
    if let Some(dash) = line.dash {
        write_val(w, "a:prstDash", dash_token(dash))?;
    }
    w.write_event(Event::End(BytesEnd::new("a:ln")))
        .map_err(pkg)
}

/// The `a:prstDash@val` (`ST_PresetLineDashVal`) token for a [`DashStyle`].
const fn dash_token(dash: DashStyle) -> &'static str {
    match dash {
        DashStyle::Solid => "solid",
        DashStyle::Dot => "dot",
        DashStyle::Dash => "dash",
        DashStyle::LargeDash => "lgDash",
        DashStyle::DashDot => "dashDot",
        DashStyle::LargeDashDot => "lgDashDot",
        DashStyle::LargeDashDotDot => "lgDashDotDot",
        DashStyle::SystemDash => "sysDash",
        DashStyle::SystemDot => "sysDot",
        DashStyle::SystemDashDot => "sysDashDot",
        DashStyle::SystemDashDotDot => "sysDashDotDot",
    }
}
