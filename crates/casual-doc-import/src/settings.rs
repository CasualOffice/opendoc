//! Settings-part parsing: `word/settings.xml` -> v1 `DocumentSettings`.
//!
//! The load-bearing settings are modeled (font-embedding flags, header parity,
//! default tab stop, revision tracking, proof state, document/write protection,
//! default table style, zoom, and the `w:compatSetting` triples). Every OTHER
//! top-level setting is REPORTED (never silently dropped), so an unmodeled
//! setting is auditable and — in Retention mode — preserved by the byte floor.
//! Elements are matched by local name (namespace-agnostic); each `CT_OnOff` flag
//! is present-means-true unless an explicit `w:val` says otherwise.

use casual_doc_model::v1::{
    CompatSetting, DocumentProtection, DocumentProtectionEdit, DocumentSettings, ProofState,
    WriteProtection, Zoom, ZoomMode,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::attribute_value;
use crate::report::Reporter;

/// Parses the settings part, returning the modeled subset (default when none of
/// the recognized settings appear). Every unmodeled top-level setting — and every
/// non-`compatSetting` child of `w:compat` — is reported.
pub(crate) fn parse(
    xml: &[u8],
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<DocumentSettings, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut settings = DocumentSettings::default();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    // `level` is the nesting depth at which an element's own event fires (the root
    // `w:settings` is level 0, its direct setting children level 1, `w:compat`'s
    // children level 2). We act only on the settings themselves (level 1) and on
    // `w:compat`'s children (level 2); deeper markup inside an unmodeled setting is
    // subsumed by that setting's single report entry.
    let mut in_compat = false;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| ImportError::MalformedXml)?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(ImportError::MalformedXml),
            Event::Start(element) => {
                let level = depth;
                depth += 1;
                if depth > config.max_depth {
                    return Err(ImportError::LimitExceeded { limit: "xml_depth" });
                }
                bump(&mut elements, config.max_elements)?;
                if level == 1 && element.local_name().as_ref() == b"compat" {
                    in_compat = true;
                } else {
                    on_setting(level, in_compat, &element, &mut settings, reporter);
                }
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_setting(depth, in_compat, &element, &mut settings, reporter);
            }
            Event::End(element) => {
                if element.local_name().as_ref() == b"compat" {
                    in_compat = false;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(settings)
}

/// Handles one element at its nesting `level`. Level 1 is a direct setting;
/// level 2 while `in_compat` is a `w:compat` child. Recognized settings mutate
/// `settings`; everything else is reported.
fn on_setting(
    level: u64,
    in_compat: bool,
    element: &BytesStart<'_>,
    settings: &mut DocumentSettings,
    reporter: &mut Reporter,
) {
    let local = element.local_name();
    let local = local.as_ref();
    if in_compat && level == 2 {
        if local == b"compatSetting" {
            if !push_compat_setting(element, settings) {
                reporter.report(local);
            }
        } else {
            reporter.report(local);
        }
        return;
    }
    if level != 1 {
        // The root itself (level 0) and markup subsumed by an unmodeled setting.
        return;
    }
    if !apply_setting(local, element, settings) {
        reporter.report(local);
    }
}

/// Applies a recognized top-level setting, returning whether it was consumed.
fn apply_setting(local: &[u8], element: &BytesStart<'_>, settings: &mut DocumentSettings) -> bool {
    match local {
        b"embedTrueTypeFonts" => settings.embed_true_type_fonts = on_off(element),
        b"embedSystemFonts" => settings.embed_system_fonts = on_off(element),
        b"saveSubsetFonts" => settings.save_subset_fonts = on_off(element),
        b"evenAndOddHeaders" => settings.even_and_odd_headers = on_off(element),
        b"mirrorMargins" => settings.mirror_margins = on_off(element),
        b"trackChanges" => settings.track_changes = on_off(element),
        b"updateFields" => settings.update_fields = on_off(element),
        b"defaultTabStop" => match tab_stop(element) {
            Some(value) => settings.default_tab_stop = Some(value),
            None => return false,
        },
        b"defaultTableStyle" => match table_style(element) {
            Some(value) => settings.default_table_style = Some(value),
            None => return false,
        },
        b"proofState" => {
            let spelling = proof_state(element, b"spelling");
            let grammar = proof_state(element, b"grammar");
            if spelling.is_none() && grammar.is_none() {
                return false;
            }
            settings.proof_state.spelling = spelling;
            settings.proof_state.grammar = grammar;
        }
        b"documentProtection" => {
            settings.document_protection = Some(DocumentProtection {
                edit: protection_edit(element),
                enforcement: attr_flag(element, b"enforcement"),
                formatting: attr_flag(element, b"formatting"),
            });
        }
        b"writeProtection" => {
            settings.write_protection = Some(WriteProtection {
                recommended: attr_flag(element, b"recommended"),
            });
        }
        b"zoom" => {
            let zoom = Zoom {
                mode: zoom_mode(element),
                percent: zoom_percent(element),
            };
            if zoom.is_empty() {
                return false;
            }
            settings.zoom = zoom;
        }
        _ => return false,
    }
    true
}

/// Parses a `w:compatSetting` triple, returning whether it was well-formed and
/// pushed (name/uri present and bounded, val bounded).
fn push_compat_setting(element: &BytesStart<'_>, settings: &mut DocumentSettings) -> bool {
    let bounded = |name: &[u8]| attribute_value(element, name).filter(|v| v.len() <= 255);
    let (Some(name), Some(uri)) = (bounded(b"name"), bounded(b"uri")) else {
        return false;
    };
    if name.is_empty() || uri.is_empty() {
        return false;
    }
    let val = bounded(b"val").unwrap_or_default();
    settings.compat.push(CompatSetting { name, uri, val });
    true
}

/// Reads an OOXML `CT_OnOff` element value: present means `true` unless its
/// `w:val` is one of the falsey tokens (`false`, `0`, `off`).
fn on_off(element: &BytesStart<'_>) -> bool {
    match attribute_value(element, b"val") {
        Some(value) => !matches!(value.as_str(), "false" | "0" | "off"),
        None => true,
    }
}

/// Reads an attribute-level `CT_OnOff` (e.g. `w:enforcement`): true only for an
/// explicit truthy token; absent or falsey is `false`.
fn attr_flag(element: &BytesStart<'_>, name: &[u8]) -> bool {
    matches!(
        attribute_value(element, name).as_deref(),
        Some("1" | "true" | "on")
    )
}

/// The default tab stop in twips (`w:defaultTabStop/@w:val`), bounded 0..=31680.
fn tab_stop(element: &BytesStart<'_>) -> Option<i32> {
    attribute_value(element, b"val")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| (0..=31_680).contains(value))
}

/// The default table style name (`w:defaultTableStyle/@w:val`), non-empty and
/// bounded to 255 bytes.
fn table_style(element: &BytesStart<'_>) -> Option<String> {
    attribute_value(element, b"val").filter(|value| !value.is_empty() && value.len() <= 255)
}

/// A `w:proofState` dimension (`w:spelling`/`w:grammar`) mapped to its state.
fn proof_state(element: &BytesStart<'_>, name: &[u8]) -> Option<ProofState> {
    match attribute_value(element, name).as_deref() {
        Some("clean") => Some(ProofState::Clean),
        Some("dirty") => Some(ProofState::Dirty),
        _ => None,
    }
}

/// The `w:documentProtection/@w:edit` restriction (default `none`).
fn protection_edit(element: &BytesStart<'_>) -> DocumentProtectionEdit {
    match attribute_value(element, b"edit").as_deref() {
        Some("readOnly") => DocumentProtectionEdit::ReadOnly,
        Some("comments") => DocumentProtectionEdit::Comments,
        Some("trackedChanges") => DocumentProtectionEdit::TrackedChanges,
        Some("forms") => DocumentProtectionEdit::Forms,
        _ => DocumentProtectionEdit::None,
    }
}

/// The `w:zoom/@w:val` preset mode, if in the vocabulary.
fn zoom_mode(element: &BytesStart<'_>) -> Option<ZoomMode> {
    match attribute_value(element, b"val").as_deref() {
        Some("none") => Some(ZoomMode::None),
        Some("fullPage") => Some(ZoomMode::FullPage),
        Some("bestFit") => Some(ZoomMode::BestFit),
        Some("textFit") => Some(ZoomMode::TextFit),
        _ => None,
    }
}

/// The `w:zoom/@w:percent` magnification, bounded 1..=1000.
fn zoom_percent(element: &BytesStart<'_>) -> Option<u16> {
    attribute_value(element, b"percent")
        .and_then(|value| value.trim_end_matches('%').parse::<u16>().ok())
        .filter(|value| (1..=1_000).contains(value))
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
