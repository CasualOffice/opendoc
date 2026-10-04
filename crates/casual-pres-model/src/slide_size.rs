// SPDX-License-Identifier: Apache-2.0

//! The slide surface (`p:sldSz`).

use serde::{Deserialize, Serialize};

use crate::{PresentationError, SlideAxis};

/// The smallest slide dimension `ST_SlideSizeCoordinate` admits: one inch.
pub const MIN_SLIDE_EMU: i64 = 914_400;

/// The largest slide dimension `ST_SlideSizeCoordinate` admits: fifty-six inches.
pub const MAX_SLIDE_EMU: i64 = 51_206_400;

/// The named slide size a package declares (`p:sldSz@type`).
///
/// A hint, not the geometry: the dimensions are always carried explicitly by
/// [`SlideSize::width_emu`]/[`SlideSize::height_emu`], and PowerPoint honours those
/// over the token. It is retained because export must write back what the file said
/// — rewriting a `letter` deck as `custom` with identical dimensions is a diff in
/// every package we touch, for no gain.
///
/// These are `ST_SlideSizeType`'s sixteen values. ONLYOFFICE additionally emits a
/// `wideScreen` token, which is not one of them; a token this build does not know
/// maps to [`SlideSizeKind::Custom`] with the authored dimensions preserved, which
/// is the lossless reading.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SlideSizeKind {
    /// 4:3 on-screen show (`screen4x3`).
    Screen4x3,
    /// 16:9 on-screen show (`screen16x9`).
    Screen16x9,
    /// 16:10 on-screen show (`screen16x10`).
    Screen16x10,
    /// US Letter paper (`letter`).
    Letter,
    /// US Ledger paper (`ledger`).
    Ledger,
    /// ISO A3 paper (`A3`).
    A3,
    /// ISO A4 paper (`A4`).
    A4,
    /// ISO B4 paper (`B4ISO`).
    B4Iso,
    /// ISO B5 paper (`B5ISO`).
    B5Iso,
    /// JIS B4 paper (`B4JIS`).
    B4Jis,
    /// JIS B5 paper (`B5JIS`).
    B5Jis,
    /// 35mm slide film (`35mm`).
    Film35mm,
    /// Overhead transparency (`overhead`).
    Overhead,
    /// Banner (`banner`).
    Banner,
    /// Hagaki postcard (`hagakiCard`).
    HagakiCard,
    /// Explicit dimensions with no named size (`custom`). The default, so an
    /// unknown token degrades to "the dimensions are authoritative".
    #[default]
    Custom,
}

impl SlideSizeKind {
    /// The `p:sldSz@type` token, for export and for diagnostics.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Screen4x3 => "screen4x3",
            Self::Screen16x9 => "screen16x9",
            Self::Screen16x10 => "screen16x10",
            Self::Letter => "letter",
            Self::Ledger => "ledger",
            Self::A3 => "A3",
            Self::A4 => "A4",
            Self::B4Iso => "B4ISO",
            Self::B5Iso => "B5ISO",
            Self::B4Jis => "B4JIS",
            Self::B5Jis => "B5JIS",
            Self::Film35mm => "35mm",
            Self::Overhead => "overhead",
            Self::Banner => "banner",
            Self::HagakiCard => "hagakiCard",
            Self::Custom => "custom",
        }
    }

    /// Reads a `p:sldSz@type` token, mapping anything unrecognized to
    /// [`SlideSizeKind::Custom`] rather than failing: the dimensions carry the real
    /// geometry, so an unknown name costs nothing and refusing the package would
    /// cost the whole deck.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "screen4x3" => Self::Screen4x3,
            "screen16x9" => Self::Screen16x9,
            "screen16x10" => Self::Screen16x10,
            "letter" => Self::Letter,
            "ledger" => Self::Ledger,
            "A3" => Self::A3,
            "A4" => Self::A4,
            "B4ISO" => Self::B4Iso,
            "B5ISO" => Self::B5Iso,
            "B4JIS" => Self::B4Jis,
            "B5JIS" => Self::B5Jis,
            "35mm" => Self::Film35mm,
            "overhead" => Self::Overhead,
            "banner" => Self::Banner,
            "hagakiCard" => Self::HagakiCard,
            _ => Self::Custom,
        }
    }

    /// Every token this build knows, for guards that must derive the count rather
    /// than hand-maintain it.
    pub const ALL: [Self; 16] = [
        Self::Screen4x3,
        Self::Screen16x9,
        Self::Screen16x10,
        Self::Letter,
        Self::Ledger,
        Self::A3,
        Self::A4,
        Self::B4Iso,
        Self::B5Iso,
        Self::B4Jis,
        Self::B5Jis,
        Self::Film35mm,
        Self::Overhead,
        Self::Banner,
        Self::HagakiCard,
        Self::Custom,
    ];
}

/// The slide surface every slide in the presentation is laid out on (`p:sldSz`).
///
/// Presentation-wide, not per slide: PowerPoint has no per-slide size, which is the
/// single largest structural difference from a document's per-section page size. It
/// is why a deck needs no pagination — the surface is fixed and the content is
/// positioned on it — and why the renderer scales rather than reflows.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideSize {
    /// Surface width (`p:sldSz@cx`, EMU), within
    /// [`MIN_SLIDE_EMU`]..=[`MAX_SLIDE_EMU`].
    pub width_emu: i64,
    /// Surface height (`p:sldSz@cy`, EMU), within
    /// [`MIN_SLIDE_EMU`]..=[`MAX_SLIDE_EMU`].
    pub height_emu: i64,
    /// The declared named size, retained for export.
    #[serde(default)]
    pub kind: SlideSizeKind,
}

impl SlideSize {
    /// The 16:9 default a new deck is created at: 13.333in x 7.5in, which is what
    /// PowerPoint 2013 and later author and what `screen16x9` means in practice.
    pub const DEFAULT_16X9: Self = Self {
        width_emu: 12_192_000,
        height_emu: 6_858_000,
        kind: SlideSizeKind::Screen16x9,
    };

    /// The 4:3 surface older decks use: 10in x 7.5in.
    pub const DEFAULT_4X3: Self = Self {
        width_emu: 9_144_000,
        height_emu: 6_858_000,
        kind: SlideSizeKind::Screen4x3,
    };

    /// Checks both dimensions against `ST_SlideSizeCoordinate`.
    ///
    /// Complexity: O(1).
    pub const fn validate(&self) -> Result<(), PresentationError> {
        if self.width_emu < MIN_SLIDE_EMU || self.width_emu > MAX_SLIDE_EMU {
            return Err(PresentationError::SlideSizeOutOfDomain {
                axis: SlideAxis::Width,
                value: self.width_emu,
            });
        }
        if self.height_emu < MIN_SLIDE_EMU || self.height_emu > MAX_SLIDE_EMU {
            return Err(PresentationError::SlideSizeOutOfDomain {
                axis: SlideAxis::Height,
                value: self.height_emu,
            });
        }
        Ok(())
    }
}

impl Default for SlideSize {
    fn default() -> Self {
        Self::DEFAULT_16X9
    }
}
