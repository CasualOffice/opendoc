//! Typed paragraph and run properties and their value types.

use serde::{Deserialize, Serialize};

use super::{NumberingInstanceId, StyleId};

/// Paragraph alignment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Start-aligned.
    Start,
    /// End-aligned.
    End,
    /// Centered.
    Center,
    /// Justified.
    Justify,
}

/// The kind of a style definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleKind {
    /// A paragraph style.
    Paragraph,
    /// A character (run) style.
    Character,
}

/// An explicit break kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakKind {
    /// Line break.
    Line,
    /// Page break.
    Page,
    /// Column break.
    Column,
}

/// A theme color slot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColorRef {
    /// Dark 1.
    Dark1,
    /// Light 1.
    Light1,
    /// Dark 2.
    Dark2,
    /// Light 2.
    Light2,
    /// Accent 1.
    Accent1,
    /// Accent 2.
    Accent2,
    /// Accent 3.
    Accent3,
    /// Accent 4.
    Accent4,
    /// Accent 5.
    Accent5,
    /// Accent 6.
    Accent6,
    /// Hyperlink.
    Hyperlink,
    /// Followed hyperlink.
    FollowedHyperlink,
}

/// A theme font slot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeFontRef {
    /// Major (heading) font.
    Major,
    /// Minor (body) font.
    Minor,
}

/// An explicit sRGB color.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RgbColor {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

/// A theme color reference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeColor {
    /// The referenced slot.
    pub slot: ThemeColorRef,
}

/// A run color: theme reference or explicit RGB.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Color {
    /// A theme color slot.
    Theme(ThemeColor),
    /// An explicit RGB color.
    Rgb(RgbColor),
}

/// A named font.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontName {
    /// The font family name.
    pub name: String,
}

/// A theme font reference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeFont {
    /// The referenced slot.
    pub slot: ThemeFontRef,
}

/// A run font: theme reference or named family.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FontRef {
    /// A theme font slot.
    Theme(ThemeFont),
    /// A named font family.
    Named(FontName),
}

/// Paragraph indentation in twips.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Indentation {
    /// Leading indent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_twips: Option<i32>,
    /// Trailing indent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_twips: Option<i32>,
    /// First-line indent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_line_twips: Option<i32>,
    /// Hanging indent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hanging_twips: Option<i32>,
}

/// Paragraph spacing.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spacing {
    /// Space before, in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_twips: Option<i32>,
    /// Space after, in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_twips: Option<i32>,
    /// Line spacing as a percentage (100 = single).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_percent: Option<u16>,
}

/// A paragraph's numbering reference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingRef {
    /// The numbering instance.
    pub instance: NumberingInstanceId,
    /// The level within the instance.
    pub level: u8,
}

/// Typed paragraph properties. An empty value serializes to `{}`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphProperties {
    /// Referenced paragraph style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
    /// Numbering reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numbering: Option<NumberingRef>,
    /// Alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Indentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indentation: Option<Indentation>,
    /// Spacing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<Spacing>,
}

/// Typed run properties. An empty value serializes to `{}`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunProperties {
    /// Referenced character style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
    /// Bold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Underline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Strike-through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Font size in half-points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_half_points: Option<u32>,
    /// Font reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ref: Option<FontRef>,
}
