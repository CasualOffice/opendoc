//! Typed paragraph and run properties and their value types.

use serde::{Deserialize, Serialize};

use super::{BorderEdge, NumberingInstanceId, Shading, StyleId};

/// A paragraph border set (`w:pBdr`); any subset of edges. Reuses the shared
/// `BorderEdge` value type.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParagraphBorders {
    /// Top edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<BorderEdge>,
    /// Bottom edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<BorderEdge>,
    /// Leading (start) edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<BorderEdge>,
    /// Trailing (end) edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<BorderEdge>,
    /// Border between consecutive same-properties paragraphs (`w:between`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<BorderEdge>,
    /// Border bar (`w:bar`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar: Option<BorderEdge>,
}

impl ParagraphBorders {
    /// Whether no edge is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A custom tab stop's alignment (`w:tab/@w:val`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TabAlignment {
    /// Left/start-aligned.
    Start,
    /// Centered.
    Center,
    /// Right/end-aligned.
    End,
    /// Aligned on the decimal separator.
    Decimal,
    /// A vertical bar.
    Bar,
}

/// A custom tab stop's leader glyph (`w:tab/@w:leader`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TabLeader {
    /// A dotted leader.
    Dot,
    /// A hyphen leader.
    Hyphen,
    /// An underscore leader.
    Underscore,
    /// A middle-dot leader.
    MiddleDot,
    /// A heavy (thick) leader.
    Heavy,
}

/// A custom tab stop (`w:tabs > w:tab`). A `clear` tab is not modeled (reported).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabStop {
    /// Position in twips from the leading margin (`w:pos`), may be negative.
    pub position_twips: i32,
    /// Alignment (`w:val`).
    pub alignment: TabAlignment,
    /// Leader glyph (`w:leader`), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader: Option<TabLeader>,
}

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
    /// Keep this paragraph on the same page as the next (`w:keepNext`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_next: bool,
    /// Keep all lines of this paragraph on one page (`w:keepLines`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_lines: bool,
    /// Force a page break before this paragraph (`w:pageBreakBefore`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub page_break_before: bool,
    /// Suppress the first/last-line widow/orphan control (`w:widowControl`).
    ///
    /// `true` means widow control is ON (OOXML default is on; a value of `false`
    /// only appears when a producer explicitly disables it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub widow_control: bool,
    /// Do not add spacing between paragraphs of the same style
    /// (`w:contextualSpacing`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contextual_spacing: bool,
    /// Suppress line numbers for this paragraph (`w:suppressLineNumbers`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppress_line_numbers: bool,
    /// Outline (heading) level, `0..=9` (`w:outlineLvl`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u8>,
    /// Paragraph borders (`w:pBdr`).
    #[serde(default, skip_serializing_if = "ParagraphBorders::is_empty")]
    pub borders: ParagraphBorders,
    /// Paragraph background shading (`w:shd`).
    #[serde(default, skip_serializing_if = "Shading::is_empty")]
    pub shading: Shading,
    /// Custom tab stops (`w:tabs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabStop>,
}

/// Run vertical alignment (`w:vertAlign`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment {
    /// Normal baseline.
    Baseline,
    /// Raised (superscript).
    Superscript,
    /// Lowered (subscript).
    Subscript,
}

/// A named text-highlight color (`w:highlight`, `ST_HighlightColor`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    /// No highlight (an explicit clear).
    None,
    /// Black.
    Black,
    /// Blue.
    Blue,
    /// Cyan.
    Cyan,
    /// Dark blue.
    DarkBlue,
    /// Dark cyan.
    DarkCyan,
    /// Dark gray.
    DarkGray,
    /// Dark green.
    DarkGreen,
    /// Dark magenta.
    DarkMagenta,
    /// Dark red.
    DarkRed,
    /// Dark yellow.
    DarkYellow,
    /// Green.
    Green,
    /// Light gray.
    LightGray,
    /// Magenta.
    Magenta,
    /// Red.
    Red,
    /// White.
    White,
    /// Yellow.
    Yellow,
}

/// An emphasis mark (`w:em`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmphasisMark {
    /// No emphasis mark (an explicit clear).
    None,
    /// A dot above (or below) each character.
    Dot,
    /// A comma above each character.
    Comma,
    /// A circle above each character.
    Circle,
    /// A dot below each character.
    UnderDot,
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
    /// Font reference (the `w:rFonts@ascii`/`@asciiTheme` slot).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ref: Option<FontRef>,
    /// High-ANSI font slot (`w:rFonts@hAnsi`/`@hAnsiTheme`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ref_h_ansi: Option<FontRef>,
    /// Complex-script font slot (`w:rFonts@cs`/`@csTheme`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ref_cs: Option<FontRef>,
    /// East-Asian font slot (`w:rFonts@eastAsia`/`@eastAsiaTheme`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_ref_east_asia: Option<FontRef>,
    /// All-capitals rendering (`w:caps`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_caps: Option<bool>,
    /// Small-capitals rendering (`w:smallCaps`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_caps: Option<bool>,
    /// Hidden text (`w:vanish`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Hidden in web view (`w:webHidden`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_hidden: Option<bool>,
    /// Double strike-through (`w:dstrike`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_strike: Option<bool>,
    /// Superscript / subscript (`w:vertAlign`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Text highlight color (`w:highlight`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<HighlightColor>,
    /// Emphasis mark (`w:em`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<EmphasisMark>,
}
