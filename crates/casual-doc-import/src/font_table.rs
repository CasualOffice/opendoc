//! Font-table part parsing: `word/fontTable.xml` -> v1 `FontDescriptor`s.
//!
//! Each `w:font` becomes a descriptor keyed by its `@w:name`, retaining the
//! substitution/coverage hints a producer records (altName, panose1, charset,
//! family, pitch, the OS/2 sig, notTrueType). `panose1`/`charset`/sig values are
//! kept verbatim (opaque). Unknown elements/attributes are ignored; the byte
//! floor is Retention's job. Oversized values are dropped (bounded) so a
//! hostile part cannot fail model validation.

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    EmbeddedFace, EmbeddedFontSet, FontDescriptor, FontFamilyKind, FontPitch, FontSig,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, is_true};

/// Parses `word/fontTable.xml` into ordered font descriptors. `font_rels` maps a
/// `fontTable.xml.rels` relationship id to its `.odttf` part name, so an embedded
/// face's `r:id` resolves to a part.
pub(crate) fn parse(
    xml: &[u8],
    font_rels: &BTreeMap<String, String>,
    config: ImportConfig,
) -> Result<Vec<FontDescriptor>, ImportError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut fonts = Vec::new();
    let mut current: Option<FontDescriptor> = None;
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
                on_start(&element, &mut current, font_rels);
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_start(&element, &mut current, font_rels);
                on_end(element.local_name().as_ref(), &mut current, &mut fonts);
            }
            Event::End(element) => {
                on_end(element.local_name().as_ref(), &mut current, &mut fonts);
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(fonts)
}

fn on_start(
    element: &BytesStart<'_>,
    current: &mut Option<FontDescriptor>,
    font_rels: &BTreeMap<String, String>,
) {
    match element.local_name().as_ref() {
        b"font" => {
            let name = attribute_value(element, b"name").unwrap_or_default();
            *current = Some(FontDescriptor {
                name,
                alt_name: None,
                panose1: None,
                charset: None,
                family: None,
                pitch: None,
                sig: FontSig::default(),
                not_true_type: false,
                embedded: EmbeddedFontSet::default(),
            });
        }
        b"embedRegular" => set(current, |font| {
            font.embedded.regular = embed_face(element, font_rels);
        }),
        b"embedBold" => set(current, |font| {
            font.embedded.bold = embed_face(element, font_rels);
        }),
        b"embedItalic" => set(current, |font| {
            font.embedded.italic = embed_face(element, font_rels);
        }),
        b"embedBoldItalic" => set(current, |font| {
            font.embedded.bold_italic = embed_face(element, font_rels);
        }),
        b"altName" => set(current, |font| font.alt_name = bounded_val(element, 255)),
        b"panose1" => set(current, |font| font.panose1 = bounded_val(element, 255)),
        b"charset" => set(current, |font| font.charset = bounded_val(element, 255)),
        b"family" => set(current, |font| {
            font.family = attribute_value(element, b"val")
                .as_deref()
                .and_then(font_family_from);
        }),
        b"pitch" => set(current, |font| {
            font.pitch = attribute_value(element, b"val")
                .as_deref()
                .and_then(font_pitch_from);
        }),
        b"sig" => set(current, |font| {
            font.sig = FontSig {
                usb0: sig_val(element, b"usb0"),
                usb1: sig_val(element, b"usb1"),
                usb2: sig_val(element, b"usb2"),
                usb3: sig_val(element, b"usb3"),
                csb0: sig_val(element, b"csb0"),
                csb1: sig_val(element, b"csb1"),
            };
        }),
        b"notTrueType" => set(current, |font| {
            font.not_true_type = is_true(attribute_value(element, b"val").as_deref());
        }),
        _ => {}
    }
}

fn on_end(local: &[u8], current: &mut Option<FontDescriptor>, fonts: &mut Vec<FontDescriptor>) {
    if local == b"font"
        && let Some(font) = current.take()
    {
        // Skip an empty/oversized name (model validation would reject it).
        if !font.name.is_empty() && font.name.len() <= 255 {
            fonts.push(font);
        }
    }
}

fn set(current: &mut Option<FontDescriptor>, apply: impl FnOnce(&mut FontDescriptor)) {
    if let Some(font) = current.as_mut() {
        apply(font);
    }
}

fn bounded_val(element: &BytesStart<'_>, max: usize) -> Option<String> {
    attribute_value(element, b"val").filter(|value| !value.is_empty() && value.len() <= max)
}

/// Builds an embedded face from a `w:embed*` element: its `r:id` must resolve to
/// an `.odttf` part and a non-empty `w:fontKey` must be present (else skipped).
fn embed_face(
    element: &BytesStart<'_>,
    font_rels: &BTreeMap<String, String>,
) -> Option<EmbeddedFace> {
    let relationship_id = attribute_value(element, b"id")?;
    let part_name = font_rels.get(&relationship_id)?.clone();
    let font_key =
        attribute_value(element, b"fontKey").filter(|key| !key.is_empty() && key.len() <= 64)?;
    let subsetted = is_true(attribute_value(element, b"subsetted").as_deref());
    Some(EmbeddedFace {
        font_key,
        subsetted,
        relationship_id,
        part_name,
    })
}

fn sig_val(element: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    attribute_value(element, name).filter(|value| !value.is_empty() && value.len() <= 32)
}

fn font_family_from(value: &str) -> Option<FontFamilyKind> {
    Some(match value {
        "auto" => FontFamilyKind::Auto,
        "decorative" => FontFamilyKind::Decorative,
        "modern" => FontFamilyKind::Modern,
        "roman" => FontFamilyKind::Roman,
        "script" => FontFamilyKind::Script,
        "swiss" => FontFamilyKind::Swiss,
        _ => return None,
    })
}

fn font_pitch_from(value: &str) -> Option<FontPitch> {
    Some(match value {
        "default" => FontPitch::Default,
        "fixed" => FontPitch::Fixed,
        "variable" => FontPitch::Variable,
        _ => return None,
    })
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
