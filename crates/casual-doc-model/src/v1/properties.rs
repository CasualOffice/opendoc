//! Typed paragraph and run properties and their value types.

use serde::{Deserialize, Serialize};

use super::{BorderEdge, NumberingInstanceId, SectionId, Shading, StyleId};

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

/// A theme font slot (`w:rFonts@*Theme`, ECMA-376 §17.3.2.26). Each value names
/// a major (heading) or minor (body) collection and the script axis
/// (ascii/hAnsi/eastAsia/bidi) it resolves against in the theme font scheme.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeFontRef {
    /// `majorAscii`.
    MajorAscii,
    /// `majorHAnsi`.
    MajorHAnsi,
    /// `majorEastAsia`.
    MajorEastAsia,
    /// `majorBidi`.
    MajorBidi,
    /// `minorAscii`.
    MinorAscii,
    /// `minorHAnsi`.
    MinorHAnsi,
    /// `minorEastAsia`.
    MinorEastAsia,
    /// `minorBidi`.
    MinorBidi,
}

/// The `w:rFonts@hint` disambiguator: which slot applies to a code point that
/// falls in an ambiguous Unicode range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunFontHint {
    /// `default`.
    Default,
    /// `eastAsia`.
    EastAsia,
    /// `cs`.
    Cs,
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

/// A `w:font` family classification (`w:family@w:val`, ECMA-376 §17.8).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontFamilyKind {
    /// `auto`.
    Auto,
    /// `decorative`.
    Decorative,
    /// `modern`.
    Modern,
    /// `roman`.
    Roman,
    /// `script`.
    Script,
    /// `swiss`.
    Swiss,
}

/// A `w:font` character pitch (`w:pitch@w:val`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontPitch {
    /// `default`.
    Default,
    /// `fixed`.
    Fixed,
    /// `variable`.
    Variable,
}

/// The OS/2 Unicode + code-page coverage signature (`w:sig`). Each field is the
/// producer's 32-bit hex value retained verbatim (opaque), never reinterpreted,
/// so unknown coverage bits are preserved.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontSig {
    /// `w:usb0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb0: Option<String>,
    /// `w:usb1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb1: Option<String>,
    /// `w:usb2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb2: Option<String>,
    /// `w:usb3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb3: Option<String>,
    /// `w:csb0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csb0: Option<String>,
    /// `w:csb1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csb1: Option<String>,
}

impl FontSig {
    /// Whether no signature field is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.usb0.is_none()
            && self.usb1.is_none()
            && self.usb2.is_none()
            && self.usb3.is_none()
            && self.csb0.is_none()
            && self.csb1.is_none()
    }
}

/// A `w:font` descriptor from `word/fontTable.xml` (ECMA-376 §17.8): the
/// substitution/coverage hints a producer records for a font family. Keyed by
/// `name`; entries are preserved even when no run references the family (Word
/// emits stale entries). `panose1`/`charset` and the `sig` fields are retained
/// as written (opaque hex) so unknown bits are never dropped.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontDescriptor {
    /// The font family name (`w:font@w:name`, non-empty, at most 255 bytes).
    pub name: String,
    /// Alternate family name used as a substitution hint (`w:altName@w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_name: Option<String>,
    /// PANOSE-1 classification (`w:panose1@w:val`), opaque hex as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panose1: Option<String>,
    /// Windows charset byte (`w:charset@w:val`), opaque hex as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    /// Family classification (`w:family@w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<FontFamilyKind>,
    /// Character pitch (`w:pitch@w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<FontPitch>,
    /// OS/2 coverage signature (`w:sig`).
    #[serde(default, skip_serializing_if = "FontSig::is_empty")]
    pub sig: FontSig,
    /// Whether the font is a non-TrueType (raster) face (`w:notTrueType`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_true_type: bool,
    /// Embedded font faces for this family (`w:embedRegular`/…).
    #[serde(default, skip_serializing_if = "EmbeddedFontSet::is_empty")]
    pub embedded: EmbeddedFontSet,
}

/// One embedded font face (`w:embedRegular`/`w:embedBold`/…). The obfuscated
/// `.odttf` bytes live in a package part; the model keeps the metadata verbatim
/// so it round-trips (no de-obfuscation — that is a rendering concern).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedFace {
    /// The de-obfuscation key (`w:fontKey`, a `{GUID}`), retained verbatim.
    pub font_key: String,
    /// Whether the embedded font was subset to used glyphs (`w:subsetted`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subsetted: bool,
    /// The `fontTable.xml.rels` relationship id, retained verbatim.
    pub relationship_id: String,
    /// The `.odttf` package part name (e.g. `word/fonts/font1.odttf`).
    pub part_name: String,
}

/// The embedded faces of a font family (regular/bold/italic/bold-italic).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedFontSet {
    /// `w:embedRegular`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regular: Option<EmbeddedFace>,
    /// `w:embedBold`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<EmbeddedFace>,
    /// `w:embedItalic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<EmbeddedFace>,
    /// `w:embedBoldItalic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold_italic: Option<EmbeddedFace>,
}

impl EmbeddedFontSet {
    /// Whether no face is embedded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regular.is_none()
            && self.bold.is_none()
            && self.italic.is_none()
            && self.bold_italic.is_none()
    }

    /// The faces present, in a fixed order, with their `w:embed*` element names.
    #[must_use]
    pub fn faces(&self) -> Vec<(&'static str, &EmbeddedFace)> {
        [
            ("w:embedRegular", &self.regular),
            ("w:embedBold", &self.bold),
            ("w:embedItalic", &self.italic),
            ("w:embedBoldItalic", &self.bold_italic),
        ]
        .into_iter()
        .filter_map(|(name, face)| face.as_ref().map(|face| (name, face)))
        .collect()
    }
}

/// One theme font entry (`a:latin`/`a:ea`/`a:cs`, ECMA-376 §20.1.4.1). Its
/// `typeface` may be empty (meaning "fall back to the latin entry"); the
/// panose/pitch/charset hints are retained verbatim (opaque).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeFontEntry {
    /// The typeface name (`@typeface`, possibly empty).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub typeface: String,
    /// PANOSE classification (`@panose`), opaque hex as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panose: Option<String>,
    /// Pitch/family byte (`@pitchFamily`), opaque as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_family: Option<String>,
    /// Windows charset (`@charset`), opaque as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
}

/// A per-script typeface override (`<a:font script="Hans" typeface="..."/>`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptFont {
    /// The ISO-15924 script tag (`@script`).
    pub script: String,
    /// The typeface for that script (`@typeface`).
    pub typeface: String,
}

/// A major or minor font collection (`a:majorFont`/`a:minorFont`): the three
/// base entries plus any per-script overrides.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontCollection {
    /// Latin entry (`a:latin`).
    #[serde(default, skip_serializing_if = "ThemeFontEntry::is_default")]
    pub latin: ThemeFontEntry,
    /// East-Asian entry (`a:ea`).
    #[serde(default, skip_serializing_if = "ThemeFontEntry::is_default")]
    pub ea: ThemeFontEntry,
    /// Complex-script entry (`a:cs`).
    #[serde(default, skip_serializing_if = "ThemeFontEntry::is_default")]
    pub cs: ThemeFontEntry,
    /// Per-script typeface overrides, in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub script_overrides: Vec<ScriptFont>,
}

impl ThemeFontEntry {
    /// Whether the entry is empty (no typeface or hints).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == ThemeFontEntry::default()
    }
}

/// The theme font scheme (`theme1.xml` `a:fontScheme`): the major (heading) and
/// minor (body) collections against which `w:rFonts@*Theme` slots resolve.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontScheme {
    /// The major (heading) collection (`a:majorFont`).
    pub major: FontCollection,
    /// The minor (body) collection (`a:minorFont`).
    pub minor: FontCollection,
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
    /// This paragraph ends the referenced section: its `w:pPr` carries a nested
    /// `w:sectPr` whose geometry lives in `Definitions.sections`. The final
    /// (body-level) section is the trailing `sections` entry that no paragraph
    /// references. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_break: Option<SectionId>,
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
    /// Font hint (`w:rFonts@hint`) disambiguating slot selection for code points
    /// in an ambiguous Unicode range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_hint: Option<RunFontHint>,
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
    /// Inter-character spacing in twips (`w:spacing`), may be negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_spacing_twips: Option<i32>,
    /// Kerning activation threshold in half-points (`w:kern`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kerning_half_points: Option<u32>,
    /// Baseline offset in half-points (`w:position`), may be negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_half_points: Option<i32>,
    /// Language tags (`w:lang`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    /// Outline (hollow) effect (`w:outline`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<bool>,
    /// Shadow effect (`w:shadow`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    /// Embossed effect (`w:emboss`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emboss: Option<bool>,
    /// Imprint (engrave) effect (`w:imprint`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imprint: Option<bool>,
    /// Right-to-left run (`w:rtl`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtl: Option<bool>,
    /// Snap to the document grid (`w:snapToGrid`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_to_grid: Option<bool>,
    /// Hidden only when the paragraph mark is hidden (`w:specVanish`), distinct
    /// from `hidden` (`w:vanish`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_vanish: Option<bool>,
    /// Run border (`w:bdr`), a single border edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<BorderEdge>,
    /// Run background shading (`w:shd`).
    #[serde(default, skip_serializing_if = "Shading::is_empty")]
    pub shading: Shading,
}

/// Run language tags (`w:lang`). Each tag is a producer-written language string
/// (BCP-47-ish), retained opaquely and bounded, not parsed.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Language {
    /// Latin (`w:val`) language tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// East-Asian (`w:eastAsia`) language tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asia: Option<String>,
    /// Complex-script (`w:bidi`) language tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidi: Option<String>,
}

impl Language {
    /// Whether no tag is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
