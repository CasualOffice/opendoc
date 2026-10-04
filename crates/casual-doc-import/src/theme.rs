//! Theme-part parsing: `word/theme/theme1.xml` `a:fontScheme` -> v1 `FontScheme`,
//! `a:clrScheme` -> v1 `ColorScheme`, and `a:fmtScheme` retained verbatim.
//!
//! The font and colour schemes are modeled (so theme font/colour references
//! resolve); the format scheme is captured as an opaque XML subtree so its
//! fill/line/effect style lists round-trip without full DrawingML modeling.
//! Elements are matched by local name (namespace-agnostic), so the DrawingML
//! `a:` prefix is irrelevant. `latin`/`ea`/`cs`/`font` are only honored inside a
//! `majorFont`/`minorFont` within the `fontScheme`, and colour slots only inside
//! the `clrScheme`, so same-named elements elsewhere cannot leak in.
//!
//! Everything the part carries that is neither modeled nor retained is reported
//! (FID-R-04). The theme part is *regenerated* by the semantic writer, so an
//! unreported skip here is permanent, invisible loss: `a:objectDefaults`,
//! `a:extraClrSchemeLst`, `a:custClrLst` and `a:extLst` are dropped, and the
//! `a:theme`/`a:fontScheme` `@name` attributes are replaced by fixed writer
//! defaults. A whole-subtree loss is reported once on its outermost element and
//! its descendants are skipped, so one dropped construct is one finding.

use std::io::Cursor;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    ColorScheme, ColorTransform, DashStyle, DefinitionMap, EffectStyle, FillStyle, FontCollection,
    FontScheme, FormatScheme, GradientKind, GradientStyle, GradientStyleStop, LineStyle,
    PatternStyle, Rgba, SchemeColor, ScriptFont, ShapeStyleRef, StyleColor, SystemColor,
    ThemeFontEntry,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, parse_rgb};
use crate::report::Reporter;

/// The modeled pieces of the theme part.
#[derive(Default)]
pub(crate) struct ParsedTheme {
    /// The theme font scheme (`a:fontScheme`), if present.
    pub font_scheme: Option<FontScheme>,
    /// The theme colour scheme (`a:clrScheme`), if present.
    pub color_scheme: Option<ColorScheme>,
    /// The theme format scheme (`a:fmtScheme`), retained verbatim, if present.
    pub format_scheme_xml: Option<String>,
    /// The modeled subset of that same format scheme, for resolution. Parsed from the
    /// retained string, so the string stays byte-for-byte what export writes back.
    pub format_scheme: Option<FormatScheme>,
}

/// Whether the traversal should descend into an element's children. An element
/// whose entire subtree is a single unmapped construct is reported once and
/// answered with `No`, so its descendants do not each raise a duplicate finding.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Descend {
    Yes,
    No,
}

#[derive(Clone, Copy)]
enum FontSlot {
    Major,
    Minor,
}

#[derive(Clone, Copy)]
enum ClrSlot {
    Dark1,
    Light1,
    Dark2,
    Light2,
    Accent1,
    Accent2,
    Accent3,
    Accent4,
    Accent5,
    Accent6,
    Hyperlink,
    FollowedHyperlink,
}

#[derive(Default)]
struct Parser {
    in_font_scheme: bool,
    font_slot: Option<FontSlot>,
    font_scheme: FontScheme,
    found_font: bool,
    in_clr_scheme: bool,
    clr_slot: Option<ClrSlot>,
    color_scheme: ColorScheme,
    found_clr: bool,
    capture: Option<Writer<Cursor<Vec<u8>>>>,
    capture_depth: u32,
    format_scheme_xml: Option<String>,
    /// Nesting level inside a reported-and-skipped subtree (0 when not in one).
    skip_depth: u32,
}

/// Parses the theme part into its modeled schemes plus the retained format
/// scheme, reporting every construct that reaches neither.
pub(crate) fn parse(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<ParsedTheme, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut parser = Parser::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match &event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                    parser.capture_depth += 1;
                } else if parser.skip_depth > 0 {
                    parser.skip_depth += 1;
                } else if element.local_name().as_ref() == b"fmtScheme" {
                    parser.begin_capture(&event)?;
                } else if parser.on_start(element, reporter) == Descend::No {
                    parser.skip_depth = 1;
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                } else if parser.skip_depth > 0 {
                    // Inside a subtree already reported on its outermost element.
                } else if element.local_name().as_ref() == b"fmtScheme" {
                    parser.begin_capture(&event)?;
                    parser.finish_capture();
                } else {
                    // An empty element opens no subtree, so a `Descend::No`
                    // answer has nothing to skip.
                    parser.on_start(element, reporter);
                }
            }
            Event::End(element) => {
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                    parser.capture_depth = parser.capture_depth.saturating_sub(1);
                    if parser.capture_depth == 0 {
                        parser.finish_capture();
                    }
                } else if parser.skip_depth > 0 {
                    parser.skip_depth -= 1;
                } else {
                    parser.on_end(element.local_name().as_ref());
                }
                depth = depth.saturating_sub(1);
            }
            _ => {
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                }
            }
        }
        buffer.clear();
    }
    Ok(parser.into_parsed(&config))
}

impl Parser {
    fn on_start(&mut self, element: &BytesStart<'_>, reporter: &mut Reporter) -> Descend {
        let local = element.local_name();
        let local = local.as_ref();
        match local {
            // The part root. Its schemes are modeled below, but `@name` (the
            // theme's display name) has nowhere to live in the model and the
            // writer emits a fixed one, so a named theme loses its name. The
            // theme itself IS modeled, so the element is `degraded` and the
            // finding names the attribute whose meaning was not carried
            // (FID-R-03) rather than an element-qualified pseudo-feature.
            b"theme" => {
                report_dropped_name(element, b"theme", reporter);
                Descend::Yes
            }
            // A pure container: everything it holds is dispositioned below.
            b"themeElements" => Descend::Yes,
            b"fontScheme" => {
                self.in_font_scheme = true;
                self.found_font = true;
                report_dropped_name(element, b"fontScheme", reporter);
                Descend::Yes
            }
            b"clrScheme" => {
                self.in_clr_scheme = true;
                self.found_clr = true;
                self.color_scheme.name = attribute_value(element, b"name")
                    .filter(|value| value.len() <= 255)
                    .unwrap_or_default();
                Descend::Yes
            }
            b"majorFont" if self.in_font_scheme => {
                self.font_slot = Some(FontSlot::Major);
                Descend::Yes
            }
            b"minorFont" if self.in_font_scheme => {
                self.font_slot = Some(FontSlot::Minor);
                Descend::Yes
            }
            b"latin" | b"ea" | b"cs" if self.in_font_scheme => {
                match self.font_slot {
                    Some(slot) => {
                        let entry = theme_entry(element);
                        let collection = collection_mut(&mut self.font_scheme, slot);
                        match local {
                            b"latin" => collection.latin = entry,
                            b"ea" => collection.ea = entry,
                            _ => collection.cs = entry,
                        }
                    }
                    // An `a:latin`/`a:ea`/`a:cs` outside a major/minor
                    // collection has no slot to land in and is dropped.
                    None => reporter.report(local),
                }
                Descend::Yes
            }
            b"font" if self.in_font_scheme => {
                match (self.font_slot, script_font(element)) {
                    (Some(slot), Some(font)) => collection_mut(&mut self.font_scheme, slot)
                        .script_overrides
                        .push(font),
                    // No enclosing collection, or a script/typeface pair outside
                    // the modeled bounds: the override is dropped.
                    _ => reporter.report(local),
                }
                Descend::Yes
            }
            b"srgbClr" | b"sysClr" if self.in_clr_scheme && self.clr_slot.is_some() => {
                if let Some(slot) = self.clr_slot {
                    *clr_slot_mut(&mut self.color_scheme, slot) = scheme_color(local, element);
                }
                Descend::Yes
            }
            _ if self.in_clr_scheme => match clr_slot_from(local) {
                Some(slot) => {
                    self.clr_slot = Some(slot);
                    Descend::Yes
                }
                // A colour choice the model does not carry (`a:scrgbClr`,
                // `a:hslClr`, `a:prstClr`, a colour transform, `a:extLst`), or a
                // colour outside any slot: the slot keeps its default, so the
                // whole subtree is one loss.
                None => {
                    reporter.report(local);
                    Descend::No
                }
            },
            // An unmodeled child of the font scheme (`a:extLst`, foreign markup).
            _ if self.in_font_scheme => {
                reporter.report(local);
                Descend::No
            }
            // Everything else at theme scope: `a:objectDefaults`,
            // `a:extraClrSchemeLst`, `a:custClrLst`, `a:extLst` and any foreign
            // element. None is modeled and none is retained, so each is reported
            // once and its subtree skipped.
            _ => {
                reporter.report(local);
                Descend::No
            }
        }
    }

    fn on_end(&mut self, local: &[u8]) {
        match local {
            b"majorFont" | b"minorFont" => self.font_slot = None,
            b"fontScheme" => self.in_font_scheme = false,
            b"clrScheme" => {
                self.in_clr_scheme = false;
                self.clr_slot = None;
            }
            _ if self.in_clr_scheme && clr_slot_from(local).is_some() => self.clr_slot = None,
            _ => {}
        }
    }

    fn begin_capture(&mut self, event: &Event<'_>) -> Result<(), ImportError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        writer
            .write_event(event.borrow())
            .map_err(|_| ImportError::MalformedXml)?;
        self.capture = Some(writer);
        self.capture_depth = 1;
        Ok(())
    }

    fn write_capture(&mut self, event: &Event<'_>) -> Result<(), ImportError> {
        if let Some(writer) = self.capture.as_mut() {
            writer
                .write_event(event.borrow())
                .map_err(|_| ImportError::MalformedXml)?;
        }
        Ok(())
    }

    fn finish_capture(&mut self) {
        if let Some(writer) = self.capture.take()
            && let Ok(text) = String::from_utf8(writer.into_inner().into_inner())
        {
            self.format_scheme_xml = Some(text);
        }
        self.capture_depth = 0;
    }

    fn into_parsed(self, config: &ImportConfig) -> ParsedTheme {
        ParsedTheme {
            font_scheme: self.found_font.then_some(self.font_scheme),
            color_scheme: self.found_clr.then_some(self.color_scheme),
            // Parsed from the retained string rather than alongside the streaming
            // capture, so the string export writes back is on exactly the path it
            // already was.
            format_scheme: self
                .format_scheme_xml
                .as_deref()
                .and_then(|xml| parse_format_scheme(xml, config)),
            format_scheme_xml: self.format_scheme_xml,
        }
    }
}

/// Reports `element`'s dropped `@name` when it carries a non-empty one the model
/// has no field for, so a regenerated default does not replace it silently.
fn report_dropped_name(element: &BytesStart<'_>, local: &[u8], reporter: &mut Reporter) {
    if attribute_value(element, b"name").is_some_and(|value| !value.is_empty()) {
        reporter.report_attribute(local, b"name");
    }
}

/// Builds a supplemental script font (`a:font`) from its attribute pair, or
/// `None` when the pair is missing or outside the modeled bounds.
fn script_font(element: &BytesStart<'_>) -> Option<ScriptFont> {
    let script = attribute_value(element, b"script")?;
    let typeface = attribute_value(element, b"typeface")?;
    (!script.is_empty() && script.len() <= 32 && typeface.len() <= 255)
        .then_some(ScriptFont { script, typeface })
}

fn scheme_color(local: &[u8], element: &BytesStart<'_>) -> SchemeColor {
    if local == b"srgbClr" {
        let rgb = attribute_value(element, b"val")
            .as_deref()
            .and_then(parse_rgb)
            .unwrap_or_default();
        SchemeColor::Srgb(rgb)
    } else {
        SchemeColor::System(SystemColor {
            value: attribute_value(element, b"val")
                .filter(|value| !value.is_empty() && value.len() <= 32)
                .unwrap_or_default(),
            last_color: attribute_value(element, b"lastClr")
                .as_deref()
                .and_then(parse_rgb),
        })
    }
}

fn clr_slot_from(local: &[u8]) -> Option<ClrSlot> {
    Some(match local {
        b"dk1" => ClrSlot::Dark1,
        b"lt1" => ClrSlot::Light1,
        b"dk2" => ClrSlot::Dark2,
        b"lt2" => ClrSlot::Light2,
        b"accent1" => ClrSlot::Accent1,
        b"accent2" => ClrSlot::Accent2,
        b"accent3" => ClrSlot::Accent3,
        b"accent4" => ClrSlot::Accent4,
        b"accent5" => ClrSlot::Accent5,
        b"accent6" => ClrSlot::Accent6,
        b"hlink" => ClrSlot::Hyperlink,
        b"folHlink" => ClrSlot::FollowedHyperlink,
        _ => return None,
    })
}

fn clr_slot_mut(scheme: &mut ColorScheme, slot: ClrSlot) -> &mut SchemeColor {
    match slot {
        ClrSlot::Dark1 => &mut scheme.dark1,
        ClrSlot::Light1 => &mut scheme.light1,
        ClrSlot::Dark2 => &mut scheme.dark2,
        ClrSlot::Light2 => &mut scheme.light2,
        ClrSlot::Accent1 => &mut scheme.accent1,
        ClrSlot::Accent2 => &mut scheme.accent2,
        ClrSlot::Accent3 => &mut scheme.accent3,
        ClrSlot::Accent4 => &mut scheme.accent4,
        ClrSlot::Accent5 => &mut scheme.accent5,
        ClrSlot::Accent6 => &mut scheme.accent6,
        ClrSlot::Hyperlink => &mut scheme.hyperlink,
        ClrSlot::FollowedHyperlink => &mut scheme.followed_hyperlink,
    }
}

fn collection_mut(scheme: &mut FontScheme, slot: FontSlot) -> &mut FontCollection {
    match slot {
        FontSlot::Major => &mut scheme.major,
        FontSlot::Minor => &mut scheme.minor,
    }
}

fn theme_entry(element: &BytesStart<'_>) -> ThemeFontEntry {
    ThemeFontEntry {
        typeface: attribute_value(element, b"typeface")
            .filter(|value| value.len() <= 255)
            .unwrap_or_default(),
        panose: bounded(element, b"panose"),
        pitch_family: bounded(element, b"pitchFamily"),
        charset: bounded(element, b"charset"),
    }
}

fn bounded(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    attribute_value(element, name).filter(|value| !value.is_empty() && value.len() <= 255)
}

fn bump(elements: &mut u64, max: u64) -> Result<(), ImportError> {
    *elements += 1;
    if *elements > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}

/// Which `a:fmtScheme` style list the parser is inside.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StyleList {
    Fill,
    Line,
    Effect,
}

/// The style list an element name opens.
fn style_list(local: &[u8]) -> Option<StyleList> {
    Some(match local {
        b"fillStyleLst" => StyleList::Fill,
        b"lnStyleLst" => StyleList::Line,
        b"effectStyleLst" => StyleList::Effect,
        // The background fill list is NOT a modeled list: `a:fillRef@idx >= 1000`
        // selects it and resolves to nothing, so contributing entries here would
        // only give index arithmetic something wrong to find.
        _ => return None,
    })
}

/// An open `a:fillStyleLst` entry, by the kind its outermost element declared.
enum FillEntry {
    Solid(Option<StyleColor>),
    Gradient {
        stops: Vec<GradientStyleStop>,
        kind: Option<GradientKind>,
    },
    Pattern {
        preset: String,
        foreground: Option<StyleColor>,
        background: Option<StyleColor>,
    },
    /// `a:noFill` — representable, and it means "nothing". NOT a loss, which is
    /// why it is distinct from [`FillEntry::Unmodeled`] even though both push a
    /// `None`: a theme that says "no fill" is honoured exactly, and reporting it
    /// would be inventing a finding.
    NoFill,
    /// `a:blipFill`, `a:grpFill`, or a kind this build does not recognise.
    Unmodeled,
}

/// An open `a:lnStyleLst` entry.
struct LineEntry {
    /// `a:ln@w`.
    width_emu: i64,
    /// The colour of the entry's `a:solidFill`, if that is what it declared.
    color: Option<StyleColor>,
    /// `a:prstDash@val`.
    dash: Option<DashStyle>,
    /// Set when the outline's fill is something other than `a:solidFill`. Such an
    /// entry is NOT modeled: `ShapeStroke` carries one colour, so taking a
    /// gradient's first stop would draw a confidently wrong outline.
    non_solid_fill: bool,
}

/// Where a committed colour belongs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorSlot {
    FillSolid,
    GradientStop,
    PatternForeground,
    PatternBackground,
    LineSolid,
}

/// A colour element being accumulated with its (possibly deferred) transform.
struct PendingStyleColor {
    /// The base: the placeholder, or a concrete colour the theme fixed.
    base: Option<Rgba>,
    /// `a:lumMod`/`a:lumOff`/`a:tint`/`a:shade`/`a:alpha`, per-100000.
    transform: ColorTransform,
    slot: ColorSlot,
}

impl PendingStyleColor {
    /// The modeled colour this accumulation became.
    ///
    /// A fixed base folds its transform NOW, because the base is known; the
    /// placeholder carries the transform forward, because its base arrives only
    /// when a shape's `a:fillRef`/`a:lnRef` supplies one.
    fn finish(self) -> StyleColor {
        match self.base {
            Some(base) => StyleColor::Fixed(self.transform.apply(base)),
            None => StyleColor::Placeholder(self.transform),
        }
    }
}

/// Parses the modeled subset of a captured `a:fmtScheme` subtree.
///
/// Reads the CAPTURED string rather than hooking the streaming theme parse, which
/// keeps `format_scheme_xml` — the thing export writes back — on exactly the path it
/// was already on. Adding resolution therefore changed no output byte, and a failure
/// to parse here costs resolution only, never fidelity.
///
/// Entries are recognised by the list they sit in AND by their nesting depth below
/// it, not by their own name alone: an `a:solidFill` directly inside
/// `a:fillStyleLst` is an entry, while the same element one level deeper inside an
/// `a:ln` is that outline's colour. Depth is what tells them apart, and it is why
/// `a:gradFill` can be both a fill entry and the fill of a line entry without the
/// two confusing each other. `a:bgFillStyleLst` contributes nothing, because
/// `a:fillRef@idx >= 1000` — the only way to select it — resolves to nothing.
///
/// An unmodeled entry becomes `None` in place, so a later entry keeps its index. A
/// reference to it resolves to nothing, which is the honest answer — substituting a
/// solid for a gradient would look deliberate.
///
/// Complexity: O(n) in the captured subtree, bounded by the import element cap.
pub(crate) fn parse_format_scheme(xml: &str, config: &ImportConfig) -> Option<FormatScheme> {
    let mut reader = Reader::from_str(xml);
    let mut state = SchemeParser::default();
    let mut elements = 0_u64;

    loop {
        let event = reader.read_event().ok()?;
        match &event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element) => {
                elements += 1;
                if elements > config.max_elements {
                    return None;
                }
                state.on_start(element, matches!(event, Event::Empty(_)));
            }
            Event::End(element) => {
                let local = element.local_name();
                if style_list(local.as_ref()).is_some() || local.as_ref() == b"bgFillStyleLst" {
                    state.close_list();
                } else if state.list.is_some() {
                    state.depth = state.depth.saturating_sub(1);
                    state.on_end(local.as_ref());
                }
            }
            _ => {}
        }
    }
    state.close_list();
    state.finish()
}

/// The `a:fmtScheme` state machine: which list is open, how deep inside it the
/// cursor is, and whichever entry and colour are being accumulated.
#[derive(Default)]
struct SchemeParser {
    list: Option<StyleList>,
    /// Nesting depth below the open list element (`0` at its direct children).
    depth: u32,
    fill: Option<FillEntry>,
    line: Option<LineEntry>,
    /// The open `a:effectStyle`, and whether anything inside it is an effect.
    effect: Option<bool>,
    /// Depth of the open `a:effectLst`, so an element is only counted as an effect
    /// when it is actually inside one (`a:scene3d` beside it is not an effect).
    in_effect_lst: bool,
    pending_color: Option<PendingStyleColor>,
    /// `a:gs@pos` of the gradient stop being read.
    stop_position: Option<i32>,
    /// Set when an `a:gs` closed without yielding a colour this build can hold.
    ///
    /// The whole entry is then unmodeled, rather than a gradient missing a stop.
    /// A three-stop gradient delivered as two stops is a *different* gradient that
    /// still paints — exactly the confidently-wrong output this module refuses
    /// elsewhere — and nothing downstream could tell it had happened.
    gradient_stop_lost: bool,
    /// Which pattern colour container is open.
    pattern_slot: Option<ColorSlot>,
    /// Whether the cursor is inside an `a:ln`'s own `a:solidFill`.
    in_line_solid: bool,
    fill_styles: Vec<Option<FillStyle>>,
    line_styles: Vec<Option<LineStyle>>,
    effect_styles: Vec<EffectStyle>,
}

impl SchemeParser {
    fn on_start(&mut self, element: &BytesStart<'_>, empty: bool) {
        let local = element.local_name();
        let local = local.as_ref();
        if let Some(kind) = style_list(local) {
            self.close_list();
            // An empty list element (`<a:fillStyleLst/>`) has no `End` to close it,
            // so leaving it open would let the next sibling's children be read as
            // its entries.
            if !empty {
                self.list = Some(kind);
                self.depth = 0;
            }
            return;
        }
        if local == b"bgFillStyleLst" {
            self.close_list();
            return;
        }
        let Some(kind) = self.list else { return };
        if self.depth == 0 {
            self.open_entry(kind, local, element, empty);
        } else {
            self.inside_entry(local, element, empty);
        }
        // The list element itself returned above, so depth counts only the nesting
        // BELOW it: an entry opens at `0`, and its own children are at `1`.
        if !empty {
            self.depth = self.depth.saturating_add(1);
        }
    }

    /// A direct child of the open style list: one entry.
    fn open_entry(&mut self, kind: StyleList, local: &[u8], element: &BytesStart<'_>, empty: bool) {
        match kind {
            StyleList::Fill => {
                self.fill = Some(match local {
                    b"solidFill" => FillEntry::Solid(None),
                    b"gradFill" => FillEntry::Gradient {
                        stops: Vec::new(),
                        kind: None,
                    },
                    b"pattFill" => FillEntry::Pattern {
                        preset: bounded(element, b"prst").unwrap_or_default(),
                        foreground: None,
                        background: None,
                    },
                    b"noFill" => FillEntry::NoFill,
                    _ => FillEntry::Unmodeled,
                });
            }
            StyleList::Line if local == b"ln" => {
                self.line = Some(LineEntry {
                    width_emu: attribute_value(element, b"w")
                        .as_deref()
                        .and_then(|value| value.parse::<i64>().ok())
                        .unwrap_or(0),
                    color: None,
                    dash: None,
                    non_solid_fill: false,
                });
            }
            StyleList::Effect if local == b"effectStyle" => self.effect = Some(false),
            // A child the list's schema does not define. Ignored rather than
            // pushed: giving it an index would shift every entry after it, which
            // is the one failure mode that silently paints the WRONG style.
            StyleList::Line | StyleList::Effect => return,
        }
        if empty {
            self.push_entry();
        }
    }

    /// An element inside an open entry.
    fn inside_entry(&mut self, local: &[u8], element: &BytesStart<'_>, empty: bool) {
        match local {
            b"gs" => {
                self.stop_position = attribute_value(element, b"pos")
                    .as_deref()
                    .and_then(parse_position);
            }
            b"lin" => {
                if let Some(FillEntry::Gradient { kind, .. }) = self.fill.as_mut() {
                    *kind = Some(GradientKind::Linear {
                        angle: attribute_value(element, b"ang")
                            .as_deref()
                            .and_then(|value| value.parse::<i32>().ok())
                            .unwrap_or(0),
                    });
                }
            }
            b"path" => {
                if let Some(FillEntry::Gradient { kind, .. }) = self.fill.as_mut() {
                    *kind = Some(GradientKind::Radial);
                }
            }
            b"fgClr" => self.pattern_slot = Some(ColorSlot::PatternForeground),
            b"bgClr" => self.pattern_slot = Some(ColorSlot::PatternBackground),
            b"solidFill" if self.line.is_some() => self.in_line_solid = true,
            // The outline's fill is not solid, so the entry is not modeled.
            b"gradFill" | b"pattFill" | b"blipFill" => {
                if let Some(line) = self.line.as_mut() {
                    line.non_solid_fill = true;
                }
            }
            b"prstDash" => {
                if let Some(line) = self.line.as_mut() {
                    line.dash = attribute_value(element, b"val")
                        .as_deref()
                        .and_then(crate::body::parse_dash_style);
                }
            }
            b"effectLst" => self.in_effect_lst = !empty,
            b"srgbClr" | b"schemeClr" | b"sysClr" => {
                if let Some(slot) = self.color_slot() {
                    // `a:phClr` is the placeholder; `a:srgbClr` is a colour the theme
                    // fixes itself. A `a:schemeClr` naming anything else is a
                    // theme-relative colour this build does not resolve here, so the
                    // colour stays unset and its entry unmodeled.
                    let base = match local {
                        b"schemeClr" => match attribute_value(element, b"val").as_deref() {
                            Some("phClr") => None,
                            _ => return,
                        },
                        b"srgbClr" => match attribute_value(element, b"val")
                            .as_deref()
                            .and_then(parse_rgb)
                        {
                            Some(rgb) => Some(Rgba {
                                r: rgb.r,
                                g: rgb.g,
                                b: rgb.b,
                                a: 255,
                            }),
                            None => return,
                        },
                        _ => return,
                    };
                    self.pending_color = Some(PendingStyleColor {
                        base,
                        transform: ColorTransform::default(),
                        slot,
                    });
                    if empty {
                        self.commit_color();
                    }
                }
            }
            b"lumMod" | b"lumOff" | b"tint" | b"shade" | b"alpha" => {
                if let Some(pending) = self.pending_color.as_mut()
                    && let Some(value) = attribute_value(element, b"val")
                        .as_deref()
                        .and_then(crate::body::parse_drawing_percentage)
                    && let Ok(value) = i32::try_from(value)
                {
                    match local {
                        b"lumMod" => pending.transform.lum_mod = Some(value),
                        b"lumOff" => pending.transform.lum_off = Some(value),
                        b"tint" => pending.transform.tint = Some(value),
                        b"shade" => pending.transform.shade = Some(value),
                        _ => pending.transform.alpha = Some(value),
                    }
                }
            }
            // An element inside an open `a:effectLst` IS an effect; nothing here
            // renders one, so the entry only records that there was one.
            _ if self.in_effect_lst => {
                if let Some(carries) = self.effect.as_mut() {
                    *carries = true;
                }
            }
            _ => {}
        }
    }

    fn on_end(&mut self, local: &[u8]) {
        match local {
            b"srgbClr" | b"schemeClr" | b"sysClr" => self.commit_color(),
            // The stop's position was never consumed, so no colour reached it.
            b"gs" => self.gradient_stop_lost |= self.stop_position.take().is_some(),
            b"fgClr" | b"bgClr" => self.pattern_slot = None,
            b"solidFill" if self.depth > 0 && self.line.is_some() => self.in_line_solid = false,
            b"effectLst" => self.in_effect_lst = false,
            _ => {}
        }
        if self.depth == 0 {
            self.push_entry();
        }
    }

    /// Where a colour element starting here belongs, or `None` when it sits
    /// somewhere this build reads no colour from (an `a:blipFill`'s duotone, a
    /// line end, an effect).
    fn color_slot(&self) -> Option<ColorSlot> {
        if self.line.is_some() {
            return self.in_line_solid.then_some(ColorSlot::LineSolid);
        }
        match self.fill.as_ref()? {
            FillEntry::Solid(_) => Some(ColorSlot::FillSolid),
            FillEntry::Gradient { .. } => self.stop_position.map(|_| ColorSlot::GradientStop),
            FillEntry::Pattern { .. } => self.pattern_slot,
            FillEntry::NoFill | FillEntry::Unmodeled => None,
        }
    }

    fn commit_color(&mut self) {
        let Some(pending) = self.pending_color.take() else {
            return;
        };
        let slot = pending.slot;
        let color = pending.finish();
        match slot {
            ColorSlot::LineSolid => {
                if let Some(line) = self.line.as_mut()
                    && line.color.is_none()
                {
                    line.color = Some(color);
                }
            }
            ColorSlot::FillSolid => {
                if let Some(FillEntry::Solid(slot)) = self.fill.as_mut()
                    && slot.is_none()
                {
                    *slot = Some(color);
                }
            }
            ColorSlot::GradientStop => {
                if let Some(FillEntry::Gradient { stops, .. }) = self.fill.as_mut()
                    && let Some(position) = self.stop_position.take()
                {
                    stops.push(GradientStyleStop { position, color });
                }
            }
            ColorSlot::PatternForeground | ColorSlot::PatternBackground => {
                if let Some(FillEntry::Pattern {
                    foreground,
                    background,
                    ..
                }) = self.fill.as_mut()
                {
                    let target = if slot == ColorSlot::PatternForeground {
                        foreground
                    } else {
                        background
                    };
                    if target.is_none() {
                        *target = Some(color);
                    }
                }
            }
        }
    }

    /// Pushes whichever entry is open, as a modeled style or as the `None` that
    /// keeps every later entry's index correct.
    fn push_entry(&mut self) {
        self.pending_color = None;
        self.stop_position = None;
        self.pattern_slot = None;
        self.in_line_solid = false;
        self.in_effect_lst = false;
        let gradient_stop_lost = std::mem::take(&mut self.gradient_stop_lost);
        if let Some(entry) = self.fill.take() {
            self.fill_styles.push(match entry {
                FillEntry::Solid(color) => color.map(|color| FillStyle::Solid { color }),
                // A gradient with no stops is not a gradient. Modelling it would
                // put an empty stop list in front of the renderer, which paints
                // nothing and reports nothing.
                FillEntry::Gradient { stops, kind }
                    if !stops.is_empty() && !gradient_stop_lost =>
                {
                    Some(FillStyle::Gradient(GradientStyle {
                        stops,
                        // ECMA-376 §20.1.8.33: a gradient with neither `a:lin` nor
                        // `a:path` is a linear sweep along the default axis.
                        kind: kind.unwrap_or(GradientKind::Linear { angle: 0 }),
                    }))
                }
                FillEntry::Pattern {
                    preset,
                    foreground: Some(foreground),
                    background: Some(background),
                } => Some(FillStyle::Pattern(PatternStyle {
                    preset,
                    foreground,
                    background,
                })),
                FillEntry::Gradient { .. }
                | FillEntry::Pattern { .. }
                | FillEntry::NoFill
                | FillEntry::Unmodeled => None,
            });
        }
        if let Some(entry) = self.line.take() {
            self.line_styles
                .push((!entry.non_solid_fill).then_some(entry.color).flatten().map(
                    |color| LineStyle {
                        width_emu: entry.width_emu,
                        color,
                        dash: entry.dash,
                    },
                ));
        }
        if let Some(carries_effects) = self.effect.take() {
            self.effect_styles.push(EffectStyle { carries_effects });
        }
    }

    /// Closes the open list, pushing an entry the source left unclosed rather than
    /// dropping it (which would shift every later index).
    fn close_list(&mut self) {
        self.push_entry();
        self.list = None;
        self.depth = 0;
    }

    fn finish(self) -> Option<FormatScheme> {
        let found = !self.fill_styles.is_empty()
            || !self.line_styles.is_empty()
            || !self.effect_styles.is_empty();
        found.then_some(FormatScheme {
            fill_styles: self.fill_styles,
            line_styles: self.line_styles,
            effect_styles: self.effect_styles,
        })
    }
}

/// `a:gs@pos`, as the per-100000 position the model holds.
fn parse_position(value: &str) -> Option<i32> {
    crate::body::parse_drawing_percentage(value).and_then(|value| i32::try_from(value).ok())
}

/// Reports the appearance each shape's `wps:style` named and this build cannot
/// paint, once per reference.
///
/// Run after the body parse, because it needs both halves: the theme's format
/// scheme (which says what the entry IS) and the shape-style side table (which says
/// which entries are actually asked for). Reporting from the theme parse alone would
/// raise a finding for every Office theme, whose third effect style carries an
/// `a:outerShdw` no shape in the document need reference.
///
/// Each reference is classified exactly as `themed_appearance` in
/// `casual-doc-layout` resolves it, so a reported loss and an unfilled shape are the
/// same event seen twice. The three reasons are:
///
/// * `pattern` — an `a:pattFill` entry. Modeled, and painted by nothing.
/// * `unmodeled` — an `a:blipFill`/`a:grpFill` fill entry, a non-solid outline
///   entry, or an index past the list; the entry itself resolved to nothing.
/// * `effect-not-rendered` — an `a:effectStyle` that carries real effects. There is
///   no shadow/glow/blur primitive here, so the reference resolves, reports, and
///   renders nothing at all.
///
/// An `@idx` of `0` is NOT a loss: it means "no fill"/"no outline"/"no effect", and
/// that is honoured exactly.
///
/// Complexity: O(styled shapes) — one index per reference, no scan of the document.
pub(crate) fn report_unpaintable_style_refs(
    scheme: &FormatScheme,
    shape_styles: &DefinitionMap<NodeId, ShapeStyleRef>,
    reporter: &mut Reporter,
) {
    for (_, reference) in shape_styles.iter() {
        if let Some(idx) = reference.fill_idx.filter(|idx| *idx != 0) {
            match scheme.fill_style(idx) {
                Some(FillStyle::Solid { .. } | FillStyle::Gradient(_)) => {}
                Some(FillStyle::Pattern(_)) => {
                    reporter.report_theme_style_unpainted("fillStyleLst", "pattern");
                }
                None => reporter.report_theme_style_unpainted("fillStyleLst", "unmodeled"),
            }
        }
        if let Some(idx) = reference.line_idx.filter(|idx| *idx != 0)
            && scheme.line_style(idx).is_none()
        {
            reporter.report_theme_style_unpainted("lnStyleLst", "unmodeled");
        }
        // An effect style with an EMPTY `a:effectLst` is not a loss, and in the
        // default Office theme two of the three are exactly that.
        if let Some(idx) = reference.effect_idx.filter(|idx| *idx != 0)
            && scheme
                .effect_style(idx)
                .is_some_and(|style| style.carries_effects)
        {
            reporter.report_theme_style_unpainted("effectStyleLst", "effect-not-rendered");
        }
    }
}
