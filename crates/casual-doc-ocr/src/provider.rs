//! The recognition provider seam and its bounds.

use std::fmt;

use crate::{Interrupt, OcrPage, ProviderManifest, Raster};

/// Bounds on one provider call (PDFR-I7). Exceeding `max_pixels` refuses the
/// raster before any model runs; the line and character bounds stop
/// recognition and count the rest in [`OcrPage::lines_skipped`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OcrLimits {
    /// Largest raster accepted, in pixels.
    pub max_pixels: u64,
    /// Most lines recognised per raster.
    pub max_lines: u32,
    /// Most characters kept per line (longer lines are skipped, not cut).
    pub max_chars_per_line: u32,
}

impl Default for OcrLimits {
    /// 16 Mpx (a Letter page at 300 dpi is 8.4 Mpx), 2,000 lines, 1,000
    /// characters per line.
    fn default() -> Self {
        Self {
            max_pixels: 16_000_000,
            max_lines: 2_000,
            max_chars_per_line: 1_000,
        }
    }
}

/// Why a provider produced nothing. Never carries recognised text.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OcrError {
    /// The raster's dimensions or length are invalid.
    InvalidImage,
    /// A bound was exceeded before work started.
    Limit {
        /// The [`OcrLimits`] field.
        limit: &'static str,
        /// What the input asked for.
        observed: u64,
        /// What the limit allows.
        allowed: u64,
    },
    /// The host cancelled.
    Cancelled,
    /// A model failed to load or run, or has the wrong shape. The string is
    /// the engine's message, which never holds document content.
    Model(String),
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImage => f.write_str("the raster is invalid"),
            Self::Limit {
                limit,
                observed,
                allowed,
            } => write!(f, "OCR limit {limit} exceeded: {observed} > {allowed}"),
            Self::Cancelled => f.write_str("OCR was cancelled"),
            Self::Model(message) => write!(f, "OCR model error: {message}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// A text recogniser. Implementations are deterministic for their declared
/// [`crate::Determinism`], never touch the network, and poll `interrupt`
/// between model runs.
pub trait OcrProvider: Send + Sync {
    /// What this provider is and which model bytes it holds.
    fn manifest(&self) -> ProviderManifest;

    /// Recognises the text in `raster`.
    ///
    /// # Errors
    ///
    /// [`OcrError::Limit`] when `raster` exceeds `limits.max_pixels`,
    /// [`OcrError::Cancelled`] when `interrupt` reports a cancel, and
    /// [`OcrError::Model`] for an engine failure. Running out of time is not
    /// an error: the partial page is returned with `out_of_time` set.
    fn recognize(
        &self,
        raster: &Raster,
        limits: &OcrLimits,
        interrupt: &dyn Interrupt,
    ) -> Result<OcrPage, OcrError>;
}

/// Checks the pixel bound shared by every provider.
pub(crate) fn check_pixels(raster: &Raster, limits: &OcrLimits) -> Result<(), OcrError> {
    if raster.pixels() > limits.max_pixels {
        return Err(OcrError::Limit {
            limit: "max_pixels",
            observed: raster.pixels(),
            allowed: limits.max_pixels,
        });
    }
    Ok(())
}
