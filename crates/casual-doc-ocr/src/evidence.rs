//! Owned optical evidence (PDFR-I1: evidence only, never document state).
//!
//! Coordinates are **pixels of the raster the provider read**, origin top
//! left, y down. The caller maps them back to its own space (the PDF importer
//! divides by its render scale and adds the region's offset).

/// An axis-aligned box in raster pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OcrBox {
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

impl OcrBox {
    /// Width.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    /// Height.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// The smallest box holding both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// One recognised word: non-space characters between spaces.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    /// The text, never empty and never containing a space.
    pub text: String,
    /// Where it is.
    pub bounds: OcrBox,
    /// The geometric mean of its characters' probabilities, in `0.0..=1.0`.
    pub confidence: f32,
}

/// One recognised line, words in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrLine {
    /// The words, never empty.
    pub words: Vec<OcrWord>,
    /// The line's box: horizontally what was read, vertically the ink's
    /// extent inside the detected box ([`crate::Raster::ink_rows`]), so its
    /// height is ascender to descender whatever the detector's padding.
    ///
    /// For a turned line (`rotation_degrees` not zero) this box and its
    /// words' boxes are in the line's **upright frame**: the raster turned
    /// by `-rotation_degrees` about `pivot`, where the line reads left to
    /// right. [`OcrLine::to_raster`] maps a point of that frame back.
    pub bounds: OcrBox,
    /// Clockwise turn of the line's baseline on the raster, in degrees, in
    /// the raster's y-down frame: 0 for an axis-aligned line, the scan's skew
    /// for a skewed one, plus 180 when the orientation classifier found the
    /// line upside down.
    pub rotation_degrees: f32,
    /// The raster point the upright frame is turned about (the detected
    /// box's centre). Ignored when `rotation_degrees` is zero.
    pub pivot: [f32; 2],
}

impl OcrLine {
    /// An axis-aligned line: no turn.
    #[must_use]
    pub fn upright(words: Vec<OcrWord>, bounds: OcrBox) -> Self {
        Self {
            words,
            bounds,
            rotation_degrees: 0.0,
            pivot: [0.0, 0.0],
        }
    }

    /// Maps a point of the line's upright frame to the raster. O(1).
    #[must_use]
    pub fn to_raster(&self, x: f32, y: f32) -> (f32, f32) {
        if self.rotation_degrees == 0.0 {
            return (x, y);
        }
        let (sin, cos) = self.rotation_degrees.to_radians().sin_cos();
        let (dx, dy) = (x - self.pivot[0], y - self.pivot[1]);
        (
            self.pivot[0] + dx * cos - dy * sin,
            self.pivot[1] + dx * sin + dy * cos,
        )
    }

    /// The words joined by single spaces.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, word) in self.words.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&word.text);
        }
        out
    }

    /// The lowest word confidence.
    #[must_use]
    pub fn min_confidence(&self) -> f32 {
        self.words
            .iter()
            .map(|w| w.confidence)
            .fold(1.0_f32, f32::min)
    }
}

/// What a provider returned for one raster.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcrPage {
    /// Lines in the provider's reading order.
    pub lines: Vec<OcrLine>,
    /// Lines detected but not recognised because a line or character limit,
    /// or the time budget, stopped the provider. Zero for a complete result.
    pub lines_skipped: u32,
    /// Whether the time budget stopped recognition (the result is partial).
    pub out_of_time: bool,
    /// The skew the provider estimated for the raster, clockwise degrees
    /// (the median turn of its elongated lines); zero when it estimated none
    /// or the raster is straight. Each line's `rotation_degrees` already
    /// carries it.
    pub skew_degrees: f32,
}

/// How far identical output is promised (PDFR-I8: declared, not assumed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Determinism {
    /// Identical raster, model bytes, limits and build target give identical
    /// evidence. Different targets (x86-64, aarch64, wasm32 SIMD) may round
    /// floating point differently and are not claimed equal until a fixture
    /// proves it on each.
    SameTarget,
    /// A pure function of its input on every target (integer heuristics).
    Exact,
}

/// What produced the evidence: recorded so an import can say which model
/// read its pages and so two runs can be compared (PDFR-I8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderManifest {
    /// Provider id, e.g. `ocrs`.
    pub provider: &'static str,
    /// The provider crate version, e.g. `0.13.1`.
    pub version: &'static str,
    /// SHA-256 (lower-case hex) of each model the host supplied, in a fixed
    /// order with a role label (`detection`, `recognition`, `layout`).
    pub models: Vec<(&'static str, String)>,
    /// Decoding, e.g. `ctc-greedy`; fixed, never sampled.
    pub decoding: &'static str,
    /// The determinism this provider declares.
    pub determinism: Determinism,
}
