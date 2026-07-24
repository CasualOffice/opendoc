//! Style, numbering, section, theme, and media definition tables.

use serde::{Deserialize, Serialize};

use super::{
    AbstractNumberingId, DefinitionMap, FontName, MediaId, NumberingInstanceId,
    ParagraphProperties, RunProperties, SectionId, StyleId, StyleKind,
};

/// A style definition (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    /// Style kind.
    pub kind: StyleKind,
    /// Inherited style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on: Option<StyleId>,
    /// Paragraph property overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Run property overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
}

/// Document-wide default properties.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentDefaults {
    /// Default paragraph properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Default run properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
}

/// One abstract numbering level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingLevel {
    /// Level index.
    pub level: u8,
    /// Starting value.
    pub start: u16,
    /// Optional character style reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
}

/// An abstract numbering definition (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbstractNumbering {
    /// Ordered levels.
    pub levels: Vec<NumberingLevel>,
}

/// A per-instance numbering level override.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingOverride {
    /// Level index.
    pub level: u8,
    /// Overriding start value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u16>,
}

/// A numbering instance (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingInstance {
    /// The abstract definition this instance uses.
    pub abstract_ref: AbstractNumberingId,
    /// Per-level overrides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<NumberingOverride>,
}

/// Page size in twips.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSize {
    /// Width in twips.
    pub width_twips: i32,
    /// Height in twips.
    pub height_twips: i32,
}

/// Page margins in twips.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageMargins {
    /// Top margin.
    pub top_twips: i32,
    /// Bottom margin.
    pub bottom_twips: i32,
    /// Leading margin.
    pub start_twips: i32,
    /// Trailing margin.
    pub end_twips: i32,
}

/// Section column layout.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionColumns {
    /// Column count.
    pub count: u16,
}

/// One ordered section boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionBoundary {
    /// Stable section identity.
    pub id: SectionId,
    /// Page size.
    pub page_size: PageSize,
    /// Page margins.
    pub page_margins: PageMargins,
    /// Column layout.
    pub columns: SectionColumns,
}

/// Semantic theme references retained without embedding the theme.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeReferences {
    /// Major (heading) font family, if identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_font: Option<FontName>,
    /// Minor (body) font family, if identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_font: Option<FontName>,
}

/// A media reference (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaReference {
    /// Source relationship id.
    pub relationship_id: String,
    /// Media (content) type.
    pub media_type: String,
    /// Package part name.
    pub part_name: String,
}

/// The document definition tables.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definitions {
    /// Style definitions by id.
    #[serde(default)]
    pub styles: DefinitionMap<StyleId, Style>,
    /// Abstract numbering by id.
    #[serde(default)]
    pub abstract_numbering: DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    /// Numbering instances by id.
    #[serde(default)]
    pub numbering: DefinitionMap<NumberingInstanceId, NumberingInstance>,
    /// Ordered section boundaries.
    #[serde(default)]
    pub sections: Vec<SectionBoundary>,
    /// Media references by id.
    #[serde(default)]
    pub media: DefinitionMap<MediaId, MediaReference>,
    /// Document-wide defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_defaults: Option<DocumentDefaults>,
}
