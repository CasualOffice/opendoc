//! Settings-part parsing: `word/settings.xml` -> v1 `DocumentSettings`.
//!
//! Only the semantically load-bearing font-embedding flags are modeled today
//! (`w:embedTrueTypeFonts`, `w:embedSystemFonts`, `w:saveSubsetFonts`); every
//! other setting round-trips via Retention. Elements are matched by local name
//! (namespace-agnostic). Each flag is an OOXML `CT_OnOff`: present means true
//! unless an explicit `w:val` says otherwise.

use casual_doc_model::v1::DocumentSettings;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;

/// Parses the settings part, returning the modeled subset (default when none of
/// the recognized flags appear).
pub(crate) fn parse(xml: &[u8], config: ImportConfig) -> Result<DocumentSettings, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut settings = DocumentSettings::default();
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
                on_flag(&element, &mut settings);
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_flag(&element, &mut settings);
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        buffer.clear();
    }
    Ok(settings)
}

/// Records a recognized `CT_OnOff` flag onto the settings.
fn on_flag(element: &BytesStart<'_>, settings: &mut DocumentSettings) {
    let on = on_off(element);
    match element.local_name().as_ref() {
        b"embedTrueTypeFonts" => settings.embed_true_type_fonts = on,
        b"embedSystemFonts" => settings.embed_system_fonts = on,
        b"saveSubsetFonts" => settings.save_subset_fonts = on,
        _ => {}
    }
}

/// Reads an OOXML `CT_OnOff` value: a present element is `true` unless its
/// `w:val` is one of the falsey tokens (`false`, `0`, `off`).
fn on_off(element: &BytesStart<'_>) -> bool {
    match attribute_value(element, b"val") {
        Some(value) => !matches!(value.as_str(), "false" | "0" | "off"),
        None => true,
    }
}

/// Counts an element against the configured ceiling.
fn bump(count: &mut u64, max: u64) -> Result<(), ImportError> {
    *count += 1;
    if *count > max {
        return Err(ImportError::LimitExceeded {
            limit: "xml_elements",
        });
    }
    Ok(())
}
