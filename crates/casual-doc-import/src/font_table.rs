//! Font-table part parsing: `word/fontTable.xml` -> v1 `FontDescriptor`s.
//!
//! Each `w:font` becomes a descriptor keyed by its `@w:name`, retaining the
//! substitution/coverage hints a producer records (altName, panose1, charset,
//! family, pitch, the OS/2 sig, notTrueType). `panose1`/`charset`/sig values are
//! kept verbatim (opaque). Oversized values are dropped (bounded) so a hostile
//! part cannot fail model validation.
//!
//! **Everything this parser cannot carry is reported** (`105` FID-R-04). It had
//! zero report sites and was not even handed a reporter, which made it the worst
//! case of that class: `word/fontTable.xml` is **regenerated from the model on
//! every semantic save**, so an unreported skip here is permanent, invisible
//! loss — and every document has a font table. "The byte floor is Retention's
//! job" was only ever true in Retention mode; on the semantic path there is no
//! byte floor, which is exactly when a finding is the only record that anything
//! was there.
//!
//! Four kinds of loss are reported: an element this parser does not model; a
//! `w:font` with no usable `@w:name` (refused rather than merely dropped — there
//! is nothing left to key it by); an embedded face whose `r:id` or `w:fontKey`
//! does not resolve to a part; and a value dropped for being unrecognised or over
//! the model's bound. The last of those is an **attribute** finding on a modeled
//! element, so it is `degraded` rather than omitted (FID-R-03).

use std::collections::BTreeMap;

use casual_doc_model::v1::{
    EmbeddedFace, EmbeddedFontSet, FontDescriptor, FontFamilyKind, FontPitch, FontSig,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::config::ImportConfig;
use crate::error::ImportError;
use crate::properties::{attribute_value, is_true};
// Own line, kept out of the sorted block above (the repo's parallel-PR rule).
use crate::report::Reporter;

/// Parses `word/fontTable.xml` into ordered font descriptors. `font_rels` maps a
/// `fontTable.xml.rels` relationship id to its `.odttf` part name, so an embedded
/// face's `r:id` resolves to a part.
pub(crate) fn parse(
    xml: &[u8],
    font_rels: &BTreeMap<String, String>,
    config: ImportConfig,
    reporter: &mut Reporter,
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
                on_start(&element, &mut current, font_rels, reporter, false);
            }
            Event::Empty(element) => {
                bump(&mut elements, config.max_elements)?;
                on_start(&element, &mut current, font_rels, reporter, true);
                on_end(
                    element.local_name().as_ref(),
                    &mut current,
                    &mut fonts,
                    reporter,
                );
            }
            Event::End(element) => {
                on_end(
                    element.local_name().as_ref(),
                    &mut current,
                    &mut fonts,
                    reporter,
                );
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(fonts)
}

/// Handles one element of the font table, reporting whatever it cannot carry.
///
/// `self_closing` is whether the source wrote `<x/>`; it is forwarded so an
/// unmodeled element is judged by the same no-op policy every other parser uses,
/// rather than by a second copy of it here.
fn on_start(
    element: &BytesStart<'_>,
    current: &mut Option<FontDescriptor>,
    font_rels: &BTreeMap<String, String>,
    reporter: &mut Reporter,
    self_closing: bool,
) {
    let name = element.local_name();
    match name.as_ref() {
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
        b"embedRegular" | b"embedBold" | b"embedItalic" | b"embedBoldItalic" => {
            let face = embed_face(element, font_rels);
            if face.is_none() {
                // The element is present, so the document declares an embedded
                // face; it is refused because its `r:id` names no `.odttf` part in
                // the package or its `w:fontKey` is missing, and without both there
                // is nothing to decrypt. That is structurally unusable, not merely
                // unmodeled, which is `rejected` in `35`'s vocabulary.
                reporter.report_invalid(name.as_ref());
            }
            set(current, |font| match name.as_ref() {
                b"embedRegular" => font.embedded.regular = face,
                b"embedBold" => font.embedded.bold = face,
                b"embedItalic" => font.embedded.italic = face,
                _ => font.embedded.bold_italic = face,
            });
        }
        b"altName" => set_bounded(current, element, reporter, name.as_ref(), |font, value| {
            font.alt_name = value;
        }),
        b"panose1" => set_bounded(current, element, reporter, name.as_ref(), |font, value| {
            font.panose1 = value;
        }),
        b"charset" => set_bounded(current, element, reporter, name.as_ref(), |font, value| {
            font.charset = value;
        }),
        b"family" => {
            let family = attribute_value(element, b"val")
                .as_deref()
                .and_then(font_family_from);
            report_unreadable_val(element, reporter, name.as_ref(), family.is_none());
            set(current, |font| font.family = family);
        }
        b"pitch" => {
            let pitch = attribute_value(element, b"val")
                .as_deref()
                .and_then(font_pitch_from);
            report_unreadable_val(element, reporter, name.as_ref(), pitch.is_none());
            set(current, |font| font.pitch = pitch);
        }
        b"sig" => {
            for slot in [
                b"usb0".as_slice(),
                b"usb1",
                b"usb2",
                b"usb3",
                b"csb0",
                b"csb1",
            ] {
                // An OS/2 coverage word is kept verbatim, so the only way to lose
                // one is for it to exceed the model's bound. The slot is named, not
                // just the element: which coverage range went missing is the whole
                // content of the finding.
                if attribute_value(element, slot).is_some() && sig_val(element, slot).is_none() {
                    reporter.report_attribute(name.as_ref(), slot);
                }
            }
            set(current, |font| {
                font.sig = FontSig {
                    usb0: sig_val(element, b"usb0"),
                    usb1: sig_val(element, b"usb1"),
                    usb2: sig_val(element, b"usb2"),
                    usb3: sig_val(element, b"usb3"),
                    csb0: sig_val(element, b"csb0"),
                    csb1: sig_val(element, b"csb1"),
                };
            });
        }
        b"notTrueType" => set(current, |font| {
            font.not_true_type = is_true(attribute_value(element, b"val").as_deref());
        }),
        // `w:fonts` is the part's root container and carries nothing of its own.
        // Everything else is a construct the model does not represent, and it is now
        // named instead of skipped: the part is regenerated on save, so this finding
        // is the only trace it leaves.
        b"fonts" => {}
        _ => reporter.report_element(name.as_ref(), element, self_closing),
    }
}

/// Stores a bounded `w:val` on the open descriptor, reporting the drop when the
/// source stated a value the model cannot hold.
///
/// The element is modeled, so a lost value is `degraded` and the finding names the
/// attribute (FID-R-03). An absent `w:val` states nothing and loses nothing.
fn set_bounded(
    current: &mut Option<FontDescriptor>,
    element: &BytesStart<'_>,
    reporter: &mut Reporter,
    local: &[u8],
    apply: impl FnOnce(&mut FontDescriptor, Option<String>),
) {
    let value = bounded_val(element, 255);
    report_unreadable_val(element, reporter, local, value.is_none());
    set(current, |font| apply(font, value));
}

/// Reports `local/@val` when the source stated a `w:val` the parser could not use.
fn report_unreadable_val(
    element: &BytesStart<'_>,
    reporter: &mut Reporter,
    local: &[u8],
    dropped: bool,
) {
    if dropped && attribute_value(element, b"val").is_some() {
        reporter.report_attribute(local, b"val");
    }
}

fn on_end(
    local: &[u8],
    current: &mut Option<FontDescriptor>,
    fonts: &mut Vec<FontDescriptor>,
    reporter: &mut Reporter,
) {
    if local == b"font"
        && let Some(font) = current.take()
    {
        // Skip an empty/oversized name (model validation would reject it). The font
        // table is keyed by name, so a font without a usable one cannot be entered
        // at all — there is nothing to map and nothing to join it to, which is
        // `rejected` rather than a silent skip.
        if font.name.is_empty() || font.name.len() > 255 {
            reporter.report_invalid(local);
            return;
        }
        fonts.push(font);
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
