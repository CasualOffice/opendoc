//! What a byte-preserving save needs from the original file (`docs/182` §7,
//! P4): the last revision's cross-reference position and trailer, and, for each
//! page it rewrites, the page dictionary's entries, the effective resources and
//! the decoded content.
//!
//! Reading uses `hayro-syntax`, the parser this crate already admits hostile
//! files through; nothing here writes. The writer (`casual-doc-pdf`) appends the
//! update. Every value is handed over as the **raw bytes** it has in the file,
//! so an entry the save does not understand is carried, not re-serialized.
//!
//! Complexity: [`PdfRevision::open`] is the parser's open plus O(tail);
//! [`PdfRevision::page_parts`] is O(page dictionary + content stream).

use std::collections::BTreeSet;
use std::sync::Arc;

use hayro_interpret::TransformExt;
use hayro_syntax::Pdf;
use hayro_syntax::object::{Dict, ObjectIdentifier};

use crate::content::{Lexer, RewriteRefusal, Token};

/// Why a file, or one of its pages, cannot take a byte-preserving update. The
/// save falls back to the semantic writer and reports [`Self::id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreserveRefusal {
    /// The bytes do not open, or the last `startxref` does not point at a
    /// cross-reference section with a usable trailer (a repaired file).
    XrefUnreadable,
    /// The trailer names `/Encrypt`.
    Encrypted,
    /// The page is missing, or its dictionary or resources do not parse.
    PageUnreadable,
    /// The page's content stream does not decode.
    ContentUndecodable,
    /// The content cannot be rewritten safely.
    Rewrite(RewriteRefusal),
}

impl PreserveRefusal {
    /// A stable id for the report.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::XrefUnreadable => "xref_unreadable",
            Self::Encrypted => "encrypted",
            Self::PageUnreadable => "page_unreadable",
            Self::ContentUndecodable => "content_undecodable",
            Self::Rewrite(refusal) => refusal.id(),
        }
    }
}

impl From<RewriteRefusal> for PreserveRefusal {
    fn from(refusal: RewriteRefusal) -> Self {
        Self::Rewrite(refusal)
    }
}

/// The last revision's cross-reference facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionTail {
    /// The byte offset the last `startxref` names: the update's `/Prev`.
    pub startxref: usize,
    /// The trailer's `/Size`: the first object number the update may use.
    pub size: u32,
    /// `/Root`, raw (`1 0 R`).
    pub root: Vec<u8>,
    /// `/Info`, raw, when present.
    pub info: Option<Vec<u8>>,
    /// The first element of `/ID`, raw, when present (an update keeps it and
    /// changes the second, ISO 32000-1 §14.4).
    pub id_first: Option<Vec<u8>>,
    /// Whether the last section is a cross-reference stream (the update then
    /// writes one too).
    pub xref_stream: bool,
    /// Whether the form declares signatures (`/AcroForm /SigFlags` bit 1).
    pub signed: bool,
    /// Whether the catalog carries `/Perms` (a certification or usage-rights
    /// signature, which a validator reports as broken by any later change).
    pub certified: bool,
}

/// One page's parts, raw, for the rewritten page dictionary.
#[derive(Clone, Debug, PartialEq)]
pub struct PageParts {
    /// The page object's number and generation: the update replaces it.
    pub object: (u32, u16),
    /// The page dictionary's entries except `/Contents` and `/Resources`,
    /// raw and in file order (`/Parent`, so inheritance still applies).
    pub entries: Vec<u8>,
    /// The effective (possibly inherited) resources' entries except
    /// `/XObject`, raw.
    pub resource_entries: Vec<u8>,
    /// The effective `/XObject` dictionary's entries, raw (a referenced one is
    /// inlined, so the shared object stays untouched).
    pub xobject_entries: Vec<u8>,
    /// The names already in `/XObject`, so a new one cannot collide.
    pub xobject_names: BTreeSet<Vec<u8>>,
    /// The decoded content, every stream of `/Contents` concatenated.
    pub content: Vec<u8>,
    /// Maps the displayed page (CropBox origin, `/Rotate` applied, y up, the
    /// space the writer draws in) to the page's default user space.
    pub user_from_display: [f64; 6],
    /// The displayed width and height in points.
    pub display_size: (f64, f64),
}

/// An opened original, for one save.
pub struct PdfRevision {
    pdf: Pdf,
    tail: RevisionTail,
}

impl std::fmt::Debug for PdfRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfRevision")
            .field("tail", &self.tail)
            .finish_non_exhaustive()
    }
}

/// A dictionary entry: the key without its slash, and the value's byte range.
pub type DictEntry = (Vec<u8>, (usize, usize));

/// A dictionary's top-level entries. `None` when it is not a well-formed
/// dictionary. O(bytes).
#[must_use]
pub fn dict_entries(data: &[u8]) -> Option<Vec<DictEntry>> {
    let mut lexer = Lexer::new(data, 0);
    match lexer.next()?.ok()? {
        Token::Open(s, e) if &data[s..e] == b"<<" => {}
        _ => return None,
    }
    let mut entries = Vec::new();
    loop {
        let key = match lexer.next()?.ok()? {
            Token::Close(s, e) if &data[s..e] == b">>" => return Some(entries),
            Token::Atom(s, e) if data[s] == b'/' => data[s + 1..e].to_vec(),
            _ => return None,
        };
        let value = value_span(data, &mut lexer)?;
        entries.push((key, value));
    }
}

fn is_integer(token: &[u8]) -> bool {
    !token.is_empty() && token.iter().all(u8::is_ascii_digit)
}

/// One object's range: an atom, a balanced container, or `n g R`.
fn value_span(data: &[u8], lexer: &mut Lexer<'_>) -> Option<(usize, usize)> {
    match lexer.next()?.ok()? {
        Token::Atom(s, e) => {
            if is_integer(&data[s..e]) {
                let mut ahead = Lexer::new(data, lexer.at);
                if let (Some(Ok(Token::Atom(gs, ge))), Some(Ok(Token::Atom(rs, re)))) =
                    (ahead.next(), ahead.next())
                    && is_integer(&data[gs..ge])
                    && &data[rs..re] == b"R"
                {
                    lexer.at = ahead.at;
                    return Some((s, re));
                }
            }
            Some((s, e))
        }
        Token::Open(s, _) => {
            let mut depth = 1usize;
            loop {
                match lexer.next()?.ok()? {
                    Token::Open(..) => depth += 1,
                    Token::Close(_, e) => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((s, e));
                        }
                    }
                    Token::Atom(..) => {}
                }
            }
        }
        Token::Close(..) => None,
    }
}

fn value<'d>(data: &'d [u8], entries: &[DictEntry], key: &[u8]) -> Option<&'d [u8]> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, (s, e))| &data[*s..*e])
}

/// `entries` re-serialized as `/Key value` pairs, skipping `skip`.
fn entries_except(data: &[u8], entries: &[DictEntry], skip: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for (key, (s, e)) in entries {
        if skip.contains(&key.as_slice()) {
            continue;
        }
        out.push(b'/');
        out.extend_from_slice(key);
        out.push(b' ');
        out.extend_from_slice(&data[*s..*e]);
        out.push(b'\n');
    }
    out
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn find_from(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Reads the last revision's `startxref` and trailer. O(tail + trailer).
///
/// # Errors
///
/// [`PreserveRefusal::XrefUnreadable`] or [`PreserveRefusal::Encrypted`].
pub fn revision_tail(bytes: &[u8]) -> Result<RevisionTail, PreserveRefusal> {
    let unreadable = PreserveRefusal::XrefUnreadable;
    let window = bytes.len().saturating_sub(4096);
    let at = find_last(&bytes[window..], b"startxref").ok_or(unreadable)? + window;
    let mut lexer = Lexer::new(bytes, at + b"startxref".len());
    let Some(Ok(Token::Atom(s, e))) = lexer.next() else {
        return Err(unreadable);
    };
    let startxref: usize = std::str::from_utf8(&bytes[s..e])
        .ok()
        .and_then(|t| t.parse().ok())
        .ok_or(unreadable)?;
    if startxref >= at {
        return Err(unreadable);
    }
    let mut lexer = Lexer::new(bytes, startxref);
    lexer.skip_white();
    let (dict_start, xref_stream) = if bytes[lexer.at..].starts_with(b"xref") {
        let trailer = find_from(bytes, lexer.at, b"trailer").ok_or(unreadable)?;
        let mut lexer = Lexer::new(bytes, trailer + b"trailer".len());
        lexer.skip_white();
        (lexer.at, false)
    } else {
        // `n g obj <<…>> stream`.
        for expected in [None, None, Some(&b"obj"[..])] {
            match lexer.next() {
                Some(Ok(Token::Atom(s, e))) => match expected {
                    Some(word) if &bytes[s..e] != word => return Err(unreadable),
                    None if !is_integer(&bytes[s..e]) => return Err(unreadable),
                    _ => {}
                },
                _ => return Err(unreadable),
            }
        }
        lexer.skip_white();
        (lexer.at, true)
    };
    let mut span = Lexer::new(bytes, dict_start);
    let (s, e) = value_span(bytes, &mut span).ok_or(unreadable)?;
    let dict = &bytes[s..e];
    let entries = dict_entries(dict).ok_or(unreadable)?;
    if xref_stream && value(dict, &entries, b"Type") != Some(b"/XRef") {
        return Err(unreadable);
    }
    if value(dict, &entries, b"Encrypt").is_some() {
        return Err(PreserveRefusal::Encrypted);
    }
    let size = value(dict, &entries, b"Size")
        .and_then(|v| std::str::from_utf8(v).ok())
        .and_then(|v| v.parse().ok())
        .ok_or(unreadable)?;
    let root = value(dict, &entries, b"Root").ok_or(unreadable)?.to_vec();
    let info = value(dict, &entries, b"Info").map(<[u8]>::to_vec);
    let id_first = value(dict, &entries, b"ID").and_then(|id| {
        let mut lexer = Lexer::new(id, 0);
        match (lexer.next(), lexer.next()) {
            (Some(Ok(Token::Open(..))), Some(Ok(Token::Atom(s, e)))) => Some(id[s..e].to_vec()),
            _ => None,
        }
    });
    Ok(RevisionTail {
        startxref,
        size,
        root,
        info,
        id_first,
        xref_stream,
        signed: false,
        certified: false,
    })
}

impl PdfRevision {
    /// Opens the original for a save. The bytes were admitted at import, so
    /// this parses without re-running the import limits.
    ///
    /// # Errors
    ///
    /// [`PreserveRefusal::XrefUnreadable`] or [`PreserveRefusal::Encrypted`].
    pub fn open(bytes: Arc<Vec<u8>>) -> Result<Self, PreserveRefusal> {
        let mut tail = revision_tail(&bytes)?;
        let pdf = Pdf::new(bytes).map_err(|_| PreserveRefusal::XrefUnreadable)?;
        let root = pdf.xref().root_id();
        if let Some(catalog) = pdf.xref().get::<Dict<'_>>(root) {
            tail.certified = catalog.contains_key(b"Perms");
            tail.signed = catalog
                .get::<Dict<'_>>(b"AcroForm")
                .and_then(|form| form.get::<i32>(b"SigFlags"))
                .is_some_and(|flags| flags & 1 != 0);
        }
        Ok(Self { pdf, tail })
    }

    /// The last revision's facts.
    #[must_use]
    pub fn tail(&self) -> &RevisionTail {
        &self.tail
    }

    /// Pages in the file.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// The page object's number and generation.
    #[must_use]
    pub fn page_object(&self, index: usize) -> Option<(u32, u16)> {
        let id = self.pdf.pages().get(index)?.raw().obj_id()?;
        object_pair(id)
    }

    /// The parts of page `index` a rewritten page dictionary is built from.
    ///
    /// # Errors
    ///
    /// [`PreserveRefusal::PageUnreadable`] or
    /// [`PreserveRefusal::ContentUndecodable`].
    pub fn page_parts(&self, index: usize) -> Result<PageParts, PreserveRefusal> {
        let unreadable = PreserveRefusal::PageUnreadable;
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or(unreadable)?;
        let raw = page.raw();
        let object = raw.obj_id().and_then(object_pair).ok_or(unreadable)?;
        let data = raw.data();
        let page_entries = dict_entries(data).ok_or(unreadable)?;
        let entries = entries_except(data, &page_entries, &[b"Contents", b"Resources"]);

        // The effective resources: the page's own, else the nearest
        // ancestor's (ISO 32000-1 §7.7.3.4).
        let mut node: Dict<'_> = raw.clone();
        let mut resources = None;
        for _ in 0..64 {
            if let Some(found) = node.get::<Dict<'_>>(b"Resources") {
                resources = Some(found);
                break;
            }
            match node.get::<Dict<'_>>(b"Parent") {
                Some(parent) => node = parent,
                None => break,
            }
        }
        let (resource_entries, xobject_entries, xobject_names) = match resources {
            None => (Vec::new(), Vec::new(), BTreeSet::new()),
            Some(resources) => {
                let data = resources.data();
                let entries = dict_entries(data).ok_or(unreadable)?;
                let rest = entries_except(data, &entries, &[b"XObject"]);
                let (xobjects, names) = match resources.get::<Dict<'_>>(b"XObject") {
                    None => (Vec::new(), BTreeSet::new()),
                    Some(xobjects) => {
                        let data = xobjects.data();
                        let entries = dict_entries(data).ok_or(unreadable)?;
                        let names = entries.iter().map(|(k, _)| k.clone()).collect();
                        (entries_except(data, &entries, &[]), names)
                    }
                };
                (rest, xobjects, names)
            }
        };
        let content = if raw.contains_key(b"Contents") {
            page.page_stream()
                .ok_or(PreserveRefusal::ContentUndecodable)?
                .to_vec()
        } else {
            Vec::new()
        };
        let display_from_user = page.initial_transform(false).to_kurbo();
        let user_from_display = display_from_user.inverse().as_coeffs();
        let (width, height) = page.render_dimensions();
        Ok(PageParts {
            object,
            entries,
            resource_entries,
            xobject_entries,
            xobject_names,
            content,
            user_from_display,
            display_size: (f64::from(width), f64::from(height)),
        })
    }
}

/// The hayro half of a byte-preserving save (`docs/182` §19, §17a): opens the
/// original's last revision, checks the page count and each requested page's
/// object against what the import recorded, reads those pages' parts and
/// removes the requested shows. The refusals and their order are the ones the
/// in-process save always had: the file (`xref_unreadable`, `encrypted`),
/// `page_count_changed`, then per page `page_object_moved`, the page's own
/// refusal, the rewrite's.
///
/// O(original) to open, plus O(decoded content) of the requested pages.
///
/// # Errors
///
/// The first [`casual_doc_pdf_wire::Refusal`].
pub fn read_for_save(
    original: Arc<Vec<u8>>,
    request: &casual_doc_pdf_wire::RevisionRequest,
) -> Result<casual_doc_pdf_wire::RevisionParts, casual_doc_pdf_wire::Refusal> {
    use casual_doc_pdf_wire as wire;
    let refuse = |reason: &str, page: Option<u32>| wire::Refusal {
        reason: reason.to_owned(),
        page,
    };
    let revision = PdfRevision::open(original).map_err(|r| refuse(r.id(), None))?;
    if revision.page_count() != request.expected_pages as usize {
        return Err(refuse("page_count_changed", None));
    }
    let mut pages = Vec::with_capacity(request.pages.len());
    for page in &request.pages {
        let index = page.index as usize;
        if revision.page_object(index) != page.object {
            return Err(refuse("page_object_moved", Some(page.index)));
        }
        let parts = revision
            .page_parts(index)
            .map_err(|r| refuse(r.id(), Some(page.index)))?;
        let remove: BTreeSet<u32> = page.remove.iter().copied().collect();
        let kept = crate::content::remove_shows(&parts.content, &remove)
            .map_err(|r| refuse(r.id(), Some(page.index)))?;
        pages.push(wire::PageParts {
            index: page.index,
            object: parts.object,
            entries: parts.entries,
            resource_entries: parts.resource_entries,
            xobject_entries: parts.xobject_entries,
            xobject_names: parts.xobject_names.into_iter().collect(),
            kept_content: kept,
            user_from_display: parts.user_from_display,
            display_size: parts.display_size,
        });
    }
    let tail = revision.tail();
    Ok(wire::RevisionParts {
        page_count: u32::try_from(revision.page_count()).unwrap_or(u32::MAX),
        tail: wire::RevisionTail {
            startxref: tail.startxref as u64,
            size: tail.size,
            root: tail.root.clone(),
            info: tail.info.clone(),
            id_first: tail.id_first.clone(),
            xref_stream: tail.xref_stream,
            signed: tail.signed,
            certified: tail.certified,
        },
        pages,
    })
}

fn object_pair(id: ObjectIdentifier) -> Option<(u32, u16)> {
    Some((
        u32::try_from(id.obj_number).ok()?,
        u16::try_from(id.gen_number).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");

    #[test]
    fn dictionaries_split_into_raw_entries_with_references_whole() {
        let data: &[u8] = b"<< /Type /Page /Parent 2 0 R /Box [0 0 612 792] \
            /R << /F1 5 0 R >> /S (a >> b) /N 12 >>";
        let entries = dict_entries(data).unwrap();
        let text: Vec<(String, String)> = entries
            .iter()
            .map(|(k, (s, e))| {
                (
                    String::from_utf8_lossy(k).into_owned(),
                    String::from_utf8_lossy(&data[*s..*e]).into_owned(),
                )
            })
            .collect();
        assert_eq!(
            text,
            [
                ("Type", "/Page"),
                ("Parent", "2 0 R"),
                ("Box", "[0 0 612 792]"),
                ("R", "<< /F1 5 0 R >>"),
                ("S", "(a >> b)"),
                ("N", "12"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
        );
    }

    #[test]
    fn the_tail_names_the_last_section_and_the_trailer() {
        let tail = revision_tail(PROSE).unwrap();
        assert_eq!(tail.startxref, 1819);
        assert_eq!(tail.size, 7);
        assert_eq!(tail.root, b"1 0 R");
        assert!(!tail.xref_stream);
        assert!(revision_tail(b"%PDF-1.7\nnothing here").is_err());
    }

    #[test]
    fn a_page_gives_its_entries_resources_and_content() {
        let revision = PdfRevision::open(Arc::new(PROSE.to_vec())).unwrap();
        let parts = revision.page_parts(0).unwrap();
        assert_eq!(parts.object, (4, 0));
        let entries = String::from_utf8_lossy(&parts.entries);
        assert!(entries.contains("/Parent 2 0 R"), "{entries}");
        assert!(!entries.contains("/Contents"), "{entries}");
        assert!(String::from_utf8_lossy(&parts.resource_entries).contains("/Font"));
        assert!(parts.content.starts_with(b"BT /F3 18 Tf"));
        assert_eq!(parts.user_from_display, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(parts.display_size, (612.0, 792.0));
    }
}
