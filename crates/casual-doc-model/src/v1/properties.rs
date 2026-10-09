//! Typed paragraph and run properties and their value types.

use serde::{Deserialize, Serialize};

use super::{
    BorderEdge, DashStyle, GradientKind, NumberingInstanceId, RevisionGroup, Rgba, SectionId,
    Shading, StyleId, TextDirection,
};

/// A format-change tracked revision (`w:rPrChange`, `w:pPrChange`,
/// `w:tblPrChange`, `w:trPrChange`, `w:tcPrChange`, `w:tblGridChange`): the
/// revision metadata plus the PRIOR value of the properties the change replaced.
///
/// It is attached to the CURRENT properties (the values in effect now); `prior`
/// is a full snapshot of those same properties as they were before the change,
/// reusing the very type it hangs off (a boxed `P` — the recursion is broken by
/// the box, and a prior snapshot never itself carries a further `prop_change`).
///
/// Author/date/id are retained as the producer wrote them (opaque, bounded),
/// mirroring [`super::Revision`] metadata. `w:tblGridChange` carries only an id
/// (no author/date); those fields are simply `None` for it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropChange<P> {
    /// The revision author, if declared (non-empty, at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The revision date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The producer's revision id (`w:id`) as written, if declared (<= 64 bytes).
    /// Opaque and non-unique across changes — a grouping key, not a node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    /// OpenDoc-only atomic decision grouping, separate from serialized `w:id`.
    ///
    /// Used by editor-authored run-format changes; imported property changes
    /// leave it absent. The semantic DOCX writer does not serialize this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_group: Option<RevisionGroup>,
    /// The prior properties snapshot (the values the change replaced).
    pub prior: Box<P>,
}

/// Whether a tracked mark change (a paragraph mark, or a table row/cell) was
/// inserted or deleted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkRevisionKind {
    /// The mark was inserted (`w:pPr > w:rPr > w:ins`, `w:trPr > w:ins`, or
    /// `w:tcPr > w:cellIns`) — a tracked insertion (for a paragraph mark, a
    /// split).
    Insertion,
    /// The mark was deleted (`w:pPr > w:rPr > w:del`, `w:trPr > w:del`, or
    /// `w:tcPr > w:cellDel`) — a tracked deletion (for a paragraph mark, a merge
    /// with the next paragraph).
    Deletion,
}

/// A tracked insertion/deletion carrying only revision metadata, modeling a
/// content mark (not a span) inserted or deleted under tracked changes: a
/// paragraph mark (`w:pPr > w:rPr > w:ins`/`w:del`) — the pilcrow itself, a
/// tracked split/merge distinct from any change to the paragraph's runs — and,
/// reusing the same shape, a tracked table row (`w:trPr > w:ins`/`w:del`) or
/// cell (`w:tcPr > w:cellIns`/`w:cellDel`) insertion/deletion.
///
/// Author/date/id are retained as the producer wrote them (opaque, bounded),
/// mirroring [`super::Revision`] and [`PropChange`] metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkRevision {
    /// Whether the mark was inserted or deleted.
    pub kind: MarkRevisionKind,
    /// The revision author, if declared (non-empty, at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The revision date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The producer's revision id (`w:id`) as written, if declared (<= 64 bytes).
    /// Opaque and non-unique across changes — a grouping key, not a node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
}

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

/// The shared empty border set an absent [`BoxedParagraphBorders`] reads as.
static NO_PARAGRAPH_BORDERS: ParagraphBorders = ParagraphBorders {
    top: None,
    bottom: None,
    start: None,
    end: None,
    between: None,
    bar: None,
};

/// A [`ParagraphBorders`] stored out of line, because essentially every
/// paragraph in a document has none.
///
/// `ParagraphBorders` is 288 bytes and was held by value on every paragraph's
/// properties; `docs/111` measured that as the single largest item in a
/// paragraph's 1,565-byte model cost. Absent borders now cost one null pointer
/// (8 bytes on a 64-bit host) and a bordered paragraph pays one small
/// allocation.
///
/// It behaves as the value it wraps rather than as an option, so reading code is
/// unchanged: it derefs to a `&ParagraphBorders` (borrowing a shared empty set
/// when absent), and `Debug`, `PartialEq` and the serialized form are all
/// identical to the plain value it replaced. In particular **absent and present
/// but empty compare equal and serialize the same** — nothing observable
/// distinguishes them, so no caller has to care which one it holds.
///
/// Mutating through [`DerefMut`](std::ops::DerefMut) (`properties.borders.top =
/// …`) allocates on demand.
#[derive(Clone, Default)]
pub struct BoxedParagraphBorders(Option<Box<ParagraphBorders>>);

impl BoxedParagraphBorders {
    /// Whether no edge is set (serializes to nothing) — true both when the set
    /// is absent and when an allocated set has had every edge cleared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_none_or(|borders| borders.is_empty())
    }

    /// Whether an allocation is held. Not part of the value's meaning — an
    /// allocated but empty set is still [`is_empty`](Self::is_empty) — this is
    /// for memory assertions only.
    #[must_use]
    pub fn is_allocated(&self) -> bool {
        self.0.is_some()
    }
}

impl From<ParagraphBorders> for BoxedParagraphBorders {
    /// Stores `borders`, keeping an empty set out of line (so an empty set never
    /// allocates, whichever way it was built).
    fn from(borders: ParagraphBorders) -> Self {
        if borders.is_empty() {
            Self(None)
        } else {
            Self(Some(Box::new(borders)))
        }
    }
}

impl std::ops::Deref for BoxedParagraphBorders {
    type Target = ParagraphBorders;

    fn deref(&self) -> &ParagraphBorders {
        self.0.as_deref().unwrap_or(&NO_PARAGRAPH_BORDERS)
    }
}

impl std::ops::DerefMut for BoxedParagraphBorders {
    /// Allocates an empty set on first mutable access.
    fn deref_mut(&mut self) -> &mut ParagraphBorders {
        self.0.get_or_insert_with(Box::default)
    }
}

impl std::fmt::Debug for BoxedParagraphBorders {
    /// Prints the border set itself, so diagnostics are unchanged by the boxing.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

impl PartialEq for BoxedParagraphBorders {
    /// Compares the border sets, so absent equals present-but-empty exactly as
    /// two empty by-value sets compared equal before.
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for BoxedParagraphBorders {}

impl Serialize for BoxedParagraphBorders {
    /// Serializes as the border set itself (the field is skipped when empty), so
    /// the JSON shape is byte-identical to the unboxed value's.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoxedParagraphBorders {
    /// Accepts exactly what the unboxed value accepted; an explicitly empty
    /// `{}` is stored out of line.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ParagraphBorders::deserialize(deserializer).map(Self::from)
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
    /// Clears (suppresses) an inherited/default tab stop at this position
    /// (`w:val="clear"`); carries no leader.
    Clear,
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

/// A custom tab stop (`w:tabs > w:tab`). A `clear` tab is modeled as a
/// [`TabAlignment::Clear`] stop (a suppression of an inherited/default stop).
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

/// Vertical alignment of text on the line (`w:textAlignment/@w:val`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalTextAlignment {
    /// Automatic (`auto`).
    Auto,
    /// Aligned to the text baseline.
    Baseline,
    /// Aligned to the bottom.
    Bottom,
    /// Centered.
    Center,
    /// Aligned to the top.
    Top,
}

/// The kind of a style definition (`w:style/@w:type`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleKind {
    /// A paragraph style.
    Paragraph,
    /// A character (run) style.
    Character,
    /// A table style: carries table/row/cell defaults and per-region
    /// conditional formatting (`w:tblStylePr`).
    Table,
    /// A numbering style (`w:type="numbering"`), a thin style that names a
    /// list definition and carries metadata (`uiPriority`, `name`).
    Numbering,
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
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Tint applied to the slot color (`w:themeTint`/`w:themeFillTint`), a hex byte
    /// `00..=FF`: the fraction of the slot color kept while the remainder blends
    /// toward white. Absent leaves the slot color unmodified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_tint: Option<u8>,
    /// Shade applied to the slot color (`w:themeShade`/`w:themeFillShade`), a hex
    /// byte `00..=FF`: the fraction of the slot color kept while the remainder
    /// blends toward black. Absent leaves the slot color unmodified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_shade: Option<u8>,
}

/// A run color: theme reference or explicit RGB.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Color {
    /// The automatic color (`w:val="auto"`) — resolved by the consumer (black on
    /// a light background). Distinct from absent: an explicit `auto` *overrides*
    /// an inherited style color back to automatic instead of being dropped.
    Auto,
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

impl FontCollection {
    /// The typeface this collection supplies for one script axis.
    ///
    /// An East-Asian or complex-script entry whose `@typeface` is empty inherits
    /// the Latin entry — that empty string is DrawingML's "fall back to latin"
    /// marker, not a typeface named "" — and a Latin entry that is itself empty
    /// resolves to nothing rather than to the empty family name.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn typeface(&self, axis: ThemeFontAxis) -> Option<&str> {
        let entry = match axis {
            ThemeFontAxis::Latin => &self.latin,
            ThemeFontAxis::EastAsia => &self.ea,
            ThemeFontAxis::ComplexScript => &self.cs,
        };
        let typeface = if entry.typeface.is_empty() {
            &self.latin.typeface
        } else {
            &entry.typeface
        };
        (!typeface.is_empty()).then_some(typeface.as_str())
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

impl FontScheme {
    /// The typeface one collection of this scheme supplies for one script axis:
    /// `major` when `major` is true, `minor` otherwise.
    ///
    /// The single place the collection-and-axis rule lives. A `w:rFonts@*Theme`
    /// slot and an `a:fontRef` are two different references that both land here,
    /// and two copies of the empty-entry fallback would diverge the first time one
    /// was corrected.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn typeface(&self, major: bool, axis: ThemeFontAxis) -> Option<&str> {
        if major {
            self.major.typeface(axis)
        } else {
            self.minor.typeface(axis)
        }
    }
}

/// A system color (`a:sysClr`, ECMA-376 §20.1.2.3.33): a named system-palette
/// token plus the last computed sRGB value the producer resolved it to, used as
/// the fallback when the live system value is unavailable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemColor {
    /// The system color token (`@val`, e.g. `windowText`, `window`).
    pub value: String,
    /// The last computed sRGB value (`@lastClr`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_color: Option<RgbColor>,
}

/// A theme scheme color slot value (an `a:clrScheme` child such as `a:dk1`):
/// either an explicit sRGB color (`a:srgbClr`) or a system color (`a:sysClr`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchemeColor {
    /// An explicit sRGB color (`a:srgbClr`).
    Srgb(RgbColor),
    /// A system color (`a:sysClr`).
    System(SystemColor),
}

impl Default for SchemeColor {
    fn default() -> Self {
        SchemeColor::Srgb(RgbColor::default())
    }
}

/// A DrawingML colour transform (`a:lumMod`/`a:lumOff`/`a:tint`/`a:shade`/
/// `a:alpha`) held UNAPPLIED, in the per-100000 units the file states.
///
/// It exists because a theme format-scheme colour may be the `a:phClr`
/// placeholder, and a transform on a placeholder cannot be folded at parse time:
/// there is no base colour yet. The default Office theme's gradient entries are
/// exactly this case — all three stops are `phClr`, and ONLY the transforms tell
/// them apart, so a build that dropped them would resolve a three-stop gradient
/// to three copies of one colour. That is the flattening [`FillStyle`]'s
/// documentation refuses, arriving by a different route.
///
/// A transform over a colour the theme fixes itself is folded at parse time
/// instead, because there the base is known; that is why [`StyleColor::Fixed`]
/// carries no transform.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorTransform {
    /// `a:lumMod@val` — luminance multiplier, per-100000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lum_mod: Option<i32>,
    /// `a:lumOff@val` — luminance offset, per-100000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lum_off: Option<i32>,
    /// `a:satMod@val` — saturation multiplier, per-100000.
    ///
    /// Declared after the luminance pair and before `tint`/`shade` because that is
    /// the order [`fold_color_modifiers`] applies them in, and a field order that
    /// contradicts the application order is how a reader infers the wrong
    /// composition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sat_mod: Option<i32>,
    /// `a:tint@val` — blend toward white, per-100000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<i32>,
    /// `a:shade@val` — blend toward black, per-100000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade: Option<i32>,
    /// `a:alpha@val` — opacity, per-100000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<i32>,
}

impl ColorTransform {
    /// Whether this transform changes nothing.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Folds this transform over `base`.
    ///
    /// The arithmetic is [`fold_color_modifiers`], shared with the importer's own
    /// colour accumulator so the deferred (placeholder) and immediate (known
    /// base) paths cannot drift apart.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn apply(&self, base: Rgba) -> Rgba {
        let frac = |value: Option<i32>| value.map(|v| v as f32 / 100_000.0);
        fold_color_modifiers(
            base,
            frac(self.lum_mod),
            frac(self.lum_off),
            frac(self.sat_mod),
            frac(self.tint),
            frac(self.shade),
            frac(self.alpha),
        )
    }
}

/// Folds DrawingML colour modifiers over `base`, each given as a FRACTION
/// (`0.67`, not `67000`).
///
/// `lumMod` scales and `lumOff` offsets luminance (applied channel-wise, exact
/// for the grayscale bases these decorations use); `satMod` scales saturation in
/// HSL (see [`fold_color_modifiers`] body and the note below); `tint` lightens
/// toward white and `shade` darkens toward black; `alpha` sets opacity.
///
/// One function rather than two: the importer folds modifiers over a base it
/// already knows, and [`ColorTransform::apply`] folds the same modifiers over a
/// base only a style reference supplies. Two implementations of one rule diverge,
/// and a divergence here would show as a themed shape painting a slightly
/// different colour from an explicitly-filled one.
///
/// # The order is part of the contract
///
/// `lumMod`, `lumOff`, `satMod`, `tint`, `shade`, `alpha` — the HSL modifiers
/// first, then the two toward-white/toward-black blends, then opacity. Two pieces
/// of evidence put `satMod` in that slot rather than at the end: ECMA-376
/// §20.1.2.3 lists the `EG_ColorTransform` children alphabetically, where
/// `satMod` falls after `lumMod`/`lumOff` and before `shade`/`tint`; and Office's
/// own default theme writes them in that document order
/// (`<a:lumMod/><a:satMod/><a:tint/>`). The order matters: `satMod` and `tint` do
/// **not** commute whenever the modulation saturates (see the unit guards), so
/// appending it after the blends would paint a different colour.
///
/// Complexity: O(1).
#[must_use]
pub fn fold_color_modifiers(
    base: Rgba,
    lum_mod: Option<f32>,
    lum_off: Option<f32>,
    sat_mod: Option<f32>,
    tint: Option<f32>,
    shade: Option<f32>,
    alpha: Option<f32>,
) -> Rgba {
    let mut rgb = [f32::from(base.r), f32::from(base.g), f32::from(base.b)];
    if let Some(m) = lum_mod {
        for c in &mut rgb {
            *c *= m;
        }
    }
    if let Some(o) = lum_off {
        for c in &mut rgb {
            *c += o * 255.0;
        }
    }
    if let Some(m) = sat_mod {
        modulate_saturation(&mut rgb, m);
    }
    if let Some(t) = tint {
        let t = t.clamp(0.0, 1.0);
        for c in &mut rgb {
            *c = *c * t + 255.0 * (1.0 - t);
        }
    }
    if let Some(s) = shade {
        let s = s.clamp(0.0, 1.0);
        for c in &mut rgb {
            *c *= s;
        }
    }
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Rgba {
        r: clamp(rgb[0]),
        g: clamp(rgb[1]),
        b: clamp(rgb[2]),
        a: alpha.map_or(base.a, |a| clamp(a.clamp(0.0, 1.0) * 255.0)),
    }
}

/// Scales an sRGB triple's HSL **saturation** by `factor` in place, keeping its
/// hue and lightness exactly (`a:satMod`, ECMA-376 §20.1.2.3.26).
///
/// # Why this is not a channel-wise multiply
///
/// Saturation is not an RGB quantity. In HSL, with `L` the lightness and `C` the
/// chroma, every channel is affine in chroma for a fixed hue and lightness:
/// `c = L + C·(p − ½)` where `p` depends only on the hue. Scaling `S` scales `C`
/// by the same factor, so modulating saturation is **exactly** a linear scaling
/// of every channel about `L`:
///
/// ```text
/// L  = (max + min) / 2
/// c' = L + (c − L) · k
/// ```
///
/// That identity is what lets this stay in the RGB domain without an HSL round
/// trip: converting to HSL and back would introduce two rounding errors for an
/// operation that provably needs none, and would lose the hue of a colour whose
/// chroma the conversion flattened. Hue is preserved because every `(c − L)`
/// scales by the same `k`, and lightness because `max' + min' = 2L` again.
///
/// # The ceiling
///
/// `S` cannot exceed `1`, so `C` cannot exceed the chroma a fully saturated
/// colour has at this lightness, `min(2L, 510 − 2L)`. A `satMod` that would push
/// past it saturates there rather than wrapping or clipping per channel — clipping
/// a channel independently would shift the hue, which is the one thing a
/// saturation modifier must not do. This ceiling is also the only reason `satMod`
/// fails to commute with `tint`/`shade`/`lumMod`: all four of those are affine
/// maps, and the scaling above commutes with any affine map at a fixed `k`.
///
/// # Exactness
///
/// `<a:satMod val="100000"/>` is a bit-exact no-op, and that is a **measured**
/// property of the arithmetic rather than a special case: `k` is then exactly `1`
/// and `L + (c − L)·1` was verified to round back to `c` for all 2^24 sRGB bases,
/// so an `if k == 1.0 { return }` short circuit was written, shown to change no
/// colour, and deliberately removed — an unfalsifiable line in a colour path is
/// worse than none (`SKILL` §4).
///
/// An achromatic base returns immediately, and that one IS load-bearing: its
/// chroma is zero, so `k` would be `0/0`.
///
/// The channels are brought back into gamut first, because HSL is defined only on
/// in-gamut sRGB and `a:lumOff` can push a channel past `255`. That clamp lives
/// inside this function rather than in the fold, so a colour carrying no
/// `a:satMod` folds bit-identically to how it folded before saturation existed.
///
/// Complexity: O(1).
fn modulate_saturation(rgb: &mut [f32; 3], factor: f32) {
    for channel in rgb.iter_mut() {
        *channel = channel.clamp(0.0, 255.0);
    }
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let chroma = max - min;
    if chroma <= 0.0 {
        return;
    }
    // `2L` in 0.0..=510.0, so the ceiling is integral arithmetic on the same scale
    // as the channels and needs no division.
    let double_lum = max + min;
    let chroma_ceiling = double_lum.min(510.0 - double_lum);
    let k = (chroma * factor.max(0.0)).min(chroma_ceiling) / chroma;
    let lum = double_lum / 2.0;
    for channel in rgb.iter_mut() {
        *channel = lum + (*channel - lum) * k;
    }
}

/// A colour inside a theme format-scheme entry.
///
/// `a:phClr` is a **formal parameter**, not a colour: the matrix entry says "fill
/// with whatever the referencing shape names", and the shape's `a:fillRef`/`a:lnRef`
/// supplies the argument. Modelling it as a distinct variant rather than resolving
/// it to a default at parse time is what makes a style reference reusable across
/// shapes of different accent colours — collapsing it would give every styled shape
/// in the document the same fill.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StyleColor {
    /// `a:phClr` — the referencing shape's own colour, with this entry's
    /// [`ColorTransform`] applied to it. The transform travels with the
    /// placeholder because its base does not exist until resolution.
    Placeholder(ColorTransform),
    /// A colour fixed by the theme itself, with any transform already folded in.
    Fixed(Rgba),
}

impl StyleColor {
    /// Resolves this colour, substituting `placeholder` for `a:phClr`.
    ///
    /// `None` when the entry names the placeholder and the reference supplied no
    /// colour: there is nothing to substitute, and inventing one would paint a
    /// colour the document never states.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn resolve(&self, placeholder: Option<Rgba>) -> Option<Rgba> {
        match self {
            Self::Fixed(color) => Some(*color),
            Self::Placeholder(transform) => placeholder.map(|base| transform.apply(base)),
        }
    }
}

/// One `a:gsLst/a:gs` of a theme gradient fill-style entry: a position plus a
/// colour that is independently fixed or the `a:phClr` placeholder.
///
/// Per-stop placeholder semantics are the whole point. The default Office theme
/// writes three `phClr` stops differing only in their transforms, while a branded
/// theme commonly mixes one `phClr` stop with a fixed one, and a model that put
/// the placeholder on the entry rather than on the stop could represent neither.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStyleStop {
    /// `a:gs@pos` in per-100000 units (`0` = start, `100000` = end).
    pub position: i32,
    /// The stop's colour.
    pub color: StyleColor,
}

/// A gradient `a:fillStyleLst` entry: its ordered stops and its geometry.
///
/// The geometry type is [`super::GradientKind`], reused from the shape fill this
/// resolves into rather than duplicated, so a theme gradient and an `a:gradFill`
/// authored on the shape travel the same paint path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStyle {
    /// The stops in document order; never empty when parsed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<GradientStyleStop>,
    /// `a:lin` (linear, with its angle) or `a:path` (radial).
    pub kind: GradientKind,
}

/// A pattern `a:fillStyleLst` entry (`a:pattFill`): a preset hatch name plus its
/// foreground and background colours.
///
/// Modeled so the loss can be NAMED — nothing paints it. There is no pattern
/// primitive in the display list, so a shape referencing one resolves to no fill
/// and the import report says a pattern style entry is why. Without the entry
/// kind in the model the report could only say "unmodeled", which is a finding a
/// caller cannot act on.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternStyle {
    /// `a:pattFill@prst` (e.g. `pct25`, `ltHorz`), as the file spells it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preset: String,
    /// `a:fgClr`'s colour.
    pub foreground: StyleColor,
    /// `a:bgClr`'s colour.
    pub background: StyleColor,
}

/// One `a:fillStyleLst` entry of the theme format scheme, as far as this build
/// models it.
///
/// A picture entry (`a:blipFill`) and a group entry (`a:grpFill`) are NOT guessed
/// at — the list keeps a `None` in its place so index arithmetic still lines up,
/// and a shape referencing one keeps today's behaviour and is reported.
///
/// [`FillStyle::Pattern`] is modeled but does not paint, for the same reason:
/// silently substituting a solid for a pattern would be worse than no fill,
/// because it looks deliberate. What modelling it buys is a report that says
/// *pattern* instead of *unknown*.
///
/// # Serialized forms
///
/// Written tagged (`{"kind": "solid", "color": …}`). Read in that form **and** in
/// the untagged `{"color": …}` form every snapshot carried while a fill style
/// could only be solid, because a stored draft, version or collaboration journal
/// written then must still open: refusing it would lose the reader's document over
/// a field nothing they did changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FillStyle {
    /// `a:solidFill`.
    Solid {
        /// The fill colour.
        color: StyleColor,
    },
    /// `a:gradFill`.
    Gradient(GradientStyle),
    /// `a:pattFill`. Resolves to no fill; see the type's documentation.
    Pattern(PatternStyle),
}

/// The current, tagged form of a [`FillStyle`].
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum FillStyleTagged {
    Solid { color: StyleColor },
    Gradient(GradientStyle),
    Pattern(PatternStyle),
}

/// Every form a [`FillStyle`] has been serialized in.
#[derive(Deserialize)]
#[serde(untagged)]
enum FillStyleWire {
    Tagged(FillStyleTagged),
    /// The solid-only form, before gradients and patterns were modelled.
    Solid {
        color: StyleColor,
    },
}

impl<'de> Deserialize<'de> for FillStyle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match FillStyleWire::deserialize(deserializer)? {
            FillStyleWire::Tagged(FillStyleTagged::Solid { color })
            | FillStyleWire::Solid { color } => Self::Solid { color },
            FillStyleWire::Tagged(FillStyleTagged::Gradient(gradient)) => Self::Gradient(gradient),
            FillStyleWire::Tagged(FillStyleTagged::Pattern(pattern)) => Self::Pattern(pattern),
        })
    }
}

/// One `a:lnStyleLst` entry of the theme format scheme: a width, a solid colour and
/// an optional preset dash.
///
/// Only a SOLID outline fill becomes an entry. An `a:ln` whose fill is a gradient
/// or a pattern leaves a `None` in its place, because [`super::ShapeStroke`] holds
/// one colour and nothing else: taking the gradient's first stop would draw a
/// confidently wrong outline, which is how this list used to behave.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStyle {
    /// `a:ln@w` in EMU; `0` means the theme states no width.
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub width_emu: i64,
    /// The outline colour.
    pub color: StyleColor,
    /// `a:prstDash@val`, when the entry declares one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<DashStyle>,
}

/// One `a:effectStyleLst` entry, modeled down to the ONE fact anything in this
/// build can act on: whether the entry carries any effect at all.
///
/// Nothing renders a DrawingML effect here. There is no shadow, glow, reflection
/// or soft-edge primitive in the display list, so modelling an `a:outerShdw`'s
/// blur radius, distance, direction and colour would add a construct no user
/// could reach — the single most expensive recurring mistake in this repository
/// (`SKILL` §9.4).
///
/// So the model carries the predicate the loss report needs and nothing more.
/// Resolving the entry is load-bearing even so, because an `a:effectStyleLst`
/// routinely mixes entries that carry effects with entries that are an empty
/// `a:effectLst` — the committed `themed-shape.docx` theme has exactly that shape,
/// entry 1 empty and entry 2 an `a:outerShdw`. So a build that reported on
/// `a:effectRef@idx != 0` alone would raise a finding for a shape that lost
/// nothing, while one that reports only when the RESOLVED entry carries effects
/// raises it exactly when a shadow was actually dropped.
///
/// How Word's own shipped themes populate that list is **not** verified here: no
/// Word-authored theme part exists in this repository, and the fixture is
/// synthetic. The mechanism above does not depend on it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectStyle {
    /// Whether the entry's `a:effectLst` holds at least one effect child.
    #[serde(default, skip_serializing_if = "is_false")]
    pub carries_effects: bool,
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The theme format scheme (`a:fmtScheme`) style lists, as far as they are modeled.
///
/// Positional by construction: `a:fillRef@idx` is **one-based** into
/// [`FormatScheme::fill_styles`], `0` means "no fill", and an index at or above
/// `1000` selects the background fill list — which this build does not model, so such
/// a reference resolves to nothing rather than to the wrong entry.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatScheme {
    /// `a:fillStyleLst` entries in order; `None` where the entry is not modeled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fill_styles: Vec<Option<FillStyle>>,
    /// `a:lnStyleLst` entries in order; `None` where the entry is not modeled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_styles: Vec<Option<LineStyle>>,
    /// `a:effectStyleLst` entries in order. Every entry is recognised — the model
    /// carries only whether it holds effects — so there is no `None` here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effect_styles: Vec<EffectStyle>,
}

/// Which per-script entry of a theme font collection a reference resolves
/// against (`a:latin`/`a:ea`/`a:cs`).
///
/// Public because two independent references pick an entry this way — a
/// `w:rFonts@*Theme` slot, which encodes the axis in the slot name, and an
/// `a:fontRef`, which does not and takes it from the run's script — and the
/// empty-entry rule below must not be written twice.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeFontAxis {
    /// `a:latin`, which is also the fallback for the other two.
    #[default]
    Latin,
    /// `a:ea`.
    EastAsia,
    /// `a:cs`.
    ComplexScript,
}

/// `a:fontRef@idx` (`ST_FontCollectionIndex`, ECMA-376 §20.1.10.25): which font
/// collection of the theme a shape's text takes its typeface from.
///
/// `none` is a real value and not an absence — it says "this shape's text takes
/// no theme typeface" — which is why it is a variant rather than being folded into
/// the `Option` around [`FontReference`]. An absent `a:fontRef` and an
/// `a:fontRef idx="none"` are different statements, and collapsing them would make
/// a round trip invent one from the other.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontCollectionIndex {
    /// `none`.
    #[default]
    None,
    /// `major` — the heading collection (`a:majorFont`).
    Major,
    /// `minor` — the body collection (`a:minorFont`).
    Minor,
}

impl FontCollectionIndex {
    /// The index an `a:fontRef@idx` token names.
    ///
    /// `None` for anything outside `ST_FontCollectionIndex`, which the caller
    /// reports rather than substituting a collection: guessing `minor` would give
    /// a shape's text the body typeface and look deliberate.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token.trim() {
            "none" => Self::None,
            "major" => Self::Major,
            "minor" => Self::Minor,
            _ => return None,
        })
    }

    /// The token this index is written as.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }
}

/// A shape's `a:fontRef`: the theme font collection its text takes its typeface
/// from, and the colour its text takes.
///
/// Both halves travel, because `a:fontRef` states both and a shape that kept only
/// one would paint a themed typeface in an unthemed colour or the reverse. The
/// colour is already resolved to a concrete [`Rgba`] when it reaches here, by the
/// same fold every other shape colour goes through — `a:fontRef`'s colour child is
/// the shape's own statement, not a theme placeholder, so there is nothing to
/// defer.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontReference {
    /// `a:fontRef@idx`.
    pub index: FontCollectionIndex,
    /// The colour `a:fontRef`'s child names, folded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgba>,
}

impl FontReference {
    /// The typeface this reference resolves to in `scheme`, for one script axis.
    ///
    /// `None` for `idx="none"` (which names no collection), and for a collection
    /// whose entry and Latin fallback are both the empty "no typeface" marker — so
    /// an unresolvable reference hands back nothing rather than an invented family.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn typeface<'a>(&self, scheme: &'a FontScheme, axis: ThemeFontAxis) -> Option<&'a str> {
        match self.index {
            FontCollectionIndex::None => None,
            FontCollectionIndex::Major => scheme.typeface(true, axis),
            FontCollectionIndex::Minor => scheme.typeface(false, axis),
        }
    }
}

/// A shape's theme style reference (`wps:style`): which format-scheme entry supplies
/// its fill and outline, and the colour each substitutes for `a:phClr`.
///
/// Held in a side table keyed by the shape's node id rather than as a field on
/// `GroupShape`. The reason is practical, not semantic — this IS authored content and
/// would sit naturally on the shape — but `GroupShape` has 23 literal construction
/// sites across six crates, and adding a field to it is a breaking change to every
/// one with nothing for a merge to conflict on (`SKILL` §5a shape 1). The `charts`
/// side table took the same route for the same reason.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeStyleRef {
    /// `a:fillRef@idx`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_idx: Option<u32>,
    /// The colour `a:fillRef` names, substituted for `a:phClr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<Rgba>,
    /// `a:lnRef@idx`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_idx: Option<u32>,
    /// The colour `a:lnRef` names, substituted for `a:phClr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_color: Option<Rgba>,
    /// `a:effectRef@idx`.
    ///
    /// Captured so the reference RESOLVES, not so it renders: nothing in this
    /// build paints a DrawingML effect (see [`EffectStyle`]). Its only consumer
    /// is the loss report, which needs the index to find out whether the entry
    /// the shape named actually carries an effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_idx: Option<u32>,
    /// `a:fontRef` — the theme font collection the shape's text takes, and the
    /// colour it takes.
    ///
    /// Captured for the reason [`ShapeStyleRef::effect_idx`] is: so the reference
    /// RESOLVES. Nothing in this build applies a shape-scoped text default, so a
    /// text box's runs still take their typeface from their own `w:rFonts` and the
    /// document's defaults — but the reference now resolves far enough for the
    /// compatibility report to say whether the shape lost a typeface, a colour, or
    /// nothing at all, which an uncaptured `a:fontRef` could not.
    ///
    /// Before this field the DOCX reader dropped `a:fontRef` with no finding at
    /// all, which is the silent loss `AGENTS.md` forbids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_ref: Option<FontReference>,
}

/// Maximum UTF-8 length of a drawing object's name or title.
pub const MAX_OBJECT_NAME_BYTES: usize = 1024;

/// A drawing object's non-visual properties: its author-visible identity
/// (`wp:docPr`, or a group child's `pic:cNvPr`/`wps:cNvPr`) — its NAME, the
/// handle Word's Selection Pane lists it by and an author renames it by, and its
/// TITLE, the short accessible label a screen reader announces beside the
/// `@descr` alt text (`docs/109` HF-267) — and its locks
/// ([`ObjectName::locks`], `109` FID-AT-09).
///
/// A side table keyed by the object's node id, for the reason [`ShapeStyleRef`]
/// gives: the drawing types are constructed by literal across six crates, and a
/// new field on each is a breaking change to every literal with nothing for a
/// merge to conflict on (`SKILL` §5a shape 1).
///
/// The cost of the side table is the one it always has: an object duplicated by
/// an edit gets a new id and starts unnamed, and Word then names it on save the
/// way it names any new object. Recorded rather than hidden.
///
/// # Two statements of one object's name
///
/// A lone picture or text box is named twice: once on its frame (`wp:docPr`)
/// and once on the object inside the frame (`pic:cNvPr`, `wps:cNvPr`). Word
/// writes the same name in both; other producers do not — python-docx, which
/// generates a great many real documents, names the frame `Picture 1` and the
/// picture after the image FILE (`diagram.png`). The inner name is what a
/// reader of the picture's own properties sees, so it is kept as
/// [`ObjectName::inner_name`] / [`ObjectName::inner_title`] whenever it
/// DIFFERS from the frame's, and written back to the inner element (`109`
/// FID-AT-08). Equal to the frame's — Word's case — it is the empty state: the
/// writer puts the frame's name on both elements, so storing it would change
/// nothing and would stop write-then-reopen being a fixed point.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectName {
    /// `@name` — "Picture 3", "Text Box 7", or whatever the author typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `@title` — the accessible title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The inner element's `@name` (`pic:cNvPr`, `wps:cNvPr`) where it differs
    /// from the frame's (`wp:docPr`) — see the type's documentation. `None`
    /// means "the same as the frame's", which is how the writer emits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_name: Option<String>,
    /// The inner element's `@title` where it differs from the frame's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_title: Option<String>,
    /// The object's DrawingML locks (`a:graphicFrameLocks` on the frame,
    /// `a:picLocks`/`a:spLocks`/`a:grpSpLocks` on the object) — the other half
    /// of the same non-visual properties the name belongs to, kept in this
    /// side table so they ride the plumbing the name already has (`109`
    /// FID-AT-09). Read [`Definitions::locks_aspect_ratio`] for the one a
    /// resize handle needs.
    ///
    /// [`Definitions::locks_aspect_ratio`]: super::Definitions::locks_aspect_ratio
    #[serde(default, skip_serializing_if = "ObjectLocks::is_empty")]
    pub locks: ObjectLocks,
}

/// One DrawingML lock element's flags (`CT_GraphicalObjectFrameLocking`,
/// `CT_PictureLocking`, `CT_ShapeLocking`, `CT_GroupLocking`): each is a
/// restriction the author asked an editor to honour.
///
/// The union of the four elements' attributes, because the model stores what
/// the source said and the writer decides which of them the element it writes
/// can carry — a flag that element's schema does not list is not written.
/// `false` is the absent attribute: `noChangeAspect="0"` states nothing an
/// absent one does not.
#[allow(clippy::struct_excessive_bools)] // one bool per schema attribute, by design
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockFlags {
    /// `@noGrp` — the object may not be grouped.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_grp: bool,
    /// `@noUngrp` — the group may not be ungrouped (groups only).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_ungrp: bool,
    /// `@noDrilldown` — the frame's contents may not be selected (frames only).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_drilldown: bool,
    /// `@noSelect` — the object may not be selected.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_select: bool,
    /// `@noRot` — the object may not be rotated.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_rot: bool,
    /// `@noChangeAspect` — a resize must keep the aspect ratio. Word writes it on
    /// every picture it inserts; it is what makes a corner drag proportional.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_change_aspect: bool,
    /// `@noMove` — the object may not be moved.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_move: bool,
    /// `@noResize` — the object may not be resized.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_resize: bool,
    /// `@noEditPoints` — the geometry's points may not be edited.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_edit_points: bool,
    /// `@noAdjustHandles` — the adjust handles may not be dragged.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_adjust_handles: bool,
    /// `@noChangeArrowheads` — the arrowheads may not be changed.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_change_arrowheads: bool,
    /// `@noChangeShapeType` — the preset may not be changed.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_change_shape_type: bool,
    /// `@noTextEdit` — the shape's text may not be edited (shapes only).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_text_edit: bool,
    /// `@noCrop` — the picture may not be cropped (pictures only).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_crop: bool,
}

impl LockFlags {
    /// Whether no lock is set — an element that locks nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every flag with its schema attribute name, in schema order — the one
    /// table the importer reads by and the writer writes by.
    #[must_use]
    pub fn attributes(&self) -> [(&'static str, bool); 14] {
        [
            ("noGrp", self.no_grp),
            ("noUngrp", self.no_ungrp),
            ("noDrilldown", self.no_drilldown),
            ("noSelect", self.no_select),
            ("noRot", self.no_rot),
            ("noChangeAspect", self.no_change_aspect),
            ("noMove", self.no_move),
            ("noResize", self.no_resize),
            ("noEditPoints", self.no_edit_points),
            ("noAdjustHandles", self.no_adjust_handles),
            ("noChangeArrowheads", self.no_change_arrowheads),
            ("noChangeShapeType", self.no_change_shape_type),
            ("noTextEdit", self.no_text_edit),
            ("noCrop", self.no_crop),
        ]
    }

    /// The flag a schema attribute name sets, for the importer. `None` for a
    /// name that is not a lock.
    pub fn flag_mut(&mut self, attribute: &[u8]) -> Option<&mut bool> {
        Some(match attribute {
            b"noGrp" => &mut self.no_grp,
            b"noUngrp" => &mut self.no_ungrp,
            b"noDrilldown" => &mut self.no_drilldown,
            b"noSelect" => &mut self.no_select,
            b"noRot" => &mut self.no_rot,
            b"noChangeAspect" => &mut self.no_change_aspect,
            b"noMove" => &mut self.no_move,
            b"noResize" => &mut self.no_resize,
            b"noEditPoints" => &mut self.no_edit_points,
            b"noAdjustHandles" => &mut self.no_adjust_handles,
            b"noChangeArrowheads" => &mut self.no_change_arrowheads,
            b"noChangeShapeType" => &mut self.no_change_shape_type,
            b"noTextEdit" => &mut self.no_text_edit,
            b"noCrop" => &mut self.no_crop,
            _ => return None,
        })
    }

    /// `self` with every flag `other` sets also set.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let mut merged = self;
        for (name, on) in other.attributes() {
            if on && let Some(flag) = merged.flag_mut(name.as_bytes()) {
                *flag = true;
            }
        }
        merged
    }
}

/// The four DrawingML lock elements, each a schema type with its own set of
/// attributes — the one table the importer reads a lock element by and the
/// writer writes one by (`109` FID-AT-09).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockElement {
    /// `a:graphicFrameLocks` (`CT_GraphicalObjectFrameLocking`), on a frame.
    Frame,
    /// `a:picLocks` (`CT_PictureLocking`), on a picture.
    Picture,
    /// `a:spLocks` (`CT_ShapeLocking`), on a shape or text box.
    Shape,
    /// `a:grpSpLocks` (`CT_GroupLocking`), on a group.
    Group,
}

impl LockElement {
    /// The lock element a local element name is, if it is one.
    #[must_use]
    pub fn from_local_name(local: &[u8]) -> Option<Self> {
        match local {
            b"graphicFrameLocks" => Some(Self::Frame),
            b"picLocks" => Some(Self::Picture),
            b"spLocks" => Some(Self::Shape),
            b"grpSpLocks" => Some(Self::Group),
            _ => None,
        }
    }

    /// The element's qualified name as the writer emits it.
    #[must_use]
    pub const fn qualified_name(self) -> &'static str {
        match self {
            Self::Frame => "a:graphicFrameLocks",
            Self::Picture => "a:picLocks",
            Self::Shape => "a:spLocks",
            Self::Group => "a:grpSpLocks",
        }
    }

    /// Whether `attribute` is one of this element's own locks in its schema
    /// type. Every type has `noGrp`, `noSelect`, `noChangeAspect`, `noMove` and
    /// `noResize`; the rest are per type.
    #[must_use]
    pub fn carries(self, attribute: &[u8]) -> bool {
        let common = matches!(
            attribute,
            b"noGrp" | b"noSelect" | b"noChangeAspect" | b"noMove" | b"noResize"
        );
        let shape_like = matches!(
            attribute,
            b"noRot"
                | b"noEditPoints"
                | b"noAdjustHandles"
                | b"noChangeArrowheads"
                | b"noChangeShapeType"
        );
        common
            || match self {
                Self::Frame => attribute == b"noDrilldown",
                Self::Picture => shape_like || attribute == b"noCrop",
                Self::Shape => shape_like || attribute == b"noTextEdit",
                Self::Group => matches!(attribute, b"noUngrp" | b"noRot"),
            }
    }
}

/// A drawing object's locks: the frame's and the object's own (`109`
/// FID-AT-09).
///
/// Two sets because DrawingML states them on two elements and Word writes
/// both: `wp:cNvGraphicFramePr/a:graphicFrameLocks` on the frame around every
/// inline or floating object, and `a:picLocks` (`pic:cNvPicPr`), `a:spLocks`
/// (`wps:cNvSpPr`) or `a:grpSpLocks` (`wpg:cNvGrpSpPr`) on the object inside
/// it. A group child has no frame, so only `object` is ever set for one.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectLocks {
    /// `a:graphicFrameLocks` — the frame's locks.
    #[serde(default, skip_serializing_if = "LockFlags::is_empty")]
    pub frame: LockFlags,
    /// `a:picLocks` / `a:spLocks` / `a:grpSpLocks` — the object's own.
    #[serde(default, skip_serializing_if = "LockFlags::is_empty")]
    pub object: LockFlags,
}

impl ObjectLocks {
    /// Whether neither set locks anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frame.is_empty() && self.object.is_empty()
    }

    /// Whether a resize must keep the object's aspect ratio: `noChangeAspect`
    /// on the frame OR on the object. Word sets it on both for a picture it
    /// inserts, and honours either.
    #[must_use]
    pub const fn locks_aspect_ratio(&self) -> bool {
        self.frame.no_change_aspect || self.object.no_change_aspect
    }
}

impl ObjectName {
    /// The name a writer gives a picture that has no modelled name — Word's own
    /// default for the first picture in a document.
    pub const GENERIC_PICTURE: &'static str = "Picture 1";
    /// The `wp:docPr` name for an unnamed group.
    pub const GENERIC_GROUP: &'static str = "Group 1";
    /// The `wp:docPr` name for an unnamed chart, diagram or other object.
    pub const GENERIC_OBJECT: &'static str = "Object 1";
    /// The `wp:docPr` name for an unnamed text box.
    pub const GENERIC_TEXT_BOX: &'static str = "Text Box 1";
    /// The `wps:cNvPr` name for an unnamed shape inside a group.
    pub const GENERIC_SHAPE: &'static str = "Shape";
    /// The `wps:cNvPr` name for an unnamed text box inside a group.
    pub const GENERIC_CHILD_TEXT_BOX: &'static str = "Text Box";
    /// The `wpg:cNvPr` name for an unnamed nested group.
    pub const GENERIC_CHILD_GROUP: &'static str = "Group";

    /// Whether no part is set — an entry that says nothing and is not kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.title.is_none()
            && self.inner_name.is_none()
            && self.inner_title.is_none()
            && self.locks.is_empty()
    }

    /// `self` with a name equal to `generic` dropped.
    ///
    /// The writer names an object that has no modelled name with the `GENERIC_*`
    /// name for its kind, so that name IS the model's empty state, written out —
    /// `docs/35`'s no-op rule, the same reason an absent `@wrapText` is not
    /// normalized to `bothSides`. Keeping it would make a document with no names
    /// grow one on every save and stop being a fixed point of write → reopen;
    /// dropping it loses nothing, because the writer puts the same string back.
    #[must_use]
    pub fn without_generic(mut self, generic: &str) -> Self {
        if self.name.as_deref() == Some(generic) {
            self.name = None;
        }
        self
    }
}

impl FormatScheme {
    /// The modeled fill style a one-based `a:fillRef@idx` selects.
    ///
    /// `None` for index `0` ("no fill"), for an index past the list, and for an
    /// index at or above `1000` (the background fill list, not modeled) — so a
    /// reference this build cannot honour resolves to nothing rather than to a
    /// neighbouring entry that happens to exist.
    ///
    /// Borrowed rather than copied: a gradient entry owns its stops.
    #[must_use]
    pub fn fill_style(&self, idx: u32) -> Option<&FillStyle> {
        Self::slot(&self.fill_styles, idx)?.as_ref()
    }

    /// The modeled line style a one-based `a:lnRef@idx` selects, with the same
    /// index rules as [`FormatScheme::fill_style`].
    #[must_use]
    pub fn line_style(&self, idx: u32) -> Option<LineStyle> {
        Self::slot(&self.line_styles, idx).copied().flatten()
    }

    /// The effect style a one-based `a:effectRef@idx` selects, with the same index
    /// rules as [`FormatScheme::fill_style`].
    ///
    /// Resolving this is what keeps the effect loss report honest rather than
    /// noisy — see [`EffectStyle`] for why an index alone is not enough.
    #[must_use]
    pub fn effect_style(&self, idx: u32) -> Option<EffectStyle> {
        if idx == 0 || idx >= 1000 {
            return None;
        }
        self.effect_styles
            .get(usize::try_from(idx).ok()? - 1)
            .copied()
    }

    /// The list slot a one-based index selects, enforcing the index rules once.
    fn slot<T>(list: &[Option<T>], idx: u32) -> Option<&Option<T>> {
        if idx == 0 || idx >= 1000 {
            return None;
        }
        list.get(usize::try_from(idx).ok()? - 1)
    }
}

/// The theme color scheme (`theme1.xml` `a:clrScheme`, ECMA-376 §20.1.6.2): the
/// scheme name and the twelve named color slots that `w:themeColor` references
/// resolve against. Slot order matches the OOXML child order.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorScheme {
    /// The scheme name (`@name`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Dark 1 (`a:dk1`).
    pub dark1: SchemeColor,
    /// Light 1 (`a:lt1`).
    pub light1: SchemeColor,
    /// Dark 2 (`a:dk2`).
    pub dark2: SchemeColor,
    /// Light 2 (`a:lt2`).
    pub light2: SchemeColor,
    /// Accent 1 (`a:accent1`).
    pub accent1: SchemeColor,
    /// Accent 2 (`a:accent2`).
    pub accent2: SchemeColor,
    /// Accent 3 (`a:accent3`).
    pub accent3: SchemeColor,
    /// Accent 4 (`a:accent4`).
    pub accent4: SchemeColor,
    /// Accent 5 (`a:accent5`).
    pub accent5: SchemeColor,
    /// Accent 6 (`a:accent6`).
    pub accent6: SchemeColor,
    /// Hyperlink (`a:hlink`).
    pub hyperlink: SchemeColor,
    /// Followed hyperlink (`a:folHlink`).
    pub followed_hyperlink: SchemeColor,
}

/// One theme part (`theme1.xml`) other than the document's own.
///
/// # Why a side table and not a wider `Definitions`
///
/// A WordprocessingML package has **exactly one** theme, so
/// [`Definitions::color_scheme`](super::Definitions::color_scheme) and its three
/// siblings are flat fields and always will be: that is the document's theme, it
/// needs no key, and reading it costs a borrow. A `.pptx` may carry one theme
/// part **per slide master**, which a single triple cannot represent — the first
/// master's theme wins and every other is a loss.
///
/// This is the pattern `Definitions` already uses five times over — `media`,
/// `charts`, `shape_styles`, `shape_fill_detail` and `field_ranges` are all
/// `DefinitionMap<Id, Value>` side tables whose holder carries the key — composed
/// with the default-plus-overrides shape of `document_defaults` and the per-style
/// overrides above it. The format itself is built the same way: `p:clrMap` on the
/// master with `p:clrMapOvr` on the layout and the slide. Nothing here is new,
/// and naming the prior art is the point (`SKILL` §8).
///
/// The one mechanism for asking *which theme is in force* is
/// [`Definitions::theme`](super::Definitions::theme), which hands back a
/// [`ThemeView`] whether the answer is the document's own theme or a keyed entry.
/// A consumer that goes through it cannot tell a single-theme document from a
/// multi-theme deck, and a single-theme document pays nothing: the table is empty,
/// it is omitted from the snapshot entirely, and the view is four borrows.
///
/// The line is resolution, not access. Layout resolves through the accessor; the
/// importer that fills the fields and the writer that serializes them touch them
/// directly, because they are the storage's own reader and writer rather than
/// consumers asking a question.
///
/// # What is deliberately NOT here
///
/// The theme's display name (`a:theme@name`). The document's own theme has no
/// field for it either, and giving the table one would make an entry a *richer*
/// model of the same part than the primary — which is how two mechanisms grow
/// back. It stays an unmodelled loss, reported on both paths, until there is a
/// field on both sides.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Theme {
    /// This theme's `a:clrScheme`, matching
    /// [`Definitions::color_scheme`](super::Definitions::color_scheme).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<ColorScheme>,
    /// This theme's `a:fontScheme`, matching
    /// [`Definitions::font_scheme`](super::Definitions::font_scheme).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scheme: Option<FontScheme>,
    /// The modelled subset of this theme's `a:fmtScheme`, matching
    /// [`Definitions::format_scheme`](super::Definitions::format_scheme).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_scheme: Option<FormatScheme>,
    /// This theme's `a:fmtScheme` retained verbatim, matching
    /// [`Definitions::format_scheme_xml`](super::Definitions::format_scheme_xml).
    ///
    /// Two representations of one part, with the same strict division as on the
    /// document's own theme: the verbatim XML is what a writer emits, the typed
    /// form is what a `a:fillRef`/`a:lnRef` resolves against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_scheme_xml: Option<String>,
}

/// A theme resolved for one holder: the document's own, or one keyed entry.
///
/// Borrowed rather than cloned, because the alternative is copying a format
/// scheme — gradient stops and all — on every lookup, and the lookups sit on the
/// painting path. `Copy`, so it can be threaded through a resolution without
/// re-borrowing `Definitions`.
///
/// Every member is an `Option` for the same reason the flat fields are: a theme
/// part may state any subset of the three schemes, and a package may have no
/// theme at all.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ThemeView<'a> {
    /// The `a:clrScheme` in force.
    pub color_scheme: Option<&'a ColorScheme>,
    /// The `a:fontScheme` in force.
    pub font_scheme: Option<&'a FontScheme>,
    /// The modelled subset of the `a:fmtScheme` in force.
    pub format_scheme: Option<&'a FormatScheme>,
    /// The verbatim `a:fmtScheme` in force.
    pub format_scheme_xml: Option<&'a str>,
}

impl<'a> ThemeView<'a> {
    /// The view of one keyed [`Theme`].
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn of(theme: &'a Theme) -> Self {
        Self {
            color_scheme: theme.color_scheme.as_ref(),
            font_scheme: theme.font_scheme.as_ref(),
            format_scheme: theme.format_scheme.as_ref(),
            format_scheme_xml: theme.format_scheme_xml.as_deref(),
        }
    }

    /// Whether this view states nothing at all — no package theme, or a theme part
    /// that modelled none of its three schemes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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

/// The line-spacing rule (`w:spacing@w:lineRule`, `ST_LineSpacingRule`): how the
/// `w:line` value is interpreted (ECMA-376 §17.3.1.33).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineRule {
    /// `auto` — `w:line` is a multiple of single spacing measured in 240ths
    /// (240 = single). Modeled as [`Spacing::line_percent`].
    Auto,
    /// `atLeast` — the line is at least `w:line` twips tall; taller content grows
    /// it. `w:line` is stored in [`Spacing::line_twips`].
    AtLeast,
    /// `exact` — the line is exactly `w:line` twips tall; content is clipped.
    /// `w:line` is stored in [`Spacing::line_twips`].
    Exact,
}

/// Paragraph spacing.
///
/// Line spacing is modeled in two complementary shapes so the common
/// `lineRule="auto"` case stays byte-stable while the exact/atLeast rules are
/// represented faithfully:
/// - `auto` (a multiple of single spacing) rides [`Spacing::line_percent`] with
///   `line_rule` left `None` (the implicit default).
/// - `atLeast`/`exact` set [`Spacing::line_rule`] and carry the twip value in
///   [`Spacing::line_twips`].
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spacing {
    /// Space before, in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_twips: Option<i32>,
    /// Space after, in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_twips: Option<i32>,
    /// Line spacing as a percentage (100 = single) for the `auto` rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_percent: Option<u16>,
    /// The line-spacing rule (`w:lineRule`). Only `Some` for `atLeast`/`exact`;
    /// the `auto` rule leaves this `None` and uses `line_percent`. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_rule: Option<LineRule>,
    /// The `w:line` value in twips for the `atLeast`/`exact` rules. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_twips: Option<i32>,
    /// Automatically determine space before (`w:beforeAutospacing`): when `true`,
    /// `before_twips` is ignored and a font-size-derived default is used. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_auto: Option<bool>,
    /// Automatically determine space after (`w:afterAutospacing`): when `true`,
    /// `after_twips` is ignored and a font-size-derived default is used. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_auto: Option<bool>,
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

/// How a paragraph frame carries a drop capital (`w:framePr@dropCap`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropCapMode {
    /// The initial occupies the leading edge of the text column.
    Drop,
    /// The initial sits in the leading page margin.
    Margin,
}

/// Text wrapping requested by a drop-cap paragraph frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameWrap {
    /// Wrap text around the frame.
    Around,
    /// Do not place text beside the frame.
    NotBeside,
    /// Let the consumer select the wrapping behavior.
    Auto,
    /// Disable wrapping.
    None,
}

/// Horizontal reference box of a drop-cap paragraph frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameHorizontalAnchor {
    /// Page margin box.
    Margin,
    /// Whole page box.
    Page,
    /// Current text column.
    Text,
}

/// Vertical reference box of a drop-cap paragraph frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameVerticalAnchor {
    /// Page margin box.
    Margin,
    /// Whole page box.
    Page,
    /// Current text region.
    Text,
}

/// Named horizontal alignment of a drop-cap paragraph frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameHorizontalAlignment {
    /// Center in the reference box.
    Center,
    /// Facing-page inside edge.
    Inside,
    /// Physical left edge.
    Left,
    /// Facing-page outside edge.
    Outside,
    /// Physical right edge.
    Right,
}

/// Named vertical alignment of a drop-cap paragraph frame.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameVerticalAlignment {
    /// Bottom of the reference box.
    Bottom,
    /// Center in the reference box.
    Center,
    /// Inline with the surrounding text.
    Inline,
    /// Facing-page inside edge.
    Inside,
    /// Facing-page outside edge.
    Outside,
    /// Top of the reference box.
    Top,
}

/// The bounded `w:framePr` surface used by a drop-cap paragraph.
///
/// Generic paragraph frames (`dropCap="none"` or no drop-cap mode) remain
/// compatibility-retained and reported by import rather than being
/// misrepresented as drop caps.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DropCapFrame {
    /// Whether the initial is in-column or in-margin.
    pub mode: DropCapMode,
    /// Number of following body lines occupied by the initial (`1..=255`).
    pub lines: u8,
    /// Requested text wrap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<FrameWrap>,
    /// Horizontal reference box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_anchor: Option<FrameHorizontalAnchor>,
    /// Vertical reference box.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_anchor: Option<FrameVerticalAnchor>,
    /// Named horizontal alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_alignment: Option<FrameHorizontalAlignment>,
    /// Named vertical alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<FrameVerticalAlignment>,
    /// Explicit horizontal position in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_position_twips: Option<i32>,
    /// Explicit vertical position in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_position_twips: Option<i32>,
    /// Horizontal clearance from adjacent text in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal_space_twips: Option<u32>,
    /// Vertical clearance from adjacent text in twips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_space_twips: Option<u32>,
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
    /// Numbering explicitly REMOVED at this layer (`w:numPr/w:numId w:val="0"`).
    ///
    /// ECMA-376 §17.9.18 reserves `numId` 0 as "this paragraph is not numbered":
    /// it is not a reference to a list, it is the idiom a style or a paragraph
    /// uses to *cancel* a numbering reference inherited from its `w:basedOn`
    /// chain. Word's own `TOC Heading` style is the canonical instance — it is
    /// based on `Heading 1`, which carries `numId="1"`, and cancels it this way.
    ///
    /// This is the tri-state an inherited property needs: `numbering: None` with
    /// this flag clear means "unset, inherit"; this flag set means "off, and do
    /// not inherit". Without it a cancellation is indistinguishable from silence
    /// and the inherited list marker survives into a paragraph Word leaves
    /// unnumbered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub numbering_none: bool,
    /// Alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Indentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indentation: Option<Indentation>,
    /// Spacing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<Spacing>,
    /// Drop-cap paragraph frame (`w:framePr` with `dropCap=drop|margin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_cap_frame: Option<DropCapFrame>,
    /// Keep this paragraph on the same page as the next (`w:keepNext`).
    ///
    /// Tri-state, for the reason spelled out on [`Self::contextual_spacing`]:
    /// `None` is "absent, inherit", `Some(false)` is an explicit `w:val="0"`
    /// that CANCELS an inherited value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_next: Option<bool>,
    /// Keep all lines of this paragraph on one page (`w:keepLines`). Tri-state —
    /// see [`Self::contextual_spacing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_lines: Option<bool>,
    /// Force a page break before this paragraph (`w:pageBreakBefore`).
    /// Tri-state — see [`Self::contextual_spacing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_break_before: Option<bool>,
    /// First/last-line widow/orphan control (`w:widowControl`). Tri-state: the
    /// OOXML default is ON, so `None` means "unset, control is on" and `Some(false)`
    /// is an explicit off that must survive the cascade (a plain `bool` defaulting
    /// `false` would strand lines in every document that omits the element). The
    /// effective flag is resolved with `.unwrap_or(true)` at the layout boundary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widow_control: Option<bool>,
    /// Do not add spacing between paragraphs of the same style
    /// (`w:contextualSpacing`).
    ///
    /// Tri-state, and this field is the reason the whole `CT_OnOff` family is.
    /// ECMA-376 §17.17.4 makes `w:val="0"`/`"false"`/`"off"` an explicit OFF,
    /// which is a different statement from the element being absent: absent
    /// means "inherit whatever the style chain said", off means "cancel it
    /// here". A plain `bool` collapses the two, so the cascade can only ever OR
    /// the flag on and a cancellation is unrepresentable.
    ///
    /// This is not theoretical. Word's built-in `ListParagraph` style sets
    /// `w:contextualSpacing` ON, and a document that wants real gaps between
    /// list items cancels it per paragraph — the owner's NDA does exactly that
    /// **51 times**, each beside a `w:spacing w:after="160"` that our OR-ing
    /// cascade then suppressed. The paint lost 8pt of space in 51 places while
    /// every model-level test passed, which is the same shape as the
    /// [`Self::numbering_none`] bug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contextual_spacing: Option<bool>,
    /// Suppress line numbers for this paragraph (`w:suppressLineNumbers`).
    /// Tri-state — see [`Self::contextual_spacing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppress_line_numbers: Option<bool>,
    /// Outline (heading) level, `0..=9` (`w:outlineLvl`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_level: Option<u8>,
    /// This heading is saved **folded**: `w15:collapsed`, the `CT_OnOff` element
    /// Word writes in a heading's `w:pPr` in the Office 2012 namespace
    /// `http://schemas.microsoft.com/office/word/2012/wordml`.
    ///
    /// Microsoft's Open Specifications define it as: *"When a collapsed element is
    /// added to a paragraph (pPr) of a particular heading level and its value is
    /// true/on/1, immediately subsequent paragraphs with a higher heading level
    /// number appear collapsed when the document is opened."* So the flag sits on
    /// the heading and its SCOPE is everything after it up to the next heading of
    /// the same or a lower outline-level number. That scope is derived from the
    /// outline, never stored — storing it would duplicate a fact the heading tree
    /// already carries and let the two disagree after an edit.
    ///
    /// Tri-state, for the reason spelled out at length on
    /// [`Self::contextual_spacing`]: `None` is "absent, inherit", `Some(true)` is
    /// folded, and `Some(false)` is an explicit `w:val="0"` that CANCELS a fold a
    /// style chain contributed. A plain `bool` would make the cancellation
    /// unrepresentable and silently re-fold a section the document had unfolded.
    ///
    /// **This is the saved document DEFAULT, not a viewer's live fold state.** The
    /// split is Google's and it is the only model that can honour a Word file: an
    /// editor changes the default for everyone, while a reader's own folding is
    /// per-viewer and is never written back into the file. ADR-049 decides this;
    /// `docs/157` records the evidence. The per-viewer half lives in the shell, not
    /// here.
    ///
    /// **It is never an access control.** Whatever this says, the content is in the
    /// file, in every export, in find, and in the accessibility mirror.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    /// Paragraph borders (`w:pBdr`). Stored out of line — see
    /// [`BoxedParagraphBorders`] — because almost no paragraph has any; it reads
    /// and writes as a plain [`ParagraphBorders`].
    #[serde(default, skip_serializing_if = "BoxedParagraphBorders::is_empty")]
    pub borders: BoxedParagraphBorders,
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
    /// Right-to-left paragraph (`w:bidi`). Tri-state: several of these toggles
    /// default ON in OOXML, so an explicit `w:val="0"` (off) must be preserved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidi: Option<bool>,
    /// Break within words for East-Asian text (`w:wordWrap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_wrap: Option<bool>,
    /// Apply East-Asian line-break (kinsoku) rules (`w:kinsoku`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kinsoku: Option<bool>,
    /// Snap lines to the document grid (`w:snapToGrid`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_to_grid: Option<bool>,
    /// Mirror indents on facing pages (`w:mirrorIndents`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_indents: Option<bool>,
    /// Adjust right indent for a document grid (`w:adjustRightInd`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjust_right_ind: Option<bool>,
    /// Automatically hyphenate (`w:suppressAutoHyphens` inverted: `true` here
    /// means hyphenation is suppressed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_auto_hyphens: Option<bool>,
    /// Allow punctuation to overflow the text boundary (`w:overflowPunct`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overflow_punct: Option<bool>,
    /// Allow the first line's leading punctuation to compress (`w:topLinePunct`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_line_punct: Option<bool>,
    /// Auto-space between East-Asian and Latin text (`w:autoSpaceDE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_space_de: Option<bool>,
    /// Auto-space between East-Asian text and numbers (`w:autoSpaceDN`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_space_dn: Option<bool>,
    /// Vertical alignment of text on the line (`w:textAlignment`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_alignment: Option<VerticalTextAlignment>,
    /// Paragraph text-flow direction (`w:textDirection`), reusing the same
    /// `TextDirection` vocabulary as sections and table cells. Additive, omitted
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_direction: Option<TextDirection>,
    /// Formatting of the paragraph mark itself (`w:pPr > w:rPr`) — the run
    /// properties applied to the end-of-paragraph glyph. `Some` (even when
    /// default) means the `w:rPr` was present; additive, omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_run: Option<Box<RunProperties>>,
    /// Paragraph-properties format-change revision (`w:pPrChange`): the prior
    /// paragraph properties plus author/date/id. Additive, omitted when absent;
    /// re-emitted as the last child of `w:pPr`.
    ///
    /// Boxed (like `mark_run`) because only a tracked format change carries one:
    /// absent it costs a pointer instead of 112 bytes on every paragraph in the
    /// document (`docs/111`). `Option<Box<T>>` serializes as `Option<T>` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_change: Option<Box<PropChange<ParagraphProperties>>>,
    /// Tracked insertion/deletion of the paragraph mark itself
    /// (`w:pPr > w:rPr > w:ins` / `w:del`). Additive, omitted when absent;
    /// re-emitted as the first child of the mark's `w:rPr`.
    ///
    /// Boxed for the same reason as `prop_change`: only a tracked paragraph mark
    /// carries one, and 80 bytes per paragraph is not worth paying for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_revision: Option<Box<MarkRevision>>,
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

/// The line style of an underline (`w:u@val`, `ST_Underline`). A closed
/// vocabulary; an unrecognized producer token maps to [`Single`](Self::Single)
/// and is reported (it still underlines, just not with the exact art style). The
/// on/off state lives in [`RunProperties::underline`]; this only describes *how*
/// the line is drawn when on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnderlineStyle {
    /// A single line (`single`) — the default.
    #[default]
    Single,
    /// Two parallel lines (`double`).
    Double,
    /// A single thick line (`thick`).
    Thick,
    /// A dotted line (`dotted`, `dottedHeavy`).
    Dotted,
    /// A dashed line (`dash`, `dashedHeavy`, `dashLong`, `dashLongHeavy`).
    Dashed,
    /// A dot-dash line (`dotDash`, `dashDotHeavy`, `dotDotDash`, `dashDotDotHeavy`).
    DotDash,
    /// A wavy line (`wave`, `wavyHeavy`, `wavyDouble`).
    Wavy,
    /// Underline words only, not the spaces between them (`words`).
    Words,
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
    /// Complex-script bold (`w:bCs`): the bold toggle applied to complex-script
    /// (bidirectional) runs — Arabic/Hebrew/Thai text — independent of the Latin
    /// `bold` above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold_complex: Option<bool>,
    /// Italic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Complex-script italic (`w:iCs`): the italic toggle applied to
    /// complex-script (bidirectional) runs, independent of the Latin `italic`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic_complex: Option<bool>,
    /// Underline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Explicit underline color (`w:u@color`), sRGB. `None` (the common case, or
    /// `auto`) means the underline takes the run's text color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline_color: Option<RgbColor>,
    /// Underline line style (`w:u@val`, e.g. `double`/`wave`). `None` means the
    /// default single line; only meaningful when `underline` is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline_style: Option<UnderlineStyle>,
    /// Strike-through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Font size in half-points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_half_points: Option<u32>,
    /// Complex-script font size in half-points (`w:szCs`): the size applied to
    /// complex-script (bidirectional) runs, independent of `size_half_points`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_complex_half_points: Option<u32>,
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
    /// Do not check spelling/grammar for this run (`w:noProof`). Common on code,
    /// URLs, product names, and generated fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proof: Option<bool>,
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
    /// Horizontal character scaling percentage (`w:w` / `ST_TextScale`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_scale_percent: Option<u16>,
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
    /// Run-properties format-change revision (`w:rPrChange`): the prior run
    /// properties plus author/date/id. Additive, omitted when absent; re-emitted
    /// as the last child of `w:rPr`.
    ///
    /// Boxed because only a tracked format change carries one, and every run in
    /// the document paid 112 bytes for the empty case (`docs/111`).
    /// `Option<Box<T>>` serializes as `Option<T>` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_change: Option<Box<PropChange<RunProperties>>>,
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
