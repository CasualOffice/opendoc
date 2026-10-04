// SPDX-License-Identifier: Apache-2.0

//! A shape's text (`a:txBody`): its body properties, its nine-level default style,
//! and its paragraphs.

use casual_doc_model::NodeId;
use serde::{Deserialize, Serialize};

use crate::{PresentationError, TextCharacterProperties, TextParagraph, TextParagraphProperties};

/// How many outline levels DrawingML defines (`a:lvl1pPr` … `a:lvl9pPr`), and so the
/// bound on `a:pPr@lvl`, which is **zero-based** (`0..=8`) while the element names
/// are one-based. That off-by-one is a real source of wrong-indent bugs, so the two
/// forms never share a variable here.
pub const TEXT_LEVELS: usize = 9;

/// The highest legal `a:pPr@lvl`.
pub const MAX_TEXT_LEVEL: u8 = 8;

/// How text is fitted to its shape (`a:bodyPr`'s autofit child).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TextAutoFit {
    /// `a:noAutofit` — text overflows the shape. The default.
    #[default]
    None,
    /// `a:normAutofit` — the text was SHRUNK to fit, and the producer recorded by how
    /// much.
    ///
    /// Both values are **authoritative on load**: the producer solved the fit and
    /// wrote the answer, so honouring them is what makes the file render identically.
    /// They are re-solved on EDIT, not on open — resolving at open would replace
    /// Word's own numbers with ours and change what an untouched file looks like.
    Normal {
        /// `a:normAutofit@fontScale`, in thousandths of a percent; `None` is 100%.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_scale: Option<u32>,
        /// `a:normAutofit@lnSpcReduction`, in thousandths of a percent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_space_reduction: Option<u32>,
    },
    /// `a:spAutoFit` — the SHAPE grows to fit the text, which is the inverse
    /// relationship and must not be conflated with `normAutofit`.
    Shape,
}

/// Where text sits vertically in its shape (`a:bodyPr@anchor`, `ST_TextAnchoringType`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAnchor {
    /// `t`, the default.
    #[default]
    Top,
    /// `ctr`.
    Center,
    /// `b`.
    Bottom,
    /// `just`.
    Justify,
    /// `dist`.
    Distribute,
}

impl TextAnchor {
    /// The `a:bodyPr@anchor` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Center => "ctr",
            Self::Bottom => "b",
            Self::Justify => "just",
            Self::Distribute => "dist",
        }
    }

    /// Reads an `a:bodyPr@anchor` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        match token {
            "ctr" => Self::Center,
            "b" => Self::Bottom,
            "just" => Self::Justify,
            "dist" => Self::Distribute,
            _ => Self::Top,
        }
    }
}

/// Text flow direction (`a:bodyPr@vert`).
///
/// Re-exported from the document model rather than declared here. It was declared
/// twice: `wps:bodyPr@vert` is a DOCUMENT construct — `105` FID-L-08 tracks it as a
/// DOCX defect — so the shared vocabulary belongs in the layer that is depended on,
/// not in the one that depends. Two copies of a seven-variant token table would have
/// disagreed the first time one gained a variant.
pub use casual_doc_model::v1::TextVertical;

/// Whether text wraps inside the shape (`a:bodyPr@wrap`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextWrap {
    /// `square` — wrap at the shape's width. The default.
    #[default]
    Square,
    /// `none` — a single unwrapped line that overflows.
    None,
}

impl TextWrap {
    /// The `a:bodyPr@wrap` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::None => "none",
        }
    }

    /// Reads an `a:bodyPr@wrap` token; anything else is the default.
    #[must_use]
    pub fn from_token(token: &str) -> Self {
        if token == "none" {
            Self::None
        } else {
            Self::Square
        }
    }
}

/// Text body properties (`a:bodyPr`).
///
/// The insets carry DrawingML's own asymmetric defaults rather than zero, because an
/// absent attribute means the default and not "no inset": 0.1 inch left/right and
/// 0.05 inch top/bottom, in EMU.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBodyProperties {
    /// `a:bodyPr@lIns`, EMU.
    pub inset_left_emu: i64,
    /// `a:bodyPr@tIns`, EMU.
    pub inset_top_emu: i64,
    /// `a:bodyPr@rIns`, EMU.
    pub inset_right_emu: i64,
    /// `a:bodyPr@bIns`, EMU.
    pub inset_bottom_emu: i64,
    /// `a:bodyPr@anchor`.
    #[serde(default)]
    pub anchor: TextAnchor,
    /// `a:bodyPr@anchorCtr` — centre the text block horizontally in the shape,
    /// which is independent of paragraph alignment.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub anchor_center: bool,
    /// `a:bodyPr@wrap`.
    #[serde(default)]
    pub wrap: TextWrap,
    /// `a:bodyPr@vert`.
    #[serde(default)]
    pub vertical: TextVertical,
    /// `a:bodyPr@rot`, in 60000ths of a degree — the text's own rotation, SEPARATE
    /// from the shape's `a:xfrm@rot`. Both apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    /// The autofit mode.
    #[serde(default)]
    pub auto_fit: TextAutoFit,
    /// `a:bodyPr@numCol` — text columns inside one shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_count: Option<u16>,
    /// `a:bodyPr@spcCol` — the gap between those columns, EMU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_space_emu: Option<i64>,
}

impl Default for TextBodyProperties {
    fn default() -> Self {
        Self {
            inset_left_emu: Self::DEFAULT_SIDE_INSET_EMU,
            inset_top_emu: Self::DEFAULT_VERTICAL_INSET_EMU,
            inset_right_emu: Self::DEFAULT_SIDE_INSET_EMU,
            inset_bottom_emu: Self::DEFAULT_VERTICAL_INSET_EMU,
            anchor: TextAnchor::Top,
            anchor_center: false,
            wrap: TextWrap::Square,
            vertical: TextVertical::Horizontal,
            rotation: None,
            auto_fit: TextAutoFit::None,
            column_count: None,
            column_space_emu: None,
        }
    }
}

impl TextBodyProperties {
    /// DrawingML's default left/right inset: 0.1 inch.
    pub const DEFAULT_SIDE_INSET_EMU: i64 = 91_440;
    /// DrawingML's default top/bottom inset: 0.05 inch.
    pub const DEFAULT_VERTICAL_INSET_EMU: i64 = 45_720;
}

/// A shape's nine-level default paragraph style (`a:lstStyle`).
///
/// Indexed by the ZERO-based level, so `levels[0]` is `a:lvl1pPr`. `None` at a level
/// means the shape states nothing there and the level inherits from the tier above.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ListStyle {
    /// One optional override per outline level.
    pub levels: Vec<Option<TextParagraphProperties>>,
}

impl ListStyle {
    /// The override declared for a zero-based level, if any.
    ///
    /// # Complexity
    ///
    /// O(1).
    #[must_use]
    pub fn level(&self, level: u8) -> Option<&TextParagraphProperties> {
        self.levels.get(usize::from(level)).and_then(Option::as_ref)
    }

    /// Whether the list style declares nothing, in which case `a:lstStyle` is omitted
    /// on write rather than emitted empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.levels.iter().all(Option::is_none)
    }

    /// Refuses a list style with more than [`TEXT_LEVELS`] levels, and validates
    /// each level it does declare.
    ///
    /// Lives here rather than at each call site because there are now four kinds of
    /// `a:lstStyle` in the model — a shape's, a placeholder's, each of a master's
    /// three `p:txStyles` tiers, and the presentation's `p:defaultTextStyle` — and
    /// a per-site copy of one rule is a rule the next site forgets. The nine-level
    /// ceiling is `ST_TextIndentLevelType`'s own, so it is the same everywhere.
    ///
    /// O(levels), which is bounded by the ceiling it enforces.
    pub fn validate(&self) -> Result<(), PresentationError> {
        if self.levels.len() > TEXT_LEVELS {
            return Err(PresentationError::TooManyTextLevels(self.levels.len()));
        }
        for properties in self.levels.iter().flatten() {
            properties.validate()?;
        }
        Ok(())
    }
}

/// A shape's text (`a:txBody`).
///
/// # Why this is not `Vec<BlockNode>`
///
/// It was, as an interim carrier, and that was the one thing in this design
/// guaranteed to need redoing. A `w:p` and an `a:p` are different elements with
/// different children, different property vocabularies and different units, and the
/// list divergence is structural rather than cosmetic — see [`crate::TextBullet`].
/// Carrying slide text as document blocks would have meant converting at every
/// boundary and losing `@lvl`, the inline bullet and the nine-level `a:lstStyle`
/// outright.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBody {
    /// `a:bodyPr`.
    #[serde(default)]
    pub body_properties: TextBodyProperties,
    /// `a:lstStyle`.
    #[serde(default, skip_serializing_if = "ListStyle::is_empty")]
    pub list_style: ListStyle,
    /// `a:p`, in order. A `a:txBody` always holds at least one paragraph, even for
    /// empty text — PowerPoint writes an empty `a:p` rather than omitting it.
    pub paragraphs: Vec<TextParagraph>,
}

impl TextBody {
    /// An empty body: default properties and one empty paragraph, which is what an
    /// empty placeholder actually contains.
    #[must_use]
    pub fn empty(paragraph_id: NodeId) -> Self {
        Self {
            body_properties: TextBodyProperties::default(),
            list_style: ListStyle::default(),
            paragraphs: vec![TextParagraph::empty(paragraph_id)],
        }
    }

    /// The body's plain text, paragraphs joined by `\n`.
    ///
    /// For search, accessibility and the placeholder-prompt check — NOT a
    /// round-trip representation, since it discards every property.
    ///
    /// # Complexity
    ///
    /// O(text).
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.paragraphs
            .iter()
            .map(TextParagraph::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Visits every node id this body carries.
    ///
    /// # Complexity
    ///
    /// O(runs).
    pub fn visit_node_ids(&self, visit: &mut dyn FnMut(NodeId)) {
        for paragraph in &self.paragraphs {
            paragraph.visit_node_ids(visit);
        }
    }

    /// Checks the body's invariants: at least one paragraph, levels in range, font
    /// sizes in `ST_TextFontSize`, and a list style no deeper than nine levels.
    ///
    /// # Complexity
    ///
    /// O(runs).
    pub fn validate(&self) -> Result<(), PresentationError> {
        if self.paragraphs.is_empty() {
            return Err(PresentationError::EmptyTextBody);
        }
        self.list_style.validate()?;
        for paragraph in &self.paragraphs {
            paragraph.validate()?;
        }
        Ok(())
    }

    /// The character properties that apply to the paragraph mark of the last
    /// paragraph, which is what a caret at the very end of the text inherits.
    #[must_use]
    pub fn trailing_character_properties(&self) -> Option<&TextCharacterProperties> {
        self.paragraphs.last()?.end_properties.as_deref()
    }
}
