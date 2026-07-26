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

use std::io::Cursor;

use casual_doc_model::v1::{
    ColorScheme, FontCollection, FontScheme, SchemeColor, ScriptFont, SystemColor, ThemeFontEntry,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, parse_rgb};

/// The modeled pieces of the theme part.
#[derive(Default)]
pub(crate) struct ParsedTheme {
    /// The theme font scheme (`a:fontScheme`), if present.
    pub font_scheme: Option<FontScheme>,
    /// The theme colour scheme (`a:clrScheme`), if present.
    pub color_scheme: Option<ColorScheme>,
    /// The theme format scheme (`a:fmtScheme`), retained verbatim, if present.
    pub format_scheme_xml: Option<String>,
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
}

/// Parses the theme part into its modeled schemes plus the retained format scheme.
pub(crate) fn parse(xml: &[u8], config: ImportConfig) -> Result<ParsedTheme, ImportError> {
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
                } else if element.local_name().as_ref() == b"fmtScheme" {
                    parser.begin_capture(&event)?;
                } else {
                    parser.on_start(element);
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                } else if element.local_name().as_ref() == b"fmtScheme" {
                    parser.begin_capture(&event)?;
                    parser.finish_capture();
                } else {
                    parser.on_start(element);
                }
            }
            Event::End(element) => {
                if parser.capture.is_some() {
                    parser.write_capture(&event)?;
                    parser.capture_depth = parser.capture_depth.saturating_sub(1);
                    if parser.capture_depth == 0 {
                        parser.finish_capture();
                    }
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
    Ok(parser.into_parsed())
}

impl Parser {
    fn on_start(&mut self, element: &BytesStart<'_>) {
        let local = element.local_name();
        let local = local.as_ref();
        match local {
            b"fontScheme" => {
                self.in_font_scheme = true;
                self.found_font = true;
            }
            b"clrScheme" => {
                self.in_clr_scheme = true;
                self.found_clr = true;
                self.color_scheme.name = attribute_value(element, b"name")
                    .filter(|value| value.len() <= 255)
                    .unwrap_or_default();
            }
            b"majorFont" if self.in_font_scheme => self.font_slot = Some(FontSlot::Major),
            b"minorFont" if self.in_font_scheme => self.font_slot = Some(FontSlot::Minor),
            b"latin" | b"ea" | b"cs" if self.in_font_scheme => {
                if let Some(slot) = self.font_slot {
                    let entry = theme_entry(element);
                    let collection = collection_mut(&mut self.font_scheme, slot);
                    match local {
                        b"latin" => collection.latin = entry,
                        b"ea" => collection.ea = entry,
                        _ => collection.cs = entry,
                    }
                }
            }
            b"font" if self.in_font_scheme => {
                if let Some(slot) = self.font_slot
                    && let (Some(script), Some(typeface)) = (
                        attribute_value(element, b"script"),
                        attribute_value(element, b"typeface"),
                    )
                    && !script.is_empty()
                    && script.len() <= 32
                    && typeface.len() <= 255
                {
                    collection_mut(&mut self.font_scheme, slot)
                        .script_overrides
                        .push(ScriptFont { script, typeface });
                }
            }
            b"srgbClr" | b"sysClr" if self.in_clr_scheme => {
                if let Some(slot) = self.clr_slot {
                    *clr_slot_mut(&mut self.color_scheme, slot) = scheme_color(local, element);
                }
            }
            _ if self.in_clr_scheme => {
                if let Some(slot) = clr_slot_from(local) {
                    self.clr_slot = Some(slot);
                }
            }
            _ => {}
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

    fn into_parsed(self) -> ParsedTheme {
        ParsedTheme {
            font_scheme: self.found_font.then_some(self.font_scheme),
            color_scheme: self.found_clr.then_some(self.color_scheme),
            format_scheme_xml: self.format_scheme_xml,
        }
    }
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
