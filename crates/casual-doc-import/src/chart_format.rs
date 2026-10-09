//! `docs/155` §19 reading: chart text fonts (`c:txPr`, a title's rich text),
//! trendlines (`c:trendline`) and error bars (`c:errBars`).
//!
//! A child of `chart` so the parser's walk, shadows and carry rules are the
//! same ones (it extends `Parser` rather than duplicating it), and a separate
//! file so `chart.rs` stays under the `SKILL` §10 size line. Nothing here
//! throws: a value the model cannot hold leaves the field absent and keeps the
//! source bytes, and a trendline or error bars the model cannot hold at all is
//! carried whole on its series.

use super::*;

/// Which modelled element owns the `c:spPr` being read: a series fills and
/// strokes, a trendline or an error bar only strokes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineOwner {
    Series,
    Trendline,
    ErrorBars,
}

/// A text body being read into a [`ChartFont`] (`docs/155` §19).
///
/// A `c:txPr` declares its font on the first paragraph's `a:defRPr`; a title's
/// `c:rich` does the same and its first run's `a:rPr` overrides it, which is
/// what Word shows in the title's Font box. Anything else that formats the
/// text is recorded through `record_unconsumed` while the enclosing shadow is
/// open, which marks the shadow lossy so its verbatim bytes are kept.
#[derive(Default)]
pub(super) struct FontDraft {
    /// Whether this is a title's `c:rich` (which holds runs and text) rather
    /// than a `c:txPr` (which holds only defaults).
    pub(super) rich: bool,
    /// Paragraphs opened so far.
    pub(super) paragraphs: u32,
    /// Whether a run's `a:rPr` has already been read.
    pub(super) saw_run_properties: bool,
    /// Whether the first paragraph's `a:defRPr` has already been read.
    pub(super) saw_defaults: bool,
    /// The first paragraph's `a:defRPr`.
    pub(super) defaults: ChartFont,
    /// The first run's `a:rPr`.
    pub(super) run: Option<ChartFont>,
    /// Whether the open run properties are the defaults (`true`) or the run's.
    pub(super) in_defaults: bool,
}

impl FontDraft {
    pub(super) fn new(rich: bool) -> Self {
        Self {
            rich,
            ..Self::default()
        }
    }

    /// The open run properties' font.
    pub(super) fn current(&mut self) -> &mut ChartFont {
        if self.in_defaults {
            &mut self.defaults
        } else {
            self.run.get_or_insert_with(ChartFont::default)
        }
    }

    /// The resolved font: the run's over the paragraph defaults.
    pub(super) fn resolve(self) -> ChartFont {
        match self.run {
            Some(run) => run.over(Some(&self.defaults)),
            None => self.defaults,
        }
    }
}

/// A `c:trendline` being read, with what decides whether it can be modelled.
pub(super) struct TrendlineDraft {
    pub(super) line: Trendline,
    /// Byte offset of its `<`, so a trendline the model cannot hold is carried
    /// whole on its series instead.
    pub(super) start: usize,
    /// A value the model cannot hold was seen (an order outside 2..=6, an
    /// unknown type, a malformed number, …).
    pub(super) malformed: bool,
    /// `c:trendlineType` was seen — the schema requires it.
    pub(super) saw_kind: bool,
}

/// A `c:errBars` being read; see [`TrendlineDraft`].
pub(super) struct ErrorBarsDraft {
    pub(super) bars: ErrorBars,
    pub(super) start: usize,
    pub(super) malformed: bool,
    /// `c:errBarType` and `c:errValType` were seen — the schema requires both.
    pub(super) saw_bar_type: bool,
    pub(super) saw_value_type: bool,
}

impl Parser {
    /// What the traversal does with an element inside a text body, run
    /// properties, a trendline or error bars. The caller has already applied
    /// `chart_noop`.
    pub(super) fn on_format_start(
        &mut self,
        scope: Scope,
        local: &[u8],
        element: &BytesStart<'_>,
    ) -> Result<Step, ChartDecline> {
        match (scope, local) {
            // ---- a DrawingML text body (`c:txPr`, a title's `c:rich`) ----
            //
            // Every arm that sees something the font cannot say records it, and
            // since a text body is only ever read inside a shadow (the title's
            // `c:tx` or the `c:txPr` itself) that marks the shadow lossy and
            // keeps its bytes, rather than naming a loss.
            (Scope::TextBody, b"bodyPr" | b"lstStyle") => {
                // A vertical axis title's quarter turn is what the writer
                // generates for one, so it is the default there, not a loss.
                let default_rotation =
                    local == b"bodyPr" && self.is_vertical_title_rotation(element);
                if has_attributes(element) && !default_rotation {
                    self.record_unconsumed(local);
                }
                // Their children (autofit, list levels) are formatting the
                // font does not hold: each is recorded by `Transparent`.
                Ok(Step::Push(Scope::Transparent))
            }
            // `<a:endParaRPr lang="en-US"/>` closes every paragraph Word and
            // the writer generate; it formats no visible text. Anything more
            // (another language, run formatting) is kept.
            (Scope::TextBody, b"endParaRPr") => {
                let mut attributes = element.attributes();
                let only_default_lang = matches!(
                    (attributes.next(), attributes.next()),
                    (Some(Ok(lang)), None)
                        if lang.key.as_ref() == b"lang" && lang.value.as_ref() == b"en-US"
                );
                if !only_default_lang {
                    self.record_unconsumed(local);
                }
                Ok(Step::Push(Scope::Transparent))
            }
            (Scope::TextBody, b"p") => {
                let later = self.font.as_mut().is_some_and(|font| {
                    font.paragraphs += 1;
                    font.paragraphs > 1
                });
                if later || has_attributes(element) {
                    self.record_unconsumed(local);
                }
                Ok(Step::Push(Scope::TextBody))
            }
            (Scope::TextBody, b"pPr") => {
                if has_attributes(element) {
                    self.record_unconsumed(local);
                }
                Ok(Step::Push(Scope::TextBody))
            }
            (Scope::TextBody, b"defRPr")
                if self
                    .font
                    .as_ref()
                    .is_some_and(|font| font.paragraphs == 1 && !font.saw_defaults) =>
            {
                if let Some(font) = self.font.as_mut() {
                    font.saw_defaults = true;
                    font.in_defaults = true;
                }
                self.read_run_attributes(local, element);
                Ok(Step::Push(Scope::RunProperties))
            }
            (Scope::TextBody, b"rPr")
                if self.font.as_ref().is_some_and(|font| {
                    font.rich && font.paragraphs == 1 && !font.saw_run_properties
                }) =>
            {
                if let Some(font) = self.font.as_mut() {
                    font.saw_run_properties = true;
                    font.in_defaults = false;
                }
                self.read_run_attributes(local, element);
                Ok(Step::Push(Scope::RunProperties))
            }
            (Scope::TextBody, b"r") if self.font.as_ref().is_some_and(|font| font.rich) => {
                Ok(Step::Push(Scope::TextBody))
            }
            // A field's text is collected like a run's, but what it would
            // recompute to is not modelled.
            (Scope::TextBody, b"fld") if self.font.as_ref().is_some_and(|font| font.rich) => {
                self.record_unconsumed(local);
                Ok(Step::Push(Scope::TextBody))
            }
            (Scope::TextBody, b"t") if self.font.as_ref().is_some_and(|font| font.rich) => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::RunProperties, b"solidFill") => {
                self.fill_target = Some(FillTarget::Font);
                Ok(Step::Push(Scope::SolidFill))
            }
            (Scope::RunProperties, b"latin") => {
                let mut lossy = false;
                for attribute in element.attributes() {
                    match attribute {
                        Ok(attribute) if attribute.key.as_ref() == b"typeface" => {}
                        _ => lossy = true,
                    }
                }
                match attribute_value(element, b"typeface") {
                    Some(face) if !face.is_empty() && face.len() <= MAX_CHART_TYPEFACE_BYTES => {
                        if let Some(font) = self.font.as_mut() {
                            font.current().typeface = Some(face);
                        }
                    }
                    _ => lossy = true,
                }
                if lossy {
                    self.record_unconsumed(local);
                }
                Ok(Step::Leaf)
            }
            (Scope::Trendline | Scope::ErrorBars, b"spPr") => {
                self.fill_target = None;
                Ok(Step::Push(Scope::ShapeProperties))
            }
            // ---- a trendline (`CT_Trendline`) ----
            (Scope::Trendline, b"name") => {
                self.value_text = Some(String::new());
                Ok(Step::Leaf)
            }
            (Scope::Trendline, b"trendlineType") => {
                let kind = match attribute_value(element, b"val").as_deref() {
                    Some("linear") => Some(TrendlineKind::Linear),
                    Some("exp") => Some(TrendlineKind::Exponential),
                    Some("log") => Some(TrendlineKind::Logarithmic),
                    Some("poly") => Some(TrendlineKind::Polynomial),
                    Some("power") => Some(TrendlineKind::Power),
                    Some("movingAvg") => Some(TrendlineKind::MovingAverage),
                    _ => None,
                };
                if let Some(draft) = self.trendline.as_mut() {
                    match kind {
                        Some(kind) => {
                            draft.line.kind = kind;
                            draft.saw_kind = true;
                        }
                        None => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Trendline, b"order") => {
                if let Some(draft) = self.trendline.as_mut() {
                    match unsigned(element)
                        .filter(|order| (2..=6).contains(order))
                        .and_then(|order| u8::try_from(order).ok())
                    {
                        Some(order) => draft.line.order = Some(order),
                        None => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Trendline, b"period") => {
                if let Some(draft) = self.trendline.as_mut() {
                    match unsigned(element).filter(|period| (2..=255).contains(period)) {
                        Some(period) => draft.line.period = Some(period),
                        None => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Trendline, b"forward" | b"backward" | b"intercept") => {
                let value = verbatim_number(element);
                if let Some(draft) = self.trendline.as_mut() {
                    match value {
                        Some(value) => {
                            let slot = match local {
                                b"forward" => &mut draft.line.forward,
                                b"backward" => &mut draft.line.backward,
                                _ => &mut draft.line.intercept,
                            };
                            *slot = Some(value);
                        }
                        None => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::Trendline, b"dispRSqr" | b"dispEq") => {
                if let Some(draft) = self.trendline.as_mut() {
                    let on = is_true(attribute_value(element, b"val").as_deref());
                    if local == b"dispEq" {
                        draft.line.display_equation = on;
                    } else {
                        draft.line.display_r_squared = on;
                    }
                }
                Ok(Step::Leaf)
            }
            // ---- error bars (`CT_ErrBars`) ----
            (Scope::ErrorBars, b"errDir") => {
                if let Some(draft) = self.error_bars.as_mut() {
                    match attribute_value(element, b"val").as_deref() {
                        Some("x") => draft.bars.direction = Some(ErrorBarDirection::X),
                        Some("y") => draft.bars.direction = Some(ErrorBarDirection::Y),
                        _ => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::ErrorBars, b"errBarType") => {
                if let Some(draft) = self.error_bars.as_mut() {
                    draft.saw_bar_type = true;
                    match attribute_value(element, b"val").as_deref() {
                        Some("both") => draft.bars.bar_type = ErrorBarType::Both,
                        Some("minus") => draft.bars.bar_type = ErrorBarType::Minus,
                        Some("plus") => draft.bars.bar_type = ErrorBarType::Plus,
                        _ => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::ErrorBars, b"errValType") => {
                if let Some(draft) = self.error_bars.as_mut() {
                    draft.saw_value_type = true;
                    match attribute_value(element, b"val").as_deref() {
                        Some("cust") => draft.bars.value_type = ErrorValueType::Custom,
                        Some("fixedVal") => draft.bars.value_type = ErrorValueType::FixedValue,
                        Some("percentage") => draft.bars.value_type = ErrorValueType::Percentage,
                        Some("stdDev") => {
                            draft.bars.value_type = ErrorValueType::StandardDeviation;
                        }
                        Some("stdErr") => draft.bars.value_type = ErrorValueType::StandardError,
                        _ => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            (Scope::ErrorBars, b"noEndCap") => {
                if let Some(draft) = self.error_bars.as_mut() {
                    draft.bars.no_end_cap = is_true(attribute_value(element, b"val").as_deref());
                }
                Ok(Step::Leaf)
            }
            (Scope::ErrorBars, b"val") => {
                let value = verbatim_number(element);
                if let Some(draft) = self.error_bars.as_mut() {
                    match value {
                        Some(value) => draft.bars.value = Some(value),
                        None => draft.malformed = true,
                    }
                }
                Ok(Step::Leaf)
            }
            // Custom lengths are a data range like any other: a cache (or a
            // literal), with its formula carried and never parsed.
            (Scope::ErrorBars, b"plus" | b"minus") => {
                let slot = if local == b"plus" {
                    DataSlot::ErrorPlus
                } else {
                    DataSlot::ErrorMinus
                };
                self.range = Some((slot, DataRange::default()));
                Ok(Step::Push(Scope::DataRef(slot)))
            }
            // Anything else in these scopes is unmodelled: carried on a
            // trendline or error bars, a lossy mark inside a text body.
            _ => Ok(Step::Skip),
        }
    }

    /// Reads a run-properties element's attributes (`a:defRPr`, `a:rPr`) into
    /// the open font, recording the element when it carries anything the font
    /// cannot say: an underline, strike, kerning, spacing or baseline other
    /// than the schema's "none", a language tag, or a size or flag the model
    /// would refuse (a size outside [`CHART_FONT_SIZE_RANGE`] is left absent,
    /// never clamped, so the verbatim bytes keep the producer's value).
    pub(super) fn read_run_attributes(&mut self, local: &[u8], element: &BytesStart<'_>) {
        let mut lossy = false;
        let Some(draft) = self.font.as_mut() else {
            return;
        };
        let font = draft.current();
        for attribute in element.attributes() {
            let Ok(attribute) = attribute else {
                lossy = true;
                continue;
            };
            let value = String::from_utf8_lossy(&attribute.value);
            let value = value.as_ref();
            match (attribute.key.as_ref(), value) {
                (b"sz", raw) => match raw.trim().parse::<u32>() {
                    Ok(size) if CHART_FONT_SIZE_RANGE.contains(&size) => font.size = Some(size),
                    _ => lossy = true,
                },
                (b"b" | b"i", raw) => match font_flag(raw) {
                    Some(on) => {
                        if attribute.key.as_ref() == b"b" {
                            font.bold = Some(on);
                        } else {
                            font.italic = Some(on);
                        }
                    }
                    None => lossy = true,
                },
                // The schema's "nothing": present or absent, the same text.
                (b"u", "none") | (b"strike", "noStrike") | (b"baseline" | b"spc", "0") => {}
                _ => lossy = true,
            }
        }
        if lossy {
            self.record_unconsumed(local);
        }
    }

    /// Closes a text body: a `c:txPr`'s font lands on the container that
    /// holds it, a title's `c:rich` font on the open title.
    ///
    /// A font that declares nothing is absent: the writer writes no `c:txPr`
    /// for one, so reading it as present would not survive a round trip. A
    /// lossy `c:txPr` is kept as a shadow either way, and the writer emits a
    /// carried one whatever the font.
    pub(super) fn commit_text_font(&mut self, local: &[u8]) {
        let Some(draft) = self.font.take() else {
            return;
        };
        let font = Some(draft.resolve()).filter(|font| !font.is_empty());
        if local == b"rich" {
            if let Some(title) = self.title.as_mut()
                && font.is_some()
            {
                title.font = font;
            }
            return;
        }
        let owner = self
            .scopes
            .len()
            .checked_sub(2)
            .and_then(|at| self.scopes.get(at))
            .copied();
        match owner {
            Some(Scope::ChartSpace) => self.chart.font = font,
            Some(Scope::Legend) => {
                if let Some(legend) = self.legend.as_mut() {
                    legend.font = font;
                }
            }
            Some(Scope::Axis) => {
                if let Some(axis) = self.axis.as_mut() {
                    axis.font = font;
                }
            }
            Some(Scope::Title) => {
                if let Some(title) = self.title.as_mut() {
                    title.font = font;
                }
            }
            _ => {}
        }
    }

    /// Closes an `a:ln`: the line lands on the series, trendline or error
    /// bars whose `c:spPr` holds it.
    pub(super) fn commit_line(&mut self) {
        if let Some(line) = self.line.take()
            && line != ChartLine::default()
        {
            match self.line_owner() {
                Some(LineOwner::Series) => {
                    if let Some(series) = self.series.as_mut() {
                        series.line = Some(line);
                    }
                }
                Some(LineOwner::Trendline) => {
                    if let Some(draft) = self.trendline.as_mut() {
                        draft.line.line = Some(line);
                    }
                }
                Some(LineOwner::ErrorBars) => {
                    if let Some(draft) = self.error_bars.as_mut() {
                        draft.bars.line = Some(line);
                    }
                }
                None => {}
            }
        }
        self.fill_target = Some(FillTarget::SeriesFill);
    }

    /// Whether `element` (an `a:bodyPr`) is exactly `rot="-5400000"
    /// vert="horz"` inside the title of a left or right axis — the turn Word
    /// and the writer give every vertical axis title. `c:axPos` precedes
    /// `c:title` in every axis sequence, so the position is already read.
    pub(super) fn is_vertical_title_rotation(&self, element: &BytesStart<'_>) -> bool {
        let vertical_axis = self.title.is_some()
            && self.axis.as_ref().is_some_and(|axis| {
                matches!(
                    axis.position,
                    Some(AxisPosition::Left | AxisPosition::Right)
                )
            });
        let mut seen = 0;
        let exact = element.attributes().all(|attribute| {
            seen += 1;
            attribute.is_ok_and(|attribute| {
                matches!(
                    (attribute.key.as_ref(), attribute.value.as_ref()),
                    (b"rot", b"-5400000") | (b"vert", b"horz")
                )
            })
        });
        vertical_axis && exact && seen == 2
    }

    /// Which modelled element owns the open `c:spPr`, by the nearest series,
    /// trendline or error-bar scope. O(scope depth).
    pub(super) fn line_owner(&self) -> Option<LineOwner> {
        self.scopes.iter().rev().find_map(|scope| match scope {
            Scope::Series => Some(LineOwner::Series),
            Scope::Trendline => Some(LineOwner::Trendline),
            Scope::ErrorBars => Some(LineOwner::ErrorBars),
            _ => None,
        })
    }

    /// Opens a series' `c:trendline` or `c:errBars`.
    ///
    /// Read into the model only where the series' schema sequence places it (a
    /// pie series has no trendline; one there stays a loss, as before) and
    /// only up to the model's bound: past [`MAX_CHART_TRENDLINES`] or
    /// [`MAX_CHART_ERROR_BARS`] the rest are LOST, named and `Partial`, never
    /// silently dropped. A self-closing one has none of its required children
    /// and is carried as it stands.
    pub(super) fn begin_series_extra(&mut self, local: &[u8], self_closing: bool) -> Step {
        let trendline = local == b"trendline";
        let name = if trendline { "trendline" } else { "errBars" };
        let admitted = self
            .group
            .as_ref()
            .and_then(|group| GroupDraft::family_of(&group.local))
            .is_some_and(|family| chart_child_rank(ChartContainer::Series(family), name).is_some());
        let Some(series) = self.series.as_ref() else {
            return Step::Skip;
        };
        if !admitted || self_closing {
            return Step::Skip;
        }
        let start = self.event_start;
        if trendline {
            if series.trendlines.len() >= MAX_CHART_TRENDLINES {
                return Step::Lost;
            }
            self.trendline = Some(TrendlineDraft {
                line: Trendline::default(),
                start,
                malformed: false,
                saw_kind: false,
            });
            Step::Push(Scope::Trendline)
        } else {
            if series.error_bars.len() >= MAX_CHART_ERROR_BARS {
                return Step::Lost;
            }
            self.error_bars = Some(ErrorBarsDraft {
                bars: ErrorBars::default(),
                start,
                malformed: false,
                saw_bar_type: false,
                saw_value_type: false,
            });
            Step::Push(Scope::ErrorBars)
        }
    }

    /// Closes a trendline: onto its series when the model can hold it, or
    /// carried whole on the series when it cannot (a polynomial with no valid
    /// order, a moving average with no valid period, an unknown type, a
    /// malformed number). Nothing about a malformed trendline throws, and
    /// nothing about it reaches the model.
    pub(super) fn settle_trendline(&mut self, xml: &[u8], end: usize) {
        let Some(draft) = self.trendline.take() else {
            return;
        };
        let line = &draft.line;
        let valid = !draft.malformed
            && draft.saw_kind
            && (line.kind != TrendlineKind::Polynomial || line.order.is_some())
            && (line.kind != TrendlineKind::MovingAverage || line.period.is_some());
        if valid {
            if let Some(series) = self.series.as_mut() {
                series.trendlines.push(draft.line);
            }
        } else {
            self.carry_whole(xml, draft.start, end, "trendline", &draft.line.retained);
        }
    }

    /// Closes error bars; see `settle_trendline`. `c:errBarType` and
    /// `c:errValType` are required by the schema, and a custom type needs no
    /// further check because absent `c:plus`/`c:minus` mean "no length".
    pub(super) fn settle_error_bars(&mut self, xml: &[u8], end: usize) {
        let Some(draft) = self.error_bars.take() else {
            return;
        };
        if !draft.malformed && draft.saw_bar_type && draft.saw_value_type {
            if let Some(series) = self.series.as_mut() {
                series.error_bars.push(draft.bars);
            }
        } else {
            self.carry_whole(xml, draft.start, end, "errBars", &draft.bars.retained);
        }
    }

    /// Carries `xml[start..end]` verbatim on the open series in place of a
    /// draft the model could not hold. The fragments already carried on the
    /// draft are inside those bytes, so their share of the retained budget is
    /// returned first.
    pub(super) fn carry_whole(
        &mut self,
        xml: &[u8],
        start: usize,
        end: usize,
        name: &str,
        inner: &[ChartXml],
    ) {
        let inner: usize = inner.iter().map(|fragment| fragment.xml.len()).sum();
        self.retained_bytes = self.retained_bytes.saturating_sub(inner);
        match self.carry_target(Scope::Series, name) {
            Some(target) => {
                self.captures.push(Capture {
                    start,
                    name: name.to_owned(),
                    target,
                    shadow: false,
                    depth: 0,
                    lossy: false,
                });
                self.finish_capture(xml, end);
            }
            None => self.record_unconsumed(name.as_bytes()),
        }
    }
}

/// A `@val` kept as its verbatim lexical form (the module's no-float rule),
/// when it is one the model can hold: a finite number, unpadded, within
/// [`MAX_CHART_NUMBER_BYTES`]. Anything else is `None`, and the caller treats
/// the element as unmodelled.
pub(super) fn verbatim_number(element: &BytesStart<'_>) -> Option<String> {
    let raw = attribute_value(element, b"val")?;
    let well_formed = !raw.is_empty()
        && raw.len() <= MAX_CHART_NUMBER_BYTES
        && raw.trim() == raw
        && raw.parse::<f64>().is_ok_and(f64::is_finite);
    well_formed.then_some(raw)
}

/// An `xsd:boolean` text-run flag (`@b`, `@i`), strictly: a value outside the
/// four lexical forms is not guessed at.
pub(super) fn font_flag(raw: &str) -> Option<bool> {
    match raw {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// Whether `fragment` is exactly the trendline label the writer generates
/// when R² or the equation is shown and nothing was carried —
/// `<c:trendlineLbl><c:numFmt formatCode="General" sourceLinked="0"/></c:trendlineLbl>`,
/// ignoring whitespace between tags — so reading it back is not a loss.
pub(super) fn is_default_trendline_label(fragment: &str) -> bool {
    let compact: String = fragment
        .split('>')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(">");
    compact
        == r#"<c:trendlineLbl><c:numFmt formatCode="General" sourceLinked="0"/></c:trendlineLbl>"#
}

/// Whether an element carries any attribute at all.
pub(super) fn has_attributes(element: &BytesStart<'_>) -> bool {
    element.attributes().next().is_some()
}
