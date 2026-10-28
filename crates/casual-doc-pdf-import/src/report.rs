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
    ARTIFACT_OMITTED,
    ATTACHMENT_OMITTED,
    BOOKMARK_INFERRED,
    CLIP_IGNORED,
    COLUMNS_INFERRED,
    COMMENT_INFERRED,
    DEHYPHENATION_INFERRED,
    ENCRYPTED_UNSUPPORTED,
    FONT_EMBEDDED,
    FONT_SUBSTITUTED,
    FOOTNOTE_INFERRED,
    FOOTNOTE_MARKER_RENUMBERED,
    FORM_OMITTED,
    HEADER_FOOTER_INFERRED,
    HEADING_INFERRED,
    HYPERLINK_INFERRED,
    IMAGE_CLIP_IGNORED,
    IMAGE_REENCODED,
    IMAGE_UNDECODABLE,
    INVISIBLE_TEXT,
    JUSTIFICATION_INFERRED,
    LINK_UNRESOLVED,
    LIST_INFERRED,
    NATIVE_OCR_CONFLICT,
    NATIVE_TEXT_INVALID,
    OCR_APPLIED,
    OCR_LIMIT_REACHED,
    OCR_LOW_CONFIDENCE,
    OCR_UNAVAILABLE,
    OPTIONAL_CONTENT_FLATTENED,
    PAGE_BREAK_INFERRED,
    PAGE_SIZE_CLAMPED,
    PARAGRAPH_BOUNDARY_INFERRED,
    PATTERN_FLATTENED,
    READING_ORDER_INFERRED,
    STYLE_INFERRED,
    TABLE_INFERRED,
    TABLE_TOPOLOGY_DEGRADED,
    TAG_STRUCTURE_IGNORED,
    TEXT_RENDER_MODE_DEGRADED,
    TEXT_ROTATED,
    TRANSPARENCY_FLATTENED,
    TYPE3_GLYPHS_SUBSTITUTED,
    VECTOR_OMITTED,
    VECTOR_SPLIT,
];

/// JavaScript, launch and other non-navigation actions: never run.
pub const ACTIVE_CONTENT_BLOCKED: &str = "pdf.import.active_content_blocked";
/// Annotations that are neither links nor notes (highlights, stamps, ink,
/// shapes), and notes with no text or nothing to anchor on, are not imported.
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
/// An optical word read where valid native text already is, and the two
/// disagree. The native text is kept (PDFR-I2); the count says how often the
/// page's pixels and its text layer differ.
pub const NATIVE_OCR_CONFLICT: &str = "pdf.import.native_ocr_conflict";
/// Glyphs with no Unicode mapping: imported as U+FFFD, or, when OCR read the
/// place they are drawn, replaced by the optical text (`ocr_applied`).
pub const NATIVE_TEXT_INVALID: &str = "pdf.import.native_text_invalid";
/// Regions (pictures, or runs with no Unicode) read by OCR. The text is
/// inferred from pixels, so it is reported even when every word is confident.
pub const OCR_APPLIED: &str = "pdf.import.ocr_applied";
/// Regions or lines that needed OCR and were not read because a page, pixel,
/// line or time budget ran out, or the provider failed on them.
pub const OCR_LIMIT_REACHED: &str = "pdf.import.ocr_limit_reached";
/// Optical words under the low-confidence threshold (kept), or under the
/// floor (dropped).
pub const OCR_LOW_CONFIDENCE: &str = "pdf.import.ocr_low_confidence";
/// Regions that need OCR when the host supplied no recognition model: their
/// text is not in the document (pictures stay pictures; unmapped runs stay
/// U+FFFD).
pub const OCR_UNAVAILABLE: &str = "pdf.import.ocr_unavailable";
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
/// A tagged PDF's structure tree is not used: by fixed-layout import, which
/// has no reading order to give, or by reflow when the tree is truncated or
/// covers too little of a page to order it.
pub const TAG_STRUCTURE_IGNORED: &str = "pdf.import.tag_structure_ignored";
/// Stroke-only text drawn filled.
pub const TEXT_RENDER_MODE_DEGRADED: &str = "pdf.import.text_render_mode_degraded";
/// Rotated text imported in a frame turned by its angle (`docs/182` G-3).
/// Mapped, not a loss: reported because the frame is the importer's choice.
pub const TEXT_ROTATED: &str = "pdf.import.text_rotated";
/// Opacity, soft masks and blend modes flattened to opaque normal paint.
pub const TRANSPARENCY_FLATTENED: &str = "pdf.import.transparency_flattened";
/// Type3 glyph procedures replaced by substitute text.
pub const TYPE3_GLYPHS_SUBSTITUTED: &str = "pdf.import.type3_glyphs_substituted";
/// A path over the model's per-shape command bound, split into several shapes.
pub const VECTOR_SPLIT: &str = "pdf.import.vector_split";
/// Reflow: `/Artifact` content outside the header and footer bands (backgrounds,
/// watermarks) is not page content in a flowing document and is left out.
pub const ARTIFACT_OMITTED: &str = "pdf.import.artifact_omitted";
/// A link's in-document destination placed as a bookmark at the paragraph
/// (reflow) or frame (fixed layout) nearest the destination.
pub const BOOKMARK_INFERRED: &str = "pdf.import.bookmark_inferred";
/// Reflow: a column region became a section with `SectionColumns`.
pub const COLUMNS_INFERRED: &str = "pdf.import.columns_inferred";
/// A Text or FreeText annotation became a comment, anchored on the text
/// nearest its rectangle (the anchor is inferred; author, date and text are
/// the annotation's).
pub const COMMENT_INFERRED: &str = "pdf.import.comment_inferred";
/// Reflow: a line-end hyphen between letters was removed when joining the
/// lines (the next line starts lowercase).
pub const DEHYPHENATION_INFERRED: &str = "pdf.import.dehyphenation_inferred";
/// Reflow: a superscript marker matched to a bottom-of-page block became a
/// footnote (symbol markers are renumbered by the model).
pub const FOOTNOTE_INFERRED: &str = "pdf.import.footnote_inferred";
/// Reflow: repeated top or bottom content, or `/Artifact` content in those
/// bands, became a header or footer part.
pub const HEADER_FOOTER_INFERRED: &str = "pdf.import.header_footer_inferred";
/// Reflow: a paragraph was given a heading style from its font cluster.
pub const HEADING_INFERRED: &str = "pdf.import.heading_inferred";
/// A link annotation became a hyperlink over the characters its rectangle
/// covers.
pub const HYPERLINK_INFERRED: &str = "pdf.import.hyperlink_inferred";
/// A link whose target is not navigable (a refused scheme, a destination
/// naming nothing) or whose rectangle covers no text was not imported.
pub const LINK_UNRESOLVED: &str = "pdf.import.link_unresolved";
/// Reflow: a bullet or enumerator with a hanging indent became a numbered
/// or bulleted list item.
pub const LIST_INFERRED: &str = "pdf.import.list_inferred";
/// Reflow: font clusters became the style sheet (Normal and Heading n).
pub const STYLE_INFERRED: &str = "pdf.import.style_inferred";
/// Reflow: a ruling-line grid, or whitespace-aligned columns above the
/// confidence threshold, became a table.
pub const TABLE_INFERRED: &str = "pdf.import.table_inferred";
/// Reflow: whitespace-aligned columns scored below the table threshold and
/// were kept as paragraphs.
pub const TABLE_TOPOLOGY_DEGRADED: &str = "pdf.import.table_topology_degraded";
/// Reflow: vector art that is not a table rule is not placed in a flowing
/// document.
pub const VECTOR_OMITTED: &str = "pdf.import.vector_omitted";
/// Reflow: footnote markers that are neither a number sequence nor the
/// `*` `†` `‡` `§` sequence (continuous or restarting each page) are
/// renumbered by the model's footnote numbering.
pub const FOOTNOTE_MARKER_RENUMBERED: &str = "pdf.import.footnote_marker_renumbered";
/// Reflow: a paragraph whose lines all reach the column's right edge but the
/// last is justified.
pub const JUSTIFICATION_INFERRED: &str = "pdf.import.justification_inferred";
/// Reflow: a page that ended well above the body's bottom ended in a page
/// break, kept before the next page's first paragraph.
pub const PAGE_BREAK_INFERRED: &str = "pdf.import.page_break_inferred";
