//! Body block and inline nodes.

use serde::{Deserialize, Serialize};

use super::{
    BookmarkId, BreakKind, CommentId, MediaId, NoteId, ParagraphProperties, RunProperties, Table,
};
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

/// The relationship type and package part a first-class embedded object
/// references. The referenced part's BYTES are not modeled (they live in the
/// preservation side-table, doc-45 invariant I4); this carries only the pointer
/// the writer needs to re-emit the referencing relationship.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedPart {
    /// Source relationship id (`r:id`), reused verbatim on export so the body's
    /// reference and the emitted relationship agree.
    pub relationship_id: String,
    /// Relationship type URI (`.../chart`, `.../diagramData`, `.../oleObject`, …).
    pub relationship_type: String,
    /// Package part name (e.g. `word/charts/chart1.xml`).
    pub part_name: String,
}

/// What kind of embedded object a first-class reference points at.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddedKind {
    /// A DrawingML chart (`a:graphicData` with a `c:chart`).
    Chart,
    /// A SmartArt diagram (`a:graphicData` with a `dgm:relIds`).
    Diagram,
    /// An embedded OLE object (`w:object` with an `o:OLEObject`).
    OleObject,
    /// Any other `a:graphicData` payload, keyed by its `uri`.
    Other(String),
}

/// An inline embedded object — a chart, SmartArt diagram, or OLE object — modeled
/// as a first-class reference to its preserved package part(s).
///
/// Unlike an inline picture (whose bytes flow through the media table), an
/// embedded object's parts (`word/charts/chart1.xml`, `word/diagrams/*`,
/// `word/embeddings/*`) are opaque XML/binary the semantic model does not parse;
/// they are byte-preserved by the side-table (P1F-2). This node re-links the
/// regenerated body to those parts so they round-trip as an *editable reference*
/// rather than surviving as orphaned bytes. Anchored/floating positioning is not
/// modeled (P1F-28); the object is treated as inline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedObject {
    /// Stable identity.
    pub id: NodeId,
    /// Which kind of embedded object this references.
    pub kind: EmbeddedKind,
    /// The primary referenced part (the chart, the diagram *data model*, or the
    /// OLE embedding).
    pub part: EmbeddedPart,
    /// Additional referenced parts (a diagram's layout/quick-style/colors), in
    /// the order they were declared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_parts: Vec<EmbeddedPart>,
    /// The fallback preview image (a chart/diagram cached bitmap or an OLE
    /// `o:OLEObject` imagedata), resolving in `Definitions::media`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<MediaId>,
    /// The object's natural size in EMU (from `wp:extent` or `w:object` origins;
    /// `0×0` when the producer declared none).
    pub extent: Extent,
    /// The OLE `ProgID` (`o:OLEObject@ProgID`), if declared (non-empty, <= 255
    /// bytes). `None` for charts and diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prog_id: Option<String>,
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
///
/// A legacy form field (Word's FORMTEXT / FORMCHECKBOX / FORMDROPDOWN, delimited
/// by `w:fldChar` with a `w:ffData` block) additionally carries `form` — its
/// input configuration (field name, type, default, entries, checkbox state).
/// `None` for an ordinary field.
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
    /// Legacy form-field configuration (`w:ffData`), when this field is a legacy
    /// form field. `None` for an ordinary field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<FormFieldData>,
}

/// Maximum length, in UTF-8 bytes, of a form-field string (name, default text,
/// help/status text, macro name, format, or a drop-down list entry).
pub const MAX_FORM_FIELD_STRING_BYTES: usize = 255;

/// Maximum number of entries in a form drop-down list (`w:ddList`).
pub const MAX_FORM_FIELD_ENTRIES: usize = 512;

/// Legacy form-field configuration (`w:ffData`): the common `CT_FFData`
/// properties plus exactly one kind-specific payload (text input, checkbox, or
/// drop-down). Attached to a [`Field`] whose instruction is FORMTEXT /
/// FORMCHECKBOX / FORMDROPDOWN.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormFieldData {
    /// The form-field name (`w:name@w:val`), if declared (non-empty, bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the field accepts input (`w:enabled`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Whether to recalculate fields on exit (`w:calcOnExit`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calc_on_exit: Option<bool>,
    /// Associated help text (`w:helpText@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    /// Associated status-bar text (`w:statusText@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    /// Macro run on entry (`w:entryMacro@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_macro: Option<String>,
    /// Macro run on exit (`w:exitMacro@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_macro: Option<String>,
    /// The kind-specific payload; must agree with the field's instruction.
    pub kind: FormFieldKind,
}

/// The kind-specific payload of a [`FormFieldData`] (`CT_FFData`'s one-of
/// `w:textInput` / `w:checkBox` / `w:ddList`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FormFieldKind {
    /// A text-input form field (`w:textInput`, FORMTEXT).
    TextInput(FormTextInput),
    /// A checkbox form field (`w:checkBox`, FORMCHECKBOX).
    CheckBox(FormCheckBox),
    /// A drop-down form field (`w:ddList`, FORMDROPDOWN).
    DropDown(FormDropDown),
}

/// The value type of a text-input form field (`w:textInput/w:type@w:val`,
/// `ST_FFTextType`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FormTextType {
    /// Unconstrained text (`regular`).
    Regular,
    /// A number (`number`).
    Number,
    /// A date (`date`).
    Date,
    /// The current time (`currentTime`).
    CurrentTime,
    /// The current date (`currentDate`).
    CurrentDate,
    /// A calculated result (`calculated`).
    Calculation,
}

/// A text-input form field's configuration (`w:textInput`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormTextInput {
    /// The value type (`w:type`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_type: Option<FormTextType>,
    /// The default text (`w:default@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// The maximum input length (`w:maxLength@w:val`), if declared. `0` means
    /// unlimited, mirroring the OOXML sentinel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// The text format string (`w:format@w:val`), if declared (bounded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// The size of a checkbox form field (`w:checkBox`'s `w:size` / `w:sizeAuto`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FormCheckBoxSize {
    /// An explicit size in half-points (`w:size@w:val`, `CT_HpsMeasure`).
    Explicit(u32),
    /// Automatically sized to the surrounding text (`w:sizeAuto`).
    Auto,
}

/// A checkbox form field's configuration (`w:checkBox`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormCheckBox {
    /// The checkbox size (`w:size` / `w:sizeAuto`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<FormCheckBoxSize>,
    /// The default checked state (`w:default`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    /// The current checked state (`w:checked`, `CT_OnOff`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
}

/// A drop-down form field's configuration (`w:ddList`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormDropDown {
    /// The selected entry index (`w:result@w:val`), if declared. Zero-based into
    /// `entries`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<u32>,
    /// The list entries (`w:listEntry@w:val`), in document order (each bounded;
    /// at most `MAX_FORM_FIELD_ENTRIES`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
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

/// The start marker of a bookmark range (`w:bookmarkStart`). A zero-width point;
/// the range is the span to the `BookmarkEnd` sharing its `bookmark`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkStart {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The bookmark this opens (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

/// The end marker of a bookmark range (`w:bookmarkEnd`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkEnd {
    /// Stable identity (this marker's own id).
    pub id: NodeId,
    /// The bookmark this closes (resolves in `Definitions::bookmarks`).
    pub bookmark: BookmarkId,
}

/// Maximum content-control (structured document tag) nesting depth (an `sdt`
/// inside an `sdt` inside …). Block and inline sdt nesting share this budget.
pub const MAX_SDT_DEPTH: u32 = 8;

/// The editing behaviour of a content control (`w:sdtPr` type marker). `None`
/// means the producer wrote no type marker — the OOXML default, rich text — or a
/// marker this slice does not map (then also reported). Producer-specific detail
/// of each type (list entries, date format, checkbox glyphs) is deferred.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SdtControlKind {
    /// A rich-text control (`w:richText`).
    RichText,
    /// A plain-text control (`w:text`).
    PlainText,
    /// A combo-box control (`w:comboBox`).
    ComboBox,
    /// A drop-down-list control (`w:dropDownList`).
    DropDownList,
    /// A date-picker control (`w:date`).
    Date,
    /// A picture control (`w:picture`).
    Picture,
    /// A checkbox control (`w14:checkbox`).
    Checkbox,
    /// A grouping control (`w:group`).
    Group,
    /// A building-block gallery (`w:docPartObj` / `w:docPartList`).
    BuildingBlockGallery,
    /// A repeating-section control (`w:repeatingSection`).
    RepeatingSection,
    /// A citation control (`w:citation`).
    Citation,
    /// A bibliography control (`w:bibliography`).
    Bibliography,
}

/// Typed content-control properties (`w:sdtPr`). An empty value serializes to
/// `{}`. Everything else in `w:sdtPr` (lock, placeholder, data binding, list
/// entries, date/checkbox detail) is retained-and-reported, not modeled here.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdtProperties {
    /// Editing behaviour, if a recognized type marker was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_kind: Option<SdtControlKind>,
    /// Friendly name (`w:alias@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Programmatic tag (`w:tag@w:val`), if declared (non-empty, <= 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The producer's `w:id@w:val` as written, if declared (<= 64 bytes). Opaque
    /// and non-unique across controls — a grouping key, NOT a node identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_id: Option<String>,
}

/// A block-level content control (`w:sdt` around paragraphs/tables). Its content
/// reuses the recursive block model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// An inline-level content control (`w:sdt` around runs). A transparent inline
/// range wrapper (like `Revision`): it may wrap leaf inlines, a hyperlink/field,
/// or a nested inline sdt, and may itself appear inside a hyperlink/field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineSdt {
    /// Stable identity.
    pub id: NodeId,
    /// Control properties (always present; empty is `{}`).
    pub properties: SdtProperties,
    /// The wrapped inline content (non-empty).
    pub inlines: Vec<InlineNode>,
}

/// Maximum retained OMML markup length, in UTF-8 bytes.
pub const MAX_MATH_BYTES: usize = 65_536;

/// An opaque inline math object (an OMML `m:oMath` or `m:oMathPara` subtree).
///
/// Equation structure is not semantically modeled; the OMML subtree is retained
/// verbatim in `omml` so it round-trips losslessly, and `text` is a best-effort
/// plain-text fallback (the concatenated `m:t` runs) for search/accessibility.
/// This mirrors the opaque-retention treatment of other unmodeled constructs:
/// the equation survives a round trip and its text never leaks into the
/// surrounding paragraph runs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Math {
    /// Stable identity.
    pub id: NodeId,
    /// The retained OMML markup (non-empty, at most `MAX_MATH_BYTES` bytes).
    pub omml: String,
    /// Best-effort plain-text fallback (the concatenated `m:t` text); may be
    /// empty when the equation carries no literal text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// Inline content supported by schema v1.
//
// `Run` is by far the largest and the most common variant (it carries the full
// `RunProperties`, which grows as run-property coverage expands). Boxing it (or
// its properties) would shrink the enum but add a heap allocation on the hot
// path — most inline nodes are runs — and it ripples across every
// construction/match site and the public API; tracked with the same follow-up
// as `BlockNode` so it can land as one focused change.
#[allow(clippy::large_enum_variant)]
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
    /// An inline embedded object (chart, SmartArt diagram, or OLE object)
    /// referencing preserved package part(s).
    EmbeddedObject(EmbeddedObject),
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
    /// The start marker of a bookmark range.
    BookmarkStart(BookmarkStart),
    /// The end marker of a bookmark range.
    BookmarkEnd(BookmarkEnd),
    /// An inline-level content control wrapping inline content.
    Sdt(InlineSdt),
    /// An opaque inline math object retaining its OMML subtree verbatim.
    Math(Math),
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
            Self::EmbeddedObject(object) => object.id,
            Self::Hyperlink(hyperlink) => hyperlink.id,
            Self::Field(field) => field.id,
            Self::TextBox(text_box) => text_box.id,
            Self::NoteReference(note) => note.id,
            Self::CommentReference(comment) => comment.id,
            Self::Revision(revision) => revision.id,
            Self::BookmarkStart(node) => node.id,
            Self::BookmarkEnd(node) => node.id,
            Self::Sdt(sdt) => sdt.id,
            Self::Math(math) => math.id,
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
// `Table` is larger than `Paragraph` now that tables carry borders/margins.
// Boxing it (or its properties) is a worthwhile memory optimization for
// paragraph-heavy bodies, but it ripples across every construction/match site
// and the enum is part of the public API; tracked as a follow-up so it can land
// as one focused change rather than entangled with feature slices.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockNode {
    /// A paragraph block.
    Paragraph(Paragraph),
    /// A table block.
    Table(Table),
    /// A block-level content control wrapping block content.
    Sdt(BlockSdt),
}
