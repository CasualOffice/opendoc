//! Admission: the decompression-bomb pre-pass, the active-content inventory
//! and font identity.

use std::collections::BTreeMap;
use std::io::Read;

use flate2::read::{DeflateDecoder, ZlibDecoder};
use hayro_interpret::CacheKey;
use hayro_syntax::Filter;
use hayro_syntax::Pdf;
use hayro_syntax::object::{Array, Dict, Name, Object, Stream};
use hayro_syntax::page::Page;

use crate::limits::check;
use crate::{ActiveContent, DocumentFonts, FontEvidence, PdfReadError, PdfReadLimits};

/// Inflates `raw` into a counting sink and returns how many bytes it decodes
/// to, stopping as soon as the count passes `cap`. Memory is O(1): one 64 KiB
/// buffer. A stream that is not valid zlib is retried as raw deflate (what
/// hayro accepts); one that is neither decodes to what it decoded before the
/// error, which is all hayro could expand it to as well.
fn inflated_len(raw: &[u8], cap: u64) -> u64 {
    fn count(mut reader: impl Read, cap: u64) -> (u64, bool) {
        let mut buffer = [0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return (total, true),
                Ok(n) => {
                    total += n as u64;
                    if total > cap {
                        return (total, true);
                    }
                }
                Err(_) => return (total, false),
            }
        }
    }
    let (zlib, clean) = count(ZlibDecoder::new(raw), cap);
    if clean || zlib > 0 {
        return zlib;
    }
    count(DeflateDecoder::new(raw), cap).0
}

/// Refuses any Flate stream, or the document's Flate streams together, that
/// decode past the `docs/21` bounds. O(document) CPU, O(1) memory.
pub(crate) fn bound_decoded_streams(pdf: &Pdf, limits: &PdfReadLimits) -> Result<(), PdfReadError> {
    let mut total: u64 = 0;
    for object in pdf.objects() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if stream.filters().first() != Some(&Filter::FlateDecode) {
            continue;
        }
        let raw = stream.raw_data();
        let decoded = inflated_len(&raw, limits.max_stream_decoded_bytes);
        check(
            "max_stream_decoded_bytes",
            decoded,
            limits.max_stream_decoded_bytes,
        )?;
        total += decoded;
        check(
            "max_document_decoded_bytes",
            total,
            limits.max_document_decoded_bytes,
        )?;
    }
    Ok(())
}

fn name_is(dict: &Dict<'_>, key: &[u8], value: &[u8]) -> bool {
    dict.get::<Name<'_>>(key)
        .is_some_and(|name| name.as_str().as_bytes() == value)
}

/// Counts an action dictionary (and its `/Next` chain, bounded) into `active`.
fn count_action(action: &Dict<'_>, active: &mut ActiveContent, depth: u32) {
    if depth > 8 {
        return;
    }
    if name_is(action, b"S", b"JavaScript") {
        active.javascript += 1;
    } else if name_is(action, b"S", b"URI") || name_is(action, b"S", b"GoTo") {
        // Navigation only; a link is counted on its annotation.
    } else if action.get::<Name<'_>>(b"S").is_some() {
        active.actions += 1;
    }
    if let Some(next) = action.get::<Dict<'_>>(b"Next") {
        count_action(&next, active, depth + 1);
    } else if let Some(next) = action.get::<Array<'_>>(b"Next") {
        for next in next.iter::<Dict<'_>>().take(16) {
            count_action(&next, active, depth + 1);
        }
    }
}

fn count_additional_actions(owner: &Dict<'_>, active: &mut ActiveContent) {
    if let Some(aa) = owner.get::<Dict<'_>>(b"AA") {
        for key in aa.keys().collect::<Vec<_>>() {
            if let Some(action) = aa.get::<Dict<'_>>(key.as_str().as_bytes()) {
                count_action(&action, active, 0);
            }
        }
    }
}

/// Entries in a name tree, bounded: leaves (`/Names` pairs) and up to 4096
/// kids, depth 16.
fn name_tree_len(node: &Dict<'_>, depth: u32) -> u32 {
    if depth > 16 {
        return 0;
    }
    let mut count = node
        .get::<Array<'_>>(b"Names")
        .map_or(0, |names| (names.raw_iter().count() / 2) as u32);
    if let Some(kids) = node.get::<Array<'_>>(b"Kids") {
        for kid in kids.iter::<Dict<'_>>().take(4096) {
            count = count.saturating_add(name_tree_len(&kid, depth + 1));
        }
    }
    count
}

/// The active-content inventory: JavaScript, actions, embedded files, form
/// fields, links and annotations. Nothing is executed, fetched or extracted.
/// O(catalog + annotations).
pub(crate) fn active_content(pdf: &Pdf) -> ActiveContent {
    let mut active = ActiveContent::default();
    let xref = pdf.xref();
    if let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) {
        if let Some(open) = catalog.get::<Dict<'_>>(b"OpenAction") {
            count_action(&open, &mut active, 0);
        }
        count_additional_actions(&catalog, &mut active);
        if let Some(names) = catalog.get::<Dict<'_>>(b"Names") {
            if let Some(js) = names.get::<Dict<'_>>(b"JavaScript") {
                active.javascript = active.javascript.saturating_add(name_tree_len(&js, 0));
            }
            if let Some(files) = names.get::<Dict<'_>>(b"EmbeddedFiles") {
                active.embedded_files = active
                    .embedded_files
                    .saturating_add(name_tree_len(&files, 0));
            }
        }
        if let Some(form) = catalog.get::<Dict<'_>>(b"AcroForm") {
            active.form_fields = form
                .get::<Array<'_>>(b"Fields")
                .map_or(0, |fields| fields.raw_iter().count() as u32);
            if form.contains_key(b"XFA") {
                active.form_fields = active.form_fields.max(1);
            }
        }
        active.tagged = catalog.contains_key(b"StructTreeRoot");
        active.optional_content = catalog.contains_key(b"OCProperties");
    }
    for page in pdf.pages().iter() {
        count_additional_actions(page.raw(), &mut active);
        let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else {
            continue;
        };
        for annot in annots.iter::<Dict<'_>>().take(100_000) {
            if let Some(action) = annot.get::<Dict<'_>>(b"A") {
                count_action(&action, &mut active, 0);
            }
            count_additional_actions(&annot, &mut active);
            if name_is(&annot, b"Subtype", b"Link") {
                active.links += 1;
            } else if name_is(&annot, b"Subtype", b"FileAttachment") {
                active.embedded_files += 1;
            } else if name_is(&annot, b"Subtype", b"Widget") {
                // Counted through AcroForm; a widget without one still counts.
                active.form_fields = active.form_fields.max(1);
            } else {
                active.annotations += 1;
            }
        }
    }
    active
}

fn strip_subset_prefix(name: &str) -> &str {
    match name.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => name,
    }
}

/// Builds a font's identity from its dictionary (and its descendant's
/// descriptor for a `Type0`).
fn font_identity(dict: &Dict<'_>) -> FontEvidence {
    let raw = dict
        .get::<Name<'_>>(b"BaseFont")
        .map(|name| name.as_str().to_owned())
        .unwrap_or_default();
    let name = strip_subset_prefix(&raw).to_owned();
    let subtype = dict
        .get::<Name<'_>>(b"Subtype")
        .map(|name| name.as_str().to_owned())
        .unwrap_or_default();
    let descriptor = dict.get::<Dict<'_>>(b"FontDescriptor").or_else(|| {
        dict.get::<Array<'_>>(b"DescendantFonts")
            .and_then(|kids| kids.iter::<Dict<'_>>().next())
            .and_then(|kid| kid.get::<Dict<'_>>(b"FontDescriptor"))
    });
    let flags = descriptor
        .as_ref()
        .and_then(|d| d.get::<u32>(b"Flags"))
        .unwrap_or(0);
    let weight = descriptor
        .as_ref()
        .and_then(|d| d.get::<f32>(b"FontWeight"))
        .unwrap_or(0.0);
    let angle = descriptor
        .as_ref()
        .and_then(|d| d.get::<f32>(b"ItalicAngle"))
        .unwrap_or(0.0);
    let family = descriptor
        .as_ref()
        .and_then(|d| d.get::<hayro_syntax::object::String<'_>>(b"FontFamily"))
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned());
    let lower = name.to_ascii_lowercase();
    FontEvidence {
        bold: weight >= 600.0
            || flags & (1 << 18) != 0
            || lower.contains("bold")
            || lower.contains("black")
            || lower.contains("heavy"),
        italic: flags & (1 << 6) != 0
            || angle.abs() > 0.5
            || lower.contains("italic")
            || lower.contains("oblique"),
        serif: flags & (1 << 1) != 0 || lower.contains("times") || lower.contains("serif"),
        monospace: flags & 1 != 0 || lower.contains("courier") || lower.contains("mono"),
        family,
        name,
        subtype,
        program: None,
    }
}

fn learn_font_dict(
    font_dict: &Dict<'_>,
    fonts: &mut DocumentFonts,
    index: &mut BTreeMap<u128, usize>,
    limits: &PdfReadLimits,
) -> Result<(), PdfReadError> {
    for key in font_dict.keys().collect::<Vec<_>>() {
        let Some(font) = font_dict.get::<Dict<'_>>(key.as_str().as_bytes()) else {
            continue;
        };
        let cache_key = font.cache_key();
        if index.contains_key(&cache_key) {
            continue;
        }
        check("max_fonts", fonts.fonts.len() as u64 + 1, limits.max_fonts)?;
        index.insert(cache_key, fonts.fonts.len());
        fonts.fonts.push(font_identity(&font));
    }
    Ok(())
}

fn learn_from_forms(
    x_objects: &Dict<'_>,
    fonts: &mut DocumentFonts,
    index: &mut BTreeMap<u128, usize>,
    limits: &PdfReadLimits,
    depth: u32,
) -> Result<(), PdfReadError> {
    if depth >= 4 {
        return Ok(());
    }
    for key in x_objects.keys().collect::<Vec<_>>() {
        let Some(form) = x_objects.get::<Stream<'_>>(key.as_str().as_bytes()) else {
            continue;
        };
        if !name_is(form.dict(), b"Subtype", b"Form") {
            continue;
        }
        let Some(inner) = form.dict().get::<Dict<'_>>(b"Resources") else {
            continue;
        };
        if let Some(font_dict) = inner.get::<Dict<'_>>(b"Font") {
            learn_font_dict(&font_dict, fonts, index, limits)?;
        }
        if let Some(nested) = inner.get::<Dict<'_>>(b"XObject") {
            learn_from_forms(&nested, fonts, index, limits, depth + 1)?;
        }
    }
    Ok(())
}

/// Registers the identity of every font the page's resources (inheritance
/// resolved by hayro) and its forms' (four levels deep) declare, keyed the way
/// hayro keys its font cache. O(resources).
pub(crate) fn learn_fonts(
    page: &Page<'_>,
    fonts: &mut DocumentFonts,
    index: &mut BTreeMap<u128, usize>,
    limits: &PdfReadLimits,
) -> Result<(), PdfReadError> {
    let resources = page.resources();
    learn_font_dict(&resources.fonts, fonts, index, limits)?;
    learn_from_forms(&resources.x_objects, fonts, index, limits, 0)
}
