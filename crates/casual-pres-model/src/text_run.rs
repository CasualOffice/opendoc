// SPDX-License-Identifier: Apache-2.0

//! DrawingML run-level text properties (`a:rPr`, `a:defRPr`, `a:endParaRPr`).
//!
//! # Why this is not `v1::RunProperties`
//!
//! WordprocessingML and DrawingML describe the same typography in **different units
//! and different elements**, and the most dangerous of those differences is silent:
//! `w:sz` is in HALF-points, `a:rPr@sz` is in HUNDREDTHS of a point. A value carried
//! from one into the other without conversion is wrong by a factor of fifty and still
//! looks like a plausible font size, so it would not fail a bounds check — it would
//! just render a 12pt deck at 600pt or at 0.24pt.
//!
//! Every field here therefore names its unit, the way the document model's
//! `size_half_points` does. Reusing `RunProperties` and remembering to convert at
//! each call site is the version of this that breaks.

use casual_doc_model::v1::{Rgba, StyleColor};
use serde::{Deserialize, Serialize};

/// The smallest `a:rPr@sz` `ST_TextFontSize` admits: one point.
pub const MIN_FONT_SIZE_HUNDREDTHS: u32 = 100;

/// The largest `a:rPr@sz` `ST_TextFontSize` admits: 4000 points.
pub const MAX_FONT_SIZE_HUNDREDTHS: u32 = 400_000;

/// The `ST_TextPoint` bound applied to `a:rPr@spc` (letter spacing), in hundredths
/// of a point. Negative tightens.
pub const MAX_SPACING_HUNDREDTHS: i32 = 400_000;

/// How a run is underlined (`a:rPr@u`, `ST_TextUnderlineType`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextUnderline {
    /// Not underlined (`none`), and the default.
    #[default]
    None,
    /// Inherited from the tier above (`words` excluded) — `a:rPr` simply absent is
    /// represented by `Option<TextCharacterProperties>` rather than by this.
    Single,
    /// Underlines words but not the spaces between them (`words`).
    Words,
    /// A double line (`dbl`).
    Double,
    /// A heavy line (`heavy`).
    Heavy,
    /// A dotted line (`dotted`).
    Dotted,
    /// A dashed line (`dash`).
    Dash,
    /// A wavy line (`wavy`).
    Wavy,
}

impl TextUnderline {
    /// The `a:rPr@u` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Single => "sng",
            Self::Words => "words",
            Self::Double => "dbl",
            Self::Heavy => "heavy",
            Self::Dotted => "dotted",
            Self::Dash => "dash",
            Self::Wavy => "wavy",
        }
    }

    /// Reads an `a:rPr@u` token; an unrecognized one is [`TextUnderline::None`],
    /// which is the schema default and the lossless reading for a decoration this
    /// build cannot draw.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.token() == token)
            .unwrap_or(Self::None)
    }

    /// Every variant, so a guard derives the count.
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Single,
        Self::Words,
        Self::Double,
        Self::Heavy,
        Self::Dotted,
        Self::Dash,
        Self::Wavy,
    ];
}

/// How a run is struck through (`a:rPr@strike`, `ST_TextStrikeType`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextStrike {
    /// Not struck (`noStrike`), the default.
    #[default]
    None,
    /// A single line (`sngStrike`).
    Single,
    /// A double line (`dblStrike`).
    Double,
}

impl TextStrike {
    /// The `a:rPr@strike` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::None => "noStrike",
            Self::Single => "sngStrike",
            Self::Double => "dblStrike",
        }
    }

    /// Reads an `a:rPr@strike` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "sngStrike" => Self::Single,
            "dblStrike" => Self::Double,
            _ => Self::None,
        }
    }
}

/// Capitalization applied at render time (`a:rPr@cap`, `ST_TextCapsType`).
///
/// A **presentation** transform, not a text change: the stored characters keep their
/// authored case, which is why this is a property and not a rewrite. Losing that
/// distinction would make the transform irreversible.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextCaps {
    /// No transform (`none`), the default.
    #[default]
    None,
    /// Every character upper-cased (`all`).
    All,
    /// Lowercase letters drawn as smaller capitals (`small`).
    Small,
}

impl TextCaps {
    /// The `a:rPr@cap` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::All => "all",
            Self::Small => "small",
        }
    }

    /// Reads an `a:rPr@cap` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "all" => Self::All,
            "small" => Self::Small,
            _ => Self::None,
        }
    }
}

/// A typeface reference (`a:latin`, `a:ea`, `a:cs`).
///
/// The `+mj-lt` / `+mn-lt` forms are retained verbatim rather than resolved at
/// import: they name the theme's major/minor font, and resolving them here would make
/// the model say something the file did not. Resolution belongs at layout, the same
/// place the shape style matrix is resolved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Typeface {
    /// The `@typeface` attribute, which may be a theme reference such as `+mn-lt`.
    pub name: String,
    /// `@panose`, when the file carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panose: Option<String>,
}

impl Typeface {
    /// Whether the name is a theme reference (`+mj-*` / `+mn-*`) rather than a
    /// concrete family, which decides whether layout must resolve it.
    #[must_use]
    pub fn is_theme_reference(&self) -> bool {
        self.name.starts_with("+mj-") || self.name.starts_with("+mn-")
    }
}

/// Run-level character properties (`a:rPr` / `a:defRPr` / `a:endParaRPr`).
///
/// Every field is optional and omitted when absent, because **absence is meaningful
/// here in a way it is not in a document**: an unset property inherits through the
/// placeholder cascade (slide, then layout, then master, then `p:defaultTextStyle`),
/// so collapsing "unset" into "the default value" would freeze inherited text at the
/// wrong tier and is the single easiest way to make a whole deck render in the wrong
/// font.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextCharacterProperties {
    /// `a:rPr@sz`, in **hundredths of a point** — see the module note; the document
    /// model's equivalent is in half-points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_hundredths_point: Option<u32>,
    /// `a:rPr@b`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// `a:rPr@i`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// `a:rPr@u`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<TextUnderline>,
    /// `a:rPr@strike`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<TextStrike>,
    /// `a:rPr@cap`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<TextCaps>,
    /// `a:rPr@spc` (letter spacing), in hundredths of a point; negative tightens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_hundredths_point: Option<i32>,
    /// `a:rPr@baseline` (super/subscript), in thousandths of a percent of the font
    /// size. Positive raises. Not a boolean pair: DrawingML expresses the offset as
    /// a continuous value, and Word's super/sub are two points on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_percent: Option<i32>,
    /// `a:rPr@lang`, retained because it drives hyphenation and proofing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `a:latin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latin: Option<Typeface>,
    /// `a:ea` (East Asian).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub east_asian: Option<Typeface>,
    /// `a:cs` (complex script).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complex_script: Option<Typeface>,
    /// `a:rPr/a:solidFill` — the glyph colour, which may name `a:phClr` or a scheme
    /// colour with a transform, so it is a [`StyleColor`] rather than an [`Rgba`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<StyleColor>,
    /// `a:rPr@dirty`, retained so a round trip does not claim the text was proofed.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub dirty: bool,
}

impl TextCharacterProperties {
    /// Whether every field is unset, in which case the element is omitted on write
    /// rather than emitted empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The resolved glyph colour, substituting `substitute` for an `a:phClr`.
    ///
    /// # Complexity
    ///
    /// O(1).
    #[must_use]
    pub fn resolved_color(&self, substitute: Option<Rgba>) -> Option<Rgba> {
        self.fill.as_ref().and_then(|fill| fill.resolve(substitute))
    }

    /// Whether the declared font size is inside `ST_TextFontSize`.
    ///
    /// `true` when no size is declared: an absent size inherits, and refusing it
    /// would refuse the common case.
    #[must_use]
    pub fn size_in_domain(&self) -> bool {
        self.size_hundredths_point.is_none_or(|size| {
            (MIN_FONT_SIZE_HUNDREDTHS..=MAX_FONT_SIZE_HUNDREDTHS).contains(&size)
        })
    }
}
