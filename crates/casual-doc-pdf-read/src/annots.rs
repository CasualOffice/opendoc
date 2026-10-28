//! Page annotations as evidence (`docs/182` §5.1: "link annotations"):
//! links with their resolved target, text and free-text notes with author and
//! date, and every other kind by subtype so it can be reported.
//!
//! Nothing is executed or fetched (MF-I6): a link's URI is read as a string,
//! a `javascript:`, `file:` or `data:` URI is not offered as a target, and an
//! action other than `URI`/`GoTo` yields [`LinkTarget::Blocked`] (the
//! admission inventory already counted it as active content).
//!
//! Coordinates are page space (top-left origin, y down, CropBox and `/Rotate`
//! applied), the same frame as [`crate::PageEvidence`].
//!
//! Complexity: O(annotations on the page) plus, per named destination, a
//! name-tree lookup bounded by `MAX_NAME_TREE_NODES`.

use std::collections::BTreeMap;

use hayro_interpret::TransformExt;
use hayro_syntax::object::{Array, Dict, Name, Object, ObjectIdentifier};
use kurbo::{Point as KPoint, Rect};

use crate::{Bounds, PdfReadError, PdfSource};

/// Annotations read per page (the admission inventory's bound).
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 100_000;
/// Name-tree nodes visited per destination lookup.
pub const MAX_NAME_TREE_NODES: usize = 4_096;
/// Characters kept of an annotation's text.
pub const MAX_ANNOTATION_TEXT_CHARS: usize = 64 * 1024;

/// Where a link goes.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    /// An external URI (`/S /URI`).
    Uri(String),
    /// A place in this document (`/S /GoTo` or `/Dest`).
    Page {
        /// Zero-based page index.
        page: usize,
        /// The destination's top in page space, when it states one.
        top: Option<f64>,
    },
    /// A destination that names nothing this document defines.
    Unresolved,
    /// A non-navigation action or a refused URI scheme: not offered.
    Blocked,
}

/// What an annotation is.
#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationKind {
    /// `/Link`.
    Link(LinkTarget),
    /// `/Text` (a sticky note) or, with `free_text`, `/FreeText`.
    Note {
        /// Whether it is a `/FreeText` annotation (text drawn on the page).
        free_text: bool,
        /// Whether it replies to another annotation (`/IRT`).
        reply: bool,
    },
    /// `/Popup`: the window of another annotation, not content of its own.
    Popup,
    /// `/Widget`, counted with the form fields.
    Widget,
    /// `/FileAttachment`, counted with the embedded files.
    FileAttachment,
    /// Every other subtype (highlights, stamps, ink, shapes, ...).
    Other,
}

/// One annotation.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationEvidence {
    /// Its kind.
    pub kind: AnnotationKind,
    /// The `/Subtype` as written.
    pub subtype: String,
    /// `/Rect` in page space.
    pub rect: Bounds,
    /// `/T`, the author.
    pub author: Option<String>,
    /// `/Contents`, the note's text.
    pub contents: Option<String>,
    /// `/M` (else `/CreationDate`) as an ISO 8601 date-time.
    pub date: Option<String>,
}

/// Decodes a PDF text string (ISO 32000-1 §7.9.2.2): UTF-16BE or UTF-8 with
/// their byte-order marks, else PDFDocEncoding, read as Latin-1 (the two agree
/// outside 0x18-0x1F and 0x80-0x9F, which hold punctuation this keeps as the
/// Latin-1 code point). Bounded to `MAX_ANNOTATION_TEXT_CHARS`. O(bytes).
#[must_use]
pub fn text_string(bytes: &[u8]) -> String {
    let text: String = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units = rest
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
        char::decode_utf16(units)
            .map(|c| c.unwrap_or('\u{fffd}'))
            .collect()
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|b| char::from(*b)).collect()
    };
    text.chars()
        .map(|c| if c == '\r' { '\n' } else { c })
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(MAX_ANNOTATION_TEXT_CHARS)
        .collect()
}

/// Converts a PDF date (`D:YYYYMMDDHHmmSSOHH'mm'`, ISO 32000-1 §7.9.4) to ISO
/// 8601 with its offset kept (`2026-10-29T12:30:00+01:00`); absent fields take
/// their defaults, a missing offset is read as UTC. `None` for anything that
/// is not such a date. O(1).
#[must_use]
pub fn pdf_date(raw: &str) -> Option<String> {
    let s = raw.strip_prefix("D:").unwrap_or(raw);
    // The digits run as far as they go (the spec lets a date stop after any
    // field); what follows is the offset or nothing.
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (digits, rest) = s.split_at(end);
    if !matches!(digits.len(), 4 | 6 | 8 | 10 | 12 | 14) {
        return None;
    }
    let field = |from: usize, default: u32| -> u32 {
        digits
            .get(from..from + if from == 0 { 4 } else { 2 })
            .and_then(|part| part.parse().ok())
            .unwrap_or(default)
    };
    let (year, month, day) = (field(0, 0), field(4, 1), field(6, 1));
    let (hour, minute, second) = (field(8, 0), field(10, 0), field(12, 0));
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let second = second.min(59);
    let offset = match rest.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let numbers: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let oh: u32 = numbers.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(0);
            let om: u32 = numbers.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(0);
            if oh > 23 || om > 59 {
                return None;
            }
            format!("{sign}{oh:02}:{om:02}")
        }
        Some('Z') | None => "Z".to_owned(),
        Some(_) => return None,
    };
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{offset}"
    ))
}

fn name_is(dict: &Dict<'_>, key: &[u8], value: &[u8]) -> bool {
    dict.get::<Name<'_>>(key)
        .is_some_and(|name| name.as_str().as_bytes() == value)
}

fn string(dict: &Dict<'_>, key: &[u8]) -> Option<String> {
    dict.get::<hayro_syntax::object::String<'_>>(key)
        .map(|s| text_string(s.as_bytes()))
        .filter(|s| !s.trim().is_empty())
}

/// Whether a URI may be offered as a hyperlink target. Navigation schemes
/// only; anything that would run, read local files or inline content is not.
fn allowed_uri(uri: &str) -> bool {
    let lower = uri.trim_start().to_ascii_lowercase();
    !(lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("file:")
        || lower.starts_with("data:")
        || uri.trim().is_empty())
}

/// Looks `key` up in a name tree (ISO 32000-1 §7.9.6), bounded.
fn name_tree_lookup<'a>(root: &Dict<'a>, key: &[u8]) -> Option<Object<'a>> {
    let mut stack = vec![root.clone()];
    let mut visited = 0usize;
    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NAME_TREE_NODES {
            return None;
        }
        if let Some(names) = node.get::<Array<'_>>(b"Names") {
            let mut iter = names.iter::<Object<'_>>();
            while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
                if let Object::String(k) = &k
                    && k.as_bytes() == key
                {
                    return Some(v);
                }
            }
        }
        if let Some(kids) = node.get::<Array<'_>>(b"Kids") {
            stack.extend(kids.iter::<Dict<'_>>());
        }
    }
    None
}

struct Resolver<'p> {
    pages: BTreeMap<ObjectIdentifier, usize>,
    transforms: Vec<kurbo::Affine>,
    catalog: Option<Dict<'p>>,
}

impl<'p> Resolver<'p> {
    /// A destination: an explicit array, or a name/string through the
    /// catalog's `/Dests` dictionary or `/Names /Dests` tree.
    fn destination(&self, dest: &Object<'p>, depth: u32) -> LinkTarget {
        if depth > 4 {
            return LinkTarget::Unresolved;
        }
        match dest {
            Object::Array(array) => self.explicit(array),
            Object::Name(name) => {
                let found = self
                    .catalog
                    .as_ref()
                    .and_then(|c| c.get::<Dict<'_>>(b"Dests"))
                    .and_then(|dests| dests.get::<Object<'_>>(name.as_str().as_bytes()));
                self.named(found, depth)
            }
            Object::String(key) => {
                let found = self
                    .catalog
                    .as_ref()
                    .and_then(|c| c.get::<Dict<'_>>(b"Names"))
                    .and_then(|names| names.get::<Dict<'_>>(b"Dests"))
                    .and_then(|tree| name_tree_lookup(&tree, key.as_bytes()));
                self.named(found, depth)
            }
            _ => LinkTarget::Unresolved,
        }
    }

    fn named(&self, found: Option<Object<'p>>, depth: u32) -> LinkTarget {
        match found {
            Some(Object::Dict(dict)) => dict
                .get::<Object<'_>>(b"D")
                .map_or(LinkTarget::Unresolved, |d| self.destination(&d, depth + 1)),
            Some(other) => self.destination(&other, depth + 1),
            None => LinkTarget::Unresolved,
        }
    }

    /// `[page /XYZ left top zoom]`, `[page /FitH top]`, `[page /Fit]`, ...
    fn explicit(&self, array: &Array<'p>) -> LinkTarget {
        let mut iter = array.flex_iter();
        let page = match iter.next::<Dict<'_>>() {
            Some(dict) => dict.obj_id().and_then(|id| self.pages.get(&id).copied()),
            None => None,
        };
        // A remote-style destination names the page by number.
        let page = page.or_else(|| {
            array
                .iter::<i32>()
                .next()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n < self.transforms.len())
        });
        let Some(page) = page else {
            return LinkTarget::Unresolved;
        };
        let values: Vec<Object<'_>> = array.iter::<Object<'_>>().skip(1).collect();
        let kind = values.first().and_then(|v| match v {
            Object::Name(name) => Some(name.as_str().to_owned()),
            _ => None,
        });
        let number = |at: usize| match values.get(at) {
            Some(Object::Number(n)) => Some(n.as_f64()),
            _ => None,
        };
        let top_user = match kind.as_deref() {
            Some("XYZ") => number(2),
            Some("FitH" | "FitBH") => number(1),
            Some("FitR") => number(4),
            _ => None,
        };
        let top = top_user.map(|y| {
            let left = number(1).unwrap_or(0.0);
            (self.transforms[page] * KPoint::new(left, y)).y
        });
        LinkTarget::Page { page, top }
    }

    fn link(&self, annot: &Dict<'p>) -> LinkTarget {
        if let Some(action) = annot.get::<Dict<'_>>(b"A") {
            if name_is(&action, b"S", b"URI") {
                return action
                    .get::<hayro_syntax::object::String<'_>>(b"URI")
                    .map(|s| String::from_utf8_lossy(s.as_bytes()).trim().to_owned())
                    .filter(|uri| allowed_uri(uri))
                    .map_or(LinkTarget::Blocked, LinkTarget::Uri);
            }
            if name_is(&action, b"S", b"GoTo") {
                return action
                    .get::<Object<'_>>(b"D")
                    .map_or(LinkTarget::Unresolved, |d| self.destination(&d, 0));
            }
            return LinkTarget::Blocked;
        }
        annot
            .get::<Object<'_>>(b"Dest")
            .map_or(LinkTarget::Unresolved, |d| self.destination(&d, 0))
    }
}

impl PdfSource {
    /// The annotations of page `index`, in `/Annots` order. O(annotations).
    ///
    /// # Errors
    ///
    /// [`PdfReadError::NoSuchPage`].
    pub fn annotations(&self, index: usize) -> Result<Vec<AnnotationEvidence>, PdfReadError> {
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or(PdfReadError::NoSuchPage(index))?;
        let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") else {
            return Ok(Vec::new());
        };
        let xref = self.pdf.xref();
        let resolver = Resolver {
            pages: pages
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.raw().obj_id().map(|id| (id, i)))
                .collect(),
            transforms: pages
                .iter()
                .map(|p| p.initial_transform(true).to_kurbo())
                .collect(),
            catalog: xref.get::<Dict<'_>>(xref.root_id()),
        };
        let transform = resolver.transforms[index];
        let mut out = Vec::new();
        for annot in annots.iter::<Dict<'_>>().take(MAX_ANNOTATIONS_PER_PAGE) {
            let subtype = annot
                .get::<Name<'_>>(b"Subtype")
                .map(|n| n.as_str().to_owned())
                .unwrap_or_default();
            let rect = annot
                .get::<[f32; 4]>(b"Rect")
                .map(|[a, b, c, d]| {
                    let r = transform.transform_rect_bbox(Rect::new(
                        f64::from(a),
                        f64::from(b),
                        f64::from(c),
                        f64::from(d),
                    ));
                    Bounds {
                        x0: r.x0,
                        y0: r.y0,
                        x1: r.x1,
                        y1: r.y1,
                    }
                })
                .unwrap_or_default();
            let kind = match subtype.as_str() {
                "Link" => AnnotationKind::Link(resolver.link(&annot)),
                "Text" | "FreeText" => AnnotationKind::Note {
                    free_text: subtype == "FreeText",
                    reply: annot.contains_key(b"IRT"),
                },
                "Popup" => AnnotationKind::Popup,
                "Widget" => AnnotationKind::Widget,
                "FileAttachment" => AnnotationKind::FileAttachment,
                _ => AnnotationKind::Other,
            };
            let date = annot
                .get::<hayro_syntax::object::String<'_>>(b"M")
                .or_else(|| annot.get::<hayro_syntax::object::String<'_>>(b"CreationDate"))
                .and_then(|s| pdf_date(&String::from_utf8_lossy(s.as_bytes())));
            out.push(AnnotationEvidence {
                kind,
                subtype,
                rect,
                author: string(&annot, b"T"),
                contents: string(&annot, b"Contents"),
                date,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{pdf_date, text_string};

    #[test]
    fn pdf_dates_become_iso_8601_with_their_offset() {
        assert_eq!(
            pdf_date("D:20261029123005+01'00'").as_deref(),
            Some("2026-10-29T12:30:05+01:00")
        );
        assert_eq!(pdf_date("D:2026").as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(
            pdf_date("D:20261029").as_deref(),
            Some("2026-10-29T00:00:00Z")
        );
        assert_eq!(pdf_date("D:2026x"), None);
        assert_eq!(pdf_date("D:20261329"), None);
    }

    #[test]
    fn text_strings_decode_utf16_and_pdfdoc() {
        assert_eq!(text_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0xE9]), "Aé");
        assert_eq!(text_string(b"Ann\xe9e"), "Année");
        assert_eq!(text_string(b"a\rb"), "a\nb");
    }
}
