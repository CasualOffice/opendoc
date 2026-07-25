//! Theme-part parsing: `word/theme/theme1.xml` `a:fontScheme` -> v1 `FontScheme`.
//!
//! Only the font scheme is modeled (the colour and format schemes round-trip via
//! Retention). Elements are matched by local name (namespace-agnostic), so the
//! DrawingML `a:` prefix is irrelevant. `latin`/`ea`/`cs`/`font` are only honored
//! inside a `majorFont`/`minorFont` within the `fontScheme`, so same-named
//! elements elsewhere in the theme cannot leak in.

use casual_doc_model::v1::{FontCollection, FontScheme, ScriptFont, ThemeFontEntry};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;

#[derive(Clone, Copy)]
enum Slot {
    Major,
    Minor,
}

/// Parses the theme part, returning its font scheme if present.
pub(crate) fn parse(xml: &[u8], config: ImportConfig) -> Result<Option<FontScheme>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut in_scheme = false;
    let mut slot: Option<Slot> = None;
    let mut scheme = FontScheme::default();
    let mut found = false;
    let mut elements = 0_u64;
    let mut depth = 0_u64;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                on_start(&element, &mut in_scheme, &mut slot, &mut scheme, &mut found);
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_start(&element, &mut in_scheme, &mut slot, &mut scheme, &mut found);
            }
            Event::End(element) => {
                on_end(element.local_name().as_ref(), &mut in_scheme, &mut slot);
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(if found { Some(scheme) } else { None })
}

fn on_start(
    element: &BytesStart<'_>,
    in_scheme: &mut bool,
    slot: &mut Option<Slot>,
    scheme: &mut FontScheme,
    found: &mut bool,
) {
    let local = element.local_name();
    let local = local.as_ref();
    match local {
        b"fontScheme" => {
            *in_scheme = true;
            *found = true;
        }
        b"majorFont" if *in_scheme => *slot = Some(Slot::Major),
        b"minorFont" if *in_scheme => *slot = Some(Slot::Minor),
        b"latin" | b"ea" | b"cs" => {
            if let Some(slot) = *slot
                && *in_scheme
            {
                let entry = theme_entry(element);
                let collection = collection_mut(scheme, slot);
                match local {
                    b"latin" => collection.latin = entry,
                    b"ea" => collection.ea = entry,
                    _ => collection.cs = entry,
                }
            }
        }
        b"font" => {
            if let Some(slot) = *slot
                && *in_scheme
                && let (Some(script), Some(typeface)) = (
                    attribute_value(element, b"script"),
                    attribute_value(element, b"typeface"),
                )
                && !script.is_empty()
                && script.len() <= 32
                && typeface.len() <= 255
            {
                collection_mut(scheme, slot)
                    .script_overrides
                    .push(ScriptFont { script, typeface });
            }
        }
        _ => {}
    }
}

fn on_end(local: &[u8], in_scheme: &mut bool, slot: &mut Option<Slot>) {
    match local {
        b"majorFont" | b"minorFont" => *slot = None,
        b"fontScheme" => *in_scheme = false,
        _ => {}
    }
}

fn collection_mut(scheme: &mut FontScheme, slot: Slot) -> &mut FontCollection {
    match slot {
        Slot::Major => &mut scheme.major,
        Slot::Minor => &mut scheme.minor,
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
