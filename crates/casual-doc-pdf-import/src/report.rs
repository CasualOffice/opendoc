//! PDF-layer findings, in the `docs/131` §16.1 `pdf.import.*` vocabulary.
//!
//! Mirrors the RTF crate's report so the `casual-doc-io` adapter's translation
//! is mechanical and this crate does not depend on the dispatch layer.
//! Findings aggregate by feature: one row per feature with an occurrence count
//! and the first page it was seen on, never one row per glyph (`docs/131`
//! §16.1).

use std::collections::BTreeMap;

/// How a source construct was represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PdfModelOutcome {
    /// Represented; reported because it was inferred (PDFR-I4).
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// One aggregated finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfFinding {
    /// Stable feature id, e.g. `pdf.import.font_substituted`.
    pub feature: &'static str,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Mapping result.
    pub outcome: PdfModelOutcome,
    /// The first (zero-based) page it occurred on, when page-bound.
    pub first_page: Option<usize>,
}

/// Deterministically ordered findings (by feature id).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PdfImportReport {
    /// The findings.
    pub findings: Vec<PdfFinding>,
}

impl PdfImportReport {
    /// The finding for `feature`, if reported.
    #[must_use]
    pub fn get(&self, feature: &str) -> Option<&PdfFinding> {
        self.findings.iter().find(|f| f.feature == feature)
    }

    /// Whether `feature` was reported.
    #[must_use]
    pub fn has(&self, feature: &str) -> bool {
        self.get(feature).is_some()
    }
}

/// Accumulates findings during a job.
#[derive(Debug, Default)]
pub(crate) struct Findings {
    rows: BTreeMap<&'static str, (u32, PdfModelOutcome, Option<usize>)>,
}

impl Findings {
    pub(crate) fn record(
        &mut self,
        feature: &'static str,
        outcome: PdfModelOutcome,
        count: u32,
        page: Option<usize>,
    ) {
        if count == 0 {
            return;
        }
        let row = self.rows.entry(feature).or_insert((0, outcome, page));
        row.0 = row.0.saturating_add(count);
        row.2 = match (row.2, page) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    pub(crate) fn finish(self) -> PdfImportReport {
        PdfImportReport {
            findings: self
                .rows
                .into_iter()
                .map(|(feature, (occurrences, outcome, first_page))| PdfFinding {
                    feature,
                    occurrences,
                    outcome,
                    first_page,
                })
                .collect(),
        }
    }
}

/// Every feature id this crate can report, for the support matrix and the
/// adapter's documentation. Kept sorted.
pub const FEATURES: &[&str] = &[
    ACTIVE_CONTENT_BLOCKED,
    ANNOTATION_OMITTED,
    ATTACHMENT_OMITTED,
    CLIP_IGNORED,
    ENCRYPTED_UNSUPPORTED,
    FONT_EMBEDDED,
    FONT_SUBSTITUTED,
    FORM_OMITTED,
    IMAGE_CLIP_IGNORED,
    IMAGE_REENCODED,
    IMAGE_UNDECODABLE,
    INVISIBLE_TEXT,
    NATIVE_TEXT_INVALID,
    OPTIONAL_CONTENT_FLATTENED,
    PAGE_SIZE_CLAMPED,
    PARAGRAPH_BOUNDARY_INFERRED,
    PATTERN_FLATTENED,
    READING_ORDER_INFERRED,
    TAG_STRUCTURE_IGNORED,
    TEXT_RENDER_MODE_DEGRADED,
    TEXT_ROTATED,
    TRANSPARENCY_FLATTENED,
    TYPE3_GLYPHS_SUBSTITUTED,
    VECTOR_SPLIT,
];

/// JavaScript, launch and other non-navigation actions: never run.
pub const ACTIVE_CONTENT_BLOCKED: &str = "pdf.import.active_content_blocked";
/// Annotations (comments, links, stamps) are not imported in P1.
pub const ANNOTATION_OMITTED: &str = "pdf.import.annotation_omitted";
/// Embedded files are never extracted.
pub const ATTACHMENT_OMITTED: &str = "pdf.import.attachment_omitted";
/// A clip cut into a path or text that the model paints unclipped.
pub const CLIP_IGNORED: &str = "pdf.import.clip_ignored";
/// Encrypted files are refused (the error's finding id).
pub const ENCRYPTED_UNSUPPORTED: &str = "pdf.import.encrypted_unsupported";
/// A PDF font's embedded program is registered and draws its text
/// (`Definitions::font_programs`, `docs/182` G-1). Reported because it is the
/// importer's decision, not the producer's.
pub const FONT_EMBEDDED: &str = "pdf.import.font_embedded";
/// A PDF font is drawn, wholly or for some characters, with a substitute face:
/// it is not embedded, its program is a Type1/Type3/bare-CFF one the model
/// cannot register yet, its sfnt has no cmap covering the text it drew, or
/// another program already took its family and style.
pub const FONT_SUBSTITUTED: &str = "pdf.import.font_substituted";
/// Interactive form fields are not imported as fields.
pub const FORM_OMITTED: &str = "pdf.import.form_omitted";
/// A clip on a rotated picture is not expressible as a crop.
pub const IMAGE_CLIP_IGNORED: &str = "pdf.import.image_clip_ignored";
/// JBIG2, JPX or CCITT pictures re-encoded as PNG.
pub const IMAGE_REENCODED: &str = "pdf.import.image_reencoded";
/// A picture hayro could not decode.
pub const IMAGE_UNDECODABLE: &str = "pdf.import.image_undecodable";
/// Render mode 3 (invisible) text, imported as hidden runs.
pub const INVISIBLE_TEXT: &str = "pdf.import.invisible_text";
/// Glyphs with no Unicode mapping (imported as U+FFFD; OCR is P5).
pub const NATIVE_TEXT_INVALID: &str = "pdf.import.native_text_invalid";
/// Optional content (layers) flattened to its default visibility.
pub const OPTIONAL_CONTENT_FLATTENED: &str = "pdf.import.optional_content_flattened";
/// A page larger than the model's 22-inch section bound.
pub const PAGE_SIZE_CLAMPED: &str = "pdf.import.page_size_clamped";
/// Lines grouped into paragraphs by the fixed char/line/block margins.
pub const PARAGRAPH_BOUNDARY_INFERRED: &str = "pdf.import.paragraph_boundary_inferred";
/// Pattern or shading paints flattened to a flat colour or dropped.
pub const PATTERN_FLATTENED: &str = "pdf.import.pattern_flattened";
/// Frames ordered by content-stream order.
pub const READING_ORDER_INFERRED: &str = "pdf.import.reading_order_inferred";
/// A tagged PDF's structure tree is not used by fixed-layout import.
pub const TAG_STRUCTURE_IGNORED: &str = "pdf.import.tag_structure_ignored";
/// Stroke-only text drawn filled.
pub const TEXT_RENDER_MODE_DEGRADED: &str = "pdf.import.text_render_mode_degraded";
/// Rotated text placed upright (text boxes cannot rotate yet, `docs/182` G-3).
pub const TEXT_ROTATED: &str = "pdf.import.text_rotated";
/// Opacity, soft masks and blend modes flattened to opaque normal paint.
pub const TRANSPARENCY_FLATTENED: &str = "pdf.import.transparency_flattened";
/// Type3 glyph procedures replaced by substitute text.
pub const TYPE3_GLYPHS_SUBSTITUTED: &str = "pdf.import.type3_glyphs_substituted";
/// A path over the model's per-shape command bound, split into several shapes.
pub const VECTOR_SPLIT: &str = "pdf.import.vector_split";
