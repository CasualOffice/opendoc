//! The one image type providers read: 8-bit RGB, row-major, opaque.

use crate::OcrError;

/// An opaque RGB raster, 3 bytes per pixel, rows top to bottom.
///
/// Opaque because recognition and layout models are trained on paper: a
/// renderer's transparent background must already be composited over white.
#[derive(Clone, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Raster")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Raster {
    /// Wraps `rgb` (`width * height * 3` bytes).
    ///
    /// # Errors
    ///
    /// [`OcrError::InvalidImage`] for a zero side or a length mismatch.
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Result<Self, OcrError> {
        let expected = u64::from(width) * u64::from(height) * 3;
        if width == 0 || height == 0 || rgb.len() as u64 != expected {
            return Err(OcrError::InvalidImage);
        }
        Ok(Self { width, height, rgb })
    }

    /// Composites premultiplied RGBA (`width * height * 4` bytes, the layout
    /// renderers emit) over white. O(pixels).
    ///
    /// # Errors
    ///
    /// [`OcrError::InvalidImage`] for a zero side or a length mismatch.
    pub fn from_premultiplied_rgba_over_white(
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Self, OcrError> {
        let expected = u64::from(width) * u64::from(height) * 4;
        if width == 0 || height == 0 || rgba.len() as u64 != expected {
            return Err(OcrError::InvalidImage);
        }
        let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
        for px in rgba.chunks_exact(4) {
            // Premultiplied: out = c + (1 - a) * 255.
            let inverse = 255 - u16::from(px[3]);
            for channel in &px[..3] {
                rgb.push((u16::from(*channel) + inverse).min(255) as u8);
            }
        }
        Ok(Self { width, height, rgb })
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixels (`width * height`).
    #[must_use]
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// The RGB bytes.
    #[must_use]
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }
}
