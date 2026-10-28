// SPDX-License-Identifier: Apache-2.0

//! What crosses between the editor's main WebAssembly module and the PDF
//! module it loads the first time a PDF is opened (`docs/182` §17a).
//!
//! The main module must not link hayro, `casual-doc-pdf-read` or
//! `casual-doc-pdf-import`; the PDF module must not link the layout engine and
//! its 9 MB of faces. What one hands the other is therefore **bytes**, and this
//! crate is their one definition, linked by both:
//!
//! - [`ImportPackage`]: a finished import. The document as the normalized JSON
//!   snapshot (`Document::to_json`, the format a shared snapshot already uses),
//!   its media, the `pdf.import.*` findings, the [`SourceMap`] the
//!   byte-preserving save needs, what OCR did ([`OcrSummary`]), and the length
//!   and FNV-1a hash of the original, so the main module can check that the
//!   original bytes it keeps for the exact save are the ones imported. The
//!   original itself does not travel in the package: the host already has it.
//! - [`RevisionRequest`] / [`RevisionParts`]: the hayro half of the P4 save
//!   (`docs/182` §19). The main module decides which pages changed and which
//!   show operators to remove; the PDF module opens the original's last
//!   revision, reads those pages' parts and removes the shows; the main module
//!   lays out the overlay and writes the update.
//!
//! Decoding is bounded: every length is checked against the bytes left before
//! anything is allocated, and a trailing byte is an error. The bytes come from
//! our own other module, through the host's JavaScript, so they are treated as
//! input, not trusted.
//!
//! ```
//! use casual_doc_pdf_wire::{RevisionRequest, PageRequest};
//! let request = RevisionRequest {
//!     expected_pages: 2,
//!     pages: vec![PageRequest { index: 1, object: Some((7, 0)), remove: vec![0, 2] }],
//! };
//! let bytes = request.encode();
//! assert_eq!(RevisionRequest::decode(&bytes).unwrap(), request);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::fmt;

/// Why bytes did not decode. Carries no document content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireError(pub &'static str);

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed PDF module message: {}", self.0)
    }
}

impl std::error::Error for WireError {}

/// FNV-1a over `bytes`: the same hash `casual-doc-pdf-import` namespaces ids
/// with. O(bytes).
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// How a source construct was represented (`PdfModelOutcome`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Represented; reported because it was inferred.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// One aggregated `pdf.import.*` finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// The stable feature id.
    pub feature: String,
    /// Occurrences.
    pub occurrences: u32,
    /// The first zero-based page it was seen on.
    pub first_page: Option<u32>,
    /// What the import did with it.
    pub outcome: Outcome,
}

/// One page's origin in the original file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSource {
    /// The page object's number and generation.
    pub object: Option<(u32, u16)>,
    /// The page stream's show-operator count, when every run is numbered.
    pub shows: Option<u32>,
}

/// One text frame's origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSource {
    /// The page index.
    pub page: u32,
    /// The `TextBox` node id the frame became (`NodeId::as_u128`).
    pub frame: u128,
    /// The show operators that painted its glyphs, ascending.
    pub shows: Vec<u32>,
    /// Whether every glyph has a known show.
    pub complete: bool,
}

/// Where each page and frame came from (`casual-doc-pdf-import`'s `SourceMap`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// One per page.
    pub pages: Vec<PageSource>,
    /// One per text frame.
    pub frames: Vec<FrameSource>,
}

/// What OCR did, for the summary the editor shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OcrSummary {
    /// Pages with no valid native text and something to read optically.
    pub raster_pages: u32,
    /// Pages with native text and something to read optically.
    pub hybrid_pages: u32,
    /// Regions recognised.
    pub regions_recognized: u32,
    /// Regions that needed OCR and were not read (no model, or a budget).
    pub regions_skipped: u32,
    /// Optical words kept in the document.
    pub words_kept: u32,
    /// The recogniser, when one ran (`ProviderManifest::provider`).
    pub provider: Option<String>,
}

/// A finished PDF import, as the PDF module hands it to the main module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportPackage {
    /// The document as the normalized JSON snapshot.
    pub document_json: Vec<u8>,
    /// Media bytes keyed by the part names the document references.
    pub resources: Vec<(String, Vec<u8>)>,
    /// The `pdf.import.*` findings.
    pub findings: Vec<Finding>,
    /// Page objects and frame operators, for the byte-preserving save.
    pub sources: SourceMap,
    /// What OCR did.
    pub ocr: OcrSummary,
    /// The original's length.
    pub original_len: u64,
    /// The original's [`fnv1a64`].
    pub original_hash: u64,
    /// The encoded [`Baseline`] for the byte-preserving save; empty when the
    /// imported document could not be digested.
    pub baseline: Vec<u8>,
}

/// One edited page the save asks the PDF module to prepare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRequest {
    /// Zero-based page index.
    pub index: u32,
    /// The page object the import recorded; a different one now is a refusal.
    pub object: Option<(u32, u16)>,
    /// The show operators to remove, ascending.
    pub remove: Vec<u32>,
}

/// The pages a byte-preserving save rewrites.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevisionRequest {
    /// The page count the import recorded; a file with another is refused
    /// (`page_count_changed`).
    pub expected_pages: u32,
    /// In page order.
    pub pages: Vec<PageRequest>,
}

/// The original's last revision (`casual-doc-pdf-read`'s `RevisionTail`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevisionTail {
    /// The offset the last `startxref` names.
    pub startxref: u64,
    /// The trailer's `/Size`.
    pub size: u32,
    /// `/Root`, raw.
    pub root: Vec<u8>,
    /// `/Info`, raw.
    pub info: Option<Vec<u8>>,
    /// The first `/ID` element, raw.
    pub id_first: Option<Vec<u8>>,
    /// Whether the last section is a cross-reference stream.
    pub xref_stream: bool,
    /// `/AcroForm /SigFlags` bit 1.
    pub signed: bool,
    /// The catalog carries `/Perms`.
    pub certified: bool,
}

/// One edited page's parts (`PageParts`), its content already rewritten.
#[derive(Clone, Debug, PartialEq)]
pub struct PageParts {
    /// The page index.
    pub index: u32,
    /// The page object's number and generation.
    pub object: (u32, u16),
    /// The page dictionary's entries except `/Contents` and `/Resources`.
    pub entries: Vec<u8>,
    /// The effective resources' entries except `/XObject`.
    pub resource_entries: Vec<u8>,
    /// The effective `/XObject` entries.
    pub xobject_entries: Vec<u8>,
    /// The names already in `/XObject`.
    pub xobject_names: Vec<Vec<u8>>,
    /// The content with the requested shows removed.
    pub kept_content: Vec<u8>,
    /// Displayed page space to user space.
    pub user_from_display: [f64; 6],
    /// The displayed width and height in points.
    pub display_size: (f64, f64),
}

/// What the PDF module read for a save.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RevisionParts {
    /// Pages in the original's last revision.
    pub page_count: u32,
    /// The original's last revision.
    pub tail: RevisionTail,
    /// One per requested page, in request order.
    pub pages: Vec<PageParts>,
}

/// Why the PDF module could not prepare a save; the save falls back to the
/// semantic writer and reports `pdf.export.preserve_fallback.<reason>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// A stable reason id (`xref_unreadable`, `kept_text_would_move`, …).
    pub reason: String,
    /// The page it concerns.
    pub page: Option<u32>,
}

const PACKAGE_MAGIC: &[u8; 8] = b"ODPDFPK1";
const REQUEST_MAGIC: &[u8; 8] = b"ODPDFRQ1";

struct Out(Vec<u8>);

impl Out {
    fn new(magic: &[u8; 8]) -> Self {
        Self(magic.to_vec())
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u128(&mut self, v: u128) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u64(n as u64);
    }
    fn bytes(&mut self, v: &[u8]) {
        self.len(v.len());
        self.0.extend_from_slice(v);
    }
    fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn opt_bytes(&mut self, v: Option<&[u8]>) {
        self.bool(v.is_some());
        if let Some(v) = v {
            self.bytes(v);
        }
    }
    fn opt_u32(&mut self, v: Option<u32>) {
        self.bool(v.is_some());
        if let Some(v) = v {
            self.u32(v);
        }
    }
    fn object(&mut self, v: Option<(u32, u16)>) {
        self.bool(v.is_some());
        if let Some((n, g)) = v {
            self.u32(n);
            self.u16(g);
        }
    }
    fn u32s(&mut self, v: &[u32]) {
        self.len(v.len());
        for x in v {
            self.u32(*x);
        }
    }
}

struct In<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    fn new(bytes: &'a [u8], magic: &[u8; 8]) -> Result<Self, WireError> {
        if bytes.len() < 8 || &bytes[..8] != magic {
            return Err(WireError("wrong magic"));
        }
        Ok(Self { bytes, at: 8 })
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(WireError("truncated"))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError("bad flag")),
        }
    }
    fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn u128(&mut self) -> Result<u128, WireError> {
        Ok(u128::from_le_bytes(self.array()?))
    }
    fn f64(&mut self) -> Result<f64, WireError> {
        Ok(f64::from_le_bytes(self.array()?))
    }
    /// A count of items at least `min_item` bytes each: refused before any
    /// allocation when the bytes left cannot hold it.
    fn count(&mut self, min_item: usize) -> Result<usize, WireError> {
        let n = usize::try_from(self.u64()?).map_err(|_| WireError("length"))?;
        let left = self.bytes.len() - self.at;
        if n.checked_mul(min_item.max(1))
            .is_none_or(|need| need > left)
        {
            return Err(WireError("length exceeds message"));
        }
        Ok(n)
    }
    fn bytes(&mut self) -> Result<Vec<u8>, WireError> {
        let n = self.count(1)?;
        Ok(self.take(n)?.to_vec())
    }
    fn string(&mut self) -> Result<String, WireError> {
        String::from_utf8(self.bytes()?).map_err(|_| WireError("not UTF-8"))
    }
    fn opt_bytes(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        Ok(if self.bool()? {
            Some(self.bytes()?)
        } else {
            None
        })
    }
    fn opt_u32(&mut self) -> Result<Option<u32>, WireError> {
        Ok(if self.bool()? {
            Some(self.u32()?)
        } else {
            None
        })
    }
    fn object(&mut self) -> Result<Option<(u32, u16)>, WireError> {
        Ok(if self.bool()? {
            Some((self.u32()?, self.u16()?))
        } else {
            None
        })
    }
    fn u32s(&mut self) -> Result<Vec<u32>, WireError> {
        let n = self.count(4)?;
        (0..n).map(|_| self.u32()).collect()
    }
    fn end(self) -> Result<(), WireError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(WireError("trailing bytes"))
        }
    }
}

impl ImportPackage {
    /// The package's bytes. O(package).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new(PACKAGE_MAGIC);
        out.bytes(&self.document_json);
        out.len(self.resources.len());
        for (name, bytes) in &self.resources {
            out.str(name);
            out.bytes(bytes);
        }
        out.len(self.findings.len());
        for finding in &self.findings {
            out.str(&finding.feature);
            out.u32(finding.occurrences);
            out.opt_u32(finding.first_page);
            out.u8(match finding.outcome {
                Outcome::Mapped => 0,
                Outcome::Degraded => 1,
                Outcome::Omitted => 2,
            });
        }
        out.len(self.sources.pages.len());
        for page in &self.sources.pages {
            out.object(page.object);
            out.opt_u32(page.shows);
        }
        out.len(self.sources.frames.len());
        for frame in &self.sources.frames {
            out.u32(frame.page);
            out.u128(frame.frame);
            out.u32s(&frame.shows);
            out.bool(frame.complete);
        }
        let ocr = &self.ocr;
        for v in [
            ocr.raster_pages,
            ocr.hybrid_pages,
            ocr.regions_recognized,
            ocr.regions_skipped,
            ocr.words_kept,
        ] {
            out.u32(v);
        }
        out.opt_bytes(ocr.provider.as_deref().map(str::as_bytes));
        out.u64(self.original_len);
        out.u64(self.original_hash);
        out.bytes(&self.baseline);
        out.0
    }

    /// Decodes a package. O(bytes).
    ///
    /// # Errors
    ///
    /// [`WireError`] for anything but exactly one well-formed package.
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = In::new(bytes, PACKAGE_MAGIC)?;
        let document_json = r.bytes()?;
        let n = r.count(16)?;
        let mut resources = Vec::with_capacity(n);
        for _ in 0..n {
            resources.push((r.string()?, r.bytes()?));
        }
        let n = r.count(14)?;
        let mut findings = Vec::with_capacity(n);
        for _ in 0..n {
            findings.push(Finding {
                feature: r.string()?,
                occurrences: r.u32()?,
                first_page: r.opt_u32()?,
                outcome: match r.u8()? {
                    0 => Outcome::Mapped,
                    1 => Outcome::Degraded,
                    2 => Outcome::Omitted,
                    _ => return Err(WireError("bad outcome")),
                },
            });
        }
        let n = r.count(2)?;
        let mut pages = Vec::with_capacity(n);
        for _ in 0..n {
            pages.push(PageSource {
                object: r.object()?,
                shows: r.opt_u32()?,
            });
        }
        let n = r.count(29)?;
        let mut frames = Vec::with_capacity(n);
        for _ in 0..n {
            frames.push(FrameSource {
                page: r.u32()?,
                frame: r.u128()?,
                shows: r.u32s()?,
                complete: r.bool()?,
            });
        }
        let ocr = OcrSummary {
            raster_pages: r.u32()?,
            hybrid_pages: r.u32()?,
            regions_recognized: r.u32()?,
            regions_skipped: r.u32()?,
            words_kept: r.u32()?,
            provider: r
                .opt_bytes()?
                .map(|b| String::from_utf8(b).map_err(|_| WireError("not UTF-8")))
                .transpose()?,
        };
        let original_len = r.u64()?;
        let original_hash = r.u64()?;
        let baseline = r.bytes()?;
        r.end()?;
        Ok(Self {
            document_json,
            resources,
            findings,
            sources: SourceMap { pages, frames },
            ocr,
            original_len,
            original_hash,
            baseline,
        })
    }
}

impl RevisionRequest {
    /// The request's bytes. O(request).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new(REQUEST_MAGIC);
        out.u32(self.expected_pages);
        out.len(self.pages.len());
        for page in &self.pages {
            out.u32(page.index);
            out.object(page.object);
            out.u32s(&page.remove);
        }
        out.0
    }

    /// Decodes a request. O(bytes).
    ///
    /// # Errors
    ///
    /// [`WireError`].
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = In::new(bytes, REQUEST_MAGIC)?;
        let expected_pages = r.u32()?;
        let n = r.count(13)?;
        let mut pages = Vec::with_capacity(n);
        for _ in 0..n {
            pages.push(PageRequest {
                index: r.u32()?,
                object: r.object()?,
                remove: r.u32s()?,
            });
        }
        r.end()?;
        Ok(Self {
            expected_pages,
            pages,
        })
    }
}

/// One section's digests (`casual-doc-pdf-save`'s baseline).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SectionDigest {
    /// Everything on the page.
    pub whole: u128,
    /// Everything but the text frames, when the page is one anchor paragraph.
    pub fixed: Option<u128>,
    /// Each text frame's digest, by node id, ascending.
    pub frames: Vec<(u128, u128)>,
}

/// What a byte-preserving save compares the document against: the import's
/// content digests and the source map (`docs/182` §19.2). Opaque to the main
/// module, which keeps it beside the original and hands it back at save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Baseline {
    /// `casual-doc-diff`'s whole-document content digest.
    pub document: u128,
    /// The definitions without `sections`.
    pub definitions: u128,
    /// Every story but the body and comments.
    pub stories: u128,
    /// One per section.
    pub sections: Vec<SectionDigest>,
    /// Page objects and frame operators.
    pub sources: SourceMap,
}

const BASELINE_MAGIC: &[u8; 8] = b"ODPDFBL1";

impl Baseline {
    /// The baseline's bytes. O(sections + frames).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new(BASELINE_MAGIC);
        out.u128(self.document);
        out.u128(self.definitions);
        out.u128(self.stories);
        out.len(self.sections.len());
        for section in &self.sections {
            out.u128(section.whole);
            out.bool(section.fixed.is_some());
            if let Some(fixed) = section.fixed {
                out.u128(fixed);
            }
            out.len(section.frames.len());
            for (frame, digest) in &section.frames {
                out.u128(*frame);
                out.u128(*digest);
            }
        }
        put_sources(&mut out, &self.sources);
        out.0
    }

    /// Decodes a baseline. O(bytes).
    ///
    /// # Errors
    ///
    /// [`WireError`].
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = In::new(bytes, BASELINE_MAGIC)?;
        let document = r.u128()?;
        let definitions = r.u128()?;
        let stories = r.u128()?;
        let n = r.count(25)?;
        let mut sections = Vec::with_capacity(n);
        for _ in 0..n {
            let whole = r.u128()?;
            let fixed = if r.bool()? { Some(r.u128()?) } else { None };
            let frames_n = r.count(32)?;
            let mut frames = Vec::with_capacity(frames_n);
            for _ in 0..frames_n {
                frames.push((r.u128()?, r.u128()?));
            }
            sections.push(SectionDigest {
                whole,
                fixed,
                frames,
            });
        }
        let sources = get_sources(&mut r)?;
        r.end()?;
        Ok(Self {
            document,
            definitions,
            stories,
            sections,
            sources,
        })
    }
}

/// One page the update rewrites, as the planner decided it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedPage {
    /// The page index.
    pub index: u32,
    /// The paragraphs of the edited and added frames: the overlay draws their
    /// glyph runs and nothing else.
    pub paragraphs: Vec<u128>,
    /// Frames whose original text was removed.
    pub removed_frames: u32,
}

/// The PDF module's answer to "save this edited PDF-origin document"
/// (`docs/182` §17a): the main module lays out the overlay and writes the
/// update for [`SaveAnswer::Update`]; writes the original back for
/// [`SaveAnswer::WriteBack`]; falls back to the semantic writer, reporting the
/// reason, for [`SaveAnswer::Refused`].
#[derive(Clone, Debug, PartialEq)]
pub enum SaveAnswer {
    /// The content is the import's (an edit undone).
    WriteBack,
    /// Not provably safe; the reason is reported.
    Refused(Refusal),
    /// The edited pages, their parts with the removed shows already gone, and
    /// the original's last revision.
    Update {
        /// What the reader read.
        parts: RevisionParts,
        /// One per page in `parts.pages`, same order.
        pages: Vec<PlannedPage>,
    },
}

const ANSWER_MAGIC: &[u8; 8] = b"ODPDFSV1";

fn put_sources(out: &mut Out, sources: &SourceMap) {
    out.len(sources.pages.len());
    for page in &sources.pages {
        out.object(page.object);
        out.opt_u32(page.shows);
    }
    out.len(sources.frames.len());
    for frame in &sources.frames {
        out.u32(frame.page);
        out.u128(frame.frame);
        out.u32s(&frame.shows);
        out.bool(frame.complete);
    }
}

fn get_sources(r: &mut In<'_>) -> Result<SourceMap, WireError> {
    let n = r.count(2)?;
    let mut pages = Vec::with_capacity(n);
    for _ in 0..n {
        pages.push(PageSource {
            object: r.object()?,
            shows: r.opt_u32()?,
        });
    }
    let n = r.count(29)?;
    let mut frames = Vec::with_capacity(n);
    for _ in 0..n {
        frames.push(FrameSource {
            page: r.u32()?,
            frame: r.u128()?,
            shows: r.u32s()?,
            complete: r.bool()?,
        });
    }
    Ok(SourceMap { pages, frames })
}

impl SaveAnswer {
    /// The answer's bytes. O(answer).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::new(ANSWER_MAGIC);
        match self {
            Self::Refused(refusal) => {
                out.u8(0);
                out.str(&refusal.reason);
                out.opt_u32(refusal.page);
            }
            Self::WriteBack => out.u8(2),
            Self::Update { parts, pages } => {
                out.u8(1);
                out.u32(parts.page_count);
                let tail = &parts.tail;
                out.u64(tail.startxref);
                out.u32(tail.size);
                out.bytes(&tail.root);
                out.opt_bytes(tail.info.as_deref());
                out.opt_bytes(tail.id_first.as_deref());
                out.bool(tail.xref_stream);
                out.bool(tail.signed);
                out.bool(tail.certified);
                out.len(parts.pages.len());
                for page in &parts.pages {
                    out.u32(page.index);
                    out.u32(page.object.0);
                    out.u16(page.object.1);
                    out.bytes(&page.entries);
                    out.bytes(&page.resource_entries);
                    out.bytes(&page.xobject_entries);
                    out.len(page.xobject_names.len());
                    for name in &page.xobject_names {
                        out.bytes(name);
                    }
                    out.bytes(&page.kept_content);
                    for v in page.user_from_display {
                        out.f64(v);
                    }
                    out.f64(page.display_size.0);
                    out.f64(page.display_size.1);
                }
                out.len(pages.len());
                for page in pages {
                    out.u32(page.index);
                    out.len(page.paragraphs.len());
                    for paragraph in &page.paragraphs {
                        out.u128(*paragraph);
                    }
                    out.u32(page.removed_frames);
                }
            }
        }
        out.0
    }

    /// Decodes an answer. O(bytes).
    ///
    /// # Errors
    ///
    /// [`WireError`] when the bytes are not one well-formed answer.
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = In::new(bytes, ANSWER_MAGIC)?;
        let answer = match r.u8()? {
            0 => Self::Refused(Refusal {
                reason: r.string()?,
                page: r.opt_u32()?,
            }),
            2 => Self::WriteBack,
            1 => {
                let page_count = r.u32()?;
                let tail = RevisionTail {
                    startxref: r.u64()?,
                    size: r.u32()?,
                    root: r.bytes()?,
                    info: r.opt_bytes()?,
                    id_first: r.opt_bytes()?,
                    xref_stream: r.bool()?,
                    signed: r.bool()?,
                    certified: r.bool()?,
                };
                let n = r.count(114)?;
                let mut parts_pages = Vec::with_capacity(n);
                for _ in 0..n {
                    let index = r.u32()?;
                    let object = (r.u32()?, r.u16()?);
                    let entries = r.bytes()?;
                    let resource_entries = r.bytes()?;
                    let xobject_entries = r.bytes()?;
                    let names = r.count(8)?;
                    let mut xobject_names = Vec::with_capacity(names);
                    for _ in 0..names {
                        xobject_names.push(r.bytes()?);
                    }
                    let kept_content = r.bytes()?;
                    let mut user_from_display = [0.0; 6];
                    for v in &mut user_from_display {
                        *v = r.f64()?;
                    }
                    let display_size = (r.f64()?, r.f64()?);
                    parts_pages.push(PageParts {
                        index,
                        object,
                        entries,
                        resource_entries,
                        xobject_entries,
                        xobject_names,
                        kept_content,
                        user_from_display,
                        display_size,
                    });
                }
                let n = r.count(16)?;
                let mut pages = Vec::with_capacity(n);
                for _ in 0..n {
                    let index = r.u32()?;
                    let paragraphs_n = r.count(16)?;
                    let mut paragraphs = Vec::with_capacity(paragraphs_n);
                    for _ in 0..paragraphs_n {
                        paragraphs.push(r.u128()?);
                    }
                    pages.push(PlannedPage {
                        index,
                        paragraphs,
                        removed_frames: r.u32()?,
                    });
                }
                Self::Update {
                    parts: RevisionParts {
                        page_count,
                        tail,
                        pages: parts_pages,
                    },
                    pages,
                }
            }
            _ => return Err(WireError("bad answer tag")),
        };
        r.end()?;
        Ok(answer)
    }
}

/// Encodes the PDF module's answer to a [`RevisionRequest`] (parts, or a
/// refusal), as [`SaveAnswer`]'s bytes: `Update` with no planned pages.
#[must_use]
pub fn encode_revision(answer: &Result<RevisionParts, Refusal>) -> Vec<u8> {
    match answer {
        Ok(parts) => SaveAnswer::Update {
            parts: parts.clone(),
            pages: Vec::new(),
        },
        Err(refusal) => SaveAnswer::Refused(refusal.clone()),
    }
    .encode()
}

/// Decodes [`encode_revision`]'s bytes.
///
/// # Errors
///
/// [`WireError`] when the bytes are not one well-formed answer.
pub fn decode_revision(bytes: &[u8]) -> Result<Result<RevisionParts, Refusal>, WireError> {
    match SaveAnswer::decode(bytes)? {
        SaveAnswer::Update { parts, .. } => Ok(Ok(parts)),
        SaveAnswer::Refused(refusal) => Ok(Err(refusal)),
        SaveAnswer::WriteBack => Err(WireError("unexpected write-back answer")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> ImportPackage {
        ImportPackage {
            document_json: b"{\"v\":1}".to_vec(),
            resources: vec![("media/image1.png".to_owned(), vec![1, 2, 3])],
            findings: vec![Finding {
                feature: "pdf.import.font_substituted".to_owned(),
                occurrences: 3,
                first_page: Some(0),
                outcome: Outcome::Degraded,
            }],
            sources: SourceMap {
                pages: vec![PageSource {
                    object: Some((3, 0)),
                    shows: Some(4),
                }],
                frames: vec![FrameSource {
                    page: 0,
                    frame: 7 << 64 | 9,
                    shows: vec![0, 1],
                    complete: true,
                }],
            },
            ocr: OcrSummary {
                raster_pages: 1,
                regions_skipped: 2,
                provider: Some("pp-ocrv5".to_owned()),
                ..OcrSummary::default()
            },
            original_len: 1234,
            original_hash: fnv1a64(b"x"),
            baseline: vec![9, 9],
        }
    }

    #[test]
    fn a_package_round_trips() {
        let p = package();
        assert_eq!(ImportPackage::decode(&p.encode()).unwrap(), p);
    }

    #[test]
    fn a_save_answer_and_a_baseline_round_trip() {
        let parts = RevisionParts {
            page_count: 2,
            tail: RevisionTail {
                startxref: 99,
                size: 8,
                root: b"1 0 R".to_vec(),
                info: None,
                id_first: Some(b"<00>".to_vec()),
                xref_stream: true,
                signed: false,
                certified: true,
            },
            pages: vec![PageParts {
                index: 1,
                object: (5, 0),
                entries: b"/Parent 2 0 R".to_vec(),
                resource_entries: vec![],
                xobject_entries: vec![],
                xobject_names: vec![b"Im1".to_vec()],
                kept_content: b"q Q".to_vec(),
                user_from_display: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                display_size: (612.0, 792.0),
            }],
        };
        for answer in [
            SaveAnswer::Update {
                parts,
                pages: vec![PlannedPage {
                    index: 1,
                    paragraphs: vec![3, 4],
                    removed_frames: 1,
                }],
            },
            SaveAnswer::WriteBack,
            SaveAnswer::Refused(Refusal {
                reason: "kept_text_would_move".to_owned(),
                page: Some(1),
            }),
        ] {
            assert_eq!(SaveAnswer::decode(&answer.encode()).unwrap(), answer);
        }
        let baseline = Baseline {
            document: 1,
            definitions: 2,
            stories: 3,
            sections: vec![SectionDigest {
                whole: 4,
                fixed: Some(5),
                frames: vec![(6, 7)],
            }],
            sources: package().sources,
        };
        assert_eq!(Baseline::decode(&baseline.encode()).unwrap(), baseline);
    }

    #[test]
    fn a_truncated_or_padded_or_lying_message_is_refused() {
        let bytes = package().encode();
        for cut in [0, 7, 8, bytes.len() / 2, bytes.len() - 1] {
            assert!(
                ImportPackage::decode(&bytes[..cut]).is_err(),
                "cut at {cut}"
            );
        }
        let mut padded = bytes.clone();
        padded.push(0);
        assert_eq!(
            ImportPackage::decode(&padded),
            Err(WireError("trailing bytes"))
        );
        // A length that claims more than the message holds is refused before
        // anything is allocated for it.
        let mut lying = bytes;
        lying[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(
            ImportPackage::decode(&lying),
            Err(WireError("length exceeds message"))
        );
    }
}
