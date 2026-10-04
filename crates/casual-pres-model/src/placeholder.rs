// SPDX-License-Identifier: Apache-2.0

//! Placeholder identity (`p:ph`): the slot a shape occupies in its layout.
//!
//! This is the one construct with no document-side counterpart, and the reason a
//! presentation needs a model of its own rather than a second `Document` profile. A
//! DOCX shape states its own position, size, fill and text properties. A slide shape
//! that carries a `p:ph` states **only what it overrides** and inherits the rest from
//! the matching slot in its layout, which inherits in turn from the master. Dropping
//! the slot would not lose a decoration; it would lose the position and the text
//! formatting of nearly every shape in a real deck, because that is where PowerPoint
//! puts them.

use serde::{Deserialize, Serialize};

/// The kind of slot a placeholder fills (`p:ph@type`, `ST_PlaceholderType`).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub enum PlaceholderKind {
    /// The slide title (`title`).
    Title,
    /// A centered title, used by the title-slide layouts (`ctrTitle`). A title for
    /// every purpose here — see [`PlaceholderKind::is_title`].
    CtrTitle,
    /// A subtitle, which pairs with `ctrTitle` (`subTitle`).
    SubTitle,
    /// Body text (`body`).
    Body,
    /// A date (`dt`).
    #[serde(rename = "dt")]
    Date,
    /// A footer (`ftr`).
    #[serde(rename = "ftr")]
    Footer,
    /// A header (`hdr`), which appears on notes and handout masters rather than on
    /// slides.
    #[serde(rename = "hdr")]
    Header,
    /// The slide number (`sldNum`).
    SlideNumber,
    /// The slide image on a notes page (`sldImg`).
    SlideImage,
    /// A chart (`chart`).
    Chart,
    /// A table (`tbl`).
    Table,
    /// Clip art (`clipArt`).
    ClipArt,
    /// A diagram (`dgm`).
    #[serde(rename = "dgm")]
    Diagram,
    /// Media (`media`).
    Media,
    /// A picture (`pic`).
    Picture,
    /// A generic object (`obj`). The schema default, so an attribute-less `p:ph`
    /// lands here.
    #[default]
    Object,
}

impl PlaceholderKind {
    /// The `p:ph@type` token, for export and for diagnostics.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::CtrTitle => "ctrTitle",
            Self::SubTitle => "subTitle",
            Self::Body => "body",
            Self::Date => "dt",
            Self::Footer => "ftr",
            Self::Header => "hdr",
            Self::SlideNumber => "sldNum",
            Self::SlideImage => "sldImg",
            Self::Chart => "chart",
            Self::Table => "tbl",
            Self::ClipArt => "clipArt",
            Self::Diagram => "dgm",
            Self::Media => "media",
            Self::Picture => "pic",
            Self::Object => "obj",
        }
    }

    /// Reads a `p:ph@type` token. An absent or unrecognized attribute is
    /// [`PlaceholderKind::Object`], which is the schema's own default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "title" => Self::Title,
            "ctrTitle" => Self::CtrTitle,
            "subTitle" => Self::SubTitle,
            "body" => Self::Body,
            "dt" => Self::Date,
            "ftr" => Self::Footer,
            "hdr" => Self::Header,
            "sldNum" => Self::SlideNumber,
            "sldImg" => Self::SlideImage,
            "chart" => Self::Chart,
            "tbl" => Self::Table,
            "clipArt" => Self::ClipArt,
            "dgm" => Self::Diagram,
            "media" => Self::Media,
            "pic" => Self::Picture,
            _ => Self::Object,
        }
    }

    /// Whether this slot is a title. Both `title` and `ctrTitle` are: a layout uses
    /// one or the other, never both, and PowerPoint's object model exposes a single
    /// `Shapes.Title` that resolves either.
    #[must_use]
    pub const fn is_title(self) -> bool {
        matches!(self, Self::Title | Self::CtrTitle)
    }

    /// Every token this build knows, so a guard derives the count.
    pub const ALL: [Self; 16] = [
        Self::Title,
        Self::CtrTitle,
        Self::SubTitle,
        Self::Body,
        Self::Date,
        Self::Footer,
        Self::Header,
        Self::SlideNumber,
        Self::SlideImage,
        Self::Chart,
        Self::Table,
        Self::ClipArt,
        Self::Diagram,
        Self::Media,
        Self::Picture,
        Self::Object,
    ];
}

/// How much of the body area a placeholder covers (`p:ph@sz`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceholderSize {
    /// The whole area (`full`), the schema default.
    #[default]
    Full,
    /// Half the area (`half`).
    Half,
    /// A quarter of the area (`quarter`).
    Quarter,
}

impl PlaceholderSize {
    /// The `p:ph@sz` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Half => "half",
            Self::Quarter => "quarter",
        }
    }

    /// Reads a `p:ph@sz` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "half" => Self::Half,
            "quarter" => Self::Quarter,
            _ => Self::Full,
        }
    }
}

/// A placeholder's text direction (`p:ph@orient`, `ST_Direction`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaceholderOrientation {
    /// Left-to-right rows (`horz`), the schema default.
    #[default]
    Horizontal,
    /// Rotated, for the vertical-text layouts (`vert`).
    Vertical,
}

impl PlaceholderOrientation {
    /// The `p:ph@orient` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Horizontal => "horz",
            Self::Vertical => "vert",
        }
    }

    /// Reads a `p:ph@orient` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        if token == "vert" {
            Self::Vertical
        } else {
            Self::Horizontal
        }
    }
}

/// The slot a shape occupies in its layout (`p:ph`).
///
/// Inheritance resolves by the pair [`kind`](Self::kind) **and**
/// [`index`](Self::index), not by either alone: a two-content layout has two `body`
/// slots distinguished only by `idx`, so matching on the type would pick the wrong
/// one half the time.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Placeholder {
    /// The slot kind (`p:ph@type`).
    #[serde(default)]
    pub kind: PlaceholderKind,
    /// The slot index (`p:ph@idx`), zero when the attribute is absent.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub index: u32,
    /// The covered area (`p:ph@sz`).
    #[serde(default, skip_serializing_if = "is_full")]
    pub size: PlaceholderSize,
    /// The text direction (`p:ph@orient`).
    #[serde(default, skip_serializing_if = "is_horizontal")]
    pub orientation: PlaceholderOrientation,
    /// Whether the slot shows an authored prompt rather than the layout's
    /// ("Click to add title") — `p:ph@hasCustomPrompt`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub has_custom_prompt: bool,
}

impl Placeholder {
    /// The slot a shape is matched against its layout by.
    #[must_use]
    pub const fn slot(&self) -> (PlaceholderKind, u32) {
        (self.kind, self.index)
    }
}

/// `skip_serializing_if` helper: an omitted `p:ph@idx` is zero.
fn is_zero(index: &u32) -> bool {
    *index == 0
}

/// `skip_serializing_if` helper: an omitted `p:ph@sz` is `full`.
fn is_full(size: &PlaceholderSize) -> bool {
    matches!(size, PlaceholderSize::Full)
}

/// `skip_serializing_if` helper: an omitted `p:ph@orient` is `horz`.
fn is_horizontal(orientation: &PlaceholderOrientation) -> bool {
    matches!(orientation, PlaceholderOrientation::Horizontal)
}
