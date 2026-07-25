//! Body block and inline nodes.

use serde::{Deserialize, Serialize};

use super::{BreakKind, CommentId, MediaId, NoteId, ParagraphProperties, RunProperties, Table};
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

/// Maximum text-box nesting depth (a text box inside a text box inside ...).
pub const MAX_TEXTBOX_DEPTH: u32 = 8;

/// An inline text box holding block content (a DrawingML `wps:txbx` or a legacy
/// VML `v:textbox`). A text box is inline-anchored but carries block-level
/// content, so its `blocks` reuse the recursive block model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBox {
    /// Stable identity.
    pub id: NodeId,
    /// The text box's block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// Whether a note reference points at a footnote or an endnote.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
}

/// An inline reference to a footnote or endnote definition (`w:footnoteReference`
/// / `w:endnoteReference`). The referenced note's content is a definition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteReference {
    /// Stable identity.
    pub id: NodeId,
    /// Whether this references a footnote or an endnote.
    pub kind: NoteKind,
    /// The referenced note (resolves in `Definitions::footnotes`/`endnotes`).
    pub note: NoteId,
}

/// An inline reference to a comment definition (`w:commentReference`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentReference {
    /// Stable identity.
    pub id: NodeId,
    /// The referenced comment (resolves in `Definitions::comments`).
    pub comment: CommentId,
}

/// Maximum revision-wrapper nesting depth (a `w:ins` around a `w:del`, ...).
pub const MAX_REVISION_DEPTH: u32 = 8;

/// Whether a tracked-change range was inserted or deleted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    /// An inserted run range (`w:ins`).
    Insertion,
    /// A deleted run range (`w:del`); its runs carry `w:delText` content.
    Deletion,
}

/// A tracked-change (revision) range wrapping inline content (`w:ins`/`w:del`).
///
/// Author/date/id are retained as the producer wrote them (opaque, bounded),
/// mirroring `Comment` metadata. Deleted text is preserved verbatim in the
/// wrapped runs' `text`; the `Deletion` kind marks it deleted. A revision is a
/// transparent range marker: it may wrap leaf inlines, a hyperlink/field, or a
/// nested revision, and may itself appear inside a hyperlink/field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    /// Stable identity (this inline node's own id).
    pub id: NodeId,
    /// Whether the range was inserted or deleted.
    pub kind: RevisionKind,
    /// The revision author, if declared (non-empty, at most 255 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The revision date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The producer's revision id (`w:id`) as written, if declared (<= 64 bytes).
    /// Opaque and non-unique across ranges — a grouping key, not a node identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    /// The wrapped inline content (non-empty; may include a nested revision).
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
    /// An inline text box holding block content.
    TextBox(TextBox),
    /// An inline reference to a footnote or endnote.
    NoteReference(NoteReference),
    /// An inline reference to a comment.
    CommentReference(CommentReference),
    /// A tracked-change (insertion/deletion) range wrapping inline content.
    Revision(Revision),
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
            Self::TextBox(text_box) => text_box.id,
            Self::NoteReference(note) => note.id,
            Self::CommentReference(comment) => comment.id,
            Self::Revision(revision) => revision.id,
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
