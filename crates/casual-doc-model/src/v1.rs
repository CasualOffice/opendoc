//! Normalized document schema v1: typed properties, styles, numbering,
//! sections, theme and media references, strict validation, and a deterministic
//! total v0-to-v1 migration.
//!
//! v1 is additive: the crate-root v0 model is unchanged and remains the runtime
//! edit model this slice. v1 is the import/export and migration target
//! (`38-NORMALIZED-SCHEMA-V1-DESIGN.md`, ADR-027).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    BlockNode as V0BlockNode, Document as V0Document, IdGenerator, InlineNode as V0InlineNode,
    Mark, ModelError, NodeId, SnapshotError, SnapshotLimits, enforce_limit,
};

/// The schema version stamped on authored and migrated v1 documents.
pub const SCHEMA_VERSION_V1: u32 = 1;

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(NodeId);

        impl $name {
            /// Wraps a node ID as this definition identifier.
            #[must_use]
            pub const fn new(id: NodeId) -> Self {
                Self(id)
            }

            /// Returns the underlying node ID.
            #[must_use]
            pub const fn node_id(self) -> NodeId {
                self.0
            }
        }
    };
}

id_newtype!(
    /// Stable identity of a style definition.
    StyleId
);
id_newtype!(
    /// Stable identity of an abstract numbering definition.
    AbstractNumberingId
);
id_newtype!(
    /// Stable identity of a numbering instance.
    NumberingInstanceId
);
id_newtype!(
    /// Stable identity of a media reference.
    MediaId
);
id_newtype!(
    /// Stable identity of a section boundary.
    SectionId
);

/// A duplicate-key-rejecting, deterministically-ordered id map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionMap<K: Ord, V>(BTreeMap<K, V>);

impl<K: Ord, V> Default for DefinitionMap<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord, V> DefinitionMap<K, V> {
    /// Returns the value for a key, if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    /// Returns whether a key is present.
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates entries in ascending key order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }
}

impl<K: Ord + Serialize, V: Serialize> Serialize for DefinitionMap<K, V> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for DefinitionMap<K, V>
where
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);

        impl<'de, K, V> Visitor<'de> for MapVisitor<K, V>
        where
            K: Ord + Deserialize<'de>,
            V: Deserialize<'de>,
        {
            type Value = DefinitionMap<K, V>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object with unique definition keys")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = access.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate definition key"));
                    }
                }
                Ok(DefinitionMap(values))
            }
        }

        deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
    }
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

/// A text run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    /// Stable run identity.
    pub id: NodeId,
    /// Run properties (always present; empty is `{}`).
    pub properties: RunProperties,
    /// Grapheme text (non-empty).
    pub text: String,
}

/// An explicit tab.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tab {
    /// Stable identity.
    pub id: NodeId,
}

/// An explicit break.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Break {
    /// Stable identity.
    pub id: NodeId,
    /// Break kind.
    pub kind: BreakKind,
}

/// Inline content supported by schema v1.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineNode {
    /// A text run.
    Run(Run),
    /// An explicit tab.
    Tab(Tab),
    /// An explicit break.
    Break(Break),
}

impl InlineNode {
    fn id(&self) -> NodeId {
        match self {
            Self::Run(run) => run.id,
            Self::Tab(tab) => tab.id,
            Self::Break(node) => node.id,
        }
    }
}

/// A paragraph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Paragraph {
    /// Stable paragraph identity.
    pub id: NodeId,
    /// Paragraph properties (always present; empty is `{}`).
    pub properties: ParagraphProperties,
    /// Ordered inline content.
    pub inlines: Vec<InlineNode>,
}

/// A body-level node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    /// A paragraph block.
    Paragraph(Paragraph),
}

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

/// A normalized schema v1 document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    schema_version: u32,
    document_id: NodeId,
    body: Vec<BlockNode>,
    definitions: Definitions,
}

impl Document {
    /// Returns the schema version (always 1 for a valid v1 document).
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the document id.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.document_id
    }

    /// Returns the body blocks.
    #[must_use]
    pub fn body(&self) -> &[BlockNode] {
        &self.body
    }

    /// Returns the definition tables.
    #[must_use]
    pub const fn definitions(&self) -> &Definitions {
        &self.definitions
    }

    /// Parses one strict, bounded schema v1 JSON document.
    pub fn from_json(bytes: &[u8], limits: SnapshotLimits) -> Result<Self, SnapshotError> {
        limits.validate()?;
        enforce_limit("input_json_bytes", bytes.len(), limits.max_input_bytes)?;
        let document: Self =
            serde_json::from_slice(bytes).map_err(|_| SnapshotError::MalformedJson)?;
        document.validate().map_err(SnapshotError::InvalidModel)?;
        document.validate_snapshot_limits(limits)?;
        Ok(document)
    }

    /// Serializes a valid v1 document to deterministic compact JSON.
    pub fn to_json(&self) -> Result<Vec<u8>, SnapshotError> {
        self.validate().map_err(SnapshotError::InvalidModel)?;
        serde_json::to_vec(self).map_err(|_| SnapshotError::Serialization)
    }

    /// Validates every schema v1 invariant, first-failure-wins.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema_version != SCHEMA_VERSION_V1 {
            return Err(ModelError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.body.is_empty() {
            return Err(ModelError::EmptyDocumentBody);
        }

        self.validate_unique_ids()?;
        self.validate_styles()?;
        self.validate_numbering()?;
        self.validate_sections()?;
        self.validate_media()?;
        self.validate_document_defaults()?;
        self.validate_body()?;
        Ok(())
    }

    fn validate_document_defaults(&self) -> Result<(), ModelError> {
        if let Some(defaults) = &self.definitions.document_defaults {
            if let Some(properties) = &defaults.paragraph {
                self.check_paragraph_property_refs(properties)?;
            }
            if let Some(properties) = &defaults.run {
                self.check_run_property_refs(properties)?;
            }
        }
        Ok(())
    }

    fn validate_sections(&self) -> Result<(), ModelError> {
        for section in &self.definitions.sections {
            check_domain(
                (1..=31_680).contains(&section.page_size.width_twips),
                "section.page_size.width",
            )?;
            check_domain(
                (1..=31_680).contains(&section.page_size.height_twips),
                "section.page_size.height",
            )?;
            for margin in [
                section.page_margins.top_twips,
                section.page_margins.bottom_twips,
                section.page_margins.start_twips,
                section.page_margins.end_twips,
            ] {
                check_domain((0..=31_680).contains(&margin), "section.page_margins")?;
            }
            check_domain(
                (1..=64).contains(&section.columns.count),
                "section.column_count",
            )?;
        }
        Ok(())
    }

    fn validate_media(&self) -> Result<(), ModelError> {
        for (_, media) in self.definitions.media.iter() {
            check_domain(
                !media.relationship_id.is_empty() && media.relationship_id.len() <= 255,
                "media.relationship_id",
            )?;
            check_domain(
                !media.media_type.is_empty() && media.media_type.len() <= 255,
                "media.media_type",
            )?;
            check_domain(
                !media.part_name.is_empty() && media.part_name.len() <= 1024,
                "media.part_name",
            )?;
        }
        Ok(())
    }

    fn validate_unique_ids(&self) -> Result<(), ModelError> {
        let mut ids = BTreeSet::new();
        let mut record = |id: NodeId| -> Result<(), ModelError> {
            if ids.insert(id) {
                Ok(())
            } else {
                Err(ModelError::DuplicateNodeId(id))
            }
        };
        record(self.document_id)?;
        for (id, _) in self.definitions.styles.iter() {
            record(id.node_id())?;
        }
        for (id, _) in self.definitions.abstract_numbering.iter() {
            record(id.node_id())?;
        }
        for (id, _) in self.definitions.numbering.iter() {
            record(id.node_id())?;
        }
        for section in &self.definitions.sections {
            record(section.id.node_id())?;
        }
        for (id, _) in self.definitions.media.iter() {
            record(id.node_id())?;
        }
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            record(paragraph.id)?;
            for inline in &paragraph.inlines {
                record(inline.id())?;
            }
        }
        Ok(())
    }

    fn style_exists(&self, id: StyleId) -> bool {
        self.definitions.styles.contains_key(&id)
    }

    fn validate_styles(&self) -> Result<(), ModelError> {
        for (id, style) in self.definitions.styles.iter() {
            if let Some(properties) = &style.paragraph {
                self.check_paragraph_property_refs(properties)?;
            }
            if let Some(properties) = &style.run {
                self.check_run_property_refs(properties)?;
            }
            if let Some(based_on) = style.based_on {
                if !self.style_exists(based_on) {
                    return Err(ModelError::DanglingStyleRef(based_on.node_id()));
                }
                let base_kind = self.definitions.styles.get(&based_on).map(|base| base.kind);
                if base_kind != Some(style.kind) {
                    return Err(ModelError::StyleBasedOnKindMismatch {
                        style: id.node_id(),
                        based_on: based_on.node_id(),
                    });
                }
            }
            // Cycle detection: walk the based_on chain from this style.
            let mut visited = BTreeSet::new();
            visited.insert(*id);
            let mut current = style.based_on;
            while let Some(next) = current {
                if !visited.insert(next) {
                    return Err(ModelError::StyleBasedOnCycle(id.node_id()));
                }
                current = self
                    .definitions
                    .styles
                    .get(&next)
                    .and_then(|style| style.based_on);
            }
        }
        Ok(())
    }

    fn validate_numbering(&self) -> Result<(), ModelError> {
        for (id, instance) in self.definitions.numbering.iter() {
            let abstract_num = self
                .definitions
                .abstract_numbering
                .get(&instance.abstract_ref)
                .ok_or(ModelError::DanglingAbstractNumberingRef(
                    instance.abstract_ref.node_id(),
                ))?;
            for numbering_override in &instance.overrides {
                if let Some(start) = numbering_override.start {
                    check_domain(start <= 32_767, "numbering.override.start")?;
                }
                if !abstract_num
                    .levels
                    .iter()
                    .any(|level| level.level == numbering_override.level)
                {
                    return Err(ModelError::NumberingLevelUndefined {
                        reference: id.node_id(),
                        level: numbering_override.level,
                    });
                }
            }
        }
        // Level domain: level start values.
        for (_, abstract_num) in self.definitions.abstract_numbering.iter() {
            for level in &abstract_num.levels {
                check_domain(level.start <= 32_767, "numbering.level.start")?;
                if let Some(style) = level.style_ref {
                    if !self.style_exists(style) {
                        return Err(ModelError::DanglingStyleRef(style.node_id()));
                    }
                }
            }
        }
        Ok(())
    }

    fn resolve_numbering_level(&self, reference: &NumberingRef) -> Result<(), ModelError> {
        let instance = self.definitions.numbering.get(&reference.instance).ok_or(
            ModelError::DanglingNumberingRef(reference.instance.node_id()),
        )?;
        let abstract_num = self
            .definitions
            .abstract_numbering
            .get(&instance.abstract_ref)
            .ok_or(ModelError::DanglingAbstractNumberingRef(
                instance.abstract_ref.node_id(),
            ))?;
        if abstract_num
            .levels
            .iter()
            .any(|level| level.level == reference.level)
        {
            Ok(())
        } else {
            Err(ModelError::NumberingLevelUndefined {
                reference: reference.instance.node_id(),
                level: reference.level,
            })
        }
    }

    fn check_paragraph_property_refs(
        &self,
        properties: &ParagraphProperties,
    ) -> Result<(), ModelError> {
        if let Some(style) = properties.style_ref {
            if !self.style_exists(style) {
                return Err(ModelError::DanglingStyleRef(style.node_id()));
            }
        }
        if let Some(numbering) = &properties.numbering {
            self.resolve_numbering_level(numbering)?;
        }
        if let Some(indentation) = &properties.indentation {
            for value in [
                indentation.start_twips,
                indentation.end_twips,
                indentation.first_line_twips,
                indentation.hanging_twips,
            ]
            .into_iter()
            .flatten()
            {
                check_domain((-31_680..=31_680).contains(&value), "paragraph.indentation")?;
            }
        }
        if let Some(spacing) = &properties.spacing {
            for value in [spacing.before_twips, spacing.after_twips]
                .into_iter()
                .flatten()
            {
                check_domain((0..=31_680).contains(&value), "paragraph.spacing")?;
            }
            if let Some(percent) = spacing.line_percent {
                check_domain(
                    (1..=10_000).contains(&percent),
                    "paragraph.spacing.line_percent",
                )?;
            }
        }
        Ok(())
    }

    fn check_run_property_refs(&self, properties: &RunProperties) -> Result<(), ModelError> {
        if let Some(style) = properties.style_ref {
            if !self.style_exists(style) {
                return Err(ModelError::DanglingStyleRef(style.node_id()));
            }
        }
        if let Some(size) = properties.size_half_points {
            check_domain((1..=65_534).contains(&size), "run.size_half_points")?;
        }
        if let Some(FontRef::Named(font)) = &properties.font_ref {
            check_domain(
                !font.name.is_empty() && font.name.len() <= 255,
                "run.font_ref.name",
            )?;
        }
        Ok(())
    }

    fn validate_body(&self) -> Result<(), ModelError> {
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            self.check_paragraph_property_refs(&paragraph.properties)?;
            let mut previous_run_properties: Option<&RunProperties> = None;
            for inline in &paragraph.inlines {
                match inline {
                    InlineNode::Run(run) => {
                        if run.text.is_empty() {
                            return Err(ModelError::EmptyTextRun);
                        }
                        self.check_run_property_refs(&run.properties)?;
                        let length = run.text.graphemes(true).count();
                        u32::try_from(length)
                            .map_err(|_| ModelError::GraphemeCountOverflow(run.id))?;
                        if previous_run_properties == Some(&run.properties) {
                            return Err(ModelError::AdjacentEquivalentTextRuns(paragraph.id));
                        }
                        previous_run_properties = Some(&run.properties);
                    }
                    InlineNode::Tab(_) | InlineNode::Break(_) => {
                        previous_run_properties = None;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_snapshot_limits(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        enforce_limit("body_blocks", self.body.len(), limits.max_blocks)?;
        let mut scalar_values = 0_usize;
        for block in &self.body {
            let BlockNode::Paragraph(paragraph) = block;
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    enforce_limit("text_run_bytes", run.text.len(), limits.max_text_run_bytes)?;
                    scalar_values = scalar_values.checked_add(run.text.chars().count()).ok_or(
                        SnapshotError::LimitExceeded {
                            limit: "unicode_scalar_values",
                            observed: usize::MAX,
                            allowed: limits.max_unicode_scalar_values,
                        },
                    )?;
                }
            }
        }
        enforce_limit(
            "unicode_scalar_values",
            scalar_values,
            limits.max_unicode_scalar_values,
        )
    }
}

fn check_domain(condition: bool, property: &'static str) -> Result<(), ModelError> {
    if condition {
        Ok(())
    } else {
        Err(ModelError::PropertyValueOutOfDomain { property })
    }
}

/// A deterministic v0-to-v1 migration failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    /// The v0 source failed its own invariants.
    InvalidSource(ModelError),
    /// Migration produced an invalid v1 document (an internal defect, e.g. a
    /// source run whose grapheme count exceeds `u32::MAX`).
    ProducedInvalidV1(ModelError),
    /// The synthesized id space was exhausted.
    IdSpaceExhausted,
    /// The v0 source populated the extension map, which v1 cannot represent.
    UnsupportedSourceExtensions,
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(error) => write!(formatter, "v0 source is invalid: {error}"),
            Self::ProducedInvalidV1(error) => {
                write!(
                    formatter,
                    "migration produced an invalid v1 document: {error}"
                )
            }
            Self::IdSpaceExhausted => formatter.write_str("synthesized node id space is exhausted"),
            Self::UnsupportedSourceExtensions => {
                formatter.write_str("v0 source extension map cannot migrate to v1")
            }
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<MigrationError> for SnapshotError {
    fn from(error: MigrationError) -> Self {
        match error {
            MigrationError::InvalidSource(model) | MigrationError::ProducedInvalidV1(model) => {
                Self::InvalidModel(model)
            }
            MigrationError::IdSpaceExhausted => Self::LimitExceeded {
                limit: "synthesized_node_ids",
                observed: usize::MAX,
                allowed: usize::MAX,
            },
            MigrationError::UnsupportedSourceExtensions => {
                Self::InvalidModel(ModelError::UnsupportedV0Extensions)
            }
        }
    }
}

impl Document {
    /// Deterministically migrates a valid v0 document into v1.
    ///
    /// Migration is total and lossless over the empty-extensions v0 profile:
    /// paragraph and document ids are preserved verbatim; synthesized run ids
    /// come from `ids` in canonical document order (skipping preserved ids).
    /// A v0 source with a populated extension map is rejected — never silently
    /// dropped. Output bytes are identical for a fixed `(source, ids seed)`.
    /// Totality is conditioned on each v0 run's grapheme count fitting `u32`;
    /// an over-long run yields `ProducedInvalidV1` rather than a wrong document.
    pub fn from_v0(source: &V0Document, ids: &mut IdGenerator) -> Result<Self, MigrationError> {
        source.validate().map_err(MigrationError::InvalidSource)?;
        if !source.extensions_is_empty() {
            return Err(MigrationError::UnsupportedSourceExtensions);
        }

        let mut used: BTreeSet<NodeId> = BTreeSet::new();
        used.insert(source.id());
        for block in source.body() {
            let V0BlockNode::Paragraph(paragraph) = block;
            used.insert(paragraph.id());
        }

        let mut body = Vec::with_capacity(source.body().len());
        for block in source.body() {
            let V0BlockNode::Paragraph(paragraph) = block;
            let mut inlines = Vec::with_capacity(paragraph.inlines().len());
            for inline in paragraph.inlines() {
                let V0InlineNode::Text(run) = inline;
                let id = alloc_non_colliding(ids, &mut used)?;
                inlines.push(InlineNode::Run(Run {
                    id,
                    properties: run_properties_from_marks(run.marks()),
                    text: run.text().to_owned(),
                }));
            }
            body.push(BlockNode::Paragraph(Paragraph {
                id: paragraph.id(),
                properties: ParagraphProperties::default(),
                inlines,
            }));
        }

        let document = Self {
            schema_version: SCHEMA_VERSION_V1,
            document_id: source.id(),
            body,
            definitions: Definitions::default(),
        };
        document
            .validate()
            .map_err(MigrationError::ProducedInvalidV1)?;
        Ok(document)
    }
}

fn alloc_non_colliding(
    ids: &mut IdGenerator,
    used: &mut BTreeSet<NodeId>,
) -> Result<NodeId, MigrationError> {
    loop {
        let candidate = ids
            .next_id()
            .map_err(|_| MigrationError::IdSpaceExhausted)?;
        if used.insert(candidate) {
            return Ok(candidate);
        }
    }
}

fn run_properties_from_marks(marks: &BTreeSet<Mark>) -> RunProperties {
    let flag = |mark: Mark| marks.contains(&mark).then_some(true);
    RunProperties {
        style_ref: None,
        bold: flag(Mark::Bold),
        italic: flag(Mark::Italic),
        underline: flag(Mark::Underline),
        strike: flag(Mark::Strike),
        color: None,
        size_half_points: None,
        font_ref: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v0_blank() -> V0Document {
        V0Document::blank(
            NodeId::from_parts(7, 1).unwrap(),
            NodeId::from_parts(7, 2).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn blank_v0_migrates_to_canonical_v1_bytes() {
        let source = v0_blank();
        let mut ids = IdGenerator::new(9);
        let migrated = Document::from_v0(&source, &mut ids).unwrap();
        let json = String::from_utf8(migrated.to_json().unwrap()).unwrap();
        assert_eq!(
            json,
            "{\"schemaVersion\":1,\
             \"documentId\":\"00000000000000070000000000000001\",\
             \"body\":[{\"type\":\"paragraph\",\
             \"id\":\"00000000000000070000000000000002\",\
             \"properties\":{},\"inlines\":[]}],\
             \"definitions\":{\"styles\":{},\"abstractNumbering\":{},\
             \"numbering\":{},\"sections\":[],\"media\":{}}}"
        );
    }

    #[test]
    fn marks_migrate_to_run_properties() {
        let mut paragraph = crate::Paragraph::empty(NodeId::from_parts(1, 2).unwrap());
        let marks = BTreeSet::from([Mark::Bold, Mark::Strike]);
        paragraph.insert_text(0, "Hi".to_owned(), marks).unwrap();
        let source = document_with_paragraph(paragraph);

        let mut ids = IdGenerator::new(5);
        let migrated = Document::from_v0(&source, &mut ids).unwrap();
        let BlockNode::Paragraph(result) = &migrated.body()[0];
        let InlineNode::Run(run) = &result.inlines[0] else {
            panic!("expected a run");
        };
        assert_eq!(run.text, "Hi");
        assert_eq!(run.properties.bold, Some(true));
        assert_eq!(run.properties.strike, Some(true));
        assert_eq!(run.properties.italic, None);
    }

    #[test]
    fn migration_is_deterministic_and_reload_is_a_fixed_point() {
        let source = v0_blank();
        let first = Document::from_v0(&source, &mut IdGenerator::new(9))
            .unwrap()
            .to_json()
            .unwrap();
        let second = Document::from_v0(&source, &mut IdGenerator::new(9))
            .unwrap()
            .to_json()
            .unwrap();
        assert_eq!(first, second);

        let reloaded = Document::from_json(&first, SnapshotLimits::default()).unwrap();
        assert_eq!(reloaded.to_json().unwrap(), first);
    }

    #[test]
    fn populated_v0_extensions_are_rejected_not_dropped() {
        let json = br#"{
            "schemaVersion":0,
            "documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","inlines":[]}],
            "extensions":{"x":{"mediaType":"application/octet-stream","data":[1]}}
        }"#;
        let source = V0Document::from_json(json, SnapshotLimits::default()).unwrap();
        assert_eq!(
            Document::from_v0(&source, &mut IdGenerator::new(1)),
            Err(MigrationError::UnsupportedSourceExtensions)
        );
    }

    #[test]
    fn strict_json_rejects_unknown_fields_and_v0_extensions_field() {
        let unknown = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{},"future":true}"#;
        let has_extensions = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{},"extensions":{}}"#;
        for invalid in [unknown.as_slice(), has_extensions] {
            assert_eq!(
                Document::from_json(invalid, SnapshotLimits::default()),
                Err(SnapshotError::MalformedJson)
            );
        }
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let json = br#"{"schemaVersion":2,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{}}"#;
        assert_eq!(
            Document::from_json(json, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(
                ModelError::UnsupportedSchemaVersion(2)
            ))
        );
    }

    #[test]
    fn dangling_style_reference_is_rejected() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"styleRef":"000000000000000000000000000000ff"},"inlines":[]}],
            "definitions":{}}"#;
        assert!(matches!(
            Document::from_json(json, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(ModelError::DanglingStyleRef(_)))
        ));
    }

    #[test]
    fn based_on_cycle_and_kind_mismatch_are_rejected() {
        let cycle = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph","basedOn":"0000000000000000000000000000000b"},
              "0000000000000000000000000000000b":{"kind":"paragraph","basedOn":"0000000000000000000000000000000a"}
            }}}"#;
        assert!(matches!(
            Document::from_json(cycle, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(ModelError::StyleBasedOnCycle(
                _
            )))
        ));

        let mismatch = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph","basedOn":"0000000000000000000000000000000b"},
              "0000000000000000000000000000000b":{"kind":"character"}
            }}}"#;
        assert!(matches!(
            Document::from_json(mismatch, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(
                ModelError::StyleBasedOnKindMismatch { .. }
            ))
        ));
    }

    #[test]
    fn numbering_reference_integrity_is_enforced() {
        let dangling = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"numbering":{"instance":"000000000000000000000000000000aa","level":0}},"inlines":[]}],
            "definitions":{}}"#;
        assert!(matches!(
            Document::from_json(dangling, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(
                ModelError::DanglingNumberingRef(_)
            ))
        ));
    }

    #[test]
    fn out_of_domain_run_size_is_rejected() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003",
                "properties":{"sizeHalfPoints":0},"text":"x"}]}],
            "definitions":{}}"#;
        assert!(matches!(
            Document::from_json(json, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(
                ModelError::PropertyValueOutOfDomain {
                    property: "run.size_half_points"
                }
            ))
        ));
    }

    #[test]
    fn adjacent_equal_runs_are_rejected_but_a_tab_between_them_is_accepted() {
        let adjacent = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
        assert!(matches!(
            Document::from_json(adjacent, SnapshotLimits::default()),
            Err(SnapshotError::InvalidModel(
                ModelError::AdjacentEquivalentTextRuns(_)
            ))
        ));

        let with_tab = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"tab","id":"00000000000000030000000000000005"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
        assert!(Document::from_json(with_tab, SnapshotLimits::default()).is_ok());
    }

    #[test]
    fn named_font_round_trips() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003",
                "properties":{"fontRef":{"type":"named","name":"Arial"}},"text":"x"}]}],
            "definitions":{}}"#;
        let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
        let reexport = document.to_json().unwrap();
        let reloaded = Document::from_json(&reexport, SnapshotLimits::default()).unwrap();
        assert_eq!(reloaded.to_json().unwrap(), reexport);
    }

    fn expect_invalid(json: &[u8]) -> ModelError {
        match Document::from_json(json, SnapshotLimits::default()) {
            Err(SnapshotError::InvalidModel(error)) => error,
            other => panic!("expected InvalidModel, got {other:?}"),
        }
    }

    #[test]
    fn document_defaults_properties_are_validated() {
        let dangling = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"documentDefaults":{"paragraph":{"styleRef":"000000000000000000000000000000ff"}}}}"#;
        assert!(matches!(
            expect_invalid(dangling),
            ModelError::DanglingStyleRef(_)
        ));
        let out_of_domain = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"documentDefaults":{"run":{"sizeHalfPoints":0}}}}"#;
        assert!(matches!(
            expect_invalid(out_of_domain),
            ModelError::PropertyValueOutOfDomain {
                property: "run.size_half_points"
            }
        ));
    }

    #[test]
    fn numbering_overrides_are_validated() {
        let base = |overrides: &str| {
            format!(
                "{{\"schemaVersion\":1,\"documentId\":\"00000000000000030000000000000001\",\
                 \"body\":[{{\"type\":\"paragraph\",\"id\":\"00000000000000030000000000000002\",\"properties\":{{}},\"inlines\":[]}}],\
                 \"definitions\":{{\"abstractNumbering\":{{\"0000000000000000000000000000000a\":{{\"levels\":[{{\"level\":0,\"start\":1}}]}}}},\
                 \"numbering\":{{\"0000000000000000000000000000000b\":{{\"abstractRef\":\"0000000000000000000000000000000a\",\"overrides\":{overrides}}}}}}}}}"
            ).into_bytes()
        };
        assert!(matches!(
            expect_invalid(&base("[{\"level\":9,\"start\":1}]")),
            ModelError::NumberingLevelUndefined { level: 9, .. }
        ));
        assert!(matches!(
            expect_invalid(&base("[{\"level\":0,\"start\":60000}]")),
            ModelError::PropertyValueOutOfDomain {
                property: "numbering.override.start"
            }
        ));
    }

    #[test]
    fn undefined_numbering_level_reference_is_rejected() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002",
              "properties":{"numbering":{"instance":"0000000000000000000000000000000b","level":5}},"inlines":[]}],
            "definitions":{"abstractNumbering":{"0000000000000000000000000000000a":{"levels":[{"level":0,"start":1}]}},
              "numbering":{"0000000000000000000000000000000b":{"abstractRef":"0000000000000000000000000000000a"}}}}"#;
        assert!(matches!(
            expect_invalid(json),
            ModelError::NumberingLevelUndefined { level: 5, .. }
        ));
    }

    #[test]
    fn section_geometry_domains_are_enforced() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"sections":[{"id":"0000000000000000000000000000000c",
              "pageSize":{"widthTwips":-1,"heightTwips":100},
              "pageMargins":{"topTwips":0,"bottomTwips":0,"startTwips":0,"endTwips":0},
              "columns":{"count":1}}]}}"#;
        assert!(matches!(
            expect_invalid(json),
            ModelError::PropertyValueOutOfDomain {
                property: "section.page_size.width"
            }
        ));
    }

    #[test]
    fn media_reference_fields_are_validated() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"media":{"0000000000000000000000000000000d":{"relationshipId":"rId1","mediaType":"","partName":"word/media/x.png"}}}}"#;
        assert!(matches!(
            expect_invalid(json),
            ModelError::PropertyValueOutOfDomain {
                property: "media.media_type"
            }
        ));
    }

    #[test]
    fn duplicate_definition_map_key_is_rejected() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{
              "0000000000000000000000000000000a":{"kind":"paragraph"},
              "0000000000000000000000000000000a":{"kind":"character"}
            }}}"#;
        assert_eq!(
            Document::from_json(json, SnapshotLimits::default()),
            Err(SnapshotError::MalformedJson)
        );
    }

    #[test]
    fn cross_table_duplicate_node_id_is_rejected() {
        // A style key equal to the paragraph id.
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},"inlines":[]}],
            "definitions":{"styles":{"00000000000000030000000000000002":{"kind":"paragraph"}}}}"#;
        assert!(matches!(
            expect_invalid(json),
            ModelError::DuplicateNodeId(_)
        ));
    }

    #[test]
    fn empty_run_text_is_rejected() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[{"type":"run","id":"00000000000000030000000000000003","properties":{},"text":""}]}],
            "definitions":{}}"#;
        assert!(matches!(expect_invalid(json), ModelError::EmptyTextRun));
    }

    #[test]
    fn break_inlines_round_trip_and_separate_equal_runs() {
        let json = br#"{"schemaVersion":1,"documentId":"00000000000000030000000000000001",
            "body":[{"type":"paragraph","id":"00000000000000030000000000000002","properties":{},
              "inlines":[
                {"type":"run","id":"00000000000000030000000000000003","properties":{},"text":"a"},
                {"type":"break","id":"00000000000000030000000000000005","kind":"page"},
                {"type":"run","id":"00000000000000030000000000000004","properties":{},"text":"b"}
              ]}],
            "definitions":{}}"#;
        let document = Document::from_json(json, SnapshotLimits::default()).unwrap();
        let reexport = document.to_json().unwrap();
        assert_eq!(
            Document::from_json(&reexport, SnapshotLimits::default())
                .unwrap()
                .to_json()
                .unwrap(),
            reexport
        );
    }

    #[test]
    fn migration_skips_ids_that_collide_with_preserved_paragraph_ids() {
        // Seed the IdGenerator in the same (namespace, counter) space as the
        // preserved paragraph id so the first candidate collides and is skipped.
        let mut paragraph = crate::Paragraph::empty(NodeId::from_parts(4, 1).unwrap());
        paragraph
            .insert_text(0, "x".to_owned(), BTreeSet::new())
            .unwrap();
        let source = document_with_paragraph_ids(NodeId::from_parts(4, 9).unwrap(), paragraph);

        let migrated = Document::from_v0(&source, &mut IdGenerator::new(4)).unwrap();
        let BlockNode::Paragraph(result) = &migrated.body()[0];
        let InlineNode::Run(run) = &result.inlines[0] else {
            panic!("expected a run");
        };
        // Candidate (4,1) collides with the preserved paragraph id, so the run
        // receives (4,2); output re-validates and is deterministic.
        assert_eq!(run.id, NodeId::from_parts(4, 2).unwrap());
        migrated.validate().unwrap();
    }

    fn document_with_paragraph_ids(document_id: NodeId, paragraph: crate::Paragraph) -> V0Document {
        let json = format!(
            "{{\"schemaVersion\":0,\"documentId\":\"{document_id}\",\"body\":[{}],\"extensions\":{{}}}}",
            serde_json::to_string(&crate::BlockNode::Paragraph(paragraph)).unwrap()
        );
        V0Document::from_json(json.as_bytes(), SnapshotLimits::default()).unwrap()
    }

    fn document_with_paragraph(paragraph: crate::Paragraph) -> V0Document {
        let json = format!(
            "{{\"schemaVersion\":0,\"documentId\":\"{}\",\"body\":[{}],\"extensions\":{{}}}}",
            NodeId::from_parts(1, 1).unwrap(),
            serde_json::to_string(&crate::BlockNode::Paragraph(paragraph)).unwrap()
        );
        V0Document::from_json(json.as_bytes(), SnapshotLimits::default()).unwrap()
    }
}
