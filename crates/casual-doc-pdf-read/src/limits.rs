//! Admission limits (`docs/21` "PDF Import Defaults").

use crate::PdfReadError;

/// Every bound the reader enforces. Defaults are the `docs/21` secure
/// defaults; [`PdfReadLimits::HARD_CEILING`] is the most a host may raise them
/// to, checked by [`PdfReadLimits::validate`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PdfReadLimits {
    /// Input file bytes.
    pub max_input_bytes: u64,
    /// Indirect objects in the cross-reference table.
    pub max_objects: u64,
    /// Pages.
    pub max_pages: u64,
    /// Page width or height, in points.
    pub max_page_side_points: u64,
    /// Decoded bytes of any one Flate stream.
    pub max_stream_decoded_bytes: u64,
    /// Decoded bytes of all Flate streams together.
    pub max_document_decoded_bytes: u64,
    /// Operators in one page's content stream.
    pub max_operators_per_page: u64,
    /// Glyphs + paths + images on one page.
    pub max_evidence_per_page: u64,
    /// Declared pixels of one image.
    pub max_image_pixels: u64,
    /// Distinct fonts in the document.
    pub max_fonts: u64,
    /// Bytes of one embedded font program.
    pub max_font_program_bytes: u64,
}

const MIB: u64 = 1024 * 1024;

impl PdfReadLimits {
    /// The secure defaults.
    pub const DEFAULT: Self = Self {
        max_input_bytes: 256 * MIB,
        max_objects: 500_000,
        max_pages: 2_000,
        max_page_side_points: 14_400,
        max_stream_decoded_bytes: 32 * MIB,
        max_document_decoded_bytes: 256 * MIB,
        max_operators_per_page: 1_000_000,
        max_evidence_per_page: 100_000,
        max_image_pixels: 100_000_000,
        max_fonts: 4_096,
        max_font_program_bytes: 32 * MIB,
    };

    /// The hard ceilings no host may exceed.
    pub const HARD_CEILING: Self = Self {
        max_input_bytes: 1024 * MIB,
        max_objects: 4_000_000,
        max_pages: 10_000,
        max_page_side_points: 72_000,
        max_stream_decoded_bytes: 256 * MIB,
        max_document_decoded_bytes: 1024 * MIB,
        max_operators_per_page: 4_000_000,
        max_evidence_per_page: 1_000_000,
        max_image_pixels: 400_000_000,
        max_fonts: 16_384,
        max_font_program_bytes: 128 * MIB,
    };

    /// Checks every field is within [`Self::HARD_CEILING`].
    ///
    /// # Errors
    ///
    /// [`PdfReadError::Limit`] naming the first field above its ceiling.
    pub fn validate(&self) -> Result<(), PdfReadError> {
        let ceiling = Self::HARD_CEILING;
        for (name, value, allowed) in [
            (
                "max_input_bytes",
                self.max_input_bytes,
                ceiling.max_input_bytes,
            ),
            ("max_objects", self.max_objects, ceiling.max_objects),
            ("max_pages", self.max_pages, ceiling.max_pages),
            (
                "max_page_side_points",
                self.max_page_side_points,
                ceiling.max_page_side_points,
            ),
            (
                "max_stream_decoded_bytes",
                self.max_stream_decoded_bytes,
                ceiling.max_stream_decoded_bytes,
            ),
            (
                "max_document_decoded_bytes",
                self.max_document_decoded_bytes,
                ceiling.max_document_decoded_bytes,
            ),
            (
                "max_operators_per_page",
                self.max_operators_per_page,
                ceiling.max_operators_per_page,
            ),
            (
                "max_evidence_per_page",
                self.max_evidence_per_page,
                ceiling.max_evidence_per_page,
            ),
            (
                "max_image_pixels",
                self.max_image_pixels,
                ceiling.max_image_pixels,
            ),
            ("max_fonts", self.max_fonts, ceiling.max_fonts),
            (
                "max_font_program_bytes",
                self.max_font_program_bytes,
                ceiling.max_font_program_bytes,
            ),
        ] {
            check(name, value, allowed)?;
        }
        Ok(())
    }
}

impl Default for PdfReadLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Returns [`PdfReadError::Limit`] when `observed > allowed`.
pub(crate) fn check(limit: &'static str, observed: u64, allowed: u64) -> Result<(), PdfReadError> {
    if observed > allowed {
        Err(PdfReadError::Limit {
            limit,
            observed,
            allowed,
        })
    } else {
        Ok(())
    }
}
