//! Body block and inline nodes.

use serde::{Deserialize, Serialize};

use super::{BreakKind, MediaId, ParagraphProperties, RunProperties, Table};
use crate::NodeId;

/// OOXML `ST_PositiveCoordinate` upper bound, in English Metric Units (EMU).
pub const MAX_EMU: i64 = 27_273_042_316_900;

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

/// The natural size of a drawing, in English Metric Units (EMU).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Extent {
    /// Width in EMU (`0..=MAX_EMU`).
    pub width_emu: i64,
    /// Height in EMU (`0..=MAX_EMU`).
    pub height_emu: i64,
}

/// An inline drawing that references an embedded picture in the media table.
///
/// Only the embedded-picture case (a resolvable `r:embed`) is modeled; linked
/// blips, charts, SmartArt, and text boxes remain reported and (in Retention)
/// preserved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drawing {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced media entry (resolves in `Definitions::media`).
    pub media: MediaId,
    /// The drawing's natural size, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
}

/// An external hyperlink target (a resolved relationship URL).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalTarget {
    /// The target URL (non-empty, at most 2048 bytes).
    pub url: String,
}

/// An internal hyperlink target (a document bookmark anchor).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalTarget {
    /// The bookmark anchor name (non-empty, at most 255 bytes).
    pub anchor: String,
}

/// Where a hyperlink points.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HyperlinkTarget {
    /// An external URL resolved through the relationship graph.
    External(ExternalTarget),
    /// An internal bookmark anchor.
    Internal(InternalTarget),
}

/// An inline hyperlink wrapping a sequence of inline content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hyperlink {
    /// Stable identity.
    pub id: NodeId,
    /// Where the hyperlink points.
    pub target: HyperlinkTarget,
    /// A screen-tip, if declared (non-empty, at most 255 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// The hyperlinked inline content (non-empty; never a nested wrapper).
    pub inlines: Vec<InlineNode>,
}

/// Maximum field-instruction length, in UTF-8 bytes.
pub const MAX_FIELD_INSTRUCTION_BYTES: usize = 4096;

/// An inline field: a retained instruction and its cached result.
///
/// A field's dynamic value is not evaluated; `instruction` is the opaque field
/// code (`w:instr` / concatenated `w:instrText`) and `inlines` is the producer's
/// cached result (the runs a reader last computed). `inlines` may be empty and,
/// like a hyperlink, contains only leaf inlines — never a nested wrapper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Field {
    /// Stable identity.
    pub id: NodeId,
    /// The field instruction (non-empty, at most `MAX_FIELD_INSTRUCTION_BYTES`).
    pub instruction: String,
    /// The cached-result inline content (possibly empty; leaf inlines only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlines: Vec<InlineNode>,
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
    /// An inline drawing referencing embedded media.
    Drawing(Drawing),
    /// An inline hyperlink wrapping inline content.
    Hyperlink(Hyperlink),
    /// An inline field: an instruction and its cached result.
    Field(Field),
}

impl InlineNode {
    /// Returns the stable identity of this inline node.
    #[must_use]
    pub fn id(&self) -> NodeId {
        match self {
            Self::Run(run) => run.id,
            Self::Tab(tab) => tab.id,
            Self::Break(node) => node.id,
            Self::Drawing(drawing) => drawing.id,
            Self::Hyperlink(hyperlink) => hyperlink.id,
            Self::Field(field) => field.id,
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
    /// A table block.
    Table(Table),
}
